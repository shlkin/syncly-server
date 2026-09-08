//! 观看历史 and the media half of 我的收藏.
//!
//! Two mappings live here. Ids travel as opaque strings, and the room and media
//! links travel as possibly-empty ones — an entry has to keep reading once the
//! room or the media row it pointed at is gone. Everything else the client sends
//! is checked by the service, whose limits match the columns.

use synctv_core::models::{
    MediaFavorite, NewMediaFavorite, RecordWatchHistory, UserId, WatchHistoryEntry,
    DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::{optional_media_id_to_proto, optional_room_id_to_proto, total_to_proto};
use super::ClientApiImpl;
use crate::impls::ApiError;

impl ClientApiImpl {
    /// Folds a progress report into the caller's history. Reporting the same
    /// `source_key` again moves the entry rather than appending one, so a player
    /// may call this on a timer.
    pub async fn record_watch_history(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RecordWatchHistoryRequest,
    ) -> Result<synctv_proto::client::RecordWatchHistoryResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let recorded = RecordWatchHistory {
            room_id: crate::impls::proto_validated_optional_id(
                &req.room_id,
                &self.public_id_codec,
            )?,
            media_id: crate::impls::proto_validated_optional_id(
                &req.media_id,
                &self.public_id_codec,
            )?,
            title: req.title,
            cover_url: req.cover_url,
            position_seconds: req.position_seconds,
            duration_seconds: req.duration_seconds,
            source_key: req.source_key,
        };
        let entry = self
            .user_service
            .record_watch_history(user_id, &recorded)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::RecordWatchHistoryResponse {
            entry: Some(self.watch_history_entry_to_proto(entry)?),
        })
    }
    pub async fn list_watch_history(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListWatchHistoryRequest,
    ) -> Result<synctv_proto::client::ListWatchHistoryResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_watch_history(user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let entries = page
            .into_iter()
            .map(|entry| self.watch_history_entry_to_proto(entry))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListWatchHistoryResponse {
            entries,
            total: total_to_proto(total, "watch history")?,
        })
    }

    /// `success` is false when the id named nothing this account owns, which is
    /// also what deleting the same entry twice reports.
    pub async fn delete_watch_history_entry(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteWatchHistoryEntryRequest,
    ) -> Result<synctv_proto::client::DeleteWatchHistoryEntryResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let entry_id = self
            .public_id_codec
            .decode_watch_history_entry_id(&req.entry_id)
            .map_err(ApiError::InvalidInput)?;
        let success = self
            .user_service
            .delete_watch_history_entry(user_id, entry_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::DeleteWatchHistoryEntryResponse { success })
    }

    pub async fn clear_watch_history(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::ClearWatchHistoryResponse, ApiError> {
        let deleted_count = self
            .user_service
            .clear_watch_history(user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ClearWatchHistoryResponse {
            deleted_count: i64::try_from(deleted_count).map_err(|_| {
                ApiError::Internal("cleared entry count exceeds i64::MAX".to_string())
            })?,
        })
    }

    /// Stars a title. Starring an already-starred `source_key` refreshes its
    /// stored title and cover, so the button is a plain toggle.
    pub async fn add_media_favorite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AddMediaFavoriteRequest,
    ) -> Result<synctv_proto::client::AddMediaFavoriteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let new_favorite = NewMediaFavorite {
            room_id: crate::impls::proto_validated_optional_id(
                &req.room_id,
                &self.public_id_codec,
            )?,
            media_id: crate::impls::proto_validated_optional_id(
                &req.media_id,
                &self.public_id_codec,
            )?,
            title: req.title,
            cover_url: req.cover_url,
            source_key: req.source_key,
        };
        let favorite = self
            .user_service
            .add_media_favorite(user_id, &new_favorite)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::AddMediaFavoriteResponse {
            favorite: Some(self.media_favorite_to_proto(favorite)?),
        })
    }

    /// Unstars by `source_key`, so the client can toggle without holding an id.
    pub async fn remove_media_favorite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RemoveMediaFavoriteRequest,
    ) -> Result<synctv_proto::client::RemoveMediaFavoriteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let success = self
            .user_service
            .remove_media_favorite(user_id, &req.source_key)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::RemoveMediaFavoriteResponse { success })
    }

    pub async fn list_media_favorites(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListMediaFavoritesRequest,
    ) -> Result<synctv_proto::client::ListMediaFavoritesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_media_favorites(user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let favorites = page
            .into_iter()
            .map(|favorite| self.media_favorite_to_proto(favorite))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListMediaFavoritesResponse {
            favorites,
            total: total_to_proto(total, "media favorite")?,
        })
    }

    /// Star states for a page of cards, so a list screen needs one call rather
    /// than one per card.
    pub async fn get_media_favorite_states(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetMediaFavoriteStatesRequest,
    ) -> Result<synctv_proto::client::GetMediaFavoriteStatesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let favorited_source_keys = self
            .user_service
            .media_favorite_source_keys(user_id, &req.source_keys)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::GetMediaFavoriteStatesResponse {
            favorited_source_keys,
        })
    }

    fn watch_history_entry_to_proto(
        &self,
        entry: WatchHistoryEntry,
    ) -> Result<synctv_proto::client::WatchHistoryEntry, ApiError> {
        Ok(synctv_proto::client::WatchHistoryEntry {
            id: self
                .public_id_codec
                .encode_watch_history_entry_id(entry.id)
                .map_err(ApiError::Internal)?,
            room_id: optional_room_id_to_proto(entry.room_id, &self.public_id_codec)?,
            media_id: optional_media_id_to_proto(entry.media_id, &self.public_id_codec)?,
            title: entry.title,
            cover_url: entry.cover_url,
            position_seconds: entry.position_seconds,
            duration_seconds: entry.duration_seconds,
            source_key: entry.source_key,
            watched_at: entry.watched_at.timestamp(),
        })
    }

    fn media_favorite_to_proto(
        &self,
        favorite: MediaFavorite,
    ) -> Result<synctv_proto::client::MediaFavorite, ApiError> {
        Ok(synctv_proto::client::MediaFavorite {
            id: self
                .public_id_codec
                .encode_media_favorite_id(favorite.id)
                .map_err(ApiError::Internal)?,
            room_id: optional_room_id_to_proto(favorite.room_id, &self.public_id_codec)?,
            media_id: optional_media_id_to_proto(favorite.media_id, &self.public_id_codec)?,
            title: favorite.title,
            cover_url: favorite.cover_url,
            source_key: favorite.source_key,
            created_at: favorite.created_at.timestamp(),
        })
    }
}
