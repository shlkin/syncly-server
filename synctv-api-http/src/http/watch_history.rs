//! 观看历史 and the media half of 我的收藏.
//!
//! Both lists are the caller's own, so no id names a subject: the endpoints read
//! whoever the token authenticates. Deleting one history entry takes its public id
//! in the path, while unstarring takes a `sourceKey` in the query — a client
//! toggling a star holds the key it played, not an id it never saw.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

use super::{middleware::RequestMetadata, validation::ProtoQuery, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::{
    AddMediaFavoriteRequest, AddMediaFavoriteResponse, ClearWatchHistoryResponse,
    DeleteWatchHistoryEntryRequest, DeleteWatchHistoryEntryResponse, GetMediaFavoriteStatesRequest,
    GetMediaFavoriteStatesResponse, ListMediaFavoritesRequest, ListMediaFavoritesResponse,
    ListWatchHistoryRequest, ListWatchHistoryResponse, RecordWatchHistoryRequest,
    RecordWatchHistoryResponse, RemoveMediaFavoriteRequest, RemoveMediaFavoriteResponse,
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchHistoryEntryPath {
    pub entry_id: String,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/user/watch-history",
            get(list_watch_history)
                .post(record_watch_history)
                .delete(clear_watch_history),
        )
        .route(
            "/api/user/watch-history/{entryId}",
            axum::routing::delete(delete_watch_history_entry),
        )
        .route(
            "/api/user/media-favorites",
            get(list_media_favorites)
                .post(add_media_favorite)
                .delete(remove_media_favorite),
        )
        .route(
            "/api/user/media-favorites/states",
            post(get_media_favorite_states),
        )
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/watch-history",
        tag = "Watch History",
        params(ListWatchHistoryRequest),
        responses((status = 200, description = "Watch history page", body = ListWatchHistoryResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_watch_history(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListWatchHistoryRequest>,
) -> AppResult<Json<ListWatchHistoryResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_watch_history(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Upserts the caller's row for one title: replaying a video moves its
/// position instead of appending a second entry.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/watch-history",
        tag = "Watch History",
        request_body = RecordWatchHistoryRequest,
        responses((status = 200, description = "History entry recorded", body = RecordWatchHistoryResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn record_watch_history(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<RecordWatchHistoryRequest>,
) -> AppResult<Json<RecordWatchHistoryResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.record_watch_history(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/watch-history",
        tag = "Watch History",
        responses((status = 200, description = "History cleared", body = ClearWatchHistoryResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn clear_watch_history(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<ClearWatchHistoryResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.clear_watch_history(&auth.user_id()).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/watch-history/{entryId}",
        tag = "Watch History",
        params(("entryId" = String, Path, description = "Public watch-history entry ID")),
        responses((status = 200, description = "History entry deleted", body = DeleteWatchHistoryEntryResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn delete_watch_history_entry(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Path(path): Path<WatchHistoryEntryPath>,
) -> AppResult<Json<DeleteWatchHistoryEntryResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move {
                client_api
                    .delete_watch_history_entry(
                        &auth.user_id(),
                        DeleteWatchHistoryEntryRequest {
                            entry_id: path.entry_id,
                        },
                    )
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/user/media-favorites",
        tag = "Watch History",
        params(ListMediaFavoritesRequest),
        responses((status = 200, description = "Starred media page", body = ListMediaFavoritesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_media_favorites(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<ListMediaFavoritesRequest>,
) -> AppResult<Json<ListMediaFavoritesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move { client_api.list_media_favorites(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/media-favorites",
        tag = "Watch History",
        request_body = AddMediaFavoriteRequest,
        responses((status = 200, description = "Media starred", body = AddMediaFavoriteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn add_media_favorite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<AddMediaFavoriteRequest>,
) -> AppResult<Json<AddMediaFavoriteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.add_media_favorite(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// Unstars by `sourceKey` rather than by favorite id: the player toggling the
/// star knows what it is playing, not the row id the star was written under.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/user/media-favorites",
        tag = "Watch History",
        params(RemoveMediaFavoriteRequest),
        responses((status = 200, description = "Media unstarred", body = RemoveMediaFavoriteResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn remove_media_favorite(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    ProtoQuery(req): ProtoQuery<RemoveMediaFavoriteRequest>,
) -> AppResult<Json<RemoveMediaFavoriteResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Write,
            |auth| async move { client_api.remove_media_favorite(&auth.user_id(), req).await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}

/// POST for a read: a grid asks about many `sourceKey`s at once, and the keys
/// are long enough that a query string would risk the URL length limit.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/user/media-favorites/states",
        tag = "Watch History",
        request_body = GetMediaFavoriteStatesRequest,
        responses((status = 200, description = "Star state per source key", body = GetMediaFavoriteStatesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn get_media_favorite_states(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
    Json(req): Json<GetMediaFavoriteStatesRequest>,
) -> AppResult<Json<GetMediaFavoriteStatesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |auth| async move {
                client_api
                    .get_media_favorite_states(&auth.user_id(), req)
                    .await
            },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
