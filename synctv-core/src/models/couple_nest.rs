//! 爱的小窝: everything a bound pair keeps together beyond the two lists the
//! space already had.
//!
//! All of it is scoped to one `couple_space_id` and both members may read and
//! write all of it, so nothing here carries a per-row owner check — the only
//! question is "are you in this space", and the service answers it once.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::UserId;

// ---------------------------------------------------------------- 时光轴 ----

/// One dated entry on the timeline.
///
/// `happened_on` is the day being remembered, which is rarely the day it was
/// written: a trip gets written up afterwards, and the timeline orders by the
/// former.
#[derive(Debug, Clone)]
pub struct CoupleMemory {
    pub id: i64,
    pub space_id: i64,
    pub author_user_id: Option<UserId>,
    pub happened_on: NaiveDate,
    pub title: String,
    pub content: String,
    pub location: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCoupleMemory {
    pub happened_on: NaiveDate,
    pub title: String,
    pub content: String,
    pub location: String,
}

/// `None` leaves the stored value alone, so a form that only moved the date
/// does not have to resend the body.
#[derive(Debug, Clone, Default)]
pub struct CoupleMemoryUpdate {
    pub happened_on: Option<NaiveDate>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub location: Option<String>,
}

// ------------------------------------------------------------------ 相册 ----

#[derive(Debug, Clone)]
pub struct CoupleAlbum {
    pub id: i64,
    pub space_id: i64,
    pub created_by_user_id: Option<UserId>,
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Filled by the listing query, not stored.
    pub media_count: i64,
    /// Newest item in the album, for the cover tile.
    pub cover: Option<CoupleMedia>,
}

// ------------------------------------------------------------------ 媒体 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoupleMediaKind {
    Photo,
    Video,
}

impl CoupleMediaKind {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::Photo => 1,
            Self::Video => 2,
        }
    }

    #[must_use]
    pub const fn from_i16(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::Photo),
            2 => Some(Self::Video),
            _ => None,
        }
    }
}

/// One photo or video. The blob lives in file storage; this row keeps enough
/// shape to lay out a grid without fetching it.
#[derive(Debug, Clone)]
pub struct CoupleMedia {
    pub id: i64,
    pub space_id: i64,
    pub album_id: Option<i64>,
    pub memory_id: Option<i64>,
    pub uploader_user_id: Option<UserId>,
    pub kind: CoupleMediaKind,
    pub storage_backend: String,
    pub object_key: String,
    pub mime_type: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub taken_on: Option<NaiveDate>,
    pub caption: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCoupleMedia {
    pub album_id: Option<i64>,
    pub memory_id: Option<i64>,
    pub kind: CoupleMediaKind,
    pub storage_backend: String,
    pub object_key: String,
    pub mime_type: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub taken_on: Option<NaiveDate>,
    pub caption: String,
}

/// Which slice of the pair's media a listing wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoupleMediaScope {
    /// Everything in the space, newest first — what the album grid shows.
    All,
    /// One album.
    Album(i64),
    /// Attached to one timeline entry.
    Memory(i64),
    /// Not filed in any album.
    Unfiled,
}

// -------------------------------------------------------------- 纪念日 ----

