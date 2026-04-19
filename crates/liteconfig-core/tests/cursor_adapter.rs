//! Cursor adapter integration tests.
//!
//! Cursor has no profile concept, so `read_live`/`write_live` are no-ops.
//! It participates in MCP sync (`~/.cursor/mcp.json`) and rule import
//! (`~/.cursor/rules/*.mdc`).

use std::sync::Mutex;

use liteconfig_core::agents;
use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::services::rule_service;
use liteconfig_core::settings::Settings;
use serde_json::{json, Value};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

#[test]
fn cursor_read_live_is_none() {
    let (_home, _g) = with_temp_home();
    let settings = Settings::default();
    let adapter = agents::for_kind(AgentKind::Cursor).unwrap();
    assert!(adapter.read_live(&settings).unwrap().is_none());
}

#[test]
fn cursor_mcp_write_then_read_roundtrip() {
    let (_home, _g) = with_temp_home();
    let settings = Settings::default();
    let adapter = agents::for_kind(AgentKind::Cursor).unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    let mut enabled = std::collections::BTreeMap::new();
    enabled.insert(AgentKind::Cursor, true);
    let server = McpServer {
        id: "test-id".into(),
        name: "echo".into(),
        config: json!({ "command": "echo", "args": ["hi"] }),
        enabled,
        created_at: now,
        updated_at: now,
    };

    adapter.write_mcp(&settings, &[server.clone()]).unwrap();

    let path = liteconfig_core::paths::cursor_mcp_path(&settings).unwrap();
    assert!(path.exists());
    let text = std::fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["mcpServers"]["echo"]["command"], "echo");

    let read_back = adapter.read_mcp(&settings).unwrap();
    assert_eq!(read_back.len(), 1);
    assert_eq!(read_back[0].name, "echo");
}

#[test]
fn cursor_rule_import_picks_up_mdc_files() {
    let (_home, _g) = with_temp_home();
    let settings = Settings::default();
    let db = Database::open_in_memory().unwrap();

    let rules_dir = liteconfig_core::paths::cursor_rules_dir(&settings).unwrap();
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("style.mdc"), "Always write concise commits.").unwrap();
    std::fs::write(
        rules_dir.join("safety.mdc"),
        "Never use --force without review.",
    )
    .unwrap();

    // A non-.mdc file should be ignored.
    std::fs::write(rules_dir.join("README.txt"), "ignored").unwrap();

    let imported = rule_service::import_from_live(&db, &settings).unwrap();
    assert_eq!(imported.len(), 2, "expected 2 mdc rules, got {imported:?}");

    let names: Vec<String> = imported.iter().map(|r| r.name.clone()).collect();
    assert!(names.contains(&"style".to_string()));
    assert!(names.contains(&"safety".to_string()));

    for r in &imported {
        assert_eq!(r.enabled.get(&AgentKind::Cursor).copied(), Some(true));
    }
}

#[test]
fn rule_import_dedupes_identical_bodies_across_agents() {
    let (_home, _g) = with_temp_home();
    let settings = Settings::default();
    let db = Database::open_in_memory().unwrap();

    // Write the same body to Claude's CLAUDE.md and a Cursor .mdc.
    let body = "Write clear commit messages.\n";
    let claude_dir = liteconfig_core::paths::claude_config_dir(&settings).unwrap();
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("CLAUDE.md"), body).unwrap();

    let cursor_rules = liteconfig_core::paths::cursor_rules_dir(&settings).unwrap();
    std::fs::create_dir_all(&cursor_rules).unwrap();
    std::fs::write(cursor_rules.join("commits.mdc"), body).unwrap();

    let created = rule_service::import_from_live(&db, &settings).unwrap();
    // Only one row: second hit just flipped Cursor's enabled bit.
    assert_eq!(created.len(), 1);

    let all = db.list_rules().unwrap();
    assert_eq!(all.len(), 1);
    let r = &all[0];
    assert_eq!(r.enabled.get(&AgentKind::Claude).copied(), Some(true));
    assert_eq!(r.enabled.get(&AgentKind::Cursor).copied(), Some(true));
}
