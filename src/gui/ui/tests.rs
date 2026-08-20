//! Interaction tests: drive the real drawing code through an egui
//! context, then assert against the geometry it recorded in
//! [`super::probes`].
//!
//! These click where the app actually drew a row rather than where a
//! test author guessed it would be, so a layout change that moves a
//! control out from under its own click target fails here instead of
//! in the user's hands.

use super::directory::*;
use super::draw_file_area;
use super::extensions::*;
use super::lists::*;
use super::probes::*;
use super::theme::*;
use super::treemap::*;
use super::widgets::*;

use crate::color::Category;
use crate::gui::app::{
    DirectoryColumn, ExtensionColumn, ExtensionRow, ExtensionSortMode, FileView, GuiApp,
};
use crate::gui::icons::Icon;
use crate::model::{Node, Tree};
use crate::tui::SortMode;
use anyhow::Context;
use eframe::egui::{self, Color32};
use std::path::PathBuf;

static TEST_UI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Which `ext_totals` slot a file of this name is counted under.
///
/// Derived from the name the same way `file` below derives the node's
/// category, so the two cannot disagree — and so the fixtures do not have
/// to unwrap an `Option` they themselves just filled in.
fn category_index(name: &str) -> usize {
    crate::model::category_for_name(name).index()
}

fn file(name: &str, size: u64) -> Node {
    let category = crate::model::category_for_name(name);
    Node {
        name: name.to_string(),
        is_dir: false,
        is_symlink: false,
        size,
        physical_size: size,
        file_count: 1,
        dir_count: 0,
        modified: None,
        children: Vec::new(),
        error: false,
        category: Some(category),
        ext_totals: Vec::new(),
        unreadable_count: 0,
    }
}

fn app_with_one_file() -> GuiApp {
    let child = file("click-me.txt", 128);
    let mut totals = vec![(0, 0, 0); Category::COUNT];
    let index = category_index(&child.name);
    totals[index] = (128, 128, 1);
    GuiApp::new(Tree {
        root_path: PathBuf::from("C:\\test-root"),
        root: Node {
            name: "test-root".to_string(),
            is_dir: true,
            is_symlink: false,
            size: 128,
            physical_size: 128,
            file_count: 1,
            dir_count: 0,
            modified: None,
            children: vec![child],
            error: false,
            category: None,
            ext_totals: totals,
            unreadable_count: 0,
        },
        volume_free: None,
        volume_total: None,
    })
}

#[test]
fn treemap_selection_frame_is_fully_inside_the_tile() -> anyhow::Result<()> {
    let tile = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 80.0));
    let frame = treemap_selection_rect(tile).context("a normal tile should have a frame")?;
    let half_outer_stroke = (TREEMAP_SELECTION_WIDTH + 2.0) * 0.5;

    assert!(frame.left() - half_outer_stroke > tile.left());
    assert!(frame.top() - half_outer_stroke > tile.top());
    assert!(frame.right() + half_outer_stroke < tile.right());
    assert!(frame.bottom() + half_outer_stroke < tile.bottom());
    Ok(())
}

#[test]
fn cushion_is_one_valid_two_dimensional_gradient_mesh() {
    let rect = egui::Rect::from_min_size(egui::pos2(4.0, 8.0), egui::vec2(200.0, 100.0));
    let mesh = cushion_mesh(rect, Color32::from_rgb(155, 62, 205));

    assert!(mesh.is_valid());
    assert_eq!(mesh.vertices.len(), 25);
    assert_eq!(mesh.indices.len(), 96);
    assert_eq!(mesh.calc_bounds(), rect);
    assert_ne!(mesh.vertices[0].color, mesh.vertices[4].color);
    assert_ne!(mesh.vertices[0].color, mesh.vertices[20].color);

    for pair in mesh.vertices.windows(2) {
        let delta = (relative_luminance(pair[0].color) - relative_luminance(pair[1].color)).abs();
        assert!(delta < 0.08, "adjacent gradient vertices jump by {delta}");
    }
}

