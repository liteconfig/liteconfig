//! Rules tab `/` filter: case-insensitive substring match on `name` and
//! `body`. Focus resets when the filter string changes.

use std::collections::BTreeMap;
use std::sync::Mutex;

use liteconfig_core::db::Database;
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

fn add_rule(db: &Database, name: &str, body: &str) {
    let now = chrono::Utc::now().timestamp_millis();
    let rule = Rule {
        id: format!("id-{name}"),
        name: name.into(),
        body: body.into(),
        enabled: BTreeMap::new(),
        created_at: now,
        updated_at: now,
    };
    rule_service::upsert(db, rule).unwrap();
}

#[test]
fn filter_matches_name_and_body() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_rule(&db, "commit-style", "Write concise commit messages.");
    add_rule(&db, "safety", "Never use --force without review.");
    add_rule(&db, "testing", "Always run tests before committing.");

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.rules_view.rules.len(), 3);

    app.rules_filter_open();
    for c in "commit".chars() {
        app.rules_filter_push(c);
    }
    // Matches commit-style (name) + testing ("committing" in body).
    assert_eq!(
        app.filtered_rules_indices().len(),
        2,
        "commit should hit name + body"
    );

    app.rules_filter_clear();
    assert_eq!(app.filtered_rules_indices().len(), 3);
}

#[test]
fn filter_is_case_insensitive_and_focus_resets() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_rule(&db, "Alpha", "body one");
    add_rule(&db, "beta", "body two");
    add_rule(&db, "ALPHA-BIS", "body three");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.move_rules_focus(2);
    assert_eq!(app.rules_view.focused_idx, 2);

    app.rules_filter_open();
    for c in "ALPHA".chars() {
        app.rules_filter_push(c);
    }
    assert_eq!(app.filtered_rules_indices().len(), 2);
    assert_eq!(
        app.rules_view.focused_idx, 0,
        "typing into filter should reset focus"
    );
}

#[test]
fn enter_keeps_filter_esc_clears() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    add_rule(&db, "foo", "alpha");
    add_rule(&db, "bar", "beta");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.rules_filter_open();
    app.rules_filter_push('f');
    app.rules_filter_close_keep();
    assert!(!app.rules_view.filter_editing);
    assert_eq!(app.rules_view.filter, "f");
    assert_eq!(app.filtered_rules_indices().len(), 1);

    app.rules_filter_clear();
    assert!(app.rules_view.filter.is_empty());
    assert_eq!(app.filtered_rules_indices().len(), 2);
}
