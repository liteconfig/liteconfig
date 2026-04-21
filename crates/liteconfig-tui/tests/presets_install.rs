//! Installing a curated MCP preset populates `mcp_servers` with the right
//! command + args; opening the skill-repo chooser + committing populates
//! `skill_repos`. We skip the skill-repo install path in this test because
//! that one shells out to git clone — the scope here is verifying the popup
//! → App → DB wiring, not network.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::presets::MCP_PRESETS;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::{App, PresetsKind, PresetsPopup, Tab};
use liteconfig_tui::events;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn new_app(home: &std::path::Path) -> App {
    let db = Database::open(&home.join("db.sqlite")).unwrap();
    App::new(db, Settings::default(), SecretStore::default()).unwrap()
}

#[test]
fn install_mcp_preset_upserts_row_with_resolved_command() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = Tab::Mcp;

    // Find the fetch preset by id — it's the uvx one, no npx shim concern.
    let fetch_idx = MCP_PRESETS.iter().position(|p| p.id == "fetch").unwrap();
    app.install_mcp_preset(fetch_idx);

    let rows = app.db.list_mcp_servers().unwrap();
    let fetch = rows
        .iter()
        .find(|s| s.name == "mcp-server-fetch")
        .expect("preset row present");
    assert_eq!(
        fetch.config.get("command").and_then(|v| v.as_str()),
        Some("uvx")
    );
    let args = fetch
        .config
        .get("args")
        .and_then(|v| v.as_array())
        .expect("args array");
    assert_eq!(args[0].as_str(), Some("mcp-server-fetch"));

    // Installed preset must land disabled for every agent — user decides
    // per-agent enablement through the `a` popup.
    for on in fetch.enabled.values() {
        assert!(!on, "preset must install disabled");
    }
}

#[test]
fn popup_enter_commits_and_closes() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = Tab::Mcp;

    app.open_new_mcp_menu();
    assert!(matches!(
        app.presets_popup,
        Some(PresetsPopup {
            kind: PresetsKind::Mcp,
            ..
        })
    ));

    // Enter on the first row installs it.
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    events::handle_key(&mut app, key);

    assert!(app.presets_popup.is_none(), "Enter should close popup");
    assert_eq!(
        app.db.list_mcp_servers().unwrap().len(),
        1,
        "exactly one preset row inserted"
    );
}

#[test]
fn escape_cancels_preset_popup() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = Tab::Mcp;

    app.open_new_mcp_menu();
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    events::handle_key(&mut app, key);

    assert!(app.presets_popup.is_none());
    assert_eq!(app.db.list_mcp_servers().unwrap().len(), 0);
}
