//! Top-level UI renderer: tab bar + body + status bar.
//!
//! Widgets record their rendered rects into `HitRegistry` during the render
//! pass so the event loop can do mouse hit-testing afterward.

pub mod status_bar;
pub mod tabs;
pub mod widgets;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::events::TabHit;
use crate::theme::{key_label, KeyAction};

pub struct FrameOutput {
    pub tab_hits: Vec<TabHit>,
}

pub fn render(frame: &mut Frame<'_>, app: &App) -> FrameOutput {
    let theme = app.theme;
    let area = frame.area();

    // Fill background.
    frame.render_widget(Block::default().style(theme.default_style()), area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(1),    // body
            Constraint::Length(2), // hint bar
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let tab_hits = render_tab_bar(frame, app, layout[0]);
    render_body(frame, app, layout[1]);
    render_hint_bar(frame, app, layout[2]);
    status_bar::render(frame, app, layout[3]);
    widgets::toast::render(frame, app, area);

    FrameOutput { tab_hits }
}

fn render_tab_bar(frame: &mut Frame<'_>, app: &App, area: Rect) -> Vec<TabHit> {
    let theme = app.theme;
    let active = app.active_tab.index();

    let titles: Vec<Line<'_>> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Line::from(vec![
                Span::styled(format!(" {} ", i + 1), Style::default().fg(theme.muted)),
                Span::styled(
                    t.title(),
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(active)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_style(false))
                .title(Line::from(vec![
                    Span::styled(" liteconfig ", theme.accent_style()),
                    Span::styled(
                        format!(
                            "· {} · profile: {}",
                            app.focused_agent().display_name(),
                            app.current_profile_name(app.focused_agent())
                                .unwrap_or("(none)")
                        ),
                        theme.muted_style(),
                    ),
                ])),
        )
        .style(theme.default_style())
        .highlight_style(
            Style::default()
                .fg(theme.primary)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(Span::styled("│", theme.muted_style()));

    frame.render_widget(tabs, area);

    // Build click hit regions for each tab label. Ratatui's `Tabs` widget
    // renders each title then, between tabs, ` <divider> ` — i.e. a space, the
    // divider glyph, and another space. Stride is therefore
    // `title_width + 1 (trailing gap) + divider_width + 1 (leading gap)` →
    // `title_width + 3` for a 1-cell divider. The previous impl used
    // `+ 1`, which left every tab offset 2 cells to the left of its true rect.
    // The first row of `area` is the top border; tabs render on the second row.
    let row = area.y + 1;
    let mut col = area.x + 1; // skip left border
    let right_edge = area.x + area.width.saturating_sub(1); // stay inside right border
    let last = Tab::ALL.len().saturating_sub(1);
    let mut hits = Vec::with_capacity(Tab::ALL.len());
    for (i, t) in Tab::ALL.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, t.title());
        let w = label.chars().count() as u16;
        if col >= right_edge {
            break;
        }
        let clamped_w = w.min(right_edge.saturating_sub(col));
        hits.push(TabHit {
            tab: *t,
            x: col,
            y: row,
            w: clamped_w,
            h: 1,
        });
        // Advance past title + trailing gap + divider + leading gap, except
        // after the last tab where no divider follows.
        col = col.saturating_add(w);
        if i != last {
            col = col.saturating_add(3);
        }
    }
    hits
}

fn render_body(frame: &mut Frame<'_>, app: &App, area: Rect) {
    match app.active_tab {
        Tab::Profiles => tabs::profiles::render(frame, app, area),
        Tab::Skills => tabs::skills::render(frame, app, area),
        Tab::Mcp => tabs::mcp::render(frame, app, area),
        Tab::Rules => tabs::rules::render(frame, app, area),
        Tab::Backup => tabs::backup::render(frame, app, area),
        Tab::Sessions => tabs::placeholder::render(frame, app, area, "Sessions", "v1.1"),
        Tab::Settings => tabs::settings::render(frame, app, area),
    }
}

fn render_hint_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = app.theme;
    let hints = match app.active_tab {
        _ if app.agent_popup.is_some() => vec![
            hint("↑↓", "move"),
            hint("Space", "toggle"),
            hint("A/N", "all/none"),
            hint("Enter", "ok"),
            hint("Esc", "cancel"),
        ],
        Tab::Profiles => vec![
            hint("↑↓", "move"),
            hint("←→", "agent"),
            hint("Enter/s", "switch"),
            hint("/", "filter"),
            hint(">", "palette"),
            hint("?", "help"),
            hint(&key_label(KeyAction::Quit), "quit"),
        ],
        Tab::Skills => vec![
            hint("↑↓", "move"),
            hint("Space", "select"),
            hint("a", "agents"),
            hint("m", "cycle method"),
            hint("M", "pick method"),
            hint("s", "sync sel"),
            hint("S", "sync all"),
            hint("Enter", "sync row"),
        ],
        Tab::Mcp => vec![
            hint("↑↓", "move"),
            hint("Space", "select"),
            hint("a", "agents"),
            hint("S", "sync all"),
            hint("i", "import live"),
            hint("d", "delete"),
        ],
        Tab::Rules => vec![
            hint("↑↓", "move"),
            hint("Space", "select"),
            hint("a", "agents"),
            hint("S", "sync all"),
            hint("d", "delete"),
        ],
        Tab::Backup => vec![
            hint("↑↓", "move"),
            hint("n", "snapshot"),
            hint("r", "restore"),
            hint("p", "push GH"),
        ],
        Tab::Settings => vec![
            hint("t", "cycle theme"),
            hint("Tab", "next tab"),
            hint(&key_label(KeyAction::Quit), "quit"),
        ],
        _ => vec![
            hint("Tab", "next tab"),
            hint("/", "filter"),
            hint(">", "palette"),
            hint("?", "help"),
            hint(&key_label(KeyAction::Quit), "quit"),
        ],
    };

    let spans: Vec<Span<'_>> = hints
        .into_iter()
        .flat_map(|(k, label)| {
            vec![
                Span::styled(format!(" {k} "), theme.accent_style()),
                Span::styled(label, theme.muted_style()),
                Span::raw("  "),
            ]
        })
        .collect();

    let p = Paragraph::new(Line::from(spans)).style(theme.default_style());
    frame.render_widget(p, area);
}

fn hint(key: &str, label: &str) -> (String, String) {
    (key.to_string(), label.to_string())
}
