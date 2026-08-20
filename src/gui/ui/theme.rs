// ============================================================================
// Module:       gui::ui::theme
// Description:  The active palette, the egui style derived from it, the shared
//               spacing scale, and the colour maths the panes need.
//
// Dependencies: eframe::egui; super::themes (the palette catalog)
// ============================================================================

//! The active palette, the egui style derived from it, and the color
//! math the treemap and tables need.
//!
//! The colors themselves live in [`super::themes`]; this module is about
//! *which* palette is in force and how it reaches the drawing code.
//!
//! Every frame, [`apply_style`] installs the app's chosen [`Palette`]
//! into a thread-local before anything paints, and the drawing code
//! reads it back through [`palette`]. That is ambient state, which this
//! codebase otherwise avoids — the justification is that it is exactly
//! how `ctx.style()` already works, and the alternative is threading a
//! `&Palette` argument through roughly a hundred call sites that have no
//! other reason to know a theme exists. The property that matters —
//! that a palette change cannot leave half the window on the old one —
//! is preserved either way, because there is still exactly one palette
//! and it is set once per frame.
//!
//! Most of the palette also goes into egui's own `Visuals`, so built-in
//! widgets follow the theme without any call site being involved at all.
//! Prefer letting a color arrive that way over reading it from
//! [`palette`] by hand.

use eframe::egui::{self, Color32, Frame, Margin, Stroke, TextStyle, Vec2};
use std::cell::Cell;

pub(crate) use super::themes::Palette;

/// The window's spacing scale.
///
/// Every margin, gap, and inset in the GUI is one of these four values.
/// The point is not the numbers but that there are only four of them: the
/// panels used to pad by 10 while the toolbar padded by 12 and the menu
/// bar by 8, so nothing down the left edge of the window lined up with
/// anything else, and each pane's heading sat a different distance above
/// its own separator.
pub(super) const SPACE_XS: f32 = 4.0;
pub(super) const SPACE_SM: f32 = 8.0;
pub(super) const SPACE_MD: f32 = 12.0;
pub(super) const SPACE_LG: f32 = 18.0;

/// Inset from the edge of any panel to its content, so the left edges of
/// the toolbar, the status bar, and every pane's heading form one column.
pub(super) const PAD: f32 = SPACE_MD;

/// How long a hover highlight takes to reach full strength.
///
/// Deliberately short. A highlight that lags behind the pointer reads as
/// the app being slow rather than as motion; this is long enough to be a
/// fade rather than a flash, and no longer. Every hoverable surface uses
/// it, because one control that snaps beside one that fades is exactly
/// what makes a window feel unfinished.
pub(super) const HOVER_SECONDS: f32 = 0.11;

/// How long the tree's expand chevron takes to turn through its quarter
/// circle. A touch slower than a hover: this one is a state change worth
/// noticing, not a pointer following.
pub(super) const TURN_SECONDS: f32 = 0.15;

/// Width of the accent edge that marks the hovered table row.
pub(super) const ROW_HOVER_EDGE: f32 = 3.0;

pub(super) const TREEMAP_SELECTION_WIDTH: f32 = 3.0;

pub(super) const TABLE_HEADER_HEIGHT: f32 = 32.0;

pub(super) const TABLE_ROW_HEIGHT: f32 = 30.0;

pub(super) const VIEW_TAB_HEIGHT: f32 = 34.0;

thread_local! {
    /// The palette in force for the frame currently being painted.
    ///
    /// A `Cell` rather than a lock: this is only ever touched from the
    /// thread that is drawing, and it is read often enough per frame
    /// that a lock would be a silly thing to pay for a value that is
    /// nineteen `u32`s wide.
    static ACTIVE_PALETTE: Cell<Palette> = Cell::new(Palette::default());
}

/// The palette the current frame is being painted with.
pub(super) fn palette() -> Palette {
    ACTIVE_PALETTE.with(Cell::get)
}

pub(super) fn set_palette(palette: Palette) {
    ACTIVE_PALETTE.with(|cell| cell.set(palette));
}

