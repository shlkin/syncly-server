//! 影视 subscription sources, as a client reads them.
//!
//! One route, and no write half: editing lives under `/api/admin/vod-sources`.
//! That split is the whole point of a server source — every account gets the
//! same list, and no device can quietly change what the others see.

use axum::{extract::State, routing::get, Json, Router};

use super::{middleware::RequestMetadata, AppResult, AppState};
use synctv_api_common::impls::EndpointRateLimitCategory;
use synctv_proto::client::ListVodSourcesResponse;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/vod/sources", get(list_vod_sources))
}

/// The enabled sources, in the order an administrator set.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/vod/sources",
        tag = "Vod",
        responses((status = 200, description = "Enabled subscription sources", body = ListVodSourcesResponse)),
        security(("bearer_auth" = []))
    )
)]
pub async fn list_vod_sources(
    request_meta: RequestMetadata,
    State(state): State<AppState>,
) -> AppResult<Json<ListVodSourcesResponse>> {
    let executor = state.shared_api_runtime.client_api.clone();
    let client_api = state.shared_api_runtime.client_api.clone();
    let response = executor
        .execute_user_endpoint(
            &request_meta.0,
            EndpointRateLimitCategory::Read,
            |_auth| async move { client_api.list_vod_sources().await },
        )
        .await
        .map_err(super::error::map_api_error)?;
    Ok(Json(response))
}
