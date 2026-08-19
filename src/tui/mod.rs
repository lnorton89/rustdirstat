mod app;
mod nested;
mod search;
mod theme;
mod top_files;
mod treemap;
mod ui;

use crate::scanner::{self, Progress};
use anyhow::Result;
use app::App;
pub use app::SortMode;
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// True for Ctrl+C specifically. Raw mode disables the terminal's own
/// SIGINT-on-Ctrl+C handling (that's what "raw" means — no line discipline
/// processing input for us), so once raw mode is on, *we* are the only
/// thing that can respond to Ctrl+C; if no key binding checks for it, it
/// does nothing at all. Checked ahead of every other key handling, in both
/// the scan and browse loops, so it always works as an immediate quit
/// regardless of what modal state (delete confirm, search input, help) is
/// open — the same "get me out of here" role it has everywhere else.
fn is_ctrl_c(k: &crossterm::event::KeyEvent) -> bool {
    k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c')
}

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

/// Restores the terminal (raw mode, mouse capture, alternate screen) to a
/// normal state. Used both on the ordinary exit path and, via the panic
/// hook below, on a crash — without this, a panic mid-session leaves the
/// terminal stuck in raw mode with the alternate screen still active,
/// where neither 'q' nor Ctrl+C look like they do anything afterward (raw
/// mode is what makes Ctrl+C our responsibility instead of the shell's in
/// the first place, and a dead process isn't around to handle it).
fn restore_terminal() {
    let _ = disable_raw_mode();
    let mut stdout = std::io::stdout();
    let _ = write!(stdout, "{MOUSE_CAPTURE_OFF}\x1b[?25h");
    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = stdout.flush();
}

pub fn run(root: PathBuf) -> Result<()> {
    let default_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_panic_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    write!(stdout, "{MOUSE_CAPTURE_ON}")?;
    stdout.flush()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    restore_terminal();

    result
}

enum BrowseOutcome {
    Quit,
    Refresh,
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, root: PathBuf) -> Result<()> {
    let mut restore_to: Option<PathBuf> = None;
    let config = crate::config::load();

    loop {
        let tree = match scan_with_progress(terminal, &root)? {
            Some(t) => t,
            None => return Ok(()), // cancelled during scan
        };

        let mut app = App::new(tree);
        app.apply_config(&config);
        if let Some(target) = restore_to.take() {
            app.restore_path(&target);
        }

        match browse(terminal, &mut app)? {
            BrowseOutcome::Quit => {
                crate::config::save(&app.to_config());
                return Ok(());
            }
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
                let cancel = k.kind == KeyEventKind::Press
                    && (k.code == KeyCode::Char('q') || k.code == KeyCode::Esc || is_ctrl_c(&k));
                if cancel {
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
        // Drain every event already queued before redrawing, instead of
        // redrawing (a full list/treemap relayout — not cheap on a huge
        // tree) once per event. Without this, a large backlog — buffered
        // input a terminal replays after being resized or scrolled, or
        // simply more clicks/keys than one redraw cycle kept up with —
        // gets worked through one expensive redraw at a time, which on a
        // multi-million-file tree can take long enough to look
        // indistinguishable from "won't quit", even though a 'q' or
        // Ctrl+C sitting in that backlog was always going to be honored
        // the moment it was reached. The quit/refresh checks still run
        // after every single drained event, not just at the end of the
        // drain, so nothing after this loop can ever delay noticing them.
        let mut changed = false;
        loop {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press && is_ctrl_c(&k) => {
                    return Ok(BrowseOutcome::Quit);
                }
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    app.handle_key(k.code)?;
                    changed = true;
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        app.handle_click(m.column, m.row)?;
                        changed = true;
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        app.handle_drag(m.column);
                        changed = true;
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        app.end_drag();
                        changed = true;
                    }
                    MouseEventKind::ScrollDown => {
                        app.dispatch(app::Action::Down)?;
                        changed = true;
                    }
                    MouseEventKind::ScrollUp => {
                        app.dispatch(app::Action::Up)?;
                        changed = true;
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {
                    changed = true;
                    // Stop draining and redraw immediately, rather than
                    // continuing to process whatever else is already
                    // queued against the pre-resize layout. `click_zones`
                    // is only rebuilt inside `ui::draw`, which otherwise
                    // wouldn't run again until the whole backlog empties
                    // — so a resize followed almost immediately by a
                    // click (an ordinary sequence: widen the terminal,
                    // then click before the next paint) would evaluate
                    // that click against click zones sized for the old
                    // layout, potentially hitting the wrong button on a
                    // popup whose buttons just moved.
                    break;
                }
                _ => {}
            }

            if app.should_quit {
                return Ok(BrowseOutcome::Quit);
            }
            if app.refresh_requested {
                return Ok(BrowseOutcome::Refresh);
            }
            if app.duplicate_scan_requested {
                // Needs its own blocking progress-screen loop below, which
                // can't run mid-drain — stop draining and handle it once
                // the current backlog processing yields control back.
                break;
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }

        if app.duplicate_scan_requested {
            app.duplicate_scan_requested = false;
            if let Some(groups) = run_duplicate_scan(terminal, &app.tree)? {
                app.set_duplicate_results(groups);
            }
            changed = true;
        }
        if changed {
            terminal.draw(|f| ui::draw(f, app))?;
        }
    }
}

/// Runs the duplicate-file scan with its own progress screen, the same
/// shape as `scan_with_progress` above. Unlike that initial scan (which
/// only reads directory metadata), hashing file *content* to find
/// duplicates can genuinely take a while on a large tree, so it needs the
/// same cancellable, blocking progress UI rather than running inline in
/// `App::dispatch`.
///
/// Uses `thread::scope` (rather than `scan_with_progress`'s bare
/// `thread::spawn`) so the worker can borrow `tree` directly instead of
/// requiring an owned/`'static` copy — duplicate scanning runs against the
/// tree already sitting in `App`, and cloning a potentially huge tree just
/// to satisfy `'static` would be wasteful. That means a cancel can't just
/// abandon the thread and return immediately the way scan cancellation
/// does: `thread::scope` won't return until the worker finishes, so cancel
/// instead sets `DupProgress::cancelled`, which the hashing loop checks
/// between files to wind down quickly.
fn run_duplicate_scan<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    tree: &crate::model::Tree,
) -> Result<Option<Vec<crate::duplicates::DupGroup>>> {
    let progress = crate::duplicates::DupProgress::default();
    let started = Instant::now();

    std::thread::scope(
        |scope| -> Result<Option<Vec<crate::duplicates::DupGroup>>> {
            let handle = scope.spawn(|| crate::duplicates::find_duplicates(tree, Some(&progress)));

            let mut cancelled = false;
            loop {
                terminal.draw(|f| ui::draw_duplicate_progress(f, &progress, started))?;
                if handle.is_finished() {
                    break;
                }
                if event::poll(Duration::from_millis(150))? {
                    if let Event::Key(k) = event::read()? {
                        let cancel = k.kind == KeyEventKind::Press
                            && (k.code == KeyCode::Char('q')
                                || k.code == KeyCode::Esc
                                || is_ctrl_c(&k));
                        if cancel && !cancelled {
                            cancelled = true;
                            progress.cancelled.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }

            let groups = handle
                .join()
                .map_err(|_| anyhow::anyhow!("duplicate scan thread panicked"))?;
            Ok(if cancelled { None } else { Some(groups) })
        },
    )
}