/// A date the pair wants a countdown to. Separate from the space's own
/// `anniversary_date`, which is the single day the together-counter runs from.
#[derive(Debug, Clone)]
pub struct CoupleAnniversary {
    pub id: i64,
    pub space_id: i64,
    pub created_by_user_id: Option<UserId>,
    pub title: String,
    pub happens_on: NaiveDate,
    pub repeat_yearly: bool,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCoupleAnniversary {
    pub title: String,
    pub happens_on: NaiveDate,
    pub repeat_yearly: bool,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct CoupleAnniversaryUpdate {
    pub title: Option<String>,
    pub happens_on: Option<NaiveDate>,
    pub repeat_yearly: Option<bool>,
    pub note: Option<String>,
}

impl CoupleAnniversary {
    /// The next time this date comes round, on or after `today`.
    ///
    /// A one-off that has passed returns its own date, so the caller can show it
    /// as history rather than inventing a future for it. A yearly date lands on
    /// this year or next; 29 February in a common year falls back to the 28th,
    /// which is what a countdown should say rather than skipping three years.
    #[must_use]
    pub fn next_occurrence(&self, today: NaiveDate) -> NaiveDate {
        if !self.repeat_yearly {
            return self.happens_on;
        }
        let (month, day) = (
            chrono::Datelike::month(&self.happens_on),
            chrono::Datelike::day(&self.happens_on),
        );
        for year in [
            chrono::Datelike::year(&today),
            chrono::Datelike::year(&today) + 1,
        ] {
            let candidate = NaiveDate::from_ymd_opt(year, month, day)
                .or_else(|| NaiveDate::from_ymd_opt(year, month, day - 1));
            if let Some(candidate) = candidate.filter(|date| *date >= today) {
                return candidate;
            }
        }
        self.happens_on
    }
}

// ------------------------------------------------------------------ 清单 ----

#[derive(Debug, Clone)]
pub struct CoupleTodo {
    pub id: i64,
    pub space_id: i64,
    pub created_by_user_id: Option<UserId>,
    pub title: String,
    pub note: String,
    pub category: String,
    pub priority: i16,
    pub plan_date: Option<NaiveDate>,
    pub done_by_user_id: Option<UserId>,
    pub done_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCoupleTodo {
    pub title: String,
    pub note: String,
    pub category: String,
    pub priority: i16,
    pub plan_date: Option<NaiveDate>,
}

/// `done` is tri-state on the wire and here: absent leaves it, `Some(true)`
/// ticks it in the caller's name, `Some(false)` clears both the tick and who
/// made it.
#[derive(Debug, Clone, Default)]
pub struct CoupleTodoUpdate {
    pub title: Option<String>,
    pub note: Option<String>,
    pub category: Option<String>,
    pub priority: Option<i16>,
    pub plan_date: Option<Option<NaiveDate>>,
    pub done: Option<bool>,
}

// -------------------------------------------------------------- 宠物小屋 ----

#[derive(Debug, Clone)]
pub struct CouplePet {
    pub space_id: i64,
    pub name: String,
    pub species: i16,
    pub skin: i16,
    pub accessory: i16,
    pub stage: i16,
    pub experience: i32,
    /// Hunger and mood as they were at `settled_at`; both decay from there.
    pub hunger: i16,
    pub mood: i16,
    pub settled_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A pet as the screen sees it: stored values brought forward to now, plus the
/// streak the check-in rows imply.
#[derive(Debug, Clone)]
pub struct CouplePetView {
    pub pet: CouplePet,
    /// Decayed to the moment of reading.
    pub hunger: i16,
    pub mood: i16,
    /// Consecutive days ending today (or yesterday, if neither has been in yet
    /// today) on which at least one member interacted.
    pub streak_days: i64,
    /// True when both members have already been in today.
    pub both_checked_in_today: bool,
    pub experience_into_stage: i32,
    pub experience_for_next_stage: i32,
}

/// What one interaction does to the pet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouplePetAction {
    /// Fills hunger, a little experience.
    Feed,
    /// Lifts mood, a little experience.
    Play,
    /// Lifts mood most, once a day per member — this is the streak-keeping one.
    Pet,
}

#[derive(Debug, Clone, Default)]
pub struct CouplePetUpdate {
    pub name: Option<String>,
    pub species: Option<i16>,
    pub skin: Option<i16>,
    pub accessory: Option<i16>,
}

/// One day of the streak history panel.
#[derive(Debug, Clone)]
pub struct CouplePetCheckinDay {
    pub on_date: NaiveDate,
    /// How many of the two members were in that day.
    pub members: i64,
    pub interactions: i64,
}

// -------------------------------------------------------------- 心动 AI ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoupleAiKind {
    DatePlan,
    Diary,
    LoveLetter,
}

impl CoupleAiKind {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::DatePlan => 1,
            Self::Diary => 2,
            Self::LoveLetter => 3,
        }
    }

    #[must_use]
    pub const fn from_i16(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::DatePlan),
            2 => Some(Self::Diary),
            3 => Some(Self::LoveLetter),
            _ => None,
        }
    }
}

/// A generated piece, kept so it survives leaving the page.
#[derive(Debug, Clone)]
pub struct CoupleAiWork {
    pub id: i64,
    pub space_id: i64,
    pub author_user_id: Option<UserId>,
    pub kind: CoupleAiKind,
    pub title: String,
    /// What was asked, so a piece can be regenerated or explained later.
    pub prompt: serde_json::Value,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCoupleAiWork {
    pub kind: CoupleAiKind,
    pub title: String,
    pub prompt: serde_json::Value,
    pub content: String,
}

// ------------------------------------------------------------------ 首页 ----

/// Everything the nest's home screen needs, in one read.
///
/// Assembled server-side because the alternative is six round trips on the
/// slowest screen in the product — the one opened every time.
#[derive(Debug, Clone)]
pub struct CoupleNestHome {
    pub cover: Option<CoupleMedia>,
    /// The soonest upcoming date, already resolved through `next_occurrence`.
    pub next_anniversary: Option<CoupleAnniversary>,
    pub next_anniversary_on: Option<NaiveDate>,
    /// Newest shared media, capped for the home grid.
    pub recent_media: Vec<CoupleMedia>,
    /// Outstanding items, soonest first.
    pub open_todos: Vec<CoupleTodo>,
    pub open_todo_count: i64,
    pub done_todo_count: i64,
    pub memory_count: i64,
    pub media_count: i64,
    pub pet: Option<CouplePetView>,
}
