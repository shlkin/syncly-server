//! 情侣空间: the pairing handshake, the space itself, and the two lists it owns.
//!
//! One account holds at most one space, so `GET /api/user/couple` needs no id —
//! it returns the active space, a pending invite, or nothing at all, which is the
//! card's empty state. The favorites and shared-watch lists do carry a `spaceId`:
//! the caller may be either side of the pair, and the id keeps a stale client
//! from writing into a space it was already unbound from.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    AcceptCoupleInviteRequest, AcceptCoupleInviteResponse, AddCoupleFavoriteRequest,
    AddCoupleFavoriteResponse, DismissCoupleInviteRequest, DismissCoupleInviteResponse,
    GetCoupleSpaceResponse, InviteCouplePartnerRequest, InviteCouplePartnerResponse,
    ListCoupleFavoritesRequest, ListCoupleFavoritesResponse, ListCoupleSharedWatchEntriesRequest,
    ListCoupleSharedWatchEntriesResponse, RemoveCoupleFavoriteRequest,
    RemoveCoupleFavoriteResponse, UnbindCoupleSpaceRequest, UnbindCoupleSpaceResponse,
    UpdateCoupleSpaceRequest, UpdateCoupleSpaceResponse,
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/user/couple",
            get(get_couple_space).patch(update_couple_space),
        )
        .route("/api/user/couple/invites", post(invite_couple_partner))
        .route(
            "/api/user/couple/invites/accept",
            post(accept_couple_invite),
        )
        .route(
            "/api/user/couple/invites/dismiss",
            post(dismiss_couple_invite),
        )
        .route("/api/user/couple/unbind", post(unbind_couple_space))
        .route(
            "/api/user/couple/favorites",
            get(list_couple_favorites)
                .post(add_couple_favorite)
                .delete(remove_couple_favorite),
        )
        .route(
            "/api/user/couple/shared-watch-entries",
            get(list_couple_shared_watch_entries),
        )
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/couple",
        tag = "Couple Space",
        responses((status = 200, description = "Active space, pending invite, or empty", body = GetCoupleSpaceResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_couple_space(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<GetCoupleSpaceResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.get_couple_space(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Renames the space or moves its anniversary date; both sides may edit.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        patch,
        path = "/api/user/couple",
        tag = "Couple Space",
        request_body = UpdateCoupleSpaceRequest,
        responses((status = 200, description = "Space updated", body = UpdateCoupleSpaceResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn update_couple_space(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<UpdateCoupleSpaceRequest>,
) -> AppResult<Json<UpdateCoupleSpaceResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.update_couple_space(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Invites one partner. The invite stays pending until accepted or dismissed, so
/// this is the only write either side can make before the pair exists.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/couple/invites",
        tag = "Couple Space",
        request_body = InviteCouplePartnerRequest,
        responses((status = 200, description = "Invite sent", body = InviteCouplePartnerResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn invite_couple_partner(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<InviteCouplePartnerRequest>,
) -> AppResult<Json<InviteCouplePartnerResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.invite_couple_partner(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/couple/invites/accept",
        tag = "Couple Space",
        request_body = AcceptCoupleInviteRequest,
        responses((status = 200, description = "Invite accepted and space bound", body = AcceptCoupleInviteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn accept_couple_invite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<AcceptCoupleInviteRequest>,
) -> AppResult<Json<AcceptCoupleInviteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.accept_couple_invite(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Declines an incoming invite, or withdraws one the caller sent.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/couple/invites/dismiss",
        tag = "Couple Space",
        request_body = DismissCoupleInviteRequest,
        responses((status = 200, description = "Invite dismissed", body = DismissCoupleInviteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn dismiss_couple_invite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<DismissCoupleInviteRequest>,
) -> AppResult<Json<DismissCoupleInviteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.dismiss_couple_invite(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Unbinds the pair. Either side can, and the space's own lists go with it.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/couple/unbind",
        tag = "Couple Space",
        request_body = UnbindCoupleSpaceRequest,
        responses((status = 200, description = "Space unbound", body = UnbindCoupleSpaceResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn unbind_couple_space(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<UnbindCoupleSpaceRequest>,
) -> AppResult<Json<UnbindCoupleSpaceResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.unbind_couple_space(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/couple/favorites",
        tag = "Couple Space",
        params(ListCoupleFavoritesRequest),
        responses((status = 200, description = "Shared favorites page", body = ListCoupleFavoritesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_couple_favorites(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListCoupleFavoritesRequest>,
) -> AppResult<Json<ListCoupleFavoritesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_couple_favorites(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/couple/favorites",
        tag = "Couple Space",
        request_body = AddCoupleFavoriteRequest,
        responses((status = 200, description = "Added to the shared list", body = AddCoupleFavoriteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn add_couple_favorite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<AddCoupleFavoriteRequest>,
) -> AppResult<Json<AddCoupleFavoriteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.add_couple_favorite(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Keyed by `spaceId` + `sourceKey`, mirroring the personal unstar endpoint.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/couple/favorites",
        tag = "Couple Space",
        params(RemoveCoupleFavoriteRequest),
        responses((status = 200, description = "Removed from the shared list", body = RemoveCoupleFavoriteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn remove_couple_favorite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<RemoveCoupleFavoriteRequest>,
) -> AppResult<Json<RemoveCoupleFavoriteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .remove_couple_favorite(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// The 一起看过 rail: both partners' history interleaved into one timeline.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/couple/shared-watch-entries",
        tag = "Couple Space",
        params(ListCoupleSharedWatchEntriesRequest),
        responses((status = 200, description = "Shared watch timeline page", body = ListCoupleSharedWatchEntriesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_couple_shared_watch_entries(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListCoupleSharedWatchEntriesRequest>,
) -> AppResult<Json<ListCoupleSharedWatchEntriesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .list_couple_shared_watch_entries(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
