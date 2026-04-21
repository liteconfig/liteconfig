//! liteconfig-core
//!
//! Pure-Rust library for managing AI coding-agent configurations, skills,
//! MCP servers, and rules. Consumers (TUI, CLI, future GUIs) drive this
//! library through its public service layer.

pub mod agents;
pub mod db;
pub mod error;
pub mod fs_util;
pub mod model;
pub mod paths;
pub mod presets;
pub mod services;
pub mod settings;

#[cfg(test)]
mod test_util;

pub use error::{Error, Result};
pub use model::agent::{AgentKind, ALL_AGENT_KINDS};
pub use model::profile::Profile;
pub use model::skill::{Skill, SkillSource, StorageMode, SyncMethod};
pub use settings::Settings;
