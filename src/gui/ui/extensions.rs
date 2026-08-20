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

pub(super) fn extension_column_spec(column: ExtensionColumn) -> Column {
    match column {
        ExtensionColumn::Extension => Column::remainder()
            .at_least(64.0)
            .clip(true)
            .resizable(false),
        ExtensionColumn::Color => Column::exact(48.0).clip(true),
        ExtensionColumn::Description => Column::auto().range(72.0..=115.0).clip(true),
        ExtensionColumn::Bytes => Column::auto().range(70.0..=120.0).clip(true),
        ExtensionColumn::PercentBytes => Column::auto().range(56.0..=78.0).clip(true),
        ExtensionColumn::Files => Column::auto().range(44.0..=70.0).clip(true),
    }
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
) {
    match column {
        ExtensionColumn::Extension => {
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
) -> (
    Option<ExtensionSortMode>,
    Option<(ExtensionColumn, ExtensionColumn)>,
) {
    let label = extension_column_label(column);
    let direction = extension_sort_icon(app.extension_sort, column);
    let response = sortable_header(ui, label, direction);
    response.dnd_set_drag_payload(column);
    if response.dnd_hover_payload::<ExtensionColumn>().is_some() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            2.0,
            Stroke::new(1.0_f32, ACCENT_COLOR),
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
    ui.horizontal(|ui| {
        paint_inline_icon(ui, Icon::Extensions, 19.0, ACCENT_COLOR);
        ui.heading("Extensions");
        ui.label(
            RichText::new(format!("{} types", app.extensions.len()))
                .small()
                .color(SECONDARY_TEXT_COLOR),
        );
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
    ui.add_space(3.0);
    ui.separator();
    ui.add_space(5.0);
    let total = app.extensions.iter().map(|e| e.size).sum::<u64>().max(1);
    let rows = app.extensions.clone();
    let columns = app.extension_column_order.clone();
    let mut selected = None;
    let mut sort = None;
    let mut reorder = None;
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .vscroll(true)
        .resizable(true)
        .sense(Sense::click());
    for column in &columns {
        table = table.column(extension_column_spec(*column));
    }
    table
        .header(TABLE_HEADER_HEIGHT, |mut h| {
            for column in &columns {
                h.col(|ui| {
                    let (new_sort, new_reorder) = draw_extension_header(ui, app, *column);
                    sort = new_sort.or(sort);
                    reorder = new_reorder.or(reorder);
                });
            }
        })
        .body(|body| {
            body.rows(TABLE_ROW_HEIGHT, rows.len(), |mut row| {
                let ext = &rows[row.index()];
                row.set_selected(app.highlighted_extension.as_ref() == Some(&ext.extension));
                for column in &columns {
                    let column = *column;
                    #[cfg(test)]
                    probe(&TEST_EXTENSION_CELL_COLUMNS).push((ext.extension.clone(), column));
                    row.col(|ui| draw_extension_cell(ui, ext, column, total));
                }
                let response = row.response();
                #[cfg(test)]
                probe(&TEST_EXTENSION_ROW_RECTS).push((ext.extension.clone(), response.rect));
                if response.clicked() {
                    selected = Some((ext.extension.clone(), ext.category));
                }
            })
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
