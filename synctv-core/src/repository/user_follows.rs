//! Follow graph and public profile text.
//!
//! Modelled on the block graph in [`super::user`]: a two-column edge table with
//! the pair as its primary key, so following twice is idempotent and unfollowing
//! a stranger is a no-op rather than an error.

use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};

use super::query_builder::ilike_contains_pattern;
use super::user::UserRepository;
use crate::{
    models::{FollowedUser, PageParams, SignupMethod, User, UserId, UserRole, UserStatus},
    Result,
};

/// Counts and viewer-relative flags behind a profile page, read in one round trip.
#[derive(Debug, Clone)]
pub struct FollowProfileStats {
    pub signature: String,
    /// Stored file behind the page header, or [`None`] for the default backdrop.
    pub background_file_reference_id: Option<i64>,
    pub following_count: i64,
    pub follower_count: i64,
    pub viewer_following: bool,
    pub following_viewer: bool,
    /// Whether this profile lets anyone but its owner open the two lists. The
    /// counts are reported either way — hiding the number as well would make a
    /// private account look like an empty one.
    pub show_following: bool,
    pub show_followers: bool,
}

#[derive(sqlx::FromRow)]
struct FollowedUserListRow {
    id: UserId,
    username: String,
    signup_method: SignupMethod,
    role: UserRole,
    avatar_file_reference_id: Option<i64>,
    status: UserStatus,
    is_banned: bool,
    banned_at: Option<DateTime<Utc>>,
    banned_by: Option<UserId>,
    banned_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i32,
    deleted_at: Option<DateTime<Utc>>,
    followed_at: DateTime<Utc>,
    mutual: bool,
}

impl From<FollowedUserListRow> for FollowedUser {
    fn from(row: FollowedUserListRow) -> Self {
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
            followed_at: row.followed_at,
            mutual: row.mutual,
        }
    }
}

/// Which direction of the edge a listing walks.
#[derive(Clone, Copy)]
enum FollowDirection {
    /// Accounts the subject follows.
    Following,
    /// Accounts that follow the subject.
    Followers,
}

impl FollowDirection {
    /// Column holding the subject of the listing.
    const fn subject_column(self) -> &'static str {
        match self {
            Self::Following => "follower_user_id",
            Self::Followers => "followee_user_id",
        }
    }

    /// Column holding the account being listed.
    const fn listed_column(self) -> &'static str {
        match self {
            Self::Following => "followee_user_id",
            Self::Followers => "follower_user_id",
        }
    }
}

