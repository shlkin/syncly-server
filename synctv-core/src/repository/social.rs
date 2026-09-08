//! Friendship, direct threads and groups.
//!
//! Listings here read ids and then hydrate accounts through
//! [`UserRepository::get_by_ids`] rather than joining the fourteen columns of
//! `user_account_profiles` into every query. The friendship edge itself is
//! stored once per pair in ascending id order, so `(a, b)` and `(b, a)` are the
//! same row and a half-friendship cannot be represented.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::user::UserRepository;
use crate::{
    models::{
        AcceptedFriendRequest, AdminConversationOverview, ConversationKind, ConversationMember,
        ConversationMemberRole, ConversationSummary, Friend, FriendRequest, FriendRequestDirection,
        FriendRequestStatus, PageParams, SocialMessage, SocialMessageKind, SocialUnreadSummary,
        User, UserId,
    },
    Error, Result,
};

use super::query_builder::ilike_contains_pattern;

/// The pair as `friendships` stores it: lower id first.
fn canonical_pair(first: &UserId, second: &UserId) -> (i64, i64) {
    let (a, b) = (first.as_i64(), second.as_i64());
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Key that maps a pair of accounts to their single direct conversation.
fn direct_conversation_key(first: &UserId, second: &UserId) -> String {
    let (low, high) = canonical_pair(first, second);
    format!("{low}:{high}")
}

fn parse_enum<T>(raw: &str, label: &str) -> Result<T>
where
    T: std::str::FromStr<Err = String>,
{
    raw.parse::<T>()
        .map_err(|error| Error::Internal(format!("stored {label} is invalid: {error}")))
}

/// Accounts indexed by id, for stitching hydrated users back onto rows.
async fn user_index(repository: &UserRepository, ids: &[UserId]) -> Result<HashMap<i64, User>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut unique: Vec<UserId> = ids.to_vec();
    unique.sort_unstable_by_key(UserId::as_i64);
    unique.dedup_by_key(|id| id.as_i64());

    Ok(repository
        .get_by_ids(&unique)
        .await?
        .into_iter()
        .map(|user| (user.id.as_i64(), user))
        .collect())
}

/// A friend request as stored, before the counterpart is hydrated.
struct FriendRequestRow {
    id: i64,
    requester_user_id: UserId,
    addressee_user_id: UserId,
    message: String,
    status: String,
    created_at: DateTime<Utc>,
    responded_at: Option<DateTime<Utc>>,
}

impl UserRepository {
    /// Opens a friend request, returning its id.
    ///
    /// `None` means one was already pending in this direction: the partial unique
    /// index rejects the insert, which is the answer the caller needs rather than
    /// an error to translate.
    pub async fn create_friend_request(
        &self,
        requester_user_id: &UserId,
        addressee_user_id: &UserId,
        message: &str,
    ) -> Result<Option<i64>> {
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO friend_requests (requester_user_id, addressee_user_id, message)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
            requester_user_id as &UserId,
            addressee_user_id as &UserId,
            message,
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(id)
    }

