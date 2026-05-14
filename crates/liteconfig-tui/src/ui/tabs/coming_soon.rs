//! Coming-soon panel for tabs that have a planned milestone but no shipped
//! workflow yet.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn render(
    frame: &mut Frame<'_>,
    app: &App,
    area: ratatui::layout::Rect,
    name: &str,
    milestone: &str,
) {
    let theme = app.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(false))
        .title(Span::styled(
            format!(" {name} "),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));

    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("The {name} tab lands in milestone {milestone}."),
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Until then, this tab is intentionally empty to keep the build green.",
            Style::default().fg(theme.muted),
        )),
    ];

    let p = Paragraph::new(body)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}
