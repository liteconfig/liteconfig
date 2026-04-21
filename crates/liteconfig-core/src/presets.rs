//! Curated catalog of popular skill repositories and MCP servers. Ported
//! directly from cc-switch so our "+ New" / preset menus can match what a
//! user familiar with that tool expects.
//!
//! These are static — editing them means a code change + release. That's
//! deliberate: the catalog is small, stable, and shouldn't silently mutate
//! under the user. Discovery of newer skills goes through the skills.sh
//! search surface instead.

/// A featured external skill repo. `add_arg` is what gets fed to
/// [`crate::services::skill_repo_service::add`] — normally `owner/name`.
#[derive(Debug, Clone, Copy)]
pub struct SkillRepoPreset {
    pub owner: &'static str,
    pub name: &'static str,
    pub branch: &'static str,
    pub description: &'static str,
}

impl SkillRepoPreset {
    pub fn add_arg(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    pub fn display_name(&self) -> String {
        self.add_arg()
    }
}

/// A featured MCP server. `command` + `args` are the exact tokens that get
/// written into the `mcp_servers` config row. On Windows we wrap `npx`
/// invocations with `cmd /c` so the npx.cmd shim is resolved — matches the
/// cc-switch behaviour verbatim.
#[derive(Debug, Clone, Copy)]
pub struct McpPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub homepage: &'static str,
    pub description: &'static str,
}

pub const SKILL_REPO_PRESETS: &[SkillRepoPreset] = &[
    SkillRepoPreset {
        owner: "anthropics",
        name: "skills",
        branch: "main",
        description: "Official Anthropic Skills catalog",
    },
    SkillRepoPreset {
        owner: "ComposioHQ",
        name: "awesome-claude-skills",
        branch: "master",
        description: "Curated community Skills index",
    },
    SkillRepoPreset {
        owner: "cexll",
        name: "myclaude",
        branch: "master",
        description: "cexll's personal Skills toolkit",
    },
    SkillRepoPreset {
        owner: "JimLiu",
        name: "baoyu-skills",
        branch: "main",
        description: "Baoyu Skills bundle",
    },
];

/// Resolve the platform-specific `(command, args)` for an npx preset —
/// on Windows we bounce through `cmd /c npx ...` so the npx.cmd shim is
/// picked up. On everything else we exec `npx` directly.
pub fn platform_npx(package: &str, extra: &[&str]) -> (String, Vec<String>) {
    if cfg!(windows) {
        let mut args: Vec<String> = vec!["/c".into(), "npx".into()];
        args.extend(extra.iter().map(|s| (*s).to_string()));
        args.push(package.into());
        ("cmd".into(), args)
    } else {
        let mut args: Vec<String> = extra.iter().map(|s| (*s).to_string()).collect();
        args.push(package.into());
        ("npx".into(), args)
    }
}

pub const MCP_PRESETS: &[McpPreset] = &[
    McpPreset {
        id: "fetch",
        name: "mcp-server-fetch",
        command: "uvx",
        args: &["mcp-server-fetch"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "HTTP fetch + scrape",
    },
    McpPreset {
        id: "time",
        name: "@modelcontextprotocol/server-time",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-time"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Time / timezone utilities",
    },
    McpPreset {
        id: "memory",
        name: "@modelcontextprotocol/server-memory",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-memory"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Persistent graph memory",
    },
    McpPreset {
        id: "sequential-thinking",
        name: "@modelcontextprotocol/server-sequential-thinking",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-sequential-thinking"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Step-by-step reasoning scratchpad",
    },
    McpPreset {
        id: "context7",
        name: "@upstash/context7-mcp",
        command: "npx",
        args: &["-y", "@upstash/context7-mcp"],
        homepage: "https://context7.com",
        description: "Live up-to-date library docs",
    },
];

impl McpPreset {
    /// Resolve to `(command, args)` on the current platform. Only `npx`
    /// gets rewritten — everything else (uvx, custom binaries) runs as-is.
    pub fn resolved(&self) -> (String, Vec<String>) {
        if self.command == "npx" {
            // args: ["-y", "<package>"] → split back into extra + package.
            if let Some((package, extra)) = self.args.split_last() {
                return platform_npx(package, extra);
            }
        }
        (
            self.command.to_string(),
            self.args.iter().map(|s| (*s).to_string()).collect(),
        )
    }
}
