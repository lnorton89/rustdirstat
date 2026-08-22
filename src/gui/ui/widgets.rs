// ============================================================================
// Module:       gui::ui::widgets
// Description:  Controls shared by more than one part of the window: menu
//               rows, toolbar buttons, table headers, view tabs, and the hover
//               ramp.
//
// Dependencies: eframe::egui; super::{probes, theme}
// ============================================================================

//! Controls shared by more than one part of the window: menu rows,
//! toolbar buttons, table headers, and the view tabs.
//!
//! These are hand-painted rather than composed from egui built-ins
//! wherever the built-in cannot line its columns up -- see
//! [`menu_item`] for the case that drove it.

use crate::gui::app::FileView;
use crate::gui::icons::Icon;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, TextStyle, Vec2};

#[cfg(test)]
use super::probes::*;
use super::theme::*;

/// The hover animation for one control, on `0.0..=1.0`.
///
/// Everything hoverable in the window goes through this, so every
/// highlight in the app eases in at the same rate and with the same
/// curve. `cubic_out` rather than a linear ramp: the highlight has to be
/// most of the way there the instant the pointer lands, or it reads as
/// lag instead of as motion.
pub(super) fn hover_t(ctx: &egui::Context, id: egui::Id, hovered: bool) -> f32 {
    ctx.animate_bool_with_time_and_easing(
        id,
        hovered,
        HOVER_SECONDS,
        egui::emath::easing::cubic_out,
    )
}

/// Fades `rest` towards `hot` by the control's hover animation, and
/// returns the fill to paint.
///
/// Takes the responsibility for *which* id animates away from the twenty
/// call sites that would otherwise each invent one.
pub(super) fn hover_fill(
    ui: &egui::Ui,
    response: &egui::Response,
    rest: Color32,
    hot: Color32,
) -> Color32 {
    let hot_now = response.hovered() || response.has_focus();
    let t = hover_t(ui.ctx(), response.id.with("hover_fill"), hot_now);
    if t <= 0.0 {
        return rest;
    }
    // Blending from a transparent rest colour would drag the fill through
    // whatever RGB that transparent value happens to carry, so fade the
    // alpha instead and leave the hue alone.
    if rest.a() == 0 {
        return hot.gamma_multiply(t);
    }
    blend(rest, hot, t)
}

/// The hover animation for a control whose background is painted before
/// its own response exists.
///
/// An `egui::Frame` paints its fill and *then* lays its contents out
/// inside it, so a frame-shaped control cannot know it is hovered until
/// after it has already been painted. Carrying one bool across the frame
/// boundary is what lets such a control fade like everything else instead
/// of stepping a frame late. Pair with [`remember_hover`].
pub(super) fn remembered_hover(ui: &egui::Ui, id: egui::Id) -> f32 {
    let was_hovered = ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    hover_t(ui.ctx(), id, was_hovered)
}

/// Records this frame's hover state for [`remembered_hover`] to read on
/// the next one.
pub(super) fn remember_hover(ui: &egui::Ui, id: egui::Id, hovered: bool) {
    ui.data_mut(|data| data.insert_temp(id, hovered));
}

/// The moving half of a table row's hover state.
///
/// `egui_extras` already washes the row under the pointer in the theme's
/// `hover` colour, but it does that with no animation and one frame late,
/// so on a long list it reads as a flicker trailing the pointer rather
/// than as a highlight. This adds the part that actually carries the eye:
/// an accent edge down the left of the row that grows out of the middle.
///
/// Painted *over* the finished row rather than under it, because a row's
/// rect is not known until its cells have been drawn — an edge can sit on
/// top of a row without touching a single glyph, which a second fill
/// could not. The painter is the table body's own, so the edge is clipped
/// to the table the way the row is.
pub(super) fn row_hover_edge(painter: &egui::Painter, response: &egui::Response, id: egui::Id) {
    let t = hover_t(&response.ctx, id, response.hovered());
    if t <= 0.0 {
        return;
    }
    let mut edge = response.rect;
    edge.max.x = edge.min.x + ROW_HOVER_EDGE;
    let inset = (1.0 - t) * edge.height() * 0.5;
    painter.rect_filled(
        edge.shrink2(Vec2::new(0.0, inset)),
        egui::CornerRadius::same(2),
        palette().accent.gamma_multiply(t),
    );
}

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
            palette().accent_muted
        } else {
            hover_fill(ui, &response, Color32::TRANSPARENT, palette().hover)
        };
        let stroke = if selected || response.has_focus() {
            Stroke::new(1.0_f32, palette().accent)
        } else {
            Stroke::NONE
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(6),
            fill,
            stroke,
            egui::StrokeKind::Middle,
        );
        let color = if selected {
            palette().on_accent
        } else {
            palette().primary_text
        };
        // An unselected tab lifts by a pixel under the pointer. The fill
        // says "this is hoverable"; the lift is what says "and it will do
        // something" without borrowing the selected state's colour.
        let lift = if selected {
            0.0
        } else {
            hover_t(ui.ctx(), response.id.with("tab_lift"), response.hovered())
        };
        let center_y = rect.center().y - lift;
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + HORIZONTAL_PADDING + ICON_SIZE * 0.5, center_y),
            Vec2::splat(ICON_SIZE),
        );
        view_icon(view).paint(ui.painter(), icon_rect, color);
        ui.painter().galley(
            egui::pos2(icon_rect.right() + GAP, center_y - galley.size().y * 0.5),
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
    // Both, deliberately. egui shows nothing on hover for a disabled
    // widget unless asked separately, and these buttons are icon-only —
    // so a greyed-out one was a picture with no name and no way to find
    // out what it was, which is exactly when the tooltip matters most.
    response
        .on_hover_text(tip)
        .on_disabled_hover_text(format!("{tip} (unavailable right now)"))
}

