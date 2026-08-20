// ============================================================================
// Module:       gui::ui::extensions
// Description:  The per-extension list beside the file views, each row
//               carrying the colour the treemap paints that extension in.
//
// Dependencies: eframe::egui, egui_extras (TableBuilder);
//               crate::gui::app::GuiApp
// ============================================================================

//! The extension list beside the file views -- one row per file
//! extension, with the color the treemap paints that extension in.

use crate::gui::app::{ExtensionColumn, ExtensionSortMode, GuiApp};
use crate::gui::icons::Icon;
use crate::util::{human_bytes, thousands};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};
use egui_extras::{Column, TableBuilder};

#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::widgets::*;

/// The narrowest each column may be squeezed. See the directory table's
/// equivalent for why this is shared with the column spec rather than
/// written twice.
pub(super) fn extension_column_min_width(column: ExtensionColumn) -> f32 {
    match column {
        ExtensionColumn::Extension => 90.0,
        ExtensionColumn::Color => 48.0,
        ExtensionColumn::Description => 72.0,
        ExtensionColumn::Bytes => 70.0,
        ExtensionColumn::PercentBytes => 56.0,
        ExtensionColumn::Files => 44.0,
    }
}

/// Below this pane width the category breakdown is hidden; see the call
/// site for why it cannot simply be squeezed.
pub(super) const CATEGORIES_MIN_WIDTH: f32 = 320.0;

/// Total width the extension columns need before anything has to scroll.
pub(super) fn extension_table_min_width(columns: &[ExtensionColumn], item_spacing: f32) -> f32 {
    let widths: f32 = columns
        .iter()
        .map(|column| extension_column_min_width(*column))
        .sum();
    widths + item_spacing * columns.len().saturating_sub(1) as f32
}

/// See [`directory_column_spec`] for the reasoning: the last column on
/// screen absorbs the pane's slack and so cannot be dragged, and every
/// column before it is resizable with a floor and no ceiling.
///
/// `Extension` used to be the one absorbing, which left the first column
/// here pinned in exactly the way it was in the directory table.
///
/// [`directory_column_spec`]: super::directory::directory_column_spec
pub(super) fn extension_column_spec(column: ExtensionColumn, is_last: bool) -> Column {
    let minimum = extension_column_min_width(column);
    if is_last {
        return Column::remainder()
            .at_least(minimum)
            .clip(true)
            .resizable(false);
    }
    Column::auto()
        .range(minimum..=f32::INFINITY)
        .clip(true)
        .resizable(true)
}

pub(super) fn extension_column_label(column: ExtensionColumn) -> &'static str {
    match column {
        ExtensionColumn::Extension => "Extension",
        ExtensionColumn::Color => "Color",
        ExtensionColumn::Description => "Description",
        ExtensionColumn::Bytes => "Bytes",
        ExtensionColumn::PercentBytes => "% Bytes",
        ExtensionColumn::Files => "Files",
    }
}

pub(super) fn extension_sort_icon(
    sort: ExtensionSortMode,
    column: ExtensionColumn,
) -> Option<Icon> {
    match (column, sort) {
        (ExtensionColumn::Extension, ExtensionSortMode::ExtensionAsc)
        | (ExtensionColumn::Color, ExtensionSortMode::ColorAsc)
        | (ExtensionColumn::Description, ExtensionSortMode::DescriptionAsc)
        | (ExtensionColumn::Bytes, ExtensionSortMode::BytesAsc)
        | (ExtensionColumn::PercentBytes, ExtensionSortMode::PercentAsc)
        | (ExtensionColumn::Files, ExtensionSortMode::FilesAsc) => Some(Icon::ChevronUp),
        (ExtensionColumn::Extension, ExtensionSortMode::ExtensionDesc)
        | (ExtensionColumn::Color, ExtensionSortMode::ColorDesc)
        | (ExtensionColumn::Description, ExtensionSortMode::DescriptionDesc)
        | (ExtensionColumn::Bytes, ExtensionSortMode::BytesDesc)
        | (ExtensionColumn::PercentBytes, ExtensionSortMode::PercentDesc)
        | (ExtensionColumn::Files, ExtensionSortMode::FilesDesc) => Some(Icon::ChevronDown),
        _ => None,
    }
}

