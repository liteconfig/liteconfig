//! Register a local-path "repo" with a fake skills tree; sync; assert DB
//! upserts + repo metadata; re-sync stays idempotent.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::services::skill_repo_service;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

fn seed_repo(root: &std::path::Path, skills: &[&str]) {
    for s in skills {
        let d = root.join(s);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), format!("# {s}\n\nbody of {s}\n")).unwrap();
    }
}

#[test]
fn add_sync_and_resync_local_repo() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let repo_dir = tempfile::tempdir().unwrap();
    seed_repo(repo_dir.path(), &["alpha", "beta"]);

    let added = skill_repo_service::add(&db, repo_dir.path().to_str().unwrap()).unwrap();
    assert_eq!(added.url, repo_dir.path().to_string_lossy());
    assert_eq!(added.skill_count, 0);
    assert!(added.last_synced_at.is_none());

    let after = skill_repo_service::sync(&db, &added.id).unwrap();
    assert_eq!(after.skill_count, 2);
    assert!(after.last_synced_at.is_some());

    let skills = db.list_skills().unwrap();
    let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"alpha"), "found: {names:?}");
    assert!(names.contains(&"beta"), "found: {names:?}");

    // Re-sync is a no-op for skill count in DB (same name + same hash → no
    // new rows inserted).
    skill_repo_service::sync(&db, &added.id).unwrap();
    assert_eq!(db.list_skills().unwrap().len(), 2);
}

#[test]
fn parse_github_shorthand_adds_record() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r = skill_repo_service::add(&db, "anthropic/cookbook").unwrap();
    assert_eq!(r.name, "anthropic/cookbook");
    assert_eq!(r.owner.as_deref(), Some("anthropic"));
    assert_eq!(r.repo, "cookbook");
    assert_eq!(r.branch, "main");
    assert_eq!(r.url, "https://github.com/anthropic/cookbook.git");

    assert_eq!(db.list_skill_repos().unwrap().len(), 1);
}

#[test]
fn remove_deletes_repo_record() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r = skill_repo_service::add(&db, "anthropic/cookbook").unwrap();
    assert!(db.get_skill_repo(&r.id).unwrap().is_some());
    skill_repo_service::remove(&db, &r.id).unwrap();
    assert!(db.get_skill_repo(&r.id).unwrap().is_none());
}

#[test]
fn rejects_garbage_input() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    assert!(skill_repo_service::add(&db, "").is_err());
    assert!(skill_repo_service::add(&db, "not a repo at all").is_err());
}