#[test]
fn secondary_and_treemap_text_meet_readability_contrast() {
    assert!(contrast_ratio(SECONDARY_TEXT_COLOR, PANEL_COLOR) >= 4.5);
    assert!(contrast_ratio(PRIMARY_TEXT_COLOR, PANEL_COLOR) >= 7.0);

    for background in [
        Color32::from_rgb(155, 62, 205),
        Color32::from_rgb(65, 92, 102),
        Color32::from_rgb(190, 190, 45),
        Color32::from_rgb(35, 35, 38),
    ] {
        assert!(contrast_ratio(readable_text_color(background), background) >= 4.5);
    }
}

fn app_with_sortable_files() -> GuiApp {
    let mut largest = file("z-largest.txt", 300);
    largest.modified = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1));
    let mut smallest = file("a-smallest.txt", 10);
    smallest.modified = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(3));
    let mut middle = file("m-middle.txt", 100);
    middle.modified = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(2));
    let children = vec![largest, smallest, middle];
    let mut totals = vec![(0, 0, 0); Category::COUNT];
    for child in &children {
        let index = category_index(&child.name);
        totals[index].0 += child.size;
        totals[index].1 += child.physical_size;
        totals[index].2 += 1;
    }
    GuiApp::new(Tree {
        root_path: PathBuf::from("C:\\sortable-root"),
        root: Node {
            name: "sortable-root".to_string(),
            is_dir: true,
            is_symlink: false,
            size: 410,
            physical_size: 410,
            file_count: 3,
            dir_count: 0,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: totals,
            unreadable_count: 0,
        },
        volume_free: None,
        volume_total: None,
    })
}

fn app_with_sortable_extensions() -> GuiApp {
    let mut app = app_with_one_file();
    app.extensions = vec![
        ExtensionRow {
            extension: ".zzz".to_string(),
            category: Category::Source,
            size: 300,
            count: 2,
        },
        ExtensionRow {
            extension: ".aaa".to_string(),
            category: Category::Programs,
            size: 10,
            count: 50,
        },
        ExtensionRow {
            extension: ".mmm".to_string(),
            category: Category::Archives,
            size: 100,
            count: 5,
        },
    ];
    app.extension_sort = ExtensionSortMode::BytesDesc;
    app.sort_extensions();
    app
}

fn raw_input_at_width(events: Vec<egui::Event>, width: f32) -> egui::RawInput {
    static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let frame = FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 500.0),
        )),
        events,
        time: Some(frame as f64 / 60.0),
        ..Default::default()
    }
}

fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
    raw_input_at_width(events, 900.0)
}

fn render_directory(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_directory_tree(app, ui));
    });
}

fn render_extensions(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_extension_list(app, ui));
    });
}

fn render_largest(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_largest_files(app, ui));
    });
}

fn render_search(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_search(app, ui));
    });
}

fn render_duplicates(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_duplicates(app, ui));
    });
}

fn render_file_area(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_file_area(app, ui));
    });
}

fn pointer_button(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn pointer_move(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![egui::Event::PointerMoved(pos)]
}

fn latest_header_position(
    headers: &'static std::sync::Mutex<Vec<(&'static str, egui::Rect)>>,
    label: &str,
) -> egui::Pos2 {
    let position = probe(headers)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .map(|(_, rect)| rect.center());
    assert!(
        position.is_some(),
        "the rendered {label} header should expose a drag target"
    );
    position.unwrap_or_default()
}

fn drag_directory_header(ctx: &egui::Context, app: &mut GuiApp, source: &str, target: &str) {
    probe(&TEST_DIRECTORY_HEADER_RECTS).clear();
    for _ in 0..4 {
        render_directory(ctx, app, raw_input(Vec::new()));
    }
    let source_pos = latest_header_position(&TEST_DIRECTORY_HEADER_RECTS, source);
    let target_pos = latest_header_position(&TEST_DIRECTORY_HEADER_RECTS, target);
    render_directory(ctx, app, raw_input(pointer_button(source_pos, true)));
    render_directory(
        ctx,
        app,
        raw_input(pointer_move(source_pos + egui::vec2(16.0, 0.0))),
    );
    render_directory(ctx, app, raw_input(pointer_move(target_pos)));
    render_directory(ctx, app, raw_input(pointer_button(target_pos, false)));
}

