//! One-line toolbar of clickable pill buttons. Each button records its
//! rect into `ButtonHit`s so the event loop can dispatch mouse clicks.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::events::{ButtonAction, ButtonHit};
use crate::theme::Theme;

pub struct ToolbarButton {
    pub label: &'static str,
    pub color: Color,
    pub action: ButtonAction,
}

pub fn render(
    frame: &mut Frame<'_>,
    theme: Theme,
    area: Rect,
    buttons: &[ToolbarButton],
    hits: &mut Vec<ButtonHit>,
) {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(buttons.len() * 2);
    let mut x = area.x;
    for (i, b) in buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
            x = x.saturating_add(2);
        }
        let w = b.label.chars().count() as u16;
        // Clip to area width so hit rects never claim cells beyond the toolbar.
        let right_edge = area.x.saturating_add(area.width);
        if x >= right_edge {
            break;
        }
        let clamped_w = w.min(right_edge.saturating_sub(x));
        hits.push(ButtonHit {
            action: b.action,
            x,
            y: area.y,
            w: clamped_w,
            h: 1,
        });
        spans.push(Span::styled(
            b.label.to_string(),
            Style::default()
                .fg(theme.selection_fg)
                .bg(b.color)
                .add_modifier(Modifier::BOLD),
        ));
        x = x.saturating_add(w);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.default_style()),
        area,
    );
}
