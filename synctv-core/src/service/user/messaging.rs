//! Direct threads and groups behind the 消息 tab.
//!
//! Authorization is the substance of this file. Every conversation-scoped call
//! starts by resolving the caller's membership row: a non-member gets "not
//! found" rather than "forbidden", because whether a conversation exists is
//! itself private. Beyond membership, two rules shape who can talk to whom —
//! a direct thread stays writable only while both accounts are friends, and only
//! friends can be pulled into a group.

use crate::{
    models::{
        AdminConversationOverview, ConversationKind, ConversationMember, ConversationMemberRole,
        ConversationSummary, NewGroupConversation, PageParams, SocialMessage, UserId,
        CONVERSATION_MEMBER_MAX, CONVERSATION_TITLE_MAX_CHARS, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
        SOCIAL_MESSAGE_BODY_MAX_CHARS,
    },
    Error, Result,
};

use super::UserService;

impl UserService {
    /// Opens the direct thread with a friend, or returns the existing one.
    pub async fn open_direct_conversation(
        &self,
        user_id: &UserId,
        other_user_id: &UserId,
    ) -> Result<i64> {
        if user_id == other_user_id {
            return Err(Error::InvalidInput(
                "Cannot open a conversation with yourself".to_string(),
            ));
        }
        self.ensure_friendable(user_id, other_user_id).await?;
        if !self.may_message(user_id, other_user_id).await? {
            return Err(Error::Authorization(
                "You can only message friends".to_string(),
            ));
        }

        self.repository
            .ensure_direct_conversation(user_id, other_user_id)
            .await
    }

    /// Creates a group with the caller as owner.
    ///
    /// Only friends can be invited, and the size cap counts the owner, so the
    /// group that comes back is never already over the limit.
    pub async fn create_group_conversation(
        &self,
        owner_user_id: &UserId,
        request: &NewGroupConversation,
    ) -> Result<i64> {
        let title = validate_conversation_title(&request.title)?;

        let mut invited: Vec<UserId> = request
            .member_user_ids
            .iter()
            .copied()
            .filter(|id| id != owner_user_id)
            .collect();
        invited.sort_unstable_by_key(UserId::as_i64);
        invited.dedup_by_key(|id| id.as_i64());

        if invited.len() + 1 > CONVERSATION_MEMBER_MAX {
            return Err(Error::InvalidInput(format!(
                "a group may hold at most {CONVERSATION_MEMBER_MAX} members"
            )));
        }
        self.ensure_all_friends(owner_user_id, &invited).await?;

        self.repository
            .create_group_conversation(owner_user_id, &title, &invited)
            .await
    }

    /// The caller's conversations, most recently active first.
    pub async fn list_conversations(
        &self,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<ConversationSummary>, i64)> {
        self.repository
            .list_conversations(user_id, pagination)
            .await
    }

