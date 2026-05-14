//! End-to-end plugin install via a local directory source. Bundled skills
//! should flow into the main `skills` table and counts should be populated
//! on the plugin row.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::plugin::PluginSource;
use liteconfig_core::services::plugin_service;
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

fn seed_plugin(root: &std::path::Path) {
    // Minimal Claude Code plugin layout: a manifest + 2 skills + 1 command + 1 agent + 1 MCP server.
    std::fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    std::fs::write(
        root.join(".claude-plugin/plugin.json"),
        r#"{"name":"demo-bundle","description":"Tiny test plugin"}"#,
    )
    .unwrap();

    for s in ["foo", "bar"] {
        let d = root.join("skills").join(s);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), format!("# {s}\n")).unwrap();
    }

    std::fs::create_dir_all(root.join("commands")).unwrap();
    std::fs::write(root.join("commands/hello.md"), "# hello\nSay hi").unwrap();

    std::fs::create_dir_all(root.join("agents")).unwrap();
    std::fs::write(root.join("agents/codey.md"), "Codey subagent").unwrap();

    std::fs::write(
        root.join(".mcp.json"),
        r#"{"mcpServers":{"memory":{"command":"npx","args":["-y","memory"]}}}"#,
    )
    .unwrap();
}

#[test]
fn local_plugin_install_lands_skills_and_counts_resources() {
    let (home, _g) = with_temp_home();

    let db = Database::open(&home.path().join("db.sqlite")).unwrap();
    let src = tempfile::tempdir().unwrap();
    seed_plugin(src.path());

    let plugin = plugin_service::install(
        &db,
        PluginSource::Local {
            path: src.path().to_path_buf(),
        },
        Some("demo-bundle"),
    )
    .unwrap();

    assert_eq!(plugin.name, "demo-bundle");
    assert_eq!(plugin.contents.skills, 2, "two skills bundled");
    assert_eq!(plugin.contents.commands, 1);
    assert_eq!(plugin.contents.agents, 1);
    assert_eq!(plugin.contents.mcp_servers, 1);

    // Skills should have landed in the main skills table.
    let skill_names: Vec<_> = db
        .list_skills()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert!(
        skill_names.contains(&"foo".to_string()),
        "found: {skill_names:?}"
    );
    assert!(skill_names.contains(&"bar".to_string()));
}

#[test]
fn plugins_view_populates_after_install() {
    let (home, _g) = with_temp_home();
    let db = Database::open(&home.path().join("db.sqlite")).unwrap();
    let src = tempfile::tempdir().unwrap();
    seed_plugin(src.path());
    plugin_service::install(
        &db,
        PluginSource::Local {
            path: src.path().to_path_buf(),
        },
        Some("demo-bundle"),
    )
    .unwrap();

    // Open an App on the same file-backed DB and confirm the view loads.
    let db = Database::open(&home.path().join("db.sqlite")).unwrap();
    let mut app = App::new(db, Settings::default(), SecretStore::default()).unwrap();
    app.active_tab = Tab::Plugins;
    assert_eq!(app.plugins_view.plugins.len(), 1);
    assert_eq!(app.plugins_view.plugins[0].contents.skills, 2);
}

#[test]
fn two_phase_delete_uninstalls_plugin() {
    let (home, _g) = with_temp_home();
    let db = Database::open(&home.path().join("db.sqlite")).unwrap();
    let src = tempfile::tempdir().unwrap();
    seed_plugin(src.path());
    plugin_service::install(
        &db,
        PluginSource::Local {
            path: src.path().to_path_buf(),
        },
        Some("demo-bundle"),
    )
    .unwrap();

    let db = Database::open(&home.path().join("db.sqlite")).unwrap();
    let mut app = App::new(db, Settings::default(), SecretStore::default()).unwrap();
    app.active_tab = Tab::Plugins;
    assert_eq!(app.plugins_view.plugins.len(), 1);

    // First press arms.
    app.delete_focused_plugin();
    assert_eq!(app.plugins_view.plugins.len(), 1);
    assert!(app.plugins_view.delete_armed_at.is_some());

    // Second press commits.
    app.delete_focused_plugin();
    assert_eq!(app.plugins_view.plugins.len(), 0);
}
