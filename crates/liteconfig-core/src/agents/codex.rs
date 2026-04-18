//! Codex agent adapter.
//!
//! Live config: `~/.codex/config` (TOML).
//! Credentials: `~/.codex/auth`   (TOML).

use std::path::PathBuf;

use serde_json::Value;

use crate::agents::{AgentAdapter, AgentPaths};
use crate::fs_util::{atomic_write, atomic_write_private, read_to_string};
use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::profile::Profile;
use crate::paths;
use crate::settings::Settings;
use crate::{Error, Result};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn paths(&self, settings: &Settings) -> Result<AgentPaths> {
        let dir = paths::codex_config_dir(settings)?;
        Ok(AgentPaths {
            live_settings: paths::codex_config_path(settings)?,
            mcp_file: Some(paths::codex_config_path(settings)?),
            rule_file: Some(dir.join("AGENTS.md")),
            skills_dir: None, // Codex has no native skill system
            sessions_dir: Some(dir.join("sessions")),
            extra: vec![paths::codex_auth_path(settings)?],
        })
    }

    fn read_live(&self, settings: &Settings) -> Result<Option<Value>> {
        let cfg_path = paths::codex_config_path(settings)?;
        let auth_path = paths::codex_auth_path(settings)?;
        if !cfg_path.exists() && !auth_path.exists() {
            return Ok(None);
        }
        let config = if cfg_path.exists() {
            toml_to_json(&read_to_string(&cfg_path)?)?
        } else {
            Value::Null
        };
        let auth = if auth_path.exists() {
            toml_to_json(&read_to_string(&auth_path)?)?
        } else {
            Value::Null
        };
        Ok(Some(serde_json::json!({
            "config": config,
            "auth": auth,
        })))
    }

    fn write_live(
        &self,
        settings: &Settings,
        profile: &Profile,
        common: Option<&Value>,
    ) -> Result<()> {
        // Profile config shape:
        //   { "config": { ... toml ... }, "auth": { ... toml ... } }
        let mut merged = profile.config.clone();
        if let Some(common) = common {
            merged = crate::agents::claude::deep_merge(merged, common.clone());
        }

        let config = merged.get("config").cloned().unwrap_or(Value::Null);
        let auth = merged.get("auth").cloned().unwrap_or(Value::Null);

        let cfg_path = paths::codex_config_path(settings)?;
        let auth_path = paths::codex_auth_path(settings)?;

        atomic_write(&cfg_path, json_to_toml(&config)?.as_bytes())?;
        atomic_write_private(&auth_path, json_to_toml(&auth)?.as_bytes())?;
        Ok(())
    }

    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>> {
        let cfg_path = paths::codex_config_path(settings)?;
        if !cfg_path.exists() {
            return Ok(vec![]);
        }
        let v = toml_to_json(&read_to_string(&cfg_path)?)?;
        let servers = v
            .get("mcp_servers")
            .and_then(|x| x.as_object())
            .cloned()
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let mut out = Vec::with_capacity(servers.len());
        for (name, cfg) in servers {
            let mut enabled = std::collections::BTreeMap::new();
            enabled.insert(AgentKind::Codex, true);
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
        let cfg_path = paths::codex_config_path(settings)?;
        let existing: Value = if cfg_path.exists() {
            toml_to_json(&read_to_string(&cfg_path)?)?
        } else {
            Value::Object(serde_json::Map::new())
        };
        let mut root = existing.as_object().cloned().unwrap_or_default();
        let mut map = serde_json::Map::new();
        for s in servers
            .iter()
            .filter(|s| s.is_enabled_for(AgentKind::Codex))
        {
            map.insert(s.name.clone(), s.config.clone());
        }
        root.insert("mcp_servers".into(), Value::Object(map));
        atomic_write(&cfg_path, json_to_toml(&Value::Object(root))?.as_bytes())
    }

    fn skill_registry_target(&self, _settings: &Settings) -> Option<PathBuf> {
        None
    }
}

fn toml_to_json(s: &str) -> Result<Value> {
    let t: toml::Value = toml::from_str(s)?;
    // toml::Value -> serde_json::Value via round-trip.
    let as_json = serde_json::to_value(t)?;
    Ok(as_json)
}

fn json_to_toml(v: &Value) -> Result<String> {
    // TOML can't represent null or top-level arrays, so normalize.
    let normalized = match v {
        Value::Null => Value::Object(serde_json::Map::new()),
        other => other.clone(),
    };
    let t: toml::Value = serde_json::from_value(normalized).map_err(Error::RawJson)?;
    Ok(toml::to_string_pretty(&t)?)
}
