//! Full-screen help overlay toggled from `?` on every tab. Pulls its body
//! from a per-tab `help_text` block so each tab owns its own key reference.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Tab};

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if !app.show_help {
        return;
    }
    let theme = app.theme;

    let popup_area = centered_rect(80, 80, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" Help — {} ", app.active_tab.title());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 3 {
        return;
    }

    let lines: Vec<Line<'_>> = help_text(app.active_tab)
        .iter()
        .map(|entry| match entry {
            HelpEntry::Section(s) => Line::from(Span::styled(
                s.to_string(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            HelpEntry::Key { key, desc } => Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<14}", key),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(theme.text)),
            ]),
            HelpEntry::Text(s) => Line::from(Span::styled(
                s.to_string(),
                Style::default().fg(theme.muted),
            )),
            HelpEntry::Blank => Line::from(Span::raw("")),
        })
        .collect();

    let body_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    let hint_area = Rect {
        x: inner.x,
        y: inner.y + body_area.height,
        width: inner.width,
        height: inner.height.saturating_sub(body_area.height),
    };

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.default_style())
            .wrap(Wrap { trim: false }),
        body_area,
    );

    let hint = Line::from(vec![
        Span::styled(" ?/Esc ", theme.accent_style()),
        Span::styled("close", theme.muted_style()),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        hint_area,
    );
}

