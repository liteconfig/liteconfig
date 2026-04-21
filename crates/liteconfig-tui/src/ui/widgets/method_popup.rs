//! Per-skill sync-method picker. Opened from the Skills tab via `M` to choose
//! directly between `auto`, `symlink`, `copy`, and `inherit` instead of
//! cycling through them one press at a time.

use liteconfig_core::model::skill::SyncMethod;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, METHOD_POPUP_CHOICES};

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.method_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(60, 50, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" Sync method for \"{}\" ", popup.row_name);
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

    let items: Vec<ListItem<'_>> = METHOD_POPUP_CHOICES
        .iter()
        .map(|m| {
            let selected = *m == popup.current;
            let glyph = if selected { "● " } else { "○ " };
            let glyph_style = if selected {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            ListItem::new(Line::from(vec![
                Span::styled(glyph, glyph_style),
                Span::styled(m.as_str(), Style::default().fg(theme.text)),
                Span::raw("   "),
                Span::styled(method_blurb(*m), Style::default().fg(theme.muted)),
            ]))
        })
        .collect();

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
        popup
            .cursor
            .min(METHOD_POPUP_CHOICES.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, list_area, &mut state);

    let hint = Line::from(vec![
        Span::styled(" ↑↓ ", theme.accent_style()),
        Span::styled("move   ", theme.muted_style()),
        Span::styled(" Enter ", theme.accent_style()),
        Span::styled("pick + resync   ", theme.muted_style()),
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

fn method_blurb(m: SyncMethod) -> &'static str {
    match m {
        SyncMethod::Auto => "OS-picked (symlink on Unix, copy on Windows)",
        SyncMethod::Symlink => "fast, edits reflect everywhere",
        SyncMethod::Copy => "independent copies per agent",
        SyncMethod::Inherit => "use workspace default",
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
        width: w.max(48).min(area.width),
        height: h.max(8).min(area.height),
    }
}