fn drag_extension_header(ctx: &egui::Context, app: &mut GuiApp, source: &str, target: &str) {
    probe(&TEST_EXTENSION_HEADER_RECTS).clear();
    for _ in 0..4 {
        render_extensions(ctx, app, raw_input(Vec::new()));
    }
    let source_pos = latest_header_position(&TEST_EXTENSION_HEADER_RECTS, source);
    let target_pos = latest_header_position(&TEST_EXTENSION_HEADER_RECTS, target);
    render_extensions(ctx, app, raw_input(pointer_button(source_pos, true)));
    render_extensions(
        ctx,
        app,
        raw_input(pointer_move(source_pos + egui::vec2(16.0, 0.0))),
    );
    render_extensions(ctx, app, raw_input(pointer_move(target_pos)));
    render_extensions(ctx, app, raw_input(pointer_button(target_pos, false)));
}

fn click_directory_header(
    ctx: &egui::Context,
    app: &mut GuiApp,
    label: &str,
) -> anyhow::Result<()> {
    probe(&TEST_DIRECTORY_HEADER_RECTS).clear();
    for _ in 0..4 {
        render_directory(ctx, app, raw_input(Vec::new()));
    }
    let position = probe(&TEST_DIRECTORY_HEADER_RECTS)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .map(|(_, rect)| rect.center())
        .with_context(|| format!("the rendered {label} header should expose a click target"))?;
    render_directory(ctx, app, raw_input(pointer_button(position, true)));
    render_directory(ctx, app, raw_input(pointer_button(position, false)));
    Ok(())
}

fn rendered_child_order(ctx: &egui::Context, app: &mut GuiApp) -> Vec<usize> {
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    render_directory(ctx, app, raw_input(Vec::new()));
    probe(&TEST_DIRECTORY_ROW_RECTS)
        .iter()
        .filter_map(|(path, _)| path.first().copied())
        .collect()
}

fn latest_header_icon(label: &str) -> Option<Icon> {
    probe(&TEST_DIRECTORY_HEADER_ICONS)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .and_then(|(_, icon)| *icon)
}

fn latest_directory_header_labels() -> Vec<&'static str> {
    let headers = probe(&TEST_DIRECTORY_HEADER_RECTS);
    let mut labels: Vec<_> = headers
        .iter()
        .rev()
        .take(8)
        .map(|(label, _)| *label)
        .collect();
    labels.reverse();
    labels
}

fn click_extension_header(
    ctx: &egui::Context,
    app: &mut GuiApp,
    label: &str,
) -> anyhow::Result<()> {
    probe(&TEST_EXTENSION_HEADER_RECTS).clear();
    for _ in 0..4 {
        render_extensions(ctx, app, raw_input(Vec::new()));
    }
    let position = probe(&TEST_EXTENSION_HEADER_RECTS)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .map(|(_, rect)| rect.center())
        .with_context(|| format!("the rendered {label} header should expose a click target"))?;
    render_extensions(ctx, app, raw_input(pointer_button(position, true)));
    render_extensions(ctx, app, raw_input(pointer_button(position, false)));
    Ok(())
}

fn rendered_extension_order(ctx: &egui::Context, app: &mut GuiApp) -> Vec<String> {
    probe(&TEST_EXTENSION_ROW_RECTS).clear();
    render_extensions(ctx, app, raw_input(Vec::new()));
    probe(&TEST_EXTENSION_ROW_RECTS)
        .iter()
        .map(|(extension, _)| extension.clone())
        .collect()
}

fn latest_extension_header_icon(label: &str) -> Option<Icon> {
    probe(&TEST_EXTENSION_HEADER_ICONS)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .and_then(|(_, icon)| *icon)
}

fn latest_extension_header_labels() -> Vec<&'static str> {
    let headers = probe(&TEST_EXTENSION_HEADER_RECTS);
    let mut labels: Vec<_> = headers
        .iter()
        .rev()
        .take(6)
        .map(|(label, _)| *label)
        .collect();
    labels.reverse();
    labels
}

