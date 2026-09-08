//! Subscription sources an administrator publishes.
//!
//! Headers live in a JSONB column rather than a side table: they are a small
//! opaque bag written and read whole, never queried by key, and a source with
//! no headers — most of them — costs one empty object instead of a join.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};

use super::user::UserRepository;
use crate::{
    models::{NewVodSource, PageParams, UserId, VodSource, VodSourceFormat, VodSourceUpdate},
    Error, Result,
};

/// A stored row before its format string and header bag are parsed.
struct VodSourceRow {
    id: i64,
    name: String,
    format: String,
    endpoint: String,
    headers: serde_json::Value,
    enabled: bool,
    sort_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Reads the header bag, dropping anything that is not a string pair.
///
/// The column is written by this server from a validated map, so a malformed
/// value means the row was edited by hand; treating it as empty keeps the
/// source usable rather than failing the whole listing.
fn headers_from_json(value: &serde_json::Value) -> HashMap<String, String> {
    let Some(object) = value.as_object() else {
        return HashMap::new();
    };
    object
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
        .collect()
}

fn source_from_row(row: VodSourceRow) -> Result<VodSource> {
    Ok(VodSource {
        id: row.id,
        name: row.name,
        format: VodSourceFormat::from_str(&row.format).map_err(Error::InvalidInput)?,
        endpoint: row.endpoint,
        headers: headers_from_json(&row.headers),
        enabled: row.enabled,
        sort_order: row.sort_order,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn headers_to_json(headers: &HashMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        headers
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect(),
    )
}

impl UserRepository {
    /// The sources clients may use, in the order an administrator set.
    pub async fn list_enabled_vod_sources(&self) -> Result<Vec<VodSource>> {
        let rows = sqlx::query_as!(
            VodSourceRow,
            r#"
            SELECT id,
                   name,
                   format,
                   endpoint,
                   headers,
                   enabled,
                   sort_order,
                   created_at,
                   updated_at
            FROM vod_sources
            WHERE enabled
            ORDER BY sort_order ASC, id ASC
            "#,
        )
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(source_from_row).collect()
    }

    /// Every source, disabled ones included: the admin console edits those, so
    /// hiding them there would make a retired source unrecoverable.
    pub async fn list_all_vod_sources(
        &self,
        pagination: PageParams,
    ) -> Result<(Vec<VodSource>, i64)> {
        let limit = pagination.limit_i64()?;
        let offset = pagination.offset_i64()?;

        let total = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "total!" FROM vod_sources"#)
            .fetch_one(self.pool())
            .await?;

        let rows = sqlx::query_as!(
            VodSourceRow,
            r#"
            SELECT id,
                   name,
                   format,
                   endpoint,
                   headers,
                   enabled,
                   sort_order,
                   created_at,
                   updated_at
            FROM vod_sources
            ORDER BY sort_order ASC, id ASC
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset,
        )
        .fetch_all(self.pool())
        .await?;

        let sources = rows
            .into_iter()
            .map(source_from_row)
            .collect::<Result<Vec<_>>>()?;
        Ok((sources, total))
    }

    pub async fn create_vod_source(
        &self,
        source: &NewVodSource,
        created_by: &UserId,
    ) -> Result<VodSource> {
        let row = sqlx::query_as!(
            VodSourceRow,
            r#"
            INSERT INTO vod_sources
                (name, format, endpoint, headers, enabled, sort_order, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id,
                      name,
                      format,
                      endpoint,
                      headers,
                      enabled,
                      sort_order,
                      created_at,
                      updated_at
            "#,
            source.name,
            source.format.as_str(),
            source.endpoint,
            headers_to_json(&source.headers),
            source.enabled,
            source.sort_order,
            created_by as &UserId,
        )
        .fetch_one(self.pool())
        .await?;

        source_from_row(row)
    }

    /// Applies only the fields an edit carried; COALESCE leaves the rest.
    pub async fn update_vod_source(
        &self,
        id: i64,
        update: &VodSourceUpdate,
    ) -> Result<Option<VodSource>> {
        let headers = update.headers.as_ref().map(headers_to_json);
        let row = sqlx::query_as!(
            VodSourceRow,
            r#"
            UPDATE vod_sources
            SET name = COALESCE($2, name),
                format = COALESCE($3, format),
                endpoint = COALESCE($4, endpoint),
                headers = COALESCE($5, headers),
                enabled = COALESCE($6, enabled),
                sort_order = COALESCE($7, sort_order),
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1
            RETURNING id,
                      name,
                      format,
                      endpoint,
                      headers,
                      enabled,
                      sort_order,
                      created_at,
                      updated_at
            "#,
            id,
            update.name.as_deref(),
            update.format.map(VodSourceFormat::as_str),
            update.endpoint.as_deref(),
            headers,
            update.enabled,
            update.sort_order,
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(source_from_row).transpose()
    }

    /// Hard delete: a source holds no history of its own, and every catalog it
    /// produced was fetched live.
    pub async fn delete_vod_source(&self, id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM vod_sources WHERE id = $1", id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn vod_source(&self, id: i64) -> Result<Option<VodSource>> {
        let row = sqlx::query_as!(
            VodSourceRow,
            r#"
            SELECT id,
                   name,
                   format,
                   endpoint,
                   headers,
                   enabled,
                   sort_order,
                   created_at,
                   updated_at
            FROM vod_sources
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(source_from_row).transpose()
    }
}
