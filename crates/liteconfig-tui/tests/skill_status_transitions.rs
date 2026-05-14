//! End-to-end drift detection for the Skills tab Status column.
//!
//! Before this pass every row rendered `unknown` forever, regardless of
//! whether it had been synced or mutated on disk. The `Skill::status()`
//! helper now projects `(content_hash, last_synced_hash)` into a real
//! 4-state enum — Unknown / Unsynced / InSync / Drifted — and this test
//! walks a fresh install through every transition.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::AgentKind;
use liteconfig_core::model::skill::SkillStatus;
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

fn find(app: &App, id: &str) -> liteconfig_core::model::skill::Skill {
    app.db
        .list_skills()
        .unwrap()
        .into_iter()
        .find(|s| s.id == id)
        .expect("skill row present")
}

#[test]
fn install_sync_edit_restores_status_signal() {
    let (_home, _g) = with_temp_home();

    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();

    // Seed one local skill — install_from_local already hashes so we
    // bypass Unknown and land straight on Unsynced.
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# alpha\n").unwrap();
    let skill =
        skill_service::install_from_local(&db, &settings, src.path(), "alpha", None).unwrap();
    skill_service::set_enabled(&db, &skill.id, AgentKind::Claude, true).unwrap();

    let app = App::new(db, settings, SecretStore::default()).unwrap();
    assert_eq!(find(&app, &skill.id).status(), SkillStatus::Unsynced);

    // Sync → last_synced_hash stamped → InSync.
    skill_service::sync_one(&app.db, &app.settings, &skill.id).unwrap();
    assert_eq!(find(&app, &skill.id).status(), SkillStatus::InSync);

    // Mutate the live dir + rehash → Drifted.
    let dir = find(&app, &skill.id).directory.clone();
    std::fs::write(dir.join("SKILL.md"), "# alpha (edited)\n").unwrap();
    // Force a hash recompute without a sync (drift check).
    let mut row = find(&app, &skill.id);
    row.content_hash = Some(liteconfig_core::fs_util::hash_directory(&row.directory).unwrap());
    app.db.upsert_skill(&row).unwrap();
    assert_eq!(find(&app, &skill.id).status(), SkillStatus::Drifted);

    // Re-sync heals the drift.
    skill_service::sync_one(&app.db, &app.settings, &skill.id).unwrap();
    assert_eq!(find(&app, &skill.id).status(), SkillStatus::InSync);
}

#[test]
fn recompute_missing_hashes_promotes_unknown_to_unsynced() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let settings = Settings::default();

    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("SKILL.md"), "# x\n").unwrap();
    let skill = skill_service::install_from_local(&db, &settings, src.path(), "x", None).unwrap();

    // Simulate the old "stuck on Unknown" state by blanking the hash.
    let mut row = db.get_skill(&skill.id).unwrap().unwrap();
    row.content_hash = None;
    db.upsert_skill(&row).unwrap();
    assert_eq!(row.status(), SkillStatus::Unknown);

    // Kick the recompute; status should transition to Unsynced.
    let n = skill_service::recompute_missing_hashes(&db).unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        db.get_skill(&skill.id).unwrap().unwrap().status(),
        SkillStatus::Unsynced
    );
}