impl UserRepository {
    /// Records that `follower_user_id` follows `followee_user_id`, returning when
    /// the edge was first created.
    pub async fn follow_user(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<DateTime<Utc>> {
        let followed_at = sqlx::query_scalar!(
            r#"
            INSERT INTO user_follows (follower_user_id, followee_user_id)
            VALUES ($1, $2)
            ON CONFLICT (follower_user_id, followee_user_id)
            DO UPDATE SET follower_user_id = EXCLUDED.follower_user_id
            RETURNING created_at AS "created_at!"
            "#,
            follower_user_id as &UserId,
            followee_user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(followed_at)
    }

    pub async fn unfollow_user(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM user_follows WHERE follower_user_id = $1 AND followee_user_id = $2",
            follower_user_id as &UserId,
            followee_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn is_following(
        &self,
        follower_user_id: &UserId,
        followee_user_id: &UserId,
    ) -> Result<bool> {
        let is_following = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM user_follows
                WHERE follower_user_id = $1 AND followee_user_id = $2
            ) AS "is_following!"
            "#,
            follower_user_id as &UserId,
            followee_user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(is_following)
    }

    /// Reads the profile counters for `subject_user_id`, plus how it relates to
    /// `viewer_user_id`. A missing viewer leaves both flags false, which is what a
    /// signed-out reader should see.
    ///
    /// Soft-deleted accounts are excluded from the counts, so a page never claims
    /// followers the reader could not open.
    pub async fn follow_profile_stats(
        &self,
        subject_user_id: &UserId,
        viewer_user_id: Option<&UserId>,
    ) -> Result<FollowProfileStats> {
        let row = sqlx::query!(
            r#"
            SELECT
                COALESCE(
                    (SELECT d.signature FROM user_profile_details d WHERE d.user_id = $1),
                    ''
                ) AS "signature!",
                COALESCE(
                    (SELECT d.show_following FROM user_profile_details d WHERE d.user_id = $1),
                    TRUE
                ) AS "show_following!",
                COALESCE(
                    (SELECT d.show_followers FROM user_profile_details d WHERE d.user_id = $1),
                    TRUE
                ) AS "show_followers!",
                (
                    SELECT d.background_file_reference_id
                    FROM user_profile_details d
                    WHERE d.user_id = $1
                ) AS "background_file_reference_id?",
                (
                    SELECT COUNT(*)
                    FROM user_follows f
                    JOIN user_account_profiles p ON p.id = f.followee_user_id
                    WHERE f.follower_user_id = $1 AND p.deleted_at IS NULL
                ) AS "following_count!",
                (
                    SELECT COUNT(*)
                    FROM user_follows f
                    JOIN user_account_profiles p ON p.id = f.follower_user_id
                    WHERE f.followee_user_id = $1 AND p.deleted_at IS NULL
                ) AS "follower_count!",
                EXISTS (
                    SELECT 1 FROM user_follows
                    WHERE follower_user_id = $2 AND followee_user_id = $1
                ) AS "viewer_following!",
                EXISTS (
                    SELECT 1 FROM user_follows
                    WHERE follower_user_id = $1 AND followee_user_id = $2
                ) AS "following_viewer!"
            "#,
            subject_user_id as &UserId,
            viewer_user_id as Option<&UserId>,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(FollowProfileStats {
            signature: row.signature,
            background_file_reference_id: row.background_file_reference_id,
            following_count: row.following_count,
            follower_count: row.follower_count,
            viewer_following: row.viewer_following,
            following_viewer: row.following_viewer,
            show_following: row.show_following,
            show_followers: row.show_followers,
        })
    }

    /// Accounts `subject_user_id` follows, newest edge first.
    pub async fn list_following(
        &self,
        subject_user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<FollowedUser>, i64)> {
        self.list_follow_edges(
            FollowDirection::Following,
            subject_user_id,
            pagination,
            search,
        )
        .await
    }

    /// Accounts that follow `subject_user_id`, newest edge first.
    pub async fn list_followers(
        &self,
        subject_user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<FollowedUser>, i64)> {
        self.list_follow_edges(
            FollowDirection::Followers,
            subject_user_id,
            pagination,
            search,
        )
        .await
    }

    /// Shared body of the two listings: same joins and filters, opposite ends of
    /// the edge.
    async fn list_follow_edges(
        &self,
        direction: FollowDirection,
        subject_user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<FollowedUser>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let search_pattern = search.and_then(ilike_contains_pattern);
        let subject_column = direction.subject_column();
        let listed_column = direction.listed_column();

        let mut count_builder = QueryBuilder::<Postgres>::new(format!(
            "SELECT COUNT(*) FROM user_follows f \
             JOIN user_account_profiles p ON p.id = f.{listed_column} \
             WHERE f.{subject_column} = "
        ));
        count_builder.push_bind(subject_user_id);
        count_builder.push(" AND p.deleted_at IS NULL");
        if let Some(pattern) = search_pattern.as_ref() {
            count_builder
                .push(" AND p.username ILIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\'");
        }
        let total = count_builder
            .build_query_scalar::<i64>()
            .fetch_one(self.pool())
            .await?;

        let mut list_builder = QueryBuilder::<Postgres>::new(format!(
            "
            SELECT p.id,
                   p.username,
                   p.signup_method,
                   p.role,
                   p.avatar_file_reference_id,
                   p.status,
                   p.is_banned,
                   p.banned_at,
                   p.banned_by,
                   p.banned_reason,
                   p.created_at,
                   p.updated_at,
                   p.version,
                   p.deleted_at,
                   f.created_at AS followed_at,
                   EXISTS (
                       SELECT 1 FROM user_follows back
                       WHERE back.follower_user_id = f.{listed_column}
                         AND back.followee_user_id = f.{subject_column}
                   ) AS mutual
            FROM user_follows f
            JOIN user_account_profiles p ON p.id = f.{listed_column}
            WHERE f.{subject_column} = "
        ));
        list_builder.push_bind(subject_user_id);
        list_builder.push(" AND p.deleted_at IS NULL");
        if let Some(pattern) = search_pattern.as_ref() {
            list_builder
                .push(" AND p.username ILIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\'");
        }
        list_builder
            .push(format!(
                " ORDER BY f.created_at DESC, f.{listed_column} DESC LIMIT "
            ))
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let users = list_builder
            .build_query_as::<FollowedUserListRow>()
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

        Ok((users, total))
    }

    /// Reads the background reference behind `user_id`'s profile page and locks
    /// the row, so a concurrent change cannot leave the replaced object
    /// unreferenced and undeleted.
    pub async fn profile_background_reference_id_for_update<'e, E>(
        &self,
        user_id: &UserId,
        executor: E,
    ) -> Result<Option<i64>>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let reference_id = sqlx::query_scalar!(
            r#"
            SELECT background_file_reference_id
            FROM user_profile_details
            WHERE user_id = $1
            FOR UPDATE
            "#,
            user_id as &UserId,
        )
        .fetch_optional(executor)
        .await?
        .flatten();
        Ok(reference_id)
    }

    /// Points the profile page at `background_file_reference_id`, or back at the
    /// default backdrop when it is [`None`].
    pub async fn set_profile_background_with_executor<'e, E>(
        &self,
        user_id: &UserId,
        background_file_reference_id: Option<i64>,
        executor: E,
    ) -> Result<()>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query!(
            r#"
            INSERT INTO user_profile_details (user_id, background_file_reference_id, updated_at)
            VALUES ($1, $2, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id)
            DO UPDATE SET background_file_reference_id = EXCLUDED.background_file_reference_id,
                          updated_at = CURRENT_TIMESTAMP
            "#,
            user_id as &UserId,
            background_file_reference_id,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Stores the public signature, returning what was stored.
    pub async fn set_user_signature(&self, user_id: &UserId, signature: &str) -> Result<String> {
        let stored = sqlx::query_scalar!(
            r#"
            INSERT INTO user_profile_details (user_id, signature, updated_at)
            VALUES ($1, $2, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id)
            DO UPDATE SET signature = EXCLUDED.signature, updated_at = CURRENT_TIMESTAMP
            RETURNING signature AS "signature!"
            "#,
            user_id as &UserId,
            signature,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(stored)
    }
}
