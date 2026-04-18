//! Claude Code agent adapter.
//!
//! Live config: `~/.claude/settings.json` (JSON).
//! MCP list:    `~/.claude.json`           (JSON, separate file).

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::agents::{AgentAdapter, AgentPaths};
use crate::fs_util::{atomic_write, read_to_string};
use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::profile::Profile;
use crate::paths;
use crate::settings::Settings;
use crate::{Error, Result};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn paths(&self, settings: &Settings) -> Result<AgentPaths> {
        let dir = paths::claude_config_dir(settings)?;
        Ok(AgentPaths {
            live_settings: paths::claude_settings_path(settings)?,
            mcp_file: Some(paths::claude_mcp_path(settings)?),
            rule_file: Some(dir.join("CLAUDE.md")),
            skills_dir: Some(dir.join("skills")),
            sessions_dir: Some(dir.join("sessions")),
            extra: vec![],
        })
    }

    fn read_live(&self, settings: &Settings) -> Result<Option<Value>> {
        let path = paths::claude_settings_path(settings)?;
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
            merged = deep_merge(merged, common.clone());
        }
        let path = paths::claude_settings_path(settings)?;
        let bytes = serde_json::to_vec_pretty(&merged)?;
        atomic_write(&path, &bytes)
    }

    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>> {
        let path = paths::claude_mcp_path(settings)?;
        if !path.exists() {
            return Ok(vec![]);
        }
        let text = read_to_string(&path)?;
        let v: Value = serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.clone(),
            source,
        })?;
        let map = v
            .get("mcpServers")
            .and_then(|x| x.as_object())
            .cloned()
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let mut out = Vec::with_capacity(map.len());
        for (name, cfg) in map {
            let mut enabled = std::collections::BTreeMap::new();
            enabled.insert(AgentKind::Claude, true);
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
        let path = paths::claude_mcp_path(settings)?;
        // Preserve any unrelated top-level keys the user may have added.
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
        let mut map = Map::new();
        for s in servers
            .iter()
            .filter(|s| s.is_enabled_for(AgentKind::Claude))
        {
            map.insert(s.name.clone(), s.config.clone());
        }
        root.insert("mcpServers".into(), Value::Object(map));
        let bytes = serde_json::to_vec_pretty(&Value::Object(root))?;
        atomic_write(&path, &bytes)
    }

    fn skill_registry_target(&self, settings: &Settings) -> Option<PathBuf> {
        paths::claude_config_dir(settings)
            .ok()
            .map(|d| d.join("skills"))
    }
}

/// Recursively merge `overlay` into `base`: for object nodes, overlay keys
/// win; for everything else, overlay replaces.
pub fn deep_merge(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let merged = match a.remove(&k) {
                    Some(existing) => deep_merge(existing, v),
                    None => v,
                };
                a.insert(k, merged);
            }
            Value::Object(a)
        }
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deep_merge_nested_objects() {
        let a = json!({ "env": { "FOO": "1", "BAR": "2" }, "theme": "dark" });
        let b = json!({ "env": { "BAR": "overridden", "BAZ": "3" } });
        let merged = deep_merge(a, b);
        assert_eq!(merged["env"]["FOO"], "1");
        assert_eq!(merged["env"]["BAR"], "overridden");
        assert_eq!(merged["env"]["BAZ"], "3");
        assert_eq!(merged["theme"], "dark");
    }

    #[test]
    fn deep_merge_overlay_replaces_non_objects() {
        let a = json!({ "x": [1, 2, 3] });
        let b = json!({ "x": [9] });
        let merged = deep_merge(a, b);
        assert_eq!(merged["x"], json!([9]));
    }
}
