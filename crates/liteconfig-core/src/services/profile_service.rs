//! Profile lifecycle: list, create/update, delete, and the headline **switch**
//! operation that atomically swaps which profile is live for an agent.
//!
//! Key invariants:
//!   - Atomic file writes for live configs (via adapter impls).
//!   - **Backfill** — before overwriting the live config, read it and save the
//!     parsed value onto the previously-active profile row, so external edits
//!     don't silently vanish.
//!   - **Common-config merge** — the per-agent `common_config` row is deep-merged
//!     on top of the profile config before the write.
//!   - **Secret resolution** — `@secret:*` placeholders in the profile config
//!     are resolved against `secrets.local.json` immediately before the write,
//!     so raw secrets never touch the DB.

use serde_json::Value;

use crate::agents::{self, AgentAdapter};
use crate::db::Database;
use crate::model::agent::AgentKind;
use crate::model::profile::Profile;
use crate::services::secrets_service::{self, SecretStore};
use crate::settings::Settings;
use crate::{Error, Result};

/// List every profile for an agent, alphabetized by name.
pub fn list(db: &Database, agent: AgentKind) -> Result<Vec<Profile>> {
    db.list_profiles(agent)
}

/// Insert or update a profile. Bumps `updated_at`.
pub fn upsert(db: &Database, mut profile: Profile) -> Result<Profile> {
    profile.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_profile(&profile)?;
    Ok(profile)
}

/// Delete a profile. If it's the currently-active profile for its agent,
/// clear the device-local pointer too.
pub fn delete(db: &Database, settings: &mut Settings, agent: AgentKind, id: &str) -> Result<()> {
    db.delete_profile(agent, id)?;
    if settings.current_profile_for(agent) == Some(id) {
        settings.set_current_profile(agent, None);
        settings.save()?;
    }
    Ok(())
}

/// Duplicate a profile, returning the new row. The copy is not made active.
pub fn duplicate(db: &Database, agent: AgentKind, id: &str) -> Result<Profile> {
    let existing = db
        .get_profile(agent, id)?
        .ok_or_else(|| Error::ProfileNotFound {
            agent: agent.id().to_string(),
            id: id.to_string(),
        })?;
    let mut copy = Profile::new(
        agent,
        format!("{} (copy)", existing.name),
        existing.config.clone(),
    );
    copy.meta = existing.meta.clone();
    db.upsert_profile(&copy)?;
    Ok(copy)
}

/// Switch the live config for `agent` to the profile with `id`.
///
/// Performs in order:
///   1. **Backfill** — if a different profile is currently active, read the
///      live config and save it back onto that profile's row.
///   2. Resolve `@secret:*` placeholders against `secrets.local.json`.
///   3. Deep-merge the agent's common config on top of the resolved config.
///   4. Atomically write the live config via the agent adapter.
///   5. Update the device-local `current_profile` pointer.
///
/// On any error, the device pointer is left unchanged.
pub fn switch(
    db: &Database,
    settings: &mut Settings,
    secrets: &SecretStore,
    agent: AgentKind,
    id: &str,
) -> Result<()> {
    let adapter = agents::for_kind(agent)?;

    let target = db
        .get_profile(agent, id)?
        .ok_or_else(|| Error::ProfileNotFound {
            agent: agent.id().to_string(),
            id: id.to_string(),
        })?;

    backfill_if_needed(db, settings, agent, adapter, Some(id))?;

    let resolved = secrets_service::resolve(&target.config, secrets)?;
    let common = db.get_common_config(agent)?;

    // Build a redacted "profile for write" — adapter deep-merges common on top.
    let profile_for_write = Profile {
        config: resolved,
        ..target.clone()
    };
    adapter.write_live(settings, &profile_for_write, common.as_ref())?;

    settings.set_current_profile(agent, Some(id.to_string()));
    settings.save()?;
    Ok(())
}

