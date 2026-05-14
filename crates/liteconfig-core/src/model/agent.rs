use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The fixed set of agents liteconfig knows about at compile time.
///
/// The order here is *load-bearing*: it drives column order, agent-dot order
/// in pills, and tab layouts. Prefer appending new variants to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    Cursor,
    OpenCode,
}

pub const ALL_AGENT_KINDS: &[AgentKind] = &[
    AgentKind::Claude,
    AgentKind::Codex,
    AgentKind::Gemini,
    AgentKind::Cursor,
    AgentKind::OpenCode,
];

impl AgentKind {
    pub fn id(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Gemini => "gemini",
            AgentKind::Cursor => "cursor",
            AgentKind::OpenCode => "opencode",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude Code",
            AgentKind::Codex => "Codex",
            AgentKind::Gemini => "Gemini CLI",
            AgentKind::Cursor => "Cursor",
            AgentKind::OpenCode => "OpenCode",
        }
    }

    /// Two-letter label used inside agent pills (`Cl`, `Cx`, `Gm`, `Cr`, `Oc`).
    pub fn short_label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Cl",
            AgentKind::Codex => "Cx",
            AgentKind::Gemini => "Gm",
            AgentKind::Cursor => "Cr",
            AgentKind::OpenCode => "Oc",
        }
    }

    /// Whether this agent has a "profile settings" concept.
    ///
    /// Cursor does not support full provider config swaps; it only
    /// participates in MCP, skills, and rules sync. OpenCode has a single
    /// `opencode.json` that profile switching maps over, same as Claude.
    pub fn supports_profiles(self) -> bool {
        !matches!(self, AgentKind::Cursor)
    }
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for AgentKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "claude" => Ok(AgentKind::Claude),
            "codex" => Ok(AgentKind::Codex),
            "gemini" => Ok(AgentKind::Gemini),
            "cursor" => Ok(AgentKind::Cursor),
            "opencode" => Ok(AgentKind::OpenCode),
            other => Err(Error::UnknownAgent(other.to_string())),
        }
    }
}
