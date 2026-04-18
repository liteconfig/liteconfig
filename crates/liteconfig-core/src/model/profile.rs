use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::agent::AgentKind;

/// A named provider configuration for a single agent.
///
/// `config` carries the normalized provider config with `@secret:<name>`
/// placeholders where secrets should land at write time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub agent: AgentKind,
    pub name: String,
    pub config: Value,
    #[serde(default)]
    pub meta: ProfileMeta,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileMeta {
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub sort_index: i64,
    /// For Claude: "anthropic" | "openai_chat" — drives on-the-fly format
    /// translation for OpenAI-compatible endpoints.
    #[serde(default)]
    pub api_format: Option<String>,
}

impl Profile {
    pub fn new(agent: AgentKind, name: impl Into<String>, config: Value) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent,
            name: name.into(),
            config,
            meta: ProfileMeta::default(),
            created_at: now,
            updated_at: now,
        }
    }
}
