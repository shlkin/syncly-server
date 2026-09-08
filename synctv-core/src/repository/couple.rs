//! 情侣空间: invite, bind, unbind, the shared list, and what both members watched.
//!
//! `couple_space_members` is the reason most of these writes are transactional:
//! it holds one row per member of an *active* space, keyed by the account, and
//! that key is what makes "at most one active space per person" true in the
//! database rather than only in the service.

use chrono::{DateTime, NaiveDate, Utc};

use super::user::UserRepository;
use crate::{
    models::{
        CoupleFavorite, CoupleSpace, CoupleSpaceStatus, MediaId, NewCoupleFavorite, PageParams,
        RoomId, SharedWatchEntry, UserId,
    },
    Error, Result,
};

/// A space as stored, before its status text is parsed.
struct CoupleSpaceRow {
    id: i64,
    initiator_user_id: UserId,
    partner_user_id: UserId,
    status: String,
    title: String,
    invite_message: String,
    anniversary_date: Option<NaiveDate>,
    created_at: DateTime<Utc>,
    bound_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
}

impl CoupleSpaceRow {
    fn into_space(self) -> Result<CoupleSpace> {
        Ok(CoupleSpace {
            id: self.id,
            initiator_user_id: self.initiator_user_id,
            partner_user_id: self.partner_user_id,
            status: self.status.parse::<CoupleSpaceStatus>().map_err(|error| {
                Error::Internal(format!("stored couple space status is invalid: {error}"))
            })?,
            title: self.title,
            invite_message: self.invite_message,
            anniversary_date: self.anniversary_date,
            created_at: self.created_at,
            bound_at: self.bound_at,
            ended_at: self.ended_at,
        })
    }
}

impl UserRepository {
    /// The space that concerns this account: the active one, or else the invite
    /// still waiting on an answer.
    pub async fn couple_space_for_user(&self, user_id: &UserId) -> Result<Option<CoupleSpace>> {
        let row = sqlx::query_as!(
            CoupleSpaceRow,
            r#"
            SELECT id,
                   initiator_user_id AS "initiator_user_id!: UserId",
                   partner_user_id AS "partner_user_id!: UserId",
                   status,
                   title,
                   invite_message,
                   anniversary_date,
                   created_at,
                   bound_at,
                   ended_at
            FROM couple_spaces
            WHERE (initiator_user_id = $1 OR partner_user_id = $1)
              AND status IN ('pending', 'active')
            ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END, created_at DESC, id DESC
            LIMIT 1
            "#,
            user_id as &UserId,
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(CoupleSpaceRow::into_space).transpose()
    }

    /// Reads one space by id, whatever its status.
    pub async fn couple_space(&self, space_id: i64) -> Result<Option<CoupleSpace>> {
        let row = sqlx::query_as!(
            CoupleSpaceRow,
            r#"
            SELECT id,
                   initiator_user_id AS "initiator_user_id!: UserId",
                   partner_user_id AS "partner_user_id!: UserId",
                   status,
                   title,
                   invite_message,
                   anniversary_date,
                   created_at,
                   bound_at,
                   ended_at
            FROM couple_spaces
            WHERE id = $1
            "#,
            space_id,
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(CoupleSpaceRow::into_space).transpose()
    }

    /// True when the account is in an active space.
    pub async fn has_active_couple_space(&self, user_id: &UserId) -> Result<bool> {
        let bound = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM couple_space_members WHERE user_id = $1
            ) AS "bound!"
            "#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(bound)
    }

    /// True when both accounts are in the same active space.
    ///
    /// `couple_space_members` only holds live memberships — ending a space
    /// removes the rows — so sharing a space id here means bound right now.
    pub async fn are_bound_couple(&self, first: &UserId, second: &UserId) -> Result<bool> {
        if first == second {
            return Ok(false);
        }
        let bound = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM couple_space_members AS a
                JOIN couple_space_members AS b
                  ON a.couple_space_id = b.couple_space_id
                WHERE a.user_id = $1 AND b.user_id = $2
            ) AS "bound!"
            "#,
            first as &UserId,
            second as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(bound)
    }

