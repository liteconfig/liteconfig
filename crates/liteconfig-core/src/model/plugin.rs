//! Claude Code plugin rows.
//!
//! A plugin bundles skills + MCP servers + subagents + slash commands under
//! one folder. Installing a plugin clones/copies its source into
//! `~/.liteconfig/plugins/<id>/` and scans for the CC layout:
//! - `.claude-plugin/plugin.json` — manifest (name, description)
//! - `skills/<name>/SKILL.md`
//! - `commands/<name>.md`
//! - `agents/<name>.md`
//! - `.mcp.json` / `mcp.json` — MCP servers

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::agent::AgentKind;

/// Where the plugin's source lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginSource {
    /// Cloned from a git URL into `~/.liteconfig/plugins/<id>/`.
    Git {
        url: String,
        #[serde(default = "default_branch")]
        branch: String,
    },
    /// Copied from a local directory.
    Local { path: PathBuf },
}

fn default_branch() -> String {
    "main".to_string()
}

/// Count of each resource type a plugin contributes. Populated by
/// `plugin_service::sync` after a scan; rendered in the Plugins tab.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PluginContents {
    #[serde(default)]
    pub skills: u32,
    #[serde(default)]
    pub mcp_servers: u32,
    #[serde(default)]
    pub commands: u32,
    #[serde(default)]
    pub agents: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Canonical on-disk directory under `~/.liteconfig/plugins/<id>/`.
    pub directory: PathBuf,
    pub source: PluginSource,
    /// Per-agent enablement — whether this plugin's resources should sync
    /// to that agent on the next `sync_all`.
    #[serde(default)]
    pub enabled: BTreeMap<AgentKind, bool>,
    #[serde(default)]
    pub contents: PluginContents,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub installed_at: i64,
    #[serde(default)]
    pub last_synced_at: Option<i64>,
}

impl Plugin {
    pub fn is_enabled_for(&self, agent: AgentKind) -> bool {
        *self.enabled.get(&agent).unwrap_or(&false)
    }
}
