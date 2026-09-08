//! Personal watch history and starred media.
//!
//! Both records denormalise the title and cover of what they point at, because
//! an entry has to keep reading correctly once the room or the media row is
//! gone. `source_key` is the caller-supplied identity of the thing watched: it
//! is what makes re-watching update a row instead of appending one.

use chrono::{DateTime, Utc};

use super::{MediaId, RoomId};

/// Longest accepted `title`, matching the column.
pub const WATCH_ENTRY_TITLE_MAX_CHARS: usize = 300;
/// Longest accepted `cover_url`, matching the column.
pub const WATCH_ENTRY_COVER_URL_MAX_CHARS: usize = 500;
/// Longest accepted `source_key`, matching the column.
pub const WATCH_ENTRY_SOURCE_KEY_MAX_CHARS: usize = 200;

/// One line of "观看历史".
#[derive(Debug, Clone)]
pub struct WatchHistoryEntry {
    pub id: i64,
    /// `None` once the room has been deleted: the entry still reads, but there
    /// is nowhere to resume.
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub position_seconds: f64,
    /// Zero when the client did not know the total length.
    pub duration_seconds: f64,
    pub source_key: String,
    pub watched_at: DateTime<Utc>,
}

/// A progress report from a client, folded into the history by `source_key`.
#[derive(Debug, Clone)]
pub struct RecordWatchHistory {
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub position_seconds: f64,
    pub duration_seconds: f64,
    pub source_key: String,
}

/// One entry of the media half of "我的收藏". Rooms are the other half and live
/// in `user_favorite_rooms`.
#[derive(Debug, Clone)]
pub struct MediaFavorite {
    pub id: i64,
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub source_key: String,
    pub created_at: DateTime<Utc>,
}

/// Fields a client supplies when starring a piece of media.
#[derive(Debug, Clone)]
pub struct NewMediaFavorite {
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub source_key: String,
}
