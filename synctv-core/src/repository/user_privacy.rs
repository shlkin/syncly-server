//! Who may find an account, and who may read its follow graph.
//!
//! The switches live in `user_profile_details` beside the signature, and an
//! account with no row there is fully visible — that is what every account was
//! before the columns existed, so a missing row and three `TRUE`s have to mean
//! the same thing. Every read here goes through [`UserPrivacySettings::default`]
//! for that reason rather than treating the absent row as an error.

use sqlx::{Postgres, QueryBuilder};

use super::query_builder::ilike_contains_pattern;
use super::user::UserRepository;
use crate::{
    models::{PageParams, SignupMethod, User, UserId, UserRole, UserStatus},
    Result,
};

/// The three switches on the privacy page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserPrivacySettings {
    /// Off removes the account from people search. It does not hide the profile
    /// from anyone who already has the link — search is a directory, not a lock.
    pub discoverable: bool,
    pub show_following: bool,
    pub show_followers: bool,
}

impl Default for UserPrivacySettings {
    fn default() -> Self {
        Self {
            discoverable: true,
            show_following: true,
            show_followers: true,
        }
    }
}

/// One row of people search: the account, plus where the searcher already
/// stands with it so the row can offer the action that is actually left.
#[derive(Debug, Clone)]
pub struct UserSearchHit {
    pub user: User,
    pub is_friend: bool,
    pub request_sent: bool,
    pub request_received: bool,
    pub is_following: bool,
}

