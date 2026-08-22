// ============================================================================
// Module:       tui::ui::popups
// Description:  Every popup over the main view — prompts, confirmations,
//               properties, tools, help — and the centring helpers they share.
//
// Dependencies: ratatui; super (the shared drawing imports)
// ============================================================================

//! Every popup the TUI puts over the main view: prompts,
//! confirmations, properties, the tool list, and help — plus the
//! centring helpers they share.
//!
//! The frames themselves come from [`super::widgets`]: `open_popup`
//! draws the shadow, clears what is underneath, and returns the area
//! inside the border, so a popup here is its contents and nothing else.

use super::*;

pub(super) fn draw_search_prompt(f: &mut Frame, app: &mut App) {
    text_prompt(
        f,
        " Search this subtree (Esc to cancel) ".to_string(),
        &app.search.query,
        "Enter to search, Esc to cancel. * and ? are wildcards; prefix with re: for a regex.",
    );
}

pub(super) fn draw_move_prompt(f: &mut Frame, app: &mut App) {
    let name = app
        .display_children()
        .get(app.selected)
        .map(|(_, n)| n.name.to_string_lossy().to_string())
        .unwrap_or_default();
    text_prompt(
        f,
        format!(" Move '{name}' to (Esc to cancel) "),
        &app.move_to.destination,
        "Enter a destination folder (or full path) and press Enter. Esc to cancel.",
    );
}

pub(super) fn draw_properties_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 40, f.area());
    let inner = open_popup(
        f,
        area,
        popup_block(format!(" {} ", tr("tui.popup.properties"))),
    );

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
    let inner = open_popup(
        f,
        area,
        popup_block(format!(" {} ", tr("tui.popup.wintools"))),
    );

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

/// The user's own commands, and what each one runs.
///
/// The list shows the *template* — the command before substitution —
/// because that is what identifies a cleanup. The confirmation below
/// shows the resolved argv instead, which is what identifies the thing
/// about to happen. See `docs/CLEANUPS_THREAT_MODEL.md`.
pub(super) fn draw_cleanups_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(70, 60, f.area());
    let inner = open_popup(
        f,
        area,
        popup_block(format!(" {} ", tr("tui.popup.cleanups"))),
    );

    // Pushed before the per-entry zones, for the reason spelled out in
    // `draw_wintools_popup`: zones are searched last-pushed-first.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::ToggleCleanups,
    });

    if app.cleanups.list.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                tr("tui.cleanups.empty"),
                Style::default().fg(Color::Reset),
            )),
            Line::from(""),
            Line::from(Span::styled(
                tr("tui.cleanups.empty.body"),
                Style::default().fg(theme::MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                tr("tui.cleanups.placeholders"),
                Style::default().fg(theme::MUTED),
            )),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    }

    // Zones built in the same pass as the lines, tracking the real row
    // each entry lands on — see `draw_wintools_popup`, which this is the
    // twin of.
    let mut lines: Vec<Line> = Vec::new();
    let mut row = 0u16;
    for (index, cleanup) in app.cleanups.list.iter().enumerate() {
        let selected = index == app.cleanups.selected;
        let marker = if selected { "> " } else { "  " };
        let style = if selected {
            theme::selection()
        } else {
            Style::default().fg(Color::Reset)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{}", cleanup.name),
            style,
        )));
        if row < inner.height {
            app.click_zones.push(ClickZone {
                x: inner.x,
                y: inner.y + row,
                w: inner.width,
                h: 1,
                action: Action::SelectCleanup(index),
            });
        }
        row += 1;
        if selected {
            let template = cleanup
                .args
                .iter()
                .fold(cleanup.program.clone(), |mut acc, arg| {
                    acc.push(' ');
                    acc.push_str(arg);
                    acc
                });
            for line in wrap_text(&format!("    {template}"), inner.width as usize) {
                lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(theme::MUTED),
                )));
                row += 1;
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// "This is the command, exactly as it will run."
///
/// Reads the resolved argv off the pending cleanup rather than
/// re-resolving the selected one: the selection can move between the
/// question and the answer, and what is confirmed has to be what runs.
pub(super) fn draw_cleanup_confirm_popup(f: &mut Frame, app: &mut App) {
    let Some(pending) = &app.cleanups.pending else {
        return;
    };
    let (name, preview) = (pending.name.clone(), pending.command.preview());
    let area = centered_rect(64, 30, f.area());
    let inner = open_popup(
        f,
        area,
        danger_block(format!(" {} ", tr("tui.popup.confirm"))),
    );

    // Wrapped by hand at the width `Paragraph` will use, so the button
    // row's y can be counted rather than guessed — same reason as
    // `draw_wintool_confirm_popup`.
    let inner_w = inner.width as usize;
    let question_lines = wrap_text(
        &tr_with("tui.cleanups.question", &[("name", &name)]),
        inner_w,
    );
    let mut desc_lines = wrap_text(&tr("tui.cleanups.consequence"), inner_w);
    desc_lines.push(String::new());
    desc_lines.extend(wrap_text(&preview, inner_w));

    let mut text = confirm_body(&question_lines, &desc_lines);
    text.push(Line::from(""));
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelCleanup,
    });
    let row_y = inner.y + text.len() as u16;
    let buttons = [
        ConfirmButton {
            label: " [ Y ]es ",
            style: theme::filled_button(theme::DANGER),
            action: Action::ConfirmCleanup,
        },
        cancel_button(Action::CancelCleanup),
    ];
    let row = button_row(app, &buttons, inner.x, row_y);
    text.push(row);
    f.render_widget(Paragraph::new(text), inner);
}

