//! SQLite database: schema, migrations, and typed CRUD helpers for every
//! table. The DB is the single source of truth for liteconfig data; live
//! agent config files are derived output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::model::plugin::{Plugin, PluginContents, PluginSource};
use crate::model::profile::{Profile, ProfileMeta};
use crate::model::rule::Rule;
use crate::model::skill::{Skill, SkillSource, StorageMode, SyncMethod};
use crate::model::skill_repo::SkillRepo;
use crate::paths::{ensure_dir, liteconfig_db_path, liteconfig_dir};
use crate::{Error, Result};

pub const SCHEMA_VERSION: i32 = 4;

pub struct Database {
    conn: Connection,
    /// The on-disk location this DB was opened from, if any. `None` for
    /// in-memory handles. Exposed so background workers can reopen an
    /// independent connection to the same data without sharing this handle.
    path: Option<PathBuf>,
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
        let mut db = Database {
            conn,
            path: Some(path.to_path_buf()),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Database { conn, path: None };
        db.migrate()?;
        Ok(db)
    }

    /// Where this handle was opened from. `None` for in-memory. Background
    /// workers use this to open their own independent connection.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
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
        if current < 2 {
            self.conn.execute_batch(MIGRATION_V2)?;
        }
        if current < 3 {
            self.conn.execute_batch(MIGRATION_V3)?;
        }
        if current < 4 {
            self.conn.execute_batch(MIGRATION_V4)?;
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
                    repo_branch, content_hash, sync_method, enabled_json, installed_at, updated_at,
                    last_synced_hash
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
                    repo_branch, content_hash, sync_method, enabled_json, installed_at, updated_at,
                    last_synced_hash
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
                                  installed_at, updated_at, last_synced_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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
                 updated_at = excluded.updated_at,
                 last_synced_hash = excluded.last_synced_hash",
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
                skill.last_synced_hash,
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

    // ---------- skill repos ----------

    pub fn list_skill_repos(&self) -> Result<Vec<SkillRepo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, owner, repo, branch, url, last_synced_at, skill_count
             FROM skill_repos ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_skill_repo)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_skill_repo(&self, id: &str) -> Result<Option<SkillRepo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, owner, repo, branch, url, last_synced_at, skill_count
             FROM skill_repos WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_skill_repo)
            .optional()
            .map_err(Error::from)
    }

    pub fn upsert_skill_repo(&self, repo: &SkillRepo) -> Result<()> {
        self.conn.execute(
            "INSERT INTO skill_repos (id, name, owner, repo, branch, url, last_synced_at, skill_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 owner = excluded.owner,
                 repo = excluded.repo,
                 branch = excluded.branch,
                 url = excluded.url,
                 last_synced_at = excluded.last_synced_at,
                 skill_count = excluded.skill_count",
            params![
                repo.id,
                repo.name,
                repo.owner,
                repo.repo,
                repo.branch,
                repo.url,
                repo.last_synced_at,
                repo.skill_count,
            ],
        )?;
        Ok(())
    }

    pub fn delete_skill_repo(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM skill_repos WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- plugins ----------

    pub fn list_plugins(&self) -> Result<Vec<Plugin>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, directory, source_json, enabled_json,
                    contents_json, content_hash, installed_at, last_synced_at
             FROM plugins ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], row_to_plugin)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_plugin(&self, id: &str) -> Result<Option<Plugin>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, directory, source_json, enabled_json,
                    contents_json, content_hash, installed_at, last_synced_at
             FROM plugins WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_plugin)
            .optional()
            .map_err(Error::from)
    }

    pub fn upsert_plugin(&self, plugin: &Plugin) -> Result<()> {
        let source_json = serde_json::to_string(&plugin.source)?;
        let enabled_json = serde_json::to_string(&plugin.enabled)?;
        let contents_json = serde_json::to_string(&plugin.contents)?;
        self.conn.execute(
            "INSERT INTO plugins (id, name, description, directory, source_json, enabled_json,
                                    contents_json, content_hash, installed_at, last_synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 description = excluded.description,
                 directory = excluded.directory,
                 source_json = excluded.source_json,
                 enabled_json = excluded.enabled_json,
                 contents_json = excluded.contents_json,
                 content_hash = excluded.content_hash,
                 last_synced_at = excluded.last_synced_at",
            params![
                plugin.id,
                plugin.name,
                plugin.description,
                plugin.directory.to_string_lossy().to_string(),
                source_json,
                enabled_json,
                contents_json,
                plugin.content_hash,
                plugin.installed_at,
                plugin.last_synced_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_plugin(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
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

const MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS skill_repos (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    owner           TEXT,
    repo            TEXT NOT NULL,
    branch          TEXT NOT NULL,
    url             TEXT NOT NULL,
    last_synced_at  INTEGER,
    skill_count     INTEGER NOT NULL DEFAULT 0
);
"#;

// V3 adds `last_synced_hash` to `skills` for drift detection. Default NULL
// so every existing row boots as "unsynced" — the startup rescan then
// populates `content_hash` and the first explicit sync populates
// `last_synced_hash`.
const MIGRATION_V3: &str = r#"
ALTER TABLE skills ADD COLUMN last_synced_hash TEXT;
"#;

// V4 adds the `plugins` table — each row = one Claude Code-style plugin
// bundle cloned into `~/.liteconfig/plugins/<id>/`. The bundle's skills /
// MCP servers / rules are imported into the existing tables; this row is
// just the "installed plugin" metadata.
const MIGRATION_V4: &str = r#"
CREATE TABLE IF NOT EXISTS plugins (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    directory       TEXT NOT NULL,
    source_json     TEXT NOT NULL,
    enabled_json    TEXT NOT NULL,
    contents_json   TEXT NOT NULL,
    content_hash    TEXT,
    installed_at    INTEGER NOT NULL,
    last_synced_at  INTEGER
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
    let last_synced_hash: Option<String> = row.get(13)?;

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
        last_synced_hash,
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

fn row_to_skill_repo(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillRepo> {
    Ok(SkillRepo {
        id: row.get(0)?,
        name: row.get(1)?,
        owner: row.get(2)?,
        repo: row.get(3)?,
        branch: row.get(4)?,
        url: row.get(5)?,
        last_synced_at: row.get(6)?,
        skill_count: row.get::<_, i64>(7)? as u32,
    })
}

fn row_to_plugin(row: &rusqlite::Row<'_>) -> rusqlite::Result<Plugin> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: Option<String> = row.get(2)?;
    let directory: String = row.get(3)?;
    let source_json: String = row.get(4)?;
    let enabled_json: String = row.get(5)?;
    let contents_json: String = row.get(6)?;
    let content_hash: Option<String> = row.get(7)?;
    let installed_at: i64 = row.get(8)?;
    let last_synced_at: Option<i64> = row.get(9)?;

    let source: PluginSource = serde_json::from_str(&source_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let enabled: BTreeMap<AgentKind, bool> = serde_json::from_str(&enabled_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let contents: PluginContents = serde_json::from_str(&contents_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Plugin {
        id,
        name,
        description,
        directory: PathBuf::from(directory),
        source,
        enabled,
        contents,
        content_hash,
        installed_at,
        last_synced_at,
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
