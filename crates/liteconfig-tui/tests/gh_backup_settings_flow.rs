//! Settings tab → GitHub-backup section editing flow. Each edit must persist
//! to `~/.liteconfig/settings.json` so the `p` action on the Backup tab sees
//! the new values without a restart.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::{App, SettingsRow};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn new_app() -> App {
    let db = Database::open_in_memory().unwrap();
    App::new(db, Settings::default(), SecretStore::default()).unwrap()
}

#[test]
fn space_toggles_enabled_and_persists() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();
    assert!(!app.settings.github_backup.enabled);

    // Default focus is on the Enabled row.
    assert_eq!(app.settings_view.focused_row, Some(SettingsRow::GhEnabled));

    app.settings_toggle_focused();
    assert!(app.settings.github_backup.enabled);

    // Reload from disk — the flip must have been persisted.
    let on_disk = Settings::load_or_default().unwrap();
    assert!(
        on_disk.github_backup.enabled,
        "Space should persist to settings.json"
    );

    // Toggling again flips back off and re-saves.
    app.settings_toggle_focused();
    let on_disk = Settings::load_or_default().unwrap();
    assert!(!on_disk.github_backup.enabled);
}

#[test]
fn enter_on_repo_url_opens_input_and_commit_persists() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();

    // Walk focus to RepoUrl (one Down from default GhEnabled).
    app.move_settings_focus(1);
    assert_eq!(app.settings_view.focused_row, Some(SettingsRow::GhRepoUrl));

    app.settings_begin_edit();
    assert!(app.settings_view.input_buf.is_some());

    for c in "git@github.com:me/dotfiles.git".chars() {
        app.settings_input_push(c);
    }
    app.settings_input_commit();

    assert_eq!(
        app.settings.github_backup.repo_url,
        "git@github.com:me/dotfiles.git"
    );
    let on_disk = Settings::load_or_default().unwrap();
    assert_eq!(
        on_disk.github_backup.repo_url,
        "git@github.com:me/dotfiles.git"
    );
    assert!(
        app.settings_view.input_buf.is_none(),
        "Commit should close the input"
    );
}

#[test]
fn esc_cancels_edit_without_changing_settings() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();

    // Focus RepoUrl.
    app.move_settings_focus(1);
    app.settings_begin_edit();
    app.settings_input_push('x');
    app.settings_input_cancel();

    assert!(app.settings.github_backup.repo_url.is_empty());
    assert!(app.settings_view.input_buf.is_none());
}

#[test]
fn blank_branch_falls_back_to_main() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();
    app.settings.github_backup.branch = "feature".into();
    app.settings.save().unwrap();

    // Focus Branch (two Down from default GhEnabled).
    app.move_settings_focus(1);
    app.move_settings_focus(1);
    assert_eq!(app.settings_view.focused_row, Some(SettingsRow::GhBranch));

    app.settings_begin_edit();
    // Clear existing text by backspacing through all of it.
    for _ in 0..20 {
        app.settings_input_pop();
    }
    app.settings_input_commit();

    assert_eq!(
        app.settings.github_backup.branch, "main",
        "blank branch commit should fall back to 'main'"
    );
}

#[test]
fn text_row_ignores_toggle_and_bool_row_ignores_edit() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();

    // Space on a text row is a no-op (no panic, no change).
    app.move_settings_focus(1); // RepoUrl
    let before = app.settings.github_backup.repo_url.clone();
    app.settings_toggle_focused();
    assert_eq!(app.settings.github_backup.repo_url, before);

    // Enter on a bool row is a no-op.
    app.move_settings_focus(-1); // back to Enabled
    assert_eq!(app.settings_view.focused_row, Some(SettingsRow::GhEnabled));
    app.settings_begin_edit();
    assert!(
        app.settings_view.input_buf.is_none(),
        "Enter on a bool row shouldn't open the text input"
    );
}