/// Width of the tree's disclosure control. Also the space a file row
/// leaves where a folder row's toggle would be, so the two line up.
pub(super) const EXPAND_TOGGLE_WIDTH: f32 = 20.0;

/// The disclosure control in front of a directory row.
///
/// One chevron that *turns* rather than two that swap. Swapping
/// `ChevronRight` for `ChevronDown` told you the new state and nothing
/// about how you got there; turning through the quarter circle ties the
/// row that just unfolded to the control that unfolded it, which matters
/// in a tree where expanding a row pushes everything below it down the
/// screen.
///
/// `id` identifies the *node*, not the row: the table hands its cells ids
/// built from the row index, and expanding a folder renumbers every row
/// under it — so an index-keyed animation would hand a half-turned
/// chevron to whichever node had just slid into that slot.
pub(super) fn expand_toggle(ui: &mut egui::Ui, id: egui::Id, expanded: bool) -> egui::Response {
    const GLYPH: f32 = 12.0;
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(EXPAND_TOGGLE_WIDTH)),
    );
    let center = response.rect.center();
    let hot = hover_fill(ui, &response, Color32::TRANSPARENT, palette().hover);
    if hot.a() > 0 {
        ui.painter().rect_filled(
            egui::Rect::from_center_size(center, Vec2::splat(EXPAND_TOGGLE_WIDTH)),
            egui::CornerRadius::same(5),
            hot,
        );
    }
    // A quarter turn clockwise takes the right-pointing chevron to a
    // down-pointing one, so the two states the tree has are the two ends
    // of one animation rather than two unrelated pictures.
    let turns =
        ui.ctx()
            .animate_value_with_time(id, if expanded { 0.25 } else { 0.0 }, TURN_SECONDS);
    #[cfg(test)]
    probe(&TEST_CHEVRON_TURNS).push(turns);
    Icon::ChevronRight.paint_turned(
        ui.painter(),
        egui::Rect::from_center_size(center, Vec2::splat(GLYPH)),
        ui.style().interact(&response).fg_stroke.color,
        turns,
    );
    response.on_hover_text(if expanded { "Collapse" } else { "Expand" })
}

/// Returns where the icon landed, which the tree's row-alignment test
/// needs and every other caller ignores.
pub(super) fn paint_inline_icon(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    color: Color32,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    icon.paint(ui.painter(), rect.shrink(1.0), color);
    rect
}

/// Lays the full-colour brand mark into the current row, sized like an
/// inline icon.
///
/// Separate from [`paint_inline_icon`] because it takes no colour: the
/// mark is identity rather than interface, so there is nothing here for
/// a caller — or a theme — to tint. See [`crate::brand`].
pub(super) fn paint_inline_brand(ui: &mut egui::Ui, size: f32) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    crate::gui::icons::paint_brand(ui.painter(), rect.shrink(1.0));
    rect
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
            // Faded in rather than switched on. A menu is a column of
            // identical rows, so the highlight appearing from nowhere as
            // the pointer travels down it is the one place in the window
            // where a hard cut is most obvious.
            let hot = response.hovered() || response.highlighted() || response.has_focus();
            let t = if selected {
                1.0
            } else {
                hover_t(ui.ctx(), response.id.with("menu_row"), hot)
            };
            if t > 0.0 {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.corner_radius,
                    visuals.weak_bg_fill.gamma_multiply(t),
                    Stroke::new(
                        visuals.bg_stroke.width,
                        visuals.bg_stroke.color.gamma_multiply(t),
                    ),
                    egui::StrokeKind::Middle,
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
                    palette().secondary_text
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

pub(super) fn accent_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let palette = palette();
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.weak_bg_fill = palette.accent_muted;
        ui.visuals_mut().widgets.inactive.bg_fill = palette.accent_muted;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.accent);
        ui.visuals_mut().widgets.inactive.fg_stroke.color = palette.on_accent;
        ui.button(label)
    })
    .inner
}