pub(super) fn extension_sort_after_click(
    sort: ExtensionSortMode,
    column: ExtensionColumn,
) -> ExtensionSortMode {
    match column {
        ExtensionColumn::Extension => {
            if sort == ExtensionSortMode::ExtensionAsc {
                ExtensionSortMode::ExtensionDesc
            } else {
                ExtensionSortMode::ExtensionAsc
            }
        }
        ExtensionColumn::Color => {
            if sort == ExtensionSortMode::ColorAsc {
                ExtensionSortMode::ColorDesc
            } else {
                ExtensionSortMode::ColorAsc
            }
        }
        ExtensionColumn::Description => {
            if sort == ExtensionSortMode::DescriptionAsc {
                ExtensionSortMode::DescriptionDesc
            } else {
                ExtensionSortMode::DescriptionAsc
            }
        }
        ExtensionColumn::Bytes => {
            if sort == ExtensionSortMode::BytesDesc {
                ExtensionSortMode::BytesAsc
            } else {
                ExtensionSortMode::BytesDesc
            }
        }
        ExtensionColumn::PercentBytes => {
            if sort == ExtensionSortMode::PercentDesc {
                ExtensionSortMode::PercentAsc
            } else {
                ExtensionSortMode::PercentDesc
            }
        }
        ExtensionColumn::Files => {
            if sort == ExtensionSortMode::FilesDesc {
                ExtensionSortMode::FilesAsc
            } else {
                ExtensionSortMode::FilesDesc
            }
        }
    }
}

pub(super) fn draw_extension_cell(
    ui: &mut egui::Ui,
    ext: &crate::gui::app::ExtensionRow,
    column: ExtensionColumn,
    total: u64,
    shell_icon: Option<&egui::TextureHandle>,
) {
    match column {
        ExtensionColumn::Extension => {
            // Prefer the icon the OS itself shows for this file type: a
            // `.docx` should look like whatever Word looks like on this
            // machine. That is information the drawn set cannot carry —
            // it knows "document", not "Word document". Where the
            // platform has nothing (or is not Windows), the category
            // glyph is the fallback.
            match shell_icon {
                Some(texture) => {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(Vec2::splat(15.0))
                            .maintain_aspect_ratio(true),
                    );
                }
                None => {
                    paint_inline_icon(
                        ui,
                        Icon::for_category(ext.category),
                        14.0,
                        ui.visuals().text_color(),
                    );
                }
            }
            ui.add_space(2.0);
            let _response = ui.label(&ext.extension);
            #[cfg(test)]
            probe(&TEST_EXTENSION_TEXT_RECTS).push((ext.extension.clone(), _response.rect));
        }
        ExtensionColumn::Color => {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
            ui.painter().rect(
                rect,
                3.0,
                extension_color(&ext.extension),
                Stroke::new(1.0_f32, Color32::from_white_alpha(80)),
            );
        }
        ExtensionColumn::Description => {
            ui.label(ext.category.label());
        }
        ExtensionColumn::Bytes => {
            ui.label(human_bytes(ext.size));
        }
        ExtensionColumn::PercentBytes => {
            ui.label(format!("{:.1}%", ext.size as f64 / total as f64 * 100.0));
        }
        ExtensionColumn::Files => {
            ui.label(thousands(ext.count));
        }
    }
}

pub(super) fn draw_extension_header(
    ui: &mut egui::Ui,
    app: &GuiApp,
    column: ExtensionColumn,
    claims_width: bool,
) -> (
    Option<ExtensionSortMode>,
    Option<(ExtensionColumn, ExtensionColumn)>,
) {
    let label = extension_column_label(column);
    let direction = extension_sort_icon(app.extension_sort, column);
    let response = sortable_header(ui, label, direction, claims_width);
    response.dnd_set_drag_payload(column);
    if response.dnd_hover_payload::<ExtensionColumn>().is_some() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            2.0,
            Stroke::new(1.0_f32, palette().accent),
        );
    }
    let reorder = response
        .dnd_release_payload::<ExtensionColumn>()
        .map(|source| (*source, column));
    #[cfg(test)]
    {
        probe(&TEST_EXTENSION_HEADER_RECTS).push((label, response.rect));
        probe(&TEST_EXTENSION_HEADER_ICONS).push((label, direction));
    }
    let sort = response
        .clicked()
        .then(|| extension_sort_after_click(app.extension_sort, column));
    (sort, reorder)
}

