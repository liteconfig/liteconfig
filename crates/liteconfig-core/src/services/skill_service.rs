//! Skills: install from local path or GitHub, toggle per-agent enablement,
//! and sync (symlink or copy) into each enabled agent's skills directory.
//!
//! This module is the headline non-profile feature. It's implemented in three
//! layers:
//!   - DB layer (via `Database`) — CRUD of the `skills` table.
//!   - Filesystem layer — materialize the skill directory under the storage
//!     root (`~/.liteconfig/skills/<id>` or `~/.agents/skills/<id>`), and sync
//!     it into each enabled agent's skill dir.
//!   - Orchestration (here) — glue the two together.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir as symlink;

use crate::agents;
use crate::db::Database;
use crate::fs_util::hash_directory;
use crate::model::agent::AgentKind;
use crate::model::skill::{Skill, SkillSource, StorageMode, SyncMethod};
use crate::paths::{ensure_dir, liteconfig_skills_dir, unified_skills_dir};
use crate::settings::Settings;
use crate::{Error, Result};

/// List every skill, alphabetized by name.
pub fn list(db: &Database) -> Result<Vec<Skill>> {
    db.list_skills()
}

/// Get a skill by id.
pub fn get(db: &Database, id: &str) -> Result<Skill> {
    db.get_skill(id)?
        .ok_or_else(|| Error::SkillNotFound(id.to_string()))
}

/// Install a skill from a local directory. The source is copied into the
/// storage root; after this the source may be deleted without breaking
/// anything.
pub fn install_from_local(
    db: &Database,
    settings: &Settings,
    source_dir: &Path,
    name: &str,
    description: Option<String>,
) -> Result<Skill> {
    if !source_dir.exists() || !source_dir.is_dir() {
        return Err(Error::InvalidConfig(format!(
            "skill source does not exist or is not a directory: {}",
            source_dir.display()
        )));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let dest = storage_root(settings)?.join(&id);
    ensure_dir(dest.parent().unwrap_or(Path::new("/")))?;
    copy_dir_recursive(source_dir, &dest)?;

    let now = chrono::Utc::now().timestamp_millis();
    let skill = Skill {
        id: id.clone(),
        name: name.to_string(),
        description,
        directory: dest.clone(),
        source: SkillSource::Local,
        sync_method: SyncMethod::Inherit,
        enabled: Default::default(),
        content_hash: Some(hash_directory(&dest)?),
        installed_at: now,
        updated_at: now,
    };
    db.upsert_skill(&skill)?;
    Ok(skill)
}

/// Delete a skill from the DB and remove its materialized directory. Also
/// unlinks/removes any materializations in agent skill dirs.
pub fn uninstall(db: &Database, settings: &Settings, id: &str) -> Result<()> {
    let skill = get(db, id)?;
    for agent in enabled_agents(&skill) {
        if let Some(dest) = agent_skill_path(agent, &skill, settings) {
            let _ = remove_path_if_exists(&dest);
        }
    }
    if skill.directory.exists() {
        std::fs::remove_dir_all(&skill.directory).map_err(|source| Error::Io {
            path: skill.directory.clone(),
            source,
        })?;
    }
    db.delete_skill(id)?;
    Ok(())
}

/// Toggle a single agent's enablement on a skill. Returns the updated skill.
pub fn set_enabled(db: &Database, id: &str, agent: AgentKind, enabled: bool) -> Result<Skill> {
    let mut skill = get(db, id)?;
    skill.enabled.insert(agent, enabled);
    skill.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_skill(&skill)?;
    Ok(skill)
}

/// Set the sync method for a skill.
pub fn set_sync_method(db: &Database, id: &str, method: SyncMethod) -> Result<Skill> {
    let mut skill = get(db, id)?;
    skill.sync_method = method;
    skill.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_skill(&skill)?;
    Ok(skill)
}

/// Scan each agent's skills directory for existing subdirs and register them
/// in the DB. Skills found in multiple agents' directories are de-duped by
/// name — the first-seen directory becomes canonical and subsequent hits
/// merely flip that agent's `enabled` bit. Idempotent: rerunning never
/// inserts a duplicate row for a name already present.
pub fn scan_from_live(db: &Database, settings: &Settings) -> Result<Vec<Skill>> {
    use std::collections::BTreeMap;
    let mut existing_by_name: BTreeMap<String, Skill> = db
        .list_skills()?
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();

    let mut created: Vec<Skill> = Vec::new();
    let now = chrono::Utc::now().timestamp_millis();

    for agent in crate::model::agent::ALL_AGENT_KINDS.iter().copied() {
        let adapter = agents::for_kind(agent)?;
        let Some(skills_dir) = adapter.paths(settings)?.skills_dir else {
            continue;
        };
        if !skills_dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };

            if let Some(existing) = existing_by_name.get_mut(&name) {
                if !existing.is_enabled_for(agent) {
                    existing.enabled.insert(agent, true);
                    existing.updated_at = now;
                    db.upsert_skill(existing)?;
                }
                continue;
            }

            let description = read_skill_description(&path);
            let mut enabled = std::collections::BTreeMap::new();
            enabled.insert(agent, true);
            let skill = Skill {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.clone(),
                description,
                directory: path.clone(),
                source: SkillSource::Local,
                sync_method: SyncMethod::Inherit,
                enabled,
                content_hash: None,
                installed_at: now,
                updated_at: now,
            };
            db.upsert_skill(&skill)?;
            existing_by_name.insert(name, skill.clone());
            created.push(skill);
        }
    }
    Ok(created)
}

