//! MCP server CRUD + sync-all-enabled fan-out.
//!
//! The `mcp_servers` table holds the canonical list with a per-agent
//! enablement map. Calling `sync_all` writes every agent's MCP section from
//! this canonical list, one adapter at a time.

use serde_json::Value;

use crate::agents;
use crate::db::Database;
use crate::model::agent::AgentKind;
use crate::model::mcp::McpServer;
use crate::settings::Settings;
use crate::{Error, Result};

pub fn list(db: &Database) -> Result<Vec<McpServer>> {
    db.list_mcp_servers()
}

pub fn upsert(db: &Database, mut server: McpServer) -> Result<McpServer> {
    server.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_mcp_server(&server)?;
    Ok(server)
}

pub fn delete(db: &Database, id: &str) -> Result<()> {
    db.delete_mcp_server(id)
}

/// Flip one agent's enablement bit on a server.
pub fn set_enabled(db: &Database, id: &str, agent: AgentKind, enabled: bool) -> Result<McpServer> {
    let mut server = db
        .list_mcp_servers()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| Error::InvalidConfig(format!("mcp server {id} not found")))?;
    server.enabled.insert(agent, enabled);
    server.updated_at = chrono::Utc::now().timestamp_millis();
    db.upsert_mcp_server(&server)?;
    Ok(server)
}

/// Write each agent's MCP section from the current DB state. Runs every
/// adapter, not just one.
pub fn sync_all(db: &Database, settings: &Settings) -> Result<()> {
    let servers = db.list_mcp_servers()?;
    for agent in crate::model::agent::ALL_AGENT_KINDS {
        let adapter = agents::for_kind(*agent)?;
        adapter.write_mcp(settings, &servers)?;
    }
    Ok(())
}

/// Import MCP servers from each agent's live config into the DB. Servers
/// with the same name across multiple agents are merged into a single row
/// with enablement flags set for every agent they were found in.
pub fn import_from_live(db: &Database, settings: &Settings) -> Result<Vec<McpServer>> {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<String, McpServer> = BTreeMap::new();
    for agent in crate::model::agent::ALL_AGENT_KINDS {
        let adapter = agents::for_kind(*agent)?;
        for server in adapter.read_mcp(settings)? {
            by_name
                .entry(server.name.clone())
                .and_modify(|existing| {
                    existing.enabled.insert(*agent, true);
                    if existing.config == Value::Null {
                        existing.config = server.config.clone();
                    }
                })
                .or_insert_with(|| {
                    let mut s = server;
                    s.enabled.insert(*agent, true);
                    s
                });
        }
    }
    let out: Vec<McpServer> = by_name.into_values().collect();
    for server in &out {
        db.upsert_mcp_server(server)?;
    }
    Ok(out)
}
