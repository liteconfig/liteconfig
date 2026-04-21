//! Shared one-line filter input, reused across Skills/MCP/Rules tabs.
//! Three states: editing (cursor glyph + hints), persisted (dimmed filter
//! string), empty (help hint only).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
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
    let spans: Vec<Span<'_>> = if editing {
        vec![
            Span::styled(" / ", theme.accent_style()),
            Span::styled(
                filter.to_string(),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▏", Style::default().fg(theme.primary)),
            Span::styled(
                "   Enter keep · Esc clear · Backspace del",
                Style::default().fg(theme.muted),
            ),
        ]
    } else if !filter.is_empty() {
        vec![
            Span::styled(" / ", Style::default().fg(theme.muted)),
            Span::styled(filter.to_string(), Style::default().fg(theme.accent)),
            Span::styled(
                "   (press / to edit, Esc to clear)",
                Style::default().fg(theme.muted),
            ),
        ]
    } else {
        vec![Span::styled(
            empty_hint.to_string(),
            Style::default().fg(theme.muted),
        )]
    };
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.default_style()),
        area,
    );
}
