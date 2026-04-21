//! TaskRunner integration: a file-backed DB lets `sync_all_skills` run
//! through the background worker. We poll `tick` until the task finishes
//! and assert the materialized output landed on disk.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_service;
use liteconfig_core::settings::Settings;
use liteconfig_tui::app::App;
use liteconfig_tui::tasks::{TaskRunner, TaskStatus};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

/// Drive the app's tick until `cond` returns true or the deadline elapses.
fn pump_until<F: FnMut(&mut App) -> bool>(app: &mut App, mut cond: F) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.tick();
        if cond(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("task never finished");
}

#[test]
fn sync_all_skills_runs_on_background_thread_and_reloads_view() {
    let (home, _g) = with_temp_home();

    let db_path = home.path().join("db.sqlite");
    let db = Database::open(&db_path).unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    // Two skills; enable one for Gemini so the sync has work to do.
    let src_a = tempfile::tempdir().unwrap();
    std::fs::write(src_a.path().join("SKILL.md"), "# A").unwrap();
    let a = skill_service::install_from_local(&db, &settings, src_a.path(), "alpha", None).unwrap();
    skill_service::set_enabled(&db, &a.id, AgentKind::Gemini, true).unwrap();

    let src_b = tempfile::tempdir().unwrap();
    std::fs::write(src_b.path().join("SKILL.md"), "# B").unwrap();
    skill_service::install_from_local(&db, &settings, src_b.path(), "bravo", None).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    assert!(app.db.path().is_some(), "file-backed DB should expose path");

    app.sync_all_skills();
    assert_eq!(
        app.tasks.running_count(),
        1,
        "task should be queued on the worker"
    );

    pump_until(&mut app, |a| a.tasks.running_count() == 0);

    // Background task landed the file for `alpha` under Gemini's skill dir.
    let gemini = liteconfig_core::agents::for_kind(AgentKind::Gemini).unwrap();
    let target = gemini.skill_registry_target(&app.settings).unwrap();
    assert!(
        target.join(&a.id).exists(),
        "alpha should be materialized under {target:?} after the background sync"
    );

    let entry = app
        .tasks
        .log()
        .iter()
        .find(|e| e.name == "Sync all skills")
        .expect("log has the entry");
    assert!(
        matches!(entry.status, TaskStatus::Ok(_)),
        "task status should be Ok, got {:?}",
        entry.status
    );
}

#[test]
fn sync_all_skills_falls_back_to_inline_for_in_memory_db() {
    let (_home, _g) = with_temp_home();

    // In-memory DB has no path → should not spawn a task, just run inline.
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# x").unwrap();
    let s = skill_service::install_from_local(&db, &settings, src.path(), "x", None).unwrap();
    skill_service::set_enabled(&db, &s.id, AgentKind::Claude, true).unwrap();

    let mut app = App::new(db, settings, secrets).unwrap();
    assert!(app.db.path().is_none());

    app.sync_all_skills();
    assert_eq!(
        app.tasks.running_count(),
        0,
        "in-memory DB should run inline, not queue a task"
    );

    let claude = liteconfig_core::agents::for_kind(AgentKind::Claude).unwrap();
    let target = claude.skill_registry_target(&app.settings).unwrap();
    assert!(target.join(&s.id).exists());
}

#[test]
fn running_count_reflects_live_work() {
    // Standalone TaskRunner test: one slow job keeps `running_count` at 1
    // until it finishes, then the drain transitions it to 0.
    let mut runner = TaskRunner::new();
    runner.submit("slow", || {
        std::thread::sleep(Duration::from_millis(120));
        Ok("done".to_string())
    });
    assert_eq!(runner.running_count(), 1);

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let done = runner.drain_completed();
        if !done.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(runner.running_count(), 0);
}
