//! Settings tab: read-only snapshot of the active settings, to give the user
//! a single place to confirm paths and defaults before features like overrides
//! land.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let s = &app.settings;

    let mut lines: Vec<Line<'_>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Paths",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )));
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

    lines.push(Line::from(Span::styled(
        "Defaults",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )));
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
    lines.push(Line::from(Span::styled(
        "GitHub backup",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(kv(
        "Enabled".into(),
        yes_no(s.github_backup.enabled).into(),
        theme,
    ));
    lines.push(kv("Repo".into(), s.github_backup.repo_url.clone(), theme));
    lines.push(kv("Branch".into(), s.github_backup.branch.clone(), theme));
    lines.push(kv(
        "Auto-sync".into(),
        yes_no(s.github_backup.auto_sync).into(),
        theme,
    ));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Available themes",
        Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
    )));
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

    let p = Paragraph::new(lines).block(
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

fn kv<'a>(k: String, v: String, theme: crate::theme::Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {k:<26} "), Style::default().fg(theme.muted)),
        Span::styled(v, Style::default().fg(theme.text)),
    ])
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
