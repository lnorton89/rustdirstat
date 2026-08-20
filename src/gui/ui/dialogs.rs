//! Modal dialogs. Each one draws only when its `show_*` flag on
//! [`GuiApp`] is set, and clears the flag itself when dismissed.

use crate::gui::app::{GuiApp, PaneOrientation};
use crate::gui::icons::Icon;
use crate::util::{format_modified, human_bytes, thousands};
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Stroke, Vec2};

use super::theme::*;
use super::widgets::*;

pub(super) fn draw_delete_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    let Some(pending) = &app.pending_delete else {
        return;
    };
    let (name, permanent, is_dir) = (pending.name.clone(), pending.permanent, pending.is_dir);
    let mut confirm = false;
    let mut empty = false;
    let mut cancel = false;
    egui::Window::new(if permanent {
        "Permanently delete"
    } else {
        "Move to Recycle Bin"
    })
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
    .show(ctx, |ui| {
        ui.set_min_width(420.0);
        icon_heading(ui, Icon::Trash, &format!("Delete “{name}”?"));
        ui.add_space(6.0);
        if permanent {
            Frame::none()
                .fill(Color32::from_rgb(68, 31, 37))
                .rounding(egui::Rounding::same(7.0))
                .inner_margin(Margin::symmetric(10.0, 8.0))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(157, 61, 72)))
                .show(ui, |ui| {
                    ui.colored_label(Color32::from_rgb(255, 175, 181), "This cannot be undone.");
                });
        } else {
            ui.label("The item can be restored from the Recycle Bin.");
        }
        if is_dir {
            ui.label("Empty keeps the folder and removes its contents.");
        }
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            let delete = if permanent {
                danger_button(ui, "Delete permanently")
            } else {
                accent_button(ui, "Move to Recycle Bin")
            };
            if delete.clicked() {
                confirm = true;
            }
            if is_dir && ui.button("Empty folder").clicked() {
                empty = true;
            }
        });
    });
    if confirm {
        if let Err(e) = app.confirm_delete() {
            app.status = Some(format!("Delete failed: {e}"));
        }
    }
    if empty {
        if let Err(e) = app.confirm_empty() {
            app.status = Some(format!("Empty failed: {e}"));
        }
    }
    if cancel {
        app.pending_delete = None;
    }
}

pub(super) fn draw_properties_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_properties {
        return;
    }
    let mut open = true;
    egui::Window::new("Properties")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            icon_heading(ui, Icon::Info, "Item details");
            ui.add_space(6.0);
            if let (Some(node), Some(path)) = (app.selected_node(), app.selected_fs_path()) {
                egui::Grid::new("properties_grid")
                    .spacing(Vec2::new(18.0, 8.0))
                    .show(ui, |ui| {
                        property(ui, "Name", &node.name);
                        property(ui, "Path", &path.display().to_string());
                        property(ui, "Type", if node.is_dir { "Folder" } else { "File" });
                        property(ui, "Logical size", &human_bytes(node.size));
                        property(ui, "Physical size", &human_bytes(node.physical_size));
                        property(ui, "Files", &thousands(node.file_count));
                        property(ui, "Subdirectories", &thousands(node.dir_count));
                        property(ui, "Last change", &format_modified(node.modified));
                        property(ui, "Unreadable items", &thousands(node.unreadable_count));
                    });
            } else {
                ui.label("Select an item first.");
            }
        });
    if !open {
        app.show_properties = false;
    }
}

pub(super) fn property(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).color(SECONDARY_TEXT_COLOR));
    ui.label(value);
    ui.end_row();
}