/// Import existing live configs into the DB as profiles. Runs once per
/// agent that `supports_profiles()`: reads the agent's live config, redacts
/// any raw secrets, and inserts a profile named `<agent>-imported` if no
/// profile with that name exists for that agent yet.
///
/// Returns the list of profiles that were newly created (may be empty).
pub fn import_from_live(
    db: &Database,
    settings: &Settings,
    secrets: &SecretStore,
) -> Result<Vec<Profile>> {
    use crate::model::agent::ALL_AGENT_KINDS;
    let mut created = Vec::new();
    for agent in ALL_AGENT_KINDS.iter().copied() {
        if !agent.supports_profiles() {
            continue;
        }
        let adapter = agents::for_kind(agent)?;
        let Some(live) = adapter.read_live(settings)? else {
            continue;
        };
        let name = format!("{}-imported", agent.id());
        let existing = db.list_profiles(agent)?.into_iter().any(|p| p.name == name);
        if existing {
            continue;
        }
        let redacted = secrets_service::redact(&live, secrets);
        let profile = Profile::new(agent, name, redacted);
        db.upsert_profile(&profile)?;
        created.push(profile);
    }
    Ok(created)
}

/// Backfill the currently-active profile from the live config, without
/// switching. Useful if the user wants to "import" external edits before
/// touching anything else.
pub fn backfill_current(
    db: &Database,
    settings: &Settings,
    secrets: &SecretStore,
    agent: AgentKind,
) -> Result<()> {
    let adapter = agents::for_kind(agent)?;
    backfill_with_redaction(db, settings, agent, adapter, secrets)
}

fn backfill_if_needed(
    db: &Database,
    settings: &Settings,
    agent: AgentKind,
    adapter: &dyn AgentAdapter,
    switching_to: Option<&str>,
) -> Result<()> {
    let current = match settings.current_profile_for(agent) {
        Some(id) => id.to_string(),
        None => return Ok(()),
    };
    // Don't backfill onto the same row we're about to overwrite from — the
    // live file is (supposedly) already a materialization of that profile.
    if switching_to == Some(current.as_str()) {
        return Ok(());
    }

    let Some(mut existing) = db.get_profile(agent, &current)? else {
        return Ok(());
    };
    let Some(live) = adapter.read_live(settings)? else {
        return Ok(());
    };
    // Re-apply secret redaction so any raw keys present in the live file go
    // back into the DB as `@secret:*` references.
    let store = SecretStore::load_or_default().unwrap_or_default();
    existing.config = secrets_service::redact(&live, &store);
    existing.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_profile(&existing)?;
    Ok(())
}

