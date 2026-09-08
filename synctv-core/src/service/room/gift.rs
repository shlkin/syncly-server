//! Sending a gift inside a room, and announcing it there.
//!
//! This lives on [`RoomService`] rather than `UserService` because the announce
//! half needs the chat repository and the realtime outbox, which only the room
//! service holds. The debit and the announcement are two transactions: the gift
//! record is the truth, the chat message is a notification, so a failure to
//! announce must not undo a send that already happened.

use crate::{
    models::{
        ChatGiftMetadata, ChatMessage, ChatMessageType, ChatMetadata, RealtimeEvent, RoomId,
        RoomPermission, SendGift, SendGiftResult, UserId,
    },
    repository::realtime_outbox::NewRealtimeOutboxEvent,
    service::RoomService,
    Error, Result,
};

impl RoomService {
    /// Sends a gift from one room member to another and posts a system message
    /// so the room sees it.
    ///
    /// Requires [`RoomPermission::SendChatMessages`]: the send produces a chat
    /// message, so someone who may not speak in the room may not broadcast a
    /// gift either. The recipient must be a member for the same reason — a room
    /// is not a way to reach strangers.
    pub async fn send_room_gift(&self, request: &SendGift) -> Result<SendGiftResult> {
        let Some(room_id) = request.room_id else {
            return Err(Error::InvalidInput("a room gift needs a room".to_string()));
        };

        self.check_permission(
            &room_id,
            &request.sender_id,
            RoomPermission::SendChatMessages,
        )
        .await?;

        if self
            .get_member(&room_id, &request.recipient_id)
            .await?
            .is_none()
        {
            return Err(Error::NotFound("recipient is not in this room".to_string()));
        }

        let result = self.user_service.send_gift(request).await?;

        // A repeat of an earlier send is announced once, when it first happened.
        if !result.already_sent {
            self.announce_gift(room_id, &result).await?;
        }

        Ok(result)
    }

    /// Posts the system chat message for a send.
    async fn announce_gift(&self, room_id: RoomId, result: &SendGiftResult) -> Result<()> {
        let record = &result.record;
        let recipient_user_id = record
            .recipient_id
            .ok_or_else(|| Error::Internal("gift record lost its recipient".to_string()))?;

        let (sender_username, recipient_username) = self
            .gift_usernames(record.sender_id, recipient_user_id)
            .await?;

        let content = format!(
            "{sender_username} 送给 {recipient_username} {} × {}",
            record.gift_name, record.quantity
        );
        let mut message = ChatMessage::new(room_id, record.sender_id, content);
        message.message_type = ChatMessageType::SystemGift;
        message.metadata = Some(ChatMetadata::Gift(ChatGiftMetadata {
            record_id: record.id,
            sender_user_id: record.sender_id,
            sender_username,
            recipient_user_id,
            recipient_username,
            gift_key: record.gift_key.clone(),
            gift_name: record.gift_name.clone(),
            gift_icon: record.gift_icon.clone(),
            quantity: record.quantity,
            total_points: record.total_points,
            message: record.message.clone(),
        }));

        let occurred_at = self.clock.now();
        message.created_at = occurred_at;
        let event_id = synctv_common::snanoid!(16);

        let mut tx = self.pool().begin().await?;
        let logged = self
            .chat_repo
            .insert_message_event_in_tx(
                &mut tx,
                crate::repository::chat::InsertChatMessageEvent {
                    message: &message,
                    attachments: &[],
                    mentions: &[],
                    actor_user_id: record.sender_id,
                    event_id: &event_id,
                    occurred_at,
                },
            )
            .await?;

        let event = RealtimeEvent::ChatMessageEvent {
            event_id: logged.event.event_id.clone(),
            room_id,
            actor_user_id: record.sender_id,
            event: logged.event,
            timestamp: occurred_at,
        };
        self.insert_realtime_outbox_tx(
            &mut tx,
            Some(&NewRealtimeOutboxEvent {
                id: event.event_id().to_string(),
                enqueue_outbox: true,
                aggregate_type: "room".to_string(),
                aggregate_id: room_id.to_string(),
                event_type: event.event_type().to_string(),
                event_version: 1,
                aggregate_version: None,
                payload: event,
            }),
        )
        .await?;
        tx.commit().await?;

        Ok(())
    }

    /// Both display names for the banner, resolved in one lookup.
    ///
    /// A missing name falls back to the encoded id rather than failing the
    /// announcement: the send has already been paid for at this point.
    async fn gift_usernames(
        &self,
        sender_id: UserId,
        recipient_id: UserId,
    ) -> Result<(String, String)> {
        let names = self
            .user_service
            .get_usernames(&[sender_id, recipient_id])
            .await?;
        let sender = names
            .get(&sender_id)
            .cloned()
            .unwrap_or_else(|| sender_id.to_string());
        let recipient = names
            .get(&recipient_id)
            .cloned()
            .unwrap_or_else(|| recipient_id.to_string());
        Ok((sender, recipient))
    }
}

/// Sends outside a room: no audience, so nothing is announced.
impl RoomService {
    /// Convenience for a profile-page send, which is only a debit and a record.
    pub async fn send_direct_gift(&self, request: &SendGift) -> Result<SendGiftResult> {
        if request.room_id.is_some() {
            return Err(Error::InvalidInput(
                "a direct gift must not carry a room".to_string(),
            ));
        }
        self.user_service.send_gift(request).await
    }
}
