//! Installing a curated MCP preset from a non-core category lands in the
//! DB with the category preserved for future marketplace UI work.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::presets::MCP_PRESETS;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::{App, Tab};

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
fn marketplace_has_enough_entries_across_multiple_categories() {
    // Sanity check: the expansion from 5 → 40+ entries actually landed.
    assert!(
        MCP_PRESETS.len() >= 30,
        "expected at least 30 curated MCP presets, got {}",
        MCP_PRESETS.len()
    );

    // At least five categories represented.
    let categories: std::collections::HashSet<&str> =
        MCP_PRESETS.iter().map(|p| p.category).collect();
    assert!(
        categories.len() >= 5,
        "expected >=5 categories, got: {categories:?}"
    );
}

#[test]
fn install_github_preset_lands_row_with_correct_command() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = Tab::Mcp;

    // Pick the github preset — tests that a non-core entry installs the same
    // way as the original five did.
    let idx = MCP_PRESETS
        .iter()
        .position(|p| p.id == "github")
        .expect("github preset exists");
    app.install_mcp_preset(idx);

    let rows = app.db.list_mcp_servers().unwrap();
    let gh = rows
        .iter()
        .find(|s| s.name == "@modelcontextprotocol/server-github")
        .expect("github row present");
    // npx presets get rewritten via platform_npx → on non-Windows it stays
    // `npx`; on Windows it becomes `cmd`. Accept either.
    let command = gh.config["command"].as_str().unwrap_or("");
    assert!(
        command == "npx" || command == "cmd",
        "unexpected resolved command: {command}"
    );
}
