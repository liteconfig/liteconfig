//! Drives the Rules tab state machine: insert rules, toggle agents via the
//! popup, commit, and confirm `rule_service::sync_all` wrote the bodies to
//! each adapter's rule file.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::model::rule::Rule;
use liteconfig_core::services::rule_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;

use liteconfig_tui::app::App;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn fixture_rule(name: &str, body: &str) -> Rule {
    let now = chrono::Utc::now().timestamp_millis();
    Rule {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        body: body.to_string(),
        enabled: Default::default(),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn popup_commit_enables_rule_and_writes_file() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    rule_service::upsert(&db, fixture_rule("style-guide", "use rustfmt")).unwrap();
    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.rules_view.rules.len(), 1);

    app.open_agent_popup_for_focused_rule();
    assert!(app.agent_popup.is_some());
    app.agent_popup_toggle();
    app.agent_popup_commit();
    assert!(app.agent_popup.is_none());

    let reloaded = &app.rules_view.rules[0];
    assert!(*reloaded.enabled.get(&AgentKind::Claude).unwrap_or(&false));

    let path = liteconfig_core::agents::for_kind(AgentKind::Claude)
        .unwrap()
        .paths(&app.settings)
        .unwrap()
        .rule_file
        .expect("claude should have a rule file path");
    assert!(path.exists(), "claude rule file should exist at {:?}", path);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("use rustfmt"),
        "claude rule file should contain rule body: {body}"
    );
}

#[test]
fn delete_removes_focused_rule() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    rule_service::upsert(&db, fixture_rule("a", "A")).unwrap();
    rule_service::upsert(&db, fixture_rule("b", "B")).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.rules_view.rules.len(), 2);

    app.delete_focused_rule();
    assert_eq!(app.rules_view.rules.len(), 1);
}
