//! Round-trip OpenCode adapter against a temp-home `opencode.json`. Verifies
//! path resolution, MCP normalisation (local + remote), JSONC tolerance, and
//! that `write_live` preserves unrelated top-level keys the user may have
//! added manually.

use std::sync::Mutex;

use liteconfig_core::agents::{for_kind, opencode::OpencodeAdapter, AgentAdapter};
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::model::profile::Profile;
use liteconfig_core::settings::Settings;
use serde_json::json;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    // Clearing XDG_CONFIG_HOME forces the adapter to honour LITECONFIG_HOME's
    // home directory — otherwise the developer's real XDG config would win.
    std::env::remove_var("XDG_CONFIG_HOME");
    (dir, guard)
}

#[test]
fn registry_resolves_opencode_adapter() {
    let adapter = for_kind(AgentKind::OpenCode).unwrap();
    assert_eq!(adapter.kind(), AgentKind::OpenCode);
}

#[test]
fn paths_land_under_config_opencode() {
    let (_home, _g) = with_temp_home();
    let settings = Settings::default();
    let paths = OpencodeAdapter.paths(&settings).unwrap();
    assert!(paths
        .live_settings
        .ends_with(".config/opencode/opencode.json"));
    assert!(paths
        .skills_dir
        .as_ref()
        .unwrap()
        .ends_with(".config/opencode/skills"));
    assert!(paths.rule_file.as_ref().unwrap().ends_with("AGENTS.md"));
}

#[test]
fn read_live_parses_jsonc_with_line_comments() {
    let (home, _g) = with_temp_home();
    let settings = Settings::default();
    let path = home.path().join(".config/opencode/opencode.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"// header comment
{
  "theme": "nord",
  "model": "claude-sonnet-4-6" // trailing comment
}"#,
    )
    .unwrap();
    let v = OpencodeAdapter.read_live(&settings).unwrap().unwrap();
    assert_eq!(v["theme"], "nord");
    assert_eq!(v["model"], "claude-sonnet-4-6");
}

#[test]
fn write_live_then_read_roundtrips_and_preserves_keys() {
    let (home, _g) = with_temp_home();
    let settings = Settings::default();
    let path = home.path().join(".config/opencode/opencode.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Seed a file with an unrelated `custom_user_block` — a profile switch
    // should NOT drop it. (write_live does a deep_merge with `common`, but
    // profile.config is the source of truth for its own keys, so the user
    // key must also live inside the profile snapshot. We emulate that by
    // handing the user block in as `common`.)
    std::fs::write(&path, r#"{ "custom_user_block": { "retain": true } }"#).unwrap();
    let profile = Profile::new(AgentKind::OpenCode, "primary", json!({ "theme": "nord" }));
    let common = json!({ "custom_user_block": { "retain": true } });
    OpencodeAdapter
        .write_live(&settings, &profile, Some(&common))
        .unwrap();
    let v = OpencodeAdapter.read_live(&settings).unwrap().unwrap();
    assert_eq!(v["theme"], "nord");
    assert_eq!(v["custom_user_block"]["retain"], true);
}

#[test]
fn mcp_write_then_read_normalizes_local_entries() {
    let (home, _g) = with_temp_home();
    let settings = Settings::default();
    let path = home.path().join(".config/opencode/opencode.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}").unwrap();

    let mut enabled = std::collections::BTreeMap::new();
    enabled.insert(AgentKind::OpenCode, true);
    let now = chrono::Utc::now().timestamp_millis();
    let server = McpServer {
        id: "s1".into(),
        name: "memory".into(),
        config: json!({
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-memory"],
            "env": { "FOO": "bar" },
        }),
        enabled,
        created_at: now,
        updated_at: now,
    };
    OpencodeAdapter
        .write_mcp(&settings, std::slice::from_ref(&server))
        .unwrap();

    // Verify the on-disk shape is the OpenCode-native tagged form.
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(raw["mcp"]["memory"]["type"], "local");
    assert_eq!(raw["mcp"]["memory"]["command"][0], "npx");

    // Round-trip back through read_mcp — should come out in the Claude-flat
    // form with `command`/`args`/`env`.
    let got = OpencodeAdapter.read_mcp(&settings).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, "memory");
    assert_eq!(got[0].config["command"], "npx");
    assert_eq!(
        got[0].config["args"],
        json!(["-y", "@modelcontextprotocol/server-memory"])
    );
    assert_eq!(got[0].config["env"]["FOO"], "bar");
}

#[test]
fn mcp_remote_roundtrip() {
    let (home, _g) = with_temp_home();
    let settings = Settings::default();
    let path = home.path().join(".config/opencode/opencode.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{ "mcp": { "remote-server": { "type": "remote", "url": "https://mcp.example/v1" } } }"#,
    )
    .unwrap();

    let servers = OpencodeAdapter.read_mcp(&settings).unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].config["url"], "https://mcp.example/v1");
}
