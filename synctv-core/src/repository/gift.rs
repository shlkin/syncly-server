//! The gift catalog and the sends that spend points.
//!
//! `send_gift` moves points between two accounts, and has to be atomic across
//! four tables, so it runs in a transaction shaped like
//! [`UserRepository::claim_check_in`]: the balance row is locked, the debit is
//! refused if it would overdraw, and only then are the ledger row and the gift
//! record written. `gift_records.request_id` makes a retried send converge on
//! the first one instead of charging twice.

use chrono::{DateTime, Utc};

use crate::{
    models::{
        Gift, GiftRecord, GiftRecordDirection, GiftRecordView, GiftStats, GiftUpdate, NewGift,
        PageParams, RoomId, SendGift, SendGiftResult, UserId, POINT_REASON_GIFT_RECEIVE,
        POINT_REASON_GIFT_SEND,
    },
    Error, Result,
};

use super::user::UserRepository;

/// Flat projection shared by the record listings, which differ only in their
/// `WHERE` clause.
struct GiftRecordRow {
    id: i64,
    sender_id: UserId,
    recipient_id: Option<UserId>,
    room_id: Option<RoomId>,
    gift_id: i64,
    gift_key: String,
    gift_name: String,
    gift_icon: String,
    unit_price_points: i32,
    quantity: i32,
    total_points: i64,
    transaction_id: Option<i64>,
    message: String,
    created_at: DateTime<Utc>,
    sender_username: String,
    recipient_username: Option<String>,
}

impl From<GiftRecordRow> for GiftRecordView {
    fn from(row: GiftRecordRow) -> Self {
        Self {
            record: GiftRecord {
                id: row.id,
                sender_id: row.sender_id,
                recipient_id: row.recipient_id,
                room_id: row.room_id,
                gift_id: row.gift_id,
                gift_key: row.gift_key,
                gift_name: row.gift_name,
                gift_icon: row.gift_icon,
                unit_price_points: row.unit_price_points,
                quantity: row.quantity,
                total_points: row.total_points,
                transaction_id: row.transaction_id,
                message: row.message,
                created_at: row.created_at,
            },
            sender_username: row.sender_username,
            recipient_username: row.recipient_username,
        }
    }
}

impl UserRepository {
    /// The catalog, in display order.
    ///
    /// `enabled_only` is what the send panel passes; an admin screen passes
    /// false so a disabled entry can still be edited.
    pub async fn list_gifts(&self, enabled_only: bool) -> Result<Vec<Gift>> {
        let gifts = sqlx::query_as!(
            Gift,
            r#"
            SELECT id,
                   gift_key AS key,
                   name,
                   description,
                   icon,
                   price_points,
                   sort_order,
                   is_enabled,
                   created_at,
                   updated_at,
                   updated_by AS "updated_by?: UserId"
            FROM gift_catalog
            WHERE NOT $1::bool OR is_enabled
            ORDER BY sort_order, id
            "#,
            enabled_only,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(gifts)
    }

    /// One catalog entry by id.
    pub async fn find_gift(&self, gift_id: i64) -> Result<Option<Gift>> {
        let gift = sqlx::query_as!(
            Gift,
            r#"
            SELECT id,
                   gift_key AS key,
                   name,
                   description,
                   icon,
                   price_points,
                   sort_order,
                   is_enabled,
                   created_at,
                   updated_at,
                   updated_by AS "updated_by?: UserId"
            FROM gift_catalog
            WHERE id = $1
            "#,
            gift_id,
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(gift)
    }

    /// Adds a catalog entry.
    ///
    /// A duplicate key is a caller error rather than an internal one: admins
    /// pick the key, so the conflict is reported as such.
    pub async fn create_gift(&self, gift: &NewGift, actor_user_id: &UserId) -> Result<Gift> {
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO gift_catalog
                (gift_key, name, description, icon, price_points, sort_order, is_enabled, updated_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (gift_key) DO NOTHING
            RETURNING id
            "#,
            gift.key,
            gift.name,
            gift.description,
            gift.icon,
            gift.price_points,
            gift.sort_order,
            gift.is_enabled,
            actor_user_id as &UserId,
        )
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| {
            Error::AlreadyExists(format!("gift key `{}` is already used", gift.key))
        })?;

        self.find_gift(id)
            .await?
            .ok_or_else(|| Error::Internal("inserted gift disappeared".to_string()))
    }

    /// Applies a partial update.
    ///
    /// `COALESCE` per column leaves untouched fields as stored, so two admins
    /// editing different fields do not overwrite each other.
    pub async fn update_gift(
        &self,
        gift_id: i64,
        update: &GiftUpdate,
        actor_user_id: &UserId,
    ) -> Result<Gift> {
        let affected = sqlx::query!(
            r#"
            UPDATE gift_catalog
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                icon = COALESCE($4, icon),
                price_points = COALESCE($5, price_points),
                sort_order = COALESCE($6, sort_order),
                is_enabled = COALESCE($7, is_enabled),
                updated_at = CURRENT_TIMESTAMP,
                updated_by = $8
            WHERE id = $1
            "#,
            gift_id,
            update.name.as_deref(),
            update.description.as_deref(),
            update.icon.as_deref(),
            update.price_points,
            update.sort_order,
            update.is_enabled,
            actor_user_id as &UserId,
        )
        .execute(self.pool())
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(Error::NotFound("gift not found".to_string()));
        }

        self.find_gift(gift_id)
            .await?
            .ok_or_else(|| Error::Internal("updated gift disappeared".to_string()))
    }