    /// Id of the open request in this direction, if there is one.
    pub async fn pending_friend_request_id(
        &self,
        requester_user_id: &UserId,
        addressee_user_id: &UserId,
    ) -> Result<Option<i64>> {
        let id = sqlx::query_scalar!(
            r#"
            SELECT id FROM friend_requests
            WHERE requester_user_id = $1 AND addressee_user_id = $2 AND status = 'pending'
            "#,
            requester_user_id as &UserId,
            addressee_user_id as &UserId,
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(id)
    }

    /// Accepts a request the caller received, in one transaction: the request
    /// becomes terminal, the friendship edge appears, any mirror request from the
    /// other side is settled too, and the direct thread is opened so the client
    /// can navigate straight into it.
    ///
    /// `None` means the id named no request that this account could accept —
    /// wrong recipient, already answered, or gone.
    pub async fn accept_friend_request(
        &self,
        request_id: i64,
        addressee_user_id: &UserId,
    ) -> Result<Option<AcceptedFriendRequest>> {
        let mut tx = self.pool().begin().await?;

        let Some(requester_user_id) = sqlx::query_scalar!(
            r#"
            UPDATE friend_requests
            SET status = 'accepted', responded_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND addressee_user_id = $2 AND status = 'pending'
            RETURNING requester_user_id AS "requester_user_id!: UserId"
            "#,
            request_id,
            addressee_user_id as &UserId,
        )
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };

        // Both sides may have asked before either answered. Settling the mirror
        // request keeps it out of the other account's inbox now that they are
        // already friends.
        sqlx::query!(
            r#"
            UPDATE friend_requests
            SET status = 'accepted', responded_at = CURRENT_TIMESTAMP
            WHERE requester_user_id = $1 AND addressee_user_id = $2 AND status = 'pending'
            "#,
            addressee_user_id as &UserId,
            &requester_user_id as &UserId,
        )
        .execute(&mut *tx)
        .await?;

        let (low, high) = canonical_pair(addressee_user_id, &requester_user_id);
        let friended_at = sqlx::query_scalar!(
            r#"
            INSERT INTO friendships (low_user_id, high_user_id)
            VALUES ($1, $2)
            ON CONFLICT (low_user_id, high_user_id)
            DO UPDATE SET low_user_id = EXCLUDED.low_user_id
            RETURNING created_at AS "created_at!"
            "#,
            low,
            high,
        )
        .fetch_one(&mut *tx)
        .await?;

        let conversation_id =
            Self::ensure_direct_conversation_tx(&mut tx, addressee_user_id, &requester_user_id)
                .await?;

        tx.commit().await?;

        let requester = self
            .get_by_id(&requester_user_id)
            .await?
            .ok_or_else(|| Error::NotFound("User not found".to_string()))?;

        Ok(Some(AcceptedFriendRequest {
            friend: Friend {
                user: requester,
                friended_at,
            },
            conversation_id,
        }))
    }