fn assert_extension_header_click(
    ctx: &egui::Context,
    app: &mut GuiApp,
    label: &str,
    expected_mode: ExtensionSortMode,
    expected_order: &[&str],
    expected_icon: Icon,
) -> anyhow::Result<()> {
    click_extension_header(ctx, app, label)?;
    assert_eq!(app.extension_sort, expected_mode);
    assert_eq!(rendered_extension_order(ctx, app), expected_order);
    assert_eq!(latest_extension_header_icon(label), Some(expected_icon));
    Ok(())
}

#[test]
fn clicking_directory_headers_changes_and_toggles_sort_order() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_files();

    render_directory(&ctx, &mut app, raw_input(Vec::new()));
    assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronDown));
    assert_eq!(latest_header_icon("Name"), None);
    assert_eq!(latest_header_icon("Last change"), None);

    click_directory_header(&ctx, &mut app, "Name")?;
    assert!(matches!(app.sort, SortMode::NameAsc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
    assert_eq!(latest_header_icon("Name"), Some(Icon::ChevronUp));
    assert_eq!(latest_header_icon("Size"), None);
    click_directory_header(&ctx, &mut app, "Name")?;
    assert!(matches!(app.sort, SortMode::NameDesc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
    assert_eq!(latest_header_icon("Name"), Some(Icon::ChevronDown));

    click_directory_header(&ctx, &mut app, "Size")?;
    assert!(matches!(app.sort, SortMode::SizeDesc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
    assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronDown));
    assert_eq!(latest_header_icon("Name"), None);
    click_directory_header(&ctx, &mut app, "Size")?;
    assert!(matches!(app.sort, SortMode::SizeAsc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
    assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronUp));

    click_directory_header(&ctx, &mut app, "Last change")?;
    assert!(matches!(app.sort, SortMode::ModifiedDesc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
    assert_eq!(latest_header_icon("Last change"), Some(Icon::ChevronDown));
    assert_eq!(latest_header_icon("Size"), None);
    click_directory_header(&ctx, &mut app, "Last change")?;
    assert!(matches!(app.sort, SortMode::ModifiedAsc));
    assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
    assert_eq!(latest_header_icon("Last change"), Some(Icon::ChevronUp));
    Ok(())
}

#[test]
fn dragging_directory_header_reorders_headers_and_row_columns() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    drag_directory_header(&ctx, &mut app, "Name", "Files");
    assert_eq!(
        app.directory_column_order,
        [
            DirectoryColumn::Size,
            DirectoryColumn::SubtreePercentage,
            DirectoryColumn::PercentTotal,
            DirectoryColumn::Name,
            DirectoryColumn::Files,
            DirectoryColumn::Subdirs,
            DirectoryColumn::LastChange,
            DirectoryColumn::Attributes,
        ]
    );
    probe(&TEST_DIRECTORY_CELL_COLUMNS).clear();
    render_directory(&ctx, &mut app, raw_input(Vec::new()));
    assert_eq!(
        latest_directory_header_labels(),
        [
            "Size",
            "Subtree percentage",
            "% of total",
            "Name",
            "Files",
            "Subdirs",
            "Last change",
            "Attributes",
        ]
    );
    let child_columns: Vec<_> = probe(&TEST_DIRECTORY_CELL_COLUMNS)
        .iter()
        .filter(|(path, _)| path == &[0])
        .map(|(_, column)| *column)
        .collect();
    assert_eq!(child_columns, app.directory_column_order);
}

#[test]
fn extension_headers_sort_rendered_rows_and_show_direction() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_extensions();

    assert_eq!(
        rendered_extension_order(&ctx, &mut app),
        [".zzz", ".mmm", ".aaa"]
    );
    assert_eq!(
        latest_extension_header_icon("Bytes"),
        Some(Icon::ChevronDown)
    );
    assert_eq!(
        latest_extension_header_labels(),
        [
            "Extension",
            "Color",
            "Description",
            "Bytes",
            "% Bytes",
            "Files"
        ]
    );

    assert_extension_header_click(
        &ctx,
        &mut app,
        "Extension",
        ExtensionSortMode::ExtensionAsc,
        &[".aaa", ".mmm", ".zzz"],
        Icon::ChevronUp,
    )?;
    assert_eq!(latest_extension_header_icon("Bytes"), None);
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Extension",
        ExtensionSortMode::ExtensionDesc,
        &[".zzz", ".mmm", ".aaa"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Color",
        ExtensionSortMode::ColorAsc,
        &[".mmm", ".zzz", ".aaa"],
        Icon::ChevronUp,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Color",
        ExtensionSortMode::ColorDesc,
        &[".aaa", ".zzz", ".mmm"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Description",
        ExtensionSortMode::DescriptionAsc,
        &[".mmm", ".aaa", ".zzz"],
        Icon::ChevronUp,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Description",
        ExtensionSortMode::DescriptionDesc,
        &[".zzz", ".aaa", ".mmm"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Bytes",
        ExtensionSortMode::BytesDesc,
        &[".zzz", ".mmm", ".aaa"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Bytes",
        ExtensionSortMode::BytesAsc,
        &[".aaa", ".mmm", ".zzz"],
        Icon::ChevronUp,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "% Bytes",
        ExtensionSortMode::PercentDesc,
        &[".zzz", ".mmm", ".aaa"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "% Bytes",
        ExtensionSortMode::PercentAsc,
        &[".aaa", ".mmm", ".zzz"],
        Icon::ChevronUp,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Files",
        ExtensionSortMode::FilesDesc,
        &[".aaa", ".mmm", ".zzz"],
        Icon::ChevronDown,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Files",
        ExtensionSortMode::FilesAsc,
        &[".zzz", ".mmm", ".aaa"],
        Icon::ChevronUp,
    )?;
    Ok(())
}

#[test]
fn dragging_extension_header_reorders_headers_and_row_columns() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_extensions();

    drag_extension_header(&ctx, &mut app, "Extension", "Files");
    assert_eq!(
        app.extension_column_order,
        [
            ExtensionColumn::Color,
            ExtensionColumn::Description,
            ExtensionColumn::Bytes,
            ExtensionColumn::PercentBytes,
            ExtensionColumn::Extension,
            ExtensionColumn::Files,
        ]
    );
    probe(&TEST_EXTENSION_CELL_COLUMNS).clear();
    render_extensions(&ctx, &mut app, raw_input(Vec::new()));
    assert_eq!(
        latest_extension_header_labels(),
        [
            "Color",
            "Description",
            "Bytes",
            "% Bytes",
            "Extension",
            "Files"
        ]
    );
    let first_row_columns: Vec<_> = probe(&TEST_EXTENSION_CELL_COLUMNS)
        .iter()
        .filter(|(extension, _)| extension == ".zzz")
        .map(|(_, column)| *column)
        .collect();
    assert_eq!(first_row_columns, app.extension_column_order);
}

#[test]
fn application_labels_do_not_capture_text_selection() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    apply_style(&ctx);
    assert!(!ctx.style().interaction.selectable_labels);
    assert!(!ctx.style().interaction.multi_widget_text_select);
}

#[test]
fn professional_theme_has_distinct_layers_and_accessible_copy() {
    assert!(relative_luminance(APP_COLOR) < relative_luminance(PANEL_COLOR));
    assert!(relative_luminance(PANEL_COLOR) < relative_luminance(RAISED_COLOR));
    assert!(relative_luminance(RAISED_COLOR) < relative_luminance(HOVER_COLOR));
    assert!(contrast_ratio(PRIMARY_TEXT_COLOR, PANEL_COLOR) >= 7.0);
    assert!(contrast_ratio(SECONDARY_TEXT_COLOR, PANEL_COLOR) >= 4.5);
    assert!(contrast_ratio(ACCENT_COLOR, PANEL_COLOR) >= 4.5);
    let control_heights = [TABLE_HEADER_HEIGHT, TABLE_ROW_HEIGHT, VIEW_TAB_HEIGHT];
    assert!(control_heights.iter().all(|height| *height >= 30.0));
    assert!(control_heights[0] >= control_heights[1]);
}

#[test]
fn clicking_a_view_tab_switches_the_file_view() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_VIEW_TAB_RECTS).clear();
    for _ in 0..3 {
        render_file_area(&ctx, &mut app, raw_input(Vec::new()));
    }
    let search = probe(&TEST_VIEW_TAB_RECTS)
        .iter()
        .rev()
        .find(|(view, _)| *view == FileView::SearchResults)
        .map(|(_, rect)| rect.center())
        .context("the Search Results tab should render a click target")?;

    render_file_area(&ctx, &mut app, raw_input(pointer_button(search, true)));
    render_file_area(&ctx, &mut app, raw_input(pointer_button(search, false)));

    assert_eq!(app.file_view, FileView::SearchResults);
    Ok(())
}

#[test]
fn menu_icons_never_overlap_their_labels() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    probe(&TEST_ICON_MENU_LAYOUTS).clear();
    let _ = ctx.run(raw_input(Vec::new()), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            icon_selectable_label(ui, true, Icon::Tree, "All files");
            icon_button(ui, true, Icon::Settings, "     Settings…");
            icon_button(ui, false, Icon::Duplicate, "     Duplicate Files");
        });
    });

    let layouts = probe(&TEST_ICON_MENU_LAYOUTS);
    assert_eq!(layouts.len(), 3);
    for (label, item, icon, text) in layouts.iter() {
        assert!(
            icon.right() + 7.5 <= text.left(),
            "{label} icon overlaps its text: icon={icon:?}, text={text:?}"
        );
        assert!(
            item.contains_rect(*icon) && item.contains_rect(*text),
            "{label} content escaped its clickable row: item={item:?}, icon={icon:?}, text={text:?}"
        );
    }
}

