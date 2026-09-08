//! 每日签到 and the points ledger it feeds.
//!
//! Reading and claiming share one path and differ by method: `GET` reports
//! whether today is already claimed and what a claim would pay, `POST` claims
//! it. Neither takes a body — the day is the server's, not the client's.

use axum::{extract::State, routing::get, Json, Router};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    ClaimCheckInResponse, GetCheckInStatusResponse, ListPointTransactionsRequest,
    ListPointTransactionsResponse,
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/user/check-in",
            get(get_check_in_status).post(claim_check_in),
        )
        .route(
            "/api/user/points/transactions",
            get(list_point_transactions),
        )
}

/// Reports today's claim state plus the configured award, so the card can show
/// "领取 5 积分" before the user commits to the claim.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/check-in",
        tag = "Check-In",
        responses((status = 200, description = "Check-in status for today", body = GetCheckInStatusResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_check_in_status(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<GetCheckInStatusResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.get_check_in_status(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Claims today. A second claim on the same server-side day is rejected by the
/// service rather than silently paying twice.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/check-in",
        tag = "Check-In",
        responses(
            (status = 200, description = "Points awarded", body = ClaimCheckInResponse),
            (status = 409, description = "Already checked in today", body = crate::openapi::GoogleRpcStatusSchema)
        ),
        security(("bearer_auth" = []))
    )
)]
pub async fn claim_check_in(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<ClaimCheckInResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.claim_check_in(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/points/transactions",
        tag = "Check-In",
        params(ListPointTransactionsRequest),
        responses((status = 200, description = "Points ledger page", body = ListPointTransactionsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_point_transactions(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListPointTransactionsRequest>,
) -> AppResult<Json<ListPointTransactionsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .list_point_transactions(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