pub(super) fn draw_extension_list(app: &mut GuiApp, ui: &mut egui::Ui) {
    // Wrapped, and the trimmings drop out as the pane narrows. A plain
    // `horizontal` row reports the sum of its children as a minimum
    // width, and that propagates out through the panel until the divider
    // refuses to be dragged any further — the panel was pinned at 245px
    // by its own heading.
    ui.horizontal_wrapped(|ui| {
        section_title(ui, Icon::Extensions, "Extensions");
        if ui.available_width() > 90.0 {
            ui.label(
                RichText::new(format!("{} types", app.extensions.len()))
                    .small()
                    .color(palette().secondary_text),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let highlighted =
                app.highlighted_extension.is_some() || app.highlighted_category.is_some();
            if ui
                .add_enabled(highlighted, egui::Button::new("Clear highlight").small())
                .clicked()
            {
                app.highlighted_extension = None;
                app.highlighted_category = None;
            }
        });
    });
    ui.add_space(SPACE_SM);
    // The category breakdown sits above the per-extension table because
    // it answers the coarser question first — what kind of thing is
    // filling the disk — and the table then says which extensions make
    // that up. Collapsible, since on a narrow pane the table is what
    // people are usually here for.
    // Hidden below a certain pane width rather than allowed to squeeze.
    // A category chip is a fixed-size row of icon, name, size and
    // percentage; `horizontal_wrapped` can wrap between chips but not
    // inside one, so the widest chip becomes a minimum width for the
    // whole panel and the divider cannot be dragged past it. The table
    // is what people keep a narrow pane open for anyway.
    if ui.available_width() >= CATEGORIES_MIN_WIDTH {
        egui::CollapsingHeader::new(RichText::new("File categories").strong())
            .id_salt("file_categories")
            .default_open(true)
            .show(ui, |ui| super::categories::draw_categories(app, ui));
    }
    section_rule(ui);
    let total = app.extensions.iter().map(|e| e.size).sum::<u64>().max(1);
    let rows = app.extensions.clone();
    let columns = app.extension_column_order.clone();
    // Resolved up front rather than inside the row closure: looking one
    // up needs `&mut app` for the cache, which the closure cannot have
    // while it is also reading the rest of the app. Handles are cheap to
    // clone, and there are only ever as many as there are extensions on
    // screen.
    let shell_icons: Vec<Option<egui::TextureHandle>> = {
        let ctx = ui.ctx().clone();
        rows.iter()
            .map(|ext| app.shell_icons.get(&ctx, &ext.extension).cloned())
            .collect()
    };
    let mut selected = None;
    let mut sort = None;
    let mut reorder = None;
    let minimum_width = extension_table_min_width(&columns, ui.spacing().item_spacing.x);
    // Same story as the directory table: this panel is resizable, and
    // below a certain width every column but the first was simply clipped
    // away with no scrollbar and no way to reach them — but only wrap
    // when that is actually the case. Inside a horizontal scroll area
    // `Column::remainder()` settles at its minimum instead of taking up
    // the slack, so wrapping unconditionally stops the columns reflowing
    // when the pane is dragged and stops the resize handles working at
    // all.
    // Always wrapped, and the width stated explicitly. See the
    // directory table for the reasoning: rendering straight into the
    // pane lets the table's minimum width propagate outwards and stops
    // the panel being dragged narrower than its own columns.
    let table_width = ui.available_width().max(minimum_width);
    let mut render_table = |ui: &mut egui::Ui| {
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .vscroll(true)
            .resizable(true)
            // Without this the cell layout is top-down, so a cell
            // holding more than one widget stacks them instead of
            // putting them side by side — which is what happened the
            // moment the extension column gained an icon next to its
            // label. The directory table has always set it.
            .cell_layout(Layout::left_to_right(Align::Center))
            .sense(Sense::click());
        let last = columns.len().saturating_sub(1);
        for (index, column) in columns.iter().enumerate() {
            table = table.column(extension_column_spec(*column, index == last));
        }
        table
            .header(TABLE_HEADER_HEIGHT, |mut h| {
                for (index, column) in columns.iter().enumerate() {
                    h.col(|ui| {
                        // The last heading does not claim its cell's
                        // width; see `sortable_header`.
                        let (new_sort, new_reorder) =
                            draw_extension_header(ui, app, *column, index != last);
                        sort = new_sort.or(sort);
                        reorder = new_reorder.or(reorder);
                    });
                }
            })
            .body(|mut body| {
                let painter = body.ui_mut().painter().clone();
                body.rows(TABLE_ROW_HEIGHT, rows.len(), |mut row| {
                    let index = row.index();
                    let ext = &rows[index];
                    row.set_selected(app.highlighted_extension.as_ref() == Some(&ext.extension));
                    for column in &columns {
                        let column = *column;
                        #[cfg(test)]
                        probe(&TEST_EXTENSION_CELL_COLUMNS).push((ext.extension.clone(), column));
                        row.col(|ui| {
                            draw_extension_cell(
                                ui,
                                ext,
                                column,
                                total,
                                shell_icons.get(index).and_then(Option::as_ref),
                            );
                        });
                    }
                    let response = row.response();
                    row_hover_edge(
                        &painter,
                        &response,
                        egui::Id::new(("extension_row", &ext.extension)),
                    );
                    #[cfg(test)]
                    probe(&TEST_EXTENSION_ROW_RECTS).push((ext.extension.clone(), response.rect));
                    if response.clicked() {
                        selected = Some((ext.extension.clone(), ext.category));
                    }
                })
            });
    };

    egui::ScrollArea::horizontal()
        .id_salt("extension_hscroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(table_width);
            render_table(ui);
        });

    if let Some((source, target)) = reorder {
        app.reorder_extension_column(source, target);
    }
    if let Some(mode) = sort {
        app.extension_sort = mode;
        app.sort_extensions();
    }
    if let Some((ext, category)) = selected {
        let same = app.highlighted_extension.as_ref() == Some(&ext);
        app.highlighted_extension = (!same).then_some(ext);
        app.highlighted_category = (!same).then_some(category);
    }
}
