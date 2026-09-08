//! Friendship, and the direct and group conversations behind the "消息" tab.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{User, UserId};

/// Longest accepted note attached to a friend request.
pub const FRIEND_REQUEST_MESSAGE_MAX_CHARS: usize = 200;
/// Longest accepted group title.
pub const CONVERSATION_TITLE_MAX_CHARS: usize = 100;
/// Longest accepted message body.
pub const SOCIAL_MESSAGE_BODY_MAX_CHARS: usize = 4000;
/// Ceiling on group size, counting the owner.
pub const CONVERSATION_MEMBER_MAX: usize = 200;

/// Lifecycle of a friend request. Only `Pending` is live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendRequestStatus {
    Pending,
    Accepted,
    Declined,
    /// Withdrawn by the requester before the other side answered.
    Cancelled,
}

impl FriendRequestStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::str::FromStr for FriendRequestStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "accepted" => Ok(Self::Accepted),
            "declined" => Ok(Self::Declined),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("invalid friend request status: {other}")),
        }
    }
}

impl std::fmt::Display for FriendRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which side of a request the reader is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendRequestDirection {
    /// Someone asked the reader.
    Incoming,
    /// The reader asked someone.
    Outgoing,
}

/// A friend request as one side sees it: the counterpart plus the direction,
/// rather than a requester/addressee pair the caller would have to interpret.
#[derive(Debug, Clone)]
pub struct FriendRequest {
    pub id: i64,
    pub counterpart: User,
    pub direction: FriendRequestDirection,
    pub message: String,
    pub status: FriendRequestStatus,
    pub created_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
}

/// An accepted friendship, from the reader's side.
#[derive(Debug, Clone)]
pub struct Friend {
    pub user: User,
    pub friended_at: DateTime<Utc>,
}

/// What accepting a request produced. The conversation is opened eagerly so the
/// client can navigate straight into a thread.
#[derive(Debug, Clone)]
pub struct AcceptedFriendRequest {
    pub friend: Friend,
    pub conversation_id: i64,
}

/// What sending a friend request produced.
#[derive(Debug, Clone)]
pub enum FriendRequestOutcome {
    /// Waiting for the other side to answer.
    Pending { request_id: i64 },
    /// The other side had already asked. Two people asking each other is mutual
    /// consent, so the request is settled immediately rather than left in an
    /// inbox that would be confusing to both.
    Accepted(AcceptedFriendRequest),
}

/// The numbers the 消息 tab badges itself with.
#[derive(Debug, Clone, Copy)]
pub struct SocialUnreadSummary {
    pub unread_message_count: i64,
    /// Threads with at least one unread message, for a "N 个会话" line.
    pub unread_conversation_count: i64,
    pub pending_friend_request_count: i64,
}

/// Direct threads and groups share one table; this is the discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKind {
    /// Exactly two members, created on demand between friends.
    Direct,
    /// Owner plus invited members, with a title.
    Group,
}

impl ConversationKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
        }
    }
}

impl std::str::FromStr for ConversationKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "direct" => Ok(Self::Direct),
            "group" => Ok(Self::Group),
            other => Err(format!("invalid conversation kind: {other}")),
        }
    }
}

impl std::fmt::Display for ConversationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a member may do in a group. Direct threads store `Member` for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMemberRole {
    Member,
    /// May add and remove members; may not delete the group.
    Admin,
    Owner,
}

impl ConversationMemberRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }

    /// True for the roles allowed to change the member list and the title.
    #[must_use]
    pub const fn can_manage(self) -> bool {
        matches!(self, Self::Admin | Self::Owner)
    }
}

impl std::str::FromStr for ConversationMemberRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "member" => Ok(Self::Member),
            "admin" => Ok(Self::Admin),
            "owner" => Ok(Self::Owner),
            other => Err(format!("invalid conversation member role: {other}")),
        }
    }
}

impl std::fmt::Display for ConversationMemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Text written by a member, or a system line recording a membership change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SocialMessageKind {
    Text,
    System,
}

impl SocialMessageKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::System => "system",
        }
    }
}

impl std::str::FromStr for SocialMessageKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "system" => Ok(Self::System),
            other => Err(format!("invalid social message kind: {other}")),
        }
    }
}

impl std::fmt::Display for SocialMessageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One message in a thread.
#[derive(Debug, Clone)]
pub struct SocialMessage {
    pub id: i64,
    pub conversation_id: i64,
    /// `None` for system lines, and for text whose author has been deleted.
    pub sender: Option<User>,
    pub kind: SocialMessageKind,
    /// Empty when `deleted` is true: a removed message keeps its place in the id
    /// sequence because every member's read cursor is an id in it.
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub deleted: bool,
}

/// A conversation with everything the list row needs, read as one unit.
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: i64,
    pub kind: ConversationKind,
    /// Empty for direct threads, which render as `counterpart`.
    pub title: String,
    pub owner_user_id: Option<UserId>,
    /// The other participant of a direct thread; `None` for a group.
    pub counterpart: Option<User>,
    pub member_count: i64,
    pub viewer_role: ConversationMemberRole,
    pub muted: bool,
    pub unread_count: i64,
    pub last_message: Option<SocialMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A member of a group, for the member sheet.
#[derive(Debug, Clone)]
pub struct ConversationMember {
    pub user: User,
    pub role: ConversationMemberRole,
    pub joined_at: DateTime<Utc>,
}

/// Fields accepted when creating a group.
#[derive(Debug, Clone)]
pub struct NewGroupConversation {
    pub title: String,
    /// Accounts invited at creation time, excluding the creator.
    pub member_user_ids: Vec<UserId>,
}

/// One conversation as an administrator sees it in the moderation listing.
///
/// Deliberately not a [`ConversationSummary`]: that shape answers "how does
/// this member see the thread", and an administrator auditing a group is not a
/// member of it. There is no read cursor to report and no mute state to honour,
/// so what is left is the shape of the thread itself.
#[derive(Debug, Clone)]
pub struct AdminConversationOverview {
    pub id: i64,
    pub kind: ConversationKind,
    /// Empty for direct threads, which have no title of their own.
    pub title: String,
    /// The group's creator, absent for a direct thread or a deleted account.
    pub owner: Option<User>,
    /// Every member, including the owner. Counted over live accounts only, so
    /// it matches what the member listing returns.
    pub member_count: i64,
    /// Messages still standing; a soft-deleted message is not counted.
    pub message_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
