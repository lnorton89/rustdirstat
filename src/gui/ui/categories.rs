// ============================================================================
// Module:       gui::ui::categories
// Description:  The file-category breakdown pane: one proportional bar for the
//               whole of it, then a row per category.
//
// Dependencies: eframe::egui; crate::stats::ExtStat, crate::color::Category,
//               crate::gui::app::GuiApp
// ============================================================================

//! The file-category breakdown: what kinds of thing are taking up the
//! space, as opposed to which individual files.
//!
//! Shaped after the storage screens on Android and iOS — one proportional
//! bar for the whole of it, then a row per category. That layout answers
//! "what is filling this up" at a glance, which neither the file tree nor
//! the per-extension list does: the tree is organised by location and the
//! extension list is long and fine-grained, so a drive that is 60% video
//! looks like a hundred separate `.mkv` rows rather than one fact.
//!
//! The totals are free. `Node::ext_totals` is rolled up per category
//! during the scan, so this is an array read rather than a walk, even at
//! the root of a whole drive.

use crate::color::Category;
use crate::gui::app::GuiApp;
use crate::gui::icons::Icon;
use crate::stats::ExtStat;
use crate::util::human_bytes;
use eframe::egui::{self, RichText, Sense, Vec2};

use super::theme::*;
use super::widgets::*;

/// Height of the proportional bar.
const BAR_HEIGHT: f32 = 14.0;
/// A segment narrower than this is widened to it, so a category that is
/// present but tiny still shows as a sliver rather than vanishing. The
/// distortion is bounded and the row below always states the real
/// percentage.
const MIN_SEGMENT_PX: f32 = 3.0;

pub(super) fn draw_categories(app: &mut GuiApp, ui: &mut egui::Ui) {
    let stats = crate::stats::extension_stats(app.zoom_node(), app.use_physical);
    if stats.is_empty() {
        return;
    }
    let total: u64 = stats.iter().map(|stat| stat.size).sum::<u64>().max(1);

    draw_bar(ui, &stats, total, app.highlighted_category);
    ui.add_space(6.0);

    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for stat in &stats {
            if category_chip(
                ui,
                stat,
                total,
                app.highlighted_category == Some(stat.category),
            ) {
                clicked = Some(stat.category);
            }
        }
    });

    if let Some(category) = clicked {
        // Clicking the highlighted category again clears it, so the chip
        // is a toggle rather than a one-way trip that needs the separate
        // "Clear highlight" button to undo.
        if app.highlighted_category == Some(category) {
            app.highlighted_category = None;
        } else {
            app.highlighted_category = Some(category);
            app.highlighted_extension = None;
        }
    }
}

fn draw_bar(ui: &mut egui::Ui, stats: &[ExtStat], total: u64, highlighted: Option<Category>) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), BAR_HEIGHT), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter()
        .rect_filled(rect, egui::Rounding::same(4.0), palette().app);

    let mut x = rect.left();
    for stat in stats {
        let share = stat.size as f32 / total as f32;
        let width = (rect.width() * share).max(MIN_SEGMENT_PX);
        let segment = egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(width.min(rect.right() - x), rect.height()),
        );
        if segment.width() <= 0.0 {
            break;
        }
        let mut color = to_color32(stat.category.color());
        if highlighted.is_some_and(|category| category != stat.category) {
            color = blend(color, palette().panel, 0.72);
        }
        ui.painter().rect_filled(segment, 0.0, color);
        x = segment.right();
    }

    // Rounded ends over square segments, so the bar reads as one object
    // rather than as a row of blocks that happens to be adjacent.
    ui.painter().rect_stroke(
        rect,
        egui::Rounding::same(4.0),
        egui::Stroke::new(1.0_f32, palette().border),
    );
}

/// One category chip: icon, name, size and share, in a rounded pill.
///
/// Hand-laid rather than built from an `egui::Frame`, because a `Frame`
/// does not wrap. A frame measures itself against the space remaining on
/// the current line and *then* allocates what it measured, so in a
/// `horizontal_wrapped` row a chip that does not fit overflows the row
/// instead of moving to the next line. The row's width then exceeds the
/// pane's, a side panel stores its content's width as its own, and the
/// divider cannot be dragged back — nine chips in a row pinned the
/// extensions pane at nearly the full window width.
///
/// Measuring first and calling `allocate_exact_size` puts the decision
/// where the layout can act on it: an exact allocation is what
/// `horizontal_wrapped` wraps on. Painting then happens into the rect it
/// gave back, the same way [`sortable_header`] and [`view_tab`] work.
fn category_chip(ui: &mut egui::Ui, stat: &ExtStat, total: u64, selected: bool) -> bool {
    const ICON: f32 = 15.0;
    const ICON_GAP: f32 = 2.0;
    let margin = egui::vec2(SPACE_SM, SPACE_XS + 2.0);

    let percent = stat.size as f64 / total as f64 * 100.0;
    let color = to_color32(stat.category.color());

    let name = egui::WidgetText::from(RichText::new(stat.category.label()).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let detail = egui::WidgetText::from(
        RichText::new(format!("{}  ·  {percent:.1}%", human_bytes(stat.size)))
            .small()
            .color(palette().secondary_text),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Small,
    );

    let gap = ui.spacing().item_spacing.x;
    let width = margin.x * 2.0 + ICON + ICON_GAP + name.size().x + gap + detail.size().x;
    let height = margin.y * 2.0 + name.size().y.max(detail.size().y).max(ICON);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());

    // A chip is painted before its own response exists in the frame
    // where it first appears, so the hover ramp reads the previous
    // frame's state; see [`remembered_hover`].
    let id = ui.id().with(("category_chip", stat.category.label()));
    let t = remembered_hover(ui, id);
    remember_hover(ui, id, response.hovered());

    if ui.is_rect_visible(rect) {
        let rest = if selected {
            palette().accent_muted
        } else {
            palette().raised
        };
        let stroke = egui::Stroke::new(
            1.0_f32,
            if selected {
                palette().accent
            } else {
                blend(palette().border, palette().border_strong, t)
            },
        );
        ui.painter().rect(
            rect,
            egui::Rounding::same(6.0),
            blend(rest, palette().hover, t),
            stroke,
        );

        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + margin.x + ICON * 0.5, rect.center().y),
            Vec2::splat(ICON),
        );
        Icon::for_category(stat.category).paint(ui.painter(), icon_rect, color);

        let name_x = icon_rect.right() + ICON_GAP;
        ui.painter().galley(
            egui::pos2(name_x, rect.center().y - name.size().y * 0.5),
            name.clone(),
            palette().primary_text,
        );
        ui.painter().galley(
            egui::pos2(
                name_x + name.size().x + gap,
                rect.center().y - detail.size().y * 0.5,
            ),
            detail,
            palette().secondary_text,
        );
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
        .on_hover_text(format!(
            "{} · {} files · click to highlight in the treemap",
            stat.category.label(),
            crate::util::thousands(stat.count)
        ))
        .clicked()
}
