//! MCP tab `/` filter mirrors the Skills one: case-insensitive substring
//! match on `name` and the `command` string inside `config`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::services::mcp_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::App;
use serde_json::json;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn add_server(db: &Database, name: &str, command: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    let server = McpServer {
        id: format!("id-{name}"),
        name: name.into(),
        config: json!({ "command": command, "args": [] }),
        enabled: BTreeMap::new(),
        created_at: now,
        updated_at: now,
    };
    mcp_service::upsert(db, server).unwrap();
}

#[test]
fn filter_matches_name_and_command() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_server(&db, "github-mcp", "node");
    add_server(&db, "filesystem", "python");
    add_server(&db, "pytest-runner", "pytest");

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.mcp_view.servers.len(), 3);

    app.mcp_filter_open();
    for c in "py".chars() {
        app.mcp_filter_push(c);
    }
    // "py" matches pytest-runner (name) + filesystem (command=python).
    assert_eq!(
        app.filtered_mcp_indices().len(),
        2,
        "py should hit both name and command"
    );

    app.mcp_filter_clear();
    assert_eq!(app.filtered_mcp_indices().len(), 3);
}

#[test]
fn filter_is_case_insensitive_and_focus_resets() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_server(&db, "Alpha", "node");
    add_server(&db, "beta", "python");
    add_server(&db, "ALPHA-BIS", "node");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.move_mcp_focus(2);
    assert_eq!(app.mcp_view.focused_idx, 2);

    app.mcp_filter_open();
    for c in "ALPHA".chars() {
        app.mcp_filter_push(c);
    }
    assert_eq!(app.filtered_mcp_indices().len(), 2);
    assert_eq!(
        app.mcp_view.focused_idx, 0,
        "typing into filter should reset focus"
    );
}

#[test]
fn enter_keeps_filter_esc_clears() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_server(&db, "foo", "node");
    add_server(&db, "bar", "node");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.mcp_filter_open();
    app.mcp_filter_push('f');
    app.mcp_filter_close_keep();
    assert!(!app.mcp_view.filter_editing);
    assert_eq!(app.mcp_view.filter, "f");
    assert_eq!(app.filtered_mcp_indices().len(), 1);

    app.mcp_filter_clear();
    assert!(app.mcp_view.filter.is_empty());
    assert_eq!(app.filtered_mcp_indices().len(), 2);
}
