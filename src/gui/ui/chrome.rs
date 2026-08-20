//! The frame around the workspace: menu bar, toolbar, status bar.

use crate::gui::app::{size_label, FileView, GuiApp, PaneOrientation};
use crate::gui::icons::Icon;
use crate::util::thousands;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Stroke, Vec2};

use super::actions::*;
use super::theme::*;
use super::widgets::*;

pub(super) fn draw_menu_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar")
        .frame(
            Frame::none()
                .fill(APP_COLOR)
                .inner_margin(Margin::symmetric(8.0, 5.0))
                .stroke(Stroke::new(1.0_f32, BORDER_COLOR)),
        )
        .show(ctx, |ui| {
            // The top-level menu names sit in the tightest strip in the
            // window, so give them their own roomier padding rather than
            // inheriting the compact one the toolbar buttons want. The
            // gap between names matters as much as the padding inside
            // them: with too little, "Cleanup Treemap View" reads as one
            // run of words rather than three separate targets.
            ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
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
                ui.menu_button("Edit", |ui| {
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
                ui.menu_button("Cleanup", |ui| {
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
                        app.show_properties = true;
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
                ui.menu_button("Treemap", |ui| {
                    if menu_choice(
                        ui,
                        app.orientation == PaneOrientation::Horizontal,
                        "Horizontal — below",
                    )
                    .clicked()
                    {
                        app.orientation = PaneOrientation::Horizontal;
                        ui.close_menu();
                    }
                    if menu_choice(
                        ui,
                        app.orientation == PaneOrientation::Vertical,
                        "Vertical — right",
                    )
                    .clicked()
                    {
                        app.orientation = PaneOrientation::Vertical;
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
                    menu_toggle(ui, &mut app.show_grid, "Grid lines");
                    menu_toggle(ui, &mut app.show_labels, "File labels");
                    menu_toggle(ui, &mut app.show_free_space, "Free space");
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
                ui.menu_button("View", |ui| {
                    view_menu_item(app, ui, FileView::AllFiles);
                    view_menu_item(app, ui, FileView::LargestFiles);
                    if icon_button(ui, !app.is_busy(), Icon::Duplicate, "Duplicate Files").clicked()
                    {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                    view_menu_item(app, ui, FileView::SearchResults);
                    ui.separator();
                    menu_toggle(ui, &mut app.show_extension_view, "Extension list");
                    menu_toggle(ui, &mut app.show_treemap, "Treemap");
                    menu_toggle(ui, &mut app.show_toolbar, "Toolbar");
                    menu_toggle(ui, &mut app.show_status_bar, "Status bar");
                    ui.separator();
                    if icon_button(ui, true, Icon::Settings, "Settings…").clicked() {
                        app.show_settings = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if icon_button(ui, true, Icon::Tools, "Windows maintenance…").clicked() {
                        app.show_windows_tools = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::Duplicate, "Find duplicate files").clicked() {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if icon_button(ui, true, Icon::Help, "WinDirStat view guide").clicked() {
                        app.show_about = true;
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Info, "About RustDirStat").clicked() {
                        app.show_about = true;
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
                .fill(PANEL_COLOR)
                .inner_margin(Margin::symmetric(12.0, 9.0))
                .stroke(Stroke::new(1.0_f32, BORDER_COLOR)),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Toolbar buttons are icon-only, so they need more air
                // between them than text controls do to stay readable as
                // separate targets rather than one strip of glyphs.
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 8.0);
                paint_inline_icon(ui, Icon::App, 20.0, ACCENT_COLOR);
                ui.add_space(4.0);
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
                    app.show_properties = true;
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
                let orient = if app.orientation == PaneOrientation::Horizontal {
                    Icon::LayoutHorizontal
                } else {
                    Icon::LayoutVertical
                };
                if tool(
                    ui,
                    orient,
                    &format!("Layout: {} (click to switch)", app.orientation.label()),
                )
                .clicked()
                {
                    app.orientation.toggle();
                }
                if tool(ui, Icon::Settings, "Settings").clicked() {
                    app.show_settings = true;
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Tools, "Windows maintenance tools")
                    .clicked()
                {
                    app.show_windows_tools = true;
                }
                if ui.available_width() > 240.0 {
                    toolbar_separator(ui);
                    Frame::none()
                        .fill(APP_COLOR)
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(Margin::symmetric(9.0, 5.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(app.tree.root_path.display().to_string())
                                    .color(SECONDARY_TEXT_COLOR),
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
    ui.add_space(5.0);
    ui.separator();
    ui.add_space(5.0);
}

pub(super) fn draw_status_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            Frame::none()
                .fill(APP_COLOR)
                .inner_margin(Margin::symmetric(12.0, 5.0))
                .stroke(Stroke::new(1.0_f32, BORDER_COLOR)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if app.is_busy() {
                    ui.spinner();
                } else {
                    paint_inline_icon(ui, Icon::Info, 14.0, SECONDARY_TEXT_COLOR);
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
