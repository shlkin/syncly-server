//! Daily check-in rules, claims, and the point ledger they feed.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::UserId;

/// Ledger reason recorded for points granted by a daily check-in.
pub const POINT_REASON_CHECK_IN: &str = "check_in";

/// Largest award an admin may configure, matching the column's CHECK.
pub const CHECK_IN_POINTS_MAX: i32 = 100_000;
/// Westmost day boundary an admin may configure, in minutes from UTC (UTC-12).
pub const CHECK_IN_DAY_BOUNDARY_MIN_MINUTES: i32 = -720;
/// Eastmost day boundary an admin may configure, in minutes from UTC (UTC+14).
pub const CHECK_IN_DAY_BOUNDARY_MAX_MINUTES: i32 = 840;

/// How a claim's award is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CheckInAwardMode {
    /// Every claim awards [`CheckInConfig::fixed_points`].
    #[default]
    Fixed,
    /// Every claim draws uniformly from the configured inclusive range.
    Random,
}

impl CheckInAwardMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Random => "random",
        }
    }
}

impl std::str::FromStr for CheckInAwardMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fixed" => Ok(Self::Fixed),
            "random" => Ok(Self::Random),
            other => Err(format!("invalid check-in award mode: {other}")),
        }
    }
}

impl std::fmt::Display for CheckInAwardMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Admin-owned award rules, stored as a single row.
#[derive(Debug, Clone)]
pub struct CheckInConfig {
    /// False disables claiming entirely; the client still renders the card.
    pub enabled: bool,
    pub award_mode: CheckInAwardMode,
    pub fixed_points: i32,
    pub random_min_points: i32,
    pub random_max_points: i32,
    /// Minutes east of UTC that decide which calendar day a claim lands on.
    pub day_boundary_offset_minutes: i32,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
}

/// Partial update of [`CheckInConfig`]; `None` leaves a field as stored.
#[derive(Debug, Clone, Default)]
pub struct CheckInConfigUpdate {
    pub enabled: Option<bool>,
    pub award_mode: Option<CheckInAwardMode>,
    pub fixed_points: Option<i32>,
    pub random_min_points: Option<i32>,
    pub random_max_points: Option<i32>,
    pub day_boundary_offset_minutes: Option<i32>,
}

impl CheckInConfigUpdate {
    /// True when the update would change nothing, so the caller can skip a write.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.award_mode.is_none()
            && self.fixed_points.is_none()
            && self.random_min_points.is_none()
            && self.random_max_points.is_none()
            && self.day_boundary_offset_minutes.is_none()
    }
}

/// What the check-in card renders before the user taps it.
#[derive(Debug, Clone)]
pub struct CheckInStatus {
    pub config: CheckInConfig,
    /// The calendar day the server would credit a claim to right now.
    pub today: NaiveDate,
    pub checked_in_today: bool,
    /// Consecutive days including today when already claimed, else the streak
    /// that a claim today would continue.
    pub streak: i32,
    pub last_check_in_date: Option<NaiveDate>,
    pub balance: i64,
    pub total_earned: i64,
}

/// Outcome of a claim.
#[derive(Debug, Clone)]
pub struct CheckInResult {
    pub check_in_date: NaiveDate,
    pub points_awarded: i32,
    pub streak: i32,
    pub balance: i64,
    pub total_earned: i64,
    /// True when today's claim already existed. The response then describes that
    /// earlier claim, so a retried request is not a failure and awards nothing.
    pub already_checked_in: bool,
}

/// One movement in the point ledger.
#[derive(Debug, Clone)]
pub struct PointTransaction {
    pub id: i64,
    /// Signed: positive for awards, negative for spending.
    pub delta: i64,
    pub balance_after: i64,
    pub reason: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}