pub(super) fn draw_wintool_confirm_popup(f: &mut Frame, app: &mut App, idx: usize) {
    let Some(tool) = crate::wintools::TOOLS.get(idx) else {
        return;
    };
    let area = centered_rect(60, 24, f.area());
    let inner = open_popup(
        f,
        area,
        danger_block(format!(" {} ", tr("tui.popup.confirm"))),
    );

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

    let mut text = confirm_body(&question_lines, &desc_lines);
    text.push(Line::from(""));
    // Registered before the row is drawn so the whole-popup cancel zone
    // sits underneath the per-button ones.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelWinTool,
    });
    let row_y = inner.y + text.len() as u16;
    let buttons = [
        ConfirmButton {
            label: " [ Y ]es ",
            style: theme::filled_button(theme::DANGER),
            action: Action::ConfirmWinTool,
        },
        cancel_button(Action::CancelWinTool),
    ];
    let row = button_row(app, &buttons, inner.x, row_y);
    text.push(row);
    f.render_widget(Paragraph::new(text), inner);
}

/// One button in a confirmation popup's button row.
pub(super) struct ConfirmButton {
    /// The drawn text, padding included. The click zone is exactly this
    /// wide, because both come from this one string.
    pub label: &'static str,
    pub style: Style,
    pub action: Action,
}

/// Draws a row of buttons and registers a click zone for each.
///
/// Each popup used to write these out twice — once as styled spans, once
/// as click zones placed with hand-written offsets (`inner.x + 12`,
/// `w: 9`, `next_x += 14`) that had to match the label widths by eye.
/// Change a label and the zone slid off it silently: the button still
/// drew, and clicking it did nothing, or did its neighbour's job. Widths
/// now come from the labels themselves, so the two cannot disagree.
///
/// `y` is the row the returned line will occupy. Callers pass
/// `inner.y + text.len() as u16` *before* pushing it, rather than adding
/// up section line counts by hand — that sum was the other half of the
/// same arithmetic, re-derived in every popup that had a button row.
pub(super) fn button_row(
    app: &mut App,
    buttons: &[ConfirmButton],
    x: u16,
    y: u16,
) -> Line<'static> {
    /// Blank columns between buttons.
    const GAP: u16 = 3;

    let mut spans = Vec::new();
    let mut cursor = x;
    for (index, button) in buttons.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" ".repeat(GAP as usize)));
            cursor += GAP;
        }
        let width = UnicodeWidthStr::width(button.label) as u16;
        spans.push(Span::styled(button.label, button.style));
        app.click_zones.push(ClickZone {
            x: cursor,
            y,
            w: width,
            h: 1,
            action: button.action.clone(),
        });
        cursor += width;
    }
    Line::from(spans)
}

