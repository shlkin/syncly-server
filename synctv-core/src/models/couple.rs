//! 情侣空间: a two-account shared space, its list, and what both members watched.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::{MediaId, RoomId, User, UserId};

/// Longest accepted space title.
pub const COUPLE_SPACE_TITLE_MAX_CHARS: usize = 100;
/// Longest accepted invite note.
pub const COUPLE_INVITE_MESSAGE_MAX_CHARS: usize = 200;
/// Longest accepted note on a shared favourite.
pub const COUPLE_FAVORITE_NOTE_MAX_CHARS: usize = 200;

/// Where a space is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoupleSpaceStatus {
    /// Invited, not yet accepted. Nothing is shared yet.
    Pending,
    Active,
    /// Unbound by either side. Kept readable, but no longer writable.
    Ended,
}

impl CoupleSpaceStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Ended => "ended",
        }
    }
}

impl std::str::FromStr for CoupleSpaceStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "ended" => Ok(Self::Ended),
            other => Err(format!("invalid couple space status: {other}")),
        }
    }
}

impl std::fmt::Display for CoupleSpaceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The stored row.
#[derive(Debug, Clone)]
pub struct CoupleSpace {
    pub id: i64,
    pub initiator_user_id: UserId,
    pub partner_user_id: UserId,
    pub status: CoupleSpaceStatus,
    pub title: String,
    pub invite_message: String,
    /// The date the "在一起 N 天" counter starts from. Set when the invite is
    /// accepted, and editable afterwards by either member.
    pub anniversary_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub bound_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// The space as one member sees it.
#[derive(Debug, Clone)]
pub struct CoupleSpaceView {
    pub space: CoupleSpace,
    /// The other account, whichever column it sits in.
    pub partner: User,
    /// True when the reader sent the invite. The pending screen differs: the
    /// initiator can only withdraw, the partner can accept or decline.
    pub viewer_is_initiator: bool,
    /// Whole days from the anniversary to today, inclusive. `None` until bound.
    pub days_together: Option<i64>,
    pub favorite_count: i64,
    /// How many titles both members have in their watch history.
    pub shared_watch_count: i64,
}

/// One entry of the couple's shared list.
#[derive(Debug, Clone)]
pub struct CoupleFavorite {
    pub id: i64,
    pub added_by_user_id: Option<UserId>,
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub source_key: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// Fields a member supplies when adding to the shared list.
#[derive(Debug, Clone)]
pub struct NewCoupleFavorite {
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub title: String,
    pub cover_url: String,
    pub source_key: String,
    pub note: String,
}

/// A title both members watched, derived by intersecting their personal watch
/// histories on `source_key`. There is no shared-playback table to read: room
/// playback history is keyed by room, not by viewer.
#[derive(Debug, Clone)]
pub struct SharedWatchEntry {
    pub source_key: String,
    pub title: String,
    pub cover_url: String,
    pub room_id: Option<RoomId>,
    pub media_id: Option<MediaId>,
    pub viewer_watched_at: DateTime<Utc>,
    pub partner_watched_at: DateTime<Utc>,
}
