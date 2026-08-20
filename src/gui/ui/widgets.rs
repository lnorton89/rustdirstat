//! Controls shared by more than one part of the window: menu rows,
//! toolbar buttons, table headers, and the view tabs.
//!
//! These are hand-painted rather than composed from egui built-ins
//! wherever the built-in cannot line its columns up -- see
//! [`menu_item`] for the case that drove it.

use crate::gui::app::FileView;
use crate::gui::icons::Icon;
use eframe::egui::{self, Color32, Frame, Margin, RichText, Sense, Stroke, TextStyle, Vec2};

#[cfg(test)]
use super::probes::*;
use super::theme::*;

pub(super) fn view_icon(view: FileView) -> Icon {
    match view {
        FileView::AllFiles => Icon::Tree,
        FileView::LargestFiles => Icon::Largest,
        FileView::DuplicateFiles => Icon::Duplicate,
        FileView::SearchResults => Icon::Search,
    }
}

pub(super) fn view_tab(ui: &mut egui::Ui, selected: bool, view: FileView) -> egui::Response {
    const ICON_SIZE: f32 = 16.0;
    const GAP: f32 = 8.0;
    const HORIZONTAL_PADDING: f32 = 11.0;
    let galley = egui::WidgetText::from(RichText::new(view.label()).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let width = HORIZONTAL_PADDING * 2.0 + ICON_SIZE + GAP + galley.size().x;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, VIEW_TAB_HEIGHT), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            ACCENT_MUTED_COLOR
        } else if response.hovered() || response.has_focus() {
            HOVER_COLOR
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if selected || response.has_focus() {
            Stroke::new(1.0_f32, ACCENT_COLOR)
        } else {
            Stroke::NONE
        };
        ui.painter()
            .rect(rect, egui::Rounding::same(6.0), fill, stroke);
        let color = if selected {
            Color32::WHITE
        } else {
            PRIMARY_TEXT_COLOR
        };
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                rect.left() + HORIZONTAL_PADDING + ICON_SIZE * 0.5,
                rect.center().y,
            ),
            Vec2::splat(ICON_SIZE),
        );
        view_icon(view).paint(ui.painter(), icon_rect, color);
        ui.painter().galley(
            egui::pos2(
                icon_rect.right() + GAP,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            color,
        );
    }
    #[cfg(test)]
    probe(&TEST_VIEW_TAB_RECTS).push((view, rect));
    response.on_hover_text(format!("Show {}", view.label()))
}

pub(super) fn tool(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    tool_enabled(ui, true, icon, tip)
}

pub(super) fn tool_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: Icon,
    tip: &str,
) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(Vec2::new(40.0, 36.0)),
    );
    let color = if enabled {
        ui.style().interact(&response).fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    icon.paint(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(18.0)),
        color,
    );
    response.on_hover_text(tip)
}

pub(super) fn compact_icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(20.0)),
    );
    let color = ui.style().interact(&response).fg_stroke.color;
    icon.paint(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(12.0)),
        color,
    );
    response.on_hover_text(tip)
}

pub(super) fn paint_inline_icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    icon.paint(ui.painter(), rect.shrink(1.0), color);
}

pub(super) fn icon_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    icon_menu_item(ui, true, selected, icon, label)
}

pub(super) fn icon_button(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    icon_menu_item(ui, enabled, false, icon, label)
}

pub(super) fn icon_menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    selected: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    menu_item(ui, enabled, selected, Some(icon), label, None)
}

/// A menu row with its keyboard shortcut in a right-aligned column.
pub(super) fn menu_action(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: Icon,
    label: &str,
    shortcut: &str,
) -> egui::Response {
    menu_item(ui, enabled, false, Some(icon), label, Some(shortcut))
}

/// A menu row that turns a setting on and off, marked with a checkmark in
/// the same icon column the action rows use. egui's own `checkbox` puts
/// its box at a different indent and baseline, which is what made these
/// menus look ragged next to the icon rows above them.
pub(super) fn menu_toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    let icon = if *value { Some(Icon::Check) } else { None };
    if menu_item(ui, true, false, icon, label, None).clicked() {
        *value = !*value;
    }
}

