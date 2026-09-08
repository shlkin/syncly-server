//! 影视 subscription sources, as a client browses them.
//!
//! Read-only on this side. The client has always told a server source apart
//! from one the user imported on their own device — it refuses to edit the
//! first kind — and these are that first kind: an administrator publishes one
//! and every account sees it, instead of the endpoint being typed in again on
//! every device.

use synctv_core::models::{VodSource, VodSourceFormat};

use super::ClientApiImpl;
use crate::impls::ApiError;

impl ClientApiImpl {
    /// The enabled sources, in the order an administrator set.
    pub async fn list_vod_sources(
        &self,
    ) -> Result<synctv_proto::client::ListVodSourcesResponse, ApiError> {
        let sources = self
            .user_service
            .list_enabled_vod_sources()
            .await
            .map_err(ApiError::from)?;

        Ok(synctv_proto::client::ListVodSourcesResponse {
            sources: sources
                .into_iter()
                .map(|source| vod_source_to_proto(source, &self.public_id_codec))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

pub(crate) const fn vod_source_format_to_proto(format: VodSourceFormat) -> i32 {
    match format {
        VodSourceFormat::MaccmsJson => synctv_proto::client::VodSourceFormat::MaccmsJson as i32,
        VodSourceFormat::MaccmsXml => synctv_proto::client::VodSourceFormat::MaccmsXml as i32,
        VodSourceFormat::M3u => synctv_proto::client::VodSourceFormat::M3u as i32,
    }
}

/// `None` for the unspecified value, which is what a client that left the
/// field unset sends — the caller decides whether that is an error or a
/// field it may leave alone.
pub(crate) fn vod_source_format_from_proto(format: i32) -> Option<VodSourceFormat> {
    match synctv_proto::client::VodSourceFormat::try_from(format) {
        Ok(synctv_proto::client::VodSourceFormat::MaccmsJson) => Some(VodSourceFormat::MaccmsJson),
        Ok(synctv_proto::client::VodSourceFormat::MaccmsXml) => Some(VodSourceFormat::MaccmsXml),
        Ok(synctv_proto::client::VodSourceFormat::M3u) => Some(VodSourceFormat::M3u),
        _ => None,
    }
}

/// The client-facing shape. `enabled` is deliberately absent: a client is only
/// ever handed enabled sources, so carrying the flag would invite a client to
/// render one it was never given.
pub(crate) fn vod_source_to_proto(
    source: VodSource,
    public_id_codec: &synctv_adapter::PublicIdCodec,
) -> Result<synctv_proto::client::VodSource, ApiError> {
    Ok(synctv_proto::client::VodSource {
        id: public_id_codec
            .encode_vod_source_id(source.id)
            .map_err(ApiError::Internal)?,
        name: source.name,
        format: vod_source_format_to_proto(source.format),
        endpoint: source.endpoint,
        headers: source.headers,
        sort_order: source.sort_order,
        created_at: source.created_at.timestamp(),
        updated_at: source.updated_at.timestamp(),
    })
}
