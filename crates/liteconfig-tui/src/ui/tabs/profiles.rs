//! Profiles tab: one sub-table per agent, listing its profiles with an
//! indicator marking the one currently live on this machine.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border_style(true))
        .title(Span::styled(
            " Profiles ",
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    if inner.height < 3 {
        return;
    }

    // Only agents with a profile concept; skips Cursor.
    let agents = App::profile_agents();
    // One block per agent, stacked vertically. Equal share of height for now.
    let n = agents.len() as u16;
    if n == 0 {
        return;
    }
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let slots = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, agent) in agents.iter().enumerate() {
        let focused = app.focused_agent_idx == i;
        let slot = slots[i];
        let Some(view) = app.profile_views.get(agent) else {
            continue;
        };

        let current_id = app.settings.current_profile_for(*agent);

        let items: Vec<ListItem<'_>> = view
            .profiles
            .iter()
            .map(|p| {
                let is_current = current_id == Some(p.id.as_str());
                let bullet = if is_current { "● " } else { "  " };
                let bullet_style = if is_current {
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.muted)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(bullet, bullet_style),
                    Span::styled(p.name.clone(), Style::default().fg(theme.text)),
                    Span::raw("   "),
                    Span::styled(
                        p.meta.notes.clone().unwrap_or_default(),
                        Style::default().fg(theme.muted),
                    ),
                ]))
            })
            .collect();

        let empty_hint = if view.profiles.is_empty() {
            Some(
                Paragraph::new(Span::styled(
                    "(no profiles yet — press n to add one)",
                    Style::default().fg(theme.muted),
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(theme.border_style(focused))
                        .title(Line::from(vec![
                            Span::styled(
                                format!(" {} ", agent.display_name()),
                                Style::default()
                                    .fg(if focused { theme.primary } else { theme.text })
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" ({}) ", agent.id()),
                                Style::default().fg(theme.muted),
                            ),
                        ])),
                ),
            )
        } else {
            None
        };

        if let Some(p) = empty_hint {
            frame.render_widget(p, slot);
            continue;
        }

        let title = Line::from(vec![
            Span::styled(
                format!(" {} ", agent.display_name()),
                Style::default()
                    .fg(if focused { theme.primary } else { theme.text })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " · {} profiles · live: {} ",
                    view.profiles.len(),
                    app.current_profile_name(*agent).unwrap_or("(none)")
                ),
                Style::default().fg(theme.muted),
            ),
        ]);

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border_style(focused))
                    .title(title),
            )
            .highlight_style(theme.selection_style())
            .highlight_symbol("▶ ");

        let mut state = ListState::default();
        if !view.profiles.is_empty() {
            state.select(Some(view.selected));
        }

        frame.render_stateful_widget(list, slot, &mut state);
    }
}
