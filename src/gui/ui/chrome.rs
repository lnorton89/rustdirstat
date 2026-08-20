// ============================================================================
// Module:       gui::ui::chrome
// Description:  The frame around the workspace: menu bar, toolbar, and status
//               bar.
//
// Dependencies: eframe::egui; crate::gui::app::GuiApp, super::{actions, theme,
//               widgets}
// ============================================================================

//! The frame around the workspace: menu bar, toolbar, status bar.

use super::modal::ModalPage;
use crate::gui::app::{size_label, FileView, GuiApp, PaneOrientation};
use crate::gui::icons::Icon;
use crate::util::thousands;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Stroke, Vec2};

use super::actions::*;
use super::theme::*;
use super::widgets::*;

/// Padding inside each top-level menu name, and the gap between them.
///
/// The bar is the tightest strip in the window, and egui's own menu style
/// is built for a compact look that leaves the names running into each
/// other. `menu_bar_names_are_clearly_separated` pins the resulting
/// on-screen gap so this cannot silently regress again.
pub(super) const MENU_BAR_BUTTON_PADDING: Vec2 = Vec2::new(12.0, 9.0);
pub(super) const MENU_BAR_ITEM_GAP: f32 = 10.0;

/// A menu bar's highlight is square and fills the bar.
///
/// The rest of the window rounds its widgets by 6, which under a
/// top-level menu name paints a pill floating in the middle of the bar —
/// it reads as a button someone dropped there rather than as the bar
/// responding. Squaring it, and letting it run the full height of the
/// strip, is what every desktop menu bar does and is why they read as
/// bars.
const MENU_BAR_ROUNDING: egui::Rounding = egui::Rounding::ZERO;
/// Floors the test measures against. Deliberately below what is
/// configured above, so ordinary tuning does not trip them, but far above
/// what egui's `set_menu_style` leaves behind (2px padding, 0 gap) — the
/// state this whole arrangement exists to prevent.
#[cfg(test)]
pub(super) const MENU_BAR_MIN_SIDE_PADDING: f32 = 8.0;
#[cfg(test)]
pub(super) const MENU_BAR_MIN_GAP: f32 = 6.0;

/// A top-level menu name, recording where it landed so the layout test
/// can measure the real gaps rather than trusting the spacing settings to
/// have survived egui's own menu styling.
fn menu_bar_button<R>(
    ui: &mut egui::Ui,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    #[cfg(test)]
    super::probes::probe(&super::probes::TEST_MENU_BAR_ROUNDING)
        .push((label.to_owned(), ui.visuals().widgets.hovered.rounding.nw));
    let response = ui.menu_button(label, add_contents);
    #[cfg(test)]
    super::probes::probe(&super::probes::TEST_MENU_BAR_RECTS)
        .push((label.to_owned(), response.response.rect));
    response
}

/// Squares off the hover, open, and pressed backgrounds for the bar.
///
/// Has to be applied to the child `Ui` *inside* `menu::bar`, for the same
/// reason the padding does: `set_menu_style` runs first and overwrites
/// whatever was configured on the way in.
fn square_off_menu_bar(ui: &mut egui::Ui) {
    let widgets = &mut ui.visuals_mut().widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.rounding = MENU_BAR_ROUNDING;
    }
}

