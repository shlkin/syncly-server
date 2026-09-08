//! 礼物: catalog curation, and sending one by spending points.
//!
//! Validation lives here rather than in the repository so a bad field comes back
//! as a field error instead of a database CHECK violation. The one rule that is
//! not a length or a range: a send is refused when either side has blocked the
//! other, because a gift is a social interaction and would otherwise be a way
//! around a block.

use crate::{
    models::{
        Gift, GiftRecordDirection, GiftRecordView, GiftStats, GiftUpdate, NewGift, PageParams,
        RoomId, SendGift, SendGiftResult, UserId, GIFT_DESCRIPTION_MAX_CHARS, GIFT_ICON_MAX_CHARS,
        GIFT_KEY_MAX_CHARS, GIFT_MESSAGE_MAX_CHARS, GIFT_NAME_MAX_CHARS, GIFT_PRICE_MAX,
        GIFT_QUANTITY_MAX, GIFT_REQUEST_ID_MAX_CHARS,
    },
    repository::{GiftShowcase, PointLedgerEntry},
    Error, Result,
};

use super::UserService;

/// Most room sends one listing returns. The room panel shows a short feed, not a
/// history, and the cap keeps an unbounded client value out of the query.
const ROOM_GIFT_FEED_MAX: i64 = 50;

impl UserService {
    /// The catalog. `enabled_only` is what a send panel asks for; an admin screen
    /// passes false so a disabled entry stays editable.
    pub async fn list_gifts(&self, enabled_only: bool) -> Result<Vec<Gift>> {
        self.repository.list_gifts(enabled_only).await
    }

    /// Adds a catalog entry. Admin-only; the caller decides who may reach this.
    pub async fn create_gift(&self, gift: &NewGift, actor_user_id: &UserId) -> Result<Gift> {
        let key = gift.key.trim().to_ascii_lowercase();
        if !is_valid_gift_key(&key) {
            return Err(Error::InvalidInput(format!(
                "gift key must be 2 to {GIFT_KEY_MAX_CHARS} characters of a-z, 0-9 or underscore"
            )));
        }

        let name = gift.name.trim().to_string();
        if name.is_empty() || name.chars().count() > GIFT_NAME_MAX_CHARS {
            return Err(Error::InvalidInput(format!(
                "gift name must be 1 to {GIFT_NAME_MAX_CHARS} characters"
            )));
        }

        let description = trimmed_within(
            &gift.description,
            GIFT_DESCRIPTION_MAX_CHARS,
            "gift description",
        )?;
        let icon = trimmed_within(&gift.icon, GIFT_ICON_MAX_CHARS, "gift icon")?;
        validate_price(gift.price_points)?;

        self.repository
            .create_gift(
                &NewGift {
                    key,
                    name,
                    description,
                    icon,
                    price_points: gift.price_points,
                    sort_order: gift.sort_order,
                    is_enabled: gift.is_enabled,
                },
                actor_user_id,
            )
            .await
    }

    /// Applies an admin's partial update. An empty update reads the entry back
    /// rather than writing, so a no-op request does not bump `updated_at`.
    pub async fn update_gift(
        &self,
        gift_id: i64,
        update: &GiftUpdate,
        actor_user_id: &UserId,
    ) -> Result<Gift> {
        if update.is_empty() {
            return self
                .repository
                .find_gift(gift_id)
                .await?
                .ok_or_else(|| Error::NotFound("gift not found".to_string()));
        }

        let name = match update.name.as_deref() {
            Some(raw) => {
                let name = raw.trim().to_string();
                if name.is_empty() || name.chars().count() > GIFT_NAME_MAX_CHARS {
                    return Err(Error::InvalidInput(format!(
                        "gift name must be 1 to {GIFT_NAME_MAX_CHARS} characters"
                    )));
                }
                Some(name)
            }
            None => None,
        };

        let description = match update.description.as_deref() {
            Some(raw) => Some(trimmed_within(
                raw,
                GIFT_DESCRIPTION_MAX_CHARS,
                "gift description",
            )?),
            None => None,
        };

        let icon = match update.icon.as_deref() {
            Some(raw) => Some(trimmed_within(raw, GIFT_ICON_MAX_CHARS, "gift icon")?),
            None => None,
        };

        if let Some(price) = update.price_points {
            validate_price(price)?;
        }

        self.repository
            .update_gift(
                gift_id,
                &GiftUpdate {
                    name,
                    description,
                    icon,
                    price_points: update.price_points,
                    sort_order: update.sort_order,
                    is_enabled: update.is_enabled,
                },
                actor_user_id,
            )
            .await
    }

