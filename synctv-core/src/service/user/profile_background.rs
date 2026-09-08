//! The picture behind a member's profile page header.
//!
//! Same shape as [`super::avatar`], with two differences that are the point of
//! the feature: the policy accepts animated GIF, and the reference is stored in
//! `user_profile_details` beside the signature rather than on the `users` row,
//! so the projection behind authentication and every member list stays as it is.

use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    models::{
        CompleteFileUploadSession, CompleteFileUploadSessionResult, CreateFileUploadSession,
        FileMetadata, FileObjectDownload, FileRangeRequest, FileReferenceMetadata,
        FileUploadManifestPart, FileUploadRange, FileUploadSessionCreateResult, GetFileObject,
        StoreFileUpload, StoreFileUploadResult, SubmittedFileReference, UserId,
    },
    service::{
        file_storage::{FileStorageCleanupOrigin, FileStorageContext},
        user::UserService,
        user_profile_background_upload_policy,
    },
    Error, Result,
};

const USER_PROFILE_BACKGROUND_REFERENCE_KIND: &str = "user_profile_background";

fn profile_background_storage_scope(user_id: UserId) -> String {
    format!("users/{}/profile-backgrounds", user_id.as_i64())
}

fn storage_unconfigured() -> Error {
    Error::InvalidInput("file storage is not configured for profile backgrounds".to_string())
}

/// What a client asks for before it starts sending the bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserProfileBackgroundUploadSession {
    pub client_background_id: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub parts: Vec<FileUploadManifestPart>,
    pub metadata: FileMetadata,
}

