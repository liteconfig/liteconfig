//! Exercises the Skills tab state machine: install a skill, toggle an agent
//! via the popup, commit, and confirm the skill_service side-effects landed.
//!
//! We don't drive the TUI; we call App methods directly — the same path the
//! key-event dispatcher takes.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_service;
use liteconfig_core::settings::Settings;

use liteconfig_tui::app::App;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

#[test]
fn agent_popup_commit_syncs_skill_to_new_agent() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    // Stage a local skill on disk + in the DB.
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# hello").unwrap();
    let skill =
        skill_service::install_from_local(&db, &settings, src.path(), "popup-demo", None).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.skills_view.skills.len(), 1);

    // Open popup, toggle Claude on, commit.
    app.open_agent_popup_for_focused();
    assert!(app.agent_popup.is_some());
    // Cursor starts at 0 == Claude in ALL_AGENT_KINDS order.
    app.agent_popup_toggle();
    app.agent_popup_commit();
    assert!(app.agent_popup.is_none());

    // Verify the skill now reports Claude enabled, and the materialized dir
    // exists under Claude's skill registry.
    let reloaded = skill_service::get(&app.db, &skill.id).unwrap();
    assert!(
        reloaded.is_enabled_for(AgentKind::Claude),
        "Claude should be enabled after popup commit"
    );
    let adapter = liteconfig_core::agents::for_kind(AgentKind::Claude).unwrap();
    let materialized = adapter
        .skill_registry_target(&app.settings)
        .unwrap()
        .join(&skill.id);
    assert!(
        materialized.exists(),
        "expected materialized skill at {:?}",
        materialized
    );
}

#[test]
fn sync_all_materializes_enabled_skills() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    // Two skills — one enabled for Gemini, one disabled everywhere.
    let src_a = tempfile::tempdir().unwrap();
    std::fs::write(src_a.path().join("SKILL.md"), "# A").unwrap();
    let a = skill_service::install_from_local(&db, &settings, src_a.path(), "alpha", None).unwrap();
    skill_service::set_enabled(&db, &a.id, AgentKind::Gemini, true).unwrap();

    let src_b = tempfile::tempdir().unwrap();
    std::fs::write(src_b.path().join("SKILL.md"), "# B").unwrap();
    let _b =
        skill_service::install_from_local(&db, &settings, src_b.path(), "bravo", None).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    app.sync_all_skills();

    let gemini = liteconfig_core::agents::for_kind(AgentKind::Gemini).unwrap();
    let target = gemini.skill_registry_target(&app.settings).unwrap();
    assert!(target.join(&a.id).exists(), "alpha should be in Gemini dir");
    assert!(
        !target.join(&_b.id).exists(),
        "bravo should not be materialized — no agent enabled"
    );
}

#[test]
fn cycle_method_rotates_through_values() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "hi").unwrap();
    let s = skill_service::install_from_local(&db, &settings, src.path(), "rot", None).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    // Default on install is Inherit.
    assert_eq!(app.focused_sync_method_label(), "inherit");
    app.cycle_focused_skill_method();
    assert_eq!(app.focused_sync_method_label(), "auto");
    app.cycle_focused_skill_method();
    assert_eq!(app.focused_sync_method_label(), "symlink");

    // And the DB row reflects it.
    let reloaded = skill_service::get(&app.db, &s.id).unwrap();
    assert_eq!(reloaded.sync_method.as_str(), "symlink");
}
