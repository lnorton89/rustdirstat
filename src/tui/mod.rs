mod app;
mod nested;
mod theme;
mod top_files;
mod treemap;
mod ui;

use crate::scanner::{self, Progress};
use anyhow::Result;
use app::App;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn run(root: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
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
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                app.handle_key(k.code)?;
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    app.handle_click(m.column, m.row)?;
                }
                MouseEventKind::ScrollDown => app.dispatch(app::Action::Down)?,
                MouseEventKind::ScrollUp => app.dispatch(app::Action::Up)?,
                _ => continue,
            },
            Event::Resize(_, _) => {}
            _ => continue,
        }

        if app.should_quit {
            return Ok(BrowseOutcome::Quit);
        }
        if app.refresh_requested {
            return Ok(BrowseOutcome::Refresh);
        }
        terminal.draw(|f| ui::draw(f, app))?;
    }
}
