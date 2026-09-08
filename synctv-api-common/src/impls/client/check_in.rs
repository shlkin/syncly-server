//! Daily check-in and the points it awards.
//!
//! The rules are server-wide and admin-owned, so this module only reads them;
//! `impls::admin::check_in` is what writes them, through the same conversions.

use synctv_core::models::{
    CheckInAwardMode, CheckInConfig, PointTransaction, UserId, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};

use super::convert::{
    date_to_proto, optional_date_to_proto, optional_user_id_to_proto, total_to_proto,
};
use super::ClientApiImpl;
use crate::impls::ApiError;

impl ClientApiImpl {
    /// Everything the check-in card renders before the user taps it.
    pub async fn get_check_in_status(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::GetCheckInStatusResponse, ApiError> {
        let status = self
            .user_service
            .check_in_status(user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::GetCheckInStatusResponse {
            config: Some(check_in_config_to_proto(
                &status.config,
                &self.public_id_codec,
            )?),
            today: date_to_proto(status.today),
            checked_in_today: status.checked_in_today,
            streak: status.streak,
            last_check_in_date: optional_date_to_proto(status.last_check_in_date),
            balance: status.balance,
            total_earned: status.total_earned,
        })
    }

    /// Claims today's check-in. Claiming twice in one day is not an error: the
    /// response then describes the earlier claim and awards nothing, so a client
    /// that retries a lost response converges on the same answer.
    pub async fn claim_check_in(
        &self,
        user_id: &UserId,
    ) -> Result<synctv_proto::client::ClaimCheckInResponse, ApiError> {
        let result = self
            .user_service
            .claim_check_in(user_id)
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ClaimCheckInResponse {
            check_in_date: date_to_proto(result.check_in_date),
            points_awarded: result.points_awarded,
            streak: result.streak,
            balance: result.balance,
            total_earned: result.total_earned,
            already_checked_in: result.already_checked_in,
        })
    }

    /// The point statement, newest movement first.
    pub async fn list_point_transactions(
        &self,
        user_id: &UserId,
        req: synctv_proto::client::ListPointTransactionsRequest,
    ) -> Result<synctv_proto::client::ListPointTransactionsResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
        );
        let (page, total) = self
            .user_service
            .list_point_transactions(user_id, pagination)
            .await
            .map_err(ApiError::from)?;
        let transactions = page
            .into_iter()
            .map(|transaction| self.point_transaction_to_proto(transaction))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(synctv_proto::client::ListPointTransactionsResponse {
            transactions,
            total: total_to_proto(total, "point transaction")?,
        })
    }

    fn point_transaction_to_proto(
        &self,
        transaction: PointTransaction,
    ) -> Result<synctv_proto::client::PointTransaction, ApiError> {
        Ok(synctv_proto::client::PointTransaction {
            id: self
                .public_id_codec
                .encode_point_transaction_id(transaction.id)
                .map_err(ApiError::Internal)?,
            delta: transaction.delta,
            balance_after: transaction.balance_after,
            reason: transaction.reason,
            description: transaction.description,
            created_at: transaction.created_at.timestamp(),
        })
    }
}

/// The award rules, as both the client card and the admin form read them.
pub(crate) fn check_in_config_to_proto(
    config: &CheckInConfig,
    public_id_codec: &synctv_adapter::PublicIdCodec,
) -> Result<synctv_proto::client::CheckInConfig, ApiError> {
    Ok(synctv_proto::client::CheckInConfig {
        enabled: config.enabled,
        award_mode: check_in_award_mode_to_proto(config.award_mode),
        fixed_points: config.fixed_points,
        random_min_points: config.random_min_points,
        random_max_points: config.random_max_points,
        day_boundary_offset_minutes: config.day_boundary_offset_minutes,
        updated_at: config.updated_at.timestamp(),
        updated_by_user_id: optional_user_id_to_proto(config.updated_by, public_id_codec)?,
    })
}

fn check_in_award_mode_to_proto(mode: CheckInAwardMode) -> i32 {
    match mode {
        CheckInAwardMode::Fixed => synctv_proto::client::CheckInAwardMode::Fixed as i32,
        CheckInAwardMode::Random => synctv_proto::client::CheckInAwardMode::Random as i32,
    }
}

/// Only a named mode may be stored. UNSPECIFIED reaching here means the client
/// sent the field and left it blank, which is a different mistake from omitting
/// it — omitting it is how an admin says "leave the mode alone".
pub(crate) fn check_in_award_mode_from_proto(value: i32) -> Result<CheckInAwardMode, ApiError> {
    match synctv_proto::client::CheckInAwardMode::try_from(value) {
        Ok(synctv_proto::client::CheckInAwardMode::Fixed) => Ok(CheckInAwardMode::Fixed),
        Ok(synctv_proto::client::CheckInAwardMode::Random) => Ok(CheckInAwardMode::Random),
        _ => Err(ApiError::InvalidInput(
            "award_mode must be fixed or random".to_string(),
        )),
    }
}
