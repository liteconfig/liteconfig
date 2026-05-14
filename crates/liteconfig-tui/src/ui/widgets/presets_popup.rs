//! Curated presets chooser. Same layout scaffold as [`agent_popup`] but the
//! list body is driven by the static catalog in
//! [`liteconfig_core::presets`] — either skill-repo URLs or MCP servers.

use liteconfig_core::presets::{MCP_PRESETS, SKILL_REPO_PRESETS};

use crate::app::PLUGIN_PRESETS;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, PresetsKind};

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.presets_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, popup_area);

    let title = match popup.kind {
        PresetsKind::SkillRepo => " New skill repo — curated presets ".to_string(),
        PresetsKind::Mcp => " New MCP server — curated presets ".to_string(),
        PresetsKind::Plugin => " Install plugin — curated presets ".to_string(),
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

    let items: Vec<ListItem<'_>> = match popup.kind {
        PresetsKind::SkillRepo => SKILL_REPO_PRESETS
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        p.add_arg(),
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(p.description, Style::default().fg(theme.muted)),
                ]))
            })
            .collect(),
        PresetsKind::Mcp => MCP_PRESETS
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{:<10}]", p.category),
                        Style::default().fg(theme.accent),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        p.name,
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(p.description, Style::default().fg(theme.muted)),
                ]))
            })
            .collect(),
        PresetsKind::Plugin => PLUGIN_PRESETS
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{}/{}", p.owner, p.name),
                        Style::default()
                            .fg(theme.primary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(p.description, Style::default().fg(theme.muted)),
                ]))
            })
            .collect(),
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

    let total = popup.len();
    let list = List::new(items)
        .highlight_style(theme.selection_style())
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    state.select(Some(popup.cursor.min(total.saturating_sub(1))));
    frame.render_stateful_widget(list, list_area, &mut state);

    let mut hint_spans = vec![
        Span::styled(" ↑/↓ ", theme.accent_style()),
        Span::styled("move   ", theme.muted_style()),
        Span::styled(" Enter ", theme.accent_style()),
        Span::styled("install   ", theme.muted_style()),
    ];
    if matches!(popup.kind, PresetsKind::Mcp) {
        hint_spans.push(Span::styled(" ^F ", theme.accent_style()));
        hint_spans.push(Span::styled("live search   ", theme.muted_style()));
    }
    hint_spans.push(Span::styled(" Esc ", theme.accent_style()));
    hint_spans.push(Span::styled("cancel", theme.muted_style()));
    let hint = Line::from(hint_spans);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        hint_area,
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
        height: h.max(8).min(area.height),
    }
}