pub(super) fn danger_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let palette = palette();
    // A little stronger than the callout background: this is the control
    // that performs the destructive action, so it has to outrank the
    // panel explaining it rather than blend into it.
    let fill = blend(
        palette.panel,
        palette.danger,
        if palette.mode.is_dark() { 0.34 } else { 0.22 },
    );
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.weak_bg_fill = fill;
        ui.visuals_mut().widgets.inactive.bg_fill = fill;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.danger);
        ui.visuals_mut().widgets.inactive.fg_stroke.color = readable_text_color(fill);
        ui.button(label)
    })
    .inner
}

/// Width of the strip at a header's right edge left to the column
/// resizer rather than claimed for click-to-sort and drag-to-reorder.
pub(super) const HEADER_RESIZE_MARGIN: f32 = 8.0;

/// Distance from the left edge of a header cell to the start of its
/// label.
///
/// Shared by both header widgets. They used to disagree: the sortable
/// one painted its galley at `left + 7`, while the plain one asked for
/// `add_space(7.0)` and then a label — and `add_space` is followed by the
/// row's item spacing, so the text landed at 15 instead. Two tables side
/// by side had their column names in two different places.
pub(super) const HEADER_TEXT_INSET: f32 = 7.0;

/// A column heading: fills its cell, sorts on click, reorders on drag.
///
/// `claims_width` is what the cell reports *needing*, which is not the
/// same as what it covers. `egui_extras` records the widest thing a
/// column ever allocated and, for the `remainder()` column, refuses to
/// shrink the column below it — so a header that allocates its whole
/// cell pins the column at the widest it has ever been. Widen the window
/// and narrow it again and the table kept the wider layout, showing a
/// horizontal scrollbar for space it no longer needed.
///
/// So the last column's header allocates only the room its text wants,
/// while still painting and sensing across the full cell. It looks and
/// behaves identically; it just stops claiming the width as a floor.
pub(super) fn sortable_header(
    ui: &mut egui::Ui,
    label: &'static str,
    direction: Option<Icon>,
    claims_width: bool,
) -> egui::Response {
    const ICON_SIZE: f32 = 12.0;
    const ICON_GAP: f32 = 5.0;
    let galley = egui::WidgetText::from(RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let full = ui.available_size_before_wrap();
    let size = if claims_width {
        full
    } else {
        let wanted = galley.size().x
            + HEADER_TEXT_INSET * 2.0
            + if direction.is_some() {
                ICON_SIZE + ICON_GAP
            } else {
                0.0
            };
        egui::vec2(wanted.min(full.x), full.y)
    };
    // The header fills the column, but it must not *sense* the whole of
    // it: `egui_extras` puts the column's resize handle on the boundary
    // at the right-hand edge, and a header that senses drags across its
    // full width swallows every one of those drags as a reorder — which
    // is why resizing stopped working the moment headers became
    // draggable. Leaving the last few pixels unclaimed gives the
    // resizer its grab strip back.
    let (allocated, _) = ui.allocate_exact_size(size, Sense::hover());
    // Painted and sensed across the whole cell whatever was allocated,
    // so a heading that claims less still looks like a full-width one.
    let rect = egui::Rect::from_min_max(
        allocated.min,
        egui::pos2(allocated.left() + full.x, allocated.bottom()),
    );
    let mut draggable = rect;
    draggable.max.x = (draggable.max.x - HEADER_RESIZE_MARGIN).max(draggable.min.x);
    let response = ui.interact(
        draggable,
        ui.id().with(("sortable_header", label)),
        Sense::click_and_drag(),
    );
    if ui.is_rect_visible(rect) {
        let fill = if direction.is_some() {
            palette().accent_muted
        } else {
            hover_fill(ui, &response, palette().raised, palette().hover)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), fill);
        let text_pos = egui::pos2(
            rect.left() + HEADER_TEXT_INSET,
            rect.center().y - galley.size().y * 0.5,
        );
        let color = if direction.is_some() {
            palette().on_accent
        } else {
            ui.style().interact(&response).text_color()
        };
        ui.painter().galley(text_pos, galley.clone(), color);
        #[cfg(test)]
        probe(&TEST_TABLE_HEADER_TEXT).push((label.to_owned(), text_pos.x));
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

/// A column name in a table whose columns cannot be sorted or reordered.
///
/// Painted the same way [`sortable_header`] is, rather than composed from
/// `add_space` and a label, so the two kinds of header put their text in
/// the same place. See [`HEADER_TEXT_INSET`] for what they used to do
/// instead.
pub(super) fn table_header_label(ui: &mut egui::Ui, label: &str) {
    let galley = egui::WidgetText::from(RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let (rect, _) = ui.allocate_exact_size(ui.available_size_before_wrap(), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), palette().raised);
        ui.painter().galley(
            egui::pos2(
                rect.left() + HEADER_TEXT_INSET,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            palette().primary_text,
        );
    }
    #[cfg(test)]
    probe(&TEST_TABLE_HEADER_TEXT).push((label.to_owned(), rect.left() + HEADER_TEXT_INSET));
}

/// The icon-and-name pair that opens a pane.
///
/// One helper rather than three near-identical `horizontal` blocks, so
/// the icon size and the gap after it cannot drift apart between the
/// treemap pane, the extension pane, and the search view — which they
/// had.
pub(super) fn section_title(ui: &mut egui::Ui, icon: Icon, title: &str) {
    paint_inline_icon(ui, icon, SECTION_ICON_SIZE, palette().accent);
    ui.add_space(SPACE_XS);
    ui.heading(title);
}

pub(super) const SECTION_ICON_SIZE: f32 = 19.0;

/// The rule under a pane's heading, with identical air above and below.
///
/// `ui.separator()` allocates its own padding on top of the row spacing,
/// so every pane that reached for one and then nudged it with a hand-picked
/// `add_space` ended up with a different gap: 3px above and 5px below in
/// the treemap, 6 and 5 in the extension list, and something else again in
/// search.
pub(super) fn section_rule(ui: &mut egui::Ui) {
    ui.add_space(SPACE_XS);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 0.0, palette().border);
    }
    ui.add_space(SPACE_XS);
    #[cfg(test)]
    probe(&TEST_SECTION_RULE_RECTS).push(rect);
}

