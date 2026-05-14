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
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::events::{ButtonHit, TabHit};
use crate::theme::{key_label, KeyAction};

pub struct FrameOutput {
    pub tab_hits: Vec<TabHit>,
    pub button_hits: Vec<ButtonHit>,
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
    let mut button_hits: Vec<ButtonHit> = Vec::new();
    render_body(frame, app, layout[1], &mut button_hits);
    render_hint_bar(frame, app, layout[2]);
    status_bar::render(frame, app, layout[3]);
    widgets::activity_panel::render(frame, app, area);
    widgets::help_overlay::render(frame, app, area);
    widgets::install_log_popup::render(frame, app, area);
    widgets::toast::render(frame, app, area);

    FrameOutput {
        tab_hits,
        button_hits,
    }
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

    // Branded title: gradient-animated `liteconfig` wordmark + muted suffix.
    let mut title_spans: Vec<Span<'_>> = vec![Span::raw(" ")];
    title_spans.extend(theme.gradient_title("liteconfig", app.tick_idx));
    title_spans.push(Span::raw(" "));
    title_spans.push(Span::styled(
        format!(
            "· {} · profile: {}",
            app.focused_agent().display_name(),
            app.current_profile_name(app.focused_agent())
                .unwrap_or("(none)")
        ),
        theme.muted_style(),
    ));

    let tabs = Tabs::new(titles)
        .select(active)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.border_style(false))
                .title(Line::from(title_spans)),
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

fn render_body(frame: &mut Frame<'_>, app: &App, area: Rect, hits: &mut Vec<ButtonHit>) {
    match app.active_tab {
        Tab::Profiles => tabs::profiles::render(frame, app, area),
        Tab::Skills => tabs::skills::render(frame, app, area, hits),
        Tab::Mcp => tabs::mcp::render(frame, app, area, hits),
        Tab::Rules => tabs::rules::render(frame, app, area, hits),
        Tab::Plugins => tabs::plugins::render(frame, app, area, hits),
        Tab::Backup => tabs::backup::render(frame, app, area, hits),
        Tab::Sessions => tabs::coming_soon::render(frame, app, area, "Sessions", "v1.1"),
        Tab::Settings => tabs::settings::render(frame, app, area),
    }
}

fn render_hint_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    // Split the 2-row region: row 0 = key hints, row 1 = task-progress
    // banner (blank when nothing is running).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    render_hints(frame, app, rows[0]);
    render_task_banner(frame, app, rows[1]);
}

fn render_task_banner(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if app.tasks.running_count() == 0 {
        return;
    }
    let theme = app.theme;
    let glyph = status_bar::spinner_glyph(app.tick_idx);
    let count = app.tasks.running_count();
    let name = app
        .tasks
        .running_entries()
        .next()
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let text = if count > 1 {
        format!(" {glyph} {name} · {} queued", count - 1)
    } else {
        format!(" {glyph} {name}")
    };
    let p = Paragraph::new(Span::styled(text, theme.accent_style())).style(theme.default_style());
    frame.render_widget(p, area);
}

fn render_hints(frame: &mut Frame<'_>, app: &App, area: Rect) {
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
        Tab::Plugins => vec![
            hint("↑↓", "move"),
            hint("n", "install"),
            hint("d", "delete (×2)"),
        ],
        Tab::Backup => vec![
            hint("↑↓", "move"),
            hint("n", "snapshot"),
            hint("r", "restore"),
            hint("p", "push GH"),
            hint("d", "delete (×2)"),
        ],
        Tab::Settings => vec![
            hint("↑↓", "focus row"),
            hint("Space", "toggle"),
            hint("Enter", "edit"),
            hint("t", "theme"),
            hint("?", "help"),
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
