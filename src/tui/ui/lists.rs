// ============================================================================
// Module:       tui::ui::lists
// Description:  The scrolling lists that can occupy the main pane: directory
//               listing, largest files, search results, and duplicate groups.
//
// Dependencies: ratatui; super (the shared drawing imports), super::app::App
// ============================================================================

//! The scrolling lists that can occupy the main pane: the directory
//! listing, largest files, search results, and duplicate groups.

use super::*;

/// Cells given to the proportional size bar in the file list and the
/// biggest-files view. One value, because the two sit in the same pane
/// and a reader flipping between them should see the same column.
const BAR_WIDTH: usize = 10;

/// The recursive subtree search results — independent of the normal
/// directory browser, listing every match found anywhere below the
/// current directory (not just its direct children).
pub(super) fn draw_search_results(f: &mut Frame, app: &mut App, area: Rect) {
    let base = app.path_indices.clone();
    let base_path = app.current_path();
    let phys = app.use_physical;
    let show_details = app.detailed;

    let items: Vec<ListItem> = app
        .search
        .results
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

    let mut title = format!(
        " Search: \"{}\" — {} matches",
        app.search.query,
        app.search.results.len()
    );
    if app.search.truncated {
        title.push_str(" (truncated)");
    }
    title.push_str(" — S to close ");
    pane_list(f, app, area, items, title, Action::StartSubtreeSearch);

    if let Some(err) = &app.search.error {
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
}

pub(super) fn draw_duplicates(f: &mut Frame, app: &mut App, area: Rect) {
    let root_path = app.tree.root_path.clone();
    let mut group_num = 0usize;

    let items: Vec<ListItem> = app
        .duplicates
        .rows
        .iter()
        .map(|row| match row {
            DupRow::Header {
                size,
                count,
                wasted,
            } => {
                group_num += 1;
                ListItem::new(Line::from(Span::styled(
                    format!(
                        "Group {group_num} — {count} × {}  ({} wasted)",
                        human_bytes(*size),
                        human_bytes(*wasted)
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

    let mut title = if app.duplicates.group_count == 0 {
        " No duplicate files found".to_string()
    } else {
        format!(
            " Duplicates: {} groups, {} wasted",
            thousands(app.duplicates.group_count as u64),
            human_bytes(app.duplicates.total_wasted)
        )
    };
    if app.duplicates.truncated {
        title.push_str(" (showing largest groups)");
    }
    if app.duplicates.skipped > 0 {
        title.push_str(&format!(
            " ({} files not checked — limit reached)",
            thousands(app.duplicates.skipped as u64)
        ));
    }
    if app.duplicates.read_failures > 0 {
        title.push_str(&format!(
            " ({} files could not be read)",
            thousands(app.duplicates.read_failures as u64)
        ));
    }
    title.push_str(" — u to close ");
    pane_list(f, app, area, items, title, Action::ToggleDuplicates);
}

pub(super) fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let disp = app.display_children();
    let phys = app.use_physical;
    let total = app.current_node().effective_size(phys).max(1);
    let max_sibling = disp
        .iter()
        .map(|(_, n)| n.effective_size(phys))
        .max()
        .unwrap_or(1)
        .max(1);
    let show_details = app.detailed;

    let items: Vec<ListItem> = disp
        .iter()
        .map(|(_, node)| {
            let shown_size = node.effective_size(phys);
            let pct = shown_size as f64 / total as f64 * 100.0;

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

            let mut spans = size_bar(shown_size, max_sibling, BAR_WIDTH, bar_color);
            spans.extend([
                Span::raw("  "),
                Span::styled(
                    format!("{:>9}", human_bytes(shown_size)),
                    Style::default().fg(theme::MUTED),
                ),
                Span::raw(format!(" {:>5.1}%  ", pct)),
                Span::styled(icon, Style::default().fg(name_color)),
                Span::styled(
                    format!("{}{}", node.name.to_string_lossy(), suffix),
                    name_style,
                ),
                Span::styled(err, Style::default().fg(theme::DANGER)),
                Span::styled(warn, Style::default().fg(theme::WARNING)),
            ]);
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
    pane_list(f, app, area, items, title, Action::CycleSort);
}

/// The "biggest files anywhere in this subtree" flat view — independent of
/// the normal directory-by-directory browser, for spotting what's actually
/// worth deleting without navigating.
pub(super) fn draw_top_files(f: &mut Frame, app: &mut App, area: Rect) {
    let base = app.path_indices.clone();
    let base_path = app.current_path();
    let phys = app.use_physical;
    let max_size = app
        .top_files_cache
        .first()
        .map(|t| if phys { t.physical_size } else { t.size })
        .unwrap_or(1)
        .max(1);
    let show_details = app.detailed;

    let items: Vec<ListItem> = app
        .top_files_cache
        .iter()
        .map(|tf| {
            let shown_size = if phys { tf.physical_size } else { tf.size };
            let muted = app
                .highlighted_category
                .is_some_and(|h| Some(h) != tf.category);
            let color = if muted { theme::MUTED } else { theme::ACCENT };
            let name_color = if muted { theme::MUTED } else { Color::Reset };

            let mut full_idx = base.clone();
            full_idx.extend(&tf.index_path);
            let full_path = app.tree.path_for(&full_idx);
            let rel = full_path.strip_prefix(&base_path).unwrap_or(&full_path);

            let mut spans = size_bar(shown_size, max_size, BAR_WIDTH, color);
            spans.extend([
                Span::raw("  "),
                Span::styled(
                    format!("{:>9}", human_bytes(shown_size)),
                    Style::default().fg(theme::MUTED),
                ),
                Span::raw("  "),
                Span::styled(rel.display().to_string(), Style::default().fg(name_color)),
            ]);
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
    pane_list(f, app, area, items, title, Action::ToggleTopFiles);
}
