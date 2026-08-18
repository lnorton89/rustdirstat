use super::app::App;
use super::treemap;
use crate::color;
use crate::model::Node;
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
        Line::from(format!("{dirs} directories, {files} files, {}", human_bytes(bytes))),
        Line::from(""),
        Line::from(Span::styled("(press q to cancel)", Style::default().fg(Color::DarkGray))),
    ];

    let block = Block::default().borders(Borders::ALL).title(" rustdirstat ");
    let p = Paragraph::new(text).block(block).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(p, area);
}

pub fn draw(f: &mut Frame, app: &App) {
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

    if let Some(path) = &app.pending_delete {
        draw_confirm_popup(f, path);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let node = app.current_node();
    let title = format!(
        " {}  —  {} ({} files){} ",
        node.path.display(),
        human_bytes(node.size),
        node.file_count,
        if node.error { "  <access denied>" } else { "" }
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block, area);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let disp = app.display_children();
    let total = app.current_node().size.max(1);
    let max_sibling = disp.iter().map(|(_, n)| n.size).max().unwrap_or(1).max(1);
    let bar_width: usize = 16;

    let items: Vec<ListItem> = disp
        .iter()
        .map(|(_, node)| {
            let pct = node.size as f64 / total as f64 * 100.0;
            let filled = ((node.size as f64 / max_sibling as f64) * bar_width as f64).round() as usize;
            let filled = filled.min(bar_width);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width - filled));

            let name_style = if node.is_dir {
                Style::default().fg(color::category_color("Directory")).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color::category_color(color::category_for_ext(node.extension())))
            };
            let suffix = if node.is_dir { "/" } else if node.is_symlink { "@" } else { "" };
            let err = if node.error { " <access denied>" } else { "" };

            let line = Line::from(vec![
                Span::styled(bar, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(format!("{:>10}", human_bytes(node.size)), Style::default().fg(Color::Gray)),
                Span::raw(format!(" {:>5.1}% ", pct)),
                Span::styled(format!("{}{}", node.name, suffix), name_style),
                Span::styled(err, Style::default().fg(Color::Red)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = format!(" files — sort: {} (s to cycle) ", app.sort.label());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    if !disp.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_treemap(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" treemap (t to hide) ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let disp = app.display_children();
    let widget = TreemapWidget { items: &disp, selected: app.selected };
    f.render_widget(widget, inner);
}

struct TreemapWidget<'a> {
    items: &'a [(usize, &'a Node)],
    selected: usize,
}

impl<'a> Widget for TreemapWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.items.is_empty() || area.width == 0 || area.height == 0 {
            return;
        }
        let sizes: Vec<u64> = self.items.iter().map(|(_, n)| n.size).collect();
        let rects = treemap::layout(&sizes, area.width, area.height);

        for (i, ((_, node), r)) in self.items.iter().zip(rects.iter()).enumerate() {
            if r.w == 0 || r.h == 0 {
                continue;
            }
            let base = if node.is_dir {
                color::category_color("Directory")
            } else {
                color::category_color(color::category_for_ext(node.extension()))
            };
            // Alternate a lighter shade so adjacent same-category siblings
            // remain visually distinguishable.
            let bg = if i % 2 == 0 { lighten(base, 1.0) } else { lighten(base, 0.8) };
            let selected = i == self.selected;

            for yy in 0..r.h {
                for xx in 0..r.w {
                    let px = area.x + r.x + xx;
                    let py = area.y + r.y + yy;
                    if px >= area.x + area.width || py >= area.y + area.height {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((px, py)) {
                        cell.set_symbol(" ");
                        cell.set_bg(bg);
                    }
                }
            }

            if selected {
                draw_border(buf, area, *r, Color::White);
            }

            if r.w >= 3 && r.h >= 1 {
                let label = truncate(&node.name, r.w as usize - 1);
                let style = Style::default().fg(contrast_fg(bg)).bg(bg).add_modifier(Modifier::BOLD);
                buf.set_string(area.x + r.x, area.y + r.y, &label, style);
            }
        }
    }
}

fn draw_border(buf: &mut Buffer, area: Rect, r: treemap::Rect, c: Color) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    for xx in 0..r.w {
        let px = area.x + r.x + xx;
        for &py in &[area.y + r.y, area.y + r.y + r.h.saturating_sub(1)] {
            if px < area.x + area.width && py < area.y + area.height {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_fg(c);
                }
            }
        }
    }
    for yy in 0..r.h {
        let py = area.y + r.y + yy;
        for &px in &[area.x + r.x, area.x + r.x + r.w.saturating_sub(1)] {
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
        format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

fn draw_ext_stats(f: &mut Frame, app: &App, area: Rect) {
    let total: u64 = app.ext_stats.iter().map(|s| s.size).sum::<u64>().max(1);
    let mut spans = Vec::new();
    for stat in app.ext_stats.iter().take(8) {
        let pct = stat.size as f64 / total as f64 * 100.0;
        spans.push(Span::styled("■ ", Style::default().fg(color::category_color(&stat.category))));
        spans.push(Span::raw(format!("{} {} ({:.0}%)  ", stat.category, human_bytes(stat.size), pct)));
    }
    let block = Block::default().borders(Borders::ALL).title(" extensions (current view) ");
    let p = Paragraph::new(Line::from(spans)).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(msg) = &app.message {
        Line::from(Span::styled(msg.as_str(), Style::default().fg(Color::Yellow)))
    } else {
        Line::from(
            "↑/↓ navigate  →/Enter open  ←/Backspace up  s sort  t treemap  o open in file manager  d delete  q quit",
        )
    };
    f.render_widget(Paragraph::new(text), area);
}

fn draw_confirm_popup(f: &mut Frame, path: &std::path::Path) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" confirm delete ")
        .border_style(Style::default().fg(Color::Red));
    let text = vec![
        Line::from(format!("Permanently delete '{}'?", path.display())),
        Line::from(""),
        Line::from(Span::styled("This cannot be undone. (y/n)", Style::default().fg(Color::Red))),
    ];
    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
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
