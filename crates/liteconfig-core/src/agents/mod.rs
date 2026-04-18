//! The `AgentAdapter` abstraction: one implementation per supported agent.
//!
//! Adding a new agent means:
//!   1. Add a variant to `AgentKind`.
//!   2. Implement `AgentAdapter` for it.
//!   3. Register the adapter in `registry()`.
//!
//! Nothing in the service layer needs to change.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;

use crate::model::agent::{AgentKind, ALL_AGENT_KINDS};
use crate::model::mcp::McpServer;
use crate::model::profile::Profile;
use crate::settings::Settings;
use crate::{Error, Result};

pub mod claude;
pub mod codex;
pub mod gemini;

/// The canonical file paths an agent cares about, resolved for the current
/// `Settings` (which may contain overrides).
#[derive(Debug, Clone)]
pub struct AgentPaths {
    pub live_settings: PathBuf,
    pub mcp_file: Option<PathBuf>,
    pub rule_file: Option<PathBuf>,
    pub skills_dir: Option<PathBuf>,
    pub sessions_dir: Option<PathBuf>,
    pub extra: Vec<PathBuf>,
}

pub trait AgentAdapter: Send + Sync {
    fn kind(&self) -> AgentKind;

    fn paths(&self, settings: &Settings) -> Result<AgentPaths>;

    /// Read the agent's current live config into a normalized `Profile`.
    /// Used by the backfill mechanism during profile switches.
    fn read_live(&self, settings: &Settings) -> Result<Option<Value>>;

    /// Write a profile's config to the agent's live files. `common` is an
    /// optional snippet to deep-merge on top of the profile's config first.
    /// Any `@secret:*` placeholders must already have been resolved by the
    /// caller.
    fn write_live(
        &self,
        settings: &Settings,
        profile: &Profile,
        common: Option<&Value>,
    ) -> Result<()>;

    /// Read MCP servers from the agent's live config (for imports).
    fn read_mcp(&self, settings: &Settings) -> Result<Vec<McpServer>>;

    /// Write the supplied MCP servers to the agent's live config.
    fn write_mcp(&self, settings: &Settings, servers: &[McpServer]) -> Result<()>;

    /// Where this agent looks for skill files. `None` means the agent does
    /// not natively support skills.
    fn skill_registry_target(&self, settings: &Settings) -> Option<PathBuf> {
        self.paths(settings).ok().and_then(|p| p.skills_dir)
    }
}

/// Global static registry of adapters. Use `for_kind()` to look one up.
pub fn registry() -> &'static [Box<dyn AgentAdapter>] {
    static CELL: OnceLock<Vec<Box<dyn AgentAdapter>>> = OnceLock::new();
    CELL.get_or_init(|| {
        let adapters: Vec<Box<dyn AgentAdapter>> = vec![
            Box::new(claude::ClaudeAdapter),
            Box::new(codex::CodexAdapter),
            Box::new(gemini::GeminiAdapter),
        ];
        // Sanity-check: every AgentKind variant has an adapter.
        for k in ALL_AGENT_KINDS {
            assert!(
                adapters.iter().any(|a| a.kind() == *k),
                "missing adapter for {k:?}"
            );
        }
        adapters
    })
}

pub fn for_kind(kind: AgentKind) -> Result<&'static dyn AgentAdapter> {
    registry()
        .iter()
        .find(|a| a.kind() == kind)
        .map(|b| b.as_ref())
        .ok_or_else(|| Error::UnknownAgent(kind.id().to_string()))
}
