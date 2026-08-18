use super::app::{Action, App, ClickZone};
use super::nested::{self, TreemapItem};
use crate::color;
use crate::scanner::Progress;
use crate::util::human_bytes;
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
            Style::default().add_modifier(Modifier::BOLD),
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
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
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
            Constraint::Length(3), // header
            Constraint::Min(6),    // body
            Constraint::Length(3), // extension breakdown
            Constraint::Length(1), // footer
        ])
        .split(area);

    draw_header(f, app, chunks[0]);

    if app.show_treemap {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[1]);
        draw_list(f, app, body[0]);
        draw_treemap(f, app, body[1]);
    } else {
        draw_list(f, app, chunks[1]);
    }

    draw_ext_stats(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);

    if let Some(path) = app.pending_delete.clone() {
        draw_confirm_popup(f, app, &path);
    }
}

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let node = app.current_node();
    let title = format!(
        " {}  —  {} ({} files){}  [click to go up] ",
        node.path.display(),
        human_bytes(node.size),
        node.file_count,
        if node.error { "  <access denied>" } else { "" }
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block, area);
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::Back,
    });
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let disp = app.display_children();
    let disp_len = disp.len();
    let total = app.current_node().size.max(1);
    let max_sibling = disp.iter().map(|(_, n)| n.size).max().unwrap_or(1).max(1);
    let bar_width: usize = 14;

    let items: Vec<ListItem> = disp
        .iter()
        .map(|(_, node)| {
            let pct = node.size as f64 / total as f64 * 100.0;
            let filled =
                ((node.size as f64 / max_sibling as f64) * bar_width as f64).round() as usize;
            let filled = filled.min(bar_width);

            let cat_color = if node.is_dir {
                color::category_color("Directory")
            } else {
                color::category_color(color::category_for_ext(node.extension()))
            };
            let bar_color =
                dim_unless_matching(cat_color, &app.highlighted_category, &node_category(node));

            let name_style = if node.is_dir {
                Style::default().fg(bar_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(bar_color)
            };
            let suffix = if node.is_dir {
                "/"
            } else if node.is_symlink {
                "@"
            } else {
                ""
            };
            let err = if node.error { " <access denied>" } else { "" };
            let count = if node.is_dir {
                format!(" {}f", node.file_count)
            } else {
                String::new()
            };

            let line = Line::from(vec![
                Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
                Span::styled(
                    "░".repeat(bar_width - filled),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:>10}", human_bytes(node.size)),
                    Style::default().fg(Color::Gray),
                ),
                Span::raw(format!(" {:>5.1}% ", pct)),
                Span::styled(format!("{}{}", node.name, suffix), name_style),
                Span::styled(count, Style::default().fg(Color::DarkGray)),
                Span::styled(err, Style::default().fg(Color::Red)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(
        " files — sort: {} (click title or press s to cycle) ",
        app.sort.label()
    );
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    if disp_len > 0 {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);

    // Title bar: click to cycle sort.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::CycleSort,
    });

    // One click zone per visible row, honoring the list's scroll offset.
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

fn node_category(node: &crate::model::Node) -> String {
    if node.is_dir {
        "Directory".to_string()
    } else {
        color::category_for_ext(node.extension()).to_string()
    }
}

fn dim_unless_matching(c: Color, highlighted: &Option<String>, category: &str) -> Color {
    match highlighted {
        Some(h) if h != category => Color::Rgb(70, 70, 70),
        _ => c,
    }
}

fn draw_treemap(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" treemap — click a tile to jump to it (t to hide) ");
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

    let items = nested::build(
        app.current_node(),
        inner.x,
        inner.y,
        inner.width,
        inner.height,
    );
    let selected_orig = app
        .display_children()
        .get(app.selected)
        .map(|(idx, _)| *idx);

    let widget = TreemapWidget {
        items: &items,
        selected_orig,
        highlighted: app.highlighted_category.clone(),
    };
    f.render_widget(widget, inner);

    for item in &items {
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
    highlighted: Option<String>,
}

impl<'a> Widget for TreemapWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, item) in self.items.iter().enumerate() {
            if item.w == 0 || item.h == 0 {
                continue;
            }
            let base = color::category_color(&item.category);
            // Cushion-style shading: darker with depth, alternated by
            // sibling index, so nested rectangles read as distinct tiles.
            let depth_factor = 1.0 - (item.depth as f32 * 0.09).min(0.55);
            let parity = if i % 2 == 0 { 1.0 } else { 0.88 };
            let mut bg = lighten(base, depth_factor * parity);
            if let Some(h) = &self.highlighted {
                if h != &item.category {
                    bg = Color::Rgb(50, 50, 50);
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

            let is_selected = item.depth == 0
                && item.index_path.len() == 1
                && Some(item.index_path[0]) == self.selected_orig;
            if is_selected {
                draw_border(buf, area, item.x, item.y, item.w, item.h, Color::White);
            }

            if item.w >= 3 && item.h >= 1 {
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

fn draw_ext_stats(f: &mut Frame, app: &mut App, area: Rect) {
    let total: u64 = app.ext_stats.iter().map(|s| s.size).sum::<u64>().max(1);
    let mut spans = Vec::new();
    let mut x = area.x + 1;
    for (i, stat) in app.ext_stats.iter().take(9).enumerate() {
        let pct = stat.size as f64 / total as f64 * 100.0;
        let color = dim_unless_matching(
            color::category_color(&stat.category),
            &app.highlighted_category,
            &stat.category,
        );
        let text = format!(
            "{} ■ {} {} ({:.0}%)  ",
            i + 1,
            stat.category,
            human_bytes(stat.size),
            pct
        );
        let w = text.chars().count() as u16;
        spans.push(Span::styled(text, Style::default().fg(color)));
        app.click_zones.push(ClickZone {
            x,
            y: area.y + 1,
            w,
            h: 1,
            action: Action::ToggleHighlight(stat.category.clone()),
        });
        x += w;
    }
    let title = if app.highlighted_category.is_some() {
        " extensions — click to toggle highlight (0 to clear) "
    } else {
        " extensions (current view) — click a category to highlight it in the treemap "
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(Line::from(spans))
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &mut App, area: Rect) {
    if let Some(msg) = app.message.clone() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(Color::Yellow),
            ))),
            area,
        );
        return;
    }

    let buttons: [(&str, Action); 8] = [
        ("↑↓ nav", Action::Down),
        ("→ open", Action::OpenSelected),
        ("← back", Action::Back),
        ("s sort", Action::CycleSort),
        ("t treemap", Action::ToggleTreemap),
        ("o file-mgr", Action::OpenInFileManager),
        ("d delete", Action::RequestDelete),
        ("q quit", Action::Quit),
    ];

    let mut spans = Vec::new();
    let mut x = area.x;
    for (i, (label, action)) in buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
            x += 2;
        }
        let w = label.chars().count() as u16;
        spans.push(Span::styled(*label, Style::default().fg(Color::Cyan)));
        app.click_zones.push(ClickZone {
            x,
            y: area.y,
            w,
            h: 1,
            action: action.clone(),
        });
        x += w;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_confirm_popup(f: &mut Frame, app: &mut App, path: &std::path::Path) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" confirm delete ")
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(area);
    let text = vec![
        Line::from(format!("Permanently delete '{}'?", path.display())),
        Line::from(""),
        Line::from(Span::styled(
            "This cannot be undone.",
            Style::default().fg(Color::Red),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " [ Y ]es ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(
                " [ N ]o ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);

    // Pushed first (and so checked last, since hit-testing favors the most
    // recently pushed zone) so the buttons below take priority over it.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelDelete,
    });
    let button_row = inner.y + 4;
    app.click_zones.push(ClickZone {
        x: inner.x,
        y: button_row,
        w: 9,
        h: 1,
        action: Action::ConfirmDelete,
    });
    app.click_zones.push(ClickZone {
        x: inner.x + 12,
        y: button_row,
        w: 8,
        h: 1,
        action: Action::CancelDelete,
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
