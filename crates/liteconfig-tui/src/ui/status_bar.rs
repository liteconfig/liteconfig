//! One-line status bar pinned to the bottom. Shows the focused agent, the
//! current live-config path, and any GitHub backup state.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let gh = if app.settings.github_backup.enabled {
        format!("GH: ●{}", app.settings.github_backup.branch)
    } else {
        "GH: off".to_string()
    };
    let line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(app.live_config_hint(), Style::default().fg(theme.text_dim)),
        Span::raw("   "),
        Span::styled(gh, Style::default().fg(theme.muted)),
    ]);
    let p = Paragraph::new(line).style(theme.default_style());
    frame.render_widget(p, area);
}