pub(super) fn percentage_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(180.0), 12.0),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 4.0, palette().app);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 4.0, palette().accent);
}

pub(super) fn truncate_for_width(
    name: &str,
    max_w: f32,
    painter: &egui::Painter,
    ui: &egui::Ui,
) -> String {
    let font = TextStyle::Small.resolve(ui.style());
    let full = painter.layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE);
    if full.rect.width() <= max_w {
        return name.to_string();
    }

    // Two layouts, whatever the length of the name. This used to lay the
    // whole string out again for every character it removed — one
    // `Galley`, one `String` and one shaping pass per character — on a
    // path that runs for every visible treemap tile, every frame.
    //
    // A galley already knows where each character sits: `Row::glyphs` is
    // one entry per `char`, positioned from the galley's own origin. So
    // the cut point is a scan over that, not a search over re-layouts.
    let font_for_check = font.clone();
    let ellipsis_width = painter
        .layout_no_wrap(ELLIPSIS.to_string(), font, Color32::WHITE)
        .rect
        .width();
    let budget = max_w - ellipsis_width;
    if budget <= 0.0 {
        return String::new();
    }
    let Some(row) = full.rows.first() else {
        return String::new();
    };
    let mut kept = String::new();
    for glyph in &row.glyphs {
        if glyph.pos.x + glyph.advance_width > budget {
            break;
        }
        kept.push(glyph.chr);
    }
    if kept.is_empty() {
        return String::new();
    }
    kept.push(ELLIPSIS);

    // The scan above adds up advance widths; a laid-out string is not
    // always exactly that sum. Shaping can kern the last kept character
    // against the ellipsis, and the galley's rect carries the final
    // glyph's side bearing — egui 0.36 lays "a-rather-l…" out 0.8px wider
    // than its own advances predict, which is enough to fail the promise
    // this function exists to keep.
    //
    // So the scan is the estimate and this is the check: lay the result
    // out once, and give back characters until it really fits. It
    // normally costs one layout and no iterations at all, and it is only
    // reached for labels that are being truncated anyway.
    while measured_width(&kept, &font_for_check, painter) > max_w {
        let Some(cut) = kept
            .char_indices()
            .rev()
            .find(|(_, c)| *c != ELLIPSIS)
            .map(|(index, _)| index)
        else {
            return String::new();
        };
        kept.remove(cut);
        if kept.chars().all(|c| c == ELLIPSIS) {
            return String::new();
        }
    }
    kept
}

/// Width of one laid-out line, for [`truncate_for_width`]'s final check.
fn measured_width(text: &str, font: &egui::FontId, painter: &egui::Painter) -> f32 {
    painter
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
        .rect
        .width()
}

/// The character [`truncate_for_width`] cuts with.
const ELLIPSIS: char = '…';
