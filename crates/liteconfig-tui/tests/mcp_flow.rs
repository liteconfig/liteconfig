//! Drives the MCP tab state machine: insert servers, open the agent popup,
//! commit, and confirm `mcp_service::sync_all` was triggered (i.e. each
//! adapter's MCP file now mentions the server).

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::services::mcp_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use serde_json::json;

use liteconfig_tui::app::App;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn fixture_server(name: &str) -> McpServer {
    let now = chrono::Utc::now().timestamp_millis();
    McpServer {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        config: json!({ "command": "uvx", "args": [name] }),
        enabled: Default::default(),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn popup_commit_flips_agent_and_resyncs() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    mcp_service::upsert(&db, fixture_server("memory")).unwrap();
    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.mcp_view.servers.len(), 1);

    app.open_agent_popup_for_focused_mcp();
    assert!(app.agent_popup.is_some());
    // Cursor 0 == Claude. Toggle on.
    app.agent_popup_toggle();
    app.agent_popup_commit();
    assert!(app.agent_popup.is_none());

    let reloaded = &app.mcp_view.servers[0];
    assert!(reloaded.is_enabled_for(AgentKind::Claude));

    // sync_all should have been called as part of commit — the Claude MCP
    // file should now mention the server.
    let path = liteconfig_core::paths::claude_mcp_path(&app.settings).unwrap();
    assert!(path.exists(), "claude mcp file should exist at {:?}", path);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("memory"),
        "claude mcp file should mention 'memory': {body}"
    );
}

#[test]
fn delete_removes_focused_row() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    mcp_service::upsert(&db, fixture_server("a")).unwrap();
    mcp_service::upsert(&db, fixture_server("b")).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.mcp_view.servers.len(), 2);

    app.delete_focused_mcp();
    assert_eq!(app.mcp_view.servers.len(), 1);
}
