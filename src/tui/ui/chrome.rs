// ============================================================================
// Module:       tui::ui::chrome
// Description:  The frame around the main pane: header, footer, and the
//               extension legend along the bottom.
//
// Dependencies: ratatui; super (the shared drawing imports)
// ============================================================================

//! The frame around the main pane — header, footer, and the extension
//! legend along the bottom.

use super::*;

pub(super) fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let node = app.current_node();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(true));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut filter_suffix = if app.filter_mode {
        format!("   search: {}▌", app.filter)
    } else if !app.filter.is_empty() {
        format!("   filtered: \"{}\"", app.filter)
    } else {
        String::new()
    };
    let size_note = if app.use_physical { " (physical)" } else { "" };
    let stats_core = format!(
        "   ·   {}{}, {} files{}",
        human_bytes(node.effective_size(app.use_physical)),
        size_note,
        thousands(node.file_count),
        if node.error { "   <access denied>" } else { "" },
    );
    let mut free_space_extra = if app.path_indices.is_empty() && app.tree.is_volume_root() {
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
    //
    // But the fixed parts themselves aren't bounded — a long typed filter,
    // or the free-space/warning text together, can add up to more than the
    // whole terminal is wide before the path even enters the picture. Just
    // flooring `available_for_path` in that case would silently push the
    // warning off the right edge again, the exact failure this line was
    // rewritten to prevent. So the least essential fixed segments are
    // dropped first — free space (a nice-to-have shown only at a volume
    // root), then the filter/search suffix — before the path is ever let
    // to starve, and the warning (the highest-priority segment) is never
    // dropped.
    // Measured in terminal columns, not characters — `filt` embeds
    // whatever the user typed into the filter/search box, which can
    // legitimately contain wide (e.g. CJK) characters that render as 2
    // columns each.
    let fixed_len = |free: &str, filt: &str| {
        1 + stats_core.width() + filt.width() + free.width() + warning_extra.width()
    };
    if fixed_len(&free_space_extra, &filter_suffix) > inner.width as usize {
        free_space_extra.clear();
    }
    if fixed_len(&free_space_extra, &filter_suffix) > inner.width as usize {
        filter_suffix.clear();
    }
    let reserved = fixed_len(&free_space_extra, &filter_suffix);
    let available_for_path = (inner.width as usize).saturating_sub(reserved);
    let full_path = app.current_path().display().to_string();
    let path_display = truncate_middle(&full_path, available_for_path);

    let bar = theme::title_bar();
    let mut spans = vec![Span::styled(
        format!(" {path_display}{stats_core}{filter_suffix}"),
        bar,
    )];
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

pub(super) struct ExtLegendEntry {
    row: u16,
    col: u16,
    width: u16,
    text: String,
    category: Category,
}

/// Packs the extension-category legend into rows of at most `width`
/// columns. Shared by the vertical-layout pass (which needs only the
/// resulting row count, to size the legend panel *before* anything is
/// drawn into it) and `draw_ext_stats` itself (which needs the exact
/// per-entry row/col to place click zones) — computing the row count
/// one way and the actual positions another way is exactly how the
/// panel ended up hard-capped at a height too short for its own worst
/// case (9 categories can need 3 rows at a normal 80-column terminal,
/// the panel was fixed at 2) with click zones for the clipped row still
/// silently registered against the border underneath it.
pub(super) fn pack_ext_legend(ext_stats: &[ExtStat], width: u16) -> Vec<ExtLegendEntry> {
    let total: u64 = ext_stats.iter().map(|s| s.size).sum::<u64>().max(1);
    let mut entries = Vec::new();
    let mut col = 0u16;
    let mut row = 0u16;
    for (i, stat) in ext_stats.iter().take(Category::COUNT).enumerate() {
        let pct = stat.size as f64 / total as f64 * 100.0;
        let text = format!("{} ■ {}  {:.1}%   ", i + 1, stat.category.label(), pct);
        let w = text.chars().count() as u16;
        if col + w > width && col > 0 {
            col = 0;
            row += 1;
        }
        entries.push(ExtLegendEntry {
            row,
            col,
            width: w,
            text,
            category: stat.category,
        });
        col += w;
    }
    entries
}

/// How many rows `pack_ext_legend` will need for the legend panel —
/// called before the panel's own `Rect` exists, so the vertical layout
/// can size it to fit rather than clipping a fixed guess.
pub(super) fn ext_legend_row_count(ext_stats: &[ExtStat], width: u16) -> u16 {
    pack_ext_legend(ext_stats, width)
        .last()
        .map_or(0, |e| e.row + 1)
}

pub(super) fn draw_ext_stats(f: &mut Frame, app: &mut App, area: Rect) {
    let inner_width = area.width.saturating_sub(2);
    let entries = pack_ext_legend(&app.ext_stats, inner_width);

    let mut lines: Vec<Line> = Vec::new();
    let mut row_spans: Vec<Span> = Vec::new();
    let mut current_row = 0u16;
    for entry in &entries {
        if entry.row != current_row {
            lines.push(Line::from(std::mem::take(&mut row_spans)));
            current_row = entry.row;
        }
        let color = dim_unless_matching(
            entry.category.color(),
            app.highlighted_category,
            Some(entry.category),
        );
        row_spans.push(Span::styled(entry.text.clone(), Style::default().fg(color)));
        let y = area.y + 1 + entry.row;
        // Defense in depth: the panel is sized by `ext_legend_row_count`
        // to fit every entry, but layout constraints can still get
        // squeezed smaller than requested on a very short terminal — a
        // click zone drawn past the panel's actual bounds would be
        // clickable on whatever's rendered underneath it instead.
        if y < area.y + area.height.saturating_sub(1) {
            app.click_zones.push(ClickZone {
                x: area.x + 1 + entry.col,
                y,
                w: entry.width,
                h: 1,
                action: Action::ToggleHighlight(entry.category),
            });
        }
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

pub(super) fn draw_footer(f: &mut Frame, app: &mut App, area: Rect) {
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
    let full_buttons: [(&str, Action); 3] = [
        (" Enter  Open ", Action::OpenSelected),
        (" Backspace  Up ", Action::Back),
        (" d  Delete ", Action::RequestDelete),
    ];
    let compact_buttons: [(&str, Action); 2] = [
        (" Enter  Open ", Action::OpenSelected),
        (" d  Delete ", Action::RequestDelete),
    ];
    let quit_label = " q  Quit ";
    let help_label = "  ?  more shortcuts ";
    let fixed_tail_width = quit_label.chars().count() as u16 + help_label.chars().count() as u16;
    let full_width: u16 = full_buttons
        .iter()
        .map(|(l, _)| l.chars().count() as u16 + 2)
        .sum::<u16>()
        + fixed_tail_width;
    // On a narrow terminal, "Backspace  Up" is the button to drop first
    // — it's the one redundant affordance here (clicking the header does
    // the same thing) — rather than silently clipping the row from the
    // right, which previously lost the Quit button and the only
    // on-screen pointer to the full shortcut list first.
    let buttons: &[(&str, Action)] = if area.width >= full_width {
        &full_buttons
    } else {
        &compact_buttons
    };

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
