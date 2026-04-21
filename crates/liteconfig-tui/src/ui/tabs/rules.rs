//! Rules tab: markdown-body rules with per-agent enablement. Each enabled
//! rule is concatenated into its agent's rule file (CLAUDE.md etc.) on sync.

use liteconfig_core::model::agent::ALL_AGENT_KINDS;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::events::{ButtonAction, ButtonHit};
use crate::ui::widgets::button_bar::{self, ToolbarButton};

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect, hits: &mut Vec<ButtonHit>) {
    let theme = app.theme;

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            " Rules ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if inner.height < 8 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // toolbar
            Constraint::Length(1), // legend
            Constraint::Length(3), // filter bar (boxed)
            Constraint::Length(1), // header
            Constraint::Min(1),    // list
            Constraint::Length(1), // summary
        ])
        .split(inner);

    render_toolbar(frame, app, layout[0], hits);
    render_legend(frame, app, layout[1]);
    crate::ui::widgets::filter_bar::render(
        frame,
        app.theme,
        layout[2],
        &app.rules_view.filter,
        app.rules_view.filter_editing,
        " Press / to filter by name or body",
    );
    render_header(frame, app, layout[3]);
    render_list(frame, app, layout[4]);
    render_summary(frame, app, layout[5]);

    if app.agent_popup.is_some() {
        crate::ui::widgets::agent_popup::render(frame, app, area);
    }
}

fn render_toolbar(frame: &mut Frame<'_>, app: &App, area: Rect, hits: &mut Vec<ButtonHit>) {
    let theme = app.theme;
    let buttons = [
        ToolbarButton {
            label: " Sync all (S) ",
            color: theme.primary,
            action: ButtonAction::RulesSyncAll,
        },
        ToolbarButton {
            label: " Import live (i) ",
            color: theme.accent,
            action: ButtonAction::RulesImport,
        },
        ToolbarButton {
            label: " Delete (d) ",
            color: theme.danger,
            action: ButtonAction::RulesDelete,
        },
    ];
    button_bar::render(frame, theme, area, &buttons, hits);
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
        format!("  {:2}  {:<28}  {:<14}  {}", "", "Name", "Agents", "Body"),
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header).style(theme.default_style()), area);
}

fn render_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.rules_view;
    let filtered = app.filtered_rules_indices();

    if view.rules.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no rules yet)",
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
        .filter_map(|i| view.rules.get(*i))
        .map(|r| {
            let selected = view.selected_ids.contains(&r.id);
            let checkbox = if selected { "☑ " } else { "☐ " };

            let total = ALL_AGENT_KINDS.len();
            let on = r.enabled.values().filter(|v| **v).count();
            let mut pill = format!("{on}/{total} ");
            for agent in ALL_AGENT_KINDS {
                pill.push(if *r.enabled.get(agent).unwrap_or(&false) {
                    '●'
                } else {
                    '○'
                });
            }

            let preview: String = r
                .body
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();

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
                    format!("{:<28}  ", truncate(&r.name, 28)),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:<14}  ", pill),
                    Style::default().fg(theme.success),
                ),
                Span::styled(preview, Style::default().fg(theme.muted)),
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
    let view = &app.rules_view;
    let shown = app.filtered_rules_indices().len();
    let line = Line::from(vec![Span::styled(
        format!(
            "  Selected: {} · Showing {} of {}",
            view.selected_ids.len(),
            shown,
            view.rules.len()
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
