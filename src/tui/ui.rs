use super::app::{Action, App, ClickZone, DupRow};
use super::nested::{self, TreemapItem};
use super::theme;
use crate::color::{self, Category};
use crate::scanner::Progress;
use crate::util::{format_modified, human_bytes, thousands};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Widget, Wrap};
use ratatui::Frame;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub fn draw_scanning(f: &mut Frame, progress: &Progress, started: Instant) {
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

pub fn draw_duplicate_progress(
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

pub fn draw(f: &mut Frame, app: &mut App) {
    app.click_zones.clear();

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / title bar
            Constraint::Min(6),    // body
            Constraint::Length(4), // extension breakdown (2 rows: up to all 9 categories can wrap)
            Constraint::Length(1), // footer / status bar
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

fn draw_search_prompt(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 20, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(" Search this subtree ", theme::title_bar()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text =
        vec![
        Line::from(vec![Span::raw("> "), Span::raw(&app.search_query), Span::raw("▌")]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter to search, Esc to cancel. * and ? are wildcards; prefix with re: for a regex.",
            Style::default().fg(theme::MUTED),
        )),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

fn draw_move_prompt(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 20, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let name = app
        .display_children()
        .get(app.selected)
        .map(|(_, n)| n.name.clone())
        .unwrap_or_default();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            format!(" Move '{name}' to "),
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(vec![
            Span::raw("> "),
            Span::raw(&app.move_destination),
            Span::raw("▌"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter a destination folder (or full path) and press Enter. Esc to cancel.",
            Style::default().fg(theme::MUTED),
        )),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
}

fn draw_properties_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 40, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            " Properties (press any key to close) ",
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some((_, node)) = app
        .display_children()
        .get(app.selected)
        .map(|(i, n)| (*i, *n))
    else {
        f.render_widget(Paragraph::new("Nothing selected."), inner);
        return;
    };

    let mut full_path = app.current_path();
    full_path.push(&node.name);
    let kind = if node.is_dir {
        "Directory"
    } else if node.is_symlink {
        "Symlink"
    } else {
        "File"
    };
    let category =
        node.category
            .map(|c| c.label())
            .unwrap_or(if node.is_dir { "-" } else { "Other" });

    let mut rows: Vec<(&str, String)> = vec![
        ("Path", full_path.display().to_string()),
        ("Type", kind.to_string()),
        ("Category", category.to_string()),
        ("Size (logical)", human_bytes(node.size)),
        ("Size (physical)", human_bytes(node.physical_size)),
        ("Modified", format_modified(node.modified)),
    ];
    if node.is_dir {
        rows.push(("Files", thousands(node.file_count)));
        rows.push(("Subdirectories", thousands(node.dir_count)));
    }
    if node.unreadable_count > 0 {
        rows.push(("Unreadable entries", thousands(node.unreadable_count)));
    }
    if node.error {
        rows.push(("Access", "denied".to_string()));
    }

    let lines: Vec<Line> = rows
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(
                    format!("{:<17}", label),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(value.clone()),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::ToggleProperties,
    });
}

fn draw_wintools_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(70, 60, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            " Windows system tools (Esc/T to close) ",
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let available = cfg!(windows);
    let mut lines: Vec<Line> = Vec::new();
    for (i, tool) in crate::wintools::TOOLS.iter().enumerate() {
        let selected = i == app.wintools_selected;
        let name_style = if !available {
            Style::default().fg(theme::MUTED)
        } else if selected {
            theme::selection()
        } else if tool.destructive {
            Style::default().fg(theme::WARNING)
        } else {
            Style::default().fg(Color::Reset)
        };
        let marker = if selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{marker}{}", tool.name),
            name_style,
        )));
        if selected {
            lines.push(Line::from(Span::styled(
                format!("    {}", tool.description),
                Style::default().fg(theme::MUTED),
            )));
        }
    }
    if !available {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "These tools call native Windows utilities and aren't available on this platform.",
            Style::default().fg(theme::MUTED),
        )));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::ToggleWinTools,
    });
    for (i, _) in crate::wintools::TOOLS.iter().enumerate() {
        // Each entry is drawn as either one line (unselected) or two
        // lines (selected, with its description) — approximate the
        // click target as the single name row; good enough for a list
        // this short, and selecting first before activating (arrow keys,
        // or a first click to select then a second to open) is the same
        // two-step interaction other apps use for a destructive menu.
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        app.click_zones.push(ClickZone {
            x: inner.x,
            y,
            w: inner.width,
            h: 1,
            action: Action::SelectWinTool(i),
        });
    }
}