    /// Removes a catalog entry that has never been sent. Admin-only.
    pub async fn delete_gift(&self, gift_id: i64) -> Result<()> {
        self.repository.delete_gift(gift_id).await
    }
    /// Sends a gift, spending the sender's points.
    ///
    /// A repeated `request_id` returns the earlier send untouched, so a client
    /// that retries a lost response converges instead of paying twice.
    pub async fn send_gift(&self, request: &SendGift) -> Result<SendGiftResult> {
        if request.sender_id == request.recipient_id {
            return Err(Error::InvalidInput(
                "a gift cannot be sent to yourself".to_string(),
            ));
        }

        if !(1..=GIFT_QUANTITY_MAX).contains(&request.quantity) {
            return Err(Error::InvalidInput(format!(
                "gift quantity must be between 1 and {GIFT_QUANTITY_MAX}"
            )));
        }

        let message = trimmed_within(&request.message, GIFT_MESSAGE_MAX_CHARS, "gift message")?;

        let request_id = match request.request_id.as_deref() {
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else if trimmed.chars().count() > GIFT_REQUEST_ID_MAX_CHARS {
                    return Err(Error::InvalidInput(format!(
                        "gift request id must be at most {GIFT_REQUEST_ID_MAX_CHARS} characters"
                    )));
                } else {
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };

        // The recipient must exist before points move; the foreign key would
        // catch it, but as a NotFound with no indication of which side is wrong.
        if self
            .repository
            .get_by_id(&request.recipient_id)
            .await?
            .is_none()
        {
            return Err(Error::NotFound("recipient not found".to_string()));
        }

        // Either direction of a block stops a send: the recipient must not be
        // reachable by someone they blocked, and someone who blocked a user
        // should not be handing them gifts either.
        if self
            .repository
            .is_blocking(&request.recipient_id, &request.sender_id)
            .await?
            || self
                .repository
                .is_blocking(&request.sender_id, &request.recipient_id)
                .await?
        {
            return Err(Error::Authorization(
                "gifts cannot be sent between blocked accounts".to_string(),
            ));
        }

        self.repository
            .send_gift(&SendGift {
                sender_id: request.sender_id,
                recipient_id: request.recipient_id,
                room_id: request.room_id,
                gift_id: request.gift_id,
                quantity: request.quantity,
                message,
                request_id,
            })
            .await
    }

    /// One side of an account's gift history, newest first.
    pub async fn list_gift_records(
        &self,
        user_id: &UserId,
        direction: GiftRecordDirection,
        pagination: PageParams,
    ) -> Result<(Vec<GiftRecordView>, i64)> {
        self.repository
            .list_gift_records(user_id, direction, pagination)
            .await
    }

    /// The room's recent sends, so a member who joins after a gift still sees it.
    pub async fn list_room_gift_records(
        &self,
        room_id: &RoomId,
        limit: i64,
    ) -> Result<Vec<GiftRecordView>> {
        let limit = limit.clamp(1, ROOM_GIFT_FEED_MAX);
        self.repository.list_room_gift_records(room_id, limit).await
    }

    /// Lifetime gift counters for a profile.
    pub async fn gift_stats(&self, user_id: &UserId) -> Result<GiftStats> {
        self.repository.gift_stats(user_id).await
    }

    /// The caller's spendable points, for the send panel's affordability hints.
    pub async fn point_balance(&self, user_id: &UserId) -> Result<i64> {
        self.repository.point_balance(user_id).await
    }

    /// What an account has been given, grouped by gift and with the senders
    /// aggregated away. This is what a public profile shows.
    pub async fn gift_showcase(&self, user_id: &UserId, limit: i32) -> Result<GiftShowcase> {
        let limit = i64::from(limit.clamp(0, GIFT_SHOWCASE_MAX));
        let limit = if limit == 0 {
            i64::from(GIFT_SHOWCASE_DEFAULT)
        } else {
            limit
        };
        self.repository.gift_showcase(user_id, limit).await
    }

    /// Every gift on the server. Admin-only; the caller checks the role.
    pub async fn list_all_gift_records(
        &self,
        search: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<GiftRecordView>, i64)> {
        self.repository
            .list_all_gift_records(search, pagination)
            .await
    }

    /// The whole points ledger. Admin-only; the caller checks the role.
    pub async fn list_all_point_transactions(
        &self,
        user_id: Option<&UserId>,
        reason: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<PointLedgerEntry>, i64)> {
        self.repository
            .list_all_point_transactions(user_id, reason, pagination)
            .await
    }
}

/// How many kinds of gift a profile shelf shows when the client does not say.
const GIFT_SHOWCASE_DEFAULT: i32 = 12;
const GIFT_SHOWCASE_MAX: i32 = 100;

/// Keys are matched by clients and stored in records, so the accepted shape is
/// narrow: lowercase, digits and underscores only.
fn is_valid_gift_key(key: &str) -> bool {
    let length = key.chars().count();
    (2..=GIFT_KEY_MAX_CHARS).contains(&length)
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn validate_price(price_points: i32) -> Result<()> {
    if (0..=GIFT_PRICE_MAX).contains(&price_points) {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "gift price must be between 0 and {GIFT_PRICE_MAX}"
        )))
    }
}

/// Trims, then rejects on character count rather than bytes: the limits are the
/// column widths, and a Chinese name would otherwise hit them three times early.
fn trimmed_within(value: &str, max_chars: usize, field: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max_chars {
        return Err(Error::InvalidInput(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    Ok(trimmed.to_string())
}
