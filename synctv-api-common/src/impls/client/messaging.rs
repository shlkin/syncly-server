//! Direct threads and group chats behind the 消息 tab.
//!
//! Every write lands on the service, which resolves the caller's membership
//! first, so nothing here decides who may read a thread. What this module owns is
//! the read shape: a page of conversations or messages names accounts repeatedly —
//! the same author on twenty lines — so the accounts are collected, resolved once
//! each, and looked up by id while the rows are converted.

use std::collections::HashMap;

use synctv_core::models::{
    ConversationKind, ConversationMember, ConversationMemberRole, ConversationSummary,
    NewGroupConversation, SocialMessage, SocialMessageKind, User, UserId, DEFAULT_PAGE_SIZE,
    MAX_PAGE_SIZE,
};

use super::convert::{optional_user_id_to_proto, total_to_proto};
use super::ClientApiImpl;
use crate::impls::ApiError;

/// Accounts resolved to their public view, keyed for repeated lookup within one
/// page of rows.
type UserViews = HashMap<UserId, synctv_proto::client::UserPublicView>;

impl ClientApiImpl {
    /// Opens the direct thread with a friend, creating it on first use.
    pub async fn open_direct_conversation(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::OpenDirectConversationRequest,
    ) -> Result<synctv_proto::client::OpenDirectConversationResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let other_user_id =
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?;
        let conversation_id = self
            .user_service
            .open_direct_conversation(user_id, &other_user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::OpenDirectConversationResponse {
            conversation: Some(self.loaded_conversation(conversation_id, user_id).await?),
        })
    }

    /// Creates a group with the caller as owner.
    ///
    /// Only friends may be invited. A list naming a stranger is refused whole
    /// rather than quietly trimmed, so the group that comes back is the group
    /// that was asked for.
    pub async fn create_group_conversation(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::CreateGroupConversationRequest,
    ) -> Result<synctv_proto::client::CreateGroupConversationResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let member_user_ids = req
            .member_user_ids
            .into_iter()
            .map(|id| crate::impls::proto_validated_user_id(id, &self.public_id_codec))
            .collect::<Result<Vec<_>, _>>()?;
        let new_group = NewGroupConversation {
            title: req.title,
            member_user_ids,
        };
        let conversation_id = self
            .user_service
            .create_group_conversation(user_id, &new_group)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::CreateGroupConversationResponse {
            conversation: Some(self.loaded_conversation(conversation_id, user_id).await?),
        })
    }

    /// The 消息 list itself: one page of threads, most recently active first.
    pub async fn list_conversations(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListConversationsRequest,
    ) -> Result<synctv_proto::client::ListConversationsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_conversations(user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let views = self.conversation_user_views(&page).await?;
        let conversations = page
            .into_iter()
            .map(|summary| self.conversation_summary_to_proto(summary, &views))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListConversationsResponse {
            conversations,
            total: total_to_proto(total, "conversation")?,
        })
    }

    /// One thread, for the header above its messages.
    pub async fn get_conversation(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetConversationRequest,
    ) -> Result<synctv_proto::client::GetConversationResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;

        Ok(synctv_proto::client::GetConversationResponse {
            conversation: Some(self.loaded_conversation(conversation_id, user_id).await?),
        })
    }

    /// A page of the thread, newest first. `before_message_id` walks backwards
    /// from a message the client already holds, so paging stays correct while new
    /// messages arrive at the front.
    pub async fn list_conversation_messages(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListConversationMessagesRequest,
    ) -> Result<synctv_proto::client::ListConversationMessagesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let before_message_id = self.decode_optional_social_message_id(&req.before_message_id)?;
        let limit = (req.limit > 0).then(|| req.limit.cast_unsigned());
        let messages = self
            .user_service
            .list_conversation_messages(conversation_id, user_id, before_message_id, limit)
            .await
            .map_err(ApiError::from)?;
        let views = self.message_user_views(&messages).await?;
        let messages = messages
            .into_iter()
            .map(|message| self.social_message_to_proto(message, &views))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListConversationMessagesResponse { messages })
    }

    pub async fn send_conversation_message(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SendConversationMessageRequest,
    ) -> Result<synctv_proto::client::SendConversationMessageResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let message = self
            .user_service
            .send_conversation_message(conversation_id, user_id, &req.body)
            .await
            .map_err(ApiError::from)?;
        let views = self
            .message_user_views(std::slice::from_ref(&message))
            .await?;

        Ok(synctv_proto::client::SendConversationMessageResponse {
            message: Some(self.social_message_to_proto(message, &views)?),
        })
    }

    /// Withdraws one of the caller's own messages. The row stays, emptied: read
    /// cursors are message ids, so removing the row would strand them.
    pub async fn delete_conversation_message(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteConversationMessageRequest,
    ) -> Result<synctv_proto::client::DeleteConversationMessageResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let message_id = self.decode_social_message_id(&req.message_id)?;
        self.user_service
            .delete_conversation_message(conversation_id, message_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::DeleteConversationMessageResponse { success: true })
    }

    /// Advances the caller's read cursor, which is what clears the tab badge.
    /// An omitted `up_to_message_id` reads the whole thread.
    pub async fn mark_conversation_read(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::MarkConversationReadRequest,
    ) -> Result<synctv_proto::client::MarkConversationReadResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let up_to_message_id = self.decode_optional_social_message_id(&req.up_to_message_id)?;
        let last_read_message_id = self
            .user_service
            .mark_conversation_read(conversation_id, user_id, up_to_message_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::MarkConversationReadResponse {
            // A thread with no messages leaves the cursor at 0, which is not an
            // id and so has no encoding: it travels as the empty string.
            last_read_message_id: if last_read_message_id > 0 {
                self.public_id_codec
                    .encode_social_message_id(last_read_message_id)
                    .map_err(ApiError::Internal)?
            } else {
                String::new()
            },
        })
    }

    /// Silences a thread for the caller alone; the other members are unaffected.
    pub async fn set_conversation_muted(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SetConversationMutedRequest,
    ) -> Result<synctv_proto::client::SetConversationMutedResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        self.user_service
            .set_conversation_muted(conversation_id, user_id, req.muted)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::SetConversationMutedResponse { success: true })
    }

    /// The member sheet. Each row is a different account, so the avatars are
    /// batched positionally rather than through the id map.
    pub async fn list_conversation_members(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListConversationMembersRequest,
    ) -> Result<synctv_proto::client::ListConversationMembersResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_conversation_members(conversation_id, user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let users = page
            .iter()
            .map(|member| member.user.clone())
            .collect::<Vec<_>>();
        let users = self
            .batch_user_public_views_with_loaded_avatars(&users)
            .await?;
        let members = page
            .into_iter()
            .zip(users)
            .map(|(member, user)| conversation_member_to_proto(member, user))
            .collect();

        Ok(synctv_proto::client::ListConversationMembersResponse {
            members,
            total: total_to_proto(total, "conversation member")?,
        })
    }

    /// Adds friends of the caller to a group. Accounts already in it are skipped
    /// rather than refused, so `added_count` may be lower than the list sent.
    pub async fn add_conversation_members(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::AddConversationMembersRequest,
    ) -> Result<synctv_proto::client::AddConversationMembersResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let member_user_ids = req
            .user_ids
            .into_iter()
            .map(|id| crate::impls::proto_validated_user_id(id, &self.public_id_codec))
            .collect::<Result<Vec<_>, _>>()?;
        let added_count = self
            .user_service
            .add_conversation_members(conversation_id, user_id, &member_user_ids)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::AddConversationMembersResponse {
            added_count: i64::try_from(added_count).map_err(|_| {
                ApiError::Internal("added member count exceeds i64::MAX".to_string())
            })?,
        })
    }

    /// Removes someone else from a group. The caller leaves through
    /// `leave_conversation` instead.
    pub async fn remove_conversation_member(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::RemoveConversationMemberRequest,
    ) -> Result<synctv_proto::client::RemoveConversationMemberResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        let member_user_id =
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?;
        self.user_service
            .remove_conversation_member(conversation_id, user_id, &member_user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::RemoveConversationMemberResponse { success: true })
    }

    /// Leaves a group. The owner has to delete it instead, so a group is never
    /// left without one.
    pub async fn leave_conversation(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::LeaveConversationRequest,
    ) -> Result<synctv_proto::client::LeaveConversationResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        self.user_service
            .leave_conversation(conversation_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::LeaveConversationResponse { success: true })
    }

    pub async fn set_conversation_title(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SetConversationTitleRequest,
    ) -> Result<synctv_proto::client::SetConversationTitleResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        self.user_service
            .set_conversation_title(conversation_id, user_id, &req.title)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::SetConversationTitleResponse { success: true })
    }

    /// Deletes a group, and with it every message in it. Direct threads belong to
    /// both accounts, so the service refuses them here.
    pub async fn delete_conversation(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::DeleteConversationRequest,
    ) -> Result<synctv_proto::client::DeleteConversationResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let conversation_id = self.decode_conversation_id(&req.conversation_id)?;
        self.user_service
            .delete_conversation(conversation_id, user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::DeleteConversationResponse { success: true })
    }

    fn decode_conversation_id(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_conversation_id(value)
            .map_err(ApiError::InvalidInput)
    }

    fn decode_social_message_id(&self, value: &str) -> Result<i64, ApiError> {
        self.public_id_codec
            .decode_social_message_id(value)
            .map_err(ApiError::InvalidInput)
    }

    /// An empty id is how a client says "no cursor" — the paging call reads from
    /// the newest message, and the read call marks the whole thread.
    fn decode_optional_social_message_id(&self, value: &str) -> Result<Option<i64>, ApiError> {
        if value.is_empty() {
            return Ok(None);
        }
        self.decode_social_message_id(value).map(Some)
    }

    /// Reads a conversation back after a write, so opening a thread and listing
    /// threads hand the client the same row shape.
    async fn loaded_conversation(
        &self,
        conversation_id: i64,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::ConversationSummary, ApiError> {
        let summary = self
            .user_service
            .conversation_summary(conversation_id, user_id)
            .await
            .map_err(ApiError::from)?;
        let views = self
            .conversation_user_views(std::slice::from_ref(&summary))
            .await?;

        self.conversation_summary_to_proto(summary, &views)
    }

    /// Every account a page of rows names: the counterpart of each direct thread,
    /// and the author of each preview line.
    async fn conversation_user_views(
        &self,
        page: &[ConversationSummary],
    ) -> Result<UserViews, ApiError> {
        let users = page
            .iter()
            .flat_map(|summary| {
                summary.counterpart.iter().chain(
                    summary
                        .last_message
                        .as_ref()
                        .and_then(|message| message.sender.as_ref()),
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        self.user_view_map(users).await
    }

    /// The authors of a page of messages. System lines have none.
    async fn message_user_views(&self, messages: &[SocialMessage]) -> Result<UserViews, ApiError> {
        let users = messages
            .iter()
            .filter_map(|message| message.sender.clone())
            .collect::<Vec<_>>();

        self.user_view_map(users).await
    }

    /// Resolves each distinct account once. A page of a thread is mostly the same
    /// two or three authors, so the batch is much smaller than the page.
    async fn user_view_map(&self, mut users: Vec<User>) -> Result<UserViews, ApiError> {
        users.sort_unstable_by_key(|user| user.id.as_i64());
        users.dedup_by_key(|user| user.id.as_i64());
        let ids = users.iter().map(|user| user.id).collect::<Vec<_>>();
        let views = self
            .batch_user_public_views_with_loaded_avatars(&users)
            .await?;

        Ok(ids.into_iter().zip(views).collect())
    }

    fn conversation_summary_to_proto(
        &self,
        summary: ConversationSummary,
        views: &UserViews,
    ) -> Result<synctv_proto::client::ConversationSummary, ApiError> {
        let counterpart = summary
            .counterpart
            .as_ref()
            .and_then(|user| views.get(&user.id))
            .cloned();
        let last_message = summary
            .last_message
            .map(|message| self.social_message_to_proto(message, views))
            .transpose()?;

        Ok(synctv_proto::client::ConversationSummary {
            id: self
                .public_id_codec
                .encode_conversation_id(summary.id)
                .map_err(ApiError::Internal)?,
            kind: conversation_kind_to_proto(summary.kind),
            title: summary.title,
            owner_user_id: optional_user_id_to_proto(summary.owner_user_id, &self.public_id_codec)?,
            counterpart,
            member_count: summary.member_count,
            viewer_role: conversation_member_role_to_proto(summary.viewer_role),
            muted: summary.muted,
            unread_count: summary.unread_count,
            last_message,
            created_at: summary.created_at.timestamp(),
            updated_at: summary.updated_at.timestamp(),
        })
    }

    /// A sender the map does not hold is one the row did not carry: a system line,
    /// or text whose author has since been deleted. Both render without an author.
    fn social_message_to_proto(
        &self,
        message: SocialMessage,
        views: &UserViews,
    ) -> Result<synctv_proto::client::SocialMessage, ApiError> {
        let sender = message
            .sender
            .as_ref()
            .and_then(|user| views.get(&user.id))
            .cloned();

        Ok(synctv_proto::client::SocialMessage {
            id: self
                .public_id_codec
                .encode_social_message_id(message.id)
                .map_err(ApiError::Internal)?,
            conversation_id: self
                .public_id_codec
                .encode_conversation_id(message.conversation_id)
                .map_err(ApiError::Internal)?,
            sender,
            kind: social_message_kind_to_proto(message.kind),
            body: message.body,
            created_at: message.created_at.timestamp(),
            deleted: message.deleted,
        })
    }
}

fn conversation_member_to_proto(
    member: ConversationMember,
    user: synctv_proto::client::UserPublicView,
) -> synctv_proto::client::ConversationMember {
    synctv_proto::client::ConversationMember {
        user: Some(user),
        role: conversation_member_role_to_proto(member.role),
        joined_at: member.joined_at.timestamp(),
    }
}

fn conversation_kind_to_proto(kind: ConversationKind) -> i32 {
    match kind {
        ConversationKind::Direct => synctv_proto::client::ConversationKind::Direct as i32,
        ConversationKind::Group => synctv_proto::client::ConversationKind::Group as i32,
    }
}

fn conversation_member_role_to_proto(role: ConversationMemberRole) -> i32 {
    match role {
        ConversationMemberRole::Member => {
            synctv_proto::client::ConversationMemberRole::Member as i32
        }
        ConversationMemberRole::Admin => synctv_proto::client::ConversationMemberRole::Admin as i32,
        ConversationMemberRole::Owner => synctv_proto::client::ConversationMemberRole::Owner as i32,
    }
}

fn social_message_kind_to_proto(kind: SocialMessageKind) -> i32 {
    match kind {
        SocialMessageKind::Text => synctv_proto::client::SocialMessageKind::Text as i32,
        SocialMessageKind::System => synctv_proto::client::SocialMessageKind::System as i32,
    }
}
