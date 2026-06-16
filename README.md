# liteconfig

A blazing-fast TUI for managing AI coding-agent configurations across **Claude Code**, **Codex**, **Gemini CLI**, **Cursor**, and **OpenCode** — all from one place. Mouse-friendly, keyboard-optimized, zero bloat.

- **Multi-profile switching.** Switch configs per agent with one keystroke. No restart.
- **Centralized skill management.** One library, auto-sync to agents via symlink or copy.
- **MCP servers.** Manage once, sync to each agent's format.
- **Rules and prompts.** Store once, sync to `CLAUDE.md`, `AGENTS.md`, or custom formats.
- **Plugin bundles.** Install Claude Code plugin repos and surface bundled skills.
- **GitHub backups.** Local snapshots and encrypted sync. Secrets stay on your machine.

## Install

### Supported prebuilt targets

| Target | Prebuilt channels | Fallback |
|---|---|---|
| macOS `arm64`, `x86_64` | `pnpx`, `install.sh`, Homebrew | Not needed |
| Linux glibc `x86_64`, `aarch64` | `pnpx`, `install.sh`, Homebrew | Not needed |
| Linux musl / other Linux arches | Not published | `cargo install liteconfig-tui` |

### Homebrew (macOS / Linux glibc `x86_64` / `aarch64`)

```sh
brew tap liteconfig/tap
brew install liteconfig
```

The tap formula is published from tagged releases to
`liteconfig/homebrew-tap`.

### Cargo (any platform with a Rust toolchain)

```sh
cargo install liteconfig-tui
```

Use this on Alpine/musl systems and any unsupported architecture.

### pnpx (supported prebuilt targets)

```sh
pnpx liteconfig
```

The npm package is a small launcher. On first run it downloads the matching
GitHub release binary for the supported prebuilt targets above, verifies it
against `SHA256SUMS`, caches it in your system cache directory, then starts
the TUI. Unsupported Linux targets should use Cargo instead.

### curl (macOS / Linux glibc `x86_64` / `aarch64`)

```sh
curl -fsSL https://raw.githubusercontent.com/liteconfig/liteconfig/main/install.sh | sh
```

The installer downloads a prebuilt binary for your platform, requires a valid
`SHA256SUMS` entry for that exact asset, verifies the checksum, and drops it in
`~/.local/bin` (or a directory you choose with `LITECONFIG_BIN_DIR=…`). If the
checksum metadata is unavailable or does not match, installation stops.

### Build from source

```sh
git clone https://github.com/liteconfig/liteconfig
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
2. **Skills** tab — `n` opens curated repos, `p` installs through `skills`,
   `/` filters, `a` opens the agent-enablement popup, `M` picks sync method,
   `s` syncs selected, `Shift+S` syncs all.
3. **MCP** tab — `i` imports every server it finds across live configs,
   `n` opens the marketplace, `/` filters, `a` re-targets per-row,
   `Shift+S` resyncs.
4. **Rules** tab — `/` filters, `a` re-targets a rule per agent,
   `Shift+S` writes enabled rules to live files.
5. **Plugins** tab — `n` installs a curated Claude Code plugin bundle;
   `d d` removes the focused plugin.
6. **Backup** tab — `n` snapshots to `~/.liteconfig/backups/<timestamp>/`;
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
| Skills | `/` / `M` / `s` / `Shift+S` | Filter / pick method / sync selected / sync all |
| MCP | `/` / `n` / `i` / `Shift+S` / `d` | Filter / marketplace / import live / sync all / delete |
| Rules | `Shift+S` / `d` | Sync all / delete |
| Plugins | `n` / `d d` | Install curated bundle / delete |
| Backup | `n` / `r` / `p` | Snapshot / restore / push to GitHub |
| Agent popup | `Space` / `A` / `N` / `Enter` / `Esc` | Toggle / all / none / OK / cancel |

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for shipped, in-progress, and planned features.

## License

Apache-2.0 © 2026 liteconfig
