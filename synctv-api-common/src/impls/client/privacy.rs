//! People search and the privacy switches that gate it.
//!
//! Search is the only path in the app to an account the caller does not already
//! follow, so it is the one place an account can be exposed to a stranger. Two
//! things keep that narrow: the query matches usernames only, and an account
//! that turned discovery off is filtered in SQL rather than in the response, so
//! there is no shape of request that returns it.

use synctv_core::models::{UserId, DEFAULT_PAGE_SIZE};

use super::convert::total_to_proto;
use super::ClientApiImpl;
use crate::impls::ApiError;

/// Search pages are short on purpose: someone looking for a person types more
/// rather than scrolling further.
const USER_SEARCH_MAX_PAGE_SIZE: u32 = 50;

impl ClientApiImpl {
    pub async fn search_users(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SearchUsersRequest,
    ) -> Result<synctv_proto::client::SearchUsersResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            USER_SEARCH_MAX_PAGE_SIZE,
        );

        let (hits, total) = self
            .user_service
            .search_users(user_id, &req.query, pagination)
            .await
            .map_err(ApiError::from)?;

        let mut results = Vec::with_capacity(hits.len());
        for hit in hits {
            let user = self.user_public_view_with_loaded_avatar(&hit.user).await?;
            results.push(synctv_proto::client::UserSearchResult {
                user: Some(user),
                is_friend: hit.is_friend,
                request_sent: hit.request_sent,
                request_received: hit.request_received,
                is_following: hit.is_following,
            });
        }

        Ok(synctv_proto::client::SearchUsersResponse {
            results,
            total: total_to_proto(total, "user search result")?,
        })
    }

    pub async fn get_privacy_settings(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::PrivacySettings, ApiError> {
        let settings = self
            .user_service
            .privacy_settings(user_id)
            .await
            .map_err(ApiError::from)?;
        Ok(privacy_settings_to_proto(settings))
    }

    pub async fn update_privacy_settings(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdatePrivacySettingsRequest,
    ) -> Result<synctv_proto::client::PrivacySettings, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let settings = self
            .user_service
            .update_privacy_settings(
                user_id,
                req.discoverable,
                req.show_following,
                req.show_followers,
            )
            .await
            .map_err(ApiError::from)?;
        Ok(privacy_settings_to_proto(settings))
    }
}

fn privacy_settings_to_proto(
    settings: synctv_core::repository::UserPrivacySettings,
) -> synctv_proto::client::PrivacySettings {
    synctv_proto::client::PrivacySettings {
        discoverable: settings.discoverable,
        show_following: settings.show_following,
        show_followers: settings.show_followers,
    }
}
