mod app;
mod nested;
mod theme;
mod top_files;
mod treemap;
mod ui;

use crate::scanner::{self, Progress};
use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Mouse tracking modes, written directly instead of via crossterm's
/// `EnableMouseCapture` (which also turns on mode 1003, "report every
/// motion event"). We only ever act on clicks and scroll, so reporting
/// motion just means the terminal streams a `Moved` event for every pixel
/// of pointer travel — including while the window is unfocused, for as
/// long as the pointer happens to rest over it. Left running for a long
/// idle period, that can queue up an enormous backlog of stray events
/// ahead of whatever the user actually types next (this was reported as
/// "leave it open for a while and it won't quit"). Mode 1000 (click
/// reporting) + 1002 (drag-while-button-held) + 1006 (SGR extended
/// coordinates, for terminals wider than 223 columns) cover everything
/// this app uses without that flood.
const MOUSE_CAPTURE_ON: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";
const MOUSE_CAPTURE_OFF: &str = "\x1b[?1006l\x1b[?1002l\x1b[?1000l";

pub fn run(root: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    write!(stdout, "{MOUSE_CAPTURE_ON}")?;
    stdout.flush()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode()?;
    let out = terminal.backend_mut();
    write!(out, "{MOUSE_CAPTURE_OFF}")?;
    execute!(out, LeaveAlternateScreen)?;
    out.flush()?;
    terminal.show_cursor()?;

    result
}

enum BrowseOutcome {
    Quit,
    Refresh,
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, root: PathBuf) -> Result<()> {
    let mut restore_to: Option<PathBuf> = None;

    loop {
        let tree = match scan_with_progress(terminal, &root)? {
            Some(t) => t,
            None => return Ok(()), // cancelled during scan
        };

        let mut app = App::new(tree);
        if let Some(target) = restore_to.take() {
            app.restore_path(&target);
        }

        match browse(terminal, &mut app)? {
            BrowseOutcome::Quit => return Ok(()),
            BrowseOutcome::Refresh => {
                restore_to = Some(app.current_path());
            }
        }
    }
}

fn scan_with_progress<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    root: &Path,
) -> Result<Option<crate::model::Tree>> {
    let progress = Arc::new(Progress::default());
    let progress_clone = progress.clone();
    let root_clone = root.to_path_buf();
    let handle = std::thread::spawn(move || scanner::scan(&root_clone, Some(&progress_clone)));

    let started = Instant::now();
    loop {
        terminal.draw(|f| ui::draw_scanning(f, &progress, started))?;
        if handle.is_finished() {
            break;
        }
        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press && k.code == crossterm::event::KeyCode::Char('q') {
                    return Ok(None);
                }
            }
        }
    }

    let tree = handle
        .join()
        .map_err(|_| anyhow::anyhow!("scanner thread panicked"))??;
    Ok(Some(tree))
}

/// Runs the interactive browser until the user quits or requests a rescan.
/// Redraws are event-driven (blocking on `event::read()`) rather than
/// polled on a timer — nothing here animates, so redrawing on a fixed
/// interval regardless of input would just waste CPU recomputing the
/// treemap layout and list for no visible change, which matters on a
/// directory with hundreds of thousands of entries.
fn browse<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<BrowseOutcome> {
    terminal.draw(|f| ui::draw(f, app))?;
    loop {
        // Track whether this event actually changed anything worth a
        // redraw, without ever skipping the quit/refresh check below —
        // even a long run of ignored events (e.g. a terminal that still
        // reports raw mouse motion despite MOUSE_CAPTURE_ON) must never
        // stop a 'q' sitting later in the queue from being noticed as
        // soon as it's read.
        let mut changed = true;
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                app.handle_key(k.code)?;
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    app.handle_click(m.column, m.row)?;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    app.handle_drag(m.column);
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    app.end_drag();
                }
                MouseEventKind::ScrollDown => app.dispatch(app::Action::Down)?,
                MouseEventKind::ScrollUp => app.dispatch(app::Action::Up)?,
                _ => changed = false,
            },
            Event::Resize(_, _) => {}
            _ => changed = false,
        }

        if app.should_quit {
            return Ok(BrowseOutcome::Quit);
        }
        if app.refresh_requested {
            return Ok(BrowseOutcome::Refresh);
        }
        if changed {
            terminal.draw(|f| ui::draw(f, app))?;
        }
    }
}