/// A menu row that picks one of a set of mutually exclusive choices.
pub(super) fn menu_choice(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let icon = if selected { Some(Icon::Dot) } else { None };
    menu_item(ui, true, false, icon, label, None)
}

pub(super) const MENU_ICON_SIZE: f32 = 16.0;

pub(super) const MENU_ICON_GAP: f32 = 10.0;

/// Clear air between the longest label and its shortcut, so the two read
/// as separate columns instead of as one run-on string.
pub(super) const MENU_SHORTCUT_GAP: f32 = 32.0;

/// Every row claims at least this much width, which is what lets rows in
/// one menu line up and stops a short menu from collapsing into a
/// cramped sliver of a popup.
pub(super) const MENU_MIN_WIDTH: f32 = 232.0;

pub(super) const MENU_ROW_HEIGHT: f32 = 30.0;

/// Lays a menu row out as three real columns — icon, label, shortcut —
/// rather than padding one string with leading and interior spaces. The
/// UI font is proportional, so the string-padding approach this replaces
/// could not actually align anything: every menu came out ragged, and
/// the shortcuts drifted by several pixels per row.
pub(super) fn menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    selected: bool,
    icon: Option<Icon>,
    label: &str,
    shortcut: Option<&str>,
) -> egui::Response {
    let label = label.trim();
    ui.add_enabled_ui(enabled, |ui| {
        let galley = egui::WidgetText::from(label).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );
        let shortcut_galley = shortcut.map(|shortcut| {
            egui::WidgetText::from(shortcut).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                TextStyle::Button,
            )
        });
        let padding = ui.spacing().button_padding;
        let shortcut_width = shortcut_galley
            .as_ref()
            .map_or(0.0, |g| MENU_SHORTCUT_GAP + g.size().x);
        let desired = Vec2::new(
            (padding.x * 2.0 + MENU_ICON_SIZE + MENU_ICON_GAP + galley.size().x + shortcut_width)
                .max(MENU_MIN_WIDTH),
            (padding.y * 2.0 + galley.size().y)
                .max(MENU_ROW_HEIGHT)
                .max(ui.spacing().interact_size.y),
        );
        let (rect, response) = ui.allocate_at_least(desired, Sense::click());
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                rect.left() + padding.x + MENU_ICON_SIZE * 0.5,
                rect.center().y,
            ),
            Vec2::splat(MENU_ICON_SIZE),
        );
        let text_rect = egui::Rect::from_min_size(
            egui::pos2(
                icon_rect.right() + MENU_ICON_GAP,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley.size(),
        );
        let shortcut_size = shortcut_galley.as_ref().map_or(Vec2::ZERO, |g| g.size());
        let shortcut_rect = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - padding.x - shortcut_size.x,
                rect.center().y - shortcut_size.y * 0.5,
            ),
            shortcut_size,
        );

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, selected);
            if selected || response.hovered() || response.highlighted() || response.has_focus() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                );
            }
            let color = if enabled {
                visuals.text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            if let Some(icon) = icon {
                icon.paint(ui.painter(), icon_rect, color);
            }
            if let Some(shortcut_galley) = &shortcut_galley {
                let shortcut_color = if enabled {
                    SECONDARY_TEXT_COLOR
                } else {
                    ui.visuals().weak_text_color()
                };
                ui.painter()
                    .galley(shortcut_rect.min, shortcut_galley.clone(), shortcut_color);
            }
            ui.painter().galley(text_rect.min, galley, color);
        }

        #[cfg(test)]
        {
            probe(&TEST_ICON_MENU_LAYOUTS).push((label.to_owned(), rect, icon_rect, text_rect));
            probe(&TEST_MENU_ITEM_LAYOUTS).push(MenuItemLayout {
                label: label.to_owned(),
                row: rect,
                icon: icon_rect,
                text: text_rect,
                shortcut: shortcut_galley.as_ref().map(|_| shortcut_rect),
            });
        }
        response
    })
    .inner
}

