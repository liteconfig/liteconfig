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

/// Derived drift state of a skill. Drives the Status column in the Skills
/// tab and colour cues. Computed from the pair `(content_hash,
/// last_synced_hash)` — never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillStatus {
    /// No `content_hash` yet — skill hasn't been walked by the hasher.
    Unknown,
    /// Hash known, but never synced to any agent.
    Unsynced,
    /// On-disk hash matches the last-synced snapshot.
    InSync,
    /// On-disk hash differs from the last-synced snapshot — user changed
    /// files after syncing.
    Drifted,
}

impl SkillStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillStatus::Unknown => "unknown",
            SkillStatus::Unsynced => "unsynced",
            SkillStatus::InSync => "in sync",
            SkillStatus::Drifted => "drift",
        }
    }
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
    /// Live on-disk hash of the skill directory. Updated by any path that
    /// reads the files (install, scan_from_live, drift-recompute).
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Hash at the moment of the last successful sync to at least one
    /// agent. If `content_hash == last_synced_hash`, the skill is in sync;
    /// otherwise it has drifted. `None` → never synced.
    #[serde(default)]
    pub last_synced_hash: Option<String>,
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

    pub fn status(&self) -> SkillStatus {
        match (&self.content_hash, &self.last_synced_hash) {
            (None, _) => SkillStatus::Unknown,
            (Some(h), _) if h.is_empty() => SkillStatus::Unknown,
            (Some(_), None) => SkillStatus::Unsynced,
            (Some(live), Some(synced)) if live == synced => SkillStatus::InSync,
            _ => SkillStatus::Drifted,
        }
    }
}
