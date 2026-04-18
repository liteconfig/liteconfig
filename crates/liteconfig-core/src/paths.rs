//! Path resolution for liteconfig's own files and for each supported agent's
//! live config locations. All path lookups flow through this module so that
//! tests and sandboxes can override them via `Settings::path_overrides`.

use std::path::{Path, PathBuf};

use crate::settings::Settings;
use crate::{Error, Result};

/// Root of liteconfig's own storage. Defaults to `~/.liteconfig`.
pub fn liteconfig_dir() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home.join(".liteconfig"))
}

pub fn liteconfig_db_path() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("liteconfig.db"))
}

pub fn liteconfig_settings_path() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("settings.json"))
}

pub fn liteconfig_secrets_path() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("secrets.local.json"))
}

pub fn liteconfig_skills_dir() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("skills"))
}

pub fn liteconfig_backups_dir() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("backups"))
}

/// Working directory for the GitHub backup repo. We keep one long-lived clone
/// here so pushes are incremental instead of full uploads each time.
pub fn liteconfig_backup_repo_dir() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("backup-repo"))
}

pub fn liteconfig_log_path() -> Result<PathBuf> {
    Ok(liteconfig_dir()?.join("liteconfig.log"))
}

/// Unified skills directory used when `settings.skill_storage_location == "unified"`.
pub fn unified_skills_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".agents").join("skills"))
}

// ---------- per-agent paths ----------

pub fn claude_config_dir(settings: &Settings) -> Result<PathBuf> {
    if let Some(p) = settings.path_overrides.claude_config_dir.as_ref() {
        return Ok(PathBuf::from(p));
    }
    Ok(home_dir()?.join(".claude"))
}

pub fn claude_settings_path(settings: &Settings) -> Result<PathBuf> {
    Ok(claude_config_dir(settings)?.join("settings.json"))
}

pub fn claude_mcp_path(_settings: &Settings) -> Result<PathBuf> {
    // The MCP server list lives in ~/.claude.json (sibling to the dir).
    Ok(home_dir()?.join(".claude.json"))
}

pub fn codex_config_dir(settings: &Settings) -> Result<PathBuf> {
    if let Some(p) = settings.path_overrides.codex_config_dir.as_ref() {
        return Ok(PathBuf::from(p));
    }
    Ok(home_dir()?.join(".codex"))
}

pub fn codex_config_path(settings: &Settings) -> Result<PathBuf> {
    Ok(codex_config_dir(settings)?.join("config"))
}

pub fn codex_auth_path(settings: &Settings) -> Result<PathBuf> {
    Ok(codex_config_dir(settings)?.join("auth"))
}

pub fn gemini_config_dir(settings: &Settings) -> Result<PathBuf> {
    if let Some(p) = settings.path_overrides.gemini_config_dir.as_ref() {
        return Ok(PathBuf::from(p));
    }
    Ok(home_dir()?.join(".gemini"))
}

pub fn gemini_settings_path(settings: &Settings) -> Result<PathBuf> {
    Ok(gemini_config_dir(settings)?.join("settings.json"))
}

// ---------- helpers ----------

fn home_dir() -> Result<PathBuf> {
    // Honour `LITECONFIG_HOME` first — used heavily in tests so we never touch
    // the real user home directory.
    if let Ok(p) = std::env::var("LITECONFIG_HOME") {
        return Ok(PathBuf::from(p));
    }
    dirs::home_dir()
        .ok_or_else(|| Error::InvalidConfig("unable to determine user home directory".to_string()))
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}
