//! OpenCode agent adapter.
//!
//! Live config: `~/.config/opencode/opencode.json` (JSON/JSONC).
//! MCP list:    inline under the `mcp` key of the same file. See
//!              https://opencode.ai/docs/mcp-servers/ and
//!              https://opencode.ai/docs/config/.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::agents::claude::deep_merge;
use crate::agents::{AgentAdapter, AgentPaths};
use crate::fs_util::{atomic_write, read_to_string};
use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::profile::Profile;
use crate::paths;
use crate::settings::Settings;
use crate::{Error, Result};

pub struct OpencodeAdapter;

/// Strip `//` line comments from a JSONC source. Quick-and-dirty — OpenCode
/// documents JSONC support but most configs are plain JSON, so we keep this
/// scoped to end-of-line comments and ignore block comments.
fn strip_jsonc_line_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        // Heuristic: only drop `//` when it's outside a quoted string. Because
        // valid JSON URLs already escape slashes, a bare `//` outside quotes
        // is overwhelmingly a comment.
        if let Some(idx) = find_bare_double_slash(line) {
            out.push_str(&line[..idx]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn find_bare_double_slash(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];
        if c == b'\\' {
            i += 2;
            continue;
        }
        if c == b'"' {
            in_str = !in_str;
        } else if !in_str && c == b'/' && bytes[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_jsonc(path: &std::path::Path) -> Result<Value> {
    let raw = read_to_string(path)?;
    let cleaned = strip_jsonc_line_comments(&raw);
    serde_json::from_str(&cleaned).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

impl AgentAdapter for OpencodeAdapter {
    fn kind(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn paths(&self, settings: &Settings) -> Result<AgentPaths> {
        let dir = paths::opencode_config_dir(settings)?;
        Ok(AgentPaths {
            live_settings: paths::opencode_settings_path(settings)?,
            // MCP lives under the `mcp` key of opencode.json — no separate file.
            mcp_file: Some(paths::opencode_settings_path(settings)?),
            rule_file: Some(dir.join("AGENTS.md")),
            skills_dir: Some(dir.join("skills")),
            sessions_dir: None,
            extra: vec![],
        })
    }

    fn read_live(&self, settings: &Settings) -> Result<Option<Value>> {
        let path = paths::opencode_settings_path(settings)?;
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(parse_jsonc(&path)?))
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
        let path = paths::opencode_settings_path(settings)?;
        if let Some(parent) = path.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&merged)?;
        atomic_write(&path, &bytes)
    }

    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>> {
        let path = paths::opencode_settings_path(settings)?;
        if !path.exists() {
            return Ok(vec![]);
        }
        let root = parse_jsonc(&path)?;
        let map = root
            .get("mcp")
            .and_then(|x| x.as_object())
            .cloned()
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let mut out = Vec::with_capacity(map.len());
        for (name, cfg) in map {
            // OpenCode uses { type: "local", command: [...], environment }
            // or { type: "remote", url, headers }. Normalize to our
            // Claude-compatible { command, args, env, url } shape so the
            // existing UI + sync code doesn't need a special case.
            let normalized = normalize_opencode_to_lite(&cfg);
            let mut enabled = std::collections::BTreeMap::new();
            enabled.insert(AgentKind::OpenCode, true);
            out.push(McpServer {
                id: uuid::Uuid::new_v4().to_string(),
                name,
                config: normalized,
                enabled,
                created_at: now,
                updated_at: now,
            });
        }
        Ok(out)
    }

    fn write_mcp(&self, settings: &Settings, servers: &[McpServer]) -> Result<()> {
        let path = paths::opencode_settings_path(settings)?;
        let existing: Value = if path.exists() {
            parse_jsonc(&path)?
        } else {
            json!({})
        };
        let mut root = existing.as_object().cloned().unwrap_or_default();
        let mut map = Map::new();
        for s in servers
            .iter()
            .filter(|s| s.is_enabled_for(AgentKind::OpenCode))
        {
            map.insert(s.name.clone(), normalize_lite_to_opencode(&s.config));
        }
        root.insert("mcp".into(), Value::Object(map));
        if let Some(parent) = path.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&Value::Object(root))?;
        atomic_write(&path, &bytes)
    }

    fn skill_registry_target(&self, settings: &Settings) -> Option<PathBuf> {
        paths::opencode_config_dir(settings)
            .ok()
            .map(|d| d.join("skills"))
    }
}

/// Convert an OpenCode MCP entry to liteconfig's internal shape. Local
/// servers expose `{ type: "local", command: ["npx", "-y", "pkg"],
/// environment: {...} }`; remote entries use `{ type: "remote", url,
/// headers }`. The internal shape is the Claude-compatible flat form.
fn normalize_opencode_to_lite(cfg: &Value) -> Value {
    let typ = cfg.get("type").and_then(|v| v.as_str()).unwrap_or("local");
    match typ {
        "remote" => json!({
            "url": cfg.get("url").cloned().unwrap_or(Value::Null),
            "headers": cfg.get("headers").cloned().unwrap_or(Value::Null),
        }),
        _ => {
            // command is either a single string or ["cmd", "arg", ...].
            let (command, args) = match cfg.get("command") {
                Some(Value::Array(a)) => {
                    let (head, tail) = a
                        .split_first()
                        .map_or((Value::Null, vec![]), |(h, t)| (h.clone(), t.to_vec()));
                    (head, Value::Array(tail))
                }
                Some(Value::String(s)) => (Value::String(s.clone()), Value::Array(vec![])),
                _ => (Value::Null, Value::Array(vec![])),
            };
            json!({
                "command": command,
                "args": args,
                "env": cfg.get("environment").cloned().unwrap_or(Value::Null),
            })
        }
    }
}

/// Inverse of `normalize_opencode_to_lite` — take the flat
/// `{ command, args, env, url }` shape and emit OpenCode's tagged form.
fn normalize_lite_to_opencode(cfg: &Value) -> Value {
    if let Some(url) = cfg.get("url").and_then(|v| v.as_str()) {
        return json!({
            "type": "remote",
            "url": url,
            "headers": cfg.get("headers").cloned().unwrap_or(Value::Null),
        });
    }
    let mut cmd_vec: Vec<Value> = Vec::new();
    if let Some(c) = cfg.get("command") {
        cmd_vec.push(c.clone());
    }
    if let Some(Value::Array(a)) = cfg.get("args") {
        cmd_vec.extend(a.iter().cloned());
    }
    json!({
        "type": "local",
        "command": cmd_vec,
        "environment": cfg.get("env").cloned().unwrap_or(Value::Null),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_keeps_urls_inside_strings() {
        let src = r#"{ "url": "https://example.com/x", "x": 1 } // trailing
        // line comment
        { "y": 2 }"#;
        let out = strip_jsonc_line_comments(src);
        assert!(out.contains("https://example.com/x"));
        assert!(!out.contains("trailing"));
        assert!(!out.contains("line comment"));
    }

    #[test]
    fn normalize_roundtrip_local() {
        let oc = json!({
            "type": "local",
            "command": ["npx", "-y", "@modelcontextprotocol/server-memory"],
            "environment": { "FOO": "bar" }
        });
        let lite = normalize_opencode_to_lite(&oc);
        assert_eq!(lite["command"], "npx");
        assert_eq!(
            lite["args"],
            json!(["-y", "@modelcontextprotocol/server-memory"])
        );
        let back = normalize_lite_to_opencode(&lite);
        assert_eq!(back["type"], "local");
        assert_eq!(back["command"][0], "npx");
    }

    #[test]
    fn normalize_remote_preserves_url() {
        let oc = json!({ "type": "remote", "url": "https://a.example/mcp" });
        let lite = normalize_opencode_to_lite(&oc);
        assert_eq!(lite["url"], "https://a.example/mcp");
        let back = normalize_lite_to_opencode(&lite);
        assert_eq!(back["type"], "remote");
        assert_eq!(back["url"], "https://a.example/mcp");
    }
}
