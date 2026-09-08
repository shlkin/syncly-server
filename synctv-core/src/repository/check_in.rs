//! Daily check-in claims, the point ledger, and the admin award rules.
//!
//! The claim is the only write here that has to be atomic across three tables,
//! so it runs in a transaction: the `user_check_ins` primary key decides whether
//! this is the day's first claim, and only that branch touches the balance and
//! the ledger. A retried request therefore reads back the earlier claim instead
//! of awarding twice.

use chrono::NaiveDate;

use super::user::UserRepository;
use crate::{
    models::{
        CheckInAwardMode, CheckInConfig, CheckInConfigUpdate, CheckInResult, CheckInStatus,
        PageParams, PointTransaction, UserId, POINT_REASON_CHECK_IN,
    },
    Error, Result,
};

/// Balance and lifetime earnings, as stored.
#[derive(Debug, Clone, Copy)]
struct PointTotals {
    balance: i64,
    total_earned: i64,
}

impl UserRepository {
    /// Reads the single configuration row.
    pub async fn check_in_config(&self) -> Result<CheckInConfig> {
        let row = sqlx::query!(
            r#"
            SELECT enabled,
                   award_mode,
                   fixed_points,
                   random_min_points,
                   random_max_points,
                   day_boundary_offset_minutes,
                   updated_at,
                   updated_by AS "updated_by?: UserId"
            FROM check_in_config
            WHERE id = 1
            "#
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| Error::Internal("check-in configuration row is missing".to_string()))?;

        Ok(CheckInConfig {
            enabled: row.enabled,
            award_mode: row
                .award_mode
                .parse::<CheckInAwardMode>()
                .map_err(|error| {
                    Error::Internal(format!("stored check-in configuration is invalid: {error}"))
                })?,
            fixed_points: row.fixed_points,
            random_min_points: row.random_min_points,
            random_max_points: row.random_max_points,
            day_boundary_offset_minutes: row.day_boundary_offset_minutes,
            updated_at: row.updated_at,
            updated_by: row.updated_by,
        })
    }

