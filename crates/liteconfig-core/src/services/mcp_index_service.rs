//! Live search against Smithery's public MCP registry.
//!
//! API: https://registry.smithery.ai/servers?q=<query>&page=<N>&pageSize=<N>
//! Response shape (abbreviated):
//! ```json
//! { "servers": [
//!     { "qualifiedName": "owner/repo", "displayName": "...",
//!       "description": "...", "iconUrl": "...", "useCount": 42,
//!       "remote": true|false }
//!   ], "pagination": { ... } }
//! ```
//!
//! We only surface entries that expose a concrete install command — i.e.
//! anything we can resolve to an `(command, args)` pair the MCP-servers
//! table can store. For results that only hint at a remote URL, we encode
//! that via the `{ url }` form consumed by `mcp_service::upsert`.

use std::time::Duration;

use serde::Deserialize;

use crate::{Error, Result};

/// One result from Smithery's registry. Extra fields on the wire are ignored.
#[derive(Debug, Clone)]
pub struct ExternalMcp {
    pub qualified_name: String,
    pub display_name: String,
    pub description: String,
    pub use_count: u64,
    pub homepage: String,
    pub remote: bool,
}

impl ExternalMcp {
    /// Produce the normalized config blob used by `mcp_service::upsert`.
    /// Remote entries emit a `{ url }` blob; local entries default to
    /// `npx -y <package>` using the qualified name as the package id. The
    /// user can edit the resulting row if a deeper config is needed.
    pub fn install_config(&self) -> serde_json::Value {
        if self.remote {
            serde_json::json!({
                "url": format!("https://server.smithery.ai/{}/mcp", self.qualified_name),
                "homepage": self.homepage,
                "description": self.description,
            })
        } else {
            serde_json::json!({
                "command": "npx",
                "args": ["-y", self.qualified_name.clone()],
                "homepage": self.homepage,
                "description": self.description,
            })
        }
    }
}

/// Build the search URL without opening a socket. Extracted so tests assert
/// the URL shape offline.
pub fn search_url(query: &str, page: u32, page_size: u32) -> String {
    format!(
        "https://registry.smithery.ai/servers?q={}&page={}&pageSize={}",
        urlencode(query),
        page.max(1),
        page_size.clamp(1, 50),
    )
}

/// Hit Smithery's registry. 10s timeout, identical error-wrapping to
/// `skill_index_service::search` so the TUI's toast path treats them
/// uniformly.
pub fn search(query: &str, page: u32, page_size: u32) -> Result<Vec<ExternalMcp>> {
    let url = search_url(query, page, page_size);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let raw = agent
        .get(&url)
        .call()
        .map_err(|e| Error::InvalidConfig(format!("smithery: {e}")))?
        .into_string()
        .map_err(|e| Error::InvalidConfig(format!("smithery body: {e}")))?;
    parse_response(&raw)
}

fn parse_response(raw: &str) -> Result<Vec<ExternalMcp>> {
    let body: SearchBody = serde_json::from_str(raw)
        .map_err(|e| Error::InvalidConfig(format!("smithery: bad json: {e}")))?;
    Ok(body
        .servers
        .into_iter()
        .filter(|hit| !hit.qualified_name.is_empty())
        .map(|hit| ExternalMcp {
            homepage: format!("https://smithery.ai/server/{}", hit.qualified_name),
            qualified_name: hit.qualified_name,
            display_name: hit.display_name,
            description: hit.description.unwrap_or_default(),
            use_count: hit.use_count.unwrap_or(0),
            remote: hit.remote.unwrap_or(false),
        })
        .collect())
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct SearchBody {
    #[serde(default)]
    servers: Vec<HitRaw>,
}

#[derive(Debug, Deserialize)]
struct HitRaw {
    #[serde(default, rename = "qualifiedName")]
    qualified_name: String,
    #[serde(default, rename = "displayName")]
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "useCount")]
    use_count: Option<u64>,
    #[serde(default)]
    remote: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_shape_and_clamps_page_size() {
        assert_eq!(
            search_url("memory", 1, 10),
            "https://registry.smithery.ai/servers?q=memory&page=1&pageSize=10"
        );
        // Clamp below 1 to 1, above 50 to 50.
        assert!(search_url("", 0, 9999).ends_with("page=1&pageSize=50"));
    }

    #[test]
    fn parse_extracts_key_fields() {
        let body = r#"{
            "servers": [
                {"qualifiedName":"upstash/context7","displayName":"Context7","description":"Live docs","useCount":1234,"remote":false},
                {"qualifiedName":"exa-ai/exa","displayName":"Exa","description":"Search","useCount":5,"remote":true},
                {"qualifiedName":"","displayName":"skip","description":""}
            ]
        }"#;
        let hits = parse_response(body).unwrap();
        assert_eq!(hits.len(), 2, "empty qualifiedName should be dropped");
        assert_eq!(hits[0].qualified_name, "upstash/context7");
        assert!(hits[1].remote);
    }

    #[test]
    fn install_config_local_vs_remote() {
        let local = ExternalMcp {
            qualified_name: "acme/local".into(),
            display_name: "Local".into(),
            description: "d".into(),
            use_count: 0,
            homepage: "".into(),
            remote: false,
        };
        let cfg = local.install_config();
        assert_eq!(cfg["command"], "npx");
        assert_eq!(cfg["args"][1], "acme/local");

        let remote = ExternalMcp {
            qualified_name: "acme/remote".into(),
            display_name: "Remote".into(),
            description: "d".into(),
            use_count: 0,
            homepage: "".into(),
            remote: true,
        };
        let cfg = remote.install_config();
        assert!(cfg["url"].as_str().unwrap().contains("acme/remote"));
    }

    #[test]
    #[ignore = "network-dependent; run manually with --ignored"]
    fn live_hit_smithery() {
        let hits = search("memory", 1, 5).unwrap();
        assert!(!hits.is_empty());
    }
}
