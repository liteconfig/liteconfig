//! Method picker popup on the Skills tab: open → move → commit writes the
//! chosen `SyncMethod` to the DB and resyncs the row; cancel discards.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_service;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::{App, METHOD_POPUP_CHOICES};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn fresh_app_with_one_skill() -> (App, String) {
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# s").unwrap();
    let skill =
        skill_service::install_from_local(&db, &settings, src.path(), "pick-me", None).unwrap();
    let id = skill.id.clone();
    let app = App::new(db, settings, secrets).unwrap();
    (app, id)
}

#[test]
fn commit_writes_chosen_method_to_db() {
    let (_home, _g) = with_temp_home();
    let (mut app, id) = fresh_app_with_one_skill();

    app.open_method_popup_for_focused();
    let popup = app.method_popup.as_ref().expect("popup open");
    // Default is Inherit, which is index 3 in METHOD_POPUP_CHOICES.
    let inherit_idx = METHOD_POPUP_CHOICES
        .iter()
        .position(|m| m.as_str() == "inherit")
        .unwrap();
    assert_eq!(popup.cursor, inherit_idx);

    // Move to `copy` (index 2) and commit.
    let copy_idx = METHOD_POPUP_CHOICES
        .iter()
        .position(|m| m.as_str() == "copy")
        .unwrap();
    let delta = copy_idx as i32 - inherit_idx as i32;
    app.method_popup_move(delta);
    app.method_popup_commit();

    assert!(app.method_popup.is_none(), "popup closed after commit");
    let reloaded = skill_service::get(&app.db, &id).unwrap();
    assert_eq!(reloaded.sync_method.as_str(), "copy");
}

#[test]
fn cancel_discards_changes() {
    let (_home, _g) = with_temp_home();
    let (mut app, id) = fresh_app_with_one_skill();

    app.open_method_popup_for_focused();
    app.method_popup_move(1);
    app.method_popup_cancel();

    assert!(app.method_popup.is_none());
    let reloaded = skill_service::get(&app.db, &id).unwrap();
    assert_eq!(
        reloaded.sync_method.as_str(),
        "inherit",
        "cancel should leave the method untouched"
    );
}

#[test]
fn move_wraps_around() {
    let (_home, _g) = with_temp_home();
    let (mut app, _) = fresh_app_with_one_skill();

    app.open_method_popup_for_focused();
    let start = app.method_popup.as_ref().unwrap().cursor;
    app.method_popup_move(-1);
    let wrapped = app.method_popup.as_ref().unwrap().cursor;
    assert_eq!(
        wrapped,
        (start + METHOD_POPUP_CHOICES.len() - 1) % METHOD_POPUP_CHOICES.len()
    );
}
