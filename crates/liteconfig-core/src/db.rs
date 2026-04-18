//! SQLite database: schema, migrations, and typed CRUD helpers for every
//! table. The DB is the single source of truth for liteconfig data; live
//! agent config files are derived output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::profile::{Profile, ProfileMeta};
use crate::model::rule::Rule;
use crate::model::skill::{Skill, SkillSource, StorageMode, SyncMethod};
use crate::paths::{ensure_dir, liteconfig_db_path, liteconfig_dir};
use crate::{Error, Result};

pub const SCHEMA_VERSION: i32 = 1;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open the liteconfig database at the default location, creating it and
    /// running migrations as needed.
    pub fn open_default() -> Result<Self> {
        ensure_dir(&liteconfig_dir()?)?;
        let path = liteconfig_db_path()?;
        Self::open(&path)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        let current: i32 = self
            .conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if current > SCHEMA_VERSION {
            return Err(Error::Migration(format!(
                "database schema {current} is newer than this binary (supports {SCHEMA_VERSION})"
            )));
        }

        if current < 1 {
            self.conn.execute_batch(MIGRATION_V1)?;
        }

        self.conn.execute(
            "INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    // ---------- profiles ----------

    pub fn list_profiles(&self, agent: AgentKind) -> Result<Vec<Profile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, name, config_json, meta_json, created_at, updated_at
             FROM profiles WHERE agent = ?1 ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![agent.id()], row_to_profile)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_profile(&self, agent: AgentKind, id: &str) -> Result<Option<Profile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, name, config_json, meta_json, created_at, updated_at
             FROM profiles WHERE agent = ?1 AND id = ?2",
        )?;
        stmt.query_row(params![agent.id(), id], row_to_profile)
            .optional()
            .map_err(Error::from)
    }

    pub fn upsert_profile(&self, profile: &Profile) -> Result<()> {
        let config = serde_json::to_string(&profile.config)?;
        let meta = serde_json::to_string(&profile.meta)?;
        self.conn.execute(
            "INSERT INTO profiles (id, agent, name, config_json, meta_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id, agent) DO UPDATE SET
                 name = excluded.name,
                 config_json = excluded.config_json,
                 meta_json = excluded.meta_json,
                 updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.agent.id(),
                profile.name,
                config,
                meta,
                profile.created_at,
                profile.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_profile(&self, agent: AgentKind, id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM profiles WHERE agent = ?1 AND id = ?2",
            params![agent.id(), id],
        )?;
        Ok(())
    }

    // ---------- skills ----------

    pub fn list_skills(&self) -> Result<Vec<Skill>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, directory, source_kind, repo_owner, repo_name,
                    repo_branch, content_hash, sync_method, enabled_json, installed_at, updated_at
             FROM skills ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_skill)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, directory, source_kind, repo_owner, repo_name,
                    repo_branch, content_hash, sync_method, enabled_json, installed_at, updated_at
             FROM skills WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_skill)
            .optional()
            .map_err(Error::from)
    }

    pub fn upsert_skill(&self, skill: &Skill) -> Result<()> {
        let (source_kind, owner, name, branch) = match &skill.source {
            SkillSource::Local => ("local", None, None, None),
            SkillSource::Github {
                owner,
                name,
                branch,
            } => (
                "github",
                Some(owner.clone()),
                Some(name.clone()),
                Some(branch.clone()),
            ),
        };
        let enabled = serde_json::to_string(&skill.enabled)?;
        self.conn.execute(
            "INSERT INTO skills (id, name, description, directory, source_kind, repo_owner, repo_name,
                                  repo_branch, content_hash, sync_method, enabled_json,
                                  installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 description = excluded.description,
                 directory = excluded.directory,
                 source_kind = excluded.source_kind,
                 repo_owner = excluded.repo_owner,
                 repo_name = excluded.repo_name,
                 repo_branch = excluded.repo_branch,
                 content_hash = excluded.content_hash,
                 sync_method = excluded.sync_method,
                 enabled_json = excluded.enabled_json,
                 updated_at = excluded.updated_at",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory.to_string_lossy().to_string(),
                source_kind,
                owner,
                name,
                branch,
                skill.content_hash,
                skill.sync_method.as_str(),
                enabled,
                skill.installed_at,
                skill.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM skills WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- mcp servers ----------

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, config_json, enabled_json, created_at, updated_at
             FROM mcp_servers ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_mcp)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn upsert_mcp_server(&self, server: &McpServer) -> Result<()> {
        let config = serde_json::to_string(&server.config)?;
        let enabled = serde_json::to_string(&server.enabled)?;
        self.conn.execute(
            "INSERT INTO mcp_servers (id, name, config_json, enabled_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 config_json = excluded.config_json,
                 enabled_json = excluded.enabled_json,
                 updated_at = excluded.updated_at",
            params![
                server.id,
                server.name,
                config,
                enabled,
                server.created_at,
                server.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- rules ----------

    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, body, enabled_json, created_at, updated_at
             FROM rules ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_rule)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn upsert_rule(&self, rule: &Rule) -> Result<()> {
        let enabled = serde_json::to_string(&rule.enabled)?;
        self.conn.execute(
            "INSERT INTO rules (id, name, body, enabled_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 body = excluded.body,
                 enabled_json = excluded.enabled_json,
                 updated_at = excluded.updated_at",
            params![
                rule.id,
                rule.name,
                rule.body,
                enabled,
                rule.created_at,
                rule.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- common config ----------

    pub fn get_common_config(&self, agent: AgentKind) -> Result<Option<serde_json::Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT config_json, enabled FROM common_config WHERE agent = ?1")?;
        let row = stmt
            .query_row(params![agent.id()], |r| {
                let s: String = r.get(0)?;
                let enabled: i64 = r.get(1)?;
                Ok((s, enabled))
            })
            .optional()?;
        match row {
            None => Ok(None),
            Some((_, 0)) => Ok(None),
            Some((json, _)) => {
                let v: serde_json::Value = serde_json::from_str(&json)?;
                Ok(Some(v))
            }
        }
    }

    pub fn set_common_config(
        &self,
        agent: AgentKind,
        config: &serde_json::Value,
        enabled: bool,
    ) -> Result<()> {
        let json = serde_json::to_string(config)?;
        self.conn.execute(
            "INSERT INTO common_config (agent, config_json, enabled) VALUES (?1, ?2, ?3)
             ON CONFLICT(agent) DO UPDATE SET config_json = excluded.config_json, enabled = excluded.enabled",
            params![agent.id(), json, enabled as i64],
        )?;
        Ok(())
    }

    /// Escape hatch for tests / tooling — avoid in normal code paths.
    pub fn raw(&self) -> &Connection {
        &self.conn
    }
}

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS profiles (
    id          TEXT NOT NULL,
    agent       TEXT NOT NULL,
    name        TEXT NOT NULL,
    config_json TEXT NOT NULL,
    meta_json   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (id, agent)
);

CREATE TABLE IF NOT EXISTS skills (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    directory     TEXT NOT NULL,
    source_kind   TEXT NOT NULL,
    repo_owner    TEXT,
    repo_name     TEXT,
    repo_branch   TEXT,
    content_hash  TEXT,
    sync_method   TEXT NOT NULL,
    enabled_json  TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    config_json   TEXT NOT NULL,
    enabled_json  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS rules (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    body          TEXT NOT NULL,
    enabled_json  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS common_config (
    agent        TEXT PRIMARY KEY,
    config_json  TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1
);
"#;

fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<Profile> {
    let id: String = row.get(0)?;
    let agent_str: String = row.get(1)?;
    let name: String = row.get(2)?;
    let config_json: String = row.get(3)?;
    let meta_json: String = row.get(4)?;
    let created_at: i64 = row.get(5)?;
    let updated_at: i64 = row.get(6)?;

    let agent: AgentKind = agent_str.parse().map_err(|e: Error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let config = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let meta: ProfileMeta = serde_json::from_str(&meta_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Profile {
        id,
        agent,
        name,
        config,
        meta,
        created_at,
        updated_at,
    })
}

fn row_to_skill(row: &rusqlite::Row<'_>) -> rusqlite::Result<Skill> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: Option<String> = row.get(2)?;
    let directory: String = row.get(3)?;
    let source_kind: String = row.get(4)?;
    let repo_owner: Option<String> = row.get(5)?;
    let repo_name: Option<String> = row.get(6)?;
    let repo_branch: Option<String> = row.get(7)?;
    let content_hash: Option<String> = row.get(8)?;
    let sync_method_str: String = row.get(9)?;
    let enabled_json: String = row.get(10)?;
    let installed_at: i64 = row.get(11)?;
    let updated_at: i64 = row.get(12)?;

    let source = match source_kind.as_str() {
        "local" => SkillSource::Local,
        "github" => SkillSource::Github {
            owner: repo_owner.unwrap_or_default(),
            name: repo_name.unwrap_or_default(),
            branch: repo_branch.unwrap_or_else(|| "main".into()),
        },
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(Error::InvalidConfig(format!("unknown source_kind {other}"))),
            ))
        }
    };

    let sync_method = SyncMethod::parse(&sync_method_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            9,
            rusqlite::types::Type::Text,
            Box::new(Error::InvalidConfig(format!(
                "unknown sync_method {sync_method_str}"
            ))),
        )
    })?;

    let enabled: BTreeMap<AgentKind, bool> = serde_json::from_str(&enabled_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Skill {
        id,
        name,
        description,
        directory: PathBuf::from(directory),
        source,
        sync_method,
        enabled,
        content_hash,
        installed_at,
        updated_at,
    })
}

fn row_to_mcp(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServer> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let config_json: String = row.get(2)?;
    let enabled_json: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    let config = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let enabled = serde_json::from_str(&enabled_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(McpServer {
        id,
        name,
        config,
        enabled,
        created_at,
        updated_at,
    })
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<Rule> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let body: String = row.get(2)?;
    let enabled_json: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    let enabled = serde_json::from_str(&enabled_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Rule {
        id,
        name,
        body,
        enabled,
        created_at,
        updated_at,
    })
}

// Storage/SyncMethod are Copy types; silence unused-import warnings when the
// feature surface changes. (No-op at runtime.)
#[allow(dead_code)]
fn _unused_storage_mode_tag(_s: StorageMode) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrations_run_cleanly() {
        let db = Database::open_in_memory().unwrap();
        let version: String = db
            .conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn profile_crud_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let p = Profile::new(
            AgentKind::Claude,
            "primary",
            json!({ "env": { "ANTHROPIC_API_KEY": "@secret:claude-primary" } }),
        );
        db.upsert_profile(&p).unwrap();
        let back = db.get_profile(AgentKind::Claude, &p.id).unwrap().unwrap();
        assert_eq!(back.name, "primary");
        assert_eq!(back.agent, AgentKind::Claude);
        assert_eq!(
            back.config["env"]["ANTHROPIC_API_KEY"].as_str().unwrap(),
            "@secret:claude-primary"
        );
        let listed = db.list_profiles(AgentKind::Claude).unwrap();
        assert_eq!(listed.len(), 1);
        db.delete_profile(AgentKind::Claude, &p.id).unwrap();
        assert!(db.get_profile(AgentKind::Claude, &p.id).unwrap().is_none());
    }
}
