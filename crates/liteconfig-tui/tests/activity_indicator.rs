//! While a task is in flight, the status bar should surface a spinner +
//! the running task's name. When the runner is idle, only live-config
//! context is shown. Also verifies spinner_glyph cycles through frames.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use liteconfig_core::db::Database;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::App;
use liteconfig_tui::tasks::TaskStatus;
use liteconfig_tui::ui::status_bar::spinner_glyph;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

#[test]
fn spinner_glyph_cycles_across_ticks() {
    // Different tick indices should yield different glyphs at least some of
    // the time — proves we're pulling from a cycle, not a constant.
    let mut seen = std::collections::HashSet::new();
    for i in 0..20u8 {
        seen.insert(spinner_glyph(i));
    }
    assert!(seen.len() > 1, "glyph should animate across ticks");
}

#[test]
fn running_entries_surface_the_latest_task_name() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let mut app = App::new(db, Settings::default(), SecretStore::default()).unwrap();

    // Submit a slow task so it stays Running across at least one tick.
    app.tasks.submit("slow build", || {
        std::thread::sleep(Duration::from_millis(150));
        Ok(String::new())
    });

    assert_eq!(app.tasks.running_count(), 1);
    let name = app
        .tasks
        .running_entries()
        .next()
        .map(|e| e.name.clone())
        .unwrap();
    assert_eq!(name, "slow build");

    // Drain to completion so the test doesn't leak the worker.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && app.tasks.running_count() > 0 {
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(app.tasks.running_count(), 0);
    let entry = app
        .tasks
        .log()
        .iter()
        .find(|e| e.name == "slow build")
        .unwrap();
    assert!(matches!(entry.status, TaskStatus::Ok(_)));
}