fn backfill_with_redaction(
    db: &Database,
    settings: &Settings,
    agent: AgentKind,
    adapter: &dyn AgentAdapter,
    secrets: &SecretStore,
) -> Result<()> {
    let current = settings
        .current_profile_for(agent)
        .ok_or_else(|| Error::NoCurrentProfile(agent.id().to_string()))?
        .to_string();
    let mut existing = db
        .get_profile(agent, &current)?
        .ok_or_else(|| Error::ProfileNotFound {
            agent: agent.id().to_string(),
            id: current.clone(),
        })?;
    let live: Value = adapter.read_live(settings)?.unwrap_or(Value::Null);
    existing.config = secrets_service::redact(&live, secrets);
    existing.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_profile(&existing)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::agent::AgentKind;
    use crate::test_util::with_temp_home;
    use serde_json::json;

    #[test]
    fn switch_writes_live_config() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let mut settings = Settings::default();
        let secrets = SecretStore::default();

        let p = Profile::new(AgentKind::Claude, "primary", json!({ "theme": "dark" }));
        db.upsert_profile(&p).unwrap();

        switch(&db, &mut settings, &secrets, AgentKind::Claude, &p.id).unwrap();

        let live_path = crate::paths::claude_settings_path(&settings).unwrap();
        let text = std::fs::read_to_string(&live_path).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["theme"], "dark");
        assert_eq!(
            settings.current_profile_for(AgentKind::Claude),
            Some(p.id.as_str())
        );
    }

    #[test]
    fn switch_resolves_secrets() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let mut settings = Settings::default();
        let mut secrets = SecretStore::default();
        secrets.put("claude-primary", "sk-ant-real-xyz", "api_key");

        let p = Profile::new(
            AgentKind::Claude,
            "primary",
            json!({ "env": { "ANTHROPIC_API_KEY": "@secret:claude-primary" } }),
        );
        db.upsert_profile(&p).unwrap();

        switch(&db, &mut settings, &secrets, AgentKind::Claude, &p.id).unwrap();

        let live_path = crate::paths::claude_settings_path(&settings).unwrap();
        let text = std::fs::read_to_string(&live_path).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["env"]["ANTHROPIC_API_KEY"], "sk-ant-real-xyz");

        // DB still has the placeholder — raw key never persisted.
        let stored = db.get_profile(AgentKind::Claude, &p.id).unwrap().unwrap();
        assert_eq!(
            stored.config["env"]["ANTHROPIC_API_KEY"],
            "@secret:claude-primary"
        );
    }

    #[test]
    fn switch_merges_common_config() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let mut settings = Settings::default();
        let secrets = SecretStore::default();

        db.set_common_config(
            AgentKind::Claude,
            &json!({ "theme": "dark", "fontSize": 14 }),
            true,
        )
        .unwrap();

        let p = Profile::new(
            AgentKind::Claude,
            "primary",
            json!({ "theme": "solarized" }),
        );
        db.upsert_profile(&p).unwrap();

        switch(&db, &mut settings, &secrets, AgentKind::Claude, &p.id).unwrap();

        let live_path = crate::paths::claude_settings_path(&settings).unwrap();
        let text = std::fs::read_to_string(&live_path).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        // Common overlays profile, so `theme` should be "dark".
        assert_eq!(back["theme"], "dark");
        assert_eq!(back["fontSize"], 14);
    }

    #[test]
    fn switch_backfills_previous_profile() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let mut settings = Settings::default();
        let secrets = SecretStore::default();

        let a = Profile::new(AgentKind::Claude, "A", json!({ "theme": "light" }));
        let b = Profile::new(AgentKind::Claude, "B", json!({ "theme": "night" }));
        db.upsert_profile(&a).unwrap();
        db.upsert_profile(&b).unwrap();

        // Activate A first.
        switch(&db, &mut settings, &secrets, AgentKind::Claude, &a.id).unwrap();

        // Simulate user editing live file externally.
        let live_path = crate::paths::claude_settings_path(&settings).unwrap();
        std::fs::write(
            &live_path,
            serde_json::to_vec_pretty(&json!({ "theme": "light", "customKey": "edited-by-user" }))
                .unwrap(),
        )
        .unwrap();

        // Switch to B — A should be backfilled with the external edit.
        switch(&db, &mut settings, &secrets, AgentKind::Claude, &b.id).unwrap();

        let a_after = db.get_profile(AgentKind::Claude, &a.id).unwrap().unwrap();
        assert_eq!(a_after.config["customKey"], "edited-by-user");

        // And B is now live.
        let text = std::fs::read_to_string(&live_path).unwrap();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["theme"], "night");
    }

    #[test]
    fn switch_errors_on_unresolved_secret() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let mut settings = Settings::default();
        let secrets = SecretStore::default();

        let p = Profile::new(
            AgentKind::Claude,
            "primary",
            json!({ "api_key": "@secret:not-there" }),
        );
        db.upsert_profile(&p).unwrap();

        let err = switch(&db, &mut settings, &secrets, AgentKind::Claude, &p.id).unwrap_err();
        assert!(matches!(err, Error::UnresolvedSecret(_)));
        // Pointer not updated.
        assert_eq!(settings.current_profile_for(AgentKind::Claude), None);
    }
}
