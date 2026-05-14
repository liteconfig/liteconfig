//! Tails the stdout/stderr of a spawned installer (pnpx skills add, pnpm
//! bootstrap). Reads the shared buffers on each frame so lines appear as
//! they land — no blocking, no polling loop.
//!
//! Phases: a confirm-pnpm popup (yes/no) can precede the stream; once
//! installing, the log tails and Enter/Esc closes.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, InstallLogMode};
use crate::ui::status_bar::spinner_glyph;

pub fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.install_log_popup else {
        return;
    };
    let theme = app.theme;

    let popup_area = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup_area);

    let title = match &popup.mode {
        InstallLogMode::ConfirmPnpm { .. } => " Install pnpm? ".to_string(),
        InstallLogMode::Streaming(s) => format!(" Installing — {} ", s.title),
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

    if inner.height < 4 {
        return;
    }

    match &popup.mode {
        InstallLogMode::ConfirmPnpm { owner_repo } => {
            render_confirm(frame, app, inner, owner_repo);
        }
        InstallLogMode::Streaming(_) => {
            render_stream(frame, app, inner);
        }
    }
}

fn render_confirm(frame: &mut Frame<'_>, app: &App, area: Rect, owner_repo: &str) {
    let theme = app.theme;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let body = vec![
        Line::from(Span::styled(
            format!("To install {owner_repo} via the official skills CLI we need pnpm."),
            Style::default().fg(theme.text),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "Run:  curl -fsSL https://get.pnpm.io/install.sh | sh -",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            "This will modify your shell rc. Declining falls back to git-clone.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .style(theme.default_style()),
        layout[0],
    );
    let hint = Line::from(vec![
        Span::styled(" y ", theme.accent_style()),
        Span::styled("install pnpm   ", theme.muted_style()),
        Span::styled(" n ", theme.accent_style()),
        Span::styled("use git-clone   ", theme.muted_style()),
        Span::styled(" Esc ", theme.accent_style()),
        Span::styled("cancel", theme.muted_style()),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        layout[1],
    );
}

fn render_stream(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(popup) = &app.install_log_popup else {
        return;
    };
    let InstallLogMode::Streaming(stream) = &popup.mode else {
        return;
    };
    let theme = app.theme;

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    let status = stream.status();
    let status_line = match &status {
        liteconfig_core::services::skill_cli_service::RunStatus::Running => {
            Line::from(vec![Span::styled(
                format!(" {} running…", spinner_glyph(app.tick_idx)),
                theme.accent_style(),
            )])
        }
        liteconfig_core::services::skill_cli_service::RunStatus::Ok => Line::from(Span::styled(
            " ✓ installed — press Enter to close",
            Style::default().fg(theme.success),
        )),
        liteconfig_core::services::skill_cli_service::RunStatus::Err(e) => {
            Line::from(Span::styled(
                format!(" ✗ {e} — press Enter to close"),
                Style::default().fg(theme.danger),
            ))
        }
    };
    frame.render_widget(
        Paragraph::new(status_line).style(theme.default_style()),
        layout[0],
    );

    // Show at most the height of the log area.
    let max_lines = layout[1].height as usize;
    let tail = stream.snapshot_lines(max_lines.max(8));
    let items: Vec<ListItem<'_>> = tail
        .into_iter()
        .map(|l| {
            ListItem::new(Line::from(Span::styled(
                l,
                Style::default().fg(theme.muted),
            )))
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border_style(false))
            .title(Span::styled(" log ", Style::default().fg(theme.muted))),
    );
    frame.render_widget(list, layout[1]);

    let hint = Line::from(vec![
        Span::styled(" Enter/Esc ", theme.accent_style()),
        Span::styled(
            "close (runs continue in background if you close early)",
            theme.muted_style(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(theme.default_style()),
        layout[2],
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
