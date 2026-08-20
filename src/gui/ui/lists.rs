//! The three flat file views: largest files, search results, and
//! duplicate groups. Unlike the directory tree these are already
//! flat lists computed on `GuiApp`, so they only have to be
//! painted.

use crate::gui::app::GuiApp;
use crate::gui::icons::Icon;
use crate::util::{format_modified, human_bytes};
use eframe::egui::{self, RichText, Sense};
use egui_extras::{Column, TableBuilder};

#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::widgets::*;

pub(super) fn draw_largest_files(app: &mut GuiApp, ui: &mut egui::Ui) {
    let mut selected = None;
    TableBuilder::new(ui)
        .striped(true)
        .vscroll(true)
        .resizable(true)
        .sense(Sense::click())
        .column(
            Column::remainder()
                .at_least(220.0)
                .clip(true)
                .resizable(false),
        )
        .column(Column::auto().range(90.0..=125.0).clip(true))
        .column(Column::auto().range(110.0..=175.0).clip(true))
        .header(TABLE_HEADER_HEIGHT, |mut h| {
            h.col(|ui| {
                table_header_label(ui, "Path");
            });
            h.col(|ui| {
                table_header_label(ui, "Size");
            });
            h.col(|ui| {
                table_header_label(ui, "Last change");
            });
        })
        .body(|mut body| {
            let painter = body.ui_mut().painter().clone();
            body.rows(TABLE_ROW_HEIGHT, app.largest_files.len(), |mut row| {
                let file = &app.largest_files[row.index()];
                let path = file.index_path.clone();
                row.set_selected(app.selected_path.as_ref() == Some(&path));
                row.col(|ui| {
                    ui.label(crate::util::display_path(&app.tree.path_for(&path)));
                });
                row.col(|ui| {
                    ui.label(human_bytes(if app.use_physical {
                        file.physical_size
                    } else {
                        file.size
                    }));
                });
                row.col(|ui| {
                    ui.label(format_modified(file.modified));
                });
                let response = row.response();
                row_hover_edge(&painter, &response, egui::Id::new(("largest_row", &path)));
                #[cfg(test)]
                probe(&TEST_LARGEST_ROW_RECTS).push((row.index(), response.rect));
                if response.clicked() {
                    selected = Some(path);
                }
            })
        });
    if let Some(path) = selected {
        app.select_path(path);
    }
}

pub(super) fn draw_search(app: &mut GuiApp, ui: &mut egui::Ui) {
    let mut run = false;
    ui.horizontal(|ui| {
        section_title(ui, Icon::Search, "Search");
        ui.label(
            RichText::new(format!("{} results", app.search_results.len()))
                .small()
                .color(palette().secondary_text),
        );
    });
    ui.label(
        RichText::new("Find files by glob pattern or regular expression.")
            .color(palette().secondary_text),
    );
    ui.add_space(SPACE_SM);
    ui.horizontal(|ui| {
        let search_width = ui.available_width().min(420.0);
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.search_query)
                .hint_text("*.iso or re:^archive-")
                .desired_width(search_width),
        );
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            run = true;
        }
        if icon_button(ui, true, Icon::Search, "Search").clicked() {
            run = true;
        }
    });
    if run {
        app.run_search();
    }
    if let Some(error) = &app.search_error {
        ui.colored_label(palette().danger, error);
    }
    section_rule(ui);
    if app.search_results.is_empty() && app.search_error.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(SPACE_LG + SPACE_SM);
            paint_inline_icon(ui, Icon::Search, 40.0, palette().secondary_text);
            ui.heading("No search results yet");
            ui.label(
                RichText::new("Enter a pattern above and press Enter to search the current scan.")
                    .color(palette().secondary_text),
            );
        });
        return;
    }
    let mut selected = None;
    TableBuilder::new(ui)
        .striped(true)
        .vscroll(true)
        .resizable(true)
        .sense(Sense::click())
        .column(
            Column::remainder()
                .at_least(220.0)
                .clip(true)
                .resizable(false),
        )
        .column(Column::auto().range(80.0..=120.0).clip(true))
        .column(Column::auto().range(110.0..=175.0).clip(true))
        .header(TABLE_HEADER_HEIGHT, |mut header| {
            header.col(|ui| table_header_label(ui, "Path"));
            header.col(|ui| table_header_label(ui, "Size"));
            header.col(|ui| table_header_label(ui, "Last change"));
        })
        .body(|mut body| {
            let painter = body.ui_mut().painter().clone();
            body.rows(TABLE_ROW_HEIGHT, app.search_results.len(), |mut row| {
                let hit = &app.search_results[row.index()];
                let path = hit.index_path.clone();
                row.set_selected(app.selected_path.as_ref() == Some(&path));
                row.col(|ui| {
                    ui.label(crate::util::display_path(&app.tree.path_for(&path)));
                });
                row.col(|ui| {
                    ui.label(human_bytes(if app.use_physical {
                        hit.physical_size
                    } else {
                        hit.size
                    }));
                });
                row.col(|ui| {
                    ui.label(format_modified(hit.modified));
                });
                let response = row.response();
                row_hover_edge(&painter, &response, egui::Id::new(("search_row", &path)));
                #[cfg(test)]
                probe(&TEST_SEARCH_ROW_RECTS).push((path.clone(), response.rect));
                if response.clicked() {
                    selected = Some(path);
                }
            });
        });
    if let Some(path) = selected {
        app.select_path(path);
    }
}

pub(super) fn draw_duplicates(app: &mut GuiApp, ui: &mut egui::Ui) {
    if app.duplicate_groups.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(SPACE_LG + SPACE_SM);
            if app.duplicate_running() {
                ui.spinner();
                ui.heading("Finding duplicate files…");
                ui.label("Files with matching sizes are being hashed in the background.");
            } else {
                paint_inline_icon(ui, Icon::Duplicate, 46.0, palette().accent);
                ui.heading("No duplicate groups found");
                ui.label("Scan the current tree for files with identical content.");
            }
            if !app.duplicate_running()
                && icon_button(ui, true, Icon::Duplicate, "Scan for duplicates").clicked()
            {
                app.find_duplicates();
            }
        });
        return;
    }
    let mut selected = None;
    // `auto_shrink` off horizontally, or the scroll area narrows to its
    // widest row and parks its bar in the middle of the pane instead of
    // against the right edge — the only scrollbar in the app that did not
    // line up with the panel it belonged to.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (group_idx, group) in app.duplicate_groups.iter().enumerate() {
                let wasted = group
                    .size
                    .saturating_mul(group.files.len().saturating_sub(1) as u64);
                egui::CollapsingHeader::new(format!(
                    "Group {} · {} copies · {} each · {} reclaimable",
                    group_idx + 1,
                    group.files.len(),
                    human_bytes(group.size),
                    human_bytes(wasted)
                ))
                .default_open(group_idx < 5)
                .show(ui, |ui| {
                    for file in &group.files {
                        let response = ui.selectable_label(
                            app.selected_path.as_ref() == Some(&file.index_path),
                            crate::util::display_path(&app.tree.path_for(&file.index_path)),
                        );
                        #[cfg(test)]
                        probe(&TEST_DUPLICATE_ROW_RECTS)
                            .push((file.index_path.clone(), response.rect));
                        if response.clicked() {
                            selected = Some(file.index_path.clone());
                        }
                    }
                });
            }
        });
    if let Some(path) = selected {
        app.select_path(path);
    }
}
