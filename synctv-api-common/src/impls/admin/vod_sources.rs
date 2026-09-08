//! 影视 subscription sources, as the admin console curates them.
//!
//! The client-facing list returns enabled rows only; these return every row,
//! because disabling is how a source is retired — a hidden disabled row would
//! be unrecoverable, since nothing else records what its endpoint was.
//!
//! Validation lives in the service rather than here, so a source added by any
//! future importer is held to the same rules as one typed into this console.

use std::collections::HashMap;

use synctv_core::models::{VodSource, VodSourceUpdate};

use super::{i64_to_i32_api, AdminApiImpl, ApiError};
use crate::impls::client::{vod_source_format_from_proto, vod_source_format_to_proto};

impl AdminApiImpl {
    /// Every source, in the order an administrator set.
    pub async fn list_vod_sources(
        &self,
        req: synctv_proto::admin::ListVodSourcesRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::ListVodSourcesResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let pagination = crate::impls::proto_page_params(
            req.page,
            req.page_size,
            synctv_core::models::DEFAULT_PAGE_SIZE,
            synctv_core::models::MAX_PAGE_SIZE,
        );

        let (sources, total) = self.user_service.admin_list_vod_sources(pagination).await?;

        Ok(synctv_proto::admin::ListVodSourcesResponse {
            sources: sources
                .into_iter()
                .map(|source| self.admin_vod_source_to_proto(source))
                .collect::<Result<Vec<_>, _>>()?,
            total: i64_to_i32_api(total, "vod source count")?,
        })
    }

    /// Adds a source. The endpoint is checked for an absolute http/https URL in
    /// the service, so a relative address comes back as invalid input rather
    /// than as a row nothing can fetch.
    pub async fn create_vod_source(
        &self,
        req: synctv_proto::admin::CreateVodSourceRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::CreateVodSourceResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let format = vod_source_format_from_proto(req.format)
            .ok_or_else(|| ApiError::InvalidInput("Source format is required".to_string()))?;

        let source = self
            .user_service
            .create_vod_source(
                &req.name,
                format,
                &req.endpoint,
                &req.headers,
                req.enabled,
                req.sort_order,
                admin_user_id,
            )
            .await?;

        Ok(synctv_proto::admin::CreateVodSourceResponse {
            source: Some(self.admin_vod_source_to_proto(source)?),
        })
    }

    /// A partial update: an omitted field keeps its stored value.
    ///
    /// Headers are the exception a map forces — proto3 has no `optional` map,
    /// so `replace_headers` carries the presence. Without it the stored bag is
    /// left alone even when `headers` arrives populated, which is what a client
    /// that echoes back the whole row expects.
    pub async fn update_vod_source(
        &self,
        req: synctv_proto::admin::UpdateVodSourceRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::UpdateVodSourceResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let source_id = self
            .public_id_codec
            .decode_vod_source_id(&req.source_id)
            .map_err(ApiError::InvalidInput)?;

        // An unset enum is "leave it alone" here, unlike on create, where the
        // proto rejects it before this runs.
        let format = match req.format {
            Some(format) => Some(
                vod_source_format_from_proto(format)
                    .ok_or_else(|| ApiError::InvalidInput("Unknown source format".to_string()))?,
            ),
            None => None,
        };
        let headers: Option<HashMap<String, String>> = req.replace_headers.then_some(req.headers);

        let source = self
            .user_service
            .update_vod_source(
                source_id,
                VodSourceUpdate {
                    name: req.name,
                    format,
                    endpoint: req.endpoint,
                    headers,
                    enabled: req.enabled,
                    sort_order: req.sort_order,
                },
            )
            .await?;

        Ok(synctv_proto::admin::UpdateVodSourceResponse {
            source: Some(self.admin_vod_source_to_proto(source)?),
        })
    }

    /// Removes a source outright. It holds no history of its own — every
    /// catalog it produced was fetched live — so there is nothing to orphan.
    pub async fn delete_vod_source(
        &self,
        req: synctv_proto::admin::DeleteVodSourceRequest,
        admin_user_id: &synctv_core::models::UserId,
    ) -> Result<synctv_proto::admin::DeleteVodSourceResponse, ApiError> {
        crate::impls::validate_proto_request(&req)?;
        self.require_admin_actor(admin_user_id).await?;
        let source_id = self
            .public_id_codec
            .decode_vod_source_id(&req.source_id)
            .map_err(ApiError::InvalidInput)?;
        self.user_service.delete_vod_source(source_id).await?;

        Ok(synctv_proto::admin::DeleteVodSourceResponse {})
    }

    fn admin_vod_source_to_proto(
        &self,
        source: VodSource,
    ) -> Result<synctv_proto::admin::AdminVodSource, ApiError> {
        Ok(synctv_proto::admin::AdminVodSource {
            id: self
                .public_id_codec
                .encode_vod_source_id(source.id)
                .map_err(ApiError::Internal)?,
            name: source.name,
            format: vod_source_format_to_proto(source.format),
            endpoint: source.endpoint,
            headers: source.headers,
            enabled: source.enabled,
            sort_order: source.sort_order,
            created_at: source.created_at.timestamp(),
            updated_at: source.updated_at.timestamp(),
        })
    }
}