#[test]
fn menu_rows_align_and_keep_shortcuts_off_their_labels() {
    // The menus used to fake these columns by padding one string with
    // leading and interior spaces. Spaces are proportional, so nothing
    // actually lined up and long labels ran into their shortcuts.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    probe(&TEST_MENU_ITEM_LAYOUTS).clear();
    let _ = ctx.run(raw_input(Vec::new()), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            menu_action(ui, true, Icon::FolderOpen, "Select folder…", "Ctrl+O");
            menu_action(ui, true, Icon::Refresh, "Rescan", "F5");
            menu_action(ui, true, Icon::Trash, "Delete to Recycle Bin", "Del");
            icon_button(ui, true, Icon::Export, "Export CSV…");
            menu_toggle(ui, &mut true, "Grid lines");
            menu_choice(ui, true, "Logical size");
        });
    });

    let layouts = probe(&TEST_MENU_ITEM_LAYOUTS);
    assert_eq!(layouts.len(), 6);

    let width = layouts[0].row.width();
    for item in layouts.iter() {
        assert!(
            (item.row.width() - width).abs() < 0.5,
            "{} is {}px wide but the first row is {width}px — menu rows must share one width",
            item.label,
            item.row.width()
        );
        assert!(
            item.row.width() >= MENU_MIN_WIDTH - 0.5,
            "{} collapsed to {}px, below the {MENU_MIN_WIDTH}px menu floor",
            item.label,
            item.row.width()
        );
        assert!(
            item.icon.right() + MENU_ICON_GAP - 0.5 <= item.text.left(),
            "{} label starts before its icon column ends",
            item.label
        );
        if let Some(shortcut) = item.shortcut {
            assert!(
                item.text.right() < shortcut.left(),
                "{} label overlaps its shortcut: label ends at {}, shortcut starts at {}",
                item.label,
                item.text.right(),
                shortcut.left()
            );
            assert!(
                item.row.contains_rect(shortcut),
                "{} shortcut escaped its row",
                item.label
            );
        }
    }

    // Every shortcut is right-aligned to the same column.
    let shortcut_rights: Vec<f32> = layouts
        .iter()
        .filter_map(|item| item.shortcut.map(|s| s.right()))
        .collect();
    assert_eq!(shortcut_rights.len(), 3);
    for right in &shortcut_rights {
        assert!(
            (right - shortcut_rights[0]).abs() < 0.5,
            "shortcuts are not in one column: {shortcut_rights:?}"
        );
    }
}