/// The neutral "No" button both confirmations end with.
pub(super) fn cancel_button(action: Action) -> ConfirmButton {
    ConfirmButton {
        label: " [ N ]o ",
        style: Style::default()
            .fg(theme::BUTTON_NEUTRAL_FG)
            .bg(theme::BUTTON_NEUTRAL_BG)
            .add_modifier(Modifier::BOLD),
        action,
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
    let title = if permanent {
        " Permanently Delete "
    } else {
        " Move to Trash "
    };
    let inner = open_popup(f, area, danger_block(title));

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

    let mut text = confirm_body(&question_lines, &desc_lines);
    text.extend(empty_lines.iter().map(|l| Line::from(l.clone())));
    text.push(Line::from(""));
    // The whole-popup cancel zone goes down first, so the per-button
    // zones registered below sit on top of it.
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: area.height,
        action: Action::CancelDelete,
    });

    let mut buttons = vec![ConfirmButton {
        label: " [ Y ]es ",
        style: theme::filled_button(theme::DANGER),
        action: Action::ConfirmDelete,
    }];
    if is_dir {
        buttons.push(ConfirmButton {
            label: " [ E ]mpty ",
            style: theme::filled_button(theme::WARNING),
            action: Action::ConfirmEmpty,
        });
    }
    buttons.push(cancel_button(Action::CancelDelete));

    // Measured from the lines already in `text`, so the row and its
    // click zones cannot land on different lines — the hand-summed
    // version had to be updated whenever a section was added, and the
    // "Empty" option is exactly such a section.
    let row_y = inner.y + text.len() as u16;
    let row = button_row(app, &buttons, inner.x, row_y);
    text.push(row);
    f.render_widget(Paragraph::new(text), inner);
}

