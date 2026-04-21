//! liteconfig — fast TUI for syncing AI coding agent configs.
//!
//! Startup flow:
//!   1. Parse CLI args (`clap`).
//!   2. Install a panic hook that restores the terminal before the panic
//!      message prints — otherwise the user is left with a half-configured
//!      tty on a crash.
//!   3. Load the core library's DB, settings, and secret store.
//!   4. Enter the alternate screen, enable mouse capture, run the event loop.
//!   5. On exit (or panic), restore the terminal to its pre-launch state.

use std::io;
use std::time::{Duration, Instant};

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use liteconfig_core::db::Database;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::settings::Settings;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use liteconfig_tui::app::App;
use liteconfig_tui::events;
use liteconfig_tui::ui;

#[derive(Debug, Parser)]
#[command(
    name = "liteconfig",
    version,
    about = "Fast TUI for syncing AI coding agent configs."
)]
struct Cli {
    /// Use an alternate liteconfig home (useful for testing / sandboxes).
    /// Defaults to `$LITECONFIG_HOME` or `~/.liteconfig`.
    #[arg(long, value_name = "DIR")]
    home: Option<String>,

    /// Minimum log level written to ~/.liteconfig/liteconfig.log.
    #[arg(long, value_name = "LEVEL", default_value = "warn")]
    log_level: String,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    if let Some(home) = &cli.home {
        std::env::set_var("LITECONFIG_HOME", home);
    }

    init_logging(&cli.log_level)?;
    install_panic_hook();

    let db = Database::open_default().wrap_err("failed to open liteconfig database")?;
    let settings = Settings::load_or_default().wrap_err("failed to load settings")?;
    let secrets = SecretStore::load_or_default().wrap_err("failed to load secret store")?;

    let mut app = App::new(db, settings, secrets)?;

    run(&mut app)?;
    Ok(())
}

fn run(app: &mut App) -> Result<()> {
    let mut terminal = enter_terminal()?;
    let result = main_loop(&mut terminal, app);
    leave_terminal(&mut terminal)?;
    result
}

fn main_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut last_frame_output: Option<ui::FrameOutput> = None;

    while !app.should_quit {
        terminal.draw(|frame| {
            last_frame_output = Some(ui::render(frame, app));
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    events::handle_key(app, key);
                }
                Event::Mouse(mouse) => {
                    let (tab_hits, btn_hits) = last_frame_output
                        .as_ref()
                        .map(|o| (o.tab_hits.as_slice(), o.button_hits.as_slice()))
                        .unwrap_or((&[], &[]));
                    events::handle_mouse(app, mouse, tab_hits, btn_hits);
                }
                Event::Resize(_, _) => { /* next draw handles it */ }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick();
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn enter_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn leave_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restore the terminal before any panic message hits stderr. Without this, a
/// panic mid-render leaves the user with a trashed tty.
fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        hook(info);
    }));
}

fn init_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    let path = liteconfig_core::paths::liteconfig_log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();

    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("warn"));

    if let Some(file) = file {
        fmt()
            .with_env_filter(filter)
            .with_writer(file)
            .with_ansi(false)
            .try_init()
            .ok();
    }
    Ok(())
}
