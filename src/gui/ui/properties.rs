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

/// The details.
///
/// Deliberately *not* the columns again. The directory grid already shows
/// name, size, files, subdirectories, last change and both percentages,
/// and an inspector that repeated them would be a second copy of the row
/// that is already highlighted. What is here is the things a grid cannot
/// hold:
///
/// - **The full path**, which a tree of names does not have room for.
/// - **Both sizes at once, and the gap between them.** A column shows
///   whichever mode is selected; the difference between logical and
///   on-disk is the interesting number and it is never on screen.
/// - **What the filesystem says right now** — link count, created and
///   accessed times, attributes. None of it is stored per node, because
///   nine million nodes cannot each carry it; one selected item can be
///   asked directly, which is the whole reason an inspector is worth
///   having.
/// - **Share of the volume**, not just of the scan.
/// - **What a folder is made of**, from the category totals every
///   directory already carries and nothing else surfaces per folder.
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
    let facts = app.selected_item_facts();

    section(ui, "Identity", |ui| {
        row(ui, "Name", node.name.to_string_lossy().to_string());
        row(ui, "Path", crate::util::display_path(&path));
        row(
            ui,
            "Type",
            if node.is_symlink {
                "Link".to_string()
            } else if node.is_dir {
                "Folder".to_string()
            } else {
                node.category
                    .map(|category| category.label().to_string())
                    .unwrap_or_else(|| "File".to_string())
            },
        );
        if let Some(links) = facts.link_count.filter(|count| *count > 1) {
            // The one fact that changes what a size *means*: these bytes
            // are reachable through another path, and counted again
            // under it.
            row(ui, "Names", format!("{links} hard links to this file"));
        }
        if !facts.attributes.is_empty() {
            row(ui, "Attributes", facts.attributes.join(", "));
        } else if facts.read_only {
            row(ui, "Attributes", "read-only".to_string());
        }
    });

    section(ui, "Size", |ui| {
        row(ui, "Logical", human_bytes(node.size));
        row(ui, "On disk", human_bytes(node.physical_size));
        // The gap, named. Positive is slack — the tail of the last
        // cluster, which a folder of tiny files can double; negative
        // means the filesystem is storing it in less space than it
        // occupies logically, which is compression or a sparse file.
        let (label, value) = if node.physical_size >= node.size {
            ("Slack", node.physical_size - node.size)
        } else {
            ("Saved", node.size - node.physical_size)
        };
        if value > 0 {
            row(ui, label, human_bytes(value));
        }
        for (label, share) in app.selection_shares() {
            row(ui, label, format!("{share:.1}%"));
        }
    });

    if node.is_dir {
        section(ui, "Contents", |ui| {
            row(ui, "Files", thousands(node.file_count));
            row(ui, "Subfolders", thousands(node.dir_count));
            if node.unreadable_count > 0 {
                row(ui, "Unreadable", thousands(node.unreadable_count));
            }
            if node.file_count > 0 {
                row(
                    ui,
                    "Average file",
                    human_bytes(node.size / node.file_count.max(1)),
                );
            }
            // Straight off the node: every directory carries its own
            // category totals from scan time, and nothing else in the
            // window shows them *per folder* — the extensions pane is
            // about the whole scan or the current zoom.
            for (label, bytes) in top_categories(node, app.use_physical) {
                row(ui, label, human_bytes(bytes));
            }
        });
    }

    section(ui, "Times", |ui| {
        row(ui, "Modified", format_modified(node.modified));
        if let Some(created) = facts.created {
            row(ui, "Created", format_modified(Some(created)));
        }
        if let Some(accessed) = facts.accessed {
            row(ui, "Accessed", format_modified(Some(accessed)));
        }
    });

    ui.add_space(SPACE_XS);
    ui.label(
        RichText::new("Times and attributes are read from disk when the selection changes.")
            .color(palette.secondary_text)
            .small(),
    );
}

/// The three biggest categories inside a folder, by the size mode in use.
fn top_categories(node: &crate::model::Node, physical: bool) -> Vec<(&'static str, u64)> {
    let mut rows: Vec<(&'static str, u64)> = crate::color::Category::ALL
        .iter()
        .filter_map(|category| {
            let (size, physical_size, _) = *node.ext_totals.get(category.index())?;
            let bytes = if physical { physical_size } else { size };
            (bytes > 0).then_some((category.label(), bytes))
        })
        .collect();
    rows.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    rows.truncate(3);
    rows
}

/// A titled block, matching the modal's own grouping.
fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(SPACE_SM);
    ui.label(RichText::new(title).strong());
    section_rule(ui);
    ui.add_space(SPACE_XS);
    egui::Grid::new(egui::Id::new("properties_grid").with(title))
        .num_columns(2)
        .spacing(Vec2::new(SPACE_LG, SPACE_SM))
        .show(ui, add_contents);
}

/// One label/value pair inside a section.
fn row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(RichText::new(label).color(palette().secondary_text));
    ui.label(value);
    ui.end_row();
}
