//! Plugins tab — installed Claude Code plugin bundles.
//!
//! Toolbar: `+ New (n)` opens the curated plugin marketplace popup; the
//! list below shows every installed plugin with counts of its bundled
//! skills / MCP / commands / subagents. `d d` uninstalls.

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
            " Plugins ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if inner.height < 6 {
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

    render_toolbar(frame, app, layout[0], hits);
    render_legend(frame, app, layout[1]);
    render_header(frame, app, layout[2]);
    render_list(frame, app, layout[3]);
    render_summary(frame, app, layout[4]);

    if app.presets_popup.is_some() {
        crate::ui::widgets::presets_popup::render(frame, app, area);
    }
}

fn render_toolbar(frame: &mut Frame<'_>, app: &App, area: Rect, hits: &mut Vec<ButtonHit>) {
    let theme = app.theme;
    let buttons = [
        ToolbarButton {
            label: " + New (n) ",
            color: theme.accent,
            action: ButtonAction::PluginsNew,
        },
        ToolbarButton {
            label: " Delete (d) ",
            color: theme.danger,
            action: ButtonAction::PluginsDelete,
        },
    ];
    button_bar::render(frame, theme, area, &buttons, hits);
}

fn render_legend(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let line = Line::from(vec![Span::styled(
        "Each plugin bundles skills + MCP servers + commands + subagents into a single install.",
        Style::default().fg(theme.muted),
    )]);
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let header = Line::from(vec![Span::styled(
        format!(
            "  {:<40}  {:<8}  {:<8}  {:<8}  {:<8}",
            "Name", "Skills", "MCP", "Cmds", "Agents"
        ),
        Style::default()
            .fg(theme.muted)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(Paragraph::new(header).style(theme.default_style()), area);
}

fn render_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.plugins_view;

    if view.plugins.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no plugins installed — press n to browse curated presets)",
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
        .plugins
        .iter()
        .map(|p| {
            let name_trunc = truncate(&p.name, 40);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<40}  ", name_trunc),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<8}  ", p.contents.skills),
                    Style::default().fg(theme.success),
                ),
                Span::styled(
                    format!("{:<8}  ", p.contents.mcp_servers),
                    Style::default().fg(theme.accent),
                ),
                Span::styled(
                    format!("{:<8}  ", p.contents.commands),
                    Style::default().fg(theme.text),
                ),
                Span::styled(
                    format!("{:<8}", p.contents.agents),
                    Style::default().fg(theme.text),
                ),
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
    state.select(Some(view.focused_idx.min(view.plugins.len() - 1)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let view = &app.plugins_view;
    let total: u32 = view
        .plugins
        .iter()
        .map(|p| {
            p.contents.skills + p.contents.mcp_servers + p.contents.commands + p.contents.agents
        })
        .sum();
    let line = Line::from(Span::styled(
        format!(
            "  {} plugin(s), {} bundled resource(s)",
            view.plugins.len(),
            total
        ),
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(Paragraph::new(line).style(theme.default_style()), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
