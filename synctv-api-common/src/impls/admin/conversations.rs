//! Unscoped reads over the social conversations, for moderation.
//!
//! Every client-facing conversation call resolves the caller's membership row
//! first and answers "not found" to anyone else, because whether a conversation
//! exists is itself private. Moderation needs the opposite view, so the gate
//! here is `require_admin_actor` and the service methods these call take no
//! viewer at all — there is no membership to imply and none to accidentally
//! satisfy.
//!
//! Rows are flat, `user_id` beside `username`, like the gift-flow listings next
//! door: resolving an avatar costs a file-reference load per account, and this
//! table is read to find out who said what.

use synctv_core::models::ConversationKind;

use super::{i64_to_i32_api, AdminApiImpl, ApiError};

impl AdminApiImpl {
    /// Every conversation on the server, most recently active first.
    pub async fn list_conversations(
        &self,
        req: synctv_proto::admin::ListConversationsRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListConversationsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            synctv_core::models::DEFAULT_PAGE_SIZE,
            synctv_core::models::MAX_PAGE_SIZE,
        );
        let search = req.search.trim();
        let search = (!search.is_empty()).then_some(search);

        let (conversations, total) = self
            .user_service
            .admin_list_conversations(conversation_kind_from_proto(req.kind), search, pagination)
            .await?;

        Ok(synctv_proto::admin::ListConversationsResponse {
            conversations: conversations
                .into_iter()
                .map(|overview| {
                    Ok(synctv_proto::admin::AdminConversation {
                        id: self
                            .public_id_codec
                            .encode_conversation_id(overview.id)
                            .map_err(ApiError::Internal)?,
                        kind: conversation_kind_to_proto(overview.kind),
                        title: overview.title,
                        owner_user_id: overview
                            .owner
                            .as_ref()
                            .map(|owner| {
                                self.public_id_codec
                                    .encode_user_id(owner.id)
                                    .map_err(ApiError::Internal)
                            })
                            .transpose()?
                            .unwrap_or_default(),
                        owner_username: overview
                            .owner
                            .map(|owner| owner.username)
                            .unwrap_or_default(),
                        member_count: overview.member_count,
                        message_count: overview.message_count,
                        last_message_at: overview
                            .last_message_at
                            .map(|at| at.timestamp())
                            .unwrap_or_default(),
                        created_at: overview.created_at.timestamp(),
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
            total: i64_to_i32_api(total, "conversation count")?,
        })
    }

    /// Who is in one conversation, owners and admins first.
    pub async fn list_conversation_members(
        &self,
        req: synctv_proto::admin::ListConversationMembersRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListConversationMembersResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let conversation_id = self
            .public_id_codec
            .decode_conversation_id(&req.conversation_id)
            .map_err(ApiError::InvalidInput)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            synctv_core::models::MAX_PAGE_SIZE,
            synctv_core::models::MAX_PAGE_SIZE,
        );

        let (members, total) = self
            .user_service
            .admin_list_conversation_members(conversation_id, pagination)
            .await?;

        Ok(synctv_proto::admin::ListConversationMembersResponse {
            members: members
                .into_iter()
                .map(|member| {
                    Ok(synctv_proto::admin::AdminConversationMember {
                        user_id: self
                            .public_id_codec
                            .encode_user_id(member.user.id)
                            .map_err(ApiError::Internal)?,
                        username: member.user.username,
                        role: conversation_member_role_to_proto(member.role),
                        joined_at: member.joined_at.timestamp(),
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
            total: i64_to_i32_api(total, "conversation member count")?,
        })
    }

    /// A page of one conversation's history, newest first.
    pub async fn list_conversation_messages(
        &self,
        req: synctv_proto::admin::ListConversationMessagesRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListConversationMessagesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let conversation_id = self
            .public_id_codec
            .decode_conversation_id(&req.conversation_id)
            .map_err(ApiError::InvalidInput)?;
        let before_message_id = if req.before_message_id.is_empty() {
            None
        } else {
            Some(
                self.public_id_codec
                    .decode_social_message_id(&req.before_message_id)
                    .map_err(ApiError::InvalidInput)?,
            )
        };
        let limit = (req.limit > 0).then(|| req.limit.unsigned_abs());

        let messages = self
            .user_service
            .admin_list_conversation_messages(conversation_id, before_message_id, limit)
            .await?;

        Ok(synctv_proto::admin::ListConversationMessagesResponse {
            messages: messages
                .into_iter()
                .map(|message| {
                    Ok(synctv_proto::admin::AdminConversationMessage {
                        id: self
                            .public_id_codec
                            .encode_social_message_id(message.id)
                            .map_err(ApiError::Internal)?,
                        sender_user_id: message
                            .sender
                            .as_ref()
                            .map(|sender| {
                                self.public_id_codec
                                    .encode_user_id(sender.id)
                                    .map_err(ApiError::Internal)
                            })
                            .transpose()?
                            .unwrap_or_default(),
                        sender_username: message
                            .sender
                            .map(|sender| sender.username)
                            .unwrap_or_default(),
                        kind: social_message_kind_to_proto(message.kind),
                        body: message.body,
                        created_at: message.created_at.timestamp(),
                        deleted: message.deleted,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
        })
    }
}

/// `UNSPECIFIED` is how the console says "both kinds", so an unrecognised value
/// widens the listing rather than narrowing it to nothing.
fn conversation_kind_from_proto(kind: i32) -> Option<ConversationKind> {
    match synctv_proto::client::ConversationKind::try_from(kind) {
        Ok(synctv_proto::client::ConversationKind::Direct) => Some(ConversationKind::Direct),
        Ok(synctv_proto::client::ConversationKind::Group) => Some(ConversationKind::Group),
        _ => None,
    }
}

fn conversation_kind_to_proto(kind: ConversationKind) -> i32 {
    match kind {
        ConversationKind::Direct => synctv_proto::client::ConversationKind::Direct as i32,
        ConversationKind::Group => synctv_proto::client::ConversationKind::Group as i32,
    }
}

fn conversation_member_role_to_proto(role: synctv_core::models::ConversationMemberRole) -> i32 {
    match role {
        synctv_core::models::ConversationMemberRole::Member => {
            synctv_proto::client::ConversationMemberRole::Member as i32
        }
        synctv_core::models::ConversationMemberRole::Admin => {
            synctv_proto::client::ConversationMemberRole::Admin as i32
        }
        synctv_core::models::ConversationMemberRole::Owner => {
            synctv_proto::client::ConversationMemberRole::Owner as i32
        }
    }
}

fn social_message_kind_to_proto(kind: synctv_core::models::SocialMessageKind) -> i32 {
    match kind {
        synctv_core::models::SocialMessageKind::Text => {
            synctv_proto::client::SocialMessageKind::Text as i32
        }
        synctv_core::models::SocialMessageKind::System => {
            synctv_proto::client::SocialMessageKind::System as i32
        }
    }
}
