//! The directory tree: the main file view, its reorderable and
//! sortable columns, and the per-cell painting behind them.

use crate::gui::app::{DirectoryColumn, GuiApp, TreeRow};
use crate::gui::icons::Icon;
use crate::tui::SortMode;
use crate::util::{format_modified, human_bytes, thousands};
use eframe::egui::{self, Align, Color32, Layout, Sense, Stroke};
use egui_extras::{Column, TableBuilder};

use super::actions::*;
#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::widgets::*;

pub(super) enum RowAction {
    Open,
    Reveal,
    CopyPath,
    Zoom,
    Properties,
    Delete,
}

pub(super) fn visible_directory_columns(app: &GuiApp, compact: bool) -> Vec<DirectoryColumn> {
    app.directory_column_order
        .iter()
        .copied()
        .filter(|column| {
            !compact
                || matches!(
                    column,
                    DirectoryColumn::Name | DirectoryColumn::Size | DirectoryColumn::PercentTotal
                )
        })
        .collect()
}

/// The narrowest a column may be squeezed before its contents stop being
/// readable.
///
/// Shared between the column spec and the width the table claims inside
/// its horizontal scroll area. If those two disagreed the scrollbar would
/// either never appear when it was needed or never go away when it was
/// not.
pub(super) fn directory_column_min_width(column: DirectoryColumn) -> f32 {
    match column {
        DirectoryColumn::Name => 160.0,
        DirectoryColumn::Size => 75.0,
        DirectoryColumn::SubtreePercentage => 110.0,
        DirectoryColumn::PercentTotal => 60.0,
        DirectoryColumn::Files | DirectoryColumn::Subdirs => 45.0,
        DirectoryColumn::LastChange => 95.0,
        DirectoryColumn::Attributes => 55.0,
    }
}

/// Total width the visible columns need before anything has to scroll.
pub(super) fn directory_table_min_width(columns: &[DirectoryColumn], item_spacing: f32) -> f32 {
    let widths: f32 = columns
        .iter()
        .map(|column| directory_column_min_width(*column))
        .sum();
    widths + item_spacing * columns.len().saturating_sub(1) as f32
}

pub(super) fn directory_column_spec(column: DirectoryColumn) -> Column {
    let minimum = directory_column_min_width(column);
    match column {
        DirectoryColumn::Name => Column::remainder()
            .at_least(minimum)
            .clip(true)
            .resizable(false),
        DirectoryColumn::Size => Column::auto().range(minimum..=110.0).clip(true),
        DirectoryColumn::SubtreePercentage => Column::auto().range(minimum..=180.0).clip(true),
        DirectoryColumn::PercentTotal => Column::auto().range(minimum..=90.0).clip(true),
        DirectoryColumn::Files | DirectoryColumn::Subdirs => {
            Column::auto().range(minimum..=75.0).clip(true)
        }
        DirectoryColumn::LastChange => Column::auto().range(minimum..=150.0).clip(true),
        DirectoryColumn::Attributes => Column::auto().range(minimum..=90.0).clip(true),
    }
}

pub(super) fn directory_column_label(column: DirectoryColumn) -> &'static str {
    match column {
        DirectoryColumn::Name => "Name",
        DirectoryColumn::Size => "Size",
        DirectoryColumn::SubtreePercentage => "Subtree percentage",
        DirectoryColumn::PercentTotal => "% of total",
        DirectoryColumn::Files => "Files",
        DirectoryColumn::Subdirs => "Subdirs",
        DirectoryColumn::LastChange => "Last change",
        DirectoryColumn::Attributes => "Attributes",
    }
}

pub(super) fn directory_sort_icon(sort: SortMode, column: DirectoryColumn) -> Option<Icon> {
    match (column, sort) {
        (DirectoryColumn::Name, SortMode::NameAsc)
        | (DirectoryColumn::Size, SortMode::SizeAsc)
        | (DirectoryColumn::LastChange, SortMode::ModifiedAsc) => Some(Icon::ChevronUp),
        (DirectoryColumn::Name, SortMode::NameDesc)
        | (DirectoryColumn::Size, SortMode::SizeDesc)
        | (DirectoryColumn::LastChange, SortMode::ModifiedDesc) => Some(Icon::ChevronDown),
        _ => None,
    }
}