    /// Declines a request the caller received. False when the id named no request
    /// this account could decline.
    pub async fn decline_friend_request(
        &self,
        request_id: i64,
        addressee_user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE friend_requests
            SET status = 'declined', responded_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND addressee_user_id = $2 AND status = 'pending'
            "#,
            request_id,
            addressee_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Withdraws a request the caller sent.
    pub async fn cancel_friend_request(
        &self,
        request_id: i64,
        requester_user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE friend_requests
            SET status = 'cancelled', responded_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND requester_user_id = $2 AND status = 'pending'
            "#,
            request_id,
            requester_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Open requests in one direction, newest first. Settled requests are not
    /// listed: they are kept for the record, not for the inbox.
    pub async fn list_friend_requests(
        &self,
        user_id: &UserId,
        direction: FriendRequestDirection,
        pagination: PageParams,
    ) -> Result<(Vec<FriendRequest>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let (rows, total) = match direction {
            FriendRequestDirection::Incoming => {
                let total = sqlx::query_scalar!(
                    r#"
                    SELECT COUNT(*) AS "total!"
                    FROM friend_requests r
                    JOIN user_account_profiles p ON p.id = r.requester_user_id
                    WHERE r.addressee_user_id = $1 AND r.status = 'pending'
                      AND p.deleted_at IS NULL
                    "#,
                    user_id as &UserId,
                )
                .fetch_one(self.pool())
                .await?;

                let rows = sqlx::query_as!(
                    FriendRequestRow,
                    r#"
                    SELECT r.id,
                           r.requester_user_id AS "requester_user_id!: UserId",
                           r.addressee_user_id AS "addressee_user_id!: UserId",
                           r.message,
                           r.status,
                           r.created_at,
                           r.responded_at
                    FROM friend_requests r
                    JOIN user_account_profiles p ON p.id = r.requester_user_id
                    WHERE r.addressee_user_id = $1 AND r.status = 'pending'
                      AND p.deleted_at IS NULL
                    ORDER BY r.created_at DESC, r.id DESC
                    LIMIT $2 OFFSET $3
                    "#,
                    user_id as &UserId,
                    limit,
                    offset,
                )
                .fetch_all(self.pool())
                .await?;

                (rows, total)
            }
            FriendRequestDirection::Outgoing => {
                let total = sqlx::query_scalar!(
                    r#"
                    SELECT COUNT(*) AS "total!"
                    FROM friend_requests r
                    JOIN user_account_profiles p ON p.id = r.addressee_user_id
                    WHERE r.requester_user_id = $1 AND r.status = 'pending'
                      AND p.deleted_at IS NULL
                    "#,
                    user_id as &UserId,
                )
                .fetch_one(self.pool())
                .await?;

                let rows = sqlx::query_as!(
                    FriendRequestRow,
                    r#"
                    SELECT r.id,
                           r.requester_user_id AS "requester_user_id!: UserId",
                           r.addressee_user_id AS "addressee_user_id!: UserId",
                           r.message,
                           r.status,
                           r.created_at,
                           r.responded_at
                    FROM friend_requests r
                    JOIN user_account_profiles p ON p.id = r.addressee_user_id
                    WHERE r.requester_user_id = $1 AND r.status = 'pending'
                      AND p.deleted_at IS NULL
                    ORDER BY r.created_at DESC, r.id DESC
                    LIMIT $2 OFFSET $3
                    "#,
                    user_id as &UserId,
                    limit,
                    offset,
                )
                .fetch_all(self.pool())
                .await?;

                (rows, total)
            }
        };

        let counterpart_ids: Vec<UserId> = rows
            .iter()
            .map(|row| match direction {
                FriendRequestDirection::Incoming => row.requester_user_id,
                FriendRequestDirection::Outgoing => row.addressee_user_id,
            })
            .collect();
        let users = user_index(self, &counterpart_ids).await?;

        let requests = rows
            .into_iter()
            .filter_map(|row| {
                let counterpart_id = match direction {
                    FriendRequestDirection::Incoming => row.requester_user_id,
                    FriendRequestDirection::Outgoing => row.addressee_user_id,
                };
                let counterpart = users.get(&counterpart_id.as_i64())?.clone();
                Some(FriendRequest {
                    id: row.id,
                    counterpart,
                    direction,
                    message: row.message,
                    status: parse_enum::<FriendRequestStatus>(&row.status, "friend request status")
                        .ok()?,
                    created_at: row.created_at,
                    responded_at: row.responded_at,
                })
            })
            .collect();

        Ok((requests, total))
    }

    /// The caller's friends, newest edge first, optionally filtered by username.
    pub async fn list_friends(
        &self,
        user_id: &UserId,
        pagination: PageParams,
        search: Option<&str>,
    ) -> Result<(Vec<Friend>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let pattern = search.and_then(ilike_contains_pattern);

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM friendships f
            JOIN user_account_profiles p
              ON p.id = CASE WHEN f.low_user_id = $1 THEN f.high_user_id ELSE f.low_user_id END
            WHERE (f.low_user_id = $1 OR f.high_user_id = $1)
              AND p.deleted_at IS NULL
              AND ($2::text IS NULL OR p.username ILIKE $2 ESCAPE '\')
            "#,
            user_id as &UserId,
            pattern.as_deref(),
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query!(
            r#"
            SELECT CASE WHEN f.low_user_id = $1 THEN f.high_user_id ELSE f.low_user_id END
                       AS "friend_user_id!: UserId",
                   f.created_at
            FROM friendships f
            JOIN user_account_profiles p
              ON p.id = CASE WHEN f.low_user_id = $1 THEN f.high_user_id ELSE f.low_user_id END
            WHERE (f.low_user_id = $1 OR f.high_user_id = $1)
              AND p.deleted_at IS NULL
              AND ($2::text IS NULL OR p.username ILIKE $2 ESCAPE '\')
            ORDER BY f.created_at DESC, p.id DESC
            LIMIT $3 OFFSET $4
            "#,
            user_id as &UserId,
            pattern.as_deref(),
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let ids: Vec<UserId> = rows.iter().map(|row| row.friend_user_id).collect();
        let users = user_index(self, &ids).await?;

        let friends = rows
            .into_iter()
            .filter_map(|row| {
                Some(Friend {
                    user: users.get(&row.friend_user_id.as_i64())?.clone(),
                    friended_at: row.created_at,
                })
            })
            .collect();

        Ok((friends, total))
    }

    pub async fn are_friends(&self, first: &UserId, second: &UserId) -> Result<bool> {
        let (low, high) = canonical_pair(first, second);
        let are_friends = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM friendships WHERE low_user_id = $1 AND high_user_id = $2
            ) AS "are_friends!"
            "#,
            low,
            high,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(are_friends)
    }

    /// Drops the friendship. The direct thread and its messages are left alone:
    /// unfriending should not silently destroy a conversation both sides wrote.
    pub async fn remove_friendship(&self, first: &UserId, second: &UserId) -> Result<bool> {
        let (low, high) = canonical_pair(first, second);
        let result = sqlx::query!(
            "DELETE FROM friendships WHERE low_user_id = $1 AND high_user_id = $2",
            low,
            high,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Opens the direct thread between two accounts, or returns the existing one.
    ///
    /// The `direct_key` unique constraint is what makes this idempotent: two
    /// clients opening the same thread at once cannot fork the history.
    pub async fn ensure_direct_conversation(&self, first: &UserId, second: &UserId) -> Result<i64> {
        let mut tx = self.pool().begin().await?;
        let conversation_id = Self::ensure_direct_conversation_tx(&mut tx, first, second).await?;
        tx.commit().await?;
        Ok(conversation_id)
    }

    async fn ensure_direct_conversation_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        first: &UserId,
        second: &UserId,
    ) -> Result<i64> {
        let direct_key = direct_conversation_key(first, second);

        let existing = sqlx::query_scalar!(
            r#"
            INSERT INTO social_conversations (kind, direct_key)
            VALUES ('direct', $1)
            ON CONFLICT (direct_key) DO NOTHING
            RETURNING id
            "#,
            direct_key,
        )
        .fetch_optional(&mut **tx)
        .await?;

        let conversation_id = match existing {
            Some(id) => id,
            None => {
                sqlx::query_scalar!(
                    "SELECT id FROM social_conversations WHERE direct_key = $1",
                    direct_key,
                )
                .fetch_one(&mut **tx)
                .await?
            }
        };

        sqlx::query!(
            r#"
            INSERT INTO social_conversation_members (conversation_id, user_id, role)
            VALUES ($1, $2, 'member'), ($1, $3, 'member')
            ON CONFLICT (conversation_id, user_id) DO NOTHING
            "#,
            conversation_id,
            first as &UserId,
            second as &UserId,
        )
        .execute(&mut **tx)
        .await?;

        Ok(conversation_id)
    }

    /// Creates a group owned by `owner_user_id` with `member_user_ids` joined.
    pub async fn create_group_conversation(
        &self,
        owner_user_id: &UserId,
        title: &str,
        member_user_ids: &[UserId],
    ) -> Result<i64> {
        let mut tx = self.pool().begin().await?;

        let conversation_id = sqlx::query_scalar!(
            r#"
            INSERT INTO social_conversations (kind, title, owner_user_id)
            VALUES ('group', $1, $2)
            RETURNING id
            "#,
            title,
            owner_user_id as &UserId,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO social_conversation_members (conversation_id, user_id, role)
            VALUES ($1, $2, 'owner')
            "#,
            conversation_id,
            owner_user_id as &UserId,
        )
        .execute(&mut *tx)
        .await?;

        // The owner may appear in the invite list; `DO NOTHING` keeps their
        // 'owner' row rather than demoting them to a member.
        let member_ids: Vec<i64> = member_user_ids.iter().map(UserId::as_i64).collect();
        sqlx::query!(
            r#"
            INSERT INTO social_conversation_members (conversation_id, user_id, role)
            SELECT $1, member_id, 'member' FROM UNNEST($2::bigint[]) AS member_id
            ON CONFLICT (conversation_id, user_id) DO NOTHING
            "#,
            conversation_id,
            &member_ids,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(conversation_id)
    }

    /// The caller's role in a conversation, or `None` when they are not in it.
    /// Every conversation-scoped operation is authorized through this.
    pub async fn conversation_member_role(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<Option<ConversationMemberRole>> {
        let role = sqlx::query_scalar!(
            r#"
            SELECT role FROM social_conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
            conversation_id,
            user_id as &UserId,
        )
        .fetch_optional(self.pool())
        .await?;

        role.map(|role| parse_enum::<ConversationMemberRole>(&role, "conversation member role"))
            .transpose()
    }

    /// The caller's conversations, most recently active first.
    ///
    /// One query carries everything a list row needs — member count, unread
    /// count, the newest message and, for a direct thread, the other member —
    /// because the alternative is four round trips per row.
    pub async fn list_conversations(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<ConversationSummary>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM social_conversation_members
            WHERE user_id = $1
            "#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        let conversations = self
            .conversation_summaries(user_id, None, limit, offset)
            .await?;

        Ok((conversations, total))
    }

    /// One conversation as `user_id` sees it, for the thread header. `None` when
    /// they are not a member, which is also the answer for an id that is gone.
    pub async fn conversation_summary(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<Option<ConversationSummary>> {
        Ok(self
            .conversation_summaries(user_id, Some(conversation_id), 1, 0)
            .await?
            .into_iter()
            .next())
    }

    /// Shared projection behind [`Self::list_conversations`] and
    /// [`Self::conversation_summary`]: the same row shape, narrowed to one
    /// conversation when `conversation_id` is given.
    async fn conversation_summaries(
        &self,
        user_id: &UserId,
        conversation_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ConversationSummary>> {
        let rows = sqlx::query!(
            r#"
            SELECT c.id,
                   c.kind,
                   c.title,
                   c.owner_user_id AS "owner_user_id?: UserId",
                   c.created_at,
                   c.updated_at,
                   m.role,
                   m.muted,
                   (
                       SELECT COUNT(*) FROM social_conversation_members mm
                       WHERE mm.conversation_id = c.id
                   ) AS "member_count!",
                   (
                       SELECT COUNT(*) FROM social_messages sm
                       WHERE sm.conversation_id = c.id
                         AND sm.deleted_at IS NULL
                         AND sm.id > COALESCE(m.last_read_message_id, 0)
                         AND (sm.sender_user_id IS NULL OR sm.sender_user_id <> $1)
                   ) AS "unread_count!",
                   (
                       SELECT mm2.user_id FROM social_conversation_members mm2
                       WHERE mm2.conversation_id = c.id
                         AND mm2.user_id <> $1
                         AND c.kind = 'direct'
                       LIMIT 1
                   ) AS "counterpart_user_id?: UserId",
                   lm.id AS "last_message_id?",
                   lm.sender_user_id AS "last_message_sender_user_id?: UserId",
                   lm.kind AS "last_message_kind?",
                   lm.body AS "last_message_body?",
                   lm.created_at AS "last_message_created_at?",
                   (lm.deleted_at IS NOT NULL) AS "last_message_deleted?"
            FROM social_conversations c
            JOIN social_conversation_members m
              ON m.conversation_id = c.id AND m.user_id = $1
            LEFT JOIN social_messages lm ON lm.id = c.last_message_id
            WHERE ($4::bigint IS NULL OR c.id = $4)
            ORDER BY COALESCE(c.last_message_at, c.created_at) DESC, c.id DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id as &UserId,
            limit,
            offset,
            conversation_id,
        )
        .fetch_all(self.pool())
        .await?;

        let referenced_ids: Vec<UserId> = rows
            .iter()
            .flat_map(|row| {
                [row.counterpart_user_id, row.last_message_sender_user_id]
                    .into_iter()
                    .flatten()
            })
            .collect();
        let users = user_index(self, &referenced_ids).await?;

        let mut conversations = Vec::with_capacity(rows.len());
        for row in rows {
            let last_message = match (row.last_message_id, row.last_message_created_at) {
                (Some(id), Some(created_at)) => {
                    let deleted = row.last_message_deleted.unwrap_or(false);
                    Some(SocialMessage {
                        id,
                        conversation_id: row.id,
                        sender: row
                            .last_message_sender_user_id
                            .and_then(|sender_id| users.get(&sender_id.as_i64()).cloned()),
                        kind: parse_enum::<SocialMessageKind>(
                            row.last_message_kind.as_deref().unwrap_or("text"),
                            "social message kind",
                        )?,
                        // A removed message keeps its slot but not its text.
                        body: if deleted {
                            String::new()
                        } else {
                            row.last_message_body.unwrap_or_default()
                        },
                        created_at,
                        deleted,
                    })
                }
                _ => None,
            };

            conversations.push(ConversationSummary {
                id: row.id,
                kind: parse_enum::<ConversationKind>(&row.kind, "conversation kind")?,
                title: row.title,
                owner_user_id: row.owner_user_id,
                counterpart: row
                    .counterpart_user_id
                    .and_then(|id| users.get(&id.as_i64()).cloned()),
                member_count: row.member_count,
                viewer_role: parse_enum::<ConversationMemberRole>(
                    &row.role,
                    "conversation member role",
                )?,
                muted: row.muted,
                unread_count: row.unread_count,
                last_message,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(conversations)
    }

    /// A page of messages, newest first, ending before `before_message_id`.
    ///
    /// Cursor paging rather than offsets: a thread grows at the end while it is
    /// being read, which shifts every offset.
    pub async fn list_conversation_messages(
        &self,
        conversation_id: i64,
        before_message_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<SocialMessage>> {
        let rows = sqlx::query!(
            r#"
            SELECT id,
                   sender_user_id AS "sender_user_id?: UserId",
                   kind,
                   body,
                   created_at,
                   (deleted_at IS NOT NULL) AS "deleted!"
            FROM social_messages
            WHERE conversation_id = $1
              AND ($2::bigint IS NULL OR id < $2)
            ORDER BY id DESC
            LIMIT $3
            "#,
            conversation_id,
            before_message_id,
            limit,
        )
        .fetch_all(self.pool())
        .await?;

        let sender_ids: Vec<UserId> = rows.iter().filter_map(|row| row.sender_user_id).collect();
        let users = user_index(self, &sender_ids).await?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            messages.push(SocialMessage {
                id: row.id,
                conversation_id,
                sender: row
                    .sender_user_id
                    .and_then(|sender_id| users.get(&sender_id.as_i64()).cloned()),
                kind: parse_enum::<SocialMessageKind>(&row.kind, "social message kind")?,
                body: if row.deleted { String::new() } else { row.body },
                created_at: row.created_at,
                deleted: row.deleted,
            });
        }

        Ok(messages)
    }

    /// Appends a message and moves the conversation's newest-message pointer.
    pub async fn send_conversation_message(
        &self,
        conversation_id: i64,
        sender_user_id: &UserId,
        body: &str,
    ) -> Result<SocialMessage> {
        let mut tx = self.pool().begin().await?;

        let row = sqlx::query!(
            r#"
            INSERT INTO social_messages (conversation_id, sender_user_id, kind, body)
            VALUES ($1, $2, 'text', $3)
            RETURNING id, created_at
            "#,
            conversation_id,
            sender_user_id as &UserId,
            body,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            UPDATE social_conversations
            SET last_message_id = $2,
                last_message_at = $3,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            "#,
            conversation_id,
            row.id,
            row.created_at,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let sender = self.get_by_id(sender_user_id).await?;

        Ok(SocialMessage {
            id: row.id,
            conversation_id,
            sender,
            kind: SocialMessageKind::Text,
            body: body.to_string(),
            created_at: row.created_at,
            deleted: false,
        })
    }

    /// Soft-deletes a message. Only the author is allowed through the predicate,
    /// so a client cannot delete someone else's line by guessing an id.
    pub async fn delete_conversation_message(
        &self,
        conversation_id: i64,
        message_id: i64,
        sender_user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE social_messages
            SET deleted_at = CURRENT_TIMESTAMP, body = ''
            WHERE id = $1 AND conversation_id = $2 AND sender_user_id = $3
              AND deleted_at IS NULL
            "#,
            message_id,
            conversation_id,
            sender_user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Advances the caller's read cursor, to `up_to_message_id` or to the newest
    /// message. `GREATEST` keeps it monotonic, so an out-of-order request from a
    /// second device cannot make a thread unread again.
    pub async fn mark_conversation_read(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        up_to_message_id: Option<i64>,
    ) -> Result<i64> {
        let cursor = sqlx::query_scalar!(
            r#"
            UPDATE social_conversation_members m
            SET last_read_message_id = GREATEST(
                COALESCE(m.last_read_message_id, 0),
                COALESCE($3, (
                    SELECT COALESCE(MAX(sm.id), 0)
                    FROM social_messages sm
                    WHERE sm.conversation_id = $1
                ))
            )
            WHERE m.conversation_id = $1 AND m.user_id = $2
            RETURNING COALESCE(m.last_read_message_id, 0) AS "cursor!"
            "#,
            conversation_id,
            user_id as &UserId,
            up_to_message_id,
        )
        .fetch_optional(self.pool())
        .await?;

        cursor.ok_or_else(|| Error::NotFound("Conversation not found".to_string()))
    }

    /// Members of a group, owners and admins first.
    pub async fn list_conversation_members(
        &self,
        conversation_id: i64,
        pagination: PageParams,
    ) -> Result<(Vec<ConversationMember>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM social_conversation_members m
            JOIN user_account_profiles p ON p.id = m.user_id
            WHERE m.conversation_id = $1 AND p.deleted_at IS NULL
            "#,
            conversation_id,
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query!(
            r#"
            SELECT m.user_id AS "user_id!: UserId",
                   m.role,
                   m.joined_at
            FROM social_conversation_members m
            JOIN user_account_profiles p ON p.id = m.user_id
            WHERE m.conversation_id = $1 AND p.deleted_at IS NULL
            ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
                     m.joined_at ASC,
                     m.user_id ASC
            LIMIT $2 OFFSET $3
            "#,
            conversation_id,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let ids: Vec<UserId> = rows.iter().map(|row| row.user_id).collect();
        let users = user_index(self, &ids).await?;

        let mut members = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(user) = users.get(&row.user_id.as_i64()).cloned() else {
                continue;
            };
            members.push(ConversationMember {
                user,
                role: parse_enum::<ConversationMemberRole>(&row.role, "conversation member role")?,
                joined_at: row.joined_at,
            });
        }

        Ok((members, total))
    }

    /// Adds accounts to a group, returning how many were not already in it.
    pub async fn add_conversation_members(
        &self,
        conversation_id: i64,
        user_ids: &[UserId],
    ) -> Result<u64> {
        let ids: Vec<i64> = user_ids.iter().map(UserId::as_i64).collect();
        let result = sqlx::query!(
            r#"
            INSERT INTO social_conversation_members (conversation_id, user_id, role)
            SELECT $1, member_id, 'member' FROM UNNEST($2::bigint[]) AS member_id
            ON CONFLICT (conversation_id, user_id) DO NOTHING
            "#,
            conversation_id,
            &ids,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Removes one member. Used both for leaving and for being removed; the
    /// difference is which authorization the service applied first.
    pub async fn remove_conversation_member(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            DELETE FROM social_conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
            conversation_id,
            user_id as &UserId,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Renames a group.
    pub async fn set_conversation_title(&self, conversation_id: i64, title: &str) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE social_conversations
            SET title = $2, updated_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND kind = 'group'
            "#,
            conversation_id,
            title,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Deletes a group and, by cascade, its members and messages.
    pub async fn delete_conversation(&self, conversation_id: i64) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM social_conversations WHERE id = $1 AND kind = 'group'",
            conversation_id,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// What kind of conversation this is, for the rules that only apply to groups.
    pub async fn conversation_kind(
        &self,
        conversation_id: i64,
    ) -> Result<Option<ConversationKind>> {
        let kind = sqlx::query_scalar!(
            "SELECT kind FROM social_conversations WHERE id = $1",
            conversation_id,
        )
        .fetch_optional(self.pool())
        .await?;

        kind.map(|kind| parse_enum::<ConversationKind>(&kind, "conversation kind"))
            .transpose()
    }

    /// Silences or unsilences a thread for one member. The flag is per member, so
    /// muting is invisible to the other side.
    pub async fn set_conversation_muted(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        muted: bool,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE social_conversation_members
            SET muted = $3
            WHERE conversation_id = $1 AND user_id = $2
            "#,
            conversation_id,
            user_id as &UserId,
            muted,
        )
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// The three numbers the 消息 tab badges itself with, in one round trip.
    ///
    /// Muted threads still count: muting suppresses notifications, not the fact
    /// that something is unread.
    pub async fn social_unread_summary(&self, user_id: &UserId) -> Result<SocialUnreadSummary> {
        let row = sqlx::query!(
            r#"
            WITH per_conversation AS (
                SELECT (
                    SELECT COUNT(*) FROM social_messages sm
                    WHERE sm.conversation_id = m.conversation_id
                      AND sm.deleted_at IS NULL
                      AND sm.id > COALESCE(m.last_read_message_id, 0)
                      AND (sm.sender_user_id IS NULL OR sm.sender_user_id <> $1)
                ) AS unread_count
                FROM social_conversation_members m
                WHERE m.user_id = $1
            )
            -- SUM over bigint is NUMERIC in Postgres, and reading NUMERIC would
            -- pull in a decimal crate for a number that cannot exceed a count.
            SELECT COALESCE(SUM(unread_count), 0)::bigint AS "unread_message_count!",
                   COUNT(*) FILTER (WHERE unread_count > 0) AS "unread_conversation_count!",
                   (
                       SELECT COUNT(*)
                       FROM friend_requests r
                       JOIN user_account_profiles p ON p.id = r.requester_user_id
                       WHERE r.addressee_user_id = $1
                         AND r.status = 'pending'
                         AND p.deleted_at IS NULL
                   ) AS "pending_friend_request_count!"
            FROM per_conversation
            "#,
            user_id as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(SocialUnreadSummary {
            unread_message_count: row.unread_message_count,
            unread_conversation_count: row.unread_conversation_count,
            pending_friend_request_count: row.pending_friend_request_count,
        })
    }

    /// Every member id, for fanning a change out to the accounts affected.
    /// Every conversation on the server, newest activity first, for the admin
    /// moderation view. Unlike [`Self::list_conversations`] this is not scoped
    /// to a member, because an administrator auditing a group is not in it.
    ///
    /// `kind` narrows to groups or direct threads; `search` matches a group
    /// title or the username of any member, so a report about one account finds
    /// the threads it appears in.
    pub async fn list_all_conversations(
        &self,
        kind: Option<ConversationKind>,
        search: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<AdminConversationOverview>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;
        let pattern = search.and_then(ilike_contains_pattern);
        let kind = kind.map(|kind| kind.as_str().to_string());

        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "total!"
            FROM social_conversations c
            WHERE ($1::text IS NULL OR c.kind = $1)
              AND (
                $2::text IS NULL
                OR c.title ILIKE $2
                OR EXISTS (
                    SELECT 1 FROM social_conversation_members m
                    JOIN users u ON u.id = m.user_id
                    WHERE m.conversation_id = c.id AND u.username ILIKE $2
                )
              )
            "#,
            kind.as_deref(),
            pattern.as_deref(),
        )
        .fetch_one(self.pool())
        .await?;

        let rows = sqlx::query!(
            r#"
            SELECT c.id,
                   c.kind,
                   c.title,
                   c.owner_user_id AS "owner_user_id?: UserId",
                   c.last_message_at,
                   c.created_at,
                   (
                       SELECT COUNT(*) FROM social_conversation_members m
                       JOIN user_account_profiles p ON p.id = m.user_id
                       WHERE m.conversation_id = c.id AND p.deleted_at IS NULL
                   ) AS "member_count!",
                   (
                       SELECT COUNT(*) FROM social_messages sm
                       WHERE sm.conversation_id = c.id AND sm.deleted_at IS NULL
                   ) AS "message_count!"
            FROM social_conversations c
            WHERE ($1::text IS NULL OR c.kind = $1)
              AND (
                $2::text IS NULL
                OR c.title ILIKE $2
                OR EXISTS (
                    SELECT 1 FROM social_conversation_members m
                    JOIN users u ON u.id = m.user_id
                    WHERE m.conversation_id = c.id AND u.username ILIKE $2
                )
              )
            ORDER BY c.last_message_at DESC NULLS LAST, c.id DESC
            LIMIT $3 OFFSET $4
            "#,
            kind.as_deref(),
            pattern.as_deref(),
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let owner_ids: Vec<UserId> = rows.iter().filter_map(|row| row.owner_user_id).collect();
        let owners = user_index(self, &owner_ids).await?;

        let mut conversations = Vec::with_capacity(rows.len());
        for row in rows {
            conversations.push(AdminConversationOverview {
                id: row.id,
                kind: parse_enum::<ConversationKind>(&row.kind, "conversation kind")?,
                title: row.title,
                owner: row
                    .owner_user_id
                    .and_then(|owner_id| owners.get(&owner_id.as_i64()).cloned()),
                member_count: row.member_count,
                message_count: row.message_count,
                last_message_at: row.last_message_at,
                created_at: row.created_at,
            });
        }

        Ok((conversations, total))
    }

    pub async fn conversation_member_ids(&self, conversation_id: i64) -> Result<Vec<UserId>> {
        let ids = sqlx::query_scalar!(
            r#"
            SELECT user_id AS "user_id!: UserId"
            FROM social_conversation_members
            WHERE conversation_id = $1
            "#,
            conversation_id,
        )
        .fetch_all(self.pool())
        .await?;

        Ok(ids)
    }
}