fn draw_wintool_confirm_popup(f: &mut Frame, app: &mut App, idx: usize) {
    let Some(tool) = crate::wintools::TOOLS.get(idx) else {
        return;
    };
    let area = centered_rect(60, 24, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .title(Span::styled(" Confirm ", theme::danger_title_bar()))
        .border_style(Style::default().fg(theme::DANGER));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = vec![
        Line::from(format!("Run '{}'?", tool.name)),
        Line::from(""),
        Line::from(Span::styled(
            tool.description,
            Style::default().fg(theme::DANGER),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " [ Y ]es ",
                Style::default()
                    .fg(Color::Black)
                    .bg(theme::DANGER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                " [ N ]o ",
                Style::default()
                    .fg(theme::BUTTON_NEUTRAL_FG)
                    .bg(theme::BUTTON_NEUTRAL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelWinTool,
    });
    let button_row = inner.y + 4;
    app.click_zones.push(ClickZone {
        x: inner.x,
        y: button_row,
        w: 9,
        h: 1,
        action: Action::ConfirmWinTool,
    });
    app.click_zones.push(ClickZone {
        x: inner.x + 12,
        y: button_row,
        w: 8,
        h: 1,
        action: Action::CancelWinTool,
    });
}

/// The recursive subtree search results — independent of the normal
/// directory browser, listing every match found anywhere below the
/// current directory (not just its direct children).
fn draw_search_results(f: &mut Frame, app: &mut App, area: Rect) {
    let base = app.path_indices.clone();
    let base_path = app.current_path();
    let phys = app.use_physical;
    let show_details = app.detailed;

    let items: Vec<ListItem> = app
        .search_results
        .iter()
        .map(|hit| {
            let shown_size = if phys { hit.physical_size } else { hit.size };
            let muted = app
                .highlighted_category
                .is_some_and(|h| Some(h) != hit.category);
            let color = if muted {
                theme::MUTED
            } else if hit.is_dir {
                theme::ACCENT
            } else {
                Color::Reset
            };

            let mut full_idx = base.clone();
            full_idx.extend(&hit.index_path);
            let full_path = app.tree.path_for(&full_idx);
            let rel = full_path.strip_prefix(&base_path).unwrap_or(&full_path);
            let suffix = if hit.is_dir { "/" } else { "" };

            let mut spans = vec![
                Span::styled(
                    format!("{:>9}", human_bytes(shown_size)),
                    Style::default().fg(theme::MUTED),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{}{}", rel.display(), suffix),
                    Style::default().fg(color),
                ),
            ];
            if show_details {
                spans.push(Span::styled(
                    format!("  {}", format_modified(hit.modified)),
                    Style::default().fg(theme::MUTED),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = app.search_results.len();
    let mut title = format!(" Search: \"{}\" — {} matches", app.search_query, count);
    if app.search_truncated {
        title.push_str(" (truncated)");
    }
    title.push_str(" — S to close ");
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(theme::panel_border(true))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(theme::selection());

    let mut state = ListState::default();
    if count > 0 {
        state.select(Some(app.selected.min(count - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);

    if let Some(err) = &app.search_error {
        let msg_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(2),
            width: area.width.saturating_sub(4),
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                err.as_str(),
                Style::default().fg(theme::DANGER),
            )),
            msg_area,
        );
    }

    let inner_y = area.y + 1;
    let inner_h = area.height.saturating_sub(2) as usize;
    let offset = state.offset();
    for row in offset..(offset + inner_h).min(count) {
        let y = inner_y + (row - offset) as u16;
        app.click_zones.push(ClickZone {
            x: area.x + 1,
            y,
            w: area.width.saturating_sub(2),
            h: 1,
            action: Action::SelectRow(row),
        });
    }
}

fn draw_duplicates(f: &mut Frame, app: &mut App, area: Rect) {
    let root_path = app.tree.root_path.clone();
    let mut group_num = 0usize;

    let items: Vec<ListItem> = app
        .duplicate_rows
        .iter()
        .map(|row| match row {
            DupRow::Header { size, count } => {
                group_num += 1;
                let wasted = *size * (*count as u64 - 1);
                ListItem::new(Line::from(Span::styled(
                    format!(
                        "Group {group_num} — {count} × {}  ({} wasted)",
                        human_bytes(*size),
                        human_bytes(wasted)
                    ),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )))
            }
            DupRow::Member { index_path } => {
                let full_path = app.tree.path_for(index_path);
                let rel = full_path.strip_prefix(&root_path).unwrap_or(&full_path);
                ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(rel.display().to_string(), Style::default().fg(Color::Reset)),
                ]))
            }
        })
        .collect();

    let count = app.duplicate_rows.len();
    let mut title = if app.duplicate_group_count == 0 {
        " No duplicate files found".to_string()
    } else {
        format!(
            " Duplicates: {} groups, {} wasted",
            thousands(app.duplicate_group_count as u64),
            human_bytes(app.duplicate_total_wasted)
        )
    };
    if app.duplicate_truncated {
        title.push_str(" (showing largest groups)");
    }
    title.push_str(" — u to close ");
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(theme::panel_border(true))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(theme::selection());

    let mut state = ListState::default();
    if count > 0 {
        state.select(Some(app.selected.min(count - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);

    let inner_y = area.y + 1;
    let inner_h = area.height.saturating_sub(2) as usize;
    let offset = state.offset();
    for row in offset..(offset + inner_h).min(count) {
        let y = inner_y + (row - offset) as u16;
        app.click_zones.push(ClickZone {
            x: area.x + 1,
            y,
            w: area.width.saturating_sub(2),
            h: 1,
            action: Action::SelectRow(row),
        });
    }
}

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let node = app.current_node();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(true));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let filter_suffix = if app.filter_mode {
        format!("   search: {}▌", app.filter)
    } else if !app.filter.is_empty() {
        format!("   filtered: \"{}\"", app.filter)
    } else {
        String::new()
    };
    let size_note = if app.use_physical { " (physical)" } else { "" };
    let stats = format!(
        "   ·   {}{}, {} files{}{}",
        human_bytes(node.effective_size(app.use_physical)),
        size_note,
        thousands(node.file_count),
        if node.error { "   <access denied>" } else { "" },
        filter_suffix,
    );
    let free_space_extra = if app.path_indices.is_empty() && app.tree.is_volume_root() {
        match (app.tree.volume_free, app.tree.volume_total) {
            (Some(free), Some(total)) => format!(
                "   ·   {} free of {} on this volume",
                human_bytes(free),
                human_bytes(total)
            ),
            _ => String::new(),
        }
    } else {
        String::new()
    };
    // Some entries in this subtree couldn't be read (permission edge case,
    // a race with something deleting them mid-scan) and were left out of
    // every total rather than silently pretending they don't exist —
    // surfaced here so "40 KB" and "40 KB, but some of it we couldn't
    // see" don't look identical.
    let warning_extra = if node.unreadable_count > 0 {
        format!("  ⚠ {} unreadable ", thousands(node.unreadable_count))
    } else {
        String::new()
    };

    // The path is the most compressible part of this line — everything
    // else here is fixed-width and must never be silently clipped just
    // because the terminal is narrow or the path is long, especially the
    // unreadable-count warning above (that's the exact bug it exists to
    // prevent, just one level up: a truncated *line* hiding it is no
    // different from a truncated *total*). So the fixed parts are sized
    // first and the path gets whatever width is left over, truncated in
    // the middle — keeping both the volume/drive prefix and the leaf
    // directory name, the two most identifying parts of a long path —
    // rather than cut off at the end.
    let reserved = 1
        + stats.chars().count()
        + free_space_extra.chars().count()
        + warning_extra.chars().count();
    let available_for_path = (inner.width as usize).saturating_sub(reserved).max(8);
    let full_path = app.current_path().display().to_string();
    let path_display = truncate_middle(&full_path, available_for_path);

    let bar = theme::title_bar();
    let mut spans = vec![Span::styled(format!(" {path_display}{stats}"), bar)];
    if !free_space_extra.is_empty() {
        spans.push(Span::styled(free_space_extra, bar));
    }
    if !warning_extra.is_empty() {
        spans.push(Span::styled(
            warning_extra,
            bar.fg(theme::WARNING).add_modifier(Modifier::BOLD),
        ));
    }
    let p = Paragraph::new(Line::from(spans)).style(bar);
    f.render_widget(p, inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::Back,
    });
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

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let disp = app.display_children();
    let disp_len = disp.len();
    let phys = app.use_physical;
    let total = app.current_node().effective_size(phys).max(1);
    let max_sibling = disp
        .iter()
        .map(|(_, n)| n.effective_size(phys))
        .max()
        .unwrap_or(1)
        .max(1);
    let bar_width: usize = 10;
    let show_details = app.detailed;

    let items: Vec<ListItem> = disp
        .iter()
        .map(|(_, node)| {
            let shown_size = node.effective_size(phys);
            let pct = shown_size as f64 / total as f64 * 100.0;
            let filled =
                ((shown_size as f64 / max_sibling as f64) * bar_width as f64).round() as usize;
            let filled = filled.min(bar_width);

            let cat = category_of(node);
            let muted = app.highlighted_category.is_some_and(|h| Some(h) != cat);
            let bar_color = if muted { theme::MUTED } else { theme::ACCENT };
            let name_color = if muted {
                theme::MUTED
            } else if node.is_dir {
                theme::ACCENT
            } else {
                Color::Reset
            };

            let name_style = if node.is_dir {
                Style::default().fg(name_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(name_color)
            };
            let icon = if node.is_dir { "▸ " } else { "  " };
            let suffix = if node.is_dir {
                "/"
            } else if node.is_symlink {
                "@"
            } else {
                ""
            };
            let err = if node.error { " <access denied>" } else { "" };
            // Only shown when the directory itself was readable but
            // something inside it wasn't — `error` above already covers
            // "couldn't read this one at all".
            let warn = if !node.error && node.unreadable_count > 0 {
                " ⚠"
            } else {
                ""
            };

            let mut spans = vec![
                Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
                Span::styled(
                    "░".repeat(bar_width - filled),
                    Style::default().fg(theme::PANEL_BORDER),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:>9}", human_bytes(shown_size)),
                    Style::default().fg(theme::MUTED),
                ),
                Span::raw(format!(" {:>5.1}%  ", pct)),
                Span::styled(icon, Style::default().fg(name_color)),
                Span::styled(format!("{}{}", node.name, suffix), name_style),
                Span::styled(err, Style::default().fg(theme::DANGER)),
                Span::styled(warn, Style::default().fg(theme::WARNING)),
            ];
            if show_details {
                let count = if node.is_dir {
                    format!(
                        "  {} files, {} dirs",
                        thousands(node.file_count),
                        thousands(node.dir_count)
                    )
                } else {
                    String::new()
                };
                spans.push(Span::styled(count, Style::default().fg(theme::MUTED)));
                spans.push(Span::styled(
                    format!("  {}", format_modified(node.modified)),
                    Style::default().fg(theme::MUTED),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let size_label = if phys { "physical" } else { "logical" };
    let title = format!(
        " Files — sort: {}, {} size  (s sort, p size, m details) ",
        app.sort.label(),
        size_label
    );
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(theme::panel_border(true))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(theme::selection());

    let mut state = ListState::default();
    if disp_len > 0 {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::CycleSort,
    });

    let inner_y = area.y + 1;
    let inner_h = area.height.saturating_sub(2) as usize;
    let offset = state.offset();
    for row in offset..(offset + inner_h).min(disp_len) {
        let y = inner_y + (row - offset) as u16;
        app.click_zones.push(ClickZone {
            x: area.x + 1,
            y,
            w: area.width.saturating_sub(2),
            h: 1,
            action: Action::SelectRow(row),
        });
    }
}

/// The "biggest files anywhere in this subtree" flat view — independent of
/// the normal directory-by-directory browser, for spotting what's actually
/// worth deleting without navigating.
fn draw_top_files(f: &mut Frame, app: &mut App, area: Rect) {
    let base = app.path_indices.clone();
    let base_path = app.current_path();
    let phys = app.use_physical;
    let max_size = app
        .top_files_cache
        .first()
        .map(|t| if phys { t.physical_size } else { t.size })
        .unwrap_or(1)
        .max(1);
    let bar_width: usize = 10;
    let show_details = app.detailed;

    let items: Vec<ListItem> = app
        .top_files_cache
        .iter()
        .map(|tf| {
            let shown_size = if phys { tf.physical_size } else { tf.size };
            let filled =
                ((shown_size as f64 / max_size as f64) * bar_width as f64).round() as usize;
            let filled = filled.min(bar_width);
            let muted = app
                .highlighted_category
                .is_some_and(|h| Some(h) != tf.category);
            let color = if muted { theme::MUTED } else { theme::ACCENT };
            let name_color = if muted { theme::MUTED } else { Color::Reset };

            let mut full_idx = base.clone();
            full_idx.extend(&tf.index_path);
            let full_path = app.tree.path_for(&full_idx);
            let rel = full_path.strip_prefix(&base_path).unwrap_or(&full_path);

            let mut spans = vec![
                Span::styled("█".repeat(filled), Style::default().fg(color)),
                Span::styled(
                    "░".repeat(bar_width - filled),
                    Style::default().fg(theme::PANEL_BORDER),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:>9}", human_bytes(shown_size)),
                    Style::default().fg(theme::MUTED),
                ),
                Span::raw("  "),
                Span::styled(rel.display().to_string(), Style::default().fg(name_color)),
            ];
            if show_details {
                spans.push(Span::styled(
                    format!("  {}", format_modified(tf.modified)),
                    Style::default().fg(theme::MUTED),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = app.top_files_cache.len();
    let title = format!(" Biggest files in this subtree (top {count}) — f to close ");
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(theme::panel_border(true))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(theme::selection());

    let mut state = ListState::default();
    if count > 0 {
        state.select(Some(app.selected.min(count - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);

    let inner_y = area.y + 1;
    let inner_h = area.height.saturating_sub(2) as usize;
    let offset = state.offset();
    for row in offset..(offset + inner_h).min(count) {
        let y = inner_y + (row - offset) as u16;
        app.click_zones.push(ClickZone {
            x: area.x + 1,
            y,
            w: area.width.saturating_sub(2),
            h: 1,
            action: Action::SelectRow(row),
        });
    }
}

/// A permanently-visible "grab here" bar over the panel divider — bright
/// accent color, distinct from the ordinary panel border either side of it.
fn draw_resize_handle(f: &mut Frame, x: u16, y: u16, height: u16) {
    if height == 0 {
        return;
    }
    let mid = height / 2;
    let lines: Vec<Line> = (0..height)
        .map(|row| {
            // A short dotted grip glyph at the vertical center reads as
            // "drag" more clearly than a plain solid line would.
            let glyph = if row == mid || row == mid.saturating_sub(1) || row == mid + 1 {
                "┃"
            } else {
                "│"
            };
            Line::from(Span::styled(
                glyph,
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines),
        Rect {
            x,
            y,
            width: 1,
            height,
        },
    );
}

fn draw_treemap(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(false))
        .title(Span::styled(
            " Treemap — click a tile to jump to it  ·  drag the left edge to resize ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::ToggleTreemap,
    });

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Free space only makes sense relative to the whole volume, so it's
    // only shown when the scan root IS the volume root — not for every
    // scan just because you happen to be browsing at its top (a small
    // subfolder's content compared against gigabytes of unrelated free
    // space elsewhere on the drive would swamp the treemap with a
    // free-space tile representing almost the entire area), and not
    // injected into subfolder views either, where it wouldn't correspond
    // to anything real.
    let free_space = if app.path_indices.is_empty() && app.tree.is_volume_root() {
        app.tree.volume_free
    } else {
        None
    };
    let items = nested::build(
        app.current_node(),
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        app.use_physical,
        free_space,
    );
    let selected_orig = app
        .display_children()
        .get(app.selected)
        .map(|(idx, _)| *idx);

    let widget = TreemapWidget {
        items: &items,
        selected_orig,
        highlighted: app.highlighted_category,
    };
    f.render_widget(widget, inner);

    for item in &items {
        if item.is_free_space {
            continue; // not a real entry — nothing to navigate to
        }
        app.click_zones.push(ClickZone {
            x: item.x,
            y: item.y,
            w: item.w,
            h: item.h,
            action: Action::NavigateTo(item.index_path.clone()),
        });
    }
}

struct TreemapWidget<'a> {
    items: &'a [TreemapItem],
    selected_orig: Option<usize>,
    highlighted: Option<Category>,
}

impl<'a> Widget for TreemapWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, item) in self.items.iter().enumerate() {
            if item.w == 0 || item.h == 0 {
                continue;
            }
            let base = if item.is_free_space {
                color::free_space_color()
            } else if item.is_dir {
                color::directory_color()
            } else {
                color::ext_color(&item.name)
            };
            // Cushion-style shading: darker with depth, alternated by
            // sibling index, so nested rectangles read as distinct tiles.
            let depth_factor = 1.0 - (item.depth as f32 * 0.09).min(0.55);
            let parity = if i % 2 == 0 { 1.0 } else { 0.88 };
            let mut bg = lighten(base, depth_factor * parity);
            if !item.is_free_space {
                if let Some(h) = self.highlighted {
                    if Some(h) != item.category {
                        // Blend toward theme::DIM rather than replacing
                        // the tile outright — a flat gray recreates the
                        // "everything looks the same" flatness that
                        // per-extension coloring was built to fix, for
                        // every non-matching tile at once. Keeping a
                        // little of the real hue means shape and rough
                        // color still read while the highlighted set
                        // clearly stands out.
                        bg = blend_toward(bg, theme::DIM, 0.75);
                    }
                }
            }

            for yy in 0..item.h {
                for xx in 0..item.w {
                    let px = item.x + xx;
                    let py = item.y + yy;
                    if px >= area.x + area.width || py >= area.y + area.height {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((px, py)) {
                        cell.set_symbol(" ");
                        cell.set_bg(bg);
                    }
                }
            }

            // Every tile gets a dark separator border so adjacent tiles of
            // the same (or similar) color still read as distinct pieces —
            // without this, same-category siblings visually merge into one
            // shapeless blob.
            draw_border(buf, area, item.x, item.y, item.w, item.h, theme::SHADOW);

            let is_selected = item.depth == 0
                && item.index_path.len() == 1
                && Some(item.index_path[0]) == self.selected_orig;
            if is_selected {
                draw_border(
                    buf,
                    area,
                    item.x,
                    item.y,
                    item.w,
                    item.h,
                    theme::SELECTED_BORDER,
                );
            }

            // A dense tree (a build output directory, node_modules, ...)
            // recurses into thousands of small tiles — that's real data,
            // not a bug, and each one still needs its own color to be
            // accurate. But labeling every single one, down to a 3-cell
            // sliver, turns the treemap into illegible fragments like
            // "bu…"/"qu…" that add noise without conveying anything —
            // worse than no label, since it reads as clutter rather than
            // as missing information. WinDirStat doesn't label tiles it
            // can't fit a real name into either; small tiles just show
            // their color. 6 cells is enough for a short full name (e.g.
            // "src/") or a handful of real characters before the ellipsis
            // — below that, skip the label rather than draw noise.
            if item.w >= 6 && item.h >= 1 {
                let label = truncate(&item.name, item.w as usize - 1);
                let style = Style::default().fg(contrast_fg(bg)).bg(bg);
                let style = if item.is_dir {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                };
                buf.set_string(item.x, item.y, &label, style);
            }
        }
    }
}

fn draw_border(buf: &mut Buffer, area: Rect, x: u16, y: u16, w: u16, h: u16, c: Color) {
    if w == 0 || h == 0 {
        return;
    }
    for xx in 0..w {
        let px = x + xx;
        for &py in &[y, y + h.saturating_sub(1)] {
            if px < area.x + area.width && py < area.y + area.height {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_fg(c);
                }
            }
        }
    }
    for yy in 0..h {
        let py = y + yy;
        for &px in &[x, x + w.saturating_sub(1)] {
            if px < area.x + area.width && py < area.y + area.height {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_fg(c);
                }
            }
        }
    }
}

fn lighten(c: Color, factor: f32) -> Color {
    if let Color::Rgb(r, g, b) = c {
        let f = factor.clamp(0.0, 1.5);
        let adj = |v: u8| ((v as f32) * f).min(255.0) as u8;
        Color::Rgb(adj(r), adj(g), adj(b))
    } else {
        c
    }
}

/// Mixes `c` toward `target` by `t` (0 = unchanged, 1 = fully `target`).
fn blend_toward(c: Color, target: Color, t: f32) -> Color {
    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (c, target) {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t) as u8;
        Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
    } else {
        target
    }
}

fn contrast_fg(bg: Color) -> Color {
    if let Color::Rgb(r, g, b) = bg {
        let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        if luminance > 140.0 {
            Color::Black
        } else {
            Color::White
        }
    } else {
        Color::White
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        format!(
            "{}…",
            s.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

/// Truncates `s` to at most `max` characters by cutting out the middle
/// and joining the head/tail with an ellipsis — for a filesystem path,
/// the drive/volume prefix and the leaf (innermost) directory name are
/// usually the most identifying parts, so keeping both ends and losing
/// the middle preserves more useful information than `truncate`'s plain
/// trailing ellipsis would.
fn truncate_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let keep = max - 1;
    let head = keep.div_ceil(2);
    let tail = keep / 2;
    let head_str: String = chars[..head].iter().collect();
    let tail_str: String = chars[chars.len() - tail..].iter().collect();
    format!("{head_str}…{tail_str}")
}

fn draw_ext_stats(f: &mut Frame, app: &mut App, area: Rect) {
    let total: u64 = app.ext_stats.iter().map(|s| s.size).sum::<u64>().max(1);
    let inner_width = area.width.saturating_sub(2);

    // Every category the `1`-`9` highlight keys can address gets a legend
    // entry — capping this below 9 (as it once was, at 5) meant pressing
    // a key for an unshown category silently highlighted something with
    // no on-screen label to explain what just happened. Rows are packed
    // by hand (rather than relying on `Paragraph`'s own word-wrap) so the
    // click zone recorded for each entry is guaranteed to land exactly
    // where that entry was actually drawn, on whichever row it wrapped
    // to — a click zone computed independently of the real wrap points
    // would silently drift out of sync with them.
    let mut lines: Vec<Line> = Vec::new();
    let mut row_spans: Vec<Span> = Vec::new();
    let mut col = 0u16;
    let mut row = 0u16;
    for (i, stat) in app.ext_stats.iter().take(Category::COUNT).enumerate() {
        let pct = stat.size as f64 / total as f64 * 100.0;
        let color = dim_unless_matching(
            stat.category.color(),
            app.highlighted_category,
            Some(stat.category),
        );
        let text = format!("{} ■ {}  {:.1}%   ", i + 1, stat.category.label(), pct);
        let w = text.chars().count() as u16;
        if col + w > inner_width && col > 0 {
            lines.push(Line::from(std::mem::take(&mut row_spans)));
            col = 0;
            row += 1;
        }
        row_spans.push(Span::styled(text, Style::default().fg(color)));
        app.click_zones.push(ClickZone {
            x: area.x + 1 + col,
            y: area.y + 1 + row,
            w,
            h: 1,
            action: Action::ToggleHighlight(stat.category),
        });
        col += w;
    }
    if !row_spans.is_empty() {
        lines.push(Line::from(row_spans));
    }

    // This groups files by *category* (a fixed, semantic bucket), not by
    // the individual per-extension hue actually painted in the treemap
    // above — clicking still highlights the right tiles, but the swatch
    // color here is this button's own accent, not a literal preview of
    // what you'll see highlighted.
    let title = " File categories — click to highlight ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(false))
        .title(title);
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &mut App, area: Rect) {
    if let Some(msg) = app.message.clone() {
        let p = Paragraph::new(Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Black),
        )))
        .style(Style::default().bg(theme::ACCENT_TEXT));
        f.render_widget(p, area);
        return;
    }

    // Only the most common actions get a footer button; everything else
    // (sort, treemap, search, refresh, export, file manager, permanent
    // delete) stays a keyboard shortcut discoverable via "? help" — a
    // dozen buttons crammed into one row was the single busiest part of
    // the whole interface.
    let buttons: [(&str, Action); 3] = [
        (" Enter  Open ", Action::OpenSelected),
        (" Backspace  Up ", Action::Back),
        (" d  Delete ", Action::RequestDelete),
    ];

    let mut spans = Vec::new();
    let mut x = area.x;
    // Fill the whole row with the status-bar background first.
    let base = Paragraph::new("").style(theme::status_bar());
    f.render_widget(base, area);

    for (label, action) in buttons.iter() {
        let w = label.chars().count() as u16;
        spans.push(Span::styled(*label, theme::button()));
        spans.push(Span::raw("  "));
        app.click_zones.push(ClickZone {
            x,
            y: area.y,
            w,
            h: 1,
            action: action.clone(),
        });
        x += w + 2;
    }
    let quit_label = " q  Quit ";
    spans.push(Span::styled(
        quit_label,
        Style::default().bg(theme::DANGER_BG).fg(theme::DANGER),
    ));
    app.click_zones.push(ClickZone {
        x,
        y: area.y,
        w: quit_label.chars().count() as u16,
        h: 1,
        action: Action::Quit,
    });
    let help_label = "  ?  more shortcuts ";
    spans.push(Span::styled(help_label, theme::button()));
    app.click_zones.push(ClickZone {
        x: x + quit_label.chars().count() as u16,
        y: area.y,
        w: help_label.chars().count() as u16,
        h: 1,
        action: Action::ToggleHelp,
    });

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn shadow(f: &mut Frame, area: Rect) {
    let shadow_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width,
        height: area.height,
    };
    let full = f.area();
    if shadow_area.x + shadow_area.width <= full.width
        && shadow_area.y + shadow_area.height <= full.height
    {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(theme::SHADOW)),
            shadow_area,
        );
    }
}

fn draw_confirm_popup(f: &mut Frame, app: &mut App, name: &str, permanent: bool, is_dir: bool) {
    let area = centered_rect(60, 24, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let title = if permanent {
        " Permanently Delete "
    } else {
        " Move to Trash "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .title(Span::styled(title, theme::danger_title_bar()))
        .border_style(Style::default().fg(theme::DANGER));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let action_desc = if permanent {
        vec![Line::from(Span::styled(
            "This bypasses the Recycle Bin/Trash and cannot be undone.",
            Style::default().fg(theme::DANGER),
        ))]
    } else {
        vec![Line::from(
            "This can be undone from your OS Recycle Bin/Trash.",
        )]
    };
    let mut text = vec![Line::from(format!("Delete '{name}'?")), Line::from("")];
    text.extend(action_desc);
    if is_dir {
        text.push(Line::from(
            "Or empty it — delete its contents, keep the folder.",
        ));
    }
    text.push(Line::from(""));
    let mut buttons = vec![
        Span::styled(
            " [ Y ]es ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
    ];
    if is_dir {
        buttons.push(Span::styled(
            " [ E ]mpty ",
            Style::default()
                .fg(Color::Black)
                .bg(theme::WARNING)
                .add_modifier(Modifier::BOLD),
        ));
        buttons.push(Span::raw("   "));
    }
    buttons.push(Span::styled(
        " [ N ]o ",
        Style::default()
            .fg(theme::BUTTON_NEUTRAL_FG)
            .bg(theme::BUTTON_NEUTRAL_BG)
            .add_modifier(Modifier::BOLD),
    ));
    text.push(Line::from(buttons));
    let p = Paragraph::new(text).wrap(Wrap { trim: true });
    f.render_widget(p, inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelDelete,
    });
    let button_row = inner.y + if is_dir { 5 } else { 4 };
    app.click_zones.push(ClickZone {
        x: inner.x,
        y: button_row,
        w: 9,
        h: 1,
        action: Action::ConfirmDelete,
    });
    let mut next_x = inner.x + 12;
    if is_dir {
        app.click_zones.push(ClickZone {
            x: next_x,
            y: button_row,
            w: 11,
            h: 1,
            action: Action::ConfirmEmpty,
        });
        next_x += 14;
    }
    app.click_zones.push(ClickZone {
        x: next_x,
        y: button_row,
        w: 8,
        h: 1,
        action: Action::CancelDelete,
    });
}

fn draw_help_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(70, 80, f.area());
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            " rustdirstat — help (press any key to close) ",
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows: [(&str, &str); 30] = [
        ("↑/↓, k/j", "Move selection"),
        ("→/l/Enter", "Open the selected directory"),
        ("←/h/Backspace", "Go up a directory"),
        ("s", "Cycle sort order (size, name, modified)"),
        ("m", "Show/hide file counts and modified dates"),
        ("p", "Toggle logical vs. physical (on-disk) size"),
        ("t", "Toggle the treemap panel"),
        ("[ / ]", "Resize the treemap panel (or drag its left edge)"),
        ("f", "Toggle the \"biggest files\" flat view"),
        ("/", "Search/filter the current directory by name"),
        ("S", "Search this entire subtree (glob or re: regex)"),
        ("u", "Find duplicate files (by content hash) across the whole scan"),
        ("1-9", "Highlight a file-type category in the treemap"),
        ("0", "Clear the highlight"),
        ("o", "Open the selected item (its default app, or the folder)"),
        ("O", "Reveal the selected item in the OS file manager"),
        ("y", "Copy the selected item's full path to the clipboard"),
        ("M", "Move the selected item to another folder"),
        ("i", "Show properties for the selected item"),
        ("T", "Windows system tools (Disk Cleanup, DISM, shadow copies, ...)"),
        ("r", "Rescan from the root (keeps your current location)"),
        ("e", "Export a text report of the current view to a file"),
        ("E", "Export a full CSV of the current view's subtree to a file"),
        ("d", "Delete the selected item (moves to Recycle Bin/Trash)"),
        ("D", "Delete PERMANENTLY (bypasses Recycle Bin/Trash)"),
        (
            "d, then e",
            "In the delete popup, empty a folder instead (keep it, delete contents)",
        ),
        ("?", "Toggle this help"),
        ("q, Esc", "Quit"),
        ("", ""),
        ("Mouse", "Every action above also works by clicking — treemap tiles, list rows, title bars, the header, and the footer buttons are all clickable."),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, desc)| {
            if key.is_empty() {
                Line::from("")
            } else {
                Line::from(vec![
                    Span::styled(
                        format!("{:<16}", key),
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(*desc),
                ])
            }
        })
        .collect();
    let p = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(p, inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::ToggleHelp,
    });
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