#[derive(Debug, Clone)]
enum HelpEntry {
    Section(&'static str),
    Key {
        key: &'static str,
        desc: &'static str,
    },
    Text(&'static str),
    Blank,
}

fn help_text(tab: Tab) -> &'static [HelpEntry] {
    match tab {
        Tab::Profiles => PROFILES,
        Tab::Skills => SKILLS,
        Tab::Mcp => MCP,
        Tab::Rules => RULES,
        Tab::Backup => BACKUP,
        Tab::Sessions => SESSIONS,
        Tab::Settings => SETTINGS,
    }
}

const GLOBAL_FOOTER: [HelpEntry; 7] = [
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key {
        key: "Tab / ⇥",
        desc: "next tab",
    },
    HelpEntry::Key {
        key: "1–7",
        desc: "jump to tab N",
    },
    HelpEntry::Key {
        key: "L",
        desc: "open activity log (background jobs)",
    },
    HelpEntry::Key {
        key: "?",
        desc: "open this help overlay",
    },
    HelpEntry::Key {
        key: "q / Ctrl-C",
        desc: "quit",
    },
];

const PROFILES: &[HelpEntry] = &[
    HelpEntry::Section("Profiles"),
    HelpEntry::Text("One row per agent. Pick a profile to make it the live config."),
    HelpEntry::Blank,
    HelpEntry::Key {
        key: "↑/↓ or j/k",
        desc: "change selection within the focused agent",
    },
    HelpEntry::Key {
        key: "←/→ or h/l",
        desc: "move focus between agent rows",
    },
    HelpEntry::Key {
        key: "Enter or s",
        desc: "switch the focused agent to the highlighted profile",
    },
    HelpEntry::Key {
        key: "i",
        desc: "import profiles from whatever is live on disk now",
    },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key {
        key: "Tab",
        desc: "next tab",
    },
    HelpEntry::Key {
        key: "1–7",
        desc: "jump to tab N",
    },
    HelpEntry::Key {
        key: "L",
        desc: "open activity log",
    },
    HelpEntry::Key {
        key: "?",
        desc: "this help",
    },
    HelpEntry::Key {
        key: "q / Ctrl-C",
        desc: "quit",
    },
];

const SKILLS: &[HelpEntry] = &[
    HelpEntry::Section("Skills"),
    HelpEntry::Text("Library of reusable skills. Each row can be enabled for any subset of agents, synced with one of four sync methods, and originate from either a local path or a GitHub repo."),
    HelpEntry::Blank,
    HelpEntry::Section("Columns"),
    HelpEntry::Key { key: "Agents", desc: "● = enabled, ○ = off, in fixed agent order" },
    HelpEntry::Key { key: "Method", desc: "inherit = use workspace default, symlink, copy, auto = OS-picked" },
    HelpEntry::Key { key: "Source", desc: "local = filesystem install, github = cloned from a repo" },
    HelpEntry::Key { key: "Status", desc: "in sync = hash matches last sync; unknown = never synced" },
    HelpEntry::Blank,
    HelpEntry::Section("Navigation + selection"),
    HelpEntry::Key { key: "↑/↓ or j/k", desc: "move focus" },
    HelpEntry::Key { key: "Space", desc: "toggle selection on focused row" },
    HelpEntry::Key { key: "Ctrl-A", desc: "select all · Ctrl-Shift-A clears" },
    HelpEntry::Key { key: "/", desc: "filter by name/description (Esc clear, Enter keep)" },
    HelpEntry::Blank,
    HelpEntry::Section("Actions"),
    HelpEntry::Key { key: "a", desc: "agent-enablement popup (which agents sync this skill)" },
    HelpEntry::Key { key: "m", desc: "cycle sync method on focused row" },
    HelpEntry::Key { key: "M", desc: "pick sync method from a list (auto / symlink / copy / inherit)" },
    HelpEntry::Key { key: "Enter", desc: "sync focused skill to its enabled agents" },
    HelpEntry::Key { key: "s", desc: "sync all selected" },
    HelpEntry::Key { key: "S", desc: "sync every skill (runs in background for file-backed DBs)" },
    HelpEntry::Key { key: "i", desc: "import skills from live agent dirs on disk" },
    HelpEntry::Key { key: "n", desc: "open the new-skill chooser (presets / search / paste URL)" },
    HelpEntry::Key { key: "Ctrl+F", desc: "live-search skills.sh and install a hit" },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key { key: "Tab / 1-7", desc: "tabs · L activity · ? help · q quit" },
];

const MCP: &[HelpEntry] = &[
    HelpEntry::Section("MCP servers"),
    HelpEntry::Text("Model Context Protocol servers. Sync writes the enabled-for-agent list into each agent's MCP config file."),
    HelpEntry::Blank,
    HelpEntry::Key { key: "↑/↓", desc: "move focus" },
    HelpEntry::Key { key: "Space", desc: "toggle selection" },
    HelpEntry::Key { key: "/", desc: "filter by name or command" },
    HelpEntry::Key { key: "a", desc: "agent popup" },
    HelpEntry::Key { key: "n", desc: "open the new-MCP chooser (curated presets: fetch / time / memory / …)" },
    HelpEntry::Key { key: "i", desc: "import MCP servers from live configs" },
    HelpEntry::Key { key: "d", desc: "delete focused server" },
    HelpEntry::Key { key: "S", desc: "sync to all agents (background)" },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key { key: "Tab / 1-7", desc: "tabs · L activity · ? help · q quit" },
];

const RULES: &[HelpEntry] = &[
    HelpEntry::Section("Rules"),
    HelpEntry::Text("Each rule is a markdown body concatenated into each enabled agent's rule file (CLAUDE.md, AGENTS.md, .cursor/rules/*.mdc, …)."),
    HelpEntry::Blank,
    HelpEntry::Key { key: "↑/↓", desc: "move focus" },
    HelpEntry::Key { key: "Space", desc: "toggle selection" },
    HelpEntry::Key { key: "/", desc: "filter by name or body" },
    HelpEntry::Key { key: "a", desc: "agent popup" },
    HelpEntry::Key { key: "i", desc: "import rules from live configs" },
    HelpEntry::Key { key: "d", desc: "delete focused rule" },
    HelpEntry::Key { key: "S", desc: "sync to all agents (background)" },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key { key: "Tab / 1-7", desc: "tabs · L activity · ? help · q quit" },
];

const BACKUP: &[HelpEntry] = &[
    HelpEntry::Section("Backup"),
    HelpEntry::Text("Local snapshots of ~/.liteconfig, plus optional GitHub mirror."),
    HelpEntry::Blank,
    HelpEntry::Key {
        key: "n",
        desc: "create snapshot now",
    },
    HelpEntry::Key {
        key: "r",
        desc: "restore focused snapshot",
    },
    HelpEntry::Key {
        key: "p",
        desc: "push backup to GitHub (requires setup in Settings → GitHub backup)",
    },
    HelpEntry::Key {
        key: "d d",
        desc: "delete focused snapshot (press twice within 2s to confirm)",
    },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key {
        key: "Tab / 1-7",
        desc: "tabs · L activity · ? help · q quit",
    },
];

const SESSIONS: &[HelpEntry] = &[
    HelpEntry::Section("Sessions"),
    HelpEntry::Text("(placeholder — session browsing lands in a later release)"),
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key {
        key: "Tab / 1-7",
        desc: "tabs · L activity · ? help · q quit",
    },
];

const SETTINGS: &[HelpEntry] = &[
    HelpEntry::Section("Settings"),
    HelpEntry::Text("Global preferences. Some rows are interactive; the rest reflect ~/.liteconfig/settings.json."),
    HelpEntry::Blank,
    HelpEntry::Key { key: "t", desc: "cycle theme (builtin + ~/.liteconfig/themes/*.toml)" },
    HelpEntry::Blank,
    HelpEntry::Section("Global"),
    HelpEntry::Key { key: "Tab / 1-7", desc: "tabs · L activity · ? help · q quit" },
];

// Silence dead-code warning for the shared footer while every tab still
// spells its own block out.
#[allow(dead_code)]
const _KEEP: &[HelpEntry] = &GLOBAL_FOOTER;

fn centered_rect(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let w = (area.width as u32 * pct_w as u32 / 100) as u16;
    let h = (area.height as u32 * pct_h as u32 / 100) as u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.max(60).min(area.width),
        height: h.max(12).min(area.height),
    }
}
