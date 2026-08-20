// ============================================================================
// Module:       tui::ui::widgets
// Description:  The framed surfaces every TUI pane and popup is built from —
//               panel and popup blocks, the selectable list, the text prompt,
//               and the full-screen progress splash.
//
// Dependencies: ratatui; super (the shared drawing imports), super::app::App
// ============================================================================

//! The framed surfaces the rest of the TUI is built from.
//!
//! The terminal front end had no equivalent of [`crate::gui::ui::widgets`],
//! so every pane assembled its own `Block` and every scrolling view its
//! own `ListState`, click zones and all. Four list renderers carried the
//! same forty-line tail — one of them said so in a comment ("matching the
//! other three list renderers") — which meant the clamp that keeps a
//! stale `selected` from desyncing the scroll offset existed in four
//! places and had to be fixed in four places.
//!
//! Everything here is layout and framing only. Colours come from
//! [`super::theme`], as they did before.

use super::*;

/// The bordered frame every *pane* uses: the file list, the treemap, the
/// extension legend. `focused` picks the border colour.
pub(super) fn panel_block(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(focused))
        .title(title.into())
}

/// The accent-bordered frame every *popup* uses — search, move,
/// properties, tools, help.
pub(super) fn popup_block(title: impl Into<std::borrow::Cow<'static, str>>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Span::styled(title, theme::title_bar()))
}

/// The danger-bordered frame the two destructive confirmations use.
pub(super) fn danger_block(title: impl Into<std::borrow::Cow<'static, str>>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::DANGER))
        .title(Span::styled(title, theme::danger_title_bar()))
}

/// Puts a popup on screen: drop shadow, clear what is underneath, draw
/// the frame, hand back the area inside it.
///
/// The three steps have to happen in that order and always together —
/// every popup opened with the same six lines, and a popup that forgets
/// the `Clear` renders on top of whatever the main view drew.
pub(super) fn open_popup(f: &mut Frame, area: Rect, block: Block<'static>) -> Rect {
    shadow(f, area);
    f.render_widget(Clear, area);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

/// A one-cell drop shadow down and right of `area`, drawn only when it
/// fits on screen — a shadow clipped against the edge reads as a stray
/// dark line rather than depth.
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

/// A titled, bordered, scrolling list that fills `area` — the shape of
/// every main-pane view the TUI has.
///
/// Registers two kinds of click zone: one across the title bar carrying
/// `title_action` (so clicking a pane's heading toggles or cycles it),
/// and one per visible row carrying [`Action::SelectRow`]. Row zones are
/// derived from the list's own scroll offset after rendering, so they
/// follow the rows they cover rather than being computed a second way.
///
/// `app.selected` is clamped to the row count here rather than trusted.
/// It is normally kept in range by whatever action changed it, but a
/// stale value must not be able to desync the drawn selection from the
/// number of rows actually present.
pub(super) fn pane_list(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    items: Vec<ListItem<'static>>,
    title: impl Into<std::borrow::Cow<'static, str>>,
    title_action: Action,
) {
    let count = items.len();
    let list = List::new(items)
        .block(panel_block(
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
            true,
        ))
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
        action: title_action,
    });

    // After rendering, so `state.offset()` is the offset the rows were
    // actually drawn at rather than the one they had going in.
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

/// A proportional bar built from full and empty blocks, `width` cells
/// wide, showing `value` against `max`.
///
/// The file list and the biggest-files view each had their own copy of
/// the same three lines, including the rounding — and a bar that rounds
/// up past its own width overruns the column beside it.
pub(super) fn size_bar(value: u64, max: u64, width: usize, fill: Color) -> Vec<Span<'static>> {
    let max = max.max(1);
    let filled = (((value as f64 / max as f64) * width as f64).round() as usize).min(width);
    vec![
        Span::styled("█".repeat(filled), Style::default().fg(fill)),
        Span::styled(
            "░".repeat(width - filled),
            Style::default().fg(theme::PANEL_BORDER),
        ),
    ]
}

/// The body of a destructive confirmation: the question, a blank line,
/// then the consequence in the danger colour.
///
/// Both callers pre-wrap their text by hand, at the width `Paragraph`
/// will use, because the button row underneath is positioned from
/// `text.len()` — a hardcoded line offset drifts out of sync with where
/// the buttons actually render as soon as anything wraps, and then
/// clicking what looks like Yes hits nothing.
pub(super) fn confirm_body(question: &[String], consequence: &[String]) -> Vec<Line<'static>> {
    let mut text: Vec<Line> = question.iter().map(|l| Line::from(l.clone())).collect();
    text.push(Line::from(""));
    text.extend(
        consequence
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::DANGER)))),
    );
    text
}

/// A one-line text-entry popup: the typed value with a cursor, a blank
/// line, then a wrapped hint.
///
/// Sized to the hint it is given rather than to a screen percentage, so
/// a narrow terminal that wraps the hint onto three lines gets a box
/// three lines taller instead of silently clipping it.
pub(super) fn text_prompt(f: &mut Frame, title: String, value: &str, hint: &str) {
    /// The `> ` line plus the blank one under it.
    const FIXED_ROWS: u16 = 2;
    /// Width of the box, as a percentage of the terminal.
    const WIDTH_PCT: u16 = 60;

    let full_area = f.area();
    let inner_w = (full_area.width as u32 * WIDTH_PCT as u32 / 100).saturating_sub(2) as usize;
    let hint_lines = wrap_text(hint, inner_w);
    let area = centered_rect_for_lines(WIDTH_PCT, FIXED_ROWS + hint_lines.len() as u16, full_area);
    let inner = open_popup(f, area, popup_block(title));

    let mut text = vec![
        Line::from(vec![
            Span::raw("> "),
            Span::raw(value.to_owned()),
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

/// The full-screen centred panel shown while a long job runs — the
/// initial scan and the duplicate hash both use it.
///
/// `status` is the one line that differs between them beyond the
/// headline; everything else, down to the cancel hint, was written out
/// twice.
pub(super) fn progress_splash(f: &mut Frame, headline: String, status: String) {
    let text = vec![
        Line::from(Span::styled(
            "rustdirstat",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(headline),
        Line::from(status),
        Line::from(""),
        Line::from(Span::styled(
            "(press q to cancel)",
            Style::default().fg(theme::MUTED),
        )),
    ];
    let p = Paragraph::new(text)
        .block(panel_block(Line::from(" rustdirstat "), true))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(p, f.area());
}
