//! 礼物: the catalog a send panel shows, sending one, and reading the records.
//!
//! A send is routed by whether it carries a room. A room send goes through
//! [`RoomService`](synctv_core::service::RoomService) so it is gated on chat
//! permission and announced in the room; a send with no room is only a debit and
//! a record, which is what a profile page wants.

use synctv_core::models::{
    Gift, GiftRecordDirection, GiftRecordView, SendGift, UserId, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::{optional_room_id_to_proto, optional_user_id_to_proto, total_to_proto};
use super::ClientApiImpl;
use crate::impls::ApiError;

/// Room feed size when the client does not ask for one. The service caps the
/// upper end; this only decides the default.
const DEFAULT_ROOM_GIFT_FEED_LIMIT: i64 = 20;

impl ClientApiImpl {
    /// The send panel: what may be sent, and what the caller can afford.
    pub async fn list_gifts(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::ListGiftsResponse, ApiError> {
        let gifts = self
            .user_service
            .list_gifts(true)
            .await
            .map_err(ApiError::from)?;
        let balance = self
            .user_service
            .point_balance(user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ListGiftsResponse {
            gifts: gifts
                .into_iter()
                .map(|gift| self.gift_to_proto(gift))
                .collect::<Result<Vec<_>, _>>()?,
            balance,
        })
    }

    /// Sends a gift. `room_id` decides which half of the feature runs: with a
    /// room the send is announced there, without one it is silent.
    ///
    /// A repeated `request_id` is answered with the earlier send rather than
    /// charged again, so a client that retries a lost response converges.
    pub async fn send_gift(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::SendGiftRequest,
    ) -> Result<synctv_proto::client::SendGiftResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let recipient_id =
            crate::impls::proto_validated_user_id(req.recipient_user_id, &self.public_id_codec)?;
        let gift_id = self
            .public_id_codec
            .decode_gift_id(&req.gift_id)
            .map_err(ApiError::InvalidInput)?;
        let room_id = if req.room_id.is_empty() {
            None
        } else {
            Some(crate::impls::proto_validated_room_id(
                &req.room_id,
                &self.public_id_codec,
            )?)
        };

        let request = SendGift {
            sender_id: *user_id,
            recipient_id,
            room_id,
            gift_id,
            quantity: req.quantity,
            message: req.message,
            request_id: Some(req.request_id).filter(|id| !id.is_empty()),
        };
        let result = if room_id.is_some() {
            self.room_service.send_room_gift(&request).await
        } else {
            self.room_service.send_direct_gift(&request).await
        }
        .map_err(ApiError::from)?;

        let (sender_username, recipient_username) = self
            .gift_record_usernames(result.record.sender_id, result.record.recipient_id)
            .await?;

        Ok(synctv_proto::client::SendGiftResponse {
            record: Some(self.gift_record_to_proto(&GiftRecordView {
                record: result.record,
                sender_username,
                recipient_username,
            })?),
            balance: result.balance,
            already_sent: result.already_sent,
        })
    }

    /// One side of the caller's own gift history.
    pub async fn list_gift_records(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListGiftRecordsRequest,
    ) -> Result<synctv_proto::client::ListGiftRecordsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let direction = match synctv_proto::client::GiftRecordDirection::try_from(req.direction) {
            Ok(synctv_proto::client::GiftRecordDirection::Received) => {
                GiftRecordDirection::Received
            }
            // Unspecified reads what the caller has sent, which is the tab a
            // client opens on.
            _ => GiftRecordDirection::Sent,
        };
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );

        let (records, total) = self
            .user_service
            .list_gift_records(user_id, direction, pagination)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ListGiftRecordsResponse {
            records: records
                .iter()
                .map(|view| self.gift_record_to_proto(view))
                .collect::<Result<Vec<_>, _>>()?,
            total: total_to_proto(total, "gift record")?,
        })
    }

    /// The room's recent sends, so a member who joined after a gift still sees
    /// it. Membership is required: this is room content, not a public feed.
    pub async fn list_room_gifts(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListRoomGiftsRequest,
    ) -> Result<synctv_proto::client::ListRoomGiftsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let room_id = crate::impls::proto_validated_room_id(&req.room_id, &self.public_id_codec)?;
        self.room_service
            .check_membership(&room_id, user_id)
            .await
            .map_err(ApiError::from)?;

        let limit = if req.limit > 0 {
            i64::from(req.limit)
        } else {
            DEFAULT_ROOM_GIFT_FEED_LIMIT
        };
        let records = self
            .user_service
            .list_room_gift_records(&room_id, limit)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ListRoomGiftsResponse {
            records: records
                .iter()
                .map(|view| self.gift_record_to_proto(view))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Lifetime counters for a profile card. Reads another account's counters
    /// when asked: they are already visible from the records either side holds.
    pub async fn get_gift_stats(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::GetGiftStatsRequest,
    ) -> Result<synctv_proto::client::GetGiftStatsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let target = if req.user_id.is_empty() {
            *user_id
        } else {
            crate::impls::proto_validated_user_id(&req.user_id, &self.public_id_codec)?
        };
        let stats = self
            .user_service
            .gift_stats(&target)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::GetGiftStatsResponse {
            sent_count: stats.sent_count,
            sent_points: stats.sent_points,
            received_count: stats.received_count,
            received_points: stats.received_points,
        })
    }

    /// Display names for a record the caller just created. A send response
    /// carries them so the client can render the row without a second call.
    async fn gift_record_usernames(
        &self,
        sender_id: UserId,
        recipient_id: Option<UserId>,
    ) -> Result<(String, Option<String>), ApiError> {
        let mut ids = vec![sender_id];
        if let Some(recipient_id) = recipient_id {
            ids.push(recipient_id);
        }
        let names = self
            .user_service
            .get_usernames(&ids)
            .await
            .map_err(ApiError::from)?;

        Ok((
            names.get(&sender_id).cloned().unwrap_or_default(),
            recipient_id.and_then(|id| names.get(&id).cloned()),
        ))
    }

    pub(crate) fn gift_to_proto(&self, gift: Gift) -> Result<synctv_proto::client::Gift, ApiError> {
        gift_to_proto(gift, &self.public_id_codec)
    }

    fn gift_record_to_proto(
        &self,
        view: &GiftRecordView,
    ) -> Result<synctv_proto::client::GiftRecord, ApiError> {
        let record = &view.record;
        Ok(synctv_proto::client::GiftRecord {
            id: self
                .public_id_codec
                .encode_gift_record_id(record.id)
                .map_err(ApiError::Internal)?,
            sender_user_id: self
                .public_id_codec
                .encode_user_id(record.sender_id)
                .map_err(ApiError::Internal)?,
            sender_username: view.sender_username.clone(),
            recipient_user_id: optional_user_id_to_proto(
                record.recipient_id,
                &self.public_id_codec,
            )?,
            recipient_username: view.recipient_username.clone().unwrap_or_default(),
            room_id: optional_room_id_to_proto(record.room_id, &self.public_id_codec)?,
            gift_key: record.gift_key.clone(),
            gift_name: record.gift_name.clone(),
            gift_icon: record.gift_icon.clone(),
            unit_price_points: record.unit_price_points,
            quantity: record.quantity,
            total_points: record.total_points,
            message: record.message.clone(),
            created_at: record.created_at.timestamp(),
        })
    }

    /// A profile's gift shelf: what the account was given, grouped by gift.
    ///
    /// No sender reaches the response — the aggregation happens in SQL and
    /// `UserGiftShowcaseEntry` has no field to carry one. That is the point:
    /// the shelf says what someone was given, never by whom.
    pub async fn list_user_gift_showcase(
        &self,
        viewer_user_id: &UserId,
        req: synctv_proto::client::ListUserGiftShowcaseRequest,
    ) -> Result<synctv_proto::client::ListUserGiftShowcaseResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let subject_id = if req.user_id.is_empty() {
            *viewer_user_id
        } else {
            crate::impls::proto_validated_user_id(req.user_id, &self.public_id_codec)?
        };

        let showcase = self
            .user_service
            .gift_showcase(&subject_id, req.limit)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ListUserGiftShowcaseResponse {
            entries: showcase
                .entries
                .into_iter()
                .map(|entry| synctv_proto::client::UserGiftShowcaseEntry {
                    gift_key: entry.gift_key,
                    gift_name: entry.gift_name,
                    gift_icon: entry.gift_icon,
                    quantity: entry.quantity,
                    total_points: entry.total_points,
                    last_received_at: entry.last_received_at.timestamp(),
                })
                .collect(),
            total_quantity: showcase.total_quantity,
            total_points: showcase.total_points,
        })
    }
}

/// One catalog entry on the wire, as both the send panel and the admin console
/// read it — the admin form edits the same shape the panel renders.
pub(crate) fn gift_to_proto(
    gift: Gift,
    public_id_codec: &synctv_adapter::PublicIdCodec,
) -> Result<synctv_proto::client::Gift, ApiError> {
    Ok(synctv_proto::client::Gift {
        id: public_id_codec
            .encode_gift_id(gift.id)
            .map_err(ApiError::Internal)?,
        key: gift.key,
        name: gift.name,
        description: gift.description,
        icon: gift.icon,
        price_points: gift.price_points,
        sort_order: gift.sort_order,
        is_enabled: gift.is_enabled,
        created_at: gift.created_at.timestamp(),
        updated_at: gift.updated_at.timestamp(),
        updated_by_user_id: optional_user_id_to_proto(gift.updated_by, public_id_codec)?,
    })
}