impl UserService {
    pub async fn create_profile_background_upload_session(
        &self,
        user_id: &UserId,
        request: CreateUserProfileBackgroundUploadSession,
    ) -> Result<FileUploadSessionCreateResult> {
        let storage = self
            .file_storage_service
            .as_ref()
            .ok_or_else(storage_unconfigured)?;
        self.repository
            .get_by_id(user_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("User {user_id} not found")))?;
        storage
            .create_upload_session(CreateFileUploadSession {
                user_id: *user_id,
                storage_scope: profile_background_storage_scope(*user_id),
                client_file_id: request.client_background_id,
                filename: None,
                mime_type: request.mime_type,
                size_bytes: request.size_bytes,
                width: request.width,
                height: request.height,
                duration_seconds: None,
                bitrate_bps: None,
                parts: request.parts,
                metadata: request.metadata,
                policy: user_profile_background_upload_policy(),
            })
            .await
    }

    pub async fn store_profile_background_upload_object(
        &self,
        encoded_object_key: &str,
        upload_token: &str,
        content_type: Option<&str>,
        range: Option<FileUploadRange>,
        data: bytes::Bytes,
    ) -> Result<StoreFileUploadResult> {
        self.file_storage_service
            .as_ref()
            .ok_or_else(storage_unconfigured)?
            .store_upload(StoreFileUpload {
                encoded_object_key: encoded_object_key.to_string(),
                upload_token: upload_token.to_string(),
                content_type: content_type.map(str::to_string),
                range,
                data,
            })
            .await
    }

    pub async fn complete_profile_background_upload_session(
        &self,
        request: CompleteFileUploadSession,
    ) -> Result<CompleteFileUploadSessionResult> {
        self.file_storage_service
            .as_ref()
            .ok_or_else(storage_unconfigured)?
            .complete_upload_session(request)
            .await
    }

    pub async fn get_profile_background_object_stream(
        &self,
        encoded_object_key: &str,
        read_token: &str,
        range: Option<FileRangeRequest>,
    ) -> Result<FileObjectDownload> {
        self.file_storage_service
            .as_ref()
            .ok_or_else(|| Error::NotFound("File object not found".to_string()))?
            .get_object_stream(GetFileObject {
                encoded_object_key: encoded_object_key.to_string(),
                read_token: read_token.to_string(),
                range,
            })
            .await
    }

    /// Points the profile page at an uploaded file, returning its reference id.
    ///
    /// The row is locked before the swap so two uploads racing cannot both read
    /// the same predecessor and leave one stored object referenced by nobody.
    pub async fn update_profile_background(
        &self,
        user_id: &UserId,
        file: SubmittedFileReference,
    ) -> Result<i64> {
        let storage = self
            .file_storage_service
            .as_ref()
            .ok_or_else(storage_unconfigured)?;
        self.repository
            .get_by_id(user_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("User {user_id} not found")))?;

        let storage_scope = profile_background_storage_scope(*user_id);
        let upload_policy = user_profile_background_upload_policy();
        let prepared = storage
            .prepare_submitted_files(
                FileStorageContext {
                    user_id: *user_id,
                    storage_scope: &storage_scope,
                    object_kind: upload_policy.object_kind,
                    client_request_id: None,
                },
                vec![file],
            )
            .await?;
        let file = prepared.into_iter().next().ok_or_else(|| {
            Error::InvalidInput("profile background file is required".to_string())
        })?;

        let mut tx: Transaction<'_, Postgres> = self.repository.pool().begin().await?;
        let previous_reference_id = self
            .repository
            .profile_background_reference_id_for_update(user_id, &mut *tx)
            .await?;
        let new_reference_id = crate::repository::FileStorageRepository::insert_reference_in_tx(
            &mut tx,
            &file.storage_backend,
            &file.object_key,
            USER_PROFILE_BACKGROUND_REFERENCE_KIND,
            &user_id.as_i64().to_string(),
            None,
            &FileReferenceMetadata::File(FileMetadata::default()),
        )
        .await?
        .ok_or_else(|| {
            Error::InvalidInput("profile background file object is not registered".to_string())
        })?;
        self.repository
            .set_profile_background_with_executor(user_id, Some(new_reference_id), &mut *tx)
            .await?;
        tx.commit().await?;

        if let Some(previous) = self
            .background_reference_target(user_id, previous_reference_id)
            .await?
        {
            if previous.storage_backend != file.storage_backend
                || previous.object_key != file.object_key
            {
                storage
                    .schedule_delete_files(FileStorageCleanupOrigin::ReferenceReleased, &[previous])
                    .await?;
            }
        }
        Ok(new_reference_id)
    }

    /// Returns the page to the default backdrop, releasing the stored object.
    pub async fn clear_profile_background(&self, user_id: &UserId) -> Result<()> {
        let mut tx: Transaction<'_, Postgres> = self.repository.pool().begin().await?;
        let previous_reference_id = self
            .repository
            .profile_background_reference_id_for_update(user_id, &mut *tx)
            .await?;
        self.repository
            .set_profile_background_with_executor(user_id, None, &mut *tx)
            .await?;
        tx.commit().await?;

        if let (Some(storage), Some(previous)) = (
            self.file_storage_service.as_ref(),
            self.background_reference_target(user_id, previous_reference_id)
                .await?,
        ) {
            storage
                .schedule_delete_files(FileStorageCleanupOrigin::ReferenceReleased, &[previous])
                .await?;
        }
        Ok(())
    }

    /// The cleanup handle for a background reference that is being released.
    /// The owner id rides along, because that pair is what identifies the
    /// reference the cleanup job is releasing.
    async fn background_reference_target(
        &self,
        user_id: &UserId,
        reference_id: Option<i64>,
    ) -> Result<Option<crate::models::FileReferenceTarget>> {
        let Some(reference_id) = reference_id else {
            return Ok(None);
        };
        Ok(
            crate::repository::FileStorageRepository::new(self.repository.pool().clone())
                .get_reference_by_id(reference_id)
                .await?
                .map(|reference| {
                    reference.reference_target(
                        USER_PROFILE_BACKGROUND_REFERENCE_KIND,
                        user_id.as_i64().to_string(),
                    )
                }),
        )
    }
}
