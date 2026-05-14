//! Branch-selection matrix for `skill_repo_service::add` — covers the
//! explicit override, the `#branch` suffix syntax, and the default fallback.
//! Regression guard for the `ComposioHQ/awesome-claude-skills` (master)
//! "invalid configuration" clone bug.
//!
//! We assert on the DB row (no network hit happens at `add` time), so the
//! tests stay hermetic.

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

#[test]
fn explicit_branch_override_wins() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r =
        skill_repo_service::add(&db, "ComposioHQ/awesome-claude-skills", Some("master")).unwrap();
    assert_eq!(r.branch, "master");
    assert_eq!(
        r.url,
        "https://github.com/ComposioHQ/awesome-claude-skills.git"
    );
}

#[test]
fn hash_suffix_is_parsed_as_branch_when_no_override() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r = skill_repo_service::add(&db, "anthropic/cookbook#dev", None).unwrap();
    assert_eq!(r.branch, "dev");
    // The URL should not include the `#dev` suffix.
    assert_eq!(r.url, "https://github.com/anthropic/cookbook.git");
    assert_eq!(r.repo, "cookbook");
}

#[test]
fn override_beats_hash_suffix() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    // If both are given, the explicit override wins — matches how presets
    // carry their branch metadata separately from the shorthand identifier.
    let r = skill_repo_service::add(&db, "anthropic/cookbook#dev", Some("release")).unwrap();
    assert_eq!(r.branch, "release");
}

#[test]
fn default_branch_is_main() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r = skill_repo_service::add(&db, "anthropic/cookbook", None).unwrap();
    assert_eq!(r.branch, "main");
}

#[test]
fn branch_works_for_https_url_too() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();

    let r = skill_repo_service::add(
        &db,
        "https://github.com/ComposioHQ/awesome-claude-skills.git#master",
        None,
    )
    .unwrap();
    assert_eq!(r.branch, "master");
    assert_eq!(
        r.url,
        "https://github.com/ComposioHQ/awesome-claude-skills.git"
    );
}