pub(super) fn directory_sort_after_click(
    sort: SortMode,
    column: DirectoryColumn,
) -> Option<SortMode> {
    match column {
        DirectoryColumn::Name => Some(if sort == SortMode::NameAsc {
            SortMode::NameDesc
        } else {
            SortMode::NameAsc
        }),
        DirectoryColumn::Size => Some(if sort == SortMode::SizeDesc {
            SortMode::SizeAsc
        } else {
            SortMode::SizeDesc
        }),
        DirectoryColumn::LastChange => Some(if sort == SortMode::ModifiedDesc {
            SortMode::ModifiedAsc
        } else {
            SortMode::ModifiedDesc
        }),
        // Listed rather than caught by a wildcard: these are the columns
        // with nothing to sort by. A new column added to the enum should
        // fail to compile here and make someone decide, instead of
        // silently becoming unsortable.
        DirectoryColumn::SubtreePercentage
        | DirectoryColumn::PercentTotal
        | DirectoryColumn::Files
        | DirectoryColumn::Subdirs
        | DirectoryColumn::Attributes => None,
    }
}

pub(super) fn draw_directory_cell(
    ui: &mut egui::Ui,
    app: &GuiApp,
    item: &TreeRow,
    column: DirectoryColumn,
    total: u64,
) -> bool {
    match column {
        DirectoryColumn::Name => {
            ui.add_space(item.depth as f32 * 18.0);
            let mut toggle = false;
            if item.is_dir {
                let expanded = app.expanded.contains(&item.path);
                let chevron = if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                };
                toggle =
                    compact_icon_button(ui, chevron, if expanded { "Collapse" } else { "Expand" })
                        .clicked();
            } else {
                ui.add_space(28.0);
            }
            paint_inline_icon(
                ui,
                if item.is_dir {
                    Icon::Folder
                } else {
                    Icon::File
                },
                17.0,
                if item.is_dir {
                    Color32::from_rgb(238, 185, 82)
                } else {
                    ui.visuals().text_color()
                },
            );
            ui.label(&item.name);
            toggle
        }
        DirectoryColumn::Size => {
            ui.label(human_bytes(item.size));
            false
        }
        DirectoryColumn::SubtreePercentage => {
            percentage_bar(ui, item.size as f32 / item.parent_size.max(1) as f32);
            false
        }
        DirectoryColumn::PercentTotal => {
            ui.label(format!("{:.1}%", item.size as f64 / total as f64 * 100.0));
            false
        }
        DirectoryColumn::Files => {
            ui.label(thousands(item.files));
            false
        }
        DirectoryColumn::Subdirs => {
            ui.label(thousands(item.dirs));
            false
        }
        DirectoryColumn::LastChange => {
            ui.label(format_modified(item.modified));
            false
        }
        DirectoryColumn::Attributes => {
            ui.label(if item.unreadable > 0 {
                "D !"
            } else if item.symlink {
                "L"
            } else if item.is_dir {
                "D"
            } else {
                "A"
            });
            false
        }
    }
}

