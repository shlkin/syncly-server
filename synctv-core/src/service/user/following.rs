//! Follow graph and public profile text.
//!
//! The repository layer treats the edge table as a set, so the guards that make
//! following a *social* action rather than a raw insert live here: no following
//! yourself, no following an account that is gone, and no follow edge across a
//! block in either direction.

use chrono::{DateTime, Utc};

use crate::{
    models::{FollowedUser, PageParams, UserId, UserProfile},
    validation::validate_user_signature_input,
    Error, Result,
};

use super::UserService;

impl UserService {
    /// Records that `follower_user_id` follows `followee_user_id`, returning when
    /// the edge was first created.
    ///
    /// Following twice is not an error: the second call returns the timestamp of
    /// the first, so a client that retries a lost response converges instead of
    /// reporting a conflict the user cannot act on.
    pub async fn follow_user(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<DateTime<Utc>> {
        if follower_user_id == followee_user_id {
            return Err(Error::InvalidInput("Cannot follow yourself".to_string()));
        }
        if self.repository.get_by_id(followee_user_id).await?.is_none() {
            return Err(Error::NotFound("User not found".to_string()));
        }

        // A block closes the channel between two accounts. Letting a follow edge
        // through would reopen it — the blocked side would land back in the
        // blocker's follower list and in whatever notifications it feeds. The
        // message stays vague on purpose: it must not reveal that a block exists.
        if self
            .repository
            .is_blocking(followee_user_id, follower_user_id)
            .await?
        {
            return Err(Error::Authorization("Cannot follow this user".to_string()));
        }
        // The reverse direction is the caller's own block, so naming it is safe
        // and tells them exactly which action to take first.
        if self
            .repository
            .is_blocking(follower_user_id, followee_user_id)
            .await?
        {
            return Err(Error::InvalidInput(
                "Unblock this user before following them".to_string(),
            ));
        }

        self.repository
            .follow_user(follower_user_id, followee_user_id)
            .await
    }

    /// Drops the follow edge. Returns false when there was nothing to drop, so
    /// unfollowing a stranger is a no-op rather than an error.
    pub async fn unfollow_user(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<bool> {
        self.repository
            .unfollow_user(follower_user_id, followee_user_id)
            .await
    }

    pub async fn is_following(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<bool> {
        self.repository
            .is_following(follower_user_id, followee_user_id)
            .await
    }

    /// Assembles the profile page for `subject_user_id` as `viewer_user_id` sees
    /// it. A missing viewer is a signed-out reader: the relationship flags come
    /// back false rather than absent, which is what an anonymous page renders.
    pub async fn user_profile(
        &self,
        subject_user_id: &UserId,
        viewer_user_id: Option<&UserId>,
    ) -> Result<UserProfile> {
        let user = self
            .repository
            .get_by_id(subject_user_id)
            .await?
            .ok_or_else(|| Error::NotFound("User not found".to_string()))?;
        let stats = self
            .repository
            .follow_profile_stats(subject_user_id, viewer_user_id)
            .await?;

        Ok(UserProfile {
            user,
            signature: stats.signature,
            background_file_reference_id: stats.background_file_reference_id,
            following_count: stats.following_count,
            follower_count: stats.follower_count,
            viewer_following: stats.viewer_following,
            following_viewer: stats.following_viewer,
            show_following: stats.show_following,
            show_followers: stats.show_followers,
        })
    }

    /// Accounts `subject_user_id` follows, newest edge first.
    pub async fn list_following(
        &self,
        subject_user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<FollowedUser>, i64)> {
        self.ensure_user_exists(subject_user_id).await?;
        self.repository
            .list_following(subject_user_id, pagination, search)
            .await
    }

    /// Accounts that follow `subject_user_id`, newest edge first.
    pub async fn list_followers(
        &self,
        subject_user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<FollowedUser>, i64)> {
        self.ensure_user_exists(subject_user_id).await?;
        self.repository
            .list_followers(subject_user_id, pagination, search)
            .await
    }

    /// Stores the public signature, returning what was stored.
    ///
    /// The stored form is the sanitized one, so the caller can render the result
    /// without a second read.
    pub async fn update_user_signature(&self, user_id: &UserId, signature: &str) -> Result<String> {
        let sanitized = validate_user_signature_input(signature)
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        self.repository
            .set_user_signature(user_id, &sanitized)
            .await
    }

    /// Rejects listings for an id that names no live account, so a stale link
    /// reads as "gone" instead of "follows nobody".
    async fn ensure_user_exists(&self, user_id: &UserId) -> Result<()> {
        if self.repository.get_by_id(user_id).await?.is_none() {
            return Err(Error::NotFound("User not found".to_string()));
        }
        Ok(())
    }
}