    /// Removes a catalog entry that has never been sent.
    ///
    /// The foreign key is `ON DELETE RESTRICT`, so an entry with history cannot
    /// be deleted; that is reported as a conflict telling the admin to disable
    /// it instead, which keeps records readable.
    pub async fn delete_gift(&self, gift_id: i64) -> Result<()> {
        let sent = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM gift_records WHERE gift_id = $1) AS "sent!""#,
            gift_id,
        )
        .fetch_one(self.pool())
        .await?;

        if sent {
            return Err(Error::Conflict(
                "gift has already been sent; disable it instead of deleting it".to_string(),
            ));
        }

        let affected = sqlx::query!("DELETE FROM gift_catalog WHERE id = $1", gift_id)
            .execute(self.pool())
            .await?
            .rows_affected();

        if affected == 0 {
            return Err(Error::NotFound("gift not found".to_string()));
        }

        Ok(())
    }
}

impl UserRepository {
    /// Sends a gift: snapshots the catalog entry, debits the sender, credits the
    /// recipient the same amount, writes both ledger rows, and records the
    /// send — all of it or none of it.
    ///
    /// The points are transferred, not burned. That means a pair of accounts can
    /// pass the same balance back and forth for free; it is the behaviour the
    /// owner asked for, and both halves are in the ledger so it is visible.
    /// `total_earned` is deliberately not credited — see the 20260906003
    /// migration.
    ///
    /// A `request_id` already on file short-circuits to that record and charges
    /// nothing. Two *concurrent* sends sharing one key are not merged: the
    /// partial unique index rejects the second insert and the transaction rolls
    /// back, so the worst case is a failed retry, never a double charge.
    pub async fn send_gift(&self, request: &SendGift) -> Result<SendGiftResult> {
        let mut tx = self.pool().begin().await?;

        if let Some(request_id) = request.request_id.as_deref() {
            let existing = sqlx::query!(
                r#"
                SELECT id,
                       recipient_id AS "recipient_id?: UserId",
                       room_id AS "room_id?: RoomId",
                       gift_id,
                       gift_key,
                       gift_name,
                       gift_icon,
                       unit_price_points,
                       quantity,
                       total_points,
                       transaction_id,
                       message,
                       created_at
                FROM gift_records
                WHERE sender_id = $1 AND request_id = $2
                "#,
                &request.sender_id as &UserId,
                request_id,
            )
            .fetch_optional(&mut *tx)
            .await?;

            if let Some(row) = existing {
                let balance = Self::read_gift_balance(&mut *tx, &request.sender_id).await?;
                tx.commit().await?;

                return Ok(SendGiftResult {
                    record: GiftRecord {
                        id: row.id,
                        sender_id: request.sender_id,
                        recipient_id: row.recipient_id,
                        room_id: row.room_id,
                        gift_id: row.gift_id,
                        gift_key: row.gift_key,
                        gift_name: row.gift_name,
                        gift_icon: row.gift_icon,
                        unit_price_points: row.unit_price_points,
                        quantity: row.quantity,
                        total_points: row.total_points,
                        transaction_id: row.transaction_id,
                        message: row.message,
                        created_at: row.created_at,
                    },
                    balance,
                    already_sent: true,
                });
            }
        }
        // Read inside the transaction: a repricing must not land between the
        // amount charged and the amount recorded.
        let gift = sqlx::query!(
            r#"
            SELECT gift_key, name, icon, price_points, is_enabled
            FROM gift_catalog
            WHERE id = $1
            "#,
            request.gift_id,
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| Error::NotFound("gift not found".to_string()))?;

        if !gift.is_enabled {
            return Err(Error::Conflict("gift is no longer available".to_string()));
        }

        let total = i64::from(gift.price_points) * i64::from(request.quantity);

        // A free gift is legal — an admin may price one at zero — but must not
        // reach the ledger, whose rows are movements and cannot be zero.
        let (balance, transaction_id, recipient_transaction_id) = if total > 0 {
            // The WHERE clause *is* the overdraft check. Guarding on the row
            // itself means two concurrent sends cannot both pass on one balance,
            // and the column's CHECK never has to fire.
            let balance = sqlx::query_scalar!(
                r#"
                UPDATE user_point_balances
                SET balance = balance - $2,
                    updated_at = CURRENT_TIMESTAMP
                WHERE user_id = $1 AND balance >= $2
                RETURNING balance
                "#,
                &request.sender_id as &UserId,
                total,
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| Error::Conflict("insufficient points for this gift".to_string()))?;

            let transaction_id = sqlx::query_scalar!(
                r#"
                INSERT INTO user_point_transactions
                    (user_id, delta, balance_after, reason, description)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                "#,
                &request.sender_id as &UserId,
                -total,
                balance,
                POINT_REASON_GIFT_SEND,
                gift.name.as_str(),
            )
            .fetch_one(&mut *tx)
            .await?;

            // The credit half. `user_point_balances` may have no row yet for an
            // account that has never earned anything, so this upserts rather
            // than updating, and takes the new balance back for the ledger row.
            let recipient_balance = sqlx::query_scalar!(
                r#"
                INSERT INTO user_point_balances (user_id, balance, updated_at)
                VALUES ($1, $2, CURRENT_TIMESTAMP)
                ON CONFLICT (user_id)
                DO UPDATE SET balance = user_point_balances.balance + $2,
                              updated_at = CURRENT_TIMESTAMP
                RETURNING balance
                "#,
                &request.recipient_id as &UserId,
                total,
            )
            .fetch_one(&mut *tx)
            .await?;

            let recipient_transaction_id = sqlx::query_scalar!(
                r#"
                INSERT INTO user_point_transactions
                    (user_id, delta, balance_after, reason, description)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                "#,
                &request.recipient_id as &UserId,
                total,
                recipient_balance,
                POINT_REASON_GIFT_RECEIVE,
                gift.name.as_str(),
            )
            .fetch_one(&mut *tx)
            .await?;

            (
                balance,
                Some(transaction_id),
                Some(recipient_transaction_id),
            )
        } else {
            (
                Self::read_gift_balance(&mut *tx, &request.sender_id).await?,
                None,
                None,
            )
        };
        let inserted = sqlx::query!(
            r#"
            INSERT INTO gift_records
                (sender_id, recipient_id, room_id, gift_id, gift_key, gift_name,
                 gift_icon, unit_price_points, quantity, total_points,
                 transaction_id, message, request_id, recipient_transaction_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING id, created_at
            "#,
            &request.sender_id as &UserId,
            &request.recipient_id as &UserId,
            request.room_id.as_ref() as Option<&RoomId>,
            request.gift_id,
            gift.gift_key.as_str(),
            gift.name.as_str(),
            gift.icon.as_str(),
            gift.price_points,
            request.quantity,
            total,
            transaction_id,
            request.message.as_str(),
            request.request_id.as_deref(),
            recipient_transaction_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(SendGiftResult {
            record: GiftRecord {
                id: inserted.id,
                sender_id: request.sender_id,
                recipient_id: Some(request.recipient_id),
                room_id: request.room_id,
                gift_id: request.gift_id,
                gift_key: gift.gift_key,
                gift_name: gift.name,
                gift_icon: gift.icon,
                unit_price_points: gift.price_points,
                quantity: request.quantity,
                total_points: total,
                transaction_id,
                message: request.message.clone(),
                created_at: inserted.created_at,
            },
            balance,
            already_sent: false,
        })
    }
    /// One side of an account's gift history, newest first.
    ///
    /// Both directions share a query because they differ only in which column
    /// the account is matched on, and both need the same joins.
    pub async fn list_gift_records(
        &self,
        user_id: &UserId,
        direction: GiftRecordDirection,
        pagination: PageParams,
    ) -> Result<(Vec<GiftRecordView>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let received = direction == GiftRecordDirection::Received;

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM gift_records
            WHERE CASE WHEN $2::bool THEN recipient_id = $1 ELSE sender_id = $1 END
            "#,
            user_id as &UserId,
            received,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query_as!(
            GiftRecordRow,
            r#"
            SELECT g.id,
                   g.sender_id AS "sender_id: UserId",
                   g.recipient_id AS "recipient_id?: UserId",
                   g.room_id AS "room_id?: RoomId",
                   g.gift_id,
                   g.gift_key,
                   g.gift_name,
                   g.gift_icon,
                   g.unit_price_points,
                   g.quantity,
                   g.total_points,
                   g.transaction_id,
                   g.message,
                   g.created_at,
                   sender.username AS sender_username,
                   recipient.username AS "recipient_username?"
            FROM gift_records g
            JOIN users sender ON sender.id = g.sender_id
            LEFT JOIN users recipient ON recipient.id = g.recipient_id
            WHERE CASE WHEN $2::bool THEN g.recipient_id = $1 ELSE g.sender_id = $1 END
            ORDER BY g.created_at DESC, g.id DESC
            LIMIT $3 OFFSET $4
            "#,
            user_id as &UserId,
            received,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let records = rows.into_iter().map(GiftRecordView::from).collect();

        Ok((records, total))
    }
    /// The room's own recent sends, so a member who arrives after a gift was
    /// broadcast still sees it.
    pub async fn list_room_gift_records(
        &self,
        room_id: &RoomId,
        limit: i64,
    ) -> Result<Vec<GiftRecordView>> {
        let rows = sqlx::query_as!(
            GiftRecordRow,
            r#"
            SELECT g.id,
                   g.sender_id AS "sender_id: UserId",
                   g.recipient_id AS "recipient_id?: UserId",
                   g.room_id AS "room_id?: RoomId",
                   g.gift_id,
                   g.gift_key,
                   g.gift_name,
                   g.gift_icon,
                   g.unit_price_points,
                   g.quantity,
                   g.total_points,
                   g.transaction_id,
                   g.message,
                   g.created_at,
                   sender.username AS sender_username,
                   recipient.username AS "recipient_username?"
            FROM gift_records g
            JOIN users sender ON sender.id = g.sender_id
            LEFT JOIN users recipient ON recipient.id = g.recipient_id
            WHERE g.room_id = $1
            ORDER BY g.created_at DESC, g.id DESC
            LIMIT $2
            "#,
            room_id as &RoomId,
            limit,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(GiftRecordView::from).collect())
    }
    /// Lifetime counters for a profile. One round trip, zero rows for an account
    /// that has never sent or received anything.
    pub async fn gift_stats(&self, user_id: &UserId) -> Result<GiftStats> {
        let row = sqlx::query!(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN sender_id = $1 THEN quantity END), 0)::bigint
                    AS "sent_count!",
                COALESCE(SUM(CASE WHEN sender_id = $1 THEN total_points END), 0)::bigint
                    AS "sent_points!",
                COALESCE(SUM(CASE WHEN recipient_id = $1 THEN quantity END), 0)::bigint
                    AS "received_count!",
                COALESCE(SUM(CASE WHEN recipient_id = $1 THEN total_points END), 0)::bigint
                    AS "received_points!"
            FROM gift_records
            WHERE sender_id = $1 OR recipient_id = $1
            "#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(GiftStats {
            sent_count: row.sent_count,
            sent_points: row.sent_points,
            received_count: row.received_count,
            received_points: row.received_points,
        })
    }

