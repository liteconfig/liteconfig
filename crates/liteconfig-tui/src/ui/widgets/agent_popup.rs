//! Canonical agent-enablement popup. Opened from the Skills tab (and, later,
//! from MCP and Rules tabs) via `a`. Lets the user toggle a checkbox per
//! registered agent before committing with Enter.

use liteconfig_core::model::agent::ALL_AGENT_KINDS;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.agent_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" Agents for \"{}\" ", popup.row_name);
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

    let items: Vec<ListItem<'_>> = ALL_AGENT_KINDS
        .iter()
        .map(|agent| {
            let on = *popup.enabled.get(agent).unwrap_or(&false);
            let checkbox = if on { "☑ " } else { "☐ " };
            let box_style = if on {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            ListItem::new(Line::from(vec![
                Span::styled(checkbox, box_style),
                Span::styled(agent.display_name(), Style::default().fg(theme.text)),
                Span::raw("   "),
                Span::styled(
                    format!("({})", agent.id()),
                    Style::default().fg(theme.muted),
                ),
            ]))
        })
        .collect();

    // Leave the last inner row for the button hint line.
    let list_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };
    let hint_area = Rect {
        x: inner.x,
        y: inner.y + list_area.height,
        width: inner.width,
        height: inner.height.saturating_sub(list_area.height),
    };

    let list = List::new(items)
        .highlight_style(theme.selection_style())
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    state.select(Some(
        popup.cursor.min(ALL_AGENT_KINDS.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, list_area, &mut state);

    let hint = Line::from(vec![
        Span::styled(" Space ", theme.accent_style()),
        Span::styled("toggle   ", theme.muted_style()),
        Span::styled(" A ", theme.accent_style()),
        Span::styled("all   ", theme.muted_style()),
        Span::styled(" N ", theme.accent_style()),
        Span::styled("none   ", theme.muted_style()),
        Span::styled(" Enter ", theme.accent_style()),
        Span::styled("ok   ", theme.muted_style()),
        Span::styled(" Esc ", theme.accent_style()),
        Span::styled("cancel", theme.muted_style()),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        hint_area,
    );
}

/// Compute a centered rect of `pct_w`/`pct_h` percent within `area`.
fn centered_rect(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let w = (area.width as u32 * pct_w as u32 / 100) as u16;
    let h = (area.height as u32 * pct_h as u32 / 100) as u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.max(40).min(area.width),
        height: h.max(8).min(area.height),
    }
}
