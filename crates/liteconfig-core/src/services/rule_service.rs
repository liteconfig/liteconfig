//! Rules: manage `CLAUDE.md` / `AGENTS.md` / `.cursor/rules/*.mdc`-style
//! markdown files from one place, with per-agent enablement.
//!
//! Writing rules is additive per-agent: every enabled rule for an agent is
//! concatenated (in stable, alphabetical-by-name order) into that agent's
//! rule file, separated by a visible delimiter.

use crate::agents;
use crate::db::Database;
use crate::fs_util::atomic_write;
use crate::model::agent::AgentKind;
use crate::model::rule::Rule;
use crate::paths::ensure_dir;
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
