// ============================================================================
// Module:       gui::ui::directory
// Description:  The directory tree pane: the main file view, its reorderable
//               sortable columns, and the per-cell painting behind them.
//
// Dependencies: eframe::egui, egui_extras (TableBuilder);
//               crate::gui::app::{GuiApp, TreeRow}
// ============================================================================

//! The directory tree: the main file view, its reorderable and
//! sortable columns, and the per-cell painting behind them.

use crate::gui::app::{DirectoryColumn, GuiApp, TreeRow};
use crate::gui::icons::Icon;
use crate::model::SortMode;
use crate::util::{format_modified, human_bytes, thousands};
use eframe::egui::{self, Align, Layout, Sense, Stroke};
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

/// The columns to draw: all of them, in the order the user arranged them.
///
/// A narrow pane used to drop everything but Name, Size and % of total.
/// That is the wrong trade for this table — the columns are the reason
/// to look at it, and a column that vanishes when the pane gets small
/// cannot be scrolled to, resized, or even known to exist. Narrowing the
/// pane now scrolls, which is what the horizontal scroll area below is
/// for.
pub(super) fn visible_directory_columns(app: &GuiApp) -> Vec<DirectoryColumn> {
    app.directory_column_order.clone()
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

/// Width a column starts at before the user drags it.
fn directory_column_initial_width(column: DirectoryColumn) -> f32 {
    match column {
        DirectoryColumn::Name => 320.0,
        DirectoryColumn::Size => 95.0,
        DirectoryColumn::SubtreePercentage => 150.0,
        DirectoryColumn::PercentTotal => 80.0,
        DirectoryColumn::Files | DirectoryColumn::Subdirs => 70.0,
        DirectoryColumn::LastChange => 140.0,
        DirectoryColumn::Attributes => 80.0,
    }
}

/// How a column is sized, given whether it is the last one on screen.
///
/// Exactly one column has to absorb the pane's slack, or the table sits
/// at its natural width with dead space beside it and stops growing when
/// the pane does. Only a `remainder()` does that, and a `remainder()`
/// cannot also be resizable: dragging one gives it a stored width, after
/// which it absorbs nothing and the table stops filling the pane. So the
/// column that absorbs is the one column that cannot be dragged.
///
/// That used to be `Name` — the first column, the widest, and the one
/// most worth dragging. It is now the *last* column instead, wherever the
/// user has dragged that column to. A drag handle on the trailing edge is
/// the one handle nobody misses: there is no neighbour past it to push.
///
/// The others get a floor and **no ceiling**. Ceilings are what broke
/// resizing before: columns were `auto().range(min..=max)` with ranges as
/// narrow as `75..=110`, so a drag hit its stop within a few pixels and
/// looked like it had done nothing at all.
pub(super) fn directory_column_spec(column: DirectoryColumn) -> Column {
    Column::initial(directory_column_initial_width(column))
        .at_least(directory_column_min_width(column))
        .clip(true)
        .resizable(true)
}

/// A trailing column with nothing in it, holding whatever width the real
/// columns do not want.
///
/// Something has to absorb the pane's slack or the table sits at its
/// natural width with dead space beside it. Making that job a *real*
/// column — the last one — worked, but stretched it: on a wide pane the
/// last column grew to several hundred pixels of empty cell, and because
/// it soaked up every spare pixel the table could never be wider than
/// its pane, so the horizontal scrollbar had nothing to do at any width
/// anyone actually uses.
///
/// A spacer absorbs instead. Real columns keep the width they were
/// given, every one of them can be dragged, the table still reaches the
/// pane's edge, and when the columns genuinely need more room than the
/// pane has, the scroll area is there for it.
///
/// `resizable(false)` is load-bearing: a resizable `remainder()` takes a
/// stored width and stops absorbing.
pub(super) fn slack_column() -> Column {
    Column::remainder().at_least(0.0).resizable(false)
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
            ui.add_space(item.depth as f32 * SPACE_LG);
            let mut toggle = false;
            if item.is_dir {
                toggle = expand_toggle(
                    ui,
                    egui::Id::new(("tree_toggle", &item.path)),
                    app.expanded.contains(&item.path),
                )
                .clicked();
            } else {
                // The toggle's width *plus* the row spacing that would
                // follow it. `add_space` advances the cursor without
                // becoming an item, so the next widget adds no spacing of
                // its own after one — which means the gap a file leaves
                // has to account for both or its icon lands short of the
                // column every folder beside it sits in.
                let gap = EXPAND_TOGGLE_WIDTH + ui.spacing().item_spacing.x;
                ui.add_space(gap);
            }
            let _icon = paint_inline_icon(
                ui,
                if item.is_dir {
                    Icon::Folder
                } else {
                    Icon::File
                },
                17.0,
                if item.is_dir {
                    palette().warning
                } else {
                    ui.visuals().text_color()
                },
            );
            #[cfg(test)]
            probe(&TEST_TREE_NAME_ICONS).push((item.path.clone(), item.is_dir, _icon));
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
        let columns = visible_directory_columns(app);
        let minimum_width = directory_table_min_width(&columns, ui.spacing().item_spacing.x);
        // Dragging the treemap splitter left can squeeze this pane below
        // what the columns need. The table refuses to go narrower than
        // that, so what was missing was never the width but a way to
        // reach it: the overflow was clipped at the pane edge, which
        // reads as the pane being broken rather than small. The scroll
        // area turns that into a scrollbar, and costs nothing at
        // ordinary widths because the spacer column keeps the table
        // exactly as wide as the pane until the columns genuinely need
        // more.
        let available = ui.available_width();

        let mut render_table = |ui: &mut egui::Ui| {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .vscroll(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center));
            for column in &columns {
                table = table.column(directory_column_spec(*column));
            }
            table = table.column(slack_column());
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
                                true,
                            );
                            response.dnd_set_drag_payload(column);
                            if response.dnd_hover_payload::<DirectoryColumn>().is_some() {
                                ui.painter().rect_stroke(
                                    response.rect.shrink(1.0),
                                    2.0,
                                    Stroke::new(1.0_f32, palette().accent),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            if let Some(source) = response.dnd_release_payload::<DirectoryColumn>()
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
                .body(|mut body| {
                    // Cloned before the rows are walked: this is the
                    // table body's own painter, so the hover edge is
                    // clipped to the table exactly as its rows are, and
                    // no row closure can borrow a `Ui` to get one.
                    let painter = body.ui_mut().painter().clone();
                    body.rows(TABLE_ROW_HEIGHT, rows.len(), |mut row| {
                        let item = &rows[row.index()];
                        row.set_selected(app.selected_path.as_ref() == Some(&item.path));
                        for column in &columns {
                            let column = *column;
                            #[cfg(test)]
                            probe(&TEST_DIRECTORY_CELL_COLUMNS).push((item.path.clone(), column));
                            row.col(|ui| {
                                if draw_directory_cell(ui, app, item, column, total) {
                                    toggle = Some(item.path.clone());
                                }
                            });
                        }
                        // The spacer's cell, so the row spans the table.
                        row.col(|_| {});
                        let response = row.response();
                        // Keyed off the row's path, not its index: rows
                        // are virtualized, so index 0 is a different file
                        // the moment the table scrolls and the hover
                        // animation would follow the viewport instead of
                        // the pointer.
                        row_hover_edge(
                            &painter,
                            &response,
                            egui::Id::new(("tree_row", &item.path)),
                        );
                        #[cfg(test)]
                        probe(&TEST_DIRECTORY_ROW_RECTS).push((item.path.clone(), response.rect));
                        response.context_menu(|ui| {
                            if icon_button(ui, true, Icon::ExternalLink, "Open").clicked() {
                                row_action = Some((RowAction::Open, item.path.clone()));
                                ui.close();
                            }
                            if icon_button(ui, true, Icon::Folder, "Show in Explorer").clicked() {
                                row_action = Some((RowAction::Reveal, item.path.clone()));
                                ui.close();
                            }
                            if icon_button(ui, true, Icon::Copy, "Copy path").clicked() {
                                row_action = Some((RowAction::CopyPath, item.path.clone()));
                                ui.close();
                            }
                            ui.separator();
                            if icon_button(ui, true, Icon::ZoomIn, "Zoom treemap here").clicked() {
                                row_action = Some((RowAction::Zoom, item.path.clone()));
                                ui.close();
                            }
                            if icon_button(ui, true, Icon::Info, "Properties").clicked() {
                                row_action = Some((RowAction::Properties, item.path.clone()));
                                ui.close();
                            }
                            ui.separator();
                            if icon_button(ui, !item.path.is_empty(), Icon::Trash, "Delete…")
                                .clicked()
                            {
                                row_action = Some((RowAction::Delete, item.path.clone()));
                                ui.close();
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
        };

        // Always wrapped, never conditionally. Rendering the table
        // straight into the pane lets its minimum width propagate
        // outwards, which stops the *panel* from being dragged any
        // narrower than its own columns — and then, because the pane can
        // never get narrow, the scroll area never appears either.
        // Wrapping unconditionally and stating the width explicitly
        // breaks that circle. The `else` arm this used to have was
        // unreachable, and reading it cost time during an unrelated
        // layout hunt.
        let extents = {
            let scroll = egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // `set_width`, not `set_min_width`: inside a scroll
                    // area the available width is unbounded, and a
                    // `remainder()` column reads that as "take
                    // everything". Stating both ends keeps it honest and
                    // still lets the table grow with the pane.
                    ui.set_width(available.max(minimum_width));
                    render_table(ui);
                });
            (scroll.content_size.x, scroll.inner_rect.width())
        };
        #[cfg(test)]
        probe(&TEST_DIRECTORY_SCROLL).push(extents);
        #[cfg(not(test))]
        let _ = extents;
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
            RowAction::Properties => app.toggle_properties(),
            RowAction::Delete => app.request_delete_selected(false),
        }
    }
}