    /// Applies a partial update and returns the stored result.
    ///
    /// `COALESCE` per column keeps the untouched fields at their stored values
    /// without a read-modify-write, so two admins editing different fields do not
    /// overwrite each other.
    pub async fn update_check_in_config(
        &self,
        update: &CheckInConfigUpdate,
        actor_user_id: &UserId,
    ) -> Result<CheckInConfig> {
        sqlx::query!(
            r#"
            UPDATE check_in_config
            SET enabled = COALESCE($1, enabled),
                award_mode = COALESCE($2, award_mode),
                fixed_points = COALESCE($3, fixed_points),
                random_min_points = COALESCE($4, random_min_points),
                random_max_points = COALESCE($5, random_max_points),
                day_boundary_offset_minutes = COALESCE($6, day_boundary_offset_minutes),
                updated_at = CURRENT_TIMESTAMP,
                updated_by = $7
            WHERE id = 1
            "#,
            update.enabled,
            update.award_mode.map(|mode| mode.as_str().to_string()),
            update.fixed_points,
            update.random_min_points,
            update.random_max_points,
            update.day_boundary_offset_minutes,
            actor_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        self.check_in_config().await
    }

    /// Everything the check-in card needs for `today`, in one round trip.
    ///
    /// `today` is supplied by the caller because which calendar day "now" belongs
    /// to depends on the configured day boundary, which is a service concern.
    pub async fn check_in_status(
        &self,
        user_id: &UserId,
        today: NaiveDate,
        config: CheckInConfig,
    ) -> Result<CheckInStatus> {
        let row = sqlx::query!(
            r#"
            SELECT
                (
                    SELECT streak FROM user_check_ins
                    WHERE user_id = $1 AND check_in_date = $2::date
                ) AS "today_streak?",
                (
                    SELECT streak FROM user_check_ins
                    WHERE user_id = $1 AND check_in_date = $2::date - 1
                ) AS "yesterday_streak?",
                (
                    SELECT MAX(check_in_date) FROM user_check_ins WHERE user_id = $1
                ) AS "last_check_in_date?",
                COALESCE(
                    (SELECT balance FROM user_point_balances WHERE user_id = $1), 0
                ) AS "balance!",
                COALESCE(
                    (SELECT total_earned FROM user_point_balances WHERE user_id = $1), 0
                ) AS "total_earned!"
            "#,
            user_id as &UserId,
            today,
        )
        .fetch_one(self.pool())
        .await?;

        // A streak that ended yesterday is still standing: it is what a claim
        // today would extend. One that ended earlier is broken, so it reads zero.
        let streak = row.today_streak.or(row.yesterday_streak).unwrap_or(0);

        Ok(CheckInStatus {
            config,
            today,
            checked_in_today: row.today_streak.is_some(),
            streak,
            last_check_in_date: row.last_check_in_date,
            balance: row.balance,
            total_earned: row.total_earned,
        })
    }

    /// Records today's claim, awarding `points` the first time it is called for a
    /// given day.
    ///
    /// The day's row is inserted with `ON CONFLICT DO NOTHING`: if it is already
    /// there, this call reads that claim back and reports `already_checked_in`
    /// rather than failing, so a client that retries a lost response converges.
    pub async fn claim_check_in(
        &self,
        user_id: &UserId,
        today: NaiveDate,
        points: i32,
    ) -> Result<CheckInResult> {
        let mut tx = self.pool().begin().await?;

        // Yesterday's streak decides today's, read inside the transaction so it
        // cannot change between the read and the insert.
        let previous_streak = sqlx::query_scalar!(
            r#"
            SELECT streak
            FROM user_check_ins
            WHERE user_id = $1 AND check_in_date = $2::date - 1
            "#,
            user_id as &UserId,
            today,
        )
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);

        let inserted = sqlx::query!(
            r#"
            INSERT INTO user_check_ins (user_id, check_in_date, points_awarded, streak)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, check_in_date) DO NOTHING
            RETURNING points_awarded, streak
            "#,
            user_id as &UserId,
            today,
            points,
            previous_streak.saturating_add(1),
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(claim) = inserted else {
            let existing = sqlx::query!(
                r#"
                SELECT points_awarded, streak
                FROM user_check_ins
                WHERE user_id = $1 AND check_in_date = $2::date
                "#,
                user_id as &UserId,
                today,
            )
            .fetch_one(&mut *tx)
            .await?;
            let totals = Self::read_point_totals(&mut *tx, user_id).await?;
            tx.commit().await?;

            return Ok(CheckInResult {
                check_in_date: today,
                points_awarded: existing.points_awarded,
                streak: existing.streak,
                balance: totals.balance,
                total_earned: totals.total_earned,
                already_checked_in: true,
            });
        };

        // A zero award is legal (an admin may set it) but must not reach the
        // ledger, whose rows are movements and are constrained to be non-zero.
        let totals = if claim.points_awarded > 0 {
            let awarded = i64::from(claim.points_awarded);
            let totals = sqlx::query_as!(
                PointTotals,
                r#"
                INSERT INTO user_point_balances (user_id, balance, total_earned)
                VALUES ($1, $2, $2)
                ON CONFLICT (user_id) DO UPDATE
                SET balance = user_point_balances.balance + EXCLUDED.balance,
                    total_earned = user_point_balances.total_earned + EXCLUDED.total_earned,
                    updated_at = CURRENT_TIMESTAMP
                RETURNING balance, total_earned
                "#,
                user_id as &UserId,
                awarded,
            )
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query!(
                r#"
                INSERT INTO user_point_transactions (user_id, delta, balance_after, reason)
                VALUES ($1, $2, $3, $4)
                "#,
                user_id as &UserId,
                awarded,
                totals.balance,
                POINT_REASON_CHECK_IN,
            )
            .execute(&mut *tx)
            .await?;

            totals
        } else {
            Self::read_point_totals(&mut *tx, user_id).await?
        };

        tx.commit().await?;

        Ok(CheckInResult {
            check_in_date: today,
            points_awarded: claim.points_awarded,
            streak: claim.streak,
            balance: totals.balance,
            total_earned: totals.total_earned,
            already_checked_in: false,
        })
    }

    /// The point statement, newest first.
    pub async fn list_point_transactions(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<PointTransaction>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "total!" FROM user_point_transactions WHERE user_id = $1"#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        let transactions = sqlx::query_as!(
            PointTransaction,
            r#"
            SELECT id, delta, balance_after, reason, description, created_at
            FROM user_point_transactions
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

        Ok((transactions, total))
    }

    /// Balance and lifetime earnings, defaulting to zero for an account that has
    /// never earned anything and therefore has no row.
    async fn read_point_totals(
        executor: impl sqlx::PgExecutor<'_>,
        user_id: &UserId,
    ) -> Result<PointTotals> {
        let totals = sqlx::query_as!(
            PointTotals,
            r#"
            SELECT COALESCE(balance, 0) AS "balance!",
                   COALESCE(total_earned, 0) AS "total_earned!"
            FROM user_point_balances
            WHERE user_id = $1
            "#,
            user_id as &UserId,
        )
        .fetch_optional(executor)
        .await?
        .unwrap_or(PointTotals {
            balance: 0,
            total_earned: 0,
        });
        Ok(totals)
    }
}
