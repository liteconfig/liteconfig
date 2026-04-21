//! Skills tab `/` filter: case-insensitive substring match on name and
//! description; focus and summary reflect the filtered set.

use std::sync::Mutex;

use liteconfig_core::db::Database;
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

fn install_skill(db: &Database, settings: &Settings, name: &str) {
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), format!("# {name}")).unwrap();
    skill_service::install_from_local(db, settings, src.path(), name, None).unwrap();
}

#[test]
fn filter_by_name_narrows_list_and_clears_restores() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    install_skill(&db, &settings, "seo-audit");
    install_skill(&db, &settings, "seo-meta");
    install_skill(&db, &settings, "infra-cleanup");

    let mut app = App::new(db, settings, secrets).unwrap();
    assert_eq!(app.skills_view.skills.len(), 3);
    assert_eq!(app.filtered_skill_indices().len(), 3);

    app.skills_filter_open();
    for c in "seo".chars() {
        app.skills_filter_push(c);
    }
    assert_eq!(
        app.filtered_skill_indices().len(),
        2,
        "two seo-prefixed skills"
    );

    // Esc clears filter and the full list comes back.
    app.skills_filter_clear();
    assert!(app.skills_view.filter.is_empty());
    assert_eq!(app.filtered_skill_indices().len(), 3);
}

#[test]
fn filter_is_case_insensitive() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    install_skill(&db, &settings, "Alpha");
    install_skill(&db, &settings, "beta");
    install_skill(&db, &settings, "ALPHACENTAURI");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.skills_filter_open();
    for c in "alpha".chars() {
        app.skills_filter_push(c);
    }
    assert_eq!(
        app.filtered_skill_indices().len(),
        2,
        "alpha matches Alpha + ALPHACENTAURI regardless of case"
    );
}

#[test]
fn backspace_edits_and_focus_resets() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    install_skill(&db, &settings, "one");
    install_skill(&db, &settings, "two");
    install_skill(&db, &settings, "three");

    let mut app = App::new(db, settings, secrets).unwrap();
    // Move focus down before filtering to exercise the reset.
    app.move_skill_focus(2);
    assert_eq!(app.skills_view.focused_idx, 2);

    app.skills_filter_open();
    for c in "twoo".chars() {
        app.skills_filter_push(c);
    }
    assert_eq!(app.filtered_skill_indices().len(), 0);
    assert_eq!(
        app.skills_view.focused_idx, 0,
        "typing into the filter should reset focus to 0 so highlight stays valid"
    );

    app.skills_filter_pop();
    assert_eq!(
        app.filtered_skill_indices().len(),
        1,
        "after backspacing the stray 'o', 'two' matches again"
    );
}

#[test]
fn close_keeps_filter_until_cleared() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    install_skill(&db, &settings, "foo");
    install_skill(&db, &settings, "bar");

    let mut app = App::new(db, settings, secrets).unwrap();
    app.skills_filter_open();
    app.skills_filter_push('f');
    app.skills_filter_close_keep();

    assert!(
        !app.skills_view.filter_editing,
        "Enter stops editing the filter"
    );
    assert_eq!(
        app.skills_view.filter, "f",
        "Enter preserves the filter substring"
    );
    assert_eq!(app.filtered_skill_indices().len(), 1);
}
