use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::agent::AgentKind;

/// A rule = a markdown body that liteconfig writes into each agent's
/// rule file (e.g. `CLAUDE.md`, `AGENTS.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub enabled: BTreeMap<AgentKind, bool>,
    pub created_at: i64,
    pub updated_at: i64,
}
