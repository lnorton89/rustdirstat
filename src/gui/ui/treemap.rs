// ============================================================================
// Module:       gui::ui::treemap
// Description:  Paints the treemap panel from the tiles GuiApp has already
//               laid out, including the cushion shading.
//
// Dependencies: eframe::egui; crate::gui::treemap_layout::Tile, crate::color
// ============================================================================

//! Paints the treemap panel from the tiles `GuiApp` has laid out.
//!
//! Layout itself lives in `gui::treemap_layout`; this module only
//! turns tiles into pixels, including the cushion shading that
//! gives nested directories their rounded look.

use crate::color;
use crate::gui::app::{extension_label, GuiApp};
use crate::gui::icons::Icon;
use crate::gui::treemap_layout::Tile;
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextStyle};

#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::widgets::*;

pub(super) fn draw_treemap(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        section_title(ui, Icon::App, "Treemap");
        // The full path lives in the toolbar; repeating it here said
        // nothing and pushed the heading off the edge of a narrow pane.
        // What is worth saying is when the map is *not* showing the whole
        // scan — so the zoom target's name appears only while zoomed.
        if !app.zoom_path.is_empty() {
            ui.label(
                RichText::new(crate::util::display_name(&app.zoom_fs_path()))
                    .color(palette().secondary_text),
            )
            .on_hover_text(crate::util::display_path(&app.zoom_fs_path()));
        }
        if ui.available_width() > 420.0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(if app.use_physical {
                        "Physical size"
                    } else {
                        "Logical size"
                    })
                    .small()
                    .color(palette().secondary_text),
                );
            });
        }
    });
    section_rule(ui);
    let avail = ui.available_size();
    if avail.x <= 1.0 || avail.y <= 1.0 {
        return;
    }
    let (response, painter) = ui.allocate_painter(avail, Sense::click());
    // Measure the strip a tile has to reserve for its own name from the
    // font that name will actually be drawn in. Hard-coding it meant the
    // reserved 14px was shorter than a 12pt line, so the children painted
    // into the rest of the tile covered the bottom of the parent's label
    // and sliced the descenders off every `g` and `p`.
    let label_strip = ui.text_style_height(&TextStyle::Small) + LABEL_TEXT_PADDING * 2.0;
    // Tiles are only re-laid-out when the panel rect or the tree behind
    // it changes; on a large volume the layout walk is far too expensive
    // to redo for every frame of a hover.
    // A splitter drag holds the primary button down, and the layout is
    // keyed on the panel rect — so every frame of that drag would
    // otherwise re-lay-out the entire map. While the pointer is down the
    // map is built at reduced detail; the frame after it comes up
    // rebuilds it in full.
    let interactive = ui.ctx().input(|input| input.pointer.any_down());
    app.refresh_treemap(
        response.rect.min.x,
        response.rect.min.y,
        avail.x,
        avail.y,
        label_strip,
        interactive,
    );
    let mut clicked = None;
    let mut hover_text = None;
    // Painting borrows the cached tiles out of `app`, so the click it
    // records is applied once that borrow has ended.
    {
        let app = &*app;
        let tiles = &app.treemap_tiles;
        let mut selected_rect = None;
        // The treemap is one painter, not a widget per tile, so hover has
        // to be hit-tested by hand. Tiles are emitted level by level, so
        // the *last* one containing the pointer is the deepest — which is
        // also the one painted on top, and therefore the one the pointer
        // looks like it is over.
        let pointer = response.hover_pos();
        let mut hovered_rect = None;
        for tile in tiles {
            if tile.w < 1.0 || tile.h < 1.0 {
                continue;
            }
            let rect =
                egui::Rect::from_min_size(egui::pos2(tile.x, tile.y), egui::vec2(tile.w, tile.h));
            let raw = if tile.is_free_space {
                to_color32(color::free_space_color())
            } else if tile.is_aggregate {
                // Not any one extension, so give the stand-in a neutral fill
                // rather than letting it borrow a color that means something.
                palette().dim
            } else if tile.is_dir {
                to_color32(color::directory_color())
            } else {
                extension_color(&extension_label(&tile.name))
            };
            let mut base = scale(raw, 1.0 - (tile.depth as f32 * 0.05).min(0.35));
            if tile.is_node() {
                if let Some(ext) = &app.highlighted_extension {
                    if tile.is_dir || extension_label(&tile.name) != *ext {
                        base = blend(base, palette().dim, 0.78);
                    }
                } else if let Some(category) = app.highlighted_category {
                    if tile.is_dir || tile.category != Some(category) {
                        base = blend(base, palette().dim, 0.78);
                    }
                }
            }
            paint_cushion_rect(&painter, rect, base);
            // Only outline tiles with room for an outline. A 1px border on
            // each side of a 3px tile leaves one pixel of colour, so
            // gridding the dense regions turned them into black mush and
            // cost a stroke per tile to do it.
            if app.view.grid && rect.width() >= MIN_GRID_PX && rect.height() >= MIN_GRID_PX {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, palette().treemap_grid));
            }
            if app.selected_path.as_ref() == Some(&tile.index_path) {
                selected_rect = treemap_selection_rect(rect);
            }
            if app.view.labels && tile.can_label && tile.w >= 48.0 && tile.h >= label_strip {
                painter.text(
                    rect.min + egui::vec2(4.0, LABEL_TEXT_PADDING),
                    egui::Align2::LEFT_TOP,
                    truncate_for_width(&tile.name, tile.w - 8.0, &painter, ui),
                    TextStyle::Small.resolve(ui.style()),
                    readable_text_color(base),
                );
            }
            if tile.is_node() && pointer.is_some_and(|p| rect.contains(p)) {
                hovered_rect = Some(rect);
                hover_text = Some(tile_hover_text(app, tile));
                #[cfg(test)]
                probe(&TEST_TREEMAP_HOVER).push((tile.index_path.clone(), rect));
            }
            if tile.is_node()
                && response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|p| rect.contains(p))
            {
                clicked = Some(tile.index_path.clone());
            }
        }
        // Over the tiles but under the selection frame, so hovering the
        // selected tile does not paint over the marker that says so.
        //
        // An outline rather than a wash: the tile under the pointer may
        // be three pixels wide, and it may be carrying its own label. A
        // bright edge survives both.
        if let Some(rect) = hovered_rect {
            if rect.width() >= 2.0 && rect.height() >= 2.0 {
                painter.rect_stroke(
                    rect.shrink(0.5),
                    0.0,
                    Stroke::new(1.5_f32, palette().treemap_hover),
                );
            }
        }
        // Paint selection last. Otherwise tiles rendered later overwrite the
        // shared right and bottom edges of the selected tile.
        if let Some(rect) = selected_rect {
            // Two strokes: a wide backing one in the opposite polarity to
            // the marker, so the marker stays visible over a tile that
            // happens to share its color. Which of the two is the backing
            // therefore has to follow the theme, not be fixed at black.
            let marker = palette().treemap_selection;
            let backing = if palette().mode.is_dark() {
                Color32::from_black_alpha(190)
            } else {
                Color32::from_white_alpha(200)
            };
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(TREEMAP_SELECTION_WIDTH + 2.0, backing),
            );
            painter.rect_stroke(rect, 0.0, Stroke::new(TREEMAP_SELECTION_WIDTH, marker));
        }
    }
    if let Some(text) = hover_text {
        response.on_hover_text(text);
    }
    if let Some(path) = clicked {
        app.navigate_to_absolute(path);
    }
}

