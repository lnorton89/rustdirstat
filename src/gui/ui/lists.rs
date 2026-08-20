// ============================================================================
// Module:       gui::ui::lists
// Description:  The three flat file views: largest files, search results, and
//               duplicate groups.
//
// Dependencies: eframe::egui, egui_extras (TableBuilder);
//               crate::gui::app::GuiApp
// ============================================================================

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

/// Which of the two flat file views a [`path_size_date_table`] is
/// drawing.
///
/// Everything the two differ by hangs off this: the rows it reads, the
/// hover-animation id namespace, and which set of rects the test probes
/// record. They used to differ by being two copies of the same table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FlatView {
    LargestFiles,
    SearchResults,
}

impl FlatView {
    /// Namespaces the per-row hover animation, so a row in one view does
    /// not inherit the hover ramp of the row that sat at the same index
    /// in the other.
    fn id_prefix(self) -> &'static str {
        match self {
            FlatView::LargestFiles => "largest_row",
            FlatView::SearchResults => "search_row",
        }
    }
}

/// The Path / Size / Last change table both flat views are.
///
/// Takes `app` immutably and hands the clicked row back rather than
/// selecting it, because the table borrows `app` for as long as it is
/// being built.
///
/// The size column used to be 90..=125 in one copy and 80..=120 in the
/// other. Nothing chose that; it is what two copies of a layout drift
/// into.
pub(super) fn path_size_date_table(
    app: &GuiApp,
    ui: &mut egui::Ui,
    view: FlatView,
) -> Option<Vec<usize>> {
    let count = match view {
        FlatView::LargestFiles => app.largest_files.len(),
        FlatView::SearchResults => app.search.results.len(),
    };
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
        .header(TABLE_HEADER_HEIGHT, |mut header| {
            header.col(|ui| table_header_label(ui, "Path"));
            header.col(|ui| table_header_label(ui, "Size"));
            header.col(|ui| table_header_label(ui, "Last change"));
        })
        .body(|mut body| {
            let painter = body.ui_mut().painter().clone();
            body.rows(TABLE_ROW_HEIGHT, count, |mut row| {
                let index = row.index();
                let (path, size, physical_size, modified) = match view {
                    FlatView::LargestFiles => {
                        let Some(file) = app.largest_files.get(index) else {
                            return;
                        };
                        (
                            file.index_path.clone(),
                            file.size,
                            file.physical_size,
                            file.modified,
                        )
                    }
                    FlatView::SearchResults => {
                        let Some(hit) = app.search.results.get(index) else {
                            return;
                        };
                        (
                            hit.index_path.clone(),
                            hit.size,
                            hit.physical_size,
                            hit.modified,
                        )
                    }
                };
                row.set_selected(app.selected_path.as_ref() == Some(&path));
                row.col(|ui| {
                    ui.label(crate::util::display_path(&app.tree.path_for(&path)));
                });
                row.col(|ui| {
                    ui.label(human_bytes(if app.use_physical {
                        physical_size
                    } else {
                        size
                    }));
                });
                row.col(|ui| {
                    ui.label(format_modified(modified));
                });
                let response = row.response();
                row_hover_edge(
                    &painter,
                    &response,
                    egui::Id::new((view.id_prefix(), &path)),
                );
                #[cfg(test)]
                match view {
                    FlatView::LargestFiles => {
                        probe(&TEST_LARGEST_ROW_RECTS).push((index, response.rect));
                    }
                    FlatView::SearchResults => {
                        probe(&TEST_SEARCH_ROW_RECTS).push((path.clone(), response.rect));
                    }
                }
                if response.clicked() {
                    selected = Some(path);
                }
            });
        });
    selected
}

pub(super) fn draw_largest_files(app: &mut GuiApp, ui: &mut egui::Ui) {
    if let Some(path) = path_size_date_table(app, ui, FlatView::LargestFiles) {
        app.select_path(path);
    }
}

pub(super) fn draw_search(app: &mut GuiApp, ui: &mut egui::Ui) {
    let mut run = false;
    ui.horizontal(|ui| {
        section_title(ui, Icon::Search, "Search");
        ui.label(
            RichText::new(format!("{} results", app.search.results.len()))
                .small()
                .color(palette().secondary_text),
        );
    });
    ui.label(
        RichText::new(
            "Find files by glob pattern — * ? [a-z] {jpg,png} — or by regular \
             expression with re:.",
        )
        .color(palette().secondary_text),
    );
    ui.add_space(SPACE_SM);
    ui.horizontal(|ui| {
        let search_width = ui.available_width().min(420.0);
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.search.query)
                .hint_text("*.{iso,img} or re:^archive-")
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
    if let Some(error) = &app.search.error {
        ui.colored_label(palette().danger, error);
    }
    section_rule(ui);
    if app.search.results.is_empty() && app.search.error.is_none() {
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
    if let Some(path) = path_size_date_table(app, ui, FlatView::SearchResults) {
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
