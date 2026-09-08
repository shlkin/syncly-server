//! 礼物: the catalog admins curate, and the sends that spend check-in points.
//!
//! Sending burns the sender's points rather than transferring them: crediting
//! the recipient would let two accounts pass the same points back and forth for
//! free, and would inflate `total_earned`, which is meant to record earnings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{RoomId, UserId};

/// Ledger reason recorded for points a gift send spends.
pub const POINT_REASON_GIFT_SEND: &str = "gift_send";
/// The credit half of a send. A gift transfers points rather than burning them,
/// so every `gift_send` debit has a matching `gift_receive` credit on the other
/// account and the two rows net to zero across the ledger.
pub const POINT_REASON_GIFT_RECEIVE: &str = "gift_receive";

/// Largest price an admin may configure, matching the column's CHECK.
pub const GIFT_PRICE_MAX: i32 = 1_000_000;
/// Most copies one send may carry, matching the column's CHECK.
pub const GIFT_QUANTITY_MAX: i32 = 99;
/// Longest accepted catalog key.
pub const GIFT_KEY_MAX_CHARS: usize = 40;
/// Longest accepted display name.
pub const GIFT_NAME_MAX_CHARS: usize = 40;
/// Longest accepted catalog blurb.
pub const GIFT_DESCRIPTION_MAX_CHARS: usize = 200;
/// Longest accepted icon glyph or URL.
pub const GIFT_ICON_MAX_CHARS: usize = 300;
/// Longest accepted note attached to a send.
pub const GIFT_MESSAGE_MAX_CHARS: usize = 200;
/// Longest accepted idempotency key.
pub const GIFT_REQUEST_ID_MAX_CHARS: usize = 64;

/// One catalog entry.
#[derive(Debug, Clone)]
pub struct Gift {
    pub id: i64,
    /// Stable machine key, so a client can special-case artwork without keying
    /// off a row id or a display name that admins may rename.
    pub key: String,
    pub name: String,
    pub description: String,
    /// Either a short glyph (usually an emoji) or an asset URL. The client
    /// decides which by inspecting it.
    pub icon: String,
    pub price_points: i32,
    pub sort_order: i32,
    /// Disabled entries stay readable for history, but cannot be sent.
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
}

/// A new catalog entry.
#[derive(Debug, Clone)]
pub struct NewGift {
    pub key: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub price_points: i32,
    pub sort_order: i32,
    pub is_enabled: bool,
}

/// Partial update of a catalog entry; `None` leaves a field as stored.
///
/// The key is deliberately absent: records snapshot it, and clients may key
/// artwork off it, so a rename would silently orphan both.
#[derive(Debug, Clone, Default)]
pub struct GiftUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub price_points: Option<i32>,
    pub sort_order: Option<i32>,
    pub is_enabled: Option<bool>,
}

impl GiftUpdate {
    /// True when the update would change nothing, so the caller can skip a write.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.icon.is_none()
            && self.price_points.is_none()
            && self.sort_order.is_none()
            && self.is_enabled.is_none()
    }
}

/// What a caller asks for when sending.
#[derive(Debug, Clone)]
pub struct SendGift {
    pub sender_id: UserId,
    pub recipient_id: UserId,
    /// `None` when sent from a profile rather than from a room, which is the
    /// only case with no audience to broadcast to.
    pub room_id: Option<RoomId>,
    pub gift_id: i64,
    pub quantity: i32,
    pub message: String,
    /// Idempotency key. A retried send (flaky network, double tap) must charge
    /// once; a repeat returns the first record untouched.
    pub request_id: Option<String>,
}

/// One send, with the catalog fields as they were at the time.
#[derive(Debug, Clone)]
pub struct GiftRecord {
    pub id: i64,
    pub sender_id: UserId,
    /// `None` once the recipient deletes their account.
    pub recipient_id: Option<UserId>,
    pub room_id: Option<RoomId>,
    pub gift_id: i64,
    pub gift_key: String,
    pub gift_name: String,
    pub gift_icon: String,
    pub unit_price_points: i32,
    pub quantity: i32,
    pub total_points: i64,
    /// The ledger debit this send paid through, so a statement row and a gift
    /// row can be presented as one event.
    pub transaction_id: Option<i64>,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

/// A send plus the counterpart's display name, which every listing needs and
/// no listing can derive from the record alone.
#[derive(Debug, Clone)]
pub struct GiftRecordView {
    pub record: GiftRecord,
    pub sender_username: String,
    /// `None` once the recipient's account is gone.
    pub recipient_username: Option<String>,
}

/// Outcome of a send.
#[derive(Debug, Clone)]
pub struct SendGiftResult {
    pub record: GiftRecord,
    /// The sender's balance after the debit.
    pub balance: i64,
    /// True when `request_id` matched an earlier send. The result then describes
    /// that send and nothing was charged, so a retry is not a failure.
    pub already_sent: bool,
}

/// Aggregate counters rendered on a profile.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct GiftStats {
    pub sent_count: i64,
    pub sent_points: i64,
    pub received_count: i64,
    /// Points the sends were worth, not points the account holds.
    pub received_points: i64,
}

/// Which side of a send a listing is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GiftRecordDirection {
    #[default]
    Sent,
    Received,
}

impl GiftRecordDirection {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Received => "received",
        }
    }
}

impl std::str::FromStr for GiftRecordDirection {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "sent" => Ok(Self::Sent),
            "received" => Ok(Self::Received),
            other => Err(format!("invalid gift record direction: {other}")),
        }
    }
}

impl std::fmt::Display for GiftRecordDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
