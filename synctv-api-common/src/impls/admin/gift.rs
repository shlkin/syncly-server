//! The 礼物 catalog, as the admin console curates it.
//!
//! Only the catalog lives here: sending is a client call. Both sides share one
//! wire shape through `impls::client::gift_to_proto`, so the row an admin edits
//! is the row a send panel renders.
//!
//! `key` is create-only on purpose. Records snapshot the key together with the
//! name, icon and unit price at send time, so a rename would leave the history
//! pointing at a key that no longer exists.

use synctv_core::models::{GiftUpdate, NewGift};

use super::{i64_to_i32_api, AdminApiImpl, ApiError};
use crate::impls::client::gift_to_proto;

impl AdminApiImpl {
    /// The whole catalog. Disabled entries are included by default so they stay
    /// editable — disabling is how an entry is retired once it has been sent.
    pub async fn list_gift_catalog(
        &self,
        req: synctv_proto::admin::ListGiftCatalogRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListGiftCatalogResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let gifts = self.user_service.list_gifts(req.enabled_only).await?;

        Ok(synctv_proto::admin::ListGiftCatalogResponse {
            gifts: gifts
                .into_iter()
                .map(|gift| gift_to_proto(gift, &self.public_id_codec))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Adds an entry. The key is normalized and uniqueness-checked in the
    /// service, so a duplicate comes back as a conflict rather than a database
    /// error.
    pub async fn create_gift(
        &self,
        req: synctv_proto::admin::CreateGiftRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::CreateGiftResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let gift = self
            .user_service
            .create_gift(
                &NewGift {
                    key: req.key,
                    name: req.name,
                    description: req.description,
                    icon: req.icon,
                    price_points: req.price_points,
                    sort_order: req.sort_order,
                    is_enabled: req.is_enabled,
                },
                admin_user_id,
            )
            .await?;

        Ok(synctv_proto::admin::CreateGiftResponse {
            gift: Some(gift_to_proto(gift, &self.public_id_codec)?),
        })
    }

    /// A partial update: an omitted field keeps its stored value, and an update
    /// with no fields set reads the entry back rather than bumping `updated_at`.
    pub async fn update_gift(
        &self,
        req: synctv_proto::admin::UpdateGiftRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::UpdateGiftResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let gift_id = self
            .public_id_codec
            .decode_gift_id(&req.gift_id)
            .map_err(ApiError::InvalidInput)?;
        let gift = self
            .user_service
            .update_gift(
                gift_id,
                &GiftUpdate {
                    name: req.name,
                    description: req.description,
                    icon: req.icon,
                    price_points: req.price_points,
                    sort_order: req.sort_order,
                    is_enabled: req.is_enabled,
                },
                admin_user_id,
            )
            .await?;

        Ok(synctv_proto::admin::UpdateGiftResponse {
            gift: Some(gift_to_proto(gift, &self.public_id_codec)?),
        })
    }

    /// Removes an entry that has never been sent. Once it has, the delete is
    /// refused: the records that snapshot it stay readable, and disabling the
    /// entry is how it leaves the send panel.
    pub async fn delete_gift(
        &self,
        req: synctv_proto::admin::DeleteGiftRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::DeleteGiftResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let gift_id = self
            .public_id_codec
            .decode_gift_id(&req.gift_id)
            .map_err(ApiError::InvalidInput)?;
        self.user_service.delete_gift(gift_id).await?;

        Ok(synctv_proto::admin::DeleteGiftResponse {})
    }

    /// Every gift on the server, newest first: who sent it, who received it, and
    /// what it cost. The client-facing listings are scoped to one account; this
    /// is the view that shows the flow between them.
    pub async fn list_gift_records(
        &self,
        req: synctv_proto::admin::ListGiftRecordsRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListGiftRecordsResponse, ApiError> {
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

        let (records, total) = self
            .user_service
            .list_all_gift_records(search, pagination)
            .await?;

        Ok(synctv_proto::admin::ListGiftRecordsResponse {
            records: records
                .into_iter()
                .map(|view| {
                    Ok(synctv_proto::admin::GiftRecordEntry {
                        id: self
                            .public_id_codec
                            .encode_gift_record_id(view.record.id)
                            .map_err(ApiError::Internal)?,
                        sender_user_id: self
                            .public_id_codec
                            .encode_user_id(view.record.sender_id)
                            .map_err(ApiError::Internal)?,
                        sender_username: view.sender_username,
                        recipient_user_id: view
                            .record
                            .recipient_id
                            .map(|id| {
                                self.public_id_codec
                                    .encode_user_id(id)
                                    .map_err(ApiError::Internal)
                            })
                            .transpose()?
                            .unwrap_or_default(),
                        recipient_username: view.recipient_username.unwrap_or_default(),
                        room_id: view
                            .record
                            .room_id
                            .map(|id| {
                                self.public_id_codec
                                    .encode_room_id(id)
                                    .map_err(ApiError::Internal)
                            })
                            .transpose()?
                            .unwrap_or_default(),
                        gift_key: view.record.gift_key,
                        gift_name: view.record.gift_name,
                        gift_icon: view.record.gift_icon,
                        unit_price_points: view.record.unit_price_points,
                        quantity: view.record.quantity,
                        total_points: view.record.total_points,
                        message: view.record.message,
                        created_at: view.record.created_at.timestamp(),
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
            total: i64_to_i32_api(total, "gift record count")?,
        })
    }

    /// The points ledger across every account: each credit and debit in order,
    /// which is what makes a gift readable as a transfer rather than two
    /// unrelated movements.
    pub async fn list_point_transactions(
        &self,
        req: synctv_proto::admin::ListPointTransactionsRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListPointTransactionsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            synctv_core::models::DEFAULT_PAGE_SIZE,
            synctv_core::models::MAX_PAGE_SIZE,
        );
        let user_id = if req.user_id.is_empty() {
            None
        } else {
            Some(crate::impls::proto_validated_user_id(
                req.user_id,
                &self.public_id_codec,
            )?)
        };
        let reason = req.reason.trim();
        let reason = (!reason.is_empty()).then_some(reason);

        let (transactions, total) = self
            .user_service
            .list_all_point_transactions(user_id.as_ref(), reason, pagination)
            .await?;

        Ok(synctv_proto::admin::ListPointTransactionsResponse {
            transactions: transactions
                .into_iter()
                .map(|entry| {
                    Ok(synctv_proto::admin::PointTransactionEntry {
                        id: self
                            .public_id_codec
                            .encode_point_transaction_id(entry.id)
                            .map_err(ApiError::Internal)?,
                        user_id: self
                            .public_id_codec
                            .encode_user_id(entry.user_id)
                            .map_err(ApiError::Internal)?,
                        username: entry.username,
                        delta: entry.delta,
                        balance_after: entry.balance_after,
                        reason: entry.reason,
                        description: entry.description,
                        created_at: entry.created_at.timestamp(),
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?,
            total: i64_to_i32_api(total, "point transaction count")?,
        })
    }
}
