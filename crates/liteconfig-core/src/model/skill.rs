use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::agent::AgentKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMethod {
    /// Choose automatically based on OS + skill content.
    Auto,
    Symlink,
    Copy,
    /// Inherit the workspace default from Settings.
    #[default]
    Inherit,
}

impl SyncMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncMethod::Auto => "auto",
            SyncMethod::Symlink => "symlink",
            SyncMethod::Copy => "copy",
            SyncMethod::Inherit => "inherit",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            SyncMethod::Auto => SyncMethod::Symlink,
            SyncMethod::Symlink => SyncMethod::Copy,
            SyncMethod::Copy => SyncMethod::Inherit,
            SyncMethod::Inherit => SyncMethod::Auto,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(SyncMethod::Auto),
            "symlink" => Some(SyncMethod::Symlink),
            "copy" => Some(SyncMethod::Copy),
            "inherit" => Some(SyncMethod::Inherit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    /// Under `~/.liteconfig/skills/`.
    #[default]
    Liteconfig,
    /// Under `~/.agents/skills/` — shared with other tools outside liteconfig.
    Unified,
}

impl StorageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            StorageMode::Liteconfig => "liteconfig",
            StorageMode::Unified => "unified",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "liteconfig" => Some(StorageMode::Liteconfig),
            "unified" => Some(StorageMode::Unified),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillSource {
    Local,
    Github {
        owner: String,
        name: String,
        #[serde(default = "default_branch")]
        branch: String,
    },
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Canonical on-disk directory (inside liteconfig's or the unified skill store).
    pub directory: PathBuf,
    pub source: SkillSource,
    pub sync_method: SyncMethod,
    /// One boolean per agent. Uses a map so adding new agents doesn't break
    /// existing serialized rows.
    #[serde(default)]
    pub enabled: BTreeMap<AgentKind, bool>,
    #[serde(default)]
    pub content_hash: Option<String>,
    pub installed_at: i64,
    pub updated_at: i64,
}

impl Skill {
    pub fn is_enabled_for(&self, agent: AgentKind) -> bool {
        *self.enabled.get(&agent).unwrap_or(&false)
    }

    pub fn enabled_count(&self) -> usize {
        self.enabled.values().filter(|v| **v).count()
    }
}
