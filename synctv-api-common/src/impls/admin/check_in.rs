//! The server-wide 每日签到 rules, as the admin console reads and writes them.
//!
//! Only the rules live here. The claim itself is a client call, and the two share
//! one wire shape through `impls::client::check_in`, so the form an admin edits is
//! the same message the check-in card renders.

use synctv_core::models::CheckInConfigUpdate;

use super::{AdminApiImpl, ApiError};
use crate::impls::client::{check_in_award_mode_from_proto, check_in_config_to_proto};

impl AdminApiImpl {
    pub async fn get_check_in_config(
        &self,
        req: synctv_proto::admin::GetCheckInConfigRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::GetCheckInConfigResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let config = self.user_service.check_in_config().await?;

        Ok(synctv_proto::admin::GetCheckInConfigResponse {
            config: Some(check_in_config_to_proto(&config, &self.public_id_codec)?),
        })
    }

    /// A partial update: an omitted field keeps its stored value. The service
    /// validates the random range against what is stored, so raising the maximum
    /// alone is a complete request.
    pub async fn update_check_in_config(
        &self,
        req: synctv_proto::admin::UpdateCheckInConfigRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::UpdateCheckInConfigResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let update = CheckInConfigUpdate {
            enabled: req.enabled,
            award_mode: req
                .award_mode
                .map(check_in_award_mode_from_proto)
                .transpose()?,
            fixed_points: req.fixed_points,
            random_min_points: req.random_min_points,
            random_max_points: req.random_max_points,
            day_boundary_offset_minutes: req.day_boundary_offset_minutes,
        };
        let config = self
            .user_service
            .update_check_in_config(&update, admin_user_id)
            .await?;

        Ok(synctv_proto::admin::UpdateCheckInConfigResponse {
            config: Some(check_in_config_to_proto(&config, &self.public_id_codec)?),
        })
    }
}