impl UserRepository {
    /// Reads `user_id`'s switches, or the all-visible default when the account
    /// has never opened the privacy page.
    pub async fn user_privacy_settings(&self, user_id: &UserId) -> Result<UserPrivacySettings> {
        let row = sqlx::query!(
            r#"
            SELECT discoverable, show_following, show_followers
            FROM user_profile_details
            WHERE user_id = $1
            "#,
            user_id as &UserId,
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(
            row.map_or_else(UserPrivacySettings::default, |row| UserPrivacySettings {
                discoverable: row.discoverable,
                show_following: row.show_following,
                show_followers: row.show_followers,
            }),
        )
    }

    /// Applies the switches the caller sent and leaves the rest alone, so a
    /// client can flip one without having to echo the other two back.
    ///
    /// `COALESCE` on the excluded value is what makes the partial update work on
    /// the conflict path; the insert path needs the defaults spelled out because
    /// there is no existing row to fall back to.
    pub async fn update_user_privacy_settings(
        &self,
        user_id: &UserId,
        discoverable: Option<bool>,
        show_following: Option<bool>,
        show_followers: Option<bool>,
    ) -> Result<UserPrivacySettings> {
        let row = sqlx::query!(
            r#"
            INSERT INTO user_profile_details
                (user_id, discoverable, show_following, show_followers, updated_at)
            VALUES ($1, COALESCE($2, TRUE), COALESCE($3, TRUE), COALESCE($4, TRUE),
                    CURRENT_TIMESTAMP)
            ON CONFLICT (user_id) DO UPDATE SET
                discoverable = COALESCE($2, user_profile_details.discoverable),
                show_following = COALESCE($3, user_profile_details.show_following),
                show_followers = COALESCE($4, user_profile_details.show_followers),
                updated_at = CURRENT_TIMESTAMP
            RETURNING discoverable AS "discoverable!",
                      show_following AS "show_following!",
                      show_followers AS "show_followers!"
            "#,
            user_id as &UserId,
            discoverable,
            show_following,
            show_followers,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(UserPrivacySettings {
            discoverable: row.discoverable,
            show_following: row.show_following,
            show_followers: row.show_followers,
        })
    }

    /// Finds accounts by username for `viewer_user_id`.
    ///
    /// Usernames only: matching an email would let anyone confirm an address its
    /// owner never published. Accounts that turned discovery off are absent
    /// entirely rather than returned and hidden client-side, as are deleted and
    /// banned accounts, the viewer itself, and anyone either side has blocked —
    /// a search result whose only action is "blocked" is worse than no result.
    pub async fn search_discoverable_users(
        &self,
        viewer_user_id: &UserId,
        query: &str,
        pagination: PageParams,
    ) -> Result<(Vec<UserSearchHit>, i64)> {
        let trimmed = query.trim();
        let Some(pattern) = ilike_contains_pattern(trimmed) else {
            return Ok((Vec::new(), 0));
        };
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let mut count_builder: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT COUNT(*)
            FROM user_account_profiles p
            LEFT JOIN user_profile_details d ON d.user_id = p.id
            WHERE p.deleted_at IS NULL
              AND p.is_banned = FALSE
              AND COALESCE(d.discoverable, TRUE) = TRUE
              AND p.id <> "#,
        );
        count_builder.push_bind(viewer_user_id);
        count_builder.push(" AND p.username ILIKE ");
        count_builder.push_bind(pattern.clone());
        count_builder.push(
            r#" AND NOT EXISTS (
                    SELECT 1 FROM user_blocks b
                    WHERE (b.blocker_user_id = "#,
        );
        count_builder.push_bind(viewer_user_id);
        count_builder.push(
            " AND b.blocked_user_id = p.id) OR (b.blocker_user_id = p.id AND b.blocked_user_id = ",
        );
        count_builder.push_bind(viewer_user_id);
        count_builder.push("))");

        let total: i64 = count_builder
            .build_query_scalar()
            .fetch_one(self.pool())
            .await?;
        if total == 0 {
            return Ok((Vec::new(), 0));
        }

        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT p.id, p.username, p.signup_method, p.role, p.avatar_file_reference_id,
                   p.status, p.is_banned, p.banned_at, p.banned_by, p.banned_reason,
                   p.created_at, p.updated_at, p.version, p.deleted_at,
                   EXISTS (
                       SELECT 1 FROM friendships f
                       WHERE f.low_user_id = LEAST(p.id, "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(") AND f.high_user_id = GREATEST(p.id, ");
        builder.push_bind(viewer_user_id);
        builder.push(
            r#")
                   ) AS is_friend,
                   EXISTS (
                       SELECT 1 FROM friend_requests r
                       WHERE r.status = 'pending' AND r.requester_user_id = "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(
            r#" AND r.addressee_user_id = p.id
                   ) AS request_sent,
                   EXISTS (
                       SELECT 1 FROM friend_requests r
                       WHERE r.status = 'pending' AND r.addressee_user_id = "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(
            r#" AND r.requester_user_id = p.id
                   ) AS request_received,
                   EXISTS (
                       SELECT 1 FROM user_follows uf
                       WHERE uf.follower_user_id = "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(
            r#" AND uf.followee_user_id = p.id
                   ) AS is_following
            FROM user_account_profiles p
            LEFT JOIN user_profile_details d ON d.user_id = p.id
            WHERE p.deleted_at IS NULL
              AND p.is_banned = FALSE
              AND COALESCE(d.discoverable, TRUE) = TRUE
              AND p.id <> "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(" AND p.username ILIKE ");
        builder.push_bind(pattern);
        builder.push(
            r#" AND NOT EXISTS (
                    SELECT 1 FROM user_blocks b
                    WHERE (b.blocker_user_id = "#,
        );
        builder.push_bind(viewer_user_id);
        builder.push(
            " AND b.blocked_user_id = p.id) OR (b.blocker_user_id = p.id AND b.blocked_user_id = ",
        );
        builder.push_bind(viewer_user_id);
        builder.push("))");
        // An exact hit is what someone typing a full username is after, so it
        // leads regardless of how the rest sort.
        builder.push(" ORDER BY (LOWER(p.username) = LOWER(");
        builder.push_bind(trimmed.to_string());
        builder.push(")) DESC, LENGTH(p.username) ASC, p.username ASC, p.id ASC LIMIT ");
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder
            .build_query_as::<UserSearchRow>()
            .fetch_all(self.pool())
            .await?;

        Ok((rows.into_iter().map(UserSearchHit::from).collect(), total))
    }
}

#[derive(sqlx::FromRow)]
struct UserSearchRow {
    id: UserId,
    username: String,
    signup_method: SignupMethod,
    role: UserRole,
    avatar_file_reference_id: Option<i64>,
    status: UserStatus,
    is_banned: bool,
    banned_at: Option<chrono::DateTime<chrono::Utc>>,
    banned_by: Option<UserId>,
    banned_reason: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    version: i32,
    deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    is_friend: bool,
    request_sent: bool,
    request_received: bool,
    is_following: bool,
}

impl From<UserSearchRow> for UserSearchHit {
    fn from(row: UserSearchRow) -> Self {
        Self {
            user: User {
                id: row.id,
                username: row.username,
                signup_method: row.signup_method,
                role: row.role,
                avatar_file_reference_id: row.avatar_file_reference_id,
                status: row.status,
                is_banned: row.is_banned,
                banned_at: row.banned_at,
                banned_by: row.banned_by,
                banned_reason: row.banned_reason,
                created_at: row.created_at,
                updated_at: row.updated_at,
                version: row.version,
                deleted_at: row.deleted_at,
            },
            is_friend: row.is_friend,
            request_sent: row.request_sent,
            request_received: row.request_received,
            is_following: row.is_following,
        }
    }
}
