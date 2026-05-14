# Contributing to liteconfig

Contributions are welcome! This guide covers building, testing, and extending liteconfig.

## Building

```sh
git clone https://github.com/liteconfig/liteconfig
cd liteconfig
cargo build --release
# Binary: target/release/liteconfig
```

## Testing

```sh
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

## Architecture

liteconfig is split into two crates:

### `liteconfig-core` (library)
Pure Rust library with no UI dependencies. Contains:
- **Agent adapters** (`src/agents/`) — one per agent (Claude, Codex, Gemini). Implements the `AgentAdapter` trait to read/write live configs.
- **Data model** (`src/model/`) — profiles, skills, MCP servers, rules.
- **SQLite database** (`src/db.rs`) — schema, migrations, queries at `~/.liteconfig/liteconfig.db`.
- **Services** (`src/services/`) — profile switching, skill sync, MCP sync, backup/restore, secrets management.
- **Filesystem utilities** (`src/fs_util.rs`) — atomic writes, symlink/copy, content hashing.

### `liteconfig-tui` (binary)
TUI built with [ratatui](https://ratatui.rs) and [crossterm](https://docs.rs/crossterm/). Contains:
- **Event loop** (`src/main.rs`) — terminal init, event dispatch.
- **UI tabs** (`src/ui/tabs/`) — one module per screen (Profiles, Skills, MCP, Rules, Backup, Sessions, Settings).
- **Widgets** (`src/ui/widgets/`) — reusable components (scrollable lists, agent popups, dialogs).
- **Theme system** (`src/theme.rs`) — built-in and user themes (JSON-driven).

### Data layout

Everything liteconfig owns lives under `~/.liteconfig/`:

```
~/.liteconfig/
├─ liteconfig.db          SQLite: profiles, skills, MCP, rules
├─ settings.json          Device-local: active profile, theme, path overrides
├─ secrets.local.json     Local-only: API keys (0600, never synced)
├─ skills/                Canonical skill dirs (symlinked into each agent)
├─ themes/                User-defined themes (JSON files override built-ins)
├─ backups/<timestamp>/   Local snapshots
└─ backup-repo/           Long-lived git clone for GitHub backup
```

Secrets never leave the machine. Profile configs store `@secret:<name>` placeholders that resolve at write time against `secrets.local.json`.

## Tech stack

- **Core library:** `rusqlite`, `serde`, `thiserror`, `git2`, `walkdir`, `tracing`
- **TUI:** `ratatui`, `crossterm`, `clap`, `fuzzy-matcher`, `arboard`, `color-eyre`
- **Testing:** `tempfile`, `chrono`, `uuid`

## Theme contribution

### Adding a built-in theme

1. Create `themes/my-theme.json` with your color palette:

```json
{
  "name": "My Theme",
  "author": "Your Name",
  "surface":      [30, 30, 30],
  "surface_alt":  [50, 50, 50],
  "primary":      [100, 150, 200],
  "secondary":    [200, 150, 100],
  "accent":       [150, 200, 100],
  "muted":        [100, 100, 100],
  "border":       [80, 80, 80],
  "border_focus": [100, 150, 200],
  "text":         [220, 220, 220],
  "text_dim":     [150, 150, 150],
  "success":      [100, 200, 100],
  "warning":      [200, 200, 100],
  "danger":       [200, 100, 100],
  "selection_bg": [50, 50, 50],
  "selection_fg": [220, 220, 220]
}
```

All color values are `[R, G, B]` arrays with values 0-255.

2. Update `crates/liteconfig-tui/src/theme.rs`:
   - Add `("my-theme", include_str!("../../../themes/my-theme.json")),` to the `BUILTIN_THEMES` list.

3. Test it:
   ```sh
   cargo run -p liteconfig-tui
   # In the app: press 7 to go to Settings, press t to cycle themes
   ```

### User-defined themes

Users can drop JSON files into `~/.liteconfig/themes/` using the same schema above. User themes with the same slug override built-in themes.

## Color tokens

| Token | Usage |
|-------|-------|
| `surface` | Main background |
| `surface_alt` | Secondary background (panels, popups) |
| `primary` | Headers, active selections, highlights |
| `secondary` | Accents, borders, labels |
| `accent` | Links, special actions |
| `muted` | Disabled text, timestamps, metadata |
| `border` | Inactive borders |
| `border_focus` | Active borders, focus indicators |
| `text` | Primary text |
| `text_dim` | Secondary text, hints |
| `success` | Success states, checkmarks |
| `warning` | Warnings, updates, attention |
| `danger` | Errors, destructive actions |
| `selection_bg` | Selected row background |
| `selection_fg` | Selected row text |

## Code style

- No unnecessary comments. Only add a comment when the **why** is non-obvious.
- Prefer small, focused functions over large abstractions.
- Use platform-independent paths (`dirs` crate, `PathBuf`).
- Write integration tests in `src/` alongside code (no separate `tests/` dir needed for simple tests).
- Keep the core library synchronous (no async); async is only in the TUI layer if needed.

## License

All contributions are licensed under Apache-2.0. By submitting a PR, you agree to license your work under this license.
