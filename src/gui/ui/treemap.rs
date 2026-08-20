//! Paints the treemap panel from the tiles `GuiApp` has laid out.
//!
//! Layout itself lives in `gui::treemap_layout`; this module only
//! turns tiles into pixels, including the cushion shading that
//! gives nested directories their rounded look.

use crate::color;
use crate::gui::app::{extension_label, GuiApp};
use crate::gui::icons::Icon;
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextStyle};

use super::theme::*;
use super::widgets::*;

pub(super) fn draw_treemap(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        paint_inline_icon(ui, Icon::App, 19.0, ACCENT_COLOR);
        ui.heading("Treemap");
        ui.label(
            RichText::new(app.zoom_fs_path().display().to_string()).color(SECONDARY_TEXT_COLOR),
        );
        if ui.available_width() > 420.0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(if app.use_physical {
                        "Physical size"
                    } else {
                        "Logical size"
                    })
                    .small()
                    .color(SECONDARY_TEXT_COLOR),
                );
            });
        }
    });
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(5.0);
    let avail = ui.available_size();
    if avail.x <= 1.0 || avail.y <= 1.0 {
        return;
    }
    let (response, painter) = ui.allocate_painter(avail, Sense::click());
    // Tiles are only re-laid-out when the panel rect or the tree behind
    // it changes; on a large volume the layout walk is far too expensive
    // to redo for every frame of a hover.
    app.refresh_treemap(response.rect.min.x, response.rect.min.y, avail.x, avail.y);
    let mut clicked = None;
    // Painting borrows the cached tiles out of `app`, so the click it
    // records is applied once that borrow has ended.
    {
        let app = &*app;
        let tiles = &app.treemap_tiles;
        let mut selected_rect = None;
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
                Color32::from_rgb(96, 102, 114)
            } else if tile.is_dir {
                to_color32(color::directory_color())
            } else {
                extension_color(&extension_label(&tile.name))
            };
            let mut base = scale(raw, 1.0 - (tile.depth as f32 * 0.05).min(0.35));
            if tile.is_node() {
                if let Some(ext) = &app.highlighted_extension {
                    if tile.is_dir || extension_label(&tile.name) != *ext {
                        base = blend(base, Color32::from_rgb(49, 52, 60), 0.78);
                    }
                } else if let Some(category) = app.highlighted_category {
                    if tile.is_dir || tile.category != Some(category) {
                        base = blend(base, Color32::from_rgb(49, 52, 60), 0.78);
                    }
                }
            }
            paint_cushion_rect(&painter, rect, base);
            if app.show_grid {
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0_f32, Color32::from_rgb(14, 15, 18)),
                );
            }
            if app.selected_path.as_ref() == Some(&tile.index_path) {
                selected_rect = treemap_selection_rect(rect);
            }
            if app.show_labels && tile.can_label && tile.w >= 48.0 && tile.h >= 16.0 {
                painter.text(
                    rect.min + egui::vec2(4.0, 3.0),
                    egui::Align2::LEFT_TOP,
                    truncate_for_width(&tile.name, tile.w - 8.0, &painter, ui),
                    TextStyle::Small.resolve(ui.style()),
                    readable_text_color(base),
                );
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
        // Paint selection last. Otherwise tiles rendered later overwrite the
        // shared right and bottom edges of the selected tile.
        if let Some(rect) = selected_rect {
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(
                    TREEMAP_SELECTION_WIDTH + 2.0,
                    Color32::from_black_alpha(190),
                ),
            );
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(TREEMAP_SELECTION_WIDTH, Color32::WHITE),
            );
        }
    }
    if let Some(path) = clicked {
        app.navigate_to_absolute(path);
    }
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

pub(super) fn paint_cushion_rect(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    if rect.width() < MIN_CUSHION_PX || rect.height() < MIN_CUSHION_PX {
        painter.rect_filled(rect, 0.0, cushion_color(base, 0.5, 0.5));
        return;
    }
    painter.add(egui::Shape::mesh(cushion_mesh(rect, base)));
}
