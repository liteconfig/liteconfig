//! Two-phase delete on the Backup tab: the first `d` arms, the second
//! within [`DELETE_CONFIRM_WINDOW`] actually deletes. Any unrelated key
//! clears the arm.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::services::backup_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::App;
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

fn press(app: &mut App, c: char) {
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(c),
        crossterm::event::KeyModifiers::NONE,
    );
    events::handle_key(app, key);
}

#[test]
fn first_d_arms_second_d_deletes() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = liteconfig_tui::app::Tab::Backup;

    let snap = backup_service::create_snapshot().unwrap();
    app.reload_backups();
    assert_eq!(app.backup_view.snapshots.len(), 1);

    // First `d` → armed, snapshot still present.
    press(&mut app, 'd');
    assert!(app.backup_view.delete_armed_at.is_some());
    assert_eq!(app.backup_view.snapshots.len(), 1);
    assert!(snap.directory.exists(), "snapshot dir should still exist");

    // Second `d` → deletes.
    press(&mut app, 'd');
    assert!(app.backup_view.delete_armed_at.is_none());
    assert!(
        !snap.directory.exists(),
        "snapshot dir should be gone after the second d"
    );
    assert_eq!(app.backup_view.snapshots.len(), 0);
}

#[test]
fn non_d_key_clears_the_delete_arm() {
    let (home, _g) = with_temp_home();
    let mut app = new_app(home.path());
    app.active_tab = liteconfig_tui::app::Tab::Backup;

    let snap = backup_service::create_snapshot().unwrap();
    app.reload_backups();

    press(&mut app, 'd');
    assert!(app.backup_view.delete_armed_at.is_some());

    // Any other backup-tab key clears the arm.
    press(&mut app, 'j');
    assert!(app.backup_view.delete_armed_at.is_none());

    // A single `d` now is back to arming, not deleting.
    press(&mut app, 'd');
    assert!(snap.directory.exists(), "single d should not delete");
    assert_eq!(app.backup_view.snapshots.len(), 1);
}

#[test]
fn delete_service_refuses_traversal() {
    let (_home, _g) = with_temp_home();
    assert!(backup_service::delete_snapshot("").is_err());
    assert!(backup_service::delete_snapshot("..").is_err());
    assert!(backup_service::delete_snapshot("foo/bar").is_err());
    // Missing is a no-op.
    assert!(backup_service::delete_snapshot("does-not-exist-20250101").is_ok());
}
