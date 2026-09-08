//! 情侣空间: the invite handshake, the bound space, and its shared lists.
//!
//! The space id is opaque on the wire, so every call here decodes it before the
//! service decides whether the caller belongs to that space — a stranger's id and
//! a nonexistent one come back the same way, which is why membership is checked
//! there rather than here.

use synctv_core::models::{
    CoupleFavorite, CoupleSpace, CoupleSpaceStatus, CoupleSpaceView, NewCoupleFavorite,
    SharedWatchEntry, UserId, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::{
    optional_date_from_proto, optional_date_to_proto, optional_media_id_to_proto,
    optional_room_id_to_proto, optional_user_id_to_proto, total_to_proto,
};
use super::ClientApiImpl;
use crate::impls::ApiError;

impl ClientApiImpl {
    /// The space that concerns this account — the active one, or else the invite
    /// waiting on an answer. An absent space is the card's empty state.
    pub async fn get_couple_space(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::GetCoupleSpaceResponse, ApiError> {
        let view = self
            .user_service
            .couple_space_for_user(user_id)
            .await
            .map_err(ApiError::from)?;
        let space = match view {
            Some(view) => Some(self.couple_space_view_to_proto(view).await?),
            None => None,
        };

        Ok(synctv_proto::client::GetCoupleSpaceResponse { space })
    }

    /// Invites an account to bind. If they had already invited the caller, that
    /// invite is accepted instead: two people inviting each other is agreement.
    pub async fn invite_couple_partner(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::InviteCouplePartnerRequest,
    ) -> Result<synctv_proto::client::InviteCouplePartnerResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let partner_user_id =
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?;
        let view = self
            .user_service
            .invite_couple_partner(user_id, &partner_user_id, &req.title, &req.invite_message)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::InviteCouplePartnerResponse {
            space: Some(self.couple_space_view_to_proto(view).await?),
        })
    }

    /// Accepts an invite the caller received. Without an anniversary the space
    /// counts from today.
    pub async fn accept_couple_invite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AcceptCoupleInviteRequest,
    ) -> Result<synctv_proto::client::AcceptCoupleInviteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        let anniversary_date = optional_date_from_proto(&req.anniversary_date, "anniversary_date")?;
        let view = self
            .user_service
            .accept_couple_invite(space_id, user_id, anniversary_date)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::AcceptCoupleInviteResponse {
            space: Some(self.couple_space_view_to_proto(view).await?),
        })
    }

    /// Declines an invite the caller received, or withdraws one they sent.
    pub async fn dismiss_couple_invite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DismissCoupleInviteRequest,
    ) -> Result<synctv_proto::client::DismissCoupleInviteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        self.user_service
            .dismiss_couple_invite(space_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::DismissCoupleInviteResponse { success: true })
    }

    /// Unbinds an active space, freeing both accounts to bind again.
    pub async fn unbind_couple_space(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UnbindCoupleSpaceRequest,
    ) -> Result<synctv_proto::client::UnbindCoupleSpaceResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        self.user_service
            .unbind_couple_space(space_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::UnbindCoupleSpaceResponse { success: true })
    }

    /// Edits the title or the anniversary, from either side. An omitted field is
    /// left as it stands.
    pub async fn update_couple_space(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::UpdateCoupleSpaceRequest,
    ) -> Result<synctv_proto::client::UpdateCoupleSpaceResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        let anniversary_date = match req.anniversary_date.as_deref() {
            Some(value) => optional_date_from_proto(value, "anniversary_date")?,
            None => None,
        };
        let view = self
            .user_service
            .update_couple_space(space_id, user_id, req.title.as_deref(), anniversary_date)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::UpdateCoupleSpaceResponse {
            space: Some(self.couple_space_view_to_proto(view).await?),
        })
    }

    /// Adds to the shared list, from either side. Adding something already on it
    /// refreshes the entry, so the button is a plain toggle.
    pub async fn add_couple_favorite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AddCoupleFavoriteRequest,
    ) -> Result<synctv_proto::client::AddCoupleFavoriteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        let new_favorite = NewCoupleFavorite {
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
            note: req.note,
        };
        let favorite = self
            .user_service
            .add_couple_favorite(space_id, user_id, &new_favorite)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::AddCoupleFavoriteResponse {
            favorite: Some(self.couple_favorite_to_proto(favorite)?),
        })
    }

    pub async fn remove_couple_favorite(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RemoveCoupleFavoriteRequest,
    ) -> Result<synctv_proto::client::RemoveCoupleFavoriteResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        self.user_service
            .remove_couple_favorite(space_id, user_id, &req.source_key)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::RemoveCoupleFavoriteResponse { success: true })
    }

    pub async fn list_couple_favorites(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleFavoritesRequest,
    ) -> Result<synctv_proto::client::ListCoupleFavoritesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_couple_favorites(space_id, user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let favorites = page
            .into_iter()
            .map(|favorite| self.couple_favorite_to_proto(favorite))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListCoupleFavoritesResponse {
            favorites,
            total: total_to_proto(total, "couple favorite")?,
        })
    }

    /// 共同观影记录: titles both members have watched.
    pub async fn list_couple_shared_watch_entries(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListCoupleSharedWatchEntriesRequest,
    ) -> Result<synctv_proto::client::ListCoupleSharedWatchEntriesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let space_id = self.decode_couple_space_id(&req.space_id)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_couple_shared_watch_entries(space_id, user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let entries = page
            .into_iter()
            .map(|entry| self.shared_watch_entry_to_proto(entry))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListCoupleSharedWatchEntriesResponse {
            entries,
            total: total_to_proto(total, "shared watch entry")?,
        })
    }

    fn decode_couple_space_id(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_couple_space_id(value)
            .map_err(ApiError::InvalidInput)
    }

    /// The whole space screen: the row, the partner's account with their avatar
    /// resolved, and the counters.
    async fn couple_space_view_to_proto(
        &self,
        view: CoupleSpaceView,
    ) -> Result<synctv_proto::client::CoupleSpaceView, ApiError> {
        let partner = self
            .user_public_view_with_loaded_avatar(&view.partner)
            .await?;

        Ok(synctv_proto::client::CoupleSpaceView {
            space: Some(self.couple_space_to_proto(&view.space)?),
            partner: Some(partner),
            viewer_is_initiator: view.viewer_is_initiator,
            days_together: view.days_together.unwrap_or_default(),
            favorite_count: view.favorite_count,
            shared_watch_count: view.shared_watch_count,
        })
    }

    fn couple_space_to_proto(
        &self,
        space: &CoupleSpace,
    ) -> Result<synctv_proto::client::CoupleSpace, ApiError> {
        Ok(synctv_proto::client::CoupleSpace {
            id: self
                .public_id_codec
                .encode_couple_space_id(space.id)
                .map_err(ApiError::Internal)?,
            initiator_user_id: self
                .public_id_codec
                .encode_user_id(space.initiator_user_id)
                .map_err(ApiError::Internal)?,
            partner_user_id: self
                .public_id_codec
                .encode_user_id(space.partner_user_id)
                .map_err(ApiError::Internal)?,
            status: couple_space_status_to_proto(space.status),
            title: space.title.clone(),
            invite_message: space.invite_message.clone(),
            anniversary_date: optional_date_to_proto(space.anniversary_date),
            created_at: space.created_at.timestamp(),
            bound_at: space.bound_at.map(|at| at.timestamp()).unwrap_or_default(),
            ended_at: space.ended_at.map(|at| at.timestamp()).unwrap_or_default(),
        })
    }

    fn couple_favorite_to_proto(
        &self,
        favorite: CoupleFavorite,
    ) -> Result<synctv_proto::client::CoupleFavorite, ApiError> {
        Ok(synctv_proto::client::CoupleFavorite {
            id: self
                .public_id_codec
                .encode_couple_favorite_id(favorite.id)
                .map_err(ApiError::Internal)?,
            added_by_user_id: optional_user_id_to_proto(
                favorite.added_by_user_id,
                &self.public_id_codec,
            )?,
            room_id: optional_room_id_to_proto(favorite.room_id, &self.public_id_codec)?,
            media_id: optional_media_id_to_proto(favorite.media_id, &self.public_id_codec)?,
            title: favorite.title,
            cover_url: favorite.cover_url,
            source_key: favorite.source_key,
            note: favorite.note,
            created_at: favorite.created_at.timestamp(),
        })
    }

    fn shared_watch_entry_to_proto(
        &self,
        entry: SharedWatchEntry,
    ) -> Result<synctv_proto::client::SharedWatchEntry, ApiError> {
        Ok(synctv_proto::client::SharedWatchEntry {
            source_key: entry.source_key,
            title: entry.title,
            cover_url: entry.cover_url,
            room_id: optional_room_id_to_proto(entry.room_id, &self.public_id_codec)?,
            media_id: optional_media_id_to_proto(entry.media_id, &self.public_id_codec)?,
            viewer_watched_at: entry.viewer_watched_at.timestamp(),
            partner_watched_at: entry.partner_watched_at.timestamp(),
        })
    }
}

fn couple_space_status_to_proto(status: CoupleSpaceStatus) -> i32 {
    match status {
        CoupleSpaceStatus::Pending => synctv_proto::client::CoupleSpaceStatus::Pending as i32,
        CoupleSpaceStatus::Active => synctv_proto::client::CoupleSpaceStatus::Active as i32,
        CoupleSpaceStatus::Ended => synctv_proto::client::CoupleSpaceStatus::Ended as i32,
    }
}