pub(super) fn draw_menu_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar")
        .frame(
            Frame::none()
                .fill(palette().app)
                // No vertical margin: the highlight under a menu name is
                // the button's own background, so anything the frame adds
                // above and below shows as a gap the highlight cannot
                // reach — which is what made it look like a floating pill
                // instead of part of the bar. The height comes from the
                // button padding below instead.
                .inner_margin(Margin::symmetric(SPACE_XS, 0.0))
                .stroke(Stroke::new(1.0_f32, palette().border)),
        )
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // These have to be set *inside* the bar, not before it.
                // `menu::bar` runs egui's `set_menu_style` on the child
                // Ui as its first act, which hard-codes button_padding to
                // (2, 0) — so anything configured on the way in is
                // discarded, and the names come out jammed together with
                // no indication why.
                ui.spacing_mut().button_padding = MENU_BAR_BUTTON_PADDING;
                ui.spacing_mut().item_spacing.x = MENU_BAR_ITEM_GAP;
                square_off_menu_bar(ui);
                menu_bar_button(ui, "File", |ui| {
                    if menu_action(
                        ui,
                        !app.is_busy(),
                        Icon::FolderOpen,
                        "Select folder…",
                        "Ctrl+O",
                    )
                    .clicked()
                    {
                        choose_folder(app);
                        ui.close_menu();
                    }
                    if menu_action(ui, !app.is_busy(), Icon::Refresh, "Rescan", "F5").clicked() {
                        refresh(app);
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::Export, "Export CSV…").clicked() {
                        export_csv(app);
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::ExternalLink, "Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                menu_bar_button(ui, "Edit", |ui| {
                    if menu_action(
                        ui,
                        app.selected_path.is_some(),
                        Icon::Copy,
                        "Copy path",
                        "Ctrl+C",
                    )
                    .clicked()
                    {
                        copy_path(app);
                        ui.close_menu();
                    }
                    if menu_action(ui, true, Icon::Search, "Search…", "Ctrl+F").clicked() {
                        app.file_view = FileView::SearchResults;
                        ui.close_menu();
                    }
                });
                menu_bar_button(ui, "Cleanup", |ui| {
                    let selected = app.selected_path.is_some();
                    if icon_button(ui, selected, Icon::ExternalLink, "Open").clicked() {
                        open_selected(app);
                        ui.close_menu();
                    }
                    if icon_button(ui, selected, Icon::Folder, "Show in Explorer").clicked() {
                        reveal_selected(app);
                        ui.close_menu();
                    }
                    if icon_button(ui, selected, Icon::Info, "Properties").clicked() {
                        app.open_modal(ModalPage::Properties);
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_action(ui, selected, Icon::Trash, "Delete to Recycle Bin", "Del")
                        .clicked()
                    {
                        app.request_delete_selected(false);
                        ui.close_menu();
                    }
                    if menu_action(ui, selected, Icon::Trash, "Delete permanently", "Shift+Del")
                        .clicked()
                    {
                        app.request_delete_selected(true);
                        ui.close_menu();
                    }
                });
                menu_bar_button(ui, "Treemap", |ui| {
                    if menu_choice(
                        ui,
                        app.view.orientation == PaneOrientation::Horizontal,
                        "Horizontal — below",
                    )
                    .clicked()
                    {
                        app.view.orientation = PaneOrientation::Horizontal;
                        ui.close_menu();
                    }
                    if menu_choice(
                        ui,
                        app.view.orientation == PaneOrientation::Vertical,
                        "Vertical — right",
                    )
                    .clicked()
                    {
                        app.view.orientation = PaneOrientation::Vertical;
                        ui.close_menu();
                    }
                    ui.separator();
                    if menu_choice(ui, !app.use_physical, "Logical size").clicked() {
                        app.use_physical = false;
                        app.refresh_extensions();
                    }
                    if menu_choice(ui, app.use_physical, "Physical size").clicked() {
                        app.use_physical = true;
                        app.refresh_extensions();
                    }
                    ui.separator();
                    menu_toggle(ui, &mut app.view.grid, "Grid lines");
                    menu_toggle(ui, &mut app.view.labels, "File labels");
                    menu_toggle(ui, &mut app.view.free_space, "Free space");
                    ui.separator();
                    if menu_action(ui, true, Icon::ZoomIn, "Zoom in", "+").clicked() {
                        app.zoom_in();
                        ui.close_menu();
                    }
                    if menu_action(ui, true, Icon::ZoomOut, "Zoom out", "-").clicked() {
                        app.zoom_out();
                        ui.close_menu();
                    }
                    if menu_action(ui, true, Icon::Home, "Reset zoom", "Home").clicked() {
                        app.reset_zoom();
                        ui.close_menu();
                    }
                });
                menu_bar_button(ui, "View", |ui| {
                    view_menu_item(app, ui, FileView::AllFiles);
                    view_menu_item(app, ui, FileView::LargestFiles);
                    if icon_button(ui, !app.is_busy(), Icon::Duplicate, "Duplicate Files").clicked()
                    {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                    view_menu_item(app, ui, FileView::SearchResults);
                    ui.separator();
                    menu_toggle(ui, &mut app.view.extension_pane, "Extension list");
                    menu_toggle(ui, &mut app.view.treemap, "Treemap");
                    menu_toggle(ui, &mut app.view.toolbar, "Toolbar");
                    menu_toggle(ui, &mut app.view.status_bar, "Status bar");
                    ui.separator();
                    if icon_button(ui, true, Icon::Palette, "Appearance…").clicked() {
                        app.open_modal(ModalPage::Appearance);
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Settings, "Settings…").clicked() {
                        app.open_modal(ModalPage::Views);
                        ui.close_menu();
                    }
                });
                menu_bar_button(ui, "Tools", |ui| {
                    if icon_button(ui, true, Icon::Tools, "Windows maintenance…").clicked() {
                        app.open_modal(ModalPage::Maintenance);
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::Duplicate, "Find duplicate files").clicked() {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                });
                menu_bar_button(ui, "Help", |ui| {
                    // These used to open the same window as each other.
                    if icon_button(ui, true, Icon::Help, "View guide").clicked() {
                        app.open_modal(ModalPage::Guide);
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Info, "About RustDirStat").clicked() {
                        app.open_modal(ModalPage::About);
                        ui.close_menu();
                    }
                });
            });
        });
}