pub(super) fn apply_style(ctx: &egui::Context, palette: Palette) {
    set_palette(palette);
    let dark = palette.mode.is_dark();
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = dark;
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(11.0, 7.0);
    style.spacing.menu_margin = Margin::same(8.0);
    style.spacing.indent = 18.0;
    style.spacing.scroll = scroll_style();
    style.spacing.interact_size = Vec2::new(40.0, 32.0);
    // This is an application UI, not a document viewer. Selectable labels
    // steal pointer drags/clicks from table rows and make row selection feel
    // broken whenever the pointer lands on text.
    style.interaction.selectable_labels = false;
    style.interaction.multi_widget_text_select = false;
    // Tooltips appear on the frame the pointer arrives, with no delay and
    // no requirement that it hold still first.
    //
    // egui's defaults (a 0.5s delay, only once the pointer stops) assume a
    // continuously repainting app. This one repaints on input, so when the
    // pointer stops moving there are no further frames — and the timer
    // those defaults depend on has nothing to elapse on. The tooltip then
    // appears only when something else happens to be driving repaints,
    // such as a hover fade still running, which is why the toolbar's tips
    // worked erratically rather than not at all.
    //
    // The alternative is to request a repaint wherever a tip might be
    // pending, which is every hoverable widget in the app. Showing them
    // immediately needs no timer at all.
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    style.visuals.window_fill = palette.raised;
    style.visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 5.0),
        blur: 18.0,
        spread: 2.0,
        // A light theme cannot borrow the dark theme's shadow: 110/255
        // black under a white card is a smear, not a lift.
        color: Color32::from_black_alpha(if dark { 110 } else { 38 }),
    };
    style.visuals.panel_fill = palette.panel;
    style.visuals.faint_bg_color = palette.faint;
    style.visuals.extreme_bg_color = palette.app;
    style.visuals.code_bg_color = palette.app;
    style.visuals.hyperlink_color = palette.accent;
    style.visuals.error_fg_color = palette.danger;
    style.visuals.warn_fg_color = palette.warning;
    style.visuals.widgets.noninteractive.fg_stroke.color = palette.primary_text;
    style.visuals.widgets.inactive.fg_stroke.color = palette.primary_text;
    style.visuals.widgets.noninteractive.bg_fill = palette.panel;
    style.visuals.widgets.noninteractive.weak_bg_fill = palette.panel;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    // `bg_fill` and `weak_bg_fill` are deliberately different. egui uses
    // `weak_bg_fill` for buttons — which are surfaces and may share the
    // card's color — and `bg_fill` for filled controls that have to stand
    // out from whatever is behind them, the scrollbar handle among them.
    // Setting both to `raised` made every scrollbar the same color as the
    // box it was scrolling.
    style.visuals.widgets.inactive.bg_fill = palette.control;
    style.visuals.widgets.inactive.weak_bg_fill = palette.raised;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    style.visuals.widgets.hovered.bg_fill = palette.control_hover;
    style.visuals.widgets.hovered.weak_bg_fill = palette.hover;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.border_strong);
    style.visuals.widgets.hovered.fg_stroke.color = palette.primary_text;
    style.visuals.widgets.active.bg_fill = palette.accent;
    style.visuals.widgets.active.weak_bg_fill = palette.accent_muted;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    style.visuals.widgets.active.fg_stroke.color = palette.on_accent;
    style.visuals.widgets.open = style.visuals.widgets.active;
    style.visuals.selection.bg_fill = palette.selection_bg;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, palette.selection_stroke);
    for widgets in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.rounding = egui::Rounding::same(6.0);
    }
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    style.visuals.menu_rounding = egui::Rounding::same(8.0);
    style.visuals.popup_shadow = style.visuals.window_shadow;
    style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style.text_styles.insert(
        TextStyle::Heading,
        egui::FontId::new(18.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

/// The one scrollbar style in the app.
///
/// Every `ScrollArea` in the GUI takes its appearance from here by way of
/// `Style::spacing`, so there is no per-call-site tuning to drift.
///
/// Solid, not floating. egui's default scrollbars are invisible until the
/// pointer is over them and overlay the content when they do appear, so a
/// table with columns past the edge looked like it had simply lost them —
/// nothing on screen said the rest was one scroll away. Solid bars are
/// always drawn and take up their own space, which is what a desktop app
/// is expected to do.
///
/// The handle's *color* is not set here: egui takes it from
/// `widgets.*.bg_fill`, which `apply_style` points at `Palette::control`.
/// See that field for why it cannot be a surface color.
pub(super) fn scroll_style() -> egui::style::ScrollStyle {
    let mut scroll = egui::style::ScrollStyle::solid();
    // Wider than egui's 6px default and with a longer minimum handle: at
    // the default size, a handle for a very long list is a few pixels tall
    // and effectively impossible to grab.
    scroll.bar_width = 9.0;
    scroll.handle_min_length = 28.0;
    scroll.bar_inner_margin = 4.0;
    scroll.bar_outer_margin = 2.0;
    scroll
}

pub(super) fn panel_frame() -> Frame {
    let palette = palette();
    Frame::none()
        .fill(palette.panel)
        .inner_margin(Margin::same(PAD))
        .stroke(Stroke::new(1.0_f32, palette.border))
}

pub(super) fn extension_color(extension: &str) -> Color32 {
    // The hue comes from `crate::color`, the same one the TUI draws
    // this extension at. Only the saturation and value are the GUI's
    // own: light themes get a deeper, less bleached tile color, because
    // the 0.88-value pastels that read well on a near-black panel wash
    // out to indistinguishable pale blocks on a white one.
    // Files with no extension take the category color, the same as they
    // do in the TUI. Hashing the literal label "[no extension]" gave
    // them an arbitrary hue that matched nothing.
    let bare = extension.strip_prefix('.').unwrap_or(extension);
    if bare.is_empty() || extension == crate::gui::app::NO_EXTENSION_LABEL {
        return to_color32(crate::color::Category::NoExtension.color());
    }
    let hue = crate::color::extension_hue(extension);
    if palette().mode.is_dark() {
        hsv_to_rgb(hue, 0.68, 0.88)
    } else {
        hsv_to_rgb(hue, 0.78, 0.72)
    }
}

pub(super) fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color32 {
    let (r, g, b) = crate::color::hsv_to_rgb_bytes(hue, saturation, value);
    Color32::from_rgb(r, g, b)
}

pub(super) fn to_color32(c: ratatui::style::Color) -> Color32 {
    if let ratatui::style::Color::Rgb(r, g, b) = c {
        Color32::from_rgb(r, g, b)
    } else {
        Color32::GRAY
    }
}

pub(super) fn relative_luminance(c: Color32) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// CIE L*, on 0..=100.
///
/// Relative luminance is the right metric for *contrast* and the wrong
/// one for "can these two surfaces be told apart": it is linear in
/// light, so the gap between `#000000` and `#0a0a0a` measures as almost
/// nothing even though the eye reads it as a clear step. L* is the
/// perceptual axis, and near black it expands exactly where luminance
/// collapses — which matters here because several themes in the catalog
/// are deliberately near-black.
///
/// Only the theme invariant needs this — nothing paints with it — so it
/// is test-gated rather than carried into the binary.
#[cfg(test)]
pub(super) fn perceptual_lightness(c: Color32) -> f32 {
    let y = relative_luminance(c);
    let f = if y > 0.008_856 {
        y.cbrt()
    } else {
        7.787 * y + 16.0 / 116.0
    };
    (116.0 * f - 16.0).clamp(0.0, 100.0)
}

pub(super) fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (lighter, darker) = if relative_luminance(a) >= relative_luminance(b) {
        (relative_luminance(a), relative_luminance(b))
    } else {
        (relative_luminance(b), relative_luminance(a))
    };
    (lighter + 0.05) / (darker + 0.05)
}

pub(super) fn readable_text_color(background: Color32) -> Color32 {
    if contrast_ratio(Color32::WHITE, background) >= contrast_ratio(Color32::BLACK, background) {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

pub(super) fn scale(c: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.5);
    Color32::from_rgb(
        ((c.r() as f32) * f).min(255.0) as u8,
        ((c.g() as f32) * f).min(255.0) as u8,
        ((c.b() as f32) * f).min(255.0) as u8,
    )
}

pub(super) fn blend(c: Color32, target: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t) as u8;
    Color32::from_rgb(
        m(c.r(), target.r()),
        m(c.g(), target.g()),
        m(c.b(), target.b()),
    )
}
