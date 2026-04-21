//! On every launch, App should submit a background rescan of live skill
//! directories so skills installed externally (e.g. via the agent's own
//! CLI) surface without the user running Import.

use std::sync::Mutex;
use std::time::{Duration, Instant};

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

fn pump_until<F: FnMut(&mut App) -> bool>(app: &mut App, mut cond: F) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        app.tick();
        if cond(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("rescan task never finished");
}

#[test]
fn launch_picks_up_externally_installed_skill() {
    let (home, _g) = with_temp_home();

    // Pre-seed DB so auto_import_if_empty bails (it only runs on empty DBs).
    let db_path = home.path().join("db.sqlite");
    let db = Database::open(&db_path).unwrap();
    let settings = Settings::default();
    let secrets = SecretStore::default();

    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# seed").unwrap();
    skill_service::install_from_local(&db, &settings, src.path(), "seed", None).unwrap();

    // Now drop a new skill into claude's live skills dir — simulating a CLI
    // install that happened while liteconfig wasn't running.
    let claude_skills = home.path().join(".claude").join("skills");
    let foo = claude_skills.join("foo");
    std::fs::create_dir_all(&foo).unwrap();
    std::fs::write(foo.join("SKILL.md"), "# foo body").unwrap();

    // Open App — this fires rescan_live_skills_async.
    drop(db);
    let db = Database::open(&db_path).unwrap();
    let mut app = App::new(db, settings, secrets).unwrap();

    pump_until(&mut app, |a| a.tasks.running_count() == 0);

    let names: Vec<String> = app
        .db
        .list_skills()
        .unwrap()
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert!(
        names.iter().any(|n| n == "foo"),
        "rescan should pick up externally-installed skill, got {names:?}"
    );
    // Also enabled for Claude (the live dir it came from).
    let foo_row = app
        .db
        .list_skills()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "foo")
        .unwrap();
    assert!(foo_row.is_enabled_for(AgentKind::Claude));
}
