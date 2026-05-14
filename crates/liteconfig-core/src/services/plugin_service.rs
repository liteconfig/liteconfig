//! Claude Code plugin install + sync.
//!
//! A plugin's source is either a git URL (cloned into
//! `~/.liteconfig/plugins/<id>/`) or a local directory (copied in).
//! After materialization we walk the standard CC layout and count
//! resources — skills, MCP servers, subagents, slash commands — then
//! store those counts on the row for display. Bundled skills get
//! upserted into the `skills` table automatically so they show up in
//! the Skills tab.

use std::path::{Path, PathBuf};

use git2::Repository;

use crate::db::Database;
use crate::fs_util::hash_directory;
use crate::model::agent::AgentKind;
use crate::model::plugin::{Plugin, PluginContents, PluginSource};
use crate::model::skill::{Skill, SkillSource, SyncMethod};
use crate::paths::{ensure_dir, liteconfig_plugins_dir};
use crate::{Error, Result};

/// Register + materialize + scan in one pass. Cheap to call after a clone
/// failure — upsert is idempotent on the row id.
pub fn install(db: &Database, source: PluginSource, name_hint: Option<&str>) -> Result<Plugin> {
    let id = uuid::Uuid::new_v4().to_string();
    let directory = materialize(&id, &source)?;
    let manifest = read_manifest(&directory);
    let name = manifest
        .as_ref()
        .and_then(|m| m.name.clone())
        .or_else(|| name_hint.map(|s| s.to_string()))
        .unwrap_or_else(|| {
            directory
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("plugin")
                .to_string()
        });
    let description = manifest.as_ref().and_then(|m| m.description.clone());
    let now = chrono::Utc::now().timestamp_millis();
    let mut plugin = Plugin {
        id: id.clone(),
        name,
        description,
        directory: directory.clone(),
        source,
        enabled: Default::default(),
        contents: PluginContents::default(),
        content_hash: hash_directory(&directory).ok(),
        installed_at: now,
        last_synced_at: None,
    };
    db.upsert_plugin(&plugin)?;

    // First scan populates the counts + imports the plugin's skills into
    // the main `skills` table so they appear in the Skills tab.
    plugin.contents = scan_and_import(db, &plugin)?;
    plugin.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
    db.upsert_plugin(&plugin)?;
    Ok(plugin)
}

pub fn list(db: &Database) -> Result<Vec<Plugin>> {
    db.list_plugins()
}

pub fn uninstall(db: &Database, id: &str) -> Result<()> {
    if let Some(p) = db.get_plugin(id)? {
        let _ = std::fs::remove_dir_all(&p.directory);
    }
    db.delete_plugin(id)
}

pub fn sync_one(db: &Database, id: &str) -> Result<Plugin> {
    let mut plugin = db
        .get_plugin(id)?
        .ok_or_else(|| Error::InvalidConfig(format!("plugin {id} not found")))?;
    plugin.contents = scan_and_import(db, &plugin)?;
    plugin.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
    plugin.content_hash = hash_directory(&plugin.directory).ok();
    db.upsert_plugin(&plugin)?;
    Ok(plugin)
}

pub fn set_enabled(db: &Database, id: &str, agent: AgentKind, enabled: bool) -> Result<Plugin> {
    let mut plugin = db
        .get_plugin(id)?
        .ok_or_else(|| Error::InvalidConfig(format!("plugin {id} not found")))?;
    plugin.enabled.insert(agent, enabled);
    db.upsert_plugin(&plugin)?;
    Ok(plugin)
}

// ---------- materialization ----------