fn read_skill_description(dir: &Path) -> Option<String> {
    for candidate in ["SKILL.md", "README.md", "readme.md"] {
        let p = dir.join(candidate);
        if let Ok(text) = std::fs::read_to_string(&p) {
            let first = text
                .lines()
                .map(|l| l.trim_start_matches('#').trim())
                .find(|l| !l.is_empty())?;
            return Some(first.to_string());
        }
    }
    None
}

/// Sync one skill: materialize it into every enabled agent's skills dir
/// (symlink or copy per resolved method). Agents where the skill is disabled
/// have any existing materialization removed.
pub fn sync_one(db: &Database, settings: &Settings, id: &str) -> Result<()> {
    let skill = get(db, id)?;
    sync_skill(&skill, settings)?;
    Ok(())
}

/// Sync many skills by id.
pub fn sync_many(db: &Database, settings: &Settings, ids: &[String]) -> Result<()> {
    for id in ids {
        sync_one(db, settings, id)?;
    }
    Ok(())
}

/// Sync every skill.
pub fn sync_all(db: &Database, settings: &Settings) -> Result<()> {
    for skill in db.list_skills()? {
        sync_skill(&skill, settings)?;
    }
    Ok(())
}

fn sync_skill(skill: &Skill, settings: &Settings) -> Result<()> {
    let method = resolve_method(skill.sync_method, settings);
    for agent in crate::model::agent::ALL_AGENT_KINDS {
        let enabled = skill.is_enabled_for(*agent);
        let Some(dest) = agent_skill_path(*agent, skill, settings) else {
            continue;
        };
        if !enabled {
            let _ = remove_path_if_exists(&dest);
            continue;
        }
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        remove_path_if_exists(&dest)?;
        match method {
            SyncMethod::Copy => copy_dir_recursive(&skill.directory, &dest)?,
            SyncMethod::Symlink | SyncMethod::Auto | SyncMethod::Inherit => {
                // `Auto` and `Inherit` both resolve down to either Symlink or
                // Copy, handled above; if we reach here treat as symlink.
                symlink(&skill.directory, &dest).map_err(|source| Error::Io {
                    path: dest.clone(),
                    source,
                })?;
            }
        }
    }
    Ok(())
}

fn resolve_method(method: SyncMethod, settings: &Settings) -> SyncMethod {
    match method {
        SyncMethod::Inherit => settings.skill_sync_method_default,
        SyncMethod::Auto => {
            // Prefer symlinks on Unix; on platforms without symlink privileges
            // the caller can override to Copy in settings.
            SyncMethod::Symlink
        }
        other => other,
    }
}

fn storage_root(settings: &Settings) -> Result<PathBuf> {
    let root = match settings.skill_storage_location {
        StorageMode::Liteconfig => liteconfig_skills_dir()?,
        StorageMode::Unified => unified_skills_dir()?,
    };
    ensure_dir(&root)?;
    Ok(root)
}

fn agent_skill_path(agent: AgentKind, skill: &Skill, settings: &Settings) -> Option<PathBuf> {
    let adapter = agents::for_kind(agent).ok()?;
    let target = adapter.skill_registry_target(settings)?;
    Some(target.join(&skill.id))
}

fn enabled_agents(skill: &Skill) -> Vec<AgentKind> {
    skill
        .enabled
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect()
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    // `symlink_metadata` so we don't follow a broken symlink.
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let result = if meta.file_type().is_symlink() || meta.is_file() {
                std::fs::remove_file(path)
            } else {
                std::fs::remove_dir_all(path)
            };
            result.map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    ensure_dir(dst)?;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| Error::InvalidConfig(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            ensure_dir(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                ensure_dir(parent)?;
            }
            std::fs::copy(entry.path(), &target).map_err(|source| Error::Io {
                path: target.clone(),
                source,
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_temp_home;

    #[test]
    fn install_and_list_local_skill() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let settings = Settings::default();

        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("SKILL.md"), "# hello").unwrap();

        let s = install_from_local(&db, &settings, src.path(), "my-skill", None).unwrap();
        assert_eq!(s.name, "my-skill");
        assert!(s.directory.exists());
        assert!(s.directory.join("SKILL.md").exists());

        let listed = list(&db).unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn sync_enables_and_disables() {
        let _home = with_temp_home();
        let db = Database::open_in_memory().unwrap();
        let settings = Settings::default();

        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("SKILL.md"), "hello").unwrap();

        let s = install_from_local(&db, &settings, src.path(), "demo", None).unwrap();
        set_enabled(&db, &s.id, AgentKind::Claude, true).unwrap();
        set_sync_method(&db, &s.id, SyncMethod::Copy).unwrap();
        sync_one(&db, &settings, &s.id).unwrap();

        let adapter = agents::for_kind(AgentKind::Claude).unwrap();
        let dest = adapter
            .skill_registry_target(&settings)
            .unwrap()
            .join(&s.id);
        assert!(dest.exists(), "expected {dest:?} to exist after sync");
        assert!(dest.join("SKILL.md").exists());

        // Now disable and resync — it should disappear.
        set_enabled(&db, &s.id, AgentKind::Claude, false).unwrap();
        sync_one(&db, &settings, &s.id).unwrap();
        assert!(!dest.exists());
    }
}
