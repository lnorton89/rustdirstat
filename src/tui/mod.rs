mod app;
mod nested;
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
use std::path::PathBuf;
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

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, root: PathBuf) -> Result<()> {
    let progress = Arc::new(Progress::default());
    let progress_clone = progress.clone();
    let root_clone = root.clone();
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
                    return Ok(());
                }
            }
        }
    }

    let tree = handle
        .join()
        .map_err(|_| anyhow::anyhow!("scanner thread panicked"))??;
    let mut app = App::new(tree);

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    app.handle_key(k.code)?;
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        app.handle_click(m.column, m.row)?;
                    }
                    MouseEventKind::ScrollDown => {
                        app.dispatch(app::Action::Down)?;
                    }
                    MouseEventKind::ScrollUp => {
                        app.dispatch(app::Action::Up)?;
                    }
                    _ => {}
                },
                _ => {}
            }
            if app.should_quit {
                break;
            }
        }
    }

    Ok(())
}
