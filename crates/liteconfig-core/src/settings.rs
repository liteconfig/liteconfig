//! Device-local settings persisted to `~/.liteconfig/settings.json`.
//!
//! These settings are **never** synced to GitHub — they encode the current
//! machine's active profile pointer, path overrides, theme, etc.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fs_util::atomic_write;
use crate::model::agent::AgentKind;
use crate::model::skill::{StorageMode, SyncMethod};
use crate::paths::{ensure_dir, liteconfig_dir, liteconfig_settings_path};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub current_profile: BTreeMap<AgentKind, Option<String>>,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub skill_storage_location: StorageMode,
    #[serde(default = "default_skill_sync_method")]
    pub skill_sync_method_default: SyncMethod,
    #[serde(default = "default_true")]
    pub confirm_before_write: bool,
    #[serde(default)]
    pub github_backup: GithubBackupSettings,
    #[serde(default)]
    pub path_overrides: PathOverrides,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathOverrides {
    #[serde(default)]
    pub claude_config_dir: Option<String>,
    #[serde(default)]
    pub codex_config_dir: Option<String>,
    #[serde(default)]
    pub gemini_config_dir: Option<String>,
    #[serde(default)]
    pub cursor_config_dir: Option<String>,
    #[serde(default)]
    pub opencode_config_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubBackupSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub auto_sync: bool,
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_skill_sync_method() -> SyncMethod {
    SyncMethod::Symlink
}
fn default_true() -> bool {
    true
}
fn default_branch() -> String {
    "main".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            current_profile: BTreeMap::new(),
            theme: default_theme(),
            skill_storage_location: StorageMode::default(),
            skill_sync_method_default: default_skill_sync_method(),
            confirm_before_write: true,
            github_backup: GithubBackupSettings::default(),
            path_overrides: PathOverrides::default(),
        }
    }
}

impl Settings {
    pub fn load_or_default() -> Result<Self> {
        let path = liteconfig_settings_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let s: Settings = serde_json::from_slice(&bytes).map_err(|source| Error::Json {
            path: path.clone(),
            source,
        })?;
        Ok(s)
    }

    pub fn save(&self) -> Result<()> {
        ensure_dir(&liteconfig_dir()?)?;
        let path = liteconfig_settings_path()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        atomic_write(&path, &bytes)
    }

    pub fn current_profile_for(&self, agent: AgentKind) -> Option<&str> {
        self.current_profile.get(&agent).and_then(|v| v.as_deref())
    }

    pub fn set_current_profile(&mut self, agent: AgentKind, id: Option<String>) {
        self.current_profile.insert(agent, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_defaults() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, "dark");
        assert_eq!(back.skill_sync_method_default, SyncMethod::Symlink);
        assert_eq!(back.skill_storage_location, StorageMode::Liteconfig);
        assert!(back.confirm_before_write);
    }

    #[test]
    fn current_profile_setter() {
        let mut s = Settings::default();
        assert_eq!(s.current_profile_for(AgentKind::Claude), None);
        s.set_current_profile(AgentKind::Claude, Some("abc".into()));
        assert_eq!(s.current_profile_for(AgentKind::Claude), Some("abc"));
        s.set_current_profile(AgentKind::Claude, None);
        assert_eq!(s.current_profile_for(AgentKind::Claude), None);
    }
}
