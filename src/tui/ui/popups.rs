// ============================================================================
// Module:       tui::ui::popups
// Description:  Every popup over the main view — prompts, confirmations,
//               properties, tools, help — and the centring and shadow helpers
//               they share.
//
// Dependencies: ratatui; super (the shared drawing imports)
// ============================================================================

//! Every popup the TUI puts over the main view: prompts,
//! confirmations, properties, the tool list, and help — plus the
//! centring and shadow helpers they share.

use super::*;

pub(super) fn draw_search_prompt(f: &mut Frame, app: &mut App) {
    let full_area = f.area();
    let inner_w = (full_area.width as u32 * 60 / 100).saturating_sub(2) as usize;
    let hint_lines = wrap_text(
        "Enter to search, Esc to cancel. * and ? are wildcards; prefix with re: for a regex.",
        inner_w,
    );
    let content_rows = 2 + hint_lines.len() as u16;
    let area = centered_rect_for_lines(60, content_rows, full_area);
    shadow(f, area);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(
            " Search this subtree (Esc to cancel) ",
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut text = vec![
        Line::from(vec![
            Span::raw("> "),
            Span::raw(&app.search.query),
            Span::raw("▌"),
        ]),
        Line::from(""),
    ];
    text.extend(
        hint_lines
            .into_iter()
            .map(|l| Line::from(Span::styled(l, Style::default().fg(theme::MUTED)))),
    );
    f.render_widget(Paragraph::new(text), inner);
}

pub(super) fn draw_move_prompt(f: &mut Frame, app: &mut App) {
    let full_area = f.area();
    let inner_w = (full_area.width as u32 * 60 / 100).saturating_sub(2) as usize;
    let hint_lines = wrap_text(
        "Enter a destination folder (or full path) and press Enter. Esc to cancel.",
        inner_w,
    );
    let content_rows = 2 + hint_lines.len() as u16;
    let area = centered_rect_for_lines(60, content_rows, full_area);
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
            format!(" Move '{name}' to (Esc to cancel) "),
            theme::title_bar(),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut text = vec![
        Line::from(vec![
            Span::raw("> "),
            Span::raw(&app.move_to.destination),
            Span::raw("▌"),
        ]),
        Line::from(""),
    ];
    text.extend(
        hint_lines
            .into_iter()
            .map(|l| Line::from(Span::styled(l, Style::default().fg(theme::MUTED)))),
    );
    f.render_widget(Paragraph::new(text), inner);
}

pub(super) fn draw_properties_popup(f: &mut Frame, app: &mut App) {
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

pub(super) fn draw_wintools_popup(f: &mut Frame, app: &mut App) {
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

    // Pushed before the per-tool zones below (not after) — click zones
    // are searched last-pushed-first, so this full-area background zone
    // must come first or it would shadow every per-tool zone on top of
    // it instead of only catching clicks that miss them all.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::ToggleWinTools,
    });

    let available = cfg!(windows);
    let mut lines: Vec<Line> = Vec::new();
    // Click zones are built in the same pass as the lines themselves,
    // tracking the real row each entry lands on (a name row, plus a
    // variable number of wrapped description rows when selected) —
    // computing them from a second, independent pass (as this used to)
    // drifts out of sync the moment any entry's row count differs from
    // "exactly one," which every selected entry's description does.
    let mut row = 0u16;
    for (i, tool) in crate::wintools::TOOLS.iter().enumerate() {
        let selected = i == app.wintools.selected;
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
        if row < inner.height {
            app.click_zones.push(ClickZone {
                x: inner.x,
                y: inner.y + row,
                w: inner.width,
                h: 1,
                action: Action::SelectWinTool(i),
            });
        }
        row += 1;
        if selected {
            for desc_line in wrap_text(&format!("    {}", tool.description), inner.width as usize) {
                lines.push(Line::from(Span::styled(
                    desc_line,
                    Style::default().fg(theme::MUTED),
                )));
                row += 1;
            }
        }
    }
    if !available {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "These tools call native Windows utilities and aren't available on this platform.",
            Style::default().fg(theme::MUTED),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_wintool_confirm_popup(f: &mut Frame, app: &mut App, idx: usize) {
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

    // The button row can't be a hardcoded line offset — either the
    // question or (especially) the description can wrap onto more than
    // one row depending on terminal width, and a fixed offset silently
    // drifts out of sync with where the buttons actually render, so
    // clicking what looks like Yes/No hits the wrong target (or nothing).
    // Pre-wrapping by hand, the same width `Paragraph` will use, means
    // the row math and the rendered text can never disagree.
    let inner_w = inner.width as usize;
    let question_lines = wrap_text(&format!("Run '{}'?", tool.name), inner_w);
    let desc_lines = wrap_text(tool.description, inner_w);

    let mut text: Vec<Line> = question_lines
        .iter()
        .map(|l| Line::from(l.clone()))
        .collect();
    text.push(Line::from(""));
    text.extend(
        desc_lines
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::DANGER)))),
    );
    text.push(Line::from(""));
    text.push(Line::from(vec![
        Span::styled(" [ Y ]es ", theme::filled_button(theme::DANGER)),
        Span::raw("   "),
        Span::styled(
            " [ N ]o ",
            Style::default()
                .fg(theme::BUTTON_NEUTRAL_FG)
                .bg(theme::BUTTON_NEUTRAL_BG)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(Paragraph::new(text), inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelWinTool,
    });
    let button_row = inner.y + question_lines.len() as u16 + 1 + desc_lines.len() as u16 + 1;
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

pub(super) fn shadow(f: &mut Frame, area: Rect) {
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

pub(super) fn draw_confirm_popup(
    f: &mut Frame,
    app: &mut App,
    name: &str,
    permanent: bool,
    is_dir: bool,
) {
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

    // As in draw_wintool_confirm_popup: the button row can't be a fixed
    // line offset, since the item name or the description text can wrap
    // onto more than one row depending on terminal width and how long
    // the deleted item's name is — pre-wrapping by hand keeps the click
    // zones honest about where the buttons actually render.
    let inner_w = inner.width as usize;
    let question_lines = wrap_text(&format!("Delete '{name}'?"), inner_w);
    let desc_text = if permanent {
        "This bypasses the Recycle Bin/Trash and cannot be undone."
    } else {
        "This can be undone from your OS Recycle Bin/Trash."
    };
    let desc_lines = wrap_text(desc_text, inner_w);
    let empty_lines: Vec<String> = if is_dir {
        wrap_text(
            "Or empty it — delete its contents, keep the folder.",
            inner_w,
        )
    } else {
        Vec::new()
    };

    let mut text: Vec<Line> = question_lines
        .iter()
        .map(|l| Line::from(l.clone()))
        .collect();
    text.push(Line::from(""));
    text.extend(
        desc_lines
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::DANGER)))),
    );
    text.extend(empty_lines.iter().map(|l| Line::from(l.clone())));
    text.push(Line::from(""));
    let mut buttons = vec![
        Span::styled(" [ Y ]es ", theme::filled_button(theme::DANGER)),
        Span::raw("   "),
    ];
    if is_dir {
        buttons.push(Span::styled(
            " [ E ]mpty ",
            theme::filled_button(theme::WARNING),
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
    let p = Paragraph::new(text);
    f.render_widget(p, inner);

    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelDelete,
    });
    let button_row = inner.y
        + question_lines.len() as u16
        + 1
        + desc_lines.len() as u16
        + empty_lines.len() as u16
        + 1;
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

pub(super) fn draw_help_popup(f: &mut Frame, app: &mut App) {
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

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

/// Like `centered_rect`, but sized to fit exactly `content_rows` rows of
/// content (plus the two border rows), rather than a fixed screen
/// percentage. A percentage height is a guess about content that doesn't
/// scale with it — too small a guess silently clips text (`Wrap{trim:
/// true}` drops whatever doesn't fit, with no visible sign anything was
/// cut), too large leaves a mostly-empty box. Used for popups whose
/// content length is known ahead of render time via `wrap_text`.
pub(super) fn centered_rect_for_lines(percent_x: u16, content_rows: u16, r: Rect) -> Rect {
    let height = (content_rows + 2).min(r.height);
    let x_rect = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(r)[1];
    let y = r.y + r.height.saturating_sub(height) / 2;
    Rect {
        x: x_rect.x,
        y,
        width: x_rect.width,
        height,
    }
}
