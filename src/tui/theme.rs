// ============================================================================
// Module:       tui::theme
// Description:  The terminal palette and the ratatui styles built from it, so
//               the app reads as one designed surface rather than as a
//               terminal.
//
// Dependencies: ratatui::style, ratatui::widgets::BorderType
// ============================================================================

//! A small, consistent color/style palette so the app reads as one designed
//! surface — a title bar, a status bar, a selection color, panel chrome —
//! instead of ad hoc per-widget colors, which is what makes a TUI look and
//! feel like "a terminal" rather than an application.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

pub(super) const ACCENT: Color = Color::Rgb(66, 133, 219);
pub(super) const ACCENT_TEXT: Color = Color::Rgb(240, 244, 250);
pub(super) const PANEL_BORDER: Color = Color::Rgb(88, 96, 112);
pub(super) const PANEL_BORDER_ACCENT: Color = Color::Rgb(74, 120, 196);
pub(super) const SELECTION_BG: Color = Color::Rgb(43, 87, 148);
pub(super) const MUTED: Color = Color::Rgb(142, 150, 165);
pub(super) const DANGER: Color = Color::Rgb(217, 83, 79);
pub(super) const DANGER_BG: Color = Color::Rgb(58, 30, 30);
pub(super) const WARNING: Color = Color::Rgb(230, 175, 46);
pub(super) const SHADOW: Color = Color::Rgb(18, 18, 22);
/// The one "de-emphasize this" gray, used anywhere something needs to
/// visually recede without going fully invisible (a treemap tile outside
/// the active category highlight, an unselected legend entry, ...) — a
/// single shared constant instead of several ad hoc grays scattered
/// across call sites that don't agree with each other or with `MUTED`.
pub(super) const DIM: Color = Color::Rgb(55, 58, 66);
pub(super) const BUTTON_NEUTRAL_BG: Color = Color::Rgb(160, 165, 175);
pub(super) const BUTTON_NEUTRAL_FG: Color = Color::Rgb(20, 20, 24);
pub(super) const SELECTED_BORDER: Color = Color::Rgb(240, 244, 250);

pub(super) fn border_type() -> BorderType {
    BorderType::Rounded
}

/// A GUI-style title bar: filled background, bold light text.
pub(super) fn title_bar() -> Style {
    Style::default()
        .bg(ACCENT)
        .fg(ACCENT_TEXT)
        .add_modifier(Modifier::BOLD)
}

/// Like `title_bar`, but for dialogs that need to visually outrank a
/// routine one — permanent delete, destructive system tools. Filled with
/// `DANGER` rather than `ACCENT` so the highest-stakes confirmations in
/// the app are the most visually assertive ones, not the least (a plain,
/// unstyled title was the previous behavior for exactly these two
/// dialogs, which read backwards next to every other popup's filled
/// title bar).
pub(super) fn danger_title_bar() -> Style {
    Style::default()
        .bg(DANGER)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn status_bar() -> Style {
    Style::default()
        .bg(Color::Rgb(30, 33, 40))
        .fg(Color::Rgb(200, 205, 215))
}

pub(super) fn button() -> Style {
    Style::default()
        .bg(Color::Rgb(46, 50, 60))
        .fg(Color::Rgb(210, 215, 225))
}

pub(super) fn selection() -> Style {
    Style::default()
        .bg(SELECTION_BG)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn panel_border(focused: bool) -> Style {
    Style::default().fg(if focused {
        PANEL_BORDER_ACCENT
    } else {
        PANEL_BORDER
    })
}

/// A filled, bold confirm-popup button in a given accent color (`DANGER`
/// for "Yes" on a delete/destructive confirm, `WARNING` for "Empty") —
/// centralized alongside `BUTTON_NEUTRAL_BG`/`FG` so all three buttons in
/// these popups come from one place instead of three separate inline
/// `Style` literals that could quietly drift apart (e.g. if `DANGER` is
/// ever retuned darker, nothing would catch the fixed `Color::Black` text
/// on it stopping being readable).
pub(super) fn filled_button(bg: Color) -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}