fn materialize(id: &str, source: &PluginSource) -> Result<PathBuf> {
    let dest = liteconfig_plugins_dir()?.join(id);
    ensure_dir(dest.parent().unwrap_or(Path::new("/")))?;
    match source {
        PluginSource::Git { url, branch } => {
            let mut builder = git2::build::RepoBuilder::new();
            builder.branch(branch);
            if dest.exists() {
                // Re-read an existing clone — fetch + hard reset.
                let repo = Repository::open(&dest).map_err(git_err)?;
                let mut remote = repo.find_remote("origin").map_err(git_err)?;
                remote
                    .fetch(&[branch.as_str()], None, None)
                    .map_err(git_err)?;
                let fetch_head = repo
                    .find_reference("FETCH_HEAD")
                    .and_then(|f| f.peel_to_commit())
                    .map_err(git_err)?;
                repo.reset(fetch_head.as_object(), git2::ResetType::Hard, None)
                    .map_err(git_err)?;
            } else {
                builder.clone(url, &dest).map_err(git_err)?;
            }
        }
        PluginSource::Local { path } => {
            if dest.exists() {
                let _ = std::fs::remove_dir_all(&dest);
            }
            copy_dir_recursive(path, &dest)?;
        }
    }
    Ok(dest)
}

// ---------- scan + import ----------

#[derive(Debug, Clone)]
struct PluginManifest {
    name: Option<String>,
    description: Option<String>,
}

fn read_manifest(dir: &Path) -> Option<PluginManifest> {
    let path = dir.join(".claude-plugin").join("plugin.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(PluginManifest {
        name: v
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

/// Walk the plugin directory for each recognised resource folder. Count
/// entries + import skills into the main `skills` table (dedup by name —
/// skip if a same-named skill already exists, matching `skill_repo_service`
/// dedup).
fn scan_and_import(db: &Database, plugin: &Plugin) -> Result<PluginContents> {
    let dir = &plugin.directory;
    let skills = import_plugin_skills(db, plugin)?;
    let commands = count_files_with_ext(&dir.join("commands"), "md");
    let agents = count_files_with_ext(&dir.join("agents"), "md");
    let mcp_servers = count_mcp_servers(dir);
    Ok(PluginContents {
        skills: skills as u32,
        mcp_servers: mcp_servers as u32,
        commands: commands as u32,
        agents: agents as u32,
    })
}

fn import_plugin_skills(db: &Database, plugin: &Plugin) -> Result<usize> {
    let skills_dir = plugin.directory.join("skills");
    let entries = match std::fs::read_dir(&skills_dir) {
        Ok(it) => it,
        Err(_) => return Ok(0),
    };
    let existing: std::collections::HashSet<String> =
        db.list_skills()?.into_iter().map(|s| s.name).collect();
    let now = chrono::Utc::now().timestamp_millis();
    let mut count = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").exists() {
            continue;
        }
        let Some(dir_name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let name = if existing.contains(&dir_name) {
            format!("{dir_name} ({})", plugin.name)
        } else {
            dir_name
        };
        let skill = Skill {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: None,
            directory: path.clone(),
            source: SkillSource::Local,
            sync_method: SyncMethod::Inherit,
            enabled: Default::default(),
            content_hash: hash_directory(&path).ok(),
            last_synced_hash: None,
            installed_at: now,
            updated_at: now,
        };
        db.upsert_skill(&skill)?;
        count += 1;
    }
    Ok(count)
}

fn count_files_with_ext(dir: &Path, ext: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s == ext)
                .unwrap_or(false)
        })
        .count()
}

fn count_mcp_servers(dir: &Path) -> usize {
    for candidate in [".mcp.json", "mcp.json"] {
        let p = dir.join(candidate);
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(obj) = v.get("mcpServers").and_then(|x| x.as_object()) {
            return obj.len();
        }
        if let Some(obj) = v.get("mcp").and_then(|x| x.as_object()) {
            return obj.len();
        }
    }
    0
}

fn git_err(e: git2::Error) -> Error {
    Error::InvalidConfig(format!("git: {e}"))
}

/// Shallow directory copy — local plugins are small (commands + skills),
/// so recursive `std::fs::copy` is fine.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    ensure_dir(dst)?;
    for entry in std::fs::read_dir(src).map_err(|source| Error::Io {
        path: src.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: src.to_path_buf(),
            source,
        })?;
        let from = entry.path();
        let name = entry.file_name();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|source| Error::Io {
                path: to.clone(),
                source,
            })?;
        }
    }
    Ok(())
}
