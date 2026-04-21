//! Skills tab: table of installed skills with per-row agent enablement pill,
//! sync method, source, and status. Supports multi-select for batch sync.
//!
//! Keyboard: ↑/↓ moves focus, Space toggles selection, a opens the agent
//! popup, m cycles the sync method, s syncs selected, Shift+S syncs all,
//! Enter triggers a sync of the focused row.

use liteconfig_core::model::agent::ALL_AGENT_KINDS;
use liteconfig_core::model::skill::SkillSource;
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
            " Skills ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if inner.height < 5 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // toolbar (buttons)
            Constraint::Length(1), // legend
            Constraint::Length(1), // filter bar
            Constraint::Length(1), // column header
            Constraint::Min(1),    // list
            Constraint::Length(1), // summary footer
        ])
        .split(inner);

    render_toolbar(frame, app, layout[0]);
    render_legend(frame, app, layout[1]);
    render_filter(frame, app, layout[2]);
    render_header(frame, app, layout[3]);
    render_list(frame, app, layout[4]);
    render_summary(frame, app, layout[5]);

    // If the agent popup is open, overlay it.
    if app.agent_popup.is_some() {
        crate::ui::widgets::agent_popup::render(frame, app, area);
    }
    if app.method_popup.is_some() {
        crate::ui::widgets::method_popup::render(frame, app, area);
    }
}

fn render_toolbar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let line = Line::from(vec![
        button(" + New ", theme.accent, theme),
        Span::raw("  "),
        button(" Import… ", theme.accent, theme),
        Span::raw("  "),
        button(" Sync selected (s) ", theme.primary, theme),
        Span::raw("  "),
        button(" Sync all (S) ", theme.primary, theme),
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

fn render_legend(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let mut spans: Vec<Span<'_>> = vec![
        Span::styled("Agents: ", Style::default().fg(theme.muted)),
        Span::styled("● on ", Style::default().fg(theme.success)),
        Span::styled("○ off ", Style::default().fg(theme.muted)),
    ];
    for agent in ALL_AGENT_KINDS {
        spans.push(Span::styled(
            format!("{} ", agent.short_label()),
            Style::default().fg(theme.text),
        ));
    }
    spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
    spans.push(Span::styled(
        "Method: inherit=workspace default, symlink, copy, auto=OS-picked",
        Style::default().fg(theme.muted),
    ));
    spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
    spans.push(Span::styled(
        "Source: local/github",
        Style::default().fg(theme.muted),
    ));
    spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
    spans.push(Span::styled(
        "Status: in sync/unknown  (press ?)",
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.default_style()),
        area,
    );
}

fn render_filter(frame: &mut Frame<'_>, app: &App, area: Rect) {
    crate::ui::widgets::filter_bar::render(
        frame,
        app.theme,
        area,
        &app.skills_view.filter,
        app.skills_view.filter_editing,
        " Press / to filter by name or description",
    );
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let header = Line::from(vec![Span::styled(
        format!(
            "  {:2}  {:<32}  {:<14}  {:<10}  {:<8}  {}",
            "", "Name", "Agents", "Method", "Source", "Status"
        ),
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header).style(theme.default_style()), area);
}

fn render_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.skills_view;
    let filtered = app.filtered_skill_indices();

    if view.skills.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no skills installed yet — press n to add one)",
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

    if filtered.is_empty() {
        let p = Paragraph::new(Span::styled(
            format!("(no matches for \"{}\" — Esc to clear)", view.filter),
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

    let items: Vec<ListItem<'_>> = filtered
        .iter()
        .filter_map(|i| view.skills.get(*i))
        .map(|s| {
            let selected = view.selected_ids.contains(&s.id);
            let checkbox = if selected { "☑ " } else { "☐ " };

            let mut pill = String::new();
            let total = ALL_AGENT_KINDS.len();
            let on = s.enabled_count();
            pill.push_str(&format!("{on}/{total} "));
            for agent in ALL_AGENT_KINDS {
                pill.push(if s.is_enabled_for(*agent) {
                    '●'
                } else {
                    '○'
                });
            }

            let source = match &s.source {
                SkillSource::Local => "local",
                SkillSource::Github { .. } => "github",
            };

            let status = if s
                .content_hash
                .as_deref()
                .map(|h| !h.is_empty())
                .unwrap_or(false)
            {
                "in sync"
            } else {
                "unknown"
            };

            let name_trunc = truncate(&s.name, 32);
            let method_trunc = truncate(s.sync_method.as_str(), 14);

            let checkbox_style = if selected {
                Style::default()
                    .fg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };

            ListItem::new(Line::from(vec![
                Span::styled(checkbox, checkbox_style),
                Span::styled(
                    format!("{:<32}  ", name_trunc),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:<14}  ", pill),
                    Style::default().fg(theme.success),
                ),
                Span::styled(
                    format!("{:<10}  ", method_trunc),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(format!("{:<8}  ", source), Style::default().fg(theme.muted)),
                Span::styled(status, Style::default().fg(theme.muted)),
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
    if !filtered.is_empty() {
        state.select(Some(view.focused_idx.min(filtered.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.skills_view;
    let shown = app.filtered_skill_indices().len();
    let line = Line::from(vec![Span::styled(
        format!(
            "  Selected: {} · Showing {} of {}    Method (focused): {}",
            view.selected_ids.len(),
            shown,
            view.skills.len(),
            app.focused_sync_method_label()
        ),
        Style::default().fg(theme.muted),
    )]);
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