    /// One conversation, for the thread header.
    pub async fn conversation_summary(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<ConversationSummary> {
        self.repository
            .conversation_summary(conversation_id, user_id)
            .await?
            .ok_or_else(conversation_not_found)
    }

    /// A page of messages, newest first, ending before `before_message_id`.
    pub async fn list_conversation_messages(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        before_message_id: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<SocialMessage>> {
        self.require_membership(conversation_id, user_id).await?;

        self.repository
            .list_conversation_messages(conversation_id, before_message_id, message_limit(limit))
            .await
    }

    /// Appends a message written by the caller.
    pub async fn send_conversation_message(
        &self,
        conversation_id: i64,
        sender_user_id: &UserId,
        body: &str,
    ) -> Result<SocialMessage> {
        let body = validate_message_body(body)?;
        self.require_membership(conversation_id, sender_user_id)
            .await?;
        self.ensure_writable(conversation_id, sender_user_id)
            .await?;

        self.repository
            .send_conversation_message(conversation_id, sender_user_id, &body)
            .await
    }

    /// Withdraws the caller's own message. Only the author matches the predicate,
    /// so someone else's line cannot be removed by guessing an id.
    pub async fn delete_conversation_message(
        &self,
        conversation_id: i64,
        message_id: i64,
        sender_user_id: &UserId,
    ) -> Result<()> {
        self.require_membership(conversation_id, sender_user_id)
            .await?;
        if !self
            .repository
            .delete_conversation_message(conversation_id, message_id, sender_user_id)
            .await?
        {
            return Err(Error::NotFound("Message not found".to_string()));
        }
        Ok(())
    }

    /// Advances the caller's read cursor, returning where it now stands.
    pub async fn mark_conversation_read(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        up_to_message_id: Option<i64>,
    ) -> Result<i64> {
        self.require_membership(conversation_id, user_id).await?;

        self.repository
            .mark_conversation_read(conversation_id, user_id, up_to_message_id)
            .await
    }

    /// Silences a thread for the caller alone.
    pub async fn set_conversation_muted(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        muted: bool,
    ) -> Result<()> {
        if !self
            .repository
            .set_conversation_muted(conversation_id, user_id, muted)
            .await?
        {
            return Err(conversation_not_found());
        }
        Ok(())
    }

    /// Every conversation on the server, for the admin moderation view.
    ///
    /// Takes no viewer: unlike [`Self::list_conversations`] there is no
    /// membership to scope by, because an administrator auditing a group is not
    /// a member of it. Callers are admitted by `require_admin_actor` at the API
    /// boundary instead.
    pub async fn admin_list_conversations(
        &self,
        kind: Option<ConversationKind>,
        search: Option<&str>,
        pagination: PageParams,
    ) -> Result<(Vec<AdminConversationOverview>, i64)> {
        self.repository
            .list_all_conversations(kind, search, pagination)
            .await
    }

    /// Members of any conversation, without the membership check.
    ///
    /// The repository query behind this is the same one
    /// [`Self::list_conversation_members`] uses; only the gate differs.
    pub async fn admin_list_conversation_members(
        &self,
        conversation_id: i64,
        pagination: PageParams,
    ) -> Result<(Vec<ConversationMember>, i64)> {
        self.repository
            .list_conversation_members(conversation_id, pagination)
            .await
    }

    /// A page of any conversation's history, without the membership check.
    ///
    /// Soft-deleted messages come back marked and empty, exactly as they do for
    /// a member: a withdrawn line stays withdrawn, and moderation reads the
    /// thread as it stands rather than recovering what was taken back.
    pub async fn admin_list_conversation_messages(
        &self,
        conversation_id: i64,
        before_message_id: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<SocialMessage>> {
        self.repository
            .list_conversation_messages(conversation_id, before_message_id, message_limit(limit))
            .await
    }

    /// Members of a conversation the caller belongs to.
    pub async fn list_conversation_members(
        &self,
        conversation_id: i64,
        user_id: &UserId,
        pagination: PageParams,
    ) -> Result<(Vec<ConversationMember>, i64)> {
        self.require_membership(conversation_id, user_id).await?;

        self.repository
            .list_conversation_members(conversation_id, pagination)
            .await
    }

    /// Adds friends of the caller to a group, returning how many were new.
    pub async fn add_conversation_members(
        &self,
        conversation_id: i64,
        actor_user_id: &UserId,
        user_ids: &[UserId],
    ) -> Result<u64> {
        self.require_group_manager(conversation_id, actor_user_id)
            .await?;

        let mut invited: Vec<UserId> = user_ids.to_vec();
        invited.sort_unstable_by_key(UserId::as_i64);
        invited.dedup_by_key(|id| id.as_i64());
        if invited.is_empty() {
            return Ok(0);
        }
        self.ensure_all_friends(actor_user_id, &invited).await?;

        // Counted before the insert: the members table has no cap of its own, so
        // this is the only thing keeping a group inside its ceiling.
        let current = self
            .repository
            .conversation_member_ids(conversation_id)
            .await?;
        let joining = invited
            .iter()
            .filter(|id| !current.iter().any(|existing| existing == *id))
            .count();
        if current.len() + joining > CONVERSATION_MEMBER_MAX {
            return Err(Error::InvalidInput(format!(
                "a group may hold at most {CONVERSATION_MEMBER_MAX} members"
            )));
        }

        self.repository
            .add_conversation_members(conversation_id, &invited)
            .await
    }

    /// Removes someone else from a group.
    pub async fn remove_conversation_member(
        &self,
        conversation_id: i64,
        actor_user_id: &UserId,
        member_user_id: &UserId,
    ) -> Result<()> {
        if actor_user_id == member_user_id {
            return Err(Error::InvalidInput(
                "Leave the group instead of removing yourself".to_string(),
            ));
        }
        let actor_role = self
            .require_group_manager(conversation_id, actor_user_id)
            .await?;

        let member_role = self
            .repository
            .conversation_member_role(conversation_id, member_user_id)
            .await?
            .ok_or_else(|| Error::NotFound("Member not found".to_string()))?;
        // An admin may clear out members but not their peers or the owner; only
        // the owner outranks an admin.
        if member_role >= actor_role {
            return Err(Error::Authorization(
                "Cannot remove a member of equal or higher rank".to_string(),
            ));
        }

        if !self
            .repository
            .remove_conversation_member(conversation_id, member_user_id)
            .await?
        {
            return Err(Error::NotFound("Member not found".to_string()));
        }
        Ok(())
    }

    /// Leaves a group.
    ///
    /// The owner cannot: a group without an owner has nobody who can delete it,
    /// so they delete it instead.
    pub async fn leave_conversation(&self, conversation_id: i64, user_id: &UserId) -> Result<()> {
        let role = self.require_membership(conversation_id, user_id).await?;
        if self.conversation_kind(conversation_id).await? != ConversationKind::Group {
            return Err(Error::InvalidInput("Only groups can be left".to_string()));
        }
        if role == ConversationMemberRole::Owner {
            return Err(Error::InvalidInput(
                "The owner cannot leave; delete the group instead".to_string(),
            ));
        }

        if !self
            .repository
            .remove_conversation_member(conversation_id, user_id)
            .await?
        {
            return Err(conversation_not_found());
        }
        Ok(())
    }

    /// Renames a group.
    pub async fn set_conversation_title(
        &self,
        conversation_id: i64,
        actor_user_id: &UserId,
        title: &str,
    ) -> Result<()> {
        let title = validate_conversation_title(title)?;
        self.require_group_manager(conversation_id, actor_user_id)
            .await?;

        if !self
            .repository
            .set_conversation_title(conversation_id, &title)
            .await?
        {
            return Err(conversation_not_found());
        }
        Ok(())
    }

    /// Deletes a group and, by cascade, its members and messages.
    pub async fn delete_conversation(
        &self,
        conversation_id: i64,
        actor_user_id: &UserId,
    ) -> Result<()> {
        let role = self
            .require_membership(conversation_id, actor_user_id)
            .await?;
        if role != ConversationMemberRole::Owner {
            return Err(Error::Authorization(
                "Only the group owner can delete it".to_string(),
            ));
        }

        if !self.repository.delete_conversation(conversation_id).await? {
            return Err(Error::InvalidInput(
                "Only groups can be deleted".to_string(),
            ));
        }
        Ok(())
    }

    /// The caller's role, or "not found" when they are not a member. A
    /// conversation the caller is not in must not be distinguishable from one
    /// that does not exist.
    async fn require_membership(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<ConversationMemberRole> {
        self.repository
            .conversation_member_role(conversation_id, user_id)
            .await?
            .ok_or_else(conversation_not_found)
    }

    /// The caller's role, having established that this is a group they may
    /// administer.
    async fn require_group_manager(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<ConversationMemberRole> {
        let role = self.require_membership(conversation_id, user_id).await?;
        if self.conversation_kind(conversation_id).await? != ConversationKind::Group {
            return Err(Error::InvalidInput(
                "This conversation has no member list to manage".to_string(),
            ));
        }
        if !role.can_manage() {
            return Err(Error::Authorization(
                "Only a group owner or admin can do this".to_string(),
            ));
        }
        Ok(role)
    }

    async fn conversation_kind(&self, conversation_id: i64) -> Result<ConversationKind> {
        self.repository
            .conversation_kind(conversation_id)
            .await?
            .ok_or_else(conversation_not_found)
    }

    /// Whether the caller may still write here.
    ///
    /// A group is writable by any member. A direct thread is writable only while
    /// both accounts are friends, so unfriending — or being blocked — closes the
    /// thread without deleting what it holds.
    async fn ensure_writable(&self, conversation_id: i64, sender_user_id: &UserId) -> Result<()> {
        if self.conversation_kind(conversation_id).await? == ConversationKind::Group {
            return Ok(());
        }

        let counterpart = self
            .repository
            .conversation_member_ids(conversation_id)
            .await?
            .into_iter()
            .find(|member_id| member_id != sender_user_id)
            .ok_or_else(|| {
                Error::Conflict("The other participant is no longer here".to_string())
            })?;

        self.ensure_friendable(sender_user_id, &counterpart).await?;
        if !self.may_message(sender_user_id, &counterpart).await? {
            return Err(Error::Authorization(
                "You can only message friends".to_string(),
            ));
        }
        Ok(())
    }

    /// Who may hold a direct thread with whom.
    ///
    /// Friends, and a bound couple. 悄悄话 in 爱的小窝 is the same thread the
    /// 消息 tab shows, and requiring a pair who have bound their accounts to
    /// also send each other a friend request is a hoop with nothing behind it —
    /// binding already needed both of them to agree.
    async fn may_message(&self, actor: &UserId, other: &UserId) -> Result<bool> {
        if self.repository.are_friends(actor, other).await? {
            return Ok(true);
        }
        self.repository.are_bound_couple(actor, other).await
    }

    /// Rejects an invite list containing anyone the caller is not friends with.
    async fn ensure_all_friends(&self, actor_user_id: &UserId, others: &[UserId]) -> Result<()> {
        for other_user_id in others {
            if other_user_id == actor_user_id {
                continue;
            }
            self.ensure_friendable(actor_user_id, other_user_id).await?;
            if !self
                .repository
                .are_friends(actor_user_id, other_user_id)
                .await?
            {
                return Err(Error::Authorization(
                    "You can only add friends to a group".to_string(),
                ));
            }
        }
        Ok(())
    }
}

fn conversation_not_found() -> Error {
    Error::NotFound("Conversation not found".to_string())
}

fn validate_conversation_title(title: &str) -> Result<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("title is required".to_string()));
    }
    if trimmed.chars().count() > CONVERSATION_TITLE_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "title must be at most {CONVERSATION_TITLE_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// A message body. Whitespace-only is rejected rather than stored: it would
/// occupy a row, bump every unread count, and render as nothing.
fn validate_message_body(body: &str) -> Result<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("message body is required".to_string()));
    }
    if trimmed.chars().count() > SOCIAL_MESSAGE_BODY_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "message body must be at most {SOCIAL_MESSAGE_BODY_MAX_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Messages are cursor-paged, so the page size arrives on its own rather than
/// inside [`PageParams`]; the bounds are the same ones list endpoints use.
fn message_limit(limit: Option<u32>) -> i64 {
    i64::from(limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE))
}
