//! 观看历史 and the media half of 我的收藏.
//!
//! The repository writes what it is given, so the shape of a client-supplied
//! entry is settled here: `source_key` is the row's identity and must be a real
//! string, the denormalised text has to fit its column, and progress has to be a
//! finite number of seconds. Every check corresponds to a column constraint —
//! failing here turns a database error into a field error the client can act on.

use crate::{
    models::{
        MediaFavorite, NewMediaFavorite, PageParams, RecordWatchHistory, UserId, WatchHistoryEntry,
        WATCH_ENTRY_COVER_URL_MAX_CHARS, WATCH_ENTRY_SOURCE_KEY_MAX_CHARS,
        WATCH_ENTRY_TITLE_MAX_CHARS,
    },
    Error, Result,
};

use super::UserService;

impl UserService {
    /// Folds a progress report into the account's history.
    ///
    /// Watching something again updates its entry rather than appending one, so a
    /// player may call this as often as it likes.
    pub async fn record_watch_history(
        &self,
        user_id: &UserId,
        entry: &RecordWatchHistory,
    ) -> Result<WatchHistoryEntry> {
        let normalized = RecordWatchHistory {
            room_id: entry.room_id,
            media_id: entry.media_id,
            title: validate_entry_title(&entry.title)?,
            cover_url: validate_entry_cover_url(&entry.cover_url)?,
            position_seconds: validate_seconds("position_seconds", entry.position_seconds)?,
            duration_seconds: validate_seconds("duration_seconds", entry.duration_seconds)?,
            source_key: validate_entry_source_key(&entry.source_key)?,
        };

        self.repository
            .record_watch_history(user_id, &normalized)
            .await
    }

    /// One page of 观看历史, newest first.
    pub async fn list_watch_history(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<WatchHistoryEntry>, i64)> {
        self.repository
            .list_watch_history(user_id, pagination)
            .await
    }

    /// Removes one entry. False means the id named nothing this account owns,
    /// which is also what a second delete of the same entry returns.
    pub async fn delete_watch_history_entry(
        &self,
        user_id: &UserId,
        entry_id: i64,
    ) -> Result<bool> {
        self.repository
            .delete_watch_history_entry(user_id, entry_id)
            .await
    }

    /// Empties the history, returning how many entries went.
    pub async fn clear_watch_history(&self, user_id: &UserId) -> Result<u64> {
        self.repository.clear_watch_history(user_id).await
    }

    /// Stars a piece of media. Starring twice refreshes the stored title and
    /// cover instead of failing, so the button is a plain toggle.
    pub async fn add_media_favorite(
        &self,
        user_id: &UserId,
        favorite: &NewMediaFavorite,
    ) -> Result<MediaFavorite> {
        let normalized = NewMediaFavorite {
            room_id: favorite.room_id,
            media_id: favorite.media_id,
            title: validate_entry_title(&favorite.title)?,
            cover_url: validate_entry_cover_url(&favorite.cover_url)?,
            source_key: validate_entry_source_key(&favorite.source_key)?,
        };

        self.repository
            .add_media_favorite(user_id, &normalized)
            .await
    }

    /// Unstars by `source_key`, so the client can toggle without holding an id.
    pub async fn remove_media_favorite(&self, user_id: &UserId, source_key: &str) -> Result<bool> {
        let source_key = validate_entry_source_key(source_key)?;
        self.repository
            .remove_media_favorite(user_id, &source_key)
            .await
    }

    /// The media half of 我的收藏, newest first.
    pub async fn list_media_favorites(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<MediaFavorite>, i64)> {
        self.repository
            .list_media_favorites(user_id, pagination)
            .await
    }

    /// Which of these `source_key`s the account has starred, for rendering a
    /// list of cards without one query per card.
    pub async fn media_favorite_source_keys(
        &self,
        user_id: &UserId,
        source_keys: &[String],
    ) -> Result<Vec<String>> {
        if source_keys.is_empty() {
            return Ok(Vec::new());
        }
        for source_key in source_keys {
            validate_entry_source_key(source_key)?;
        }

        self.repository
            .media_favorite_source_keys(user_id, source_keys)
            .await
    }
}

/// The identity of the thing watched. Blank is rejected because the unique index
/// treats it as a real key, so an empty one would collapse every blank-keyed
/// entry of an account into a single row.
pub(super) fn validate_entry_source_key(source_key: &str) -> Result<String> {
    let trimmed = source_key.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("source_key is required".to_string()));
    }
    if trimmed.chars().count() > WATCH_ENTRY_SOURCE_KEY_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "source_key must be at most {WATCH_ENTRY_SOURCE_KEY_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

pub(super) fn validate_entry_title(title: &str) -> Result<String> {
    let trimmed = title.trim();
    if trimmed.chars().count() > WATCH_ENTRY_TITLE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "title must be at most {WATCH_ENTRY_TITLE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

pub(super) fn validate_entry_cover_url(cover_url: &str) -> Result<String> {
    let trimmed = cover_url.trim();
    if trimmed.chars().count() > WATCH_ENTRY_COVER_URL_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "cover_url must be at most {WATCH_ENTRY_COVER_URL_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Progress in seconds. Non-finite values are rejected rather than clamped: a
/// NaN position means the client lost track of where it was, and storing it
/// would make every later comparison on the column meaningless.
fn validate_seconds(field: &'static str, value: f64) -> Result<f64> {
    if !value.is_finite() {
        return Err(Error::InvalidInput(format!("{field} must be a number")));
    }
    if value < 0.0 {
        return Err(Error::InvalidInput(format!("{field} must not be negative")));
    }
    Ok(value)
}