#[test]
fn clicking_a_rendered_directory_row_changes_selection() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    // Table column widths settle over the first few immediate-mode
    // frames, just as they do while the native window is opening.
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input(Vec::new()));
    }

    let child_row = probe(&TEST_DIRECTORY_ROW_RECTS)
        .iter()
        .rev()
        .find(|(path, _)| path == &[0])
        .map(|(_, rect)| egui::pos2(rect.center().x, rect.max.y - 3.0))
        .context("the rendered child row should expose a response rectangle")?;
    render_directory(&ctx, &mut app, raw_input(pointer_button(child_row, true)));
    render_directory(&ctx, &mut app, raw_input(pointer_button(child_row, false)));

    assert_eq!(
        app.selected_path,
        Some(vec![0]),
        "row states: {:?}",
        *probe(&TEST_DIRECTORY_ROW_RECTS)
    );
    Ok(())
}

#[test]
fn directory_table_expands_to_fill_a_wider_pane() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 900.0));
    }
    let narrow_width = probe(&TEST_DIRECTORY_ROW_RECTS)
        .last()
        .context("directory row should render")?
        .1
        .width();

    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 1500.0));
    }
    let wide_width = probe(&TEST_DIRECTORY_ROW_RECTS)
        .last()
        .context("directory row should render after resize")?
        .1
        .width();

    assert!(
        wide_width > narrow_width + 500.0,
        "table should absorb pane growth: narrow={narrow_width}, wide={wide_width}, screen={}",
        ctx.screen_rect().width()
    );
    Ok(())
}