pub(super) fn draw_help_popup(f: &mut Frame, app: &mut App) {
    let area = centered_rect(70, 80, f.area());
    let inner = open_popup(f, area, popup_block(format!(" {} ", tr("tui.help.title"))));

    // Keys on the left are never translated — they are the keys — and
    // the catalogue holds only what each one does.
    let rows: [(&str, String); 31] = [
        ("↑/↓, k/j", tr("tui.help.move")),
        ("→/l/Enter", tr("tui.help.open_dir")),
        ("←/h/Backspace", tr("tui.help.up_dir")),
        ("s", tr("tui.help.sort")),
        ("m", tr("tui.help.details")),
        ("p", tr("tui.help.physical")),
        ("t", tr("tui.help.treemap")),
        ("[ / ]", tr("tui.help.resize")),
        ("f", tr("tui.help.top_files")),
        ("/", tr("tui.help.filter")),
        ("S", tr("tui.help.search")),
        ("u", tr("tui.help.duplicates")),
        ("1-9", tr("tui.help.highlight")),
        ("0", tr("tui.help.clear_highlight")),
        ("o", tr("tui.help.open")),
        ("O", tr("tui.help.reveal")),
        ("y", tr("tui.help.copy_path")),
        ("M", tr("tui.help.move_to")),
        ("i", tr("tui.help.properties")),
        ("T", tr("tui.help.wintools")),
        ("C", tr("tui.help.cleanups")),
        ("r", tr("tui.help.rescan")),
        ("e", tr("tui.help.report")),
        ("E", tr("tui.help.csv")),
        ("d", tr("tui.help.delete")),
        ("D", tr("tui.help.delete_permanent")),
        ("d, then e", tr("tui.help.empty")),
        ("?", tr("tui.help.toggle_help")),
        ("q, Esc", tr("tui.help.quit")),
        ("", String::new()),
        ("Mouse", tr("tui.help.mouse")),
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
                    Span::raw(desc.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Tree;
    use crate::tui::app::PendingDelete;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    /// Renders the whole TUI to an off-screen buffer and returns what
    /// each cell holds, so a test can ask what is actually on screen
    /// rather than what the drawing code intended.
    fn render(app: &mut App, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend);
        assert!(terminal.is_ok(), "the test terminal should initialise");
        let mut terminal = match terminal {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        let drawn = terminal.draw(|f| super::super::draw(f, app));
        assert!(drawn.is_ok(), "drawing should not fail");

        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer.cell((x, y)).map_or(" ", |c| c.symbol()).to_owned())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect()
    }

    fn app_with_delete_prompt(is_dir: bool) -> App {
        let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
        app.pending_delete = Some(PendingDelete {
            orig_idx: 0,
            name: std::ffi::OsString::from("doomed.txt"),
            permanent: false,
            is_dir,
        });
        app
    }

    /// Every button the delete prompt draws is clickable where it is
    /// drawn.
    ///
    /// The labels and the click zones used to be written out separately,
    /// the zones placed with offsets (`inner.x + 12`, `w: 9`,
    /// `next_x += 14`) that had to match the label widths by eye and the
    /// row found by summing section line counts by hand. Nothing checked
    /// the two agreed, so a changed label or an added section moved the
    /// button out from under its own target — the button drew, and
    /// clicking it did nothing.
    ///
    /// This reads the rendered buffer, so it fails if either half moves.
    #[test]
    fn every_delete_button_is_clickable_where_it_is_drawn() {
        for is_dir in [false, true] {
            let mut app = app_with_delete_prompt(is_dir);
            let screen = render(&mut app, 100, 30);

            let expected: &[(&str, &str)] = if is_dir {
                &[
                    ("Y ]es", "ConfirmDelete"),
                    ("E ]mpty", "ConfirmEmpty"),
                    ("N ]o", "CancelDelete"),
                ]
            } else {
                &[("Y ]es", "ConfirmDelete"), ("N ]o", "CancelDelete")]
            };

            for (label, action_name) in expected {
                // Where the label actually landed on screen.
                let mut found = None;
                for (y, row) in screen.iter().enumerate() {
                    if let Some(x) = row.find(label) {
                        found = Some((x as u16, y as u16));
                        break;
                    }
                }
                assert!(
                    found.is_some(),
                    "the {label:?} button should be drawn (is_dir={is_dir})"
                );
                let (x, y) = found.unwrap_or_default();

                // And the zone that claims to cover it.
                let zone = app
                    .click_zones
                    .iter()
                    .rev()
                    .find(|zone| format!("{:?}", zone.action) == *action_name);
                assert!(
                    zone.is_some(),
                    "there should be a click zone for {action_name} (is_dir={is_dir})"
                );
                let covers = zone.is_some_and(|zone| zone.contains(x, y));
                assert!(
                    covers,
                    "{label:?} is drawn at ({x},{y}) but its {action_name} zone does not \
                     cover that cell (is_dir={is_dir})"
                );
            }
        }
    }

    /// Every row a list pane draws can be clicked where it is drawn,
    /// and the pane heading above them carries its own action.
    ///
    /// The four list views each had their own copy of this wiring — the
    /// `ListState`, the selection clamp, the heading zone, and the
    /// per-row zone loop. They share one `pane_list` now, so this
    /// exercises all four through it: a row zone that stopped lining up
    /// with the row it covers would make clicking a file select a
    /// different one.
    #[test]
    fn every_list_pane_puts_a_click_zone_on_the_rows_it_draws() {
        /// Opens one of the four list panes on an `App`.
        type OpenPane = fn(&mut App);

        // Each of the four panes, and the heading action it advertises.
        let views: [(OpenPane, &str); 4] = [
            (|_| {}, "CycleSort"),
            (|app| app.show_top_files = true, "ToggleTopFiles"),
            (|app| app.search.visible = true, "StartSubtreeSearch"),
            (|app| app.duplicates.visible = true, "ToggleDuplicates"),
        ];

        for (set_up, heading_action) in views {
            let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
            set_up(&mut app);
            let _ = render(&mut app, 100, 30);

            let heading = app
                .click_zones
                .iter()
                .find(|zone| format!("{:?}", zone.action) == heading_action);
            assert!(
                heading.is_some(),
                "the pane heading should carry {heading_action}"
            );
            // The heading zone is one row tall and sits on the pane's own
            // top border, never over the rows below it.
            assert!(
                heading.is_some_and(|zone| zone.h == 1),
                "{heading_action} should claim the heading row only"
            );

            // No row zone may overlap the heading row, which is what
            // would let a click on a file toggle the whole view instead.
            let rows: Vec<_> = app
                .click_zones
                .iter()
                .filter(|zone| format!("{:?}", zone.action).starts_with("SelectRow"))
                .collect();
            for row in &rows {
                assert!(
                    heading.is_some_and(|h| row.y > h.y),
                    "a row zone sits on the heading row for {heading_action}"
                );
            }
        }
    }

    fn cleanup(name: &str, program: &str, args: &[&str]) -> crate::cleanups::Cleanup {
        crate::cleanups::Cleanup {
            name: name.to_string(),
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            confirm: true,
            capture_output: true,
        }
    }

    /// Picking a cleanup from the list is a click on the row it is drawn
    /// on, not on the row above or below it.
    ///
    /// The entries wrap: a selected one carries its command underneath,
    /// on a variable number of rows. Deriving the zones from a second
    /// pass over the list — one row per entry — drifts the moment any
    /// entry occupies more than one, which the selected one always does.
    #[test]
    fn every_cleanup_row_is_clickable_where_it_is_drawn() {
        let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
        app.cleanups.visible = true;
        app.cleanups.list = vec![
            cleanup("Empty the recycle bin", "cleanmgr", &["/sagerun:1"]),
            cleanup("Open a shell here", "cmd", &["/c", "start", "%d"]),
            cleanup("Compress it", "compact", &["/c", "%p"]),
        ];
        app.cleanups.selected = 0;
        let screen = render(&mut app, 100, 30);

        for (index, entry) in app.cleanups.list.clone().iter().enumerate() {
            let row = screen
                .iter()
                .position(|line| line.contains(&entry.name))
                .map(|y| y as u16);
            assert!(row.is_some(), "{:?} should be drawn", entry.name);
            let y = row.unwrap_or_default();
            let x = screen
                .get(y as usize)
                .and_then(|line| line.find(&entry.name))
                .unwrap_or_default() as u16;

            let action = format!("SelectCleanup({index})");
            let zone = app
                .click_zones
                .iter()
                .find(|zone| format!("{:?}", zone.action) == action);
            assert!(zone.is_some(), "there should be a zone for {action}");
            assert!(
                zone.is_some_and(|zone| zone.contains(x, y)),
                "{:?} is drawn at ({x},{y}) but its {action} zone does not cover it",
                entry.name
            );
        }
    }

    /// The confirmation shows the command that will actually run, with
    /// its placeholders already substituted.
    ///
    /// A prompt that showed the template would be asking about something
    /// nobody can check: `%p` is the whole question. And the buttons
    /// under it have to be clickable where they land, which the resolved
    /// command's length moves.
    #[test]
    fn a_cleanup_confirmation_shows_the_resolved_command_and_clickable_buttons(
    ) -> anyhow::Result<()> {
        let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
        let target = PathBuf::from("root").join("big.iso");
        let command =
            crate::cleanups::resolve(&cleanup("Compress it", "compact", &["/c", "%p"]), &target)?;
        let preview = command.preview();
        app.cleanups.pending = Some(crate::tui::app::PendingCleanup {
            name: "Compress it".to_string(),
            command,
            capture_output: true,
        });
        let screen = render(&mut app, 100, 30);

        assert!(
            screen.iter().any(|line| line.contains("big.iso")),
            "the confirmation should name the resolved target, not %p: {preview}"
        );
        assert!(
            !screen.iter().any(|line| line.contains("%p")),
            "an unsubstituted placeholder means the prompt is showing the template"
        );

        for (label, action_name) in [("Y ]es", "ConfirmCleanup"), ("N ]o", "CancelCleanup")] {
            let mut found = None;
            for (y, row) in screen.iter().enumerate() {
                if let Some(x) = row.find(label) {
                    found = Some((x as u16, y as u16));
                    break;
                }
            }
            assert!(found.is_some(), "the {label:?} button should be drawn");
            let (x, y) = found.unwrap_or_default();
            let zone = app
                .click_zones
                .iter()
                .rev()
                .find(|zone| format!("{:?}", zone.action) == action_name);
            assert!(
                zone.is_some_and(|zone| zone.contains(x, y)),
                "{label:?} is drawn at ({x},{y}) but its {action_name} zone does not cover it"
            );
        }
        Ok(())
    }

    /// The "Empty" button exists only for directories.
    #[test]
    fn a_file_gets_no_empty_button() {
        let mut app = app_with_delete_prompt(false);
        let screen = render(&mut app, 100, 30);
        assert!(
            !screen.iter().any(|row| row.contains("E ]mpty")),
            "emptying a file makes no sense, so the button should not be offered"
        );
        assert!(
            !app.click_zones
                .iter()
                .any(|zone| format!("{:?}", zone.action) == "ConfirmEmpty"),
            "and there should be no zone for it either"
        );
    }
}
