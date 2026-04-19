//! Rules: manage `CLAUDE.md` / `AGENTS.md` / `.cursor/rules/*.mdc`-style
//! markdown files from one place, with per-agent enablement.
//!
//! Writing rules is additive per-agent: every enabled rule for an agent is
//! concatenated (in stable, alphabetical-by-name order) into that agent's
//! rule file, separated by a visible delimiter.

use std::collections::BTreeMap;

use crate::agents;
use crate::db::Database;
use crate::fs_util::{atomic_write, read_to_string};
use crate::model::agent::AgentKind;
use crate::model::rule::Rule;
use crate::paths::{self, ensure_dir};
use crate::settings::Settings;
use crate::{Error, Result};

const DELIMITER: &str = "\n\n<!-- liteconfig-rule -->\n\n";

pub fn list(db: &Database) -> Result<Vec<Rule>> {
    db.list_rules()
}

pub fn upsert(db: &Database, mut rule: Rule) -> Result<Rule> {
    rule.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_rule(&rule)?;
    Ok(rule)
}

pub fn delete(db: &Database, id: &str) -> Result<()> {
    db.delete_rule(id)
}

pub fn set_enabled(db: &Database, id: &str, agent: AgentKind, enabled: bool) -> Result<Rule> {
    let mut rule = db
        .list_rules()?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| Error::InvalidConfig(format!("rule {id} not found")))?;
    rule.enabled.insert(agent, enabled);
    rule.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_rule(&rule)?;
    Ok(rule)
}

/// Write the concatenated body of every enabled rule into each agent's rule
/// file. Agents with no enabled rules get an empty file (not deleted — some
/// agents complain if the file is missing).
pub fn sync_all(db: &Database, settings: &Settings) -> Result<()> {
    let mut rules = db.list_rules()?;
    rules.sort_by_key(|r| r.name.to_lowercase());

    for agent in crate::model::agent::ALL_AGENT_KINDS {
        let adapter = agents::for_kind(*agent)?;
        let paths = adapter.paths(settings)?;
        let Some(target) = paths.rule_file else {
            continue;
        };
        if let Some(parent) = target.parent() {
            ensure_dir(parent)?;
        }
        let parts: Vec<&str> = rules
            .iter()
            .filter(|r| *r.enabled.get(agent).unwrap_or(&false))
            .map(|r| r.body.as_str())
            .collect();
        let body = parts.join(DELIMITER);
        atomic_write(&target, body.as_bytes())?;
    }
    Ok(())
}

/// Import existing rule files from each agent's live location into the DB.
/// Idempotent: identical bodies (already present) simply flip the source
/// agent's `enabled` bit instead of creating duplicates.
pub fn import_from_live(db: &Database, settings: &Settings) -> Result<Vec<Rule>> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut existing_by_body: BTreeMap<String, Rule> = db
        .list_rules()?
        .into_iter()
        .map(|r| (r.body.trim().to_string(), r))
        .collect();
    let mut created: Vec<Rule> = Vec::new();

    // Helper closure to process a single (agent, name, body) triple.
    let mut ingest = |agent: AgentKind, name: String, body: String| -> Result<()> {
        let key = body.trim().to_string();
        if key.is_empty() {
            return Ok(());
        }
        if let Some(existing) = existing_by_body.get_mut(&key) {
            if !existing.enabled.get(&agent).copied().unwrap_or(false) {
                existing.enabled.insert(agent, true);
                existing.updated_at = now;
                db.upsert_rule(existing)?;
            }
            return Ok(());
        }
        let mut enabled = BTreeMap::new();
        enabled.insert(agent, true);
        let rule = Rule {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            body,
            enabled,
            created_at: now,
            updated_at: now,
        };
        db.upsert_rule(&rule)?;
        existing_by_body.insert(key, rule.clone());
        created.push(rule);
        Ok(())
    };

    for agent in crate::model::agent::ALL_AGENT_KINDS.iter().copied() {
        let adapter = agents::for_kind(agent)?;
        let paths = adapter.paths(settings)?;
        if let Some(target) = paths.rule_file {
            if target.exists() {
                let text = read_to_string(&target)?;
                let base = target
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("rule")
                    .to_string();
                let parts: Vec<&str> = text.split(DELIMITER).collect();
                if parts.len() == 1 {
                    ingest(agent, base, text.clone())?;
                } else {
                    for (i, part) in parts.iter().enumerate() {
                        let name = format!("{base}-{i}");
                        ingest(agent, name, (*part).to_string())?;
                    }
                }
            }
        }
    }

    // Cursor stores rules as individual `.mdc` files under `~/.cursor/rules/`.
    let cursor_dir = paths::cursor_rules_dir(settings)?;
    if cursor_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&cursor_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|n| n.to_str()).map(String::from) else {
                    continue;
                };
                let body = read_to_string(&path)?;
                ingest(AgentKind::Cursor, name, body)?;
            }
        }
    }

    Ok(created)
}
