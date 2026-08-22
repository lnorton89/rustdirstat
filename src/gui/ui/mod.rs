// ============================================================================
// Module:       gui::ui
// Description:  The egui drawing code, split by the region of the window each
//               submodule paints; draw is the per-frame entry point.
//
// Dependencies: eframe::egui; crate::gui::app::GuiApp
// ============================================================================

//! The egui front end, split by what part of the window each
//! module paints.
//!
//! Everything here is immediate mode: [`draw`] runs top to bottom
//! once per frame and rebuilds the whole window from [`GuiApp`],
//! which owns all the state. Nothing in this subtree keeps state of
//! its own between frames, so a module can be read in isolation --
//! what it paints is a pure function of the app it is handed.
//!
//! The one thing to know before editing: because every frame is a
//! full rebuild, anything expensive that is proportional to the
//! size of the scanned tree must be cached on `GuiApp` rather than
//! recomputed here. See `GuiApp::refresh_visible_rows` and
//! `GuiApp::refresh_treemap`.

use crate::gui::app::{FileView, GuiApp, PaneOrientation};
use eframe::egui::{self, Frame, Margin, RichText, Stroke};

pub(super) use self::modal::{install_backdrop, ModalPage};
pub(super) use self::theme::Palette;
pub(super) use self::themes::{default_theme_id, palette_for};

mod actions;
mod categories;
mod chrome;
mod directory;
mod extensions;
mod lists;
mod modal;
mod pages;
#[cfg(test)]
pub(in crate::gui) mod probes;
mod properties;
#[cfg(test)]
mod tests;
mod theme;
mod themes;
mod treemap;
mod widgets;

use self::actions::*;
use self::chrome::*;
use self::directory::*;
use self::extensions::*;
use self::lists::*;
use self::modal::*;
use self::theme::*;
use self::treemap::*;

pub(super) fn draw(app: &mut GuiApp, ui: &mut egui::Ui) {
    apply_style(ui.ctx(), app.palette);
    draw_menu_bar(app, ui);
    if app.view.toolbar {
        draw_toolbar(app, ui);
    }
    if app.view.status_bar {
        draw_status_bar(app, ui);
    }
    draw_workspace(app, ui);
    // The inspector is drawn before the modal so a modal opened on top
    // of it covers it, which is what a scrim is for.
    properties::draw_properties_window(app, ui.ctx());
    draw_modal(app, ui.ctx());
    handle_shortcuts(app, ui.ctx());
}

pub(super) fn draw_workspace(app: &mut GuiApp, ui: &mut egui::Ui) {
    match (app.view.treemap, app.view.orientation) {
        (true, PaneOrientation::Horizontal) => {
            egui::Panel::bottom("treemap_horizontal")
                .resizable(true)
                .default_size(280.0)
                .min_size(0.0)
                .frame(panel_frame())
                .show(ui, |ui| draw_treemap(app, ui));
            draw_upper_workspace(app, ui, true);
        }
        (true, PaneOrientation::Vertical) => {
            let half = ui.available_rect_before_wrap().width() * 0.48;
            egui::Panel::right("treemap_vertical")
                .resizable(true)
                .default_size(half)
                .min_size(0.0)
                .frame(panel_frame())
                .show(ui, |ui| draw_treemap(app, ui));
            draw_upper_workspace(app, ui, false);
        }
        (false, _) => {
            draw_upper_workspace(app, ui, app.view.orientation == PaneOrientation::Horizontal)
        }
    }
}

pub(super) fn draw_upper_workspace(app: &mut GuiApp, ui: &mut egui::Ui, extension_on_right: bool) {
    if app.view.extension_pane {
        if extension_on_right {
            egui::Panel::right("extension_right")
                .resizable(true)
                .default_size(430.0)
                .min_size(0.0)
                .frame(panel_frame())
                .show(ui, |ui| draw_extension_list(app, ui));
        } else {
            egui::Panel::bottom("extension_bottom")
                .resizable(true)
                .default_size(220.0)
                .min_size(0.0)
                .frame(panel_frame())
                .show(ui, |ui| draw_extension_list(app, ui));
        }
    }
    egui::CentralPanel::default()
        .frame(panel_frame())
        .show(ui, |ui| draw_file_area(app, ui));
}

pub(super) fn draw_file_area(app: &mut GuiApp, ui: &mut egui::Ui) {
    if let Some(message) = app.busy_text() {
        Frame::NONE
            .fill(palette().accent_muted)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(Margin::symmetric(px(PAD), px(SPACE_SM)))
            .stroke(Stroke::new(1.0_f32, palette().accent))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(message).strong());
                    // Two different promises, and saying the wrong one is
                    // worse than saying nothing: until the first folder
                    // is published the previous scan is still what is on
                    // screen, and after it the window is showing the new
                    // one filling in.
                    let note = if app.live_scan {
                        "Folders appear as they finish."
                    } else {
                        "You can keep browsing the current scan."
                    };
                    ui.label(RichText::new(note).color(palette().secondary_text));
                    // Only a scan can be cancelled, so the button appears
                    // only for one. Duplicate hashing has its own control
                    // and the maintenance tools are someone else's
                    // process, which this app does not get to kill.
                    if app.scan_is_running() {
                        let cancel = ui.button("Cancel scan");
                        #[cfg(test)]
                        probes::probe(&probes::TEST_SCAN_CANCEL_RECTS).push(cancel.rect);
                        if cancel.clicked() {
                            app.cancel_scan();
                        }
                    }
                });
            });
        ui.add_space(SPACE_SM);
    }
    match app.file_view {
        FileView::AllFiles => draw_directory_tree(app, ui),
        FileView::LargestFiles => draw_largest_files(app, ui),
        FileView::DuplicateFiles => draw_duplicates(app, ui),
        FileView::SearchResults => draw_search(app, ui),
    }
}
