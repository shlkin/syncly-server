//! The privacy page, and the people search it gates.
//!
//! Search is deliberately a service concern rather than a repository one: the
//! repository will happily match any username, and it is here that the caller
//! is required to be a signed-in account, so an anonymous request cannot walk
//! the user table.

use crate::{
    models::{PageParams, UserId},
    repository::{UserPrivacySettings, UserSearchHit},
    Result,
};

use super::UserService;

impl UserService {
    /// The three switches on `user_id`'s privacy page.
    pub async fn privacy_settings(&self, user_id: &UserId) -> Result<UserPrivacySettings> {
        self.repository.user_privacy_settings(user_id).await
    }

    /// Applies the switches the caller sent; `None` leaves one as it was.
    pub async fn update_privacy_settings(
        &self,
        user_id: &UserId,
        discoverable: Option<bool>,
        show_following: Option<bool>,
        show_followers: Option<bool>,
    ) -> Result<UserPrivacySettings> {
        self.repository
            .update_user_privacy_settings(user_id, discoverable, show_following, show_followers)
            .await
    }

    /// Finds accounts by username on behalf of `viewer_user_id`.
    pub async fn search_users(
        &self,
        viewer_user_id: &UserId,
        query: &str,
        pagination: PageParams,
    ) -> Result<(Vec<UserSearchHit>, i64)> {
        self.repository
            .search_discoverable_users(viewer_user_id, query, pagination)
            .await
    }
}
