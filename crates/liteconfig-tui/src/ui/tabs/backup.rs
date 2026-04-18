//! Backup tab: list local snapshots and trigger snapshot / restore /
//! GitHub-push actions.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            " Backup ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 4 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    render_toolbar(frame, app, layout[0]);
    render_gh_status(frame, app, layout[1]);
    render_header(frame, app, layout[2]);
    render_list(frame, app, layout[3]);
    render_summary(frame, app, layout[4]);
}

fn render_toolbar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let line = Line::from(vec![
        button(" Snapshot now (n) ", theme.primary, theme),
        Span::raw("  "),
        button(" Restore (r) ", theme.accent, theme),
        Span::raw("  "),
        button(" Push GitHub (p) ", theme.accent, theme),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}

fn button(label: &str, color: ratatui::style::Color, theme: crate::theme::Theme) -> Span<'_> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme.selection_fg)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )
}

fn render_gh_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let gh = &app.settings.github_backup;
    let status = if !gh.enabled {
        "disabled".to_string()
    } else if gh.repo_url.is_empty() {
        "enabled · repo URL empty".to_string()
    } else {
        format!("→ {} ({})", gh.repo_url, gh.branch)
    };
    let line = Line::from(vec![
        Span::styled("GitHub backup: ", Style::default().fg(theme.muted)),
        Span::styled(status, Style::default().fg(theme.text)),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let header = Line::from(vec![Span::styled(
        format!("  {:<24}  {:>12}  {}", "Timestamp", "Size", "Path"),
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header).style(theme.default_style()), area);
}

fn render_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.backup_view;

    if view.snapshots.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no snapshots yet — press n to create one)",
            Style::default().fg(theme.muted),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style(false)),
        );
        frame.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem<'_>> = view
        .snapshots
        .iter()
        .map(|s| {
            let path = s.directory.display().to_string();
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<24}  ", s.timestamp),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:>10} KB  ", s.size_bytes / 1024),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(path, Style::default().fg(theme.muted)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style(true)),
        )
        .highlight_style(theme.selection_style())
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(view.focused_idx.min(view.snapshots.len() - 1)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let line = Line::from(vec![Span::styled(
        format!("  Snapshots: {}", app.backup_view.snapshots.len()),
        Style::default().fg(theme.muted),
    )]);
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}
