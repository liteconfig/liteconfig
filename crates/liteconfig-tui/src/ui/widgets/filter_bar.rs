//! Shared filter input, reused across Skills/MCP/Rules tabs. Rendered as a
//! bordered box so users read it as an input. Expects a 3-row slot.
//!
//! Three visual states: editing (accent border + cursor glyph), persisted
//! (muted border + dim text + active title), empty (muted border + hint).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme::Theme;

pub fn render(
    frame: &mut Frame<'_>,
    theme: Theme,
    area: Rect,
    filter: &str,
    editing: bool,
    empty_hint: &str,
) {
    let (border_style, title) = if editing {
        (
            Style::default().fg(theme.primary),
            " Filter · Enter keep · Esc clear ".to_string(),
        )
    } else if !filter.is_empty() {
        (
            Style::default().fg(theme.accent),
            format!(" Filter (active: {}) · / edit · Esc clear ", filter),
        )
    } else {
        (Style::default().fg(theme.muted), " Filter ".to_string())
    };

    let spans: Vec<Span<'_>> = if editing {
        vec![
            Span::styled(" / ", theme.accent_style()),
            Span::styled(
                filter.to_string(),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▏", Style::default().fg(theme.primary)),
        ]
    } else if !filter.is_empty() {
        vec![
            Span::styled(" / ", Style::default().fg(theme.muted)),
            Span::styled(filter.to_string(), Style::default().fg(theme.accent)),
        ]
    } else {
        vec![Span::styled(
            empty_hint.to_string(),
            Style::default().fg(theme.muted),
        )]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, border_style));
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(block)
            .style(theme.default_style()),
        area,
    );
}
