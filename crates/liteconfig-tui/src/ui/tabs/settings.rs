//! Settings tab: live read-out of `~/.liteconfig/settings.json` with an
//! editable GitHub-backup section (↑/↓ to focus a row, Space to toggle
//! booleans, Enter to edit text rows).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, SettingsRow};
use crate::theme::Theme;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let s = &app.settings;

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(header("Paths", theme));
    for agent in liteconfig_core::model::agent::ALL_AGENT_KINDS {
        if let Ok(adapter) = liteconfig_core::agents::for_kind(*agent) {
            if let Ok(paths) = adapter.paths(s) {
                lines.push(kv(
                    format!("{} live", agent.display_name()),
                    paths.live_settings.display().to_string(),
                    theme,
                ));
            }
        }
    }
    lines.push(Line::from(""));

    lines.push(header("Defaults", theme));
    lines.push(kv("Theme (t to cycle)".into(), s.theme.clone(), theme));
    lines.push(kv(
        "Skill storage".into(),
        s.skill_storage_location.as_str().into(),
        theme,
    ));
    lines.push(kv(
        "Skill sync default".into(),
        s.skill_sync_method_default.as_str().into(),
        theme,
    ));
    lines.push(kv(
        "Confirm before write".into(),
        yes_no(s.confirm_before_write).into(),
        theme,
    ));

    lines.push(Line::from(""));
    lines.push(header("GitHub backup", theme));
    lines.push(Line::from(Span::styled(
        "  ↑/↓ focus · Space toggle bool · Enter edit text · Esc cancel",
        Style::default().fg(theme.muted),
    )));

    let focused = app.settings_view.focused_row;
    let editing = app.settings_view.input_buf.is_some();
    let buf = app.settings_view.input_buf.as_deref();

    lines.push(gh_row(
        SettingsRow::GhEnabled,
        yes_no(s.github_backup.enabled).into(),
        focused,
        editing,
        buf,
        theme,
    ));
    lines.push(gh_row(
        SettingsRow::GhRepoUrl,
        if s.github_backup.repo_url.is_empty() {
            "(not set — press Enter to edit)".into()
        } else {
            s.github_backup.repo_url.clone()
        },
        focused,
        editing,
        buf,
        theme,
    ));
    lines.push(gh_row(
        SettingsRow::GhBranch,
        s.github_backup.branch.clone(),
        focused,
        editing,
        buf,
        theme,
    ));
    lines.push(gh_row(
        SettingsRow::GhAutoSync,
        yes_no(s.github_backup.auto_sync).into(),
        focused,
        editing,
        buf,
        theme,
    ));

    lines.push(Line::from(""));
    lines.push(header("Available themes", theme));
    for slug in &app.available_themes {
        let active = slug == &app.settings.theme;
        let marker = if active { "▶ " } else { "  " };
        lines.push(Line::from(vec![Span::styled(
            format!("  {marker}{slug}"),
            if active {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            },
        )]));
    }

    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(false))
            .title(Span::styled(
                " Settings ",
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(p, area);
}

fn header<'a>(text: &'a str, theme: Theme) -> Line<'a> {
    Line::from(Span::styled(
        text,
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    ))
}

fn kv<'a>(k: String, v: String, theme: Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {k:<26} "), Style::default().fg(theme.muted)),
        Span::styled(v, Style::default().fg(theme.text)),
    ])
}

/// One GitHub-backup row. When focused, prefixes `▶ ` and uses the accent
/// color. When editing *and* focused on a text row, swaps the value for the
/// live input buffer plus a cursor glyph.
fn gh_row<'a>(
    row: SettingsRow,
    display_value: String,
    focused: Option<SettingsRow>,
    editing: bool,
    buf: Option<&str>,
    theme: Theme,
) -> Line<'a> {
    let is_focused = focused == Some(row);
    let arrow = if is_focused { "▶ " } else { "  " };
    let label_style = if is_focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };

    let mut spans: Vec<Span<'_>> = vec![
        Span::styled(arrow.to_string(), Style::default().fg(theme.accent)),
        Span::styled(format!("{:<26} ", row.label()), label_style),
    ];

    if is_focused && editing && row.is_text() {
        spans.push(Span::styled(
            buf.unwrap_or("").to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("▏", Style::default().fg(theme.primary)));
        spans.push(Span::styled(
            "   (Enter save · Esc cancel)",
            Style::default().fg(theme.muted),
        ));
    } else {
        spans.push(Span::styled(display_value, Style::default().fg(theme.text)));
    }

    Line::from(spans)
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
