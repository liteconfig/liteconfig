//! End-to-end integration: create a profile for each supported agent, switch
//! into it, confirm the live-config file is produced in the right location
//! with the right content, then backfill an external edit and confirm it
//! captured.
//!
//! Tests serialize on `LITECONFIG_HOME`, so they share a static Mutex.

use std::sync::Mutex;

use liteconfig_core::db::Database;
use liteconfig_core::model::agent::{AgentKind, ALL_AGENT_KINDS};
use liteconfig_core::model::profile::Profile;
use liteconfig_core::paths;
use liteconfig_core::services::profile_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use serde_json::{json, Value};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home() -> (tempfile::TempDir, std::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("LITECONFIG_HOME", dir.path());
    (dir, guard)
}

#[test]
fn claude_roundtrip() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let mut settings = Settings::default();
    let secrets = SecretStore::default();

    let p = Profile::new(
        AgentKind::Claude,
        "primary",
        json!({ "theme": "dark", "env": { "k": "v" } }),
    );
    db.upsert_profile(&p).unwrap();
    profile_service::switch(&db, &mut settings, &secrets, AgentKind::Claude, &p.id).unwrap();

    let live = paths::claude_settings_path(&settings).unwrap();
    assert!(live.exists(), "Claude live config at {:?}", live);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&live).unwrap()).unwrap();
    assert_eq!(v["theme"], "dark");
}

#[test]
fn gemini_roundtrip() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let mut settings = Settings::default();
    let secrets = SecretStore::default();

    let p = Profile::new(
        AgentKind::Gemini,
        "personal",
        json!({ "model": "gemini-1.5-pro", "temperature": 0.4 }),
    );
    db.upsert_profile(&p).unwrap();
    profile_service::switch(&db, &mut settings, &secrets, AgentKind::Gemini, &p.id).unwrap();

    let live = paths::gemini_settings_path(&settings).unwrap();
    assert!(live.exists(), "Gemini live config at {:?}", live);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&live).unwrap()).unwrap();
    assert_eq!(v["model"], "gemini-1.5-pro");
}

#[test]
fn codex_roundtrip_with_auth() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let mut settings = Settings::default();
    let mut secrets = SecretStore::default();
    secrets.put("codex-azure", "aa-real-token", "api_key");

    let p = Profile::new(
        AgentKind::Codex,
        "azure",
        json!({
            "config": { "model": "gpt-4o" },
            "auth": { "OPENAI_API_KEY": "@secret:codex-azure" }
        }),
    );
    db.upsert_profile(&p).unwrap();
    profile_service::switch(&db, &mut settings, &secrets, AgentKind::Codex, &p.id).unwrap();

    let cfg = paths::codex_config_path(&settings).unwrap();
    let auth = paths::codex_auth_path(&settings).unwrap();
    assert!(cfg.exists(), "Codex config at {:?}", cfg);
    assert!(auth.exists(), "Codex auth at {:?}", auth);

    let cfg_text = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        cfg_text.contains("gpt-4o"),
        "config TOML should contain model: {cfg_text}"
    );
    let auth_text = std::fs::read_to_string(&auth).unwrap();
    assert!(
        auth_text.contains("aa-real-token"),
        "auth TOML should contain resolved secret: {auth_text}"
    );

    // Auth file must be 0600.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&auth).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "auth file mode = {:o}", mode);
    }

    // DB still carries the placeholder, not the raw key.
    let stored = db.get_profile(AgentKind::Codex, &p.id).unwrap().unwrap();
    assert_eq!(
        stored.config["auth"]["OPENAI_API_KEY"],
        "@secret:codex-azure"
    );
}

#[test]
fn every_agent_registered() {
    // Safety net: every variant of AgentKind has a registered adapter.
    for k in ALL_AGENT_KINDS {
        liteconfig_core::agents::for_kind(*k)
            .unwrap_or_else(|_| panic!("no adapter registered for {:?}", k));
    }
}

#[test]
fn switch_between_two_profiles_backfills() {
    let (_home, _g) = with_temp_home();
    let db = Database::open_in_memory().unwrap();
    let mut settings = Settings::default();
    let secrets = SecretStore::default();

    let a = Profile::new(AgentKind::Gemini, "A", json!({ "mode": "alpha" }));
    let b = Profile::new(AgentKind::Gemini, "B", json!({ "mode": "beta" }));
    db.upsert_profile(&a).unwrap();
    db.upsert_profile(&b).unwrap();

    profile_service::switch(&db, &mut settings, &secrets, AgentKind::Gemini, &a.id).unwrap();

    // User hand-edits live file.
    let live = paths::gemini_settings_path(&settings).unwrap();
    std::fs::write(
        &live,
        serde_json::to_vec_pretty(&json!({ "mode": "alpha", "hand_edit": "preserved" })).unwrap(),
    )
    .unwrap();

    profile_service::switch(&db, &mut settings, &secrets, AgentKind::Gemini, &b.id).unwrap();

    let a_back = db.get_profile(AgentKind::Gemini, &a.id).unwrap().unwrap();
    assert_eq!(a_back.config["hand_edit"], "preserved");
    let live_text = std::fs::read_to_string(&live).unwrap();
    let live_v: Value = serde_json::from_str(&live_text).unwrap();
    assert_eq!(live_v["mode"], "beta");
}
