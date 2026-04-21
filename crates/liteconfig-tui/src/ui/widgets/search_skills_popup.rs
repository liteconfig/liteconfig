//! Live skills.sh search. Top = boxed query input, bottom = result list
//! with a status line indicating loading / error / count. Enter's meaning
//! is split by focus so users can keep typing without accidentally
//! installing a row.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, SearchFocus, SearchStatus};
use crate::ui::status_bar::spinner_glyph;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.search_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            " skills.sh — live search ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 6 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // boxed query input
            Constraint::Length(1), // status line
            Constraint::Min(1),    // result list
            Constraint::Length(1), // key hints
        ])
        .split(inner);

    render_query(frame, app, layout[0]);
    render_status(frame, app, layout[1]);
    render_results(frame, app, layout[2]);
    render_hints(frame, app, layout[3]);

    let _ = popup; // silence if nothing above happens to touch it
}

fn render_query(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(p) = &app.search_popup else {
        return;
    };
    let theme = app.theme;
    let editing = matches!(p.focus, SearchFocus::Query);
    let border = if editing {
        theme.border_style(true)
    } else {
        theme.border_style(false)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(
            " Query ",
            Style::default().fg(if editing { theme.primary } else { theme.muted }),
        ));
    let body = if p.query.is_empty() {
        Span::styled("type a search…", Style::default().fg(theme.muted))
    } else {
        Span::styled(p.query.clone(), Style::default().fg(theme.text))
    };
    let mut line = vec![body];
    if editing {
        line.push(Span::styled("▏", theme.accent_style()));
    }
    let p = Paragraph::new(Line::from(line))
        .block(block)
        .style(theme.default_style());
    frame.render_widget(p, area);
}

fn render_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(p) = &app.search_popup else {
        return;
    };
    let theme = app.theme;
    let text = match &p.status {
        SearchStatus::Idle => Line::from(Span::styled(
            " Press Enter to search ",
            Style::default().fg(theme.muted),
        )),
        SearchStatus::Loading => Line::from(vec![Span::styled(
            format!(" {} searching skills.sh…", spinner_glyph(app.tick_idx)),
            theme.accent_style(),
        )]),
        SearchStatus::Error(e) => Line::from(Span::styled(
            format!(" ✗ {e}"),
            Style::default().fg(theme.danger),
        )),
        SearchStatus::Loaded => Line::from(Span::styled(
            format!(
                " {} result(s) — Tab to focus list, Enter to install",
                p.results.len()
            ),
            Style::default().fg(theme.success),
        )),
    };
    frame.render_widget(Paragraph::new(text).style(theme.default_style()), area);
}

fn render_results(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(p) = &app.search_popup else {
        return;
    };
    let theme = app.theme;
    if p.results.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            "(no results yet)",
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Center)
        .style(theme.default_style());
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem<'_>> = p
        .results
        .iter()
        .map(|hit| {
            let installs = hit
                .installs
                .map(|n| format!(" · {n} installs"))
                .unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::styled(
                    hit.name.clone(),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(hit.source.clone(), Style::default().fg(theme.muted)),
                Span::styled(installs, Style::default().fg(theme.muted)),
            ]))
        })
        .collect();

    let focused_list = matches!(p.focus, SearchFocus::Results);
    let list = List::new(items)
        .highlight_style(if focused_list {
            theme.selection_style()
        } else {
            theme.muted_style()
        })
        .highlight_symbol(if focused_list { "▶ " } else { "  " });
    let mut state = ListState::default();
    state.select(Some(p.cursor.min(p.results.len().saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_hints(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let hint = Line::from(vec![
        Span::styled(" Enter ", theme.accent_style()),
        Span::styled("search/install   ", theme.muted_style()),
        Span::styled(" Tab ", theme.accent_style()),
        Span::styled("switch focus   ", theme.muted_style()),
        Span::styled(" ↑/↓ ", theme.accent_style()),
        Span::styled("move   ", theme.muted_style()),
        Span::styled(" Esc ", theme.accent_style()),
        Span::styled("close", theme.muted_style()),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        area,
    );
}

fn centered_rect(pct_w: u16, pct_h: u16, area: Rect) -> Rect {
    let w = (area.width as u32 * pct_w as u32 / 100) as u16;
    let h = (area.height as u32 * pct_h as u32 / 100) as u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.max(60).min(area.width),
        height: h.max(10).min(area.height),
    }
}
