//! Cursor agent adapter.
//!
//! Cursor has no profile-settings concept, so `read_live`/`write_live` are
//! no-ops. It participates only in MCP sync (`~/.cursor/mcp.json`), skills
//! (`~/.cursor/skills/`), and rules (`~/.cursor/rules/*.mdc`).

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

pub struct CursorAdapter;

impl AgentAdapter for CursorAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Cursor
    }

    fn paths(&self, settings: &Settings) -> Result<AgentPaths> {
        let dir = paths::cursor_config_dir(settings)?;
        Ok(AgentPaths {
            // Present for uniformity — Cursor has no single profile-settings file.
            live_settings: dir.join("settings.json"),
            mcp_file: Some(paths::cursor_mcp_path(settings)?),
            // Cursor's rules live as individual .mdc files in a directory,
            // unlike the single-file model of other agents. Rule sync skips
            // Cursor until a per-file writer is added; auto-import still reads
            // the directory to populate the DB.
            rule_file: None,
            skills_dir: Some(dir.join("skills")),
            sessions_dir: None,
            extra: vec![],
        })
    }

    fn read_live(&self, _settings: &Settings) -> Result<Option<Value>> {
        // Cursor has no "profile settings" concept.
        Ok(None)
    }

    fn write_live(
        &self,
        _settings: &Settings,
        _profile: &Profile,
        _common: Option<&Value>,
    ) -> Result<()> {
        Err(Error::InvalidConfig(
            "Cursor has no profile settings file; use MCP/skills/rules sync instead".to_string(),
        ))
    }

    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>> {
        let path = paths::cursor_mcp_path(settings)?;
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
            enabled.insert(AgentKind::Cursor, true);
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
        let path = paths::cursor_mcp_path(settings)?;
        if let Some(parent) = path.parent() {
            paths::ensure_dir(parent)?;
        }
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
            .filter(|s| s.is_enabled_for(AgentKind::Cursor))
        {
            map.insert(s.name.clone(), s.config.clone());
        }
        root.insert("mcpServers".into(), Value::Object(map));
        atomic_write(&path, &serde_json::to_vec_pretty(&Value::Object(root))?)
    }

    fn skill_registry_target(&self, settings: &Settings) -> Option<PathBuf> {
        paths::cursor_config_dir(settings)
            .ok()
            .map(|d| d.join("skills"))
    }
}
