//! The TUI's drawing code, split by what part of the screen it owns.
//!
//! This module keeps the top-level frame — the scanning and duplicate
//! progress screens and `draw`, which lays the panes out — and hands
//! each region to a submodule.

mod chrome;
mod lists;
mod popups;
mod text;
mod treemap;

use super::app::{Action, App, ClickZone, DupRow};
use super::nested::{self, TreemapItem};
use super::theme;
use crate::color::{self, Category};
use crate::scanner::Progress;
use crate::stats::ExtStat;
use crate::util::{format_modified, human_bytes, thousands};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Widget, Wrap};
use ratatui::Frame;
use std::sync::atomic::Ordering;
use std::time::Instant;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) fn draw_scanning(f: &mut Frame, progress: &Progress, started: Instant) {
    let area = f.area();
    let files = progress.files.load(Ordering::Relaxed);
    let dirs = progress.dirs.load(Ordering::Relaxed);
    let bytes = progress.bytes.load(Ordering::Relaxed);
    let elapsed = started.elapsed().as_secs_f64();

    let text = vec![
        Line::from(Span::styled(
            "rustdirstat",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Scanning... {elapsed:.1}s")),
        Line::from(format!(
            "{dirs} directories, {files} files, {}",
            human_bytes(bytes)
        )),
        Line::from(""),
        Line::from(Span::styled(
            "(press q to cancel)",
            Style::default().fg(theme::MUTED),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(true))
        .title(" rustdirstat ");
    let p = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(p, area);
}

pub(super) fn draw_duplicate_progress(
    f: &mut Frame,
    progress: &crate::duplicates::DupProgress,
    started: Instant,
) {
    let area = f.area();
    let hashed = progress.hashed.load(Ordering::Relaxed);
    let total = progress.candidates_total.load(Ordering::Relaxed);
    let elapsed = started.elapsed().as_secs_f64();

    let status = if total > 0 {
        format!(
            "Hashed {} of {} candidate files",
            thousands(hashed),
            thousands(total)
        )
    } else {
        "Finding same-size candidates...".to_string()
    };

    let text = vec![
        Line::from(Span::styled(
            "rustdirstat",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Scanning for duplicates... {elapsed:.1}s")),
        Line::from(status),
        Line::from(""),
        Line::from(Span::styled(
            "(press q to cancel)",
            Style::default().fg(theme::MUTED),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(true))
        .title(" rustdirstat ");
    let p = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(p, area);
}

pub(super) fn draw(f: &mut Frame, app: &mut App) {
    app.click_zones.clear();

    let area = f.area();
    // Sized to fit however many rows the category legend actually needs
    // at this width (up to all 9 categories, each entry width-dependent)
    // rather than a fixed guess — a guess that happens to be too short
    // silently clips whichever categories don't fit, with no on-screen
    // sign anything is missing.
    let ext_rows = ext_legend_row_count(&app.ext_stats, area.width.saturating_sub(2)).max(1);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),            // header / title bar
            Constraint::Min(6),               // body
            Constraint::Length(ext_rows + 2), // extension breakdown
            Constraint::Length(1),            // footer / status bar
        ])
        .split(area);

    draw_header(f, app, chunks[0]);

    if app.show_duplicates {
        draw_duplicates(f, app, chunks[1]);
    } else if app.show_search {
        draw_search_results(f, app, chunks[1]);
    } else if app.show_top_files {
        draw_top_files(f, app, chunks[1]);
    } else if app.show_treemap {
        app.set_body_area(chunks[1].x, chunks[1].width);
        let treemap_pct = app.treemap_split;
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - treemap_pct),
                Constraint::Percentage(treemap_pct),
            ])
            .split(chunks[1]);
        draw_list(f, app, body[0]);
        draw_treemap(f, app, body[1]);
        // A 1-column drag handle right on the shared border: press and
        // drag to resize, like a normal GUI split pane. A terminal program
        // can't change the OS mouse cursor on hover (that's the terminal
        // emulator's job, not something any TTY app can reach), so instead
        // the handle stays permanently, visibly distinct — an accent-
        // colored bar rather than the plain panel border — so it reads as
        // grabbable without needing hover state at all.
        draw_resize_handle(f, body[1].x, body[1].y, body[1].height);
        app.click_zones.push(ClickZone {
            x: body[1].x,
            y: body[1].y,
            w: 1,
            h: body[1].height,
            action: Action::StartResize,
        });
    } else {
        draw_list(f, app, chunks[1]);
    }

    draw_ext_stats(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);

    if let Some(pending) = &app.pending_delete {
        let name = pending.name.clone();
        let permanent = pending.permanent;
        let is_dir = pending.is_dir;
        draw_confirm_popup(f, app, &name, permanent, is_dir);
    }

    if app.search_mode {
        draw_search_prompt(f, app);
    }

    if app.move_mode {
        draw_move_prompt(f, app);
    }

    if app.show_properties {
        draw_properties_popup(f, app);
    }

    if app.show_wintools {
        draw_wintools_popup(f, app);
    }

    if let Some(idx) = app.pending_wintool {
        draw_wintool_confirm_popup(f, app, idx);
    }

    if app.show_help {
        draw_help_popup(f, app);
    }
}

fn category_of(node: &crate::model::Node) -> Option<Category> {
    if node.is_dir {
        None
    } else {
        node.category
    }
}

fn dim_unless_matching(
    c: Color,
    highlighted: Option<Category>,
    category: Option<Category>,
) -> Color {
    match highlighted {
        Some(h) if Some(h) != category => theme::DIM,
        _ => c,
    }
}

use chrome::*;
use lists::*;
use popups::*;
use text::*;
use treemap::*;