    /// Opens an invite. `None` means one is already pending in this direction.
    pub async fn create_couple_invite(
        &self,
        initiator_user_id: &UserId,
        partner_user_id: &UserId,
        title: &str,
        invite_message: &str,
    ) -> Result<Option<i64>> {
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO couple_spaces (
                initiator_user_id, partner_user_id, title, invite_message
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
            initiator_user_id as &UserId,
            partner_user_id as &UserId,
            title,
            invite_message,
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(id)
    }

    /// Binds the space: the invite becomes active and both accounts take their
    /// single active-membership slot.
    ///
    /// The membership insert is checked first so the caller gets "one of you is
    /// already bound" instead of a primary key violation, but the key is still
    /// what makes the guarantee hold under concurrency.
    pub async fn accept_couple_invite(
        &self,
        space_id: i64,
        partner_user_id: &UserId,
        anniversary_date: Option<NaiveDate>,
    ) -> Result<Option<CoupleSpace>> {
        let mut tx = self.pool().begin().await?;

        let Some(row) = sqlx::query_as!(
            CoupleSpaceRow,
            r#"
            UPDATE couple_spaces
            SET status = 'active',
                bound_at = CURRENT_TIMESTAMP,
                anniversary_date = COALESCE($3, anniversary_date, CURRENT_DATE)
            WHERE id = $1 AND partner_user_id = $2 AND status = 'pending'
            RETURNING id,
                      initiator_user_id AS "initiator_user_id!: UserId",
                      partner_user_id AS "partner_user_id!: UserId",
                      status,
                      title,
                      invite_message,
                      anniversary_date,
                      created_at,
                      bound_at,
                      ended_at
            "#,
            space_id,
            partner_user_id as &UserId,
            anniversary_date,
        )
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };

        let already_bound = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM couple_space_members WHERE user_id = $1 OR user_id = $2
            ) AS "bound!"
            "#,
            &row.initiator_user_id as &UserId,
            &row.partner_user_id as &UserId,
        )
        .fetch_one(&mut *tx)
        .await?;
        if already_bound {
            return Err(Error::Conflict(
                "One of these accounts is already in a couple space".to_string(),
            ));
        }

        sqlx::query!(
            r#"
            INSERT INTO couple_space_members (user_id, couple_space_id)
            VALUES ($1, $3), ($2, $3)
            "#,
            &row.initiator_user_id as &UserId,
            &row.partner_user_id as &UserId,
            space_id,
        )
        .execute(&mut *tx)
        .await?;

        // Every other invite either side had is moot now.
        sqlx::query!(
            r#"
            UPDATE couple_spaces
            SET status = 'ended', ended_at = CURRENT_TIMESTAMP
            WHERE status = 'pending'
              AND id <> $3
              AND (
                  initiator_user_id IN ($1, $2)
                  OR partner_user_id IN ($1, $2)
              )
            "#,
            &row.initiator_user_id as &UserId,
            &row.partner_user_id as &UserId,
            space_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        row.into_space().map(Some)
    }

    /// Turns down an invite the caller received, or withdraws one they sent.
    /// Both land in the same terminal state; only the predicate differs.
    pub async fn settle_pending_couple_invite(
        &self,
        space_id: i64,
        actor_user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE couple_spaces
            SET status = 'ended', ended_at = CURRENT_TIMESTAMP
            WHERE id = $1
              AND status = 'pending'
              AND (initiator_user_id = $2 OR partner_user_id = $2)
            "#,
            space_id,
            actor_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Unbinds an active space. The row and its shared list stay readable; what
    /// goes is the active membership, which frees both accounts to bind again.
    pub async fn end_couple_space(&self, space_id: i64, actor_user_id: &UserId) -> Result<bool> {
        let mut tx = self.pool().begin().await?;

        let ended = sqlx::query!(
            r#"
            UPDATE couple_spaces
            SET status = 'ended', ended_at = CURRENT_TIMESTAMP
            WHERE id = $1
              AND status = 'active'
              AND (initiator_user_id = $2 OR partner_user_id = $2)
            "#,
            space_id,
            actor_user_id as &UserId,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;

        if !ended {
            return Ok(false);
        }

        sqlx::query!(
            "DELETE FROM couple_space_members WHERE couple_space_id = $1",
            space_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(true)
    }

    /// Edits the title or the anniversary of an active space, from either side.
    pub async fn update_couple_space(
        &self,
        space_id: i64,
        actor_user_id: &UserId,
        title: Option<&str>,
        anniversary_date: Option<NaiveDate>,
    ) -> Result<Option<CoupleSpace>> {
        let row = sqlx::query_as!(
            CoupleSpaceRow,
            r#"
            UPDATE couple_spaces
            SET title = COALESCE($3, title),
                anniversary_date = COALESCE($4, anniversary_date)
            WHERE id = $1
              AND status = 'active'
              AND (initiator_user_id = $2 OR partner_user_id = $2)
            RETURNING id,
                      initiator_user_id AS "initiator_user_id!: UserId",
                      partner_user_id AS "partner_user_id!: UserId",
                      status,
                      title,
                      invite_message,
                      anniversary_date,
                      created_at,
                      bound_at,
                      ended_at
            "#,
            space_id,
            actor_user_id as &UserId,
            title,
            anniversary_date,
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(CoupleSpaceRow::into_space).transpose()
    }

    /// Adds to the shared list, or refreshes an entry already on it.
    pub async fn add_couple_favorite(
        &self,
        space_id: i64,
        added_by_user_id: &UserId,
        favorite: &NewCoupleFavorite,
    ) -> Result<CoupleFavorite> {
        let stored = sqlx::query_as!(
            CoupleFavorite,
            r#"
            INSERT INTO couple_favorites (
                couple_space_id, added_by_user_id, room_id, media_id,
                title, cover_url, source_key, note
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (couple_space_id, source_key) DO UPDATE
            SET room_id = EXCLUDED.room_id,
                media_id = EXCLUDED.media_id,
                title = EXCLUDED.title,
                cover_url = EXCLUDED.cover_url,
                note = EXCLUDED.note
            RETURNING id,
                      added_by_user_id AS "added_by_user_id?: UserId",
                      room_id AS "room_id?: RoomId",
                      media_id AS "media_id?: MediaId",
                      title,
                      cover_url,
                      source_key,
                      note,
                      created_at
            "#,
            space_id,
            added_by_user_id as &UserId,
            favorite.room_id.as_ref() as Option<&RoomId>,
            favorite.media_id.as_ref() as Option<&MediaId>,
            favorite.title,
            favorite.cover_url,
            favorite.source_key,
            favorite.note,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(stored)
    }

    /// Removes a shared entry by `source_key`, so either member can toggle it.
    pub async fn remove_couple_favorite(&self, space_id: i64, source_key: &str) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM couple_favorites WHERE couple_space_id = $1 AND source_key = $2",
            space_id,
            source_key,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// The shared list, newest first.
    pub async fn list_couple_favorites(
        &self,
        space_id: i64,
        pagination: PageParams,
    ) -> Result<(Vec<CoupleFavorite>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM couple_favorites WHERE couple_space_id = $1"#,
            space_id,
        )
        .fetch_one(self.pool())
        .await?;

        let favorites = sqlx::query_as!(
            CoupleFavorite,
            r#"
            SELECT id,
                   added_by_user_id AS "added_by_user_id?: UserId",
                   room_id AS "room_id?: RoomId",
                   media_id AS "media_id?: MediaId",
                   title,
                   cover_url,
                   source_key,
                   note,
                   created_at
            FROM couple_favorites
            WHERE couple_space_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
            space_id,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((favorites, total))
    }

    /// Titles both members have watched, newest activity first.
    ///
    /// Derived by intersecting the two personal histories on `source_key`: there
    /// is no shared-playback table, because room playback history is keyed by
    /// room rather than by viewer.
    pub async fn list_shared_watch_entries(
        &self,
        viewer_user_id: &UserId,
        partner_user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<SharedWatchEntry>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM user_watch_history mine
            JOIN user_watch_history theirs
              ON theirs.user_id = $2 AND theirs.source_key = mine.source_key
            WHERE mine.user_id = $1
            "#,
            viewer_user_id as &UserId,
            partner_user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        let entries = sqlx::query_as!(
            SharedWatchEntry,
            r#"
            SELECT mine.source_key,
                   mine.title,
                   mine.cover_url,
                   mine.room_id AS "room_id?: RoomId",
                   mine.media_id AS "media_id?: MediaId",
                   mine.watched_at AS viewer_watched_at,
                   theirs.watched_at AS partner_watched_at
            FROM user_watch_history mine
            JOIN user_watch_history theirs
              ON theirs.user_id = $2 AND theirs.source_key = mine.source_key
            WHERE mine.user_id = $1
            ORDER BY GREATEST(mine.watched_at, theirs.watched_at) DESC, mine.source_key DESC
            LIMIT $3 OFFSET $4
            "#,
            viewer_user_id as &UserId,
            partner_user_id as &UserId,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((entries, total))
    }

    /// The two counters on the space header, in one round trip.
    pub async fn couple_space_counts(
        &self,
        space_id: i64,
        viewer_user_id: &UserId,
        partner_user_id: &UserId,
    ) -> Result<(i64, i64)> {
        let row = sqlx::query!(
            r#"
            SELECT
                (
                    SELECT COUNT(*) FROM couple_favorites WHERE couple_space_id = $1
                ) AS "favorite_count!",
                (
                    SELECT COUNT(*)
                    FROM user_watch_history mine
                    JOIN user_watch_history theirs
                      ON theirs.user_id = $3 AND theirs.source_key = mine.source_key
                    WHERE mine.user_id = $2
                ) AS "shared_watch_count!"
            "#,
            space_id,
            viewer_user_id as &UserId,
            partner_user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok((row.favorite_count, row.shared_watch_count))
    }
}
