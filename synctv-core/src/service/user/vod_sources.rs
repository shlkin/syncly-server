//! Subscription sources an administrator publishes.
//!
//! Validation lives here rather than in the repository so the admin API and any
//! future importer answer the same way: a source with no name or an endpoint
//! that is not an absolute HTTP URL is rejected before it reaches a row, where
//! the CHECK constraints would only produce a database error.

use std::collections::HashMap;

use crate::{
    models::{NewVodSource, PageParams, UserId, VodSource, VodSourceFormat, VodSourceUpdate},
    Error, Result,
};

use super::UserService;

/// Longest accepted display name, matching `vod_sources.name`.
const NAME_MAX_CHARS: usize = 120;
/// Bounds the header bag. Sources need one or two headers; a hundred is a
/// mistake or an attempt to store something else here.
const HEADERS_MAX: usize = 16;

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidInput("Source name is required".to_string()));
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(Error::InvalidInput(format!(
            "Source name may not exceed {NAME_MAX_CHARS} characters"
        )));
    }
    Ok(name.to_string())
}

/// Requires an absolute `http`/`https` endpoint.
///
/// A relative address has no meaning here: the client fetches it directly, from
/// a device that shares no origin with this server.
fn validate_endpoint(endpoint: &str) -> Result<String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(Error::InvalidInput(
            "Source endpoint is required".to_string(),
        ));
    }
    let parsed = url::Url::parse(endpoint)
        .map_err(|_| Error::InvalidInput("Source endpoint is not a URL".to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::InvalidInput(
            "Source endpoint must be http or https".to_string(),
        ));
    }
    Ok(endpoint.to_string())
}

fn validate_headers(headers: &HashMap<String, String>) -> Result<HashMap<String, String>> {
    if headers.len() > HEADERS_MAX {
        return Err(Error::InvalidInput(format!(
            "A source may carry at most {HEADERS_MAX} headers"
        )));
    }
    let mut cleaned = HashMap::with_capacity(headers.len());
    for (key, value) in headers {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // A newline in either half would let one header smuggle in another.
        if key.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
            return Err(Error::InvalidInput(
                "Header names and values may not contain line breaks".to_string(),
            ));
        }
        cleaned.insert(key.to_string(), value.trim().to_string());
    }
    Ok(cleaned)
}

impl UserService {
    /// What a client may browse: the enabled sources, in admin order.
    pub async fn list_enabled_vod_sources(&self) -> Result<Vec<VodSource>> {
        self.repository.list_enabled_vod_sources().await
    }

    /// Every source, for the admin console.
    pub async fn admin_list_vod_sources(
        &self,
        pagination: PageParams,
    ) -> Result<(Vec<VodSource>, i64)> {
        self.repository.list_all_vod_sources(pagination).await
    }

    pub async fn create_vod_source(
        &self,
        name: &str,
        format: VodSourceFormat,
        endpoint: &str,
        headers: &HashMap<String, String>,
        enabled: bool,
        sort_order: i32,
        created_by: &UserId,
    ) -> Result<VodSource> {
        let source = NewVodSource {
            name: validate_name(name)?,
            format,
            endpoint: validate_endpoint(endpoint)?,
            headers: validate_headers(headers)?,
            enabled,
            sort_order,
        };
        self.repository.create_vod_source(&source, created_by).await
    }

    pub async fn update_vod_source(&self, id: i64, update: VodSourceUpdate) -> Result<VodSource> {
        let update = VodSourceUpdate {
            name: update.name.as_deref().map(validate_name).transpose()?,
            endpoint: update
                .endpoint
                .as_deref()
                .map(validate_endpoint)
                .transpose()?,
            headers: update.headers.as_ref().map(validate_headers).transpose()?,
            ..update
        };
        self.repository
            .update_vod_source(id, &update)
            .await?
            .ok_or_else(|| Error::NotFound("Source not found".to_string()))
    }

    pub async fn delete_vod_source(&self, id: i64) -> Result<()> {
        if self.repository.delete_vod_source(id).await? {
            return Ok(());
        }
        Err(Error::NotFound("Source not found".to_string()))
    }
}
