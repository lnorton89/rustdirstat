//! The palette, the egui style derived from it, and the color math
//! the treemap and tables need.
//!
//! Colors live in one place so a change to the palette cannot leave
//! half the window on the old one. The layer names describe depth,
//! not use: `APP_COLOR` is the surface behind everything,
//! `PANEL_COLOR` sits on it, and `RAISED_COLOR` sits on that.

use eframe::egui::{self, Color32, Frame, Margin, Stroke, TextStyle, Vec2};

pub(super) const PAD: f32 = 10.0;

pub(super) const APP_COLOR: Color32 = Color32::from_rgb(17, 19, 23);

pub(super) const PANEL_COLOR: Color32 = Color32::from_rgb(23, 26, 31);

pub(super) const RAISED_COLOR: Color32 = Color32::from_rgb(29, 33, 40);

pub(super) const HOVER_COLOR: Color32 = Color32::from_rgb(37, 43, 52);

pub(super) const BORDER_COLOR: Color32 = Color32::from_rgb(52, 59, 70);

pub(super) const ACCENT_COLOR: Color32 = Color32::from_rgb(91, 155, 239);

pub(super) const ACCENT_MUTED_COLOR: Color32 = Color32::from_rgb(35, 69, 112);

pub(super) const PRIMARY_TEXT_COLOR: Color32 = Color32::from_rgb(218, 222, 230);

pub(super) const SECONDARY_TEXT_COLOR: Color32 = Color32::from_rgb(172, 179, 191);

pub(super) const TREEMAP_SELECTION_WIDTH: f32 = 3.0;

pub(super) const TABLE_HEADER_HEIGHT: f32 = 32.0;

pub(super) const TABLE_ROW_HEIGHT: f32 = 30.0;

pub(super) const VIEW_TAB_HEIGHT: f32 = 34.0;

pub(super) fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(11.0, 7.0);
    style.spacing.menu_margin = Margin::same(8.0);
    style.spacing.indent = 18.0;
    // Solid, not floating. egui's default scrollbars are invisible until
    // the pointer is over them and overlay the content when they appear,
    // so a table with columns past the edge looked like it had simply
    // lost them — there was nothing on screen to say the rest was one
    // scroll away. Solid bars are always drawn and take up their own
    // space, which is what a desktop app is expected to do.
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.interact_size = Vec2::new(40.0, 32.0);
    // This is an application UI, not a document viewer. Selectable labels
    // steal pointer drags/clicks from table rows and make row selection feel
    // broken whenever the pointer lands on text.
    style.interaction.selectable_labels = false;
    style.interaction.multi_widget_text_select = false;
    style.visuals.window_fill = RAISED_COLOR;
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER_COLOR);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 5.0),
        blur: 18.0,
        spread: 2.0,
        color: Color32::from_black_alpha(110),
    };
    style.visuals.panel_fill = PANEL_COLOR;
    style.visuals.faint_bg_color = Color32::from_rgb(27, 30, 36);
    style.visuals.extreme_bg_color = APP_COLOR;
    style.visuals.code_bg_color = APP_COLOR;
    style.visuals.hyperlink_color = ACCENT_COLOR;
    style.visuals.widgets.noninteractive.fg_stroke.color = PRIMARY_TEXT_COLOR;
    style.visuals.widgets.inactive.fg_stroke.color = PRIMARY_TEXT_COLOR;
    style.visuals.widgets.noninteractive.bg_fill = PANEL_COLOR;
    style.visuals.widgets.noninteractive.weak_bg_fill = PANEL_COLOR;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);
    style.visuals.widgets.inactive.bg_fill = RAISED_COLOR;
    style.visuals.widgets.inactive.weak_bg_fill = RAISED_COLOR;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);
    style.visuals.widgets.hovered.bg_fill = HOVER_COLOR;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER_COLOR;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(78, 91, 110));
    style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.active.bg_fill = ACCENT_MUTED_COLOR;
    style.visuals.widgets.active.weak_bg_fill = ACCENT_MUTED_COLOR;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_COLOR);
    style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.open = style.visuals.widgets.active;
    style.visuals.selection.bg_fill = Color32::from_rgb(42, 93, 164);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(145, 193, 255));
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

pub(super) fn panel_frame() -> Frame {
    Frame::none()
        .fill(PANEL_COLOR)
        .inner_margin(Margin::same(PAD))
        .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
}

pub(super) fn extension_color(extension: &str) -> Color32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in extension.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hsv_to_rgb((hash % 360) as f32, 0.68, 0.88)
}

pub(super) fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color32 {
    let c = value * saturation;
    let x = c * (1.0 - (((hue / 60.0) % 2.0) - 1.0).abs());
    let m = value - c;
    let (r, g, b) = match hue {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color32::from_rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
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
