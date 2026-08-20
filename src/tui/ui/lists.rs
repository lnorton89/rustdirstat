//! The scrolling lists that can occupy the main pane: the directory
//! listing, largest files, search results, and duplicate groups.

use super::*;

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

    let count = app.search.results.len();
    let mut title = format!(" Search: \"{}\" — {} matches", app.search.query, count);
    if app.search.truncated {
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

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::StartSubtreeSearch,
    });

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

pub(super) fn draw_duplicates(f: &mut Frame, app: &mut App, area: Rect) {
    let root_path = app.tree.root_path.clone();
    let mut group_num = 0usize;

    let items: Vec<ListItem> = app
        .duplicates
        .rows
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

    let count = app.duplicates.rows.len();
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

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::ToggleDuplicates,
    });

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

pub(super) fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
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
        // Defense in depth, matching the other three list renderers
        // (search/duplicates/top-files): `app.selected` is normally kept
        // in range by whatever action changed it, but clamping here means
        // a stale or out-of-range value can never desync the rendered
        // selection/scroll offset from the actual number of rows.
        state.select(Some(app.selected.min(disp_len - 1)));
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

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::ToggleTopFiles,
    });

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
