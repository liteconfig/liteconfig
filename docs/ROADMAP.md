# Roadmap

Living document. Tracks shipped features, work in progress, and features we intend to pick up next. See CHANGELOG-style entries in git log for release history.

## Shipped

- **Four-agent support** — Claude Code, Codex, Gemini CLI, Cursor.
- **Auto-import on first launch** — Profiles, Skills, Rules, and MCP servers are pulled from `~/.claude`, `~/.codex`, `~/.gemini`, and `~/.cursor` the first time the DB is empty. A toast announces the counts.
- **Profile switching with backfill** — previous profile's live config is snapshotted before the new one is written, so ad-hoc edits aren't lost.
- **Skills table** — install from local dir, enable per agent, symlink- or copy-sync into each agent's `skills/` directory.
- **Rules** — single source of truth for `CLAUDE.md`, `AGENTS.md`, `.cursor/rules/*.mdc`.
- **MCP import/sync** — bidirectional with `~/.claude.json`, Codex config, Gemini settings, `~/.cursor/mcp.json`.
- **Local snapshots** — timestamped copies of the DB + skills tree under `~/.liteconfig/backups/`.
- **Secrets redaction** — `@secret:<name>` placeholders are resolved at write-time and re-redacted at read-time, so exports never contain raw keys.
- **Mouse tab bar** — correct hit-testing, clicks on labels switch tabs.
- **Theme gallery** — builtin + user themes loaded from `~/.liteconfig/themes/`.

## In progress

- **Skills filter** — `/` opens a substring match against name + description, updates "Showing X of Y". Applies to MCP and Rules tabs too.
- **Per-skill sync-method picker** — `M` opens a popup to pick `auto | symlink | copy | inherit` directly instead of cycling.
- **Column legends + `?` help overlay** — `?` on any tab opens a page-level help sheet; Skills legend explains Method, Source, and Status.
- **Background task runner + activity panel** — long ops (sync-all, GitHub push, repo clone) run on a worker thread with an in-app log visible via `L`. UI stays responsive.
- **External skill repos** — register `owner/repo` or a URL; clone into `~/.liteconfig/repos/<id>/`; scanned skills are deduped across sources by `(name, content_hash)`.
- **GitHub backup setup from Settings tab** — enable, set repo URL, set branch, toggle auto-sync — all without hand-editing `settings.json`.

## Planned / future

- **Deep links (`liteconfig://`)** — one-click share for a skill, MCP server, or rule, importable by a receiving liteconfig install.
- **Session browser** — list and reopen prior Claude Code / Codex / Gemini sessions across agents.
- **Usage + cost dashboard** — per-agent token/request stats pulled from each tool's local logs.
- **Cloud-backed config dir** — pointing liteconfig at a Dropbox / iCloud / OneDrive folder for cross-machine sync, plus WebDAV for self-hosted remotes.
- **Per-file Cursor rule writer** — currently Cursor rules sync is read-only because `.cursor/rules/*.mdc` is a dir-of-files, not a single concatenated file. Needs a per-rule writer.
- **Auto-update for skills** — periodic `git pull` for skill repos; re-scan + surface changes.
- **Windows testing matrix** — Windows builds exist today; we still need a CI job that exercises the full flow.
- **i18n** — UI strings in English only.
- **Auto-launch + system tray quick-switch** — parity with cc-switch's always-available profile switcher.

Contributions welcome on any of the above — open an issue first so we can align on scope.
