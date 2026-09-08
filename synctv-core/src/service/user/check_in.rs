//! Daily check-in and the points it awards.
//!
//! Two decisions live here rather than in the repository: which calendar day
//! "now" belongs to, and how many points a claim is worth. The first depends on
//! the configured day boundary, the second on the award mode — and a random
//! award has to be drawn once, at claim time, so the repository can store the
//! number it actually credited.

use chrono::{NaiveDate, TimeDelta, Utc};

use crate::{
    models::{
        CheckInAwardMode, CheckInConfig, CheckInConfigUpdate, CheckInResult, CheckInStatus,
        PageParams, PointTransaction, UserId, CHECK_IN_DAY_BOUNDARY_MAX_MINUTES,
        CHECK_IN_DAY_BOUNDARY_MIN_MINUTES, CHECK_IN_POINTS_MAX,
    },
    Error, Result,
};

use super::UserService;

impl UserService {
    /// The award rules, as an admin left them.
    pub async fn check_in_config(&self) -> Result<CheckInConfig> {
        self.repository.check_in_config().await
    }

    /// Applies an admin's partial update. Caller-side authorization decides who
    /// may reach this; what is checked here is that the numbers are usable.
    ///
    /// The ranges mirror the column constraints so a bad value comes back as a
    /// field error instead of a database CHECK violation, and the random range is
    /// validated against the *stored* bound when only one end is being moved.
    pub async fn update_check_in_config(
        &self,
        update: &CheckInConfigUpdate,
        actor_user_id: &UserId,
    ) -> Result<CheckInConfig> {
        let stored = self.repository.check_in_config().await?;
        if update.is_empty() {
            return Ok(stored);
        }

        for (field, value) in [
            ("fixed_points", update.fixed_points),
            ("random_min_points", update.random_min_points),
            ("random_max_points", update.random_max_points),
        ] {
            if let Some(points) = value {
                if !(0..=CHECK_IN_POINTS_MAX).contains(&points) {
                    return Err(Error::InvalidInput(format!(
                        "{field} must be between 0 and {CHECK_IN_POINTS_MAX}"
                    )));
                }
            }
        }

        let random_min = update.random_min_points.unwrap_or(stored.random_min_points);
        let random_max = update.random_max_points.unwrap_or(stored.random_max_points);
        if random_min > random_max {
            return Err(Error::InvalidInput(
                "random_min_points must not exceed random_max_points".to_string(),
            ));
        }

        if let Some(offset) = update.day_boundary_offset_minutes {
            if !(CHECK_IN_DAY_BOUNDARY_MIN_MINUTES..=CHECK_IN_DAY_BOUNDARY_MAX_MINUTES)
                .contains(&offset)
            {
                return Err(Error::InvalidInput(format!(
                    "day_boundary_offset_minutes must be between \
                     {CHECK_IN_DAY_BOUNDARY_MIN_MINUTES} and {CHECK_IN_DAY_BOUNDARY_MAX_MINUTES}"
                )));
            }
        }

        self.repository
            .update_check_in_config(update, actor_user_id)
            .await
    }

    /// Everything the check-in card renders: the rules, today's claim state, the
    /// streak, and the balance.
    pub async fn check_in_status(&self, user_id: &UserId) -> Result<CheckInStatus> {
        let config = self.repository.check_in_config().await?;
        let today = check_in_date_for(config.day_boundary_offset_minutes)?;
        self.repository
            .check_in_status(user_id, today, config)
            .await
    }

    /// Claims today's check-in.
    ///
    /// Claiming twice in one day is not an error: the repository reports the
    /// earlier claim and awards nothing, so a client that retries a lost response
    /// converges on the same answer.
    pub async fn claim_check_in(&self, user_id: &UserId) -> Result<CheckInResult> {
        let config = self.repository.check_in_config().await?;
        if !config.enabled {
            return Err(Error::Authorization(
                "Daily check-in is disabled".to_string(),
            ));
        }

        let today = check_in_date_for(config.day_boundary_offset_minutes)?;
        let points = award_points(&config);

        self.repository.claim_check_in(user_id, today, points).await
    }

    /// The point statement, newest movement first.
    pub async fn list_point_transactions(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<PointTransaction>, i64)> {
        self.repository
            .list_point_transactions(user_id, pagination)
            .await
    }
}

/// Which calendar day a claim made now belongs to.
///
/// The offset is what makes "one check-in per day" mean the user's day: with a
/// UTC boundary, a UTC+8 audience would see the day roll over at 08:00.
fn check_in_date_for(day_boundary_offset_minutes: i32) -> Result<NaiveDate> {
    let offset =
        TimeDelta::try_minutes(i64::from(day_boundary_offset_minutes)).ok_or_else(|| {
            Error::Internal("check-in day boundary offset is out of range".to_string())
        })?;

    Utc::now()
        .checked_add_signed(offset)
        .map(|shifted| shifted.date_naive())
        .ok_or_else(|| Error::Internal("check-in day boundary offset overflowed".to_string()))
}

/// The award for one claim. A random draw happens once, here, so what the ledger
/// records is what the user was actually told they received.
fn award_points(config: &CheckInConfig) -> i32 {
    match config.award_mode {
        CheckInAwardMode::Fixed => config.fixed_points.clamp(0, CHECK_IN_POINTS_MAX),
        CheckInAwardMode::Random => {
            let low = config.random_min_points.clamp(0, CHECK_IN_POINTS_MAX);
            let high = config.random_max_points.clamp(low, CHECK_IN_POINTS_MAX);
            if low == high {
                low
            } else {
                rand::random_range(low..=high)
            }
        }
    }
}