pub(super) fn view_menu_item(app: &mut GuiApp, ui: &mut egui::Ui, view: FileView) {
    if icon_selectable_label(ui, app.file_view == view, view_icon(view), view.label()).clicked() {
        app.file_view = view;
        ui.close_menu();
    }
}

pub(super) fn draw_toolbar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("toolbar")
        .frame(
            Frame::none()
                .fill(palette().panel)
                .inner_margin(Margin::symmetric(PAD, SPACE_SM))
                .stroke(Stroke::new(1.0_f32, palette().border)),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Toolbar buttons are icon-only, so they need more air
                // between them than text controls do to stay readable as
                // separate targets rather than one strip of glyphs.
                ui.spacing_mut().item_spacing = Vec2::new(6.0, SPACE_SM);
                paint_inline_icon(ui, Icon::App, 20.0, palette().accent);
                ui.add_space(SPACE_XS);
                ui.label(RichText::new("RustDirStat").strong().size(15.0));
                toolbar_separator(ui);
                if tool_enabled(ui, !app.is_busy(), Icon::FolderOpen, "Select folder").clicked() {
                    choose_folder(app);
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Refresh, "Rescan (F5)").clicked() {
                    refresh(app);
                }
                toolbar_separator(ui);
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::ExternalLink,
                    "Open selected",
                )
                .clicked()
                {
                    open_selected(app);
                }
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::Folder,
                    "Show in Explorer",
                )
                .clicked()
                {
                    reveal_selected(app);
                }
                if tool_enabled(ui, app.selected_path.is_some(), Icon::Copy, "Copy path").clicked()
                {
                    copy_path(app);
                }
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::Trash,
                    "Delete to Recycle Bin",
                )
                .clicked()
                {
                    app.request_delete_selected(false);
                }
                if tool_enabled(ui, app.selected_path.is_some(), Icon::Info, "Properties").clicked()
                {
                    app.open_modal(ModalPage::Properties);
                }
                toolbar_separator(ui);
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::ZoomIn,
                    "Zoom treemap to selection",
                )
                .clicked()
                {
                    app.zoom_in();
                }
                if tool_enabled(ui, !app.zoom_path.is_empty(), Icon::ZoomOut, "Zoom out").clicked()
                {
                    app.zoom_out();
                }
                if tool(ui, Icon::Home, "Reset treemap zoom").clicked() {
                    app.reset_zoom();
                }
                toolbar_separator(ui);
                let orient = if app.view.orientation == PaneOrientation::Horizontal {
                    Icon::LayoutHorizontal
                } else {
                    Icon::LayoutVertical
                };
                if tool(
                    ui,
                    orient,
                    &format!("Layout: {} (click to switch)", app.view.orientation.label()),
                )
                .clicked()
                {
                    app.view.orientation.toggle();
                }
                if tool(ui, Icon::Palette, "Appearance and theme").clicked() {
                    app.open_modal(ModalPage::Appearance);
                }
                if tool(ui, Icon::Settings, "Settings").clicked() {
                    app.open_modal(ModalPage::Views);
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Tools, "Windows maintenance tools")
                    .clicked()
                {
                    app.open_modal(ModalPage::Maintenance);
                }
                if ui.available_width() > 240.0 {
                    toolbar_separator(ui);
                    Frame::none()
                        .fill(palette().app)
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(Margin::symmetric(SPACE_SM, SPACE_XS))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(crate::util::display_name(&app.tree.root_path))
                                    .color(palette().secondary_text),
                            );
                        });
                }
            });
        });
}

/// A toolbar group divider. The extra air on either side is what makes
/// the icon-only buttons read as a few labelled groups rather than one
/// undifferentiated row of glyphs; a bare `ui.separator()` inherits the
/// same tight item spacing as the buttons and does not separate anything.
pub(super) fn toolbar_separator(ui: &mut egui::Ui) {
    ui.add_space(SPACE_XS);
    ui.separator();
    ui.add_space(SPACE_XS);
}

pub(super) fn draw_status_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            Frame::none()
                .fill(palette().app)
                .inner_margin(Margin::symmetric(PAD, SPACE_XS + 1.0))
                .stroke(Stroke::new(1.0_f32, palette().border)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if app.is_busy() {
                    ui.spinner();
                } else {
                    paint_inline_icon(ui, Icon::Info, 14.0, palette().secondary_text);
                }
                ui.label(
                    app.busy_text()
                        .as_deref()
                        .or(app.status.as_deref())
                        .unwrap_or("Ready"),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let node = app.zoom_node();
                    ui.label(format!(
                        "{} files · {} folders · {}",
                        thousands(node.file_count),
                        thousands(node.dir_count),
                        size_label(node.effective_size(app.use_physical), app.use_physical)
                    ));
                });
            });
        });
}