#[test]
fn clicking_a_rendered_extension_row_changes_highlight() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_EXTENSION_ROW_RECTS).clear();
    probe(&TEST_EXTENSION_TEXT_RECTS).clear();
    for _ in 0..4 {
        render_extensions(&ctx, &mut app, raw_input(Vec::new()));
    }
    let row_pos = probe(&TEST_EXTENSION_TEXT_RECTS)
        .iter()
        .rev()
        .find(|(extension, _)| extension == ".txt")
        .map(|(_, rect)| rect.center())
        .context("the extension text should expose its rendered rectangle")?;
    render_extensions(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
    render_extensions(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
    assert_eq!(app.highlighted_extension.as_deref(), Some(".txt"));
    Ok(())
}

#[test]
fn clicking_a_rendered_largest_file_row_changes_selection() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_LARGEST_ROW_RECTS).clear();
    for _ in 0..4 {
        render_largest(&ctx, &mut app, raw_input(Vec::new()));
    }
    let row_pos = probe(&TEST_LARGEST_ROW_RECTS)
        .iter()
        .rev()
        .find(|(index, _)| *index == 0)
        .map(|(_, rect)| egui::pos2(rect.left() + 45.0, rect.center().y))
        .context("the largest-file row should render")?;
    render_largest(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
    render_largest(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
    assert_eq!(app.selected_path, Some(vec![0]));
    Ok(())
}

#[test]
fn clicking_a_rendered_search_result_changes_selection() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    app.search_query = "*".to_string();
    app.run_search();
    probe(&TEST_SEARCH_ROW_RECTS).clear();
    for _ in 0..4 {
        render_search(&ctx, &mut app, raw_input(Vec::new()));
    }
    let row_pos = probe(&TEST_SEARCH_ROW_RECTS)
        .iter()
        .rev()
        .find(|(path, _)| path == &[0])
        .map(|(_, rect)| rect.center())
        .context("the search result should render")?;
    render_search(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
    render_search(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
    assert_eq!(app.selected_path, Some(vec![0]));
    Ok(())
}

#[test]
fn clicking_a_rendered_duplicate_member_changes_selection() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    app.duplicate_groups = vec![crate::duplicates::DupGroup {
        size: 128,
        files: vec![crate::duplicates::DupFile {
            index_path: vec![0],
        }],
    }];
    probe(&TEST_DUPLICATE_ROW_RECTS).clear();
    for _ in 0..4 {
        render_duplicates(&ctx, &mut app, raw_input(Vec::new()));
    }
    let row_pos = probe(&TEST_DUPLICATE_ROW_RECTS)
        .iter()
        .rev()
        .find(|(path, _)| path == &[0])
        .map(|(_, rect)| rect.center())
        .context("the duplicate member should render")?;
    render_duplicates(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
    render_duplicates(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
    assert_eq!(app.selected_path, Some(vec![0]));
    Ok(())
}