pub(super) fn draw_settings_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            settings_group(ui, Icon::Settings, "Layout", |ui| {
                ui.radio_value(
                    &mut app.orientation,
                    PaneOrientation::Horizontal,
                    "Horizontal: treemap below the lists",
                );
                ui.radio_value(
                    &mut app.orientation,
                    PaneOrientation::Vertical,
                    "Vertical: treemap to the right",
                );
            });
            ui.add_space(8.0);
            settings_group(ui, Icon::Tree, "Views", |ui| {
                ui.checkbox(&mut app.show_extension_view, "Show extension list");
                ui.checkbox(&mut app.show_treemap, "Show treemap");
                ui.checkbox(&mut app.show_toolbar, "Show toolbar");
                ui.checkbox(&mut app.show_status_bar, "Show status bar");
            });
            ui.add_space(8.0);
            settings_group(ui, Icon::App, "Treemap", |ui| {
                ui.checkbox(&mut app.show_grid, "Grid lines");
                ui.checkbox(&mut app.show_labels, "File labels");
                ui.checkbox(
                    &mut app.show_free_space,
                    "Free-space tile for whole-drive scans",
                );
            });
        });
    if !open {
        app.show_settings = false;
    }
}

pub(super) fn draw_windows_tools_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_windows_tools {
        return;
    }
    let mut open = true;
    let mut selected = None;
    egui::Window::new("Windows maintenance")
        .open(&mut open)
        .resizable(true)
        .default_width(590.0)
        .show(ctx, |ui| {
            icon_heading(ui, Icon::Tools, "Storage and system tools");
            ui.label(
                "Launch built-in Windows maintenance tools for the volume containing this scan.",
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(440.0)
                .show(ui, |ui| {
                    for (index, tool) in crate::wintools::TOOLS.iter().enumerate() {
                        Frame::none()
                            .fill(RAISED_COLOR)
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(Margin::same(12.0))
                            .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(tool.name).strong());
                                        ui.label(
                                            RichText::new(tool.description)
                                                .color(SECONDARY_TEXT_COLOR),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let label = if tool.destructive {
                                            "Review…"
                                        } else {
                                            "Launch"
                                        };
                                        if ui
                                            .add_enabled(!app.is_busy(), egui::Button::new(label))
                                            .clicked()
                                        {
                                            selected = Some(index);
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
            if !cfg!(windows) {
                ui.colored_label(
                    Color32::LIGHT_YELLOW,
                    "These tools are available on Windows only.",
                );
            }
        });
    if let Some(index) = selected {
        app.request_windows_tool(index);
    }
    if !open {
        app.show_windows_tools = false;
    }
}

pub(super) fn draw_windows_tool_confirmation(app: &mut GuiApp, ctx: &egui::Context) {
    let Some(index) = app.pending_windows_tool else {
        return;
    };
    let Some(tool) = crate::wintools::TOOLS.get(index) else {
        app.pending_windows_tool = None;
        return;
    };
    let (name, description) = (tool.name, tool.description);
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new("Confirm maintenance action")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(470.0);
            icon_heading(ui, Icon::Trash, name);
            ui.label(description);
            ui.add_space(8.0);
            ui.colored_label(
                Color32::LIGHT_RED,
                "This operation may remove data and cannot be undone.",
            );
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if danger_button(ui, "Run action").clicked() {
                    confirm = true;
                }
            });
        });
    if confirm {
        app.confirm_windows_tool();
    }
    if cancel {
        app.pending_windows_tool = None;
    }
}

pub(super) fn draw_about_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }
    let mut open = true;
    egui::Window::new("WinDirStat views in RustDirStat")
        .open(&mut open)
        .resizable(true)
        .default_width(620.0)
        .show(ctx, |ui| {
            icon_heading(ui, Icon::Help, "Three coupled core views");
            ui.label("All Files is the expandable, size-sorted directory tree. Selecting an item frames the same item in the treemap.");
            ui.label("Extensions groups files by exact extension, with color, bytes, percentage, and count. Selecting a row highlights matching treemap files.");
            ui.label("Treemap represents every file as an area proportional to size, nests directory areas, uses extension colors, and selects the matching tree path when clicked.");
            ui.add_space(10.0);
            icon_heading(ui, Icon::Largest, "Additional file views");
            ui.label("Largest Files is a flat top-200 size view. Duplicate Files groups byte-identical files by hash. Search Results supports glob patterns and re: regular expressions.");
            ui.add_space(10.0);
            ui.label("All splitters are resizable to zero. The toolbar or Treemap menu switches between below/right orientations.");
        });
    if !open {
        app.show_about = false;
    }
}