    /// The sender's balance, zero for an account that has never earned and so
    /// has no row. Only used on paths that do not debit.
    /// The spendable balance, for a send panel that greys out what the caller
    /// cannot afford. Zero for an account that has never earned a point, which
    /// is indistinguishable from having spent them all — and equally true.
    pub async fn point_balance(&self, user_id: &UserId) -> Result<i64> {
        Self::read_gift_balance(self.pool(), user_id).await
    }

    async fn read_gift_balance(
        executor: impl sqlx::PgExecutor<'_>,
        user_id: &UserId,
    ) -> Result<i64> {
        let balance = sqlx::query_scalar!(
            r#"SELECT balance FROM user_point_balances WHERE user_id = $1"#,
            user_id as &UserId,
        )
        .fetch_optional(executor)
        .await?
        .unwrap_or(0);

        Ok(balance)
    }
}

/// One kind of gift on a profile's shelf, with the senders aggregated away.
#[derive(Debug, Clone)]
pub struct GiftShowcaseEntry {
    pub gift_key: String,
    pub gift_name: String,
    pub gift_icon: String,
    pub quantity: i64,
    pub total_points: i64,
    pub last_received_at: DateTime<Utc>,
}

/// The totals under the shelf, over every gift received rather than only the
/// kinds that fit on the page.
#[derive(Debug, Clone, Default)]
pub struct GiftShowcase {
    pub entries: Vec<GiftShowcaseEntry>,
    pub total_quantity: i64,
    pub total_points: i64,
}

