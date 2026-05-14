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
    /// High-level grouping for the marketplace category pane.
    pub category: &'static str,
}

/// Canonical order for the marketplace's left-pane category list. Categories
/// not in this list get appended alphabetically by the UI.
pub const MCP_CATEGORIES: &[&str] = &[
    "Search",
    "Web",
    "Dev",
    "Data",
    "AI",
    "Productivity",
    "System",
    "Core",
];

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

/// Curated catalog. Sources: the reference `modelcontextprotocol/servers`
/// repo (src/ + reference-servers), glama.ai's install rankings, and the
/// public Smithery top-list. Kept intentionally small — the live-search
/// button in the marketplace covers the long tail.
///
/// ADD NEW ENTRIES AT THE END of their category block so existing indexes
/// (e.g. in tests) remain stable.
pub const MCP_PRESETS: &[McpPreset] = &[
    // ---------- Core / reference servers ----------
    McpPreset {
        id: "fetch",
        name: "mcp-server-fetch",
        command: "uvx",
        args: &["mcp-server-fetch"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "HTTP fetch + markdown scrape",
        category: "Web",
    },
    McpPreset {
        id: "time",
        name: "@modelcontextprotocol/server-time",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-time"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Time / timezone utilities",
        category: "Core",
    },
    McpPreset {
        id: "memory",
        name: "@modelcontextprotocol/server-memory",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-memory"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Persistent graph memory",
        category: "AI",
    },
    McpPreset {
        id: "sequential-thinking",
        name: "@modelcontextprotocol/server-sequential-thinking",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-sequential-thinking"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Step-by-step reasoning scratchpad",
        category: "AI",
    },
    McpPreset {
        id: "everything",
        name: "@modelcontextprotocol/server-everything",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-everything"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Reference server exercising every MCP feature",
        category: "Core",
    },
    McpPreset {
        id: "filesystem",
        name: "@modelcontextprotocol/server-filesystem",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-filesystem"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Sandboxed filesystem read/write",
        category: "System",
    },
    McpPreset {
        id: "git",
        name: "@modelcontextprotocol/server-git",
        command: "uvx",
        args: &["mcp-server-git"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Git repo introspection + commit helpers",
        category: "Dev",
    },
    McpPreset {
        id: "github",
        name: "@modelcontextprotocol/server-github",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-github"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "GitHub issues / PRs / repo search",
        category: "Dev",
    },
    McpPreset {
        id: "gitlab",
        name: "@modelcontextprotocol/server-gitlab",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-gitlab"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "GitLab MRs / issues / pipelines",
        category: "Dev",
    },
    McpPreset {
        id: "sqlite",
        name: "@modelcontextprotocol/server-sqlite",
        command: "uvx",
        args: &["mcp-server-sqlite"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Query a local SQLite DB",
        category: "Data",
    },
    McpPreset {
        id: "postgres",
        name: "@modelcontextprotocol/server-postgres",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-postgres"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Read-only Postgres query tool",
        category: "Data",
    },
    McpPreset {
        id: "puppeteer",
        name: "@modelcontextprotocol/server-puppeteer",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-puppeteer"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Headless browser automation (Puppeteer)",
        category: "Web",
    },
    McpPreset {
        id: "brave-search",
        name: "@modelcontextprotocol/server-brave-search",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-brave-search"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Web search via Brave's API",
        category: "Search",
    },
    McpPreset {
        id: "google-drive",
        name: "@modelcontextprotocol/server-gdrive",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-gdrive"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Read + search Google Drive files",
        category: "Productivity",
    },
    McpPreset {
        id: "slack",
        name: "@modelcontextprotocol/server-slack",
        command: "npx",
        args: &["-y", "@modelcontextprotocol/server-slack"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Post to channels + read history",
        category: "Productivity",
    },
    McpPreset {
        id: "sentry",
        name: "@modelcontextprotocol/server-sentry",
        command: "uvx",
        args: &["mcp-server-sentry"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Fetch Sentry issue details + stack traces",
        category: "Dev",
    },
    McpPreset {
        id: "fetch-uvx",
        name: "mcp-server-fetch (aiohttp)",
        command: "uvx",
        args: &["mcp-server-fetch"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Alternate fetch server via uvx",
        category: "Web",
    },
    // ---------- AI tooling ----------
    McpPreset {
        id: "context7",
        name: "@upstash/context7-mcp",
        command: "npx",
        args: &["-y", "@upstash/context7-mcp"],
        homepage: "https://context7.com",
        description: "Live up-to-date library docs for any import",
        category: "AI",
    },
    McpPreset {
        id: "exa",
        name: "exa-mcp-server",
        command: "npx",
        args: &["-y", "exa-mcp-server"],
        homepage: "https://exa.ai",
        description: "Semantic web search (Exa)",
        category: "Search",
    },
    McpPreset {
        id: "tavily",
        name: "tavily-mcp",
        command: "npx",
        args: &["-y", "tavily-mcp"],
        homepage: "https://tavily.com",
        description: "LLM-optimized web search (Tavily)",
        category: "Search",
    },
    McpPreset {
        id: "firecrawl",
        name: "firecrawl-mcp",
        command: "npx",
        args: &["-y", "firecrawl-mcp"],
        homepage: "https://firecrawl.dev",
        description: "Scrape + crawl full sites into markdown",
        category: "Web",
    },
    McpPreset {
        id: "perplexity",
        name: "server-perplexity-ask",
        command: "npx",
        args: &["-y", "server-perplexity-ask"],
        homepage: "https://www.perplexity.ai",
        description: "Perplexity web answers",
        category: "Search",
    },
    McpPreset {
        id: "duckduckgo",
        name: "duckduckgo-mcp-server",
        command: "uvx",
        args: &["duckduckgo-mcp-server"],
        homepage: "https://duckduckgo.com",
        description: "DuckDuckGo instant-answer search",
        category: "Search",
    },
    // ---------- Productivity ----------
    McpPreset {
        id: "notion",
        name: "@notionhq/notion-mcp-server",
        command: "npx",
        args: &["-y", "@notionhq/notion-mcp-server"],
        homepage: "https://developers.notion.com",
        description: "Notion pages + databases",
        category: "Productivity",
    },
    McpPreset {
        id: "linear",
        name: "@linear/mcp-server",
        command: "npx",
        args: &["-y", "@linear/mcp-server"],
        homepage: "https://linear.app",
        description: "Linear issues + cycles",
        category: "Productivity",
    },
    McpPreset {
        id: "obsidian",
        name: "mcp-obsidian",
        command: "npx",
        args: &["-y", "mcp-obsidian"],
        homepage: "https://obsidian.md",
        description: "Read + write your Obsidian vault",
        category: "Productivity",
    },
    McpPreset {
        id: "todoist",
        name: "@doist/todoist-mcp",
        command: "npx",
        args: &["-y", "@doist/todoist-mcp"],
        homepage: "https://todoist.com",
        description: "Todoist tasks + projects",
        category: "Productivity",
    },
    McpPreset {
        id: "raycast",
        name: "raycast-mcp",
        command: "npx",
        args: &["-y", "raycast-mcp"],
        homepage: "https://raycast.com",
        description: "Raycast extensions bridge",
        category: "Productivity",
    },
    // ---------- Dev tooling ----------
    McpPreset {
        id: "playwright",
        name: "@playwright/mcp",
        command: "npx",
        args: &["-y", "@playwright/mcp"],
        homepage: "https://playwright.dev",
        description: "Browser automation via Playwright",
        category: "Dev",
    },
    McpPreset {
        id: "chrome-devtools",
        name: "chrome-devtools-mcp",
        command: "npx",
        args: &["-y", "chrome-devtools-mcp"],
        homepage: "https://github.com/chromedevtools/chrome-devtools-mcp",
        description: "Inspect running Chrome via DevTools protocol",
        category: "Dev",
    },
    McpPreset {
        id: "docker",
        name: "docker-mcp",
        command: "npx",
        args: &["-y", "docker-mcp"],
        homepage: "https://docker.com",
        description: "List + exec Docker containers",
        category: "Dev",
    },
    McpPreset {
        id: "kubernetes",
        name: "mcp-server-kubernetes",
        command: "npx",
        args: &["-y", "mcp-server-kubernetes"],
        homepage: "https://kubernetes.io",
        description: "kubectl-style cluster introspection",
        category: "Dev",
    },
    McpPreset {
        id: "cloudflare",
        name: "@cloudflare/mcp-server-cloudflare",
        command: "npx",
        args: &["-y", "@cloudflare/mcp-server-cloudflare"],
        homepage: "https://developers.cloudflare.com",
        description: "CF Workers / R2 / KV / DNS tools",
        category: "Dev",
    },
    McpPreset {
        id: "vercel",
        name: "@vercel/mcp-server",
        command: "npx",
        args: &["-y", "@vercel/mcp-server"],
        homepage: "https://vercel.com",
        description: "Vercel deploys + logs + envs",
        category: "Dev",
    },
    McpPreset {
        id: "supabase",
        name: "@supabase/mcp-server-supabase",
        command: "npx",
        args: &["-y", "@supabase/mcp-server-supabase"],
        homepage: "https://supabase.com",
        description: "Supabase project admin",
        category: "Dev",
    },
    McpPreset {
        id: "aws",
        name: "awslabs-mcp",
        command: "uvx",
        args: &["awslabs-mcp"],
        homepage: "https://aws.amazon.com",
        description: "AWS read-only service introspection",
        category: "Dev",
    },
    McpPreset {
        id: "terraform",
        name: "terraform-mcp-server",
        command: "npx",
        args: &["-y", "terraform-mcp-server"],
        homepage: "https://terraform.io",
        description: "Terraform state + plan helpers",
        category: "Dev",
    },
    // ---------- Data ----------
    McpPreset {
        id: "bigquery",
        name: "mcp-bigquery",
        command: "npx",
        args: &["-y", "mcp-bigquery"],
        homepage: "https://cloud.google.com/bigquery",
        description: "BigQuery read-only SQL",
        category: "Data",
    },
    McpPreset {
        id: "clickhouse",
        name: "@clickhouse/mcp-server",
        command: "npx",
        args: &["-y", "@clickhouse/mcp-server"],
        homepage: "https://clickhouse.com",
        description: "ClickHouse query tool",
        category: "Data",
    },
    McpPreset {
        id: "snowflake",
        name: "snowflake-mcp",
        command: "npx",
        args: &["-y", "snowflake-mcp"],
        homepage: "https://snowflake.com",
        description: "Snowflake SQL + warehouse info",
        category: "Data",
    },
    McpPreset {
        id: "mongodb",
        name: "mongodb-mcp-server",
        command: "npx",
        args: &["-y", "mongodb-mcp-server"],
        homepage: "https://mongodb.com",
        description: "MongoDB query + aggregation",
        category: "Data",
    },
    McpPreset {
        id: "redis",
        name: "redis-mcp",
        command: "npx",
        args: &["-y", "redis-mcp"],
        homepage: "https://redis.io",
        description: "Redis key/value inspection",
        category: "Data",
    },
    // ---------- System ----------
    McpPreset {
        id: "shell",
        name: "mcp-server-shell",
        command: "npx",
        args: &["-y", "mcp-server-shell"],
        homepage: "https://github.com/modelcontextprotocol/servers",
        description: "Run shell commands (sandboxed allow-list)",
        category: "System",
    },
    McpPreset {
        id: "macos-automator",
        name: "macos-automator-mcp",
        command: "npx",
        args: &["-y", "macos-automator-mcp"],
        homepage: "https://github.com/claude-did-this/macos-automator-mcp",
        description: "AppleScript bridge for macOS automation",
        category: "System",
    },
    McpPreset {
        id: "applescript",
        name: "applescript-mcp",
        command: "npx",
        args: &["-y", "applescript-mcp"],
        homepage: "https://github.com/joshrutkowski/applescript-mcp",
        description: "AppleScript runner (simpler alternative)",
        category: "System",
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
