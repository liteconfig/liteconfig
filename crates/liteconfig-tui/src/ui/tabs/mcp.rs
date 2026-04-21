//! MCP servers tab: one row per server, with a per-agent enablement pill,
//! source command preview, and last-modified hint. Buttons sync to all
//! agents or import from live configs.

use liteconfig_core::model::agent::ALL_AGENT_KINDS;
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
            " MCP Servers ",
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
            Constraint::Length(1), // toolbar
            Constraint::Length(1), // legend
            Constraint::Length(1), // filter bar
            Constraint::Length(1), // header
            Constraint::Min(1),    // list
            Constraint::Length(1), // summary
        ])
        .split(inner);

    render_toolbar(frame, app, layout[0]);
    render_legend(frame, app, layout[1]);
    crate::ui::widgets::filter_bar::render(
        frame,
        app.theme,
        layout[2],
        &app.mcp_view.filter,
        app.mcp_view.filter_editing,
        " Press / to filter by name or command",
    );
    render_header(frame, app, layout[3]);
    render_list(frame, app, layout[4]);
    render_summary(frame, app, layout[5]);

    if app.agent_popup.is_some() {
        crate::ui::widgets::agent_popup::render(frame, app, area);
    }
}

fn render_toolbar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let line = Line::from(vec![
        button(" Sync all (S) ", theme.primary, theme),
        Span::raw("  "),
        button(" Import live (i) ", theme.accent, theme),
        Span::raw("  "),
        button(" Delete (d) ", theme.danger, theme),
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
        Span::styled("Legend — agents: ", Style::default().fg(theme.muted)),
        Span::styled("● on  ", Style::default().fg(theme.success)),
        Span::styled("○ off    ", Style::default().fg(theme.muted)),
    ];
    for agent in ALL_AGENT_KINDS {
        spans.push(Span::styled(
            format!("{} ", agent.short_label()),
            Style::default().fg(theme.text),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.default_style()),
        area,
    );
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let header = Line::from(vec![Span::styled(
        format!(
            "  {:2}  {:<28}  {:<14}  {}",
            "", "Name", "Agents", "Command"
        ),
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header).style(theme.default_style()), area);
}

fn render_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.mcp_view;
    let filtered = app.filtered_mcp_indices();

    if view.servers.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no MCP servers yet — press i to import from live configs)",
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
        .filter_map(|i| view.servers.get(*i))
        .map(|s| {
            let selected = view.selected_ids.contains(&s.id);
            let checkbox = if selected { "☑ " } else { "☐ " };

            let total = ALL_AGENT_KINDS.len();
            let on = s.enabled.iter().filter(|(_, v)| **v).count();
            let mut pill = format!("{on}/{total} ");
            for agent in ALL_AGENT_KINDS {
                pill.push(if s.is_enabled_for(*agent) {
                    '●'
                } else {
                    '○'
                });
            }

            let command = s
                .config
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

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
                    format!("{:<28}  ", truncate(&s.name, 28)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:<14}  ", pill),
                    Style::default().fg(theme.success),
                ),
                Span::styled(truncate(&command, 50), Style::default().fg(theme.muted)),
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
    state.select(Some(view.focused_idx.min(filtered.len() - 1)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.mcp_view;
    let shown = app.filtered_mcp_indices().len();
    let line = Line::from(vec![Span::styled(
        format!(
            "  Selected: {} · Showing {} of {}",
            view.selected_ids.len(),
            shown,
            view.servers.len()
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
