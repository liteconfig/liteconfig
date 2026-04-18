//! Gemini CLI agent adapter.
//!
//! Live config: `~/.gemini/settings.json` (JSON).

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::agents::{AgentAdapter, AgentPaths};
use crate::fs_util::{atomic_write, read_to_string};
use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::profile::Profile;
use crate::paths;
use crate::settings::Settings;
use crate::{Error, Result};

pub struct GeminiAdapter;

impl AgentAdapter for GeminiAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Gemini
    }

    fn paths(&self, settings: &Settings) -> Result<AgentPaths> {
        let dir = paths::gemini_config_dir(settings)?;
        Ok(AgentPaths {
            live_settings: paths::gemini_settings_path(settings)?,
            mcp_file: Some(paths::gemini_settings_path(settings)?),
            rule_file: Some(dir.join("AGENTS.md")),
            skills_dir: Some(dir.join("skills")),
            sessions_dir: Some(dir.join("sessions")),
            extra: vec![],
        })
    }

    fn read_live(&self, settings: &Settings) -> Result<Option<Value>> {
        let path = paths::gemini_settings_path(settings)?;
        if !path.exists() {
            return Ok(None);
        }
        let text = read_to_string(&path)?;
        let v: Value = serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.clone(),
            source,
        })?;
        Ok(Some(v))
    }

    fn write_live(
        &self,
        settings: &Settings,
        profile: &Profile,
        common: Option<&Value>,
    ) -> Result<()> {
        let mut merged = profile.config.clone();
        if let Some(common) = common {
            merged = crate::agents::claude::deep_merge(merged, common.clone());
        }
        let path = paths::gemini_settings_path(settings)?;
        atomic_write(&path, &serde_json::to_vec_pretty(&merged)?)
    }

    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>> {
        let v = match self.read_live(settings)? {
            Some(v) => v,
            None => return Ok(vec![]),
        };
        let map = v
            .get("mcpServers")
            .and_then(|x| x.as_object())
            .cloned()
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let mut out = Vec::with_capacity(map.len());
        for (name, cfg) in map {
            let mut enabled = std::collections::BTreeMap::new();
            enabled.insert(AgentKind::Gemini, true);
            out.push(McpServer {
                id: uuid::Uuid::new_v4().to_string(),
                name,
                config: cfg,
                enabled,
                created_at: now,
                updated_at: now,
            });
        }
        Ok(out)
    }

    fn write_mcp(&self, settings: &Settings, servers: &[McpServer]) -> Result<()> {
        let path = paths::gemini_settings_path(settings)?;
        let existing: Value = if path.exists() {
            let text = read_to_string(&path)?;
            serde_json::from_str(&text).map_err(|source| Error::Json {
                path: path.clone(),
                source,
            })?
        } else {
            json!({})
        };
        let mut root = existing.as_object().cloned().unwrap_or_default();
        let mut map = serde_json::Map::new();
        for s in servers
            .iter()
            .filter(|s| s.is_enabled_for(AgentKind::Gemini))
        {
            map.insert(s.name.clone(), s.config.clone());
        }
        root.insert("mcpServers".into(), Value::Object(map));
        atomic_write(&path, &serde_json::to_vec_pretty(&Value::Object(root))?)
    }

    fn skill_registry_target(&self, settings: &Settings) -> Option<PathBuf> {
        paths::gemini_config_dir(settings)
            .ok()
            .map(|d| d.join("skills"))
    }
}
