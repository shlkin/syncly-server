//! The friend graph and its request inbox, plus the unread badge for 消息.
//!
//! Friendship is symmetric and needs consent, so it is two resources: a request
//! that one side sends and the other accepts, declines or lets the sender cancel,
//! and the friendship itself, which either side can drop. Removing a friend takes
//! the other account's public id in the path — a friends list has the id, and a
//! path makes the delete idempotent to retry.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    AcceptFriendRequestRequest, AcceptFriendRequestResponse, CancelFriendRequestRequest,
    CancelFriendRequestResponse, DeclineFriendRequestRequest, DeclineFriendRequestResponse,
    GetSocialUnreadSummaryResponse, ListFriendRequestsRequest, ListFriendRequestsResponse,
    ListFriendsRequest, ListFriendsResponse, RemoveFriendRequest, RemoveFriendResponse,
    SendFriendRequestRequest, SendFriendRequestResponse,
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendUserPath {
    pub user_id: String,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/user/friends", get(list_friends))
        .route(
            "/api/user/friends/{userId}",
            axum::routing::delete(remove_friend),
        )
        .route(
            "/api/user/friend-requests",
            get(list_friend_requests).post(send_friend_request),
        )
        .route(
            "/api/user/friend-requests/accept",
            post(accept_friend_request),
        )
        .route(
            "/api/user/friend-requests/decline",
            post(decline_friend_request),
        )
        .route(
            "/api/user/friend-requests/cancel",
            post(cancel_friend_request),
        )
        .route("/api/user/social/unread", get(get_social_unread_summary))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/friends",
        tag = "Friends",
        params(ListFriendsRequest),
        responses((status = 200, description = "Friends page", body = ListFriendsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_friends(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListFriendsRequest>,
) -> AppResult<Json<ListFriendsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_friends(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Drops the friendship in both directions — there is no one-sided variant.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/friends/{userId}",
        tag = "Friends",
        params(("userId" = String, Path, description = "Public user ID of the friend to remove")),
        responses((status = 200, description = "Friendship removed", body = RemoveFriendResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn remove_friend(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Path(path): Path<FriendUserPath>,
) -> AppResult<Json<RemoveFriendResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .remove_friend(
                        &auth.user_id(),
                        RemoveFriendRequest {
                            user_id: path.user_id,
                        },
                    )
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// One list, filtered by direction: incoming requests for the inbox badge,
/// outgoing for "已申请" state on a profile.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/friend-requests",
        tag = "Friends",
        params(ListFriendRequestsRequest),
        responses((status = 200, description = "Friend requests page", body = ListFriendRequestsResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_friend_requests(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListFriendRequestsRequest>,
) -> AppResult<Json<ListFriendRequestsResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_friend_requests(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/friend-requests",
        tag = "Friends",
        request_body = SendFriendRequestRequest,
        responses((status = 200, description = "Friend request sent", body = SendFriendRequestResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn send_friend_request(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<SendFriendRequestRequest>,
) -> AppResult<Json<SendFriendRequestResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.send_friend_request(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/friend-requests/accept",
        tag = "Friends",
        request_body = AcceptFriendRequestRequest,
        responses((status = 200, description = "Request accepted", body = AcceptFriendRequestResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn accept_friend_request(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<AcceptFriendRequestRequest>,
) -> AppResult<Json<AcceptFriendRequestResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.accept_friend_request(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/friend-requests/decline",
        tag = "Friends",
        request_body = DeclineFriendRequestRequest,
        responses((status = 200, description = "Request declined", body = DeclineFriendRequestResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn decline_friend_request(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<DeclineFriendRequestRequest>,
) -> AppResult<Json<DeclineFriendRequestResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .decline_friend_request(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Sender-side withdrawal, kept separate from decline so the audit trail says
/// who ended the request.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/friend-requests/cancel",
        tag = "Friends",
        request_body = CancelFriendRequestRequest,
        responses((status = 200, description = "Request cancelled", body = CancelFriendRequestResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn cancel_friend_request(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<CancelFriendRequestRequest>,
) -> AppResult<Json<CancelFriendRequestResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.cancel_friend_request(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// One cheap call for the 消息 tab badge: unread messages, unread threads and
/// pending friend requests, so the shell does not poll three endpoints.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/social/unread",
        tag = "Friends",
        responses((status = 200, description = "Unread counters", body = GetSocialUnreadSummaryResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_social_unread_summary(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<GetSocialUnreadSummaryResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.get_social_unread_summary(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