/// What the tooltip says about the tile under the pointer.
///
/// A treemap tile is a rectangle of colour with, at best, a truncated
/// name on it — so until the pointer stops there is no way to find out
/// what any of it is short of clicking and watching the tree jump. The
/// size comes from the node rather than from the tile because a tile
/// carries only its geometry.
fn tile_hover_text(app: &GuiApp, tile: &Tile) -> String {
    let Some(node) = app.tree.try_node_for(&tile.index_path) else {
        return tile.name.clone();
    };
    format!(
        "{}\n{}",
        crate::util::display_path(&app.tree.path_for(&tile.index_path)),
        crate::util::human_bytes(node.effective_size(app.use_physical)),
    )
}

pub(super) fn treemap_selection_rect(tile: egui::Rect) -> Option<egui::Rect> {
    let inset = (TREEMAP_SELECTION_WIDTH + 2.0) * 0.5 + 0.5;
    let rect = tile.shrink(inset);
    (rect.width() > TREEMAP_SELECTION_WIDTH && rect.height() > TREEMAP_SELECTION_WIDTH)
        .then_some(rect)
}

pub(super) fn cushion_color(base: Color32, x: f32, y: f32) -> Color32 {
    let highlight = (1.0 - ((x - 0.34).powi(2) * 0.65 + (y - 0.26).powi(2) * 1.35)).clamp(0.0, 1.0);
    let edge = ((x - 0.5).abs() * 0.10 + (y - 0.5).abs() * 0.22).clamp(0.0, 0.16);
    let light = 0.04 + highlight * 0.13 - y * 0.12 - edge;
    if light >= 0.0 {
        blend(base, Color32::WHITE, light)
    } else {
        blend(base, Color32::BLACK, -light)
    }
}

pub(super) fn cushion_mesh(rect: egui::Rect, base: Color32) -> egui::Mesh {
    const GRID: usize = 5;
    let mut mesh = egui::Mesh::default();
    mesh.reserve_vertices(GRID * GRID);
    mesh.reserve_triangles((GRID - 1) * (GRID - 1) * 2);
    for row in 0..GRID {
        let y = row as f32 / (GRID - 1) as f32;
        for column in 0..GRID {
            let x = column as f32 / (GRID - 1) as f32;
            mesh.colored_vertex(
                egui::pos2(egui::lerp(rect.x_range(), x), egui::lerp(rect.y_range(), y)),
                cushion_color(base, x, y),
            );
        }
    }
    for row in 0..GRID - 1 {
        for column in 0..GRID - 1 {
            let top_left = (row * GRID + column) as u32;
            let top_right = top_left + 1;
            let bottom_left = top_left + GRID as u32;
            let bottom_right = bottom_left + 1;
            mesh.add_triangle(top_left, top_right, bottom_right);
            mesh.add_triangle(top_left, bottom_right, bottom_left);
        }
    }
    mesh
}

/// Below this many pixels on a side the cushion gradient spans too few
/// pixels to be distinguishable, so the tile is painted as a flat rect.
/// That matters at scale, not for looks: a full-drive treemap emits tens
/// of thousands of tiles, and the shaded mesh costs 25 vertices and 32
/// triangles each where a flat rect costs 4 and 2.
pub(super) const MIN_CUSHION_PX: f32 = 12.0;

/// Tiles smaller than this on a side get no grid outline; see the call
/// site in `draw_treemap` for why.
const MIN_GRID_PX: f32 = 5.0;

/// Breathing room above and below a tile's name inside its reserved
/// strip. Counted twice into the strip height so the text is not flush
/// against either the tile's top edge or its children.
pub(super) const LABEL_TEXT_PADDING: f32 = 2.0;

pub(super) fn paint_cushion_rect(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    if rect.width() < MIN_CUSHION_PX || rect.height() < MIN_CUSHION_PX {
        painter.rect_filled(rect, 0.0, cushion_color(base, 0.5, 0.5));
        return;
    }
    painter.add(egui::Shape::mesh(cushion_mesh(rect, base)));
}
