//! Personal watch history and starred media.
//!
//! Both tables are keyed by `(user_id, source_key)`, so the writes are upserts:
//! watching something again moves its entry to the top with new progress instead
//! of appending a duplicate, and starring something twice is a no-op.

use super::user::UserRepository;
use crate::{
    models::{
        MediaFavorite, MediaId, NewMediaFavorite, PageParams, RecordWatchHistory, RoomId, UserId,
        WatchHistoryEntry,
    },
    Result,
};

impl UserRepository {
    /// Folds a progress report into the history, returning the stored entry.
    pub async fn record_watch_history(
        &self,
        user_id: &UserId,
        entry: &RecordWatchHistory,
    ) -> Result<WatchHistoryEntry> {
        let stored = sqlx::query_as!(
            WatchHistoryEntry,
            r#"
            INSERT INTO user_watch_history (
                user_id, room_id, media_id, title, cover_url,
                position_seconds, duration_seconds, source_key, watched_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id, source_key) DO UPDATE
            SET room_id = EXCLUDED.room_id,
                media_id = EXCLUDED.media_id,
                title = EXCLUDED.title,
                cover_url = EXCLUDED.cover_url,
                position_seconds = EXCLUDED.position_seconds,
                duration_seconds = EXCLUDED.duration_seconds,
                watched_at = CURRENT_TIMESTAMP
            RETURNING id,
                      room_id AS "room_id?: RoomId",
                      media_id AS "media_id?: MediaId",
                      title,
                      cover_url,
                      position_seconds,
                      duration_seconds,
                      source_key,
                      watched_at
            "#,
            user_id as &UserId,
            entry.room_id.as_ref() as Option<&RoomId>,
            entry.media_id.as_ref() as Option<&MediaId>,
            entry.title,
            entry.cover_url,
            entry.position_seconds,
            entry.duration_seconds,
            entry.source_key,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(stored)
    }

    /// "观看历史": one account, newest first.
    pub async fn list_watch_history(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<WatchHistoryEntry>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM user_watch_history WHERE user_id = $1"#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        let entries = sqlx::query_as!(
            WatchHistoryEntry,
            r#"
            SELECT id,
                   room_id AS "room_id?: RoomId",
                   media_id AS "media_id?: MediaId",
                   title,
                   cover_url,
                   position_seconds,
                   duration_seconds,
                   source_key,
                   watched_at
            FROM user_watch_history
            WHERE user_id = $1
            ORDER BY watched_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id as &UserId,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((entries, total))
    }

    /// Drops one entry. The `user_id` predicate is what makes the entry id safe
    /// to accept from a client: another account's id simply matches nothing.
    pub async fn delete_watch_history_entry(
        &self,
        user_id: &UserId,
        entry_id: i64,
    ) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM user_watch_history WHERE user_id = $1 AND id = $2",
            user_id as &UserId,
            entry_id,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Empties the history, returning how many entries went.
    pub async fn clear_watch_history(&self, user_id: &UserId) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM user_watch_history WHERE user_id = $1",
            user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Stars a piece of media. Starring twice refreshes the stored title and
    /// cover and returns the original entry rather than failing.
    pub async fn add_media_favorite(
        &self,
        user_id: &UserId,
        favorite: &NewMediaFavorite,
    ) -> Result<MediaFavorite> {
        let stored = sqlx::query_as!(
            MediaFavorite,
            r#"
            INSERT INTO user_media_favorites (
                user_id, room_id, media_id, title, cover_url, source_key
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id, source_key) DO UPDATE
            SET room_id = EXCLUDED.room_id,
                media_id = EXCLUDED.media_id,
                title = EXCLUDED.title,
                cover_url = EXCLUDED.cover_url
            RETURNING id,
                      room_id AS "room_id?: RoomId",
                      media_id AS "media_id?: MediaId",
                      title,
                      cover_url,
                      source_key,
                      created_at
            "#,
            user_id as &UserId,
            favorite.room_id.as_ref() as Option<&RoomId>,
            favorite.media_id.as_ref() as Option<&MediaId>,
            favorite.title,
            favorite.cover_url,
            favorite.source_key,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(stored)
    }

    /// Unstars by `source_key`, so a client can toggle without holding an entry
    /// id. Returns false when nothing was starred.
    pub async fn remove_media_favorite(&self, user_id: &UserId, source_key: &str) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM user_media_favorites WHERE user_id = $1 AND source_key = $2",
            user_id as &UserId,
            source_key,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// The media half of "我的收藏", newest first.
    pub async fn list_media_favorites(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<MediaFavorite>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM user_media_favorites WHERE user_id = $1"#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        let favorites = sqlx::query_as!(
            MediaFavorite,
            r#"
            SELECT id,
                   room_id AS "room_id?: RoomId",
                   media_id AS "media_id?: MediaId",
                   title,
                   cover_url,
                   source_key,
                   created_at
            FROM user_media_favorites
            WHERE user_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id as &UserId,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((favorites, total))
    }

    /// Whether these `source_key`s are starred, for rendering a list of cards
    /// without one query per card.
    pub async fn media_favorite_source_keys(
        &self,
        user_id: &UserId,
        source_keys: &[String],
    ) -> Result<Vec<String>> {
        let keys = sqlx::query_scalar!(
            r#"
            SELECT source_key
            FROM user_media_favorites
            WHERE user_id = $1 AND source_key = ANY($2)
            "#,
            user_id as &UserId,
            source_keys,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(keys)
    }
}
