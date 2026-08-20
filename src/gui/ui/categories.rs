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

/// One clickable category summary. Returns whether it was clicked.
fn category_chip(ui: &mut egui::Ui, stat: &ExtStat, total: u64, selected: bool) -> bool {
    let percent = stat.size as f64 / total as f64 * 100.0;
    let color = to_color32(stat.category.color());
    // A chip is a `Frame`, and a frame paints its fill before its
    // contents are laid out — so this one cannot know it is hovered until
    // after it has already been painted. The hover state therefore comes
    // from the previous frame; see [`remembered_hover`].
    let id = ui.id().with(("category_chip", stat.category.label()));
    let t = remembered_hover(ui, id);
    let rest = if selected {
        palette().accent_muted
    } else {
        palette().raised
    };
    let response = egui::Frame::none()
        .fill(blend(rest, palette().hover, t))
        .stroke(egui::Stroke::new(
            1.0_f32,
            if selected {
                palette().accent
            } else {
                blend(palette().border, palette().border_strong, t)
            },
        ))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_XS + 2.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_inline_icon(ui, Icon::for_category(stat.category), 15.0, color);
                ui.add_space(2.0);
                ui.label(RichText::new(stat.category.label()).strong());
                ui.label(
                    RichText::new(format!("{}  ·  {percent:.1}%", human_bytes(stat.size)))
                        .small()
                        .color(palette().secondary_text),
                );
            });
        })
        .response;

    let response = response.interact(Sense::click());
    remember_hover(ui, id, response.hovered());
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
