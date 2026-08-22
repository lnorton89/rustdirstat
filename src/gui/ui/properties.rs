// ============================================================================
// Module:       gui::ui::properties
// Description:  The Properties inspector: the app's one modeless window, which
//               follows the selection while the rest of the app stays usable.
//
// Dependencies: eframe::egui; crate::gui::app::GuiApp, super::{theme, widgets}
// ============================================================================

//! The Properties inspector.
//!
//! Every other surface in this app is a page of one modal card (see
//! [`super::modal`]), and for settings, the guide, and the maintenance
//! tools that is right: they are things you open, do, and dismiss. This
//! one is not. It describes whatever is selected, and the natural way to
//! use it is to leave it open while clicking through the tree — which a
//! modal forbids, because it blocks the window behind a scrim.
//!
//! So Properties is the app's one *modeless* surface: movable, closable,
//! and remembered between runs. It is deliberately the only exception to
//! the one-modal rule, and it earns it by being the only surface here
//! that describes something the user is still busy changing.
//!
//! It follows the selection rather than capturing one. A rescan restores
//! the selection by name identity, so the inspector comes back describing
//! the same item without knowing anything about rescans; and an item that
//! vanished leaves it in its "nothing selected" state rather than
//! describing a stale path.

use crate::gui::app::GuiApp;
use crate::gui::icons::Icon;
use crate::util::{format_modified, human_bytes, thousands};
use eframe::egui::{self, RichText, Vec2};

use super::theme::*;
use super::widgets::*;

/// Where the window opens the first time, and how big it is allowed to be.
///
/// Offset from the top-left rather than centred: the point of this window
/// is to sit *beside* the file list, and a centred inspector covers the
/// rows the user is about to click.
const DEFAULT_POS: [f32; 2] = [80.0, 120.0];
const DEFAULT_WIDTH: f32 = 380.0;
const MAX_HEIGHT: f32 = 520.0;

/// Draws the inspector if it is open, and records where it ended up.
///
/// Takes the `Context` rather than a `Ui` because a floating window is
/// not part of the panel layout — the same reason [`super::modal`] does.
pub(super) fn draw_properties_window(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.properties.open {
        return;
    }
    let palette = palette();
    let mut open = true;
    let frame = egui::Frame::NONE
        .fill(palette.panel)
        .stroke(egui::Stroke::new(1.0_f32, palette.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(px(SPACE_MD)))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: egui::Color32::from_black_alpha(if palette.mode.is_dark() { 140 } else { 50 }),
        });

    let mut window = egui::Window::new("Properties")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .constrain(true)
        .default_width(DEFAULT_WIDTH)
        .max_height(MAX_HEIGHT)
        .frame(frame);
    // A remembered position beats egui's memory here: egui keys window
    // state by id within one run, and this has to survive a restart.
    window = match app.properties.pos {
        Some(pos) => window.current_pos(egui::pos2(pos[0], pos[1])),
        None => window.default_pos(egui::pos2(DEFAULT_POS[0], DEFAULT_POS[1])),
    };

    let response = window.show(ctx, |ui| {
        // Scrolls rather than growing: a long path or a deep folder's
        // counts must not push the window past the screen, which is the
        // failure the six dialogs this app replaced all shared.
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| draw_details(app, ui));
    });

    if let Some(response) = response {
        let at = response.response.rect.min;
        app.properties.pos = Some([at.x, at.y]);
        #[cfg(test)]
        super::probes::probe(&super::probes::TEST_PROPERTIES_RECTS).push(response.response.rect);
    }
    // `Window::open` writes the close button's answer here.
    app.properties.open = open;
}

/// The details themselves, which are just a read of the current selection.
fn draw_details(app: &GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    let Some((node, path)) = app.selected_node().zip(app.selected_fs_path()) else {
        empty_state(
            ui,
            Icon::Info,
            "Nothing selected",
            "Pick an item in the file list or the treemap, and its details appear here.",
        );
        return;
    };
    let rows = [
        ("Name", node.name.to_string_lossy().to_string()),
        ("Path", crate::util::display_path(&path)),
        (
            "Type",
            if node.is_dir { "Folder" } else { "File" }.to_string(),
        ),
        ("Logical size", human_bytes(node.size)),
        ("Physical size", human_bytes(node.physical_size)),
        ("Files", thousands(node.file_count)),
        ("Subdirectories", thousands(node.dir_count)),
        ("Last change", format_modified(node.modified)),
        ("Unreadable items", thousands(node.unreadable_count)),
    ];
    egui::Grid::new("properties_window_grid")
        .num_columns(2)
        .spacing(Vec2::new(SPACE_LG, SPACE_SM))
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(RichText::new(label).color(palette.secondary_text));
                ui.label(value);
                ui.end_row();
            }
        });
}
