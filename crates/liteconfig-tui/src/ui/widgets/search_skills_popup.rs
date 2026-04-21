//! Live skills.sh search popup.
//!
//! Layout: boxed query input → status line → [result list | detail panel]
//! split 60/40, → key hint bar. Enter's meaning is driven by the current
//! focus zone: Query → run search, Results → install the highlighted row.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, SearchFocus, SearchStatus};
use crate::ui::status_bar::spinner_glyph;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.search_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(85, 75, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            " skills.sh — live skill search ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 8 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // boxed query input
            Constraint::Length(1), // status line
            Constraint::Min(1),    // result list + detail panel
            Constraint::Length(1), // key hints
        ])
        .split(inner);

    render_query(frame, app, layout[0]);
    render_status(frame, app, layout[1]);

    // Split the body area into list (left 60%) and detail (right 40%).
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(layout[2]);

    render_results(frame, app, body[0]);
    render_detail(frame, app, body[1]);
    render_hints(frame, app, layout[3]);

    let _ = popup;
}

fn render_query(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(p) = &app.search_popup else {
        return;
    };
    let theme = app.theme;
    let editing = matches!(p.focus, SearchFocus::Query);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if editing {
            theme.border_style(true)
        } else {
            theme.border_style(false)
        })
        .title(Span::styled(
            " Query ",
            Style::default().fg(if editing { theme.primary } else { theme.muted }),
        ));
    let body = if p.query.is_empty() {
        Span::styled("type a search term…", Style::default().fg(theme.muted))
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
            " Type a query and press Enter to search",
            Style::default().fg(theme.muted),
        )),
        SearchStatus::Loading => Line::from(Span::styled(
            format!(" {} searching skills.sh…", spinner_glyph(app.tick_idx)),
            theme.accent_style(),
        )),
        SearchStatus::Error(e) => Line::from(Span::styled(
            format!(" ✗ {e}"),
            Style::default().fg(theme.danger),
        )),
        SearchStatus::Loaded => Line::from(Span::styled(
            format!(
                " {} result(s) found — Tab to move to list, Enter to install",
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if matches!(p.focus, SearchFocus::Results) {
            theme.border_style(true)
        } else {
            theme.border_style(false)
        })
        .title(Span::styled(" Results ", Style::default().fg(theme.muted)));

    if p.results.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            "(no results yet)",
            Style::default().fg(theme.muted),
        )))
        .block(block)
        .alignment(Alignment::Center)
        .style(theme.default_style());
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem<'_>> = p
        .results
        .iter()
        .map(|hit| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    hit.name.clone(),
                    Style::default()
                        .fg(theme.primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{} · {} installs", hit.source, hit.installs),
                    Style::default().fg(theme.muted),
                ),
            ]))
        })
        .collect();

    let focused = matches!(p.focus, SearchFocus::Results);
    let list = List::new(items)
        .block(block)
        .highlight_style(if focused {
            theme.selection_style()
        } else {
            Style::default().fg(theme.muted)
        })
        .highlight_symbol(if focused { "▶ " } else { "  " });
    let mut state = ListState::default();
    state.select(Some(p.cursor.min(p.results.len().saturating_sub(1))));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_detail(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(p) = &app.search_popup else {
        return;
    };
    let theme = app.theme;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(false))
        .title(Span::styled(" Detail ", Style::default().fg(theme.muted)));

    let hit = match p.results.get(p.cursor) {
        Some(h) => h,
        None => {
            let hint = Paragraph::new(Span::styled(
                "Select a result to see details",
                Style::default().fg(theme.muted),
            ))
            .block(block)
            .alignment(Alignment::Center)
            .style(theme.default_style());
            frame.render_widget(hint, area);
            return;
        }
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let lines = vec![
        Line::from(Span::styled(
            hit.name.clone(),
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::raw("")),
        Line::from(vec![
            Span::styled("Repo:   ", Style::default().fg(theme.muted)),
            Span::styled(hit.source.clone(), Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Skill:  ", Style::default().fg(theme.muted)),
            Span::styled(hit.skill_id.clone(), Style::default().fg(theme.accent)),
        ]),
        Line::from(vec![
            Span::styled("Install:", Style::default().fg(theme.muted)),
            Span::styled(
                format!(" {} ×", hit.installs),
                Style::default().fg(theme.text),
            ),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled("GitHub:", Style::default().fg(theme.muted))),
        Line::from(Span::styled(
            hit.readme_url.clone(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::UNDERLINED),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Full description available",
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            "after install (reads SKILL.md).",
            Style::default().fg(theme.muted),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.default_style())
            .wrap(Wrap { trim: true }),
        inner,
    );
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
        Span::styled(" Bksp ", theme.accent_style()),
        Span::styled("edit query   ", theme.muted_style()),
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
        width: w.max(70).min(area.width),
        height: h.max(12).min(area.height),
    }
}
