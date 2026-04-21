//! Help overlay: `?` toggles a per-tab overlay. Any keypress while it's
//! open closes it so the user can never get stuck inside.

use std::sync::Mutex;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use liteconfig_core::db::Database;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::{App, Tab};
use liteconfig_tui::events::handle_key;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn new_app() -> App {
    let db = Database::open_in_memory().unwrap();
    App::new(db, Settings::default(), SecretStore::default()).unwrap()
}

#[test]
fn question_mark_toggles_help_from_any_tab() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();

    for tab in Tab::ALL {
        app.set_active_tab(*tab);
        assert!(!app.show_help, "help should start closed on {:?}", tab);
        handle_key(&mut app, key(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.show_help, "? should open help on {:?}", tab);
        // Any keypress closes it.
        handle_key(&mut app, key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!app.show_help, "any key should close help on {:?}", tab);
    }
}

#[test]
fn help_swallows_the_closing_key_and_does_not_advance_tab() {
    let (_home, _g) = with_temp_home();
    let mut app = new_app();

    app.set_active_tab(Tab::Skills);
    handle_key(&mut app, key(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.show_help);
    assert_eq!(app.active_tab, Tab::Skills);

    // Tab would normally advance to the next tab — while help is open it
    // should only close the overlay.
    handle_key(&mut app, key(KeyCode::Tab, KeyModifiers::NONE));
    assert!(!app.show_help);
    assert_eq!(
        app.active_tab,
        Tab::Skills,
        "closing key must not also switch tabs"
    );
}
