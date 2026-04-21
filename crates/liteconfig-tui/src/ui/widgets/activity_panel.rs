//! Activity overlay — toggled with `L` from any tab. Lists the TaskRunner
//! log newest-first, with a spinner glyph on running jobs so the user can
//! see long operations making progress without freezing the UI.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::tasks::TaskStatus;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if !app.show_activity {
        return;
    }
    let theme = app.theme;

    let popup_area = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup_area);

    let running = app.tasks.running_count();
    let title = if running > 0 {
        format!(" Activity ({running} running) ")
    } else {
        " Activity ".to_string()
    };
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

    let log = app.tasks.log();
    let items: Vec<ListItem<'_>> = if log.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  (no background jobs have run yet)",
            Style::default().fg(theme.muted),
        )))]
    } else {
        log.iter()
            .map(|entry| {
                let (glyph, glyph_style, status_text, status_style) = match &entry.status {
                    TaskStatus::Running => (
                        spinner_glyph(entry.started_at.elapsed().as_millis()),
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                        "running".to_string(),
                        Style::default().fg(theme.accent),
                    ),
                    TaskStatus::Ok(msg) => (
                        "✓",
                        Style::default()
                            .fg(theme.success)
                            .add_modifier(Modifier::BOLD),
                        if msg.is_empty() {
                            "ok".to_string()
                        } else {
                            format!("ok — {msg}")
                        },
                        Style::default().fg(theme.success),
                    ),
                    TaskStatus::Err(e) => (
                        "✗",
                        Style::default()
                            .fg(theme.danger)
                            .add_modifier(Modifier::BOLD),
                        format!("failed — {e}"),
                        Style::default().fg(theme.danger),
                    ),
                };
                let dur = match entry.finished_at {
                    Some(f) => format!(
                        "{:>5}ms",
                        f.saturating_duration_since(entry.started_at).as_millis()
                    ),
                    None => format!("{:>5}ms", entry.started_at.elapsed().as_millis()),
                };
                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(glyph.to_string(), glyph_style),
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<28}", truncate(&entry.name, 28)),
                        Style::default().fg(theme.text),
                    ),
                    Span::styled(format!("{}  ", dur), Style::default().fg(theme.muted)),
                    Span::styled(status_text, status_style),
                ]))
            })
            .collect()
    };

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

    frame.render_widget(List::new(items), list_area);

    let hint = Line::from(vec![
        Span::styled(" L/Esc ", theme.accent_style()),
        Span::styled("close", theme.muted_style()),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        hint_area,
    );
}

/// Four-frame ASCII-friendly spinner; animation driven off `started_at`
/// elapsed so every running entry ticks independently.
fn spinner_glyph(elapsed_ms: u128) -> &'static str {
    const FRAMES: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];
    FRAMES[((elapsed_ms / 120) as usize) % FRAMES.len()]
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

fn centered_rect(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let w = (area.width as u32 * pct_w as u32 / 100) as u16;
    let h = (area.height as u32 * pct_h as u32 / 100) as u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.max(50).min(area.width),
        height: h.max(10).min(area.height),
    }
}