pub(super) fn draw_directory_tree(app: &mut GuiApp, ui: &mut egui::Ui) {
    // Rebuild first, then borrow: the row list is only re-flattened out
    // of the tree when something it depends on actually changed, instead
    // of on every frame as it used to be.
    app.refresh_visible_rows();
    let mut select = None;
    let mut toggle = None;
    let mut open = None;
    let mut sort = None;
    let mut reorder = None;
    let mut row_action: Option<(RowAction, Vec<usize>)> = None;
    // Painting the table only reads app state, recording what the user
    // did into the locals above. Confining that shared borrow to a block
    // is what lets the row list be borrowed straight out of the cache
    // instead of cloned, while still leaving `app` mutable below.
    {
        let app = &*app;
        let rows = &app.visible_rows;
        let total = app.tree.root.effective_size(app.use_physical).max(1);
        let compact = ui.available_width() < 760.0;
        let columns = visible_directory_columns(app, compact);
        let minimum_width = directory_table_min_width(&columns, ui.spacing().item_spacing.x);
        // Dragging the treemap splitter left can squeeze this pane below
        // even the compact column set. The table already refuses to go
        // narrower than its columns need, so what was missing was not the
        // width but any way to reach it: the overflow was simply clipped
        // at the pane edge, which reads as the pane being broken rather
        // than small. The scroll area is what turns that into a
        // scrollbar. It costs nothing at ordinary widths, since it only
        // scrolls when the content genuinely does not fit.
        //
        // `set_min_width` restates the floor independently of how the
        // column specs happen to be written, so the guarantee does not
        // quietly depend on `Name` keeping its `at_least`.
        // Width the table should lay out to: the pane when it fits, the
        // columns' minimum when it does not.
        let table_width = ui.available_width().max(minimum_width);
        let scroll = egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // `set_width`, not `set_min_width`. Inside a horizontal
                // scroll area the available width is effectively
                // unbounded, and `Column::remainder()` will happily take
                // all of it — which pushed every column after `Name` off
                // the right-hand side and out of reach. Pinning both ends
                // is what keeps the remainder column honest.
                ui.set_width(table_width);
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .vscroll(true)
                    .sense(Sense::click())
                    .cell_layout(Layout::left_to_right(Align::Center));
                for column in &columns {
                    table = table.column(directory_column_spec(*column));
                }
                table
                    .header(TABLE_HEADER_HEIGHT, |mut h| {
                        for column in &columns {
                            let column = *column;
                            h.col(|ui| {
                                let label = directory_column_label(column);
                                let response = sortable_header(
                                    ui,
                                    label,
                                    directory_sort_icon(app.sort, column),
                                );
                                response.dnd_set_drag_payload(column);
                                if response.dnd_hover_payload::<DirectoryColumn>().is_some() {
                                    ui.painter().rect_stroke(
                                        response.rect.shrink(1.0),
                                        2.0,
                                        Stroke::new(1.0_f32, ACCENT_COLOR),
                                    );
                                }
                                if let Some(source) =
                                    response.dnd_release_payload::<DirectoryColumn>()
                                {
                                    reorder = Some((*source, column));
                                }
                                #[cfg(test)]
                                probe(&TEST_DIRECTORY_HEADER_RECTS).push((label, response.rect));
                                if response.clicked() {
                                    sort = directory_sort_after_click(app.sort, column);
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(TABLE_ROW_HEIGHT, rows.len(), |mut row| {
                            let item = &rows[row.index()];
                            row.set_selected(app.selected_path.as_ref() == Some(&item.path));
                            for column in &columns {
                                let column = *column;
                                #[cfg(test)]
                                probe(&TEST_DIRECTORY_CELL_COLUMNS)
                                    .push((item.path.clone(), column));
                                row.col(|ui| {
                                    if draw_directory_cell(ui, app, item, column, total) {
                                        toggle = Some(item.path.clone());
                                    }
                                });
                            }
                            let response = row.response();
                            #[cfg(test)]
                            probe(&TEST_DIRECTORY_ROW_RECTS)
                                .push((item.path.clone(), response.rect));
                            response.context_menu(|ui| {
                                if icon_button(ui, true, Icon::ExternalLink, "Open").clicked() {
                                    row_action = Some((RowAction::Open, item.path.clone()));
                                    ui.close_menu();
                                }
                                if icon_button(ui, true, Icon::Folder, "Show in Explorer").clicked()
                                {
                                    row_action = Some((RowAction::Reveal, item.path.clone()));
                                    ui.close_menu();
                                }
                                if icon_button(ui, true, Icon::Copy, "Copy path").clicked() {
                                    row_action = Some((RowAction::CopyPath, item.path.clone()));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if icon_button(ui, true, Icon::ZoomIn, "Zoom treemap here")
                                    .clicked()
                                {
                                    row_action = Some((RowAction::Zoom, item.path.clone()));
                                    ui.close_menu();
                                }
                                if icon_button(ui, true, Icon::Info, "Properties").clicked() {
                                    row_action = Some((RowAction::Properties, item.path.clone()));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if icon_button(ui, !item.path.is_empty(), Icon::Trash, "Delete…")
                                    .clicked()
                                {
                                    row_action = Some((RowAction::Delete, item.path.clone()));
                                    ui.close_menu();
                                }
                            });
                            if response.clicked() {
                                select = Some(item.path.clone());
                            }
                            if response.double_clicked() {
                                if item.is_dir {
                                    toggle = Some(item.path.clone());
                                } else {
                                    open = Some(item.path.clone());
                                }
                            }
                        })
                    });
            });
        #[cfg(test)]
        probe(&TEST_DIRECTORY_SCROLL).push((scroll.content_size.x, scroll.inner_rect.width()));
        #[cfg(not(test))]
        let _ = scroll;
    }
    if let Some((source, target)) = reorder {
        app.reorder_directory_column(source, target);
    }
    if let Some(mode) = sort {
        app.sort = mode;
    }
    if let Some(path) = toggle {
        app.toggle_expanded(&path);
    }
    if let Some(path) = select {
        app.select_path(path);
    }
    if let Some(path) = open {
        app.select_path(path);
        open_selected(app);
    }
    if let Some((action, path)) = row_action {
        app.select_path(path);
        match action {
            RowAction::Open => open_selected(app),
            RowAction::Reveal => reveal_selected(app),
            RowAction::CopyPath => copy_path(app),
            RowAction::Zoom => app.zoom_in(),
            RowAction::Properties => app.show_properties = true,
            RowAction::Delete => app.request_delete_selected(false),
        }
    }
}
