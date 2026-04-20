# liteconfig

A blazing-fast TUI for managing AI coding-agent configurations across **Claude Code**, **Codex**, and **Gemini** — all from one place. Mouse-friendly, keyboard-optimized, zero bloat.

- **Multi-profile switching.** Switch configs per agent with one keystroke. No restart.
- **Centralized skill management.** One library, auto-sync to agents via symlink or copy.
- **MCP servers.** Manage once, sync to each agent's format.
- **Rules and prompts.** Store once, sync to `CLAUDE.md`, `AGENTS.md`, or custom formats.
- **GitHub backups.** Local snapshots and encrypted sync. Secrets stay on your machine.

## Install

### Cargo (any platform)

```sh
cargo install liteconfig-tui
```

### curl (macOS / Linux)

```sh
curl -fsSL https://raw.githubusercontent.com/git-pi-e/liteconfig/main/install.sh | sh
```

The installer downloads a prebuilt binary for your platform, verifies the
checksum, and drops it in `~/.local/bin` (or a directory you choose with
`LITECONFIG_BIN_DIR=…`). Nothing else is touched.

### Homebrew (macOS / Linux)

```sh
brew install git-pi-e/tap/liteconfig
```

### Build from source

```sh
git clone https://github.com/git-pi-e/liteconfig
cd liteconfig
cargo build --release
# Binary: target/release/liteconfig
```

## Quickstart

```sh
# Launch — zero config; creates ~/.liteconfig/ on first run.
liteconfig
```

1. **Profiles** tab — press `n` to add a profile for the focused agent,
   `Enter` to switch. The live config (`~/.claude/settings.json` etc.) is
   written atomically; the outgoing profile is backfilled to the DB first.
2. **Skills** tab — `n` adds a skill, `a` opens the agent-enablement popup,
   `m` cycles symlink/copy/auto, `s` syncs selected, `Shift+S` syncs all.
3. **MCP** tab — `i` imports every server it finds across live configs,
   `a` re-targets per-row, `Shift+S` resyncs.
4. **Backup** tab — `n` snapshots to `~/.liteconfig/backups/<timestamp>/`;
   `r` restores; `p` pushes to the configured GitHub remote (secrets are
   never staged).

## How to use

Every action is **both clickable and keyboard-accessible**. No hidden modes. Vim users get `j`/`k`/`h`/`l` / `g`/`G` aliases, but they're entirely optional.

| Context | Key | Action |
|---|---|---|
| Global | `1`–`9` | Jump to tab N |
| Global | `Tab` / `Shift+Tab` | Next / previous tab |
| Global | `q` | Quit |
| Any table | `↑` / `↓` | Move focus |
| Any table | `Space` | Toggle selection |
| Any table | `a` | Open agent-enablement popup |
| Skills | `m` / `s` / `Shift+S` | Cycle method / sync selected / sync all |
| MCP | `i` / `Shift+S` / `d` | Import live / sync all / delete |
| Rules | `Shift+S` / `d` | Sync all / delete |
| Backup | `n` / `r` / `p` | Snapshot / restore / push to GitHub |
| Agent popup | `Space` / `A` / `N` / `Enter` / `Esc` | Toggle / all / none / OK / cancel |

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for shipped, in-progress, and planned features.

## License

Apache-2.0 © 2026 git-pi-e