pub(super) fn icon_heading(ui: &mut egui::Ui, icon: Icon, label: &str) {
    ui.horizontal(|ui| {
        paint_inline_icon(ui, icon, 19.0, ACCENT_COLOR);
        ui.heading(label);
    });
}

pub(super) fn accent_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.weak_bg_fill = ACCENT_MUTED_COLOR;
        ui.visuals_mut().widgets.inactive.bg_fill = ACCENT_MUTED_COLOR;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, ACCENT_COLOR);
        ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::WHITE;
        ui.button(label)
    })
    .inner
}

pub(super) fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        let red = Color32::from_rgb(220, 82, 92);
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::from_rgb(91, 36, 43);
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(91, 36, 43);
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, red);
        ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::WHITE;
        ui.button(label)
    })
    .inner
}

pub(super) fn settings_group(
    ui: &mut egui::Ui,
    icon: Icon,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    Frame::none()
        .fill(APP_COLOR)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(Margin::same(12.0))
        .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
        .show(ui, |ui| {
            icon_heading(ui, icon, title);
            ui.add_space(5.0);
            add_contents(ui);
        });
}

pub(super) fn sortable_header(
    ui: &mut egui::Ui,
    label: &'static str,
    direction: Option<Icon>,
) -> egui::Response {
    const ICON_SIZE: f32 = 12.0;
    const ICON_GAP: f32 = 5.0;
    let galley = egui::WidgetText::from(RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let size = ui.available_size_before_wrap();
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if ui.is_rect_visible(rect) {
        let fill = if direction.is_some() {
            ACCENT_MUTED_COLOR
        } else if response.hovered() || response.has_focus() {
            HOVER_COLOR
        } else {
            RAISED_COLOR
        };
        ui.painter()
            .rect_filled(rect, egui::Rounding::same(4.0), fill);
        let text_pos = egui::pos2(rect.left() + 7.0, rect.center().y - galley.size().y * 0.5);
        let color = if direction.is_some() {
            Color32::WHITE
        } else {
            ui.style().interact(&response).text_color()
        };
        ui.painter().galley(text_pos, galley.clone(), color);
        if let Some(icon) = direction {
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(
                    text_pos.x + galley.size().x + ICON_GAP + ICON_SIZE * 0.5,
                    rect.center().y,
                ),
                Vec2::splat(ICON_SIZE),
            );
            icon.paint(ui.painter(), icon_rect, color);
        }
    }
    #[cfg(test)]
    probe(&TEST_DIRECTORY_HEADER_ICONS).push((label, direction));
    let cursor = if response.dragged() {
        egui::CursorIcon::Grabbing
    } else {
        egui::CursorIcon::Grab
    };
    response
        .on_hover_cursor(cursor)
        .on_hover_text(format!("Click to sort by {label} · drag to reorder"))
}

pub(super) fn table_header_label(ui: &mut egui::Ui, label: &str) {
    let rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(rect, egui::Rounding::same(4.0), RAISED_COLOR);
    ui.add_space(7.0);
    ui.label(RichText::new(label).strong().color(PRIMARY_TEXT_COLOR));
}

pub(super) fn percentage_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(180.0), 12.0),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 4.0, APP_COLOR);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 4.0, ACCENT_COLOR);
}

pub(super) fn truncate_for_width(
    name: &str,
    max_w: f32,
    painter: &egui::Painter,
    ui: &egui::Ui,
) -> String {
    let font = TextStyle::Small.resolve(ui.style());
    if painter
        .layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
        .rect
        .width()
        <= max_w
    {
        return name.to_string();
    }
    let mut s = name.to_string();
    while !s.is_empty() {
        s.pop();
        let candidate = format!("{s}…");
        if painter
            .layout_no_wrap(candidate.clone(), font.clone(), Color32::WHITE)
            .rect
            .width()
            <= max_w
        {
            return candidate;
        }
    }
    String::new()
}
