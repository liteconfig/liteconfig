use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::agent::AgentKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    /// Normalized server config (command, args, env). Agent adapters translate
    /// this into each agent's native representation.
    pub config: Value,
    #[serde(default)]
    pub enabled: BTreeMap<AgentKind, bool>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl McpServer {
    pub fn is_enabled_for(&self, agent: AgentKind) -> bool {
        *self.enabled.get(&agent).unwrap_or(&false)
    }
}
