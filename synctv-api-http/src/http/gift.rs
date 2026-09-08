//! 礼物: the catalog, sending one, and the records a send leaves behind.
//!
//! Sending is one endpoint rather than two. Whether the gift is announced in a
//! room is a property of the gift, not of the route: a body carrying `roomId`
//! posts a banner in that room's chat, and a body without one is a quiet send
//! from a profile page.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    GetGiftStatsRequest, GetGiftStatsResponse, ListGiftRecordsRequest, ListGiftRecordsResponse,
    ListGiftsResponse, ListRoomGiftsRequest, ListRoomGiftsResponse, ListUserGiftShowcaseRequest,
    ListUserGiftShowcaseResponse, SendGiftRequest, SendGiftResponse,
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/gifts", get(list_gifts))
        .route("/api/gifts/send", post(send_gift))
        .route("/api/user/gifts/records", get(list_gift_records))
        .route("/api/user/gifts/stats", get(get_gift_stats))
        .route("/api/user/gifts/showcase", get(list_user_gift_showcase))
        .route("/api/rooms/gifts", get(list_room_gifts))
}

/// The send panel: the enabled catalog plus the caller's balance, so a client can
/// grey out what they cannot afford without a second call.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/gifts",
        tag = "Gifts",
        responses((status = 200, description = "Enabled catalog and the caller's balance", body = ListGiftsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_gifts(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<ListGiftsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_gifts(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Spends points to send a gift. The points are transferred: the recipient's
/// balance goes up by exactly what the sender paid, and both movements are
/// written to the ledger so an admin can read the pair as one event.
///
/// A repeated `requestId` returns the earlier send instead of charging twice.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/gifts/send",
        tag = "Gifts",
        request_body = SendGiftRequest,
        responses(
            (status = 200, description = "Gift sent, or the earlier send repeated", body = SendGiftResponse),
            (status = 409, description = "Not enough points, or the gift is no longer available", body = crate::openapi::GoogleRpcStatusSchema)
        ),
        security(("bearer_auth" = []))
    )
)]
pub async fn send_gift(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<SendGiftRequest>,
) -> AppResult<Json<SendGiftResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.send_gift(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// The caller's own history, one side at a time.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/gifts/records",
        tag = "Gifts",
        params(ListGiftRecordsRequest),
        responses((status = 200, description = "Gift records page", body = ListGiftRecordsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_gift_records(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListGiftRecordsRequest>,
) -> AppResult<Json<ListGiftRecordsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_gift_records(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Lifetime counters for a profile card. Omit `userId` for the caller's own.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/gifts/stats",
        tag = "Gifts",
        params(GetGiftStatsRequest),
        responses((status = 200, description = "Lifetime gift counters", body = GetGiftStatsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_gift_stats(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<GetGiftStatsRequest>,
) -> AppResult<Json<GetGiftStatsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.get_gift_stats(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// The room's recent sends, so a member who joins after a gift still sees it.
/// Members only — this is room content, not a public feed.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/rooms/gifts",
        tag = "Gifts",
        params(ListRoomGiftsRequest),
        responses((status = 200, description = "Recent gifts in the room, newest first", body = ListRoomGiftsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_room_gifts(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListRoomGiftsRequest>,
) -> AppResult<Json<ListRoomGiftsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_room_gifts(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// A profile's gift shelf: what an account was given, grouped by gift.
///
/// There is no sender anywhere in the response. The grouping happens in SQL, so
/// "who sent this" is not filtered out of the answer — it was never in it.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/gifts/showcase",
        tag = "Gifts",
        params(ListUserGiftShowcaseRequest),
        responses((status = 200, description = "Gifts received, grouped by gift", body = ListUserGiftShowcaseResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_user_gift_showcase(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListUserGiftShowcaseRequest>,
) -> AppResult<Json<ListUserGiftShowcaseResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .list_user_gift_showcase(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
