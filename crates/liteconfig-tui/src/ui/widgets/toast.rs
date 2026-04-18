//! Stacked toast overlay. Renders in the upper-right of the given area.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, ToastLevel};

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.toasts.is_empty() {
        return;
    }
    let theme = app.theme;

    let width = 56u16.min(area.width.saturating_sub(4));
    let mut y = area.y + 2;

    for toast in app.toasts.iter().rev() {
        let color = match toast.level {
            ToastLevel::Info => theme.primary,
            ToastLevel::Success => theme.success,
            ToastLevel::Warning => theme.warning,
            ToastLevel::Error => theme.danger,
        };
        let icon = match toast.level {
            ToastLevel::Info => "i",
            ToastLevel::Success => "✓",
            ToastLevel::Warning => "!",
            ToastLevel::Error => "✗",
        };
        let wrapped: Vec<&str> = toast
            .message
            .as_str()
            .split_inclusive(char::is_whitespace)
            .collect();
        let height = 3u16 + (wrapped.len() as u16 / 6).min(3);
        if y + height > area.y + area.height {
            break;
        }
        let rect = Rect {
            x: area.x + area.width.saturating_sub(width + 2),
            y,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(Span::styled(
                format!(" {icon} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        let p = Paragraph::new(Line::from(Span::styled(
            toast.message.clone(),
            Style::default().fg(theme.text),
        )))
        .block(block);
        frame.render_widget(p, rect);
        y += height;
    }
}
