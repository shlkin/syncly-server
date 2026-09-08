//! Subscription sources an administrator publishes to every account.
//!
//! The client has always distinguished these from the ones a user imports on
//! their own device — it refuses to edit a server source — but nothing produced
//! any, so the same endpoints had to be typed in again on every device.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How to read a source's endpoint. Closed set: the client has one adapter per
/// value, and a row it cannot parse is a row nothing can play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VodSourceFormat {
    /// MacCMS / CMS10 JSON.
    MaccmsJson,
    /// The same API rendered as XML.
    MaccmsXml,
    /// An M3U playlist: one flat list where `group-title` is the category.
    M3u,
}

impl VodSourceFormat {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaccmsJson => "maccms_json",
            Self::MaccmsXml => "maccms_xml",
            Self::M3u => "m3u",
        }
    }
}

impl FromStr for VodSourceFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "maccms_json" | "maccms" | "json" => Ok(Self::MaccmsJson),
            "maccms_xml" | "xml" => Ok(Self::MaccmsXml),
            "m3u" | "m3u8" => Ok(Self::M3u),
            other => Err(format!("unknown vod source format: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VodSource {
    pub id: i64,
    pub name: String,
    pub format: VodSourceFormat,
    pub endpoint: String,
    /// Sent with every catalog request. A browser build drops these — it cannot
    /// set `User-Agent` or `Referer` — so a source that needs them only works
    /// on a native client.
    pub headers: HashMap<String, String>,
    /// A disabled source stays editable and stops being served to clients,
    /// which is how one is retired without losing what it was.
    pub enabled: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewVodSource {
    pub name: String,
    pub format: VodSourceFormat,
    pub endpoint: String,
    pub headers: HashMap<String, String>,
    pub enabled: bool,
    pub sort_order: i32,
}

/// Fields an edit may change. `None` leaves the stored value alone, so a form
/// that only touched the name does not have to resend the rest.
#[derive(Debug, Clone, Default)]
pub struct VodSourceUpdate {
    pub name: Option<String>,
    pub format: Option<VodSourceFormat>,
    pub endpoint: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub enabled: Option<bool>,
    pub sort_order: Option<i32>,
}
