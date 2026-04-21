//! One-line status bar pinned to the bottom. Shows the focused agent, the
//! current live-config path, any GitHub backup state, and — when work is
//! in flight — an animated braille spinner plus the most-recent running
//! task's name.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

/// Frames of a braille spinner; cycled by `App::tick_idx`.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_glyph(tick: u8) -> &'static str {
    SPINNER[(tick as usize) % SPINNER.len()]
}

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let gh = if app.settings.github_backup.enabled {
        format!("GH: ●{}", app.settings.github_backup.branch)
    } else {
        "GH: off".to_string()
    };
    let running = app.tasks.running_count();
    let mut spans = vec![
        Span::styled(" ", Style::default()),
        Span::styled(app.live_config_hint(), Style::default().fg(theme.text_dim)),
        Span::raw("   "),
        Span::styled(gh, Style::default().fg(theme.muted)),
    ];
    if running > 0 {
        let glyph = spinner_glyph(app.tick_idx);
        let latest_name = app
            .tasks
            .running_entries()
            .next()
            .map(|e| truncate(&e.name, 40))
            .unwrap_or_default();
        let extra = if running > 1 {
            format!(" · +{} queued", running - 1)
        } else {
            String::new()
        };
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!("{glyph} {latest_name}{extra}  (L)"),
            Style::default().fg(theme.accent),
        ));
    }
    let line = Line::from(spans);
    let p = Paragraph::new(line).style(theme.default_style());
    frame.render_widget(p, area);
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