/// One row of the admin ledger: a movement, and whose balance it moved.
#[derive(Debug, Clone)]
pub struct PointLedgerEntry {
    pub id: i64,
    pub user_id: UserId,
    pub username: String,
    pub delta: i64,
    pub balance_after: i64,
    pub reason: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

impl UserRepository {
    /// What `user_id` has been given, grouped by gift.
    ///
    /// The grouping is the privacy boundary, not a display choice: the result
    /// has nowhere to put a sender, so no caller of this can leak who paid.
    /// A profile shows what it was given, never by whom.
    pub async fn gift_showcase(&self, user_id: &UserId, limit: i64) -> Result<GiftShowcase> {
        let rows = sqlx::query!(
            r#"
            SELECT gift_key,
                   MAX(gift_name) AS "gift_name!",
                   MAX(gift_icon) AS "gift_icon!",
                   SUM(quantity)::BIGINT AS "quantity!",
                   SUM(total_points)::BIGINT AS "total_points!",
                   MAX(created_at) AS "last_received_at!"
            FROM gift_records
            WHERE recipient_id = $1
            GROUP BY gift_key
            ORDER BY SUM(quantity) DESC, MAX(created_at) DESC
            LIMIT $2
            "#,
            user_id as &UserId,
            limit,
        )
        .fetch_all(self.pool())
        .await?;

        // Totals come from their own scan rather than the rows above: `limit`
        // cuts the list, and a shelf that says "3 gifts" under four of them
        // would be reporting the page instead of the account.
        let totals = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(quantity), 0)::BIGINT AS "quantity!",
                   COALESCE(SUM(total_points), 0)::BIGINT AS "total_points!"
            FROM gift_records
            WHERE recipient_id = $1
            "#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(GiftShowcase {
            entries: rows
                .into_iter()
                .map(|row| GiftShowcaseEntry {
                    gift_key: row.gift_key,
                    gift_name: row.gift_name,
                    gift_icon: row.gift_icon,
                    quantity: row.quantity,
                    total_points: row.total_points,
                    last_received_at: row.last_received_at,
                })
                .collect(),
            total_quantity: totals.quantity,
            total_points: totals.total_points,
        })
    }

    /// Every gift on the server, newest first, for the admin console.
    ///
    /// `search` matches either username, because an admin looking into a
    /// complaint has one name and does not yet know which side it was on.
    pub async fn list_all_gift_records(
        &self,
        search: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<GiftRecordView>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let pattern = search.and_then(crate::repository::query_builder::ilike_contains_pattern);

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM gift_records g
            JOIN users sender ON sender.id = g.sender_id
            LEFT JOIN users recipient ON recipient.id = g.recipient_id
            WHERE $1::text IS NULL
               OR sender.username ILIKE $1
               OR recipient.username ILIKE $1
            "#,
            pattern.as_deref(),
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query_as!(
            GiftRecordRow,
            r#"
            SELECT g.id,
                   g.sender_id AS "sender_id: UserId",
                   g.recipient_id AS "recipient_id?: UserId",
                   g.room_id AS "room_id?: RoomId",
                   g.gift_id,
                   g.gift_key,
                   g.gift_name,
                   g.gift_icon,
                   g.unit_price_points,
                   g.quantity,
                   g.total_points,
                   g.transaction_id,
                   g.message,
                   g.created_at,
                   sender.username AS sender_username,
                   recipient.username AS "recipient_username?"
            FROM gift_records g
            JOIN users sender ON sender.id = g.sender_id
            LEFT JOIN users recipient ON recipient.id = g.recipient_id
            WHERE $1::text IS NULL
               OR sender.username ILIKE $1
               OR recipient.username ILIKE $1
            ORDER BY g.created_at DESC, g.id DESC
            LIMIT $2 OFFSET $3
            "#,
            pattern.as_deref(),
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((rows.into_iter().map(GiftRecordView::from).collect(), total))
    }

    /// The whole points ledger, newest first, optionally narrowed to one
    /// account or one cause.
    pub async fn list_all_point_transactions(
        &self,
        user_id: Option<&UserId>,
        reason: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<PointLedgerEntry>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let reason = reason.filter(|value| !value.is_empty());

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM user_point_transactions t
            WHERE ($1::bigint IS NULL OR t.user_id = $1)
              AND ($2::text IS NULL OR t.reason = $2)
            "#,
            user_id.map(UserId::as_i64),
            reason,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query!(
            r#"
            SELECT t.id,
                   t.user_id AS "user_id: UserId",
                   u.username,
                   t.delta,
                   t.balance_after,
                   t.reason,
                   t.description,
                   t.created_at
            FROM user_point_transactions t
            JOIN users u ON u.id = t.user_id
            WHERE ($1::bigint IS NULL OR t.user_id = $1)
              AND ($2::text IS NULL OR t.reason = $2)
            ORDER BY t.created_at DESC, t.id DESC
            LIMIT $3 OFFSET $4
            "#,
            user_id.map(UserId::as_i64),
            reason,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        Ok((
            rows.into_iter()
                .map(|row| PointLedgerEntry {
                    id: row.id,
                    user_id: row.user_id,
                    username: row.username,
                    delta: row.delta,
                    balance_after: row.balance_after,
                    reason: row.reason,
                    description: row.description,
                    created_at: row.created_at,
                })
                .collect(),
            total,
        ))
    }
}
