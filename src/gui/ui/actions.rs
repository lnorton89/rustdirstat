// ============================================================================
// Module:       gui::ui::actions
// Description:  Every command the menus, toolbar, context menus, and keyboard
//               funnel into, plus keyboard shortcut dispatch.
//
// Dependencies: eframe::egui, rfd (native file dialogs);
//               crate::gui::app::GuiApp
// ============================================================================

//! The commands the menu bar, toolbar, context menus, and keyboard
//! all funnel into, plus the keyboard shortcut handling.
//!
//! Kept together so that a shortcut and the menu row that claims to
//! be its equivalent cannot drift apart.

use crate::gui::app::{FileView, GuiApp};
use eframe::egui::{self};

use super::modal::{dismiss_top, modal_is_open};

pub(super) fn handle_shortcuts(app: &mut GuiApp, ctx: &egui::Context) {
    let editing_text = ctx.egui_wants_keyboard_input();
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if escape && modal_is_open(app) {
        dismiss_top(app);
        return;
    }
    // Esc with nothing open stops a scan. It is the key every other
    // "stop what you are doing" in this app answers to, and a scan of a
    // whole volume is the one thing here long enough to want stopping.
    if escape && !editing_text && app.scan_is_running() {
        app.cancel_scan();
        return;
    }
    // A modal is modal for the keyboard too. Without this, pressing Del
    // while the "are you sure" was on screen queued a *second* delete
    // behind the one being confirmed, and F5 could swap the tree out from
    // under a pending deletion's index path.
    if modal_is_open(app) {
        return;
    }
    let (refresh_key, delete_key, shift_delete, open_dialog, search, copy) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::F5),
            i.key_pressed(egui::Key::Delete),
            i.modifiers.shift && i.key_pressed(egui::Key::Delete),
            i.modifiers.ctrl && i.key_pressed(egui::Key::O),
            i.modifiers.ctrl && i.key_pressed(egui::Key::F),
            i.modifiers.ctrl && i.key_pressed(egui::Key::C),
        )
    });
    let (zoom_in, zoom_out, reset_zoom, up, down, left, right, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            i.key_pressed(egui::Key::Minus),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Enter),
        )
    });
    if refresh_key {
        refresh(app);
    }
    if delete_key && !editing_text {
        // Shift+Del is the platform convention for bypassing the Recycle
        // Bin, and it arrives as a Delete press with the shift modifier —
        // so it has to be checked before the plain-Delete branch, or the
        // permanent variant is unreachable from the keyboard.
        app.request_delete_selected(shift_delete);
    }
    if open_dialog {
        choose_folder(app);
    }
    if search {
        app.file_view = FileView::SearchResults;
    }
    if copy && !editing_text {
        copy_path(app);
    }
    if !editing_text {
        if zoom_in {
            app.zoom_in();
        }
        if zoom_out {
            app.zoom_out();
        }
        if reset_zoom {
            app.reset_zoom();
        }
    }
    if !editing_text && app.file_view == FileView::AllFiles {
        app.refresh_visible_rows();
        let row_count = app.visible_rows.len();
        let current = app.selected_path.as_ref().and_then(|selected| {
            app.visible_rows
                .iter()
                .position(|row| &row.path == selected)
        });
        if (up || down) && row_count > 0 {
            let next = if up {
                current.unwrap_or(1).saturating_sub(1_usize)
            } else {
                (current.unwrap_or(0) + 1).min(row_count - 1)
            };
            let path = app.visible_rows[next].path.clone();
            app.select_path(path);
        }
        if let Some(path) = app.selected_path.clone() {
            let Some(node) = app.tree.node_for(&path) else {
                return;
            };
            let is_dir = node.is_dir;
            if right && is_dir && !app.expanded.contains(&path) {
                app.toggle_expanded(&path);
            }
            if left {
                if is_dir && app.expanded.contains(&path) {
                    app.toggle_expanded(&path);
                } else if !path.is_empty() {
                    app.select_path(path[..path.len() - 1].to_vec());
                }
            }
            if enter {
                if is_dir {
                    app.toggle_expanded(&path);
                } else {
                    open_selected(app);
                }
            }
        }
    }
}

pub(super) fn choose_folder(app: &mut GuiApp) {
    if let Some(path) = rfd::FileDialog::new()
        .set_directory(&app.tree.root_path)
        .pick_folder()
    {
        if let Err(e) = app.open_folder(&path) {
            app.status = Some(format!("Scan failed: {e}"));
        }
    }
}

pub(super) fn refresh(app: &mut GuiApp) {
    if let Err(e) = app.refresh_scan() {
        app.status = Some(format!("Refresh failed: {e}"));
    }
}

pub(super) fn open_selected(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        if let Err(e) = crate::util::open_path(&path) {
            app.status = Some(format!("Open failed: {e}"));
        }
    }
}

pub(super) fn reveal_selected(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };
        if let Err(e) = crate::util::open_in_file_manager(&target) {
            app.status = Some(format!("Explorer failed: {e}"));
        }
    }
}

pub(super) fn copy_path(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        let text = path.display().to_string();
        match crate::util::copy_to_clipboard(&text) {
            Ok(()) => app.status = Some(format!("Copied: {text}")),
            Err(e) => app.status = Some(format!("Copy failed: {e}")),
        }
    }
}

pub(super) fn export_csv(app: &mut GuiApp) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name("rustdirstat.csv")
        .save_file()
    {
        match crate::csv_export::write_csv_to_file(&app.tree.root_path, &app.tree.root, &path) {
            Ok(()) => app.status = Some(format!("Exported: {}", path.display())),
            Err(e) => app.status = Some(format!("Export failed: {e}")),
        }
    }
}
