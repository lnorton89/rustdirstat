//! Interaction tests: drive the real drawing code through an egui
//! context, then assert against the geometry it recorded in
//! [`super::probes`].
//!
//! These click where the app actually drew a row rather than where a
//! test author guessed it would be, so a layout change that moves a
//! control out from under its own click target fails here instead of
//! in the user's hands.

use super::chrome::*;
use super::directory::*;
use super::draw_file_area;
use super::extensions::*;
use super::lists::*;
use super::probes::*;
use super::theme::*;
use super::themes;
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

/// A root holding one folder and one file, so the two kinds of tree row
/// appear at the same depth and their layouts can be compared without a
/// depth indent standing between them.
fn app_with_a_folder_beside_a_file() -> GuiApp {
    let leaf = file("inside.bin", 64);
    let folder = Node {
        name: "sub".to_string(),
        is_dir: true,
        is_symlink: false,
        size: 64,
        physical_size: 64,
        file_count: 1,
        dir_count: 0,
        modified: None,
        children: vec![leaf],
        error: false,
        category: None,
        ext_totals: Vec::new(),
        unreadable_count: 0,
    };
    let sibling = file("beside.txt", 128);
    let mut totals = vec![(0, 0, 0); Category::COUNT];
    totals[category_index("inside.bin")] = (64, 64, 1);
    totals[category_index("beside.txt")] = (128, 128, 1);
    GuiApp::new(Tree {
        root_path: PathBuf::from("C:\\test-root"),
        root: Node {
            name: "test-root".to_string(),
            is_dir: true,
            is_symlink: false,
            size: 192,
            physical_size: 192,
            file_count: 2,
            dir_count: 1,
            modified: None,
            children: vec![folder, sibling],
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
fn treemap_label_text_stays_readable_on_any_tile() {
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
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_directory_tree(app, ui));
    });
}

fn render_extensions(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_extension_list(app, ui));
    });
}

fn render_largest(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_largest_files(app, ui));
    });
}

fn render_search(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_search(app, ui));
    });
}

fn render_duplicates(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_duplicates(app, ui));
    });
}

fn render_file_area(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| draw_file_area(app, ui));
    });
}

/// Draws the treemap into a panel framed exactly as the real one is, so
/// the tiles land where they land in the window rather than at whatever
/// coordinates a bare `CentralPanel` happens to hand out.
fn render_treemap(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default()
            .frame(super::theme::panel_frame())
            .show(ctx, |ui| draw_treemap(app, ui));
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

/// Whether a tooltip is on screen right now.
fn tooltip_is_showing(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| {
        memory
            .layer_ids()
            .any(|layer| layer.order == egui::Order::Tooltip)
    })
}

/// A tooltip has to appear without waiting for a timer.
///
/// This app repaints on input. Once the pointer stops there are no
/// further frames, so egui's defaults — wait half a second, and only once
/// the pointer holds still — have nothing to elapse on, and the tip
/// showed only when some other animation happened to be driving repaints.
/// That is what made the toolbar's tips work erratically rather than not
/// at all.
///
/// The first half of this test asserts the mechanism rather than the
/// symptom, deliberately. The failure is *the absence of frames*, and a
/// test harness works by supplying frames — an earlier version of this
/// test rendered a settled frame and passed just as happily with the fix
/// deleted, because the harness handed egui the very frame the real app
/// never produces. Pinning the two style fields is the part that can
/// actually fail; the render below then checks the wiring is intact.
#[test]
fn a_toolbar_button_shows_its_tooltip_without_waiting_for_a_timer() {
    {
        let ctx = egui::Context::default();
        let app = app_with_one_file();
        apply_style(&ctx, app.palette);
        let interaction = ctx.style().interaction.clone();
        assert_eq!(
            interaction.tooltip_delay, 0.0,
            "a tooltip delay needs a frame to elapse on, and an input-driven repaint \
             loop does not produce one once the pointer stops"
        );
        assert!(
            !interaction.show_tooltips_only_when_still,
            "waiting for the pointer to hold still needs a frame after it stops, which \
             is exactly the frame this app does not draw"
        );
    }
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    // Settle the layout so the buttons are where they will finally be.
    for _ in 0..3 {
        let _ = ctx.run(raw_input_at_width(Vec::new(), 1400.0), |ctx| {
            apply_style(ctx, app.palette);
            draw_toolbar(&mut app, ctx);
        });
    }
    assert!(
        !tooltip_is_showing(&ctx),
        "a tooltip is showing before the pointer has gone anywhere near a button"
    );

    // The first toolbar button sits just right of the app mark.
    let over_button = egui::pos2(150.0, 40.0);
    let _ = ctx.run(
        raw_input_at_width(pointer_move(over_button), 1400.0),
        |ctx| {
            apply_style(ctx, app.palette);
            draw_toolbar(&mut app, ctx);
        },
    );
    // One further frame with no events at all: the pointer is now still,
    // which is precisely the state the old defaults never got a frame in.
    let _ = ctx.run(raw_input_at_width(Vec::new(), 1400.0), |ctx| {
        apply_style(ctx, app.palette);
        draw_toolbar(&mut app, ctx);
    });

    assert!(
        tooltip_is_showing(&ctx),
        "no tooltip after the pointer settled on a toolbar button — it is waiting for a \
         timer that an input-driven repaint loop will never give it a frame to run"
    );
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
    // Rendered wide enough that every header is actually on screen, at a
    // width measured from the headers rather than guessed. The trailing
    // columns only fit a narrow pane if some column gives up width, and
    // none of them do any more: every column is resizable now, so the
    // slack sits in the last one and the rest keep the width they were
    // given. Past the pane edge a click lands on nothing, which would
    // fail this test for a reason that has nothing to do with sorting.
    let mut width = 1200.0;
    for _ in 0..2 {
        probe(&TEST_DIRECTORY_HEADER_RECTS).clear();
        for _ in 0..4 {
            render_directory(ctx, app, raw_input_at_width(Vec::new(), width));
        }
        let rightmost = probe(&TEST_DIRECTORY_HEADER_RECTS)
            .iter()
            .map(|(_, rect)| rect.right())
            .fold(0.0_f32, f32::max);
        if rightmost <= width - 32.0 {
            break;
        }
        width = rightmost + 96.0;
    }
    let position = probe(&TEST_DIRECTORY_HEADER_RECTS)
        .iter()
        .rev()
        .find(|(header, _)| *header == label)
        .map(|(_, rect)| rect.center())
        .with_context(|| format!("the rendered {label} header should expose a click target"))?;
    render_directory(
        ctx,
        app,
        raw_input_at_width(pointer_button(position, true), width),
    );
    render_directory(
        ctx,
        app,
        raw_input_at_width(pointer_button(position, false), width),
    );
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

/// The three test extensions in the order the Color column should put
/// them: by the hue they are painted at.
fn sorted_by_painted_hue(ascending: bool) -> Vec<&'static str> {
    let mut extensions = vec![".aaa", ".mmm", ".zzz"];
    extensions.sort_by_key(|extension| {
        let hue = crate::color::extension_hue(extension) as i64;
        if ascending {
            hue
        } else {
            -hue
        }
    });
    extensions
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
    // Derived from the hue each extension is actually painted at, not
    // written out as a literal permutation. A literal only records what
    // one particular hash happened to return: it does not say the column
    // sorts by the color the user can see, and it has to be re-derived
    // by hand every time the hue changes — which is exactly what
    // happened when the two front ends stopped hashing differently.
    let ascending = sorted_by_painted_hue(true);
    let descending = sorted_by_painted_hue(false);
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Color",
        ExtensionSortMode::ColorAsc,
        &ascending,
        Icon::ChevronUp,
    )?;
    assert_extension_header_click(
        &ctx,
        &mut app,
        "Color",
        ExtensionSortMode::ColorDesc,
        &descending,
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
    apply_style(&ctx, themes::Palette::default());
    assert!(!ctx.style().interaction.selectable_labels);
    assert!(!ctx.style().interaction.multi_widget_text_select);
}

#[test]
fn control_heights_stay_comfortable() {
    let control_heights = [TABLE_HEADER_HEIGHT, TABLE_ROW_HEIGHT, VIEW_TAB_HEIGHT];
    assert!(control_heights.iter().all(|height| *height >= 30.0));
    assert!(control_heights[0] >= control_heights[1]);
}

#[test]
fn bundled_catalog_parses_and_every_theme_survives() {
    let bundled = include_str!("../../../assets/themes.toml");
    let parsed = themes::parse_catalog(bundled);
    // One count against the other: `parse_catalog` drops any theme with
    // an unparseable color, so a typo would otherwise show up only as a
    // theme quietly missing from the picker.
    let declared = bundled.matches("[[theme]]").count();
    assert_eq!(
        parsed.len(),
        declared,
        "{} themes are declared but only {} parsed — check for a bad hex color",
        declared,
        parsed.len()
    );
    assert!(declared >= 15, "the catalog should not have shrunk");
}

#[test]
fn hex_colors_parse_in_every_accepted_form() {
    let expected = Some(Color32::from_rgb(0xaa, 0xbb, 0xcc));
    assert_eq!(themes::parse_hex("#aabbcc"), expected);
    assert_eq!(themes::parse_hex("aabbcc"), expected);
    assert_eq!(themes::parse_hex("#abc"), expected);
    assert_eq!(themes::parse_hex("  #AABBCC  "), expected);
    for bad in ["", "#", "#ab", "#gggggg", "#aabbccdd"] {
        assert!(themes::parse_hex(bad).is_none(), "{bad} should not parse");
    }
}

/// The one test standing between a new theme and an unreadable window.
///
/// Layer separation is checked by polarity rather than absolutely: a
/// dark theme lifts a surface toward the viewer by getting lighter, and
/// a light theme lifts `hover` by getting *darker*, so asserting one
/// fixed direction would either reject every light theme or check
/// nothing at all.
#[test]
fn theme_layers_are_distinct_and_copy_is_readable() {
    /// Minimum perceptual-lightness gap, in L* units, between two layers
    /// that have to be tellable apart. Roughly the smallest step that
    /// still reads as a step on a real display.
    const MIN_STEP: f32 = 1.5;

    let all = themes::themes();
    assert!(!all.is_empty(), "the catalog should never be empty");
    for spec in all {
        let p = themes::Palette::from_spec(spec);
        let name = &spec.name;
        let step =
            |from: Color32, to: Color32| perceptual_lightness(to) - perceptual_lightness(from);

        // app -> panel -> raised always moves toward the viewer, and in
        // both polarities that direction is lighter. It is only `hover`
        // that flips: a pale surface reads as lifted by darkening.
        assert!(
            step(p.app, p.panel) >= MIN_STEP,
            "{name}: panel is only {:.2} L* from app, too close to tell apart",
            step(p.app, p.panel)
        );
        assert!(
            step(p.panel, p.raised) >= MIN_STEP,
            "{name}: raised is only {:.2} L* from panel, too close to tell apart",
            step(p.panel, p.raised)
        );
        if p.mode.is_dark() {
            assert!(
                step(p.raised, p.hover) >= MIN_STEP,
                "{name}: a dark theme's hover must be lighter than the surface it lifts,                  but it is {:.2} L* away",
                step(p.raised, p.hover)
            );
        } else {
            assert!(
                step(p.raised, p.hover) <= -MIN_STEP,
                "{name}: a light theme's hover must be darker than the surface it lifts,                  but it is {:.2} L* away",
                step(p.raised, p.hover)
            );
        }

        assert!(
            contrast_ratio(p.primary_text, p.panel) >= 7.0,
            "{name}: primary text is {:.2}:1 on panel, below AAA",
            contrast_ratio(p.primary_text, p.panel)
        );
        assert!(
            contrast_ratio(p.secondary_text, p.panel) >= 4.5,
            "{name}: secondary text is {:.2}:1 on panel, below AA",
            contrast_ratio(p.secondary_text, p.panel)
        );
        assert!(
            contrast_ratio(p.accent, p.panel) >= 4.5,
            "{name}: accent is {:.2}:1 on panel, below AA",
            contrast_ratio(p.accent, p.panel)
        );
        // Derived colors have to clear the bar too. These are the ones a
        // theme author never sees and so cannot check by eye.
        assert!(
            contrast_ratio(p.on_accent, p.accent_muted) >= 4.5,
            "{name}: text on a selected row is {:.2}:1",
            contrast_ratio(p.on_accent, p.accent_muted)
        );
        assert!(
            contrast_ratio(p.danger_text, p.danger_bg) >= 4.5,
            "{name}: danger callout text is {:.2}:1",
            contrast_ratio(p.danger_text, p.danger_bg)
        );
        assert!(
            contrast_ratio(p.warning_text, p.warning_bg) >= 4.5,
            "{name}: warning callout text is {:.2}:1",
            contrast_ratio(p.warning_text, p.warning_bg)
        );
        assert!(
            contrast_ratio(p.primary_text, p.raised) >= 4.5,
            "{name}: primary text on a card is {:.2}:1",
            contrast_ratio(p.primary_text, p.raised)
        );
    }
}

#[test]
fn every_theme_id_is_unique_and_stable() {
    let mut seen = std::collections::HashSet::new();
    for spec in themes::themes() {
        assert!(
            seen.insert(spec.id.clone()),
            "duplicate theme id {}",
            spec.id
        );
        assert!(
            !spec.id.is_empty() && !spec.name.is_empty(),
            "a theme needs both an id and a name"
        );
    }
    assert!(
        themes::spec_by_id(themes::default_theme_id()).is_some(),
        "the default theme id should resolve"
    );
    assert!(
        themes::spec_by_id("no-such-theme").is_none(),
        "an unknown id should not resolve"
    );
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
fn menu_bar_names_are_clearly_separated() {
    // Regression test for the menu bar coming out cramped no matter what
    // spacing the code asked for. `egui::menu::bar` runs its own
    // `set_menu_style` on the child Ui as its first act, which resets
    // button_padding to (2, 0) — so spacing configured before the call
    // was silently thrown away, and nothing said so. Measuring the gaps
    // the bar actually produced is the only way to catch that; asserting
    // on the constants would have passed the whole time.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_MENU_BAR_RECTS).clear();
    let _ = ctx.run(raw_input_at_width(Vec::new(), 1280.0), |ctx| {
        apply_style(ctx, app.palette);
        draw_menu_bar(&mut app, ctx);
    });

    let names = probe(&TEST_MENU_BAR_RECTS);
    assert_eq!(
        names.len(),
        7,
        "expected every top-level menu to be recorded, got {names:?}"
    );

    // Measure the padding rather than assuming it. Deriving the gap from
    // the configured constant is what made the first version of this test
    // useless: it passed with the padding line deleted, because the
    // formula supplied the very number it was supposed to be checking.
    // The text width has to come from the same font the bar laid out
    // with, so ask the context.
    let font = egui::FontId::new(14.0, egui::FontFamily::Proportional);
    let text_width = |label: &str| {
        ctx.fonts(|fonts| {
            fonts
                .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        })
    };

    for (label, rect) in names.iter() {
        let side_padding = (rect.width() - text_width(label)) / 2.0;
        assert!(
            side_padding >= MENU_BAR_MIN_SIDE_PADDING,
            "{label} has only {side_padding:.1}px of padding beside its text \
             (target {:.1}px wide, text {:.1}px); egui's menu style resets this \
             to 2px unless the bar sets it from inside",
            rect.width(),
            text_width(label)
        );
        assert!(
            rect.height() >= 24.0,
            "{label} is only {:.1}px tall, so the hover highlight sits flush against the text",
            rect.height()
        );
    }

    for pair in names.windows(2) {
        let (left_label, left) = &pair[0];
        let (right_label, right) = &pair[1];
        assert!(
            left.right() <= right.left() + 0.5,
            "{left_label} and {right_label} overlap"
        );
        let gap = right.left() - left.right();
        assert!(
            gap >= MENU_BAR_MIN_GAP,
            "only {gap:.1}px between the {left_label} and {right_label} targets, \
             which reads as one run of words"
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

/// The extension panel has to be draggable narrower than the columns it
/// contains.
///
/// Its content used to force a floor on the panel: the table stated a
/// minimum width, that propagated out through the panel, and the divider
/// then refused to go past it. Rendering into a real `SidePanel` is the
/// only way to see that — measuring the table alone cannot, because the
/// table is not what was refusing.
/// Every extension column can be dragged, the first one included.
///
/// "ext columns dont resize" was reported on its own, and there was no
/// test to catch it because the only column-drag helper could reach the
/// directory table and nothing else. `Extension` was the `remainder()`
/// absorbing the pane's slack, and a `remainder()` cannot be resizable,
/// so the first column here was pinned for the same reason it was there.
#[test]
fn the_extension_table_resizes_its_columns_including_the_first() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_extensions();
    // Rendered before anything is measured. Without this the first
    // `before` reads an empty probe and comes back 0.0, and "wider than
    // 0.0 + 20" is true of any column at all — the test passed with the
    // fix reverted until this loop was added.
    probe(&TEST_EXTENSION_HEADER_RECTS).clear();
    for _ in 0..4 {
        render_extensions(&ctx, &mut app, raw_input_at_width(Vec::new(), 1400.0));
    }

    // `Files` is deliberately not in this list: it is the last column,
    // so it is the one absorbing the pane's slack and the one column
    // that is meant to be pinned.
    for header in ["Extension", "Bytes"] {
        let before = header_width(&TEST_EXTENSION_HEADER_RECTS, header);
        drag_column_border(
            render_extensions,
            &TEST_EXTENSION_HEADER_RECTS,
            &ctx,
            &mut app,
            header,
            60.0,
        );
        let after = header_width(&TEST_EXTENSION_HEADER_RECTS, header);
        assert!(
            after > before + 20.0,
            "dragging the {header} border 60px right moved it from {before:.0}px to \
             {after:.0}px — the column is pinned, not resizable"
        );
    }
}

#[test]
fn the_extension_panel_can_be_dragged_narrower_than_its_columns() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_extensions();

    const REQUESTED: f32 = 200.0;
    let mut panel_width = f32::INFINITY;
    apply_style(&ctx, app.palette);
    for _ in 0..4 {
        let _ = ctx.run(raw_input_at_width(Vec::new(), 1400.0), |ctx| {
            let response = egui::SidePanel::right("test_extension_panel")
                .resizable(true)
                .min_width(0.0)
                .default_width(REQUESTED)
                .show(ctx, |ui| draw_extension_list(&mut app, ui));
            panel_width = response.response.rect.width();
        });
    }

    assert!(
        panel_width <= REQUESTED + 1.0,
        "the panel came out {panel_width:.0}px wide when asked for {REQUESTED:.0}px, so its \
         contents are setting a floor the divider cannot be dragged past"
    );
}

/// Horizontal gap between columns, which is where the resize handle
/// lives. Matches `apply_style`'s `item_spacing.x`.
const COLUMN_GAP: f32 = 8.0;

/// Width of a named header on the most recent frame.
/// The width a header was last rendered at.
///
/// Missing headers assert rather than reading 0.0. A silent zero makes
/// "the column got wider" true of any column at all, and a resize test
/// written that way passed with the fix reverted.
fn header_width(
    headers: &'static std::sync::Mutex<Vec<(&'static str, egui::Rect)>>,
    label: &str,
) -> f32 {
    let width = probe(headers)
        .iter()
        .rev()
        .find(|(seen, _)| *seen == label)
        .map(|(_, rect)| rect.width());
    assert!(
        width.is_some(),
        "{label} has not been rendered, so it has no width to compare against"
    );
    width.unwrap_or_default()
}

/// Drags the border on the right-hand edge of a header, the way a user
/// resizing a column does.
///
/// Takes the render function and header probe rather than assuming the
/// directory table: the extension table had the same pinned first column
/// and no test of its own, because the only drag helper could not reach
/// it.
fn drag_column_border(
    render: fn(&egui::Context, &mut GuiApp, egui::RawInput),
    headers: &'static std::sync::Mutex<Vec<(&'static str, egui::Rect)>>,
    ctx: &egui::Context,
    app: &mut GuiApp,
    label: &str,
    by: f32,
) {
    probe(headers).clear();
    for _ in 0..4 {
        render(ctx, app, raw_input_at_width(Vec::new(), 1400.0));
    }
    let edge = probe(headers)
        .iter()
        .rev()
        .find(|(seen, _)| *seen == label)
        // The grab strip is in the gap *between* columns, one full
        // `item_spacing.x` right of the cell's own edge — not on the
        // visible boundary, which is where an earlier version of this
        // test aimed and consequently reported resizing as broken when
        // it was working.
        .map(|(_, rect)| egui::pos2(rect.right() + COLUMN_GAP, rect.center().y));
    assert!(
        edge.is_some(),
        "{label} did not render, so it has no border"
    );
    let edge = edge.unwrap_or_default();

    render(
        ctx,
        app,
        raw_input_at_width(pointer_button(edge, true), 1400.0),
    );
    for step in 1..=4 {
        let to = edge + egui::vec2(by * step as f32 / 4.0, 0.0);
        render(ctx, app, raw_input_at_width(pointer_move(to), 1400.0));
    }
    let end = edge + egui::vec2(by, 0.0);
    render(
        ctx,
        app,
        raw_input_at_width(pointer_button(end, false), 1400.0),
    );
    probe(headers).clear();
    for _ in 0..3 {
        render(ctx, app, raw_input_at_width(Vec::new(), 1400.0));
    }
}

/// The four table-sizing behaviours, asserted together.
///
/// They are in one test on purpose. Each of them has been broken at some
/// point by a change made to fix one of the others — a fixed column width
/// makes resizing work and stops the table filling the pane; a scroll
/// area gives a narrow pane somewhere to scroll and stops `remainder`
/// expanding; a `remainder` that inherits `resizable` stops absorbing
/// slack. Asserting them separately let each fix look green while it
/// regressed a sibling.
#[test]
fn the_directory_table_fills_resizes_and_scrolls() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    // 1. The pane is filled at every width, never left with dead space
    //    beside the last column.
    //
    //    Measured against the viewport the table itself reports, not
    //    against a previous measurement. "It got 400px wider" was the
    //    old assertion, and that also passes for a table that grows by
    //    400px while still leaving 100px of dead space next to it.
    //    Reaching the viewport edge is the property actually wanted, and
    //    together with (2) below — a roomy pane must not scroll — it
    //    pins the table's width from both sides at once.
    for width in [900.0, 1400.0, 1500.0] {
        // A fresh context per width. Sharing one across widths measures
        // something else: `egui_extras` keeps a resizable column's width
        // in table state and only ever grows the `remainder()`, so a
        // table that has been wide once stays wide. That is worth its own
        // fix; it is not what "a pane of this width is filled" means.
        let ctx = egui::Context::default();
        probe(&TEST_DIRECTORY_ROW_RECTS).clear();
        probe(&TEST_DIRECTORY_SCROLL).clear();
        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), width));
        }
        let row = probe(&TEST_DIRECTORY_ROW_RECTS)
            .last()
            .map_or(0.0, |(_, rect)| rect.width());
        let viewport = probe(&TEST_DIRECTORY_SCROLL)
            .last()
            .map_or(0.0, |&(_, visible)| visible);
        assert!(
            row + 1.0 >= viewport,
            "in a {width:.0}px pane the table is {row:.0}px wide inside a {viewport:.0}px \
             viewport, leaving {:.0}px of dead space beside it",
            viewport - row
        );
    }

    // 2. A wide pane does not scroll, because it does not need to.
    probe(&TEST_DIRECTORY_SCROLL).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 1400.0));
    }
    let roomy = probe(&TEST_DIRECTORY_SCROLL).last().copied();
    assert!(
        roomy.is_some(),
        "the table should report its scroll extents"
    );
    let (content, visible) = roomy.unwrap_or_default();
    assert!(
        content <= visible + 1.0,
        "a 1400px pane scrolls with {content:.0}px of content in {visible:.0}px of viewport"
    );

    // 3. Dragging a column border actually resizes that column —
    //    including the first one.
    //
    //    `Name` is named explicitly because it was the one column that
    //    could not be dragged: it was the `remainder()` soaking up the
    //    pane's slack, and a `remainder()` stops absorbing the moment it
    //    is made resizable. Exercising only `Size` missed that, and "the
    //    first column isn't resizable" had to be reported twice before
    //    any test covered it.
    for header in ["Name", "Size"] {
        let before = header_width(&TEST_DIRECTORY_HEADER_RECTS, header);
        drag_column_border(
            render_directory,
            &TEST_DIRECTORY_HEADER_RECTS,
            &ctx,
            &mut app,
            header,
            60.0,
        );
        let after = header_width(&TEST_DIRECTORY_HEADER_RECTS, header);
        assert!(
            after > before + 20.0,
            "dragging the {header} border 60px right moved it from {before:.0}px to \
             {after:.0}px — the column is pinned, not resizable"
        );
    }

    // 4. A pane too narrow for the columns scrolls rather than clipping.
    probe(&TEST_DIRECTORY_SCROLL).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 260.0));
    }
    let squeezed = probe(&TEST_DIRECTORY_SCROLL).last().copied();
    assert!(
        squeezed.is_some(),
        "the table should report its scroll extents"
    );
    let (content, visible) = squeezed.unwrap_or_default();
    assert!(
        content > visible + 1.0,
        "a 260px pane reported {content:.0}px of content in {visible:.0}px of viewport, \
         so the columns past the edge cannot be reached"
    );
}

#[test]
fn a_squeezed_directory_pane_scrolls_instead_of_clipping() {
    // Dragging the treemap splitter left far enough squeezes this pane
    // below even the compact column set. The table used to just get
    // clipped, with no scrollbar and no way to reach the columns, which
    // reads as the pane being broken rather than small. Keeping the
    // content at its minimum width is what puts a scrollbar there.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 260.0));
    }

    let squeezed = probe(&TEST_DIRECTORY_SCROLL).last().copied();
    assert!(
        squeezed.is_some(),
        "the directory table should have reported its scroll extents"
    );
    let (content, visible) = squeezed.unwrap_or_default();
    assert!(
        content > visible + 1.0,
        "in a 260px pane the table reported {content:.0}px of content in {visible:.0}px \
         of viewport, so there is nothing to scroll and the columns past the edge \
         are simply unreachable"
    );

    // And a pane with room to spare must not grow a scrollbar it does
    // not need.
    probe(&TEST_DIRECTORY_SCROLL).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 1400.0));
    }
    let roomy = probe(&TEST_DIRECTORY_SCROLL).last().copied();
    assert!(
        roomy.is_some(),
        "the directory table should have reported its scroll extents"
    );
    let (content, visible) = roomy.unwrap_or_default();
    assert!(
        content <= visible + 1.0,
        "a 1400px pane still reported {content:.0}px of content in {visible:.0}px of \
         viewport, so it scrolls when it has no reason to"
    );
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
        wide_width > narrow_width,
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

// ---------------------------------------------------------------- modal

/// Draws the modal layer for one frame at a given viewport size.
fn render_modal_at(
    ctx: &egui::Context,
    app: &mut GuiApp,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) {
    apply_style(ctx, app.palette);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..raw_input(Vec::new())
    };
    let _ = ctx.run(input, |ctx| super::modal::draw_modal(app, ctx));
}

/// Frames to run before the modal has painted anything.
///
/// The card deliberately waits on a screenshot that never arrives under
/// `egui::Context::default` — there is no renderer behind it — so these
/// frames are the backdrop giving up and falling back to a plain scrim.
/// A test that probed earlier would be measuring the wait, not the UI.
const SETTLE_FRAMES: usize = 6;

fn settle_modal(ctx: &egui::Context, app: &mut GuiApp, size: egui::Vec2) {
    for _ in 0..SETTLE_FRAMES {
        render_modal_at(ctx, app, Vec::new(), size);
    }
}

fn open_and_settle(
    ctx: &egui::Context,
    app: &mut GuiApp,
    page: super::modal::ModalPage,
) -> egui::Vec2 {
    let size = egui::vec2(1100.0, 760.0);
    app.open_modal(page);
    probe(&TEST_MODAL_SCRIM_RECTS).clear();
    probe(&TEST_MODAL_CARD_RECTS).clear();
    probe(&TEST_MODAL_NAV_RECTS).clear();
    settle_modal(ctx, app, size);
    size
}

#[test]
fn an_open_modal_covers_the_whole_window_behind_it() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let size = open_and_settle(&ctx, &mut app, super::modal::ModalPage::Views);

    let scrim = *probe(&TEST_MODAL_SCRIM_RECTS)
        .last()
        .context("an open modal should paint a scrim")?;
    // Full coverage is what makes it modal. A scrim that merely sits
    // near the card leaves the window behind it clickable.
    assert!(
        scrim.width() >= size.x && scrim.height() >= size.y,
        "the scrim {scrim:?} does not cover the {size:?} window"
    );
    Ok(())
}

#[test]
fn the_modal_card_fits_inside_even_a_small_window() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    // The regression this pins: the settings dialog sized itself to its
    // own content and ran straight off the bottom of the screen.
    for size in [egui::vec2(640.0, 400.0), egui::vec2(1600.0, 1000.0)] {
        app.modal = None;
        render_modal_at(&ctx, &mut app, Vec::new(), size);
        app.open_modal(super::modal::ModalPage::Maintenance);
        probe(&TEST_MODAL_CARD_RECTS).clear();
        settle_modal(&ctx, &mut app, size);

        let card = *probe(&TEST_MODAL_CARD_RECTS)
            .last()
            .context("an open modal should paint a card")?;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        assert!(
            screen.contains_rect(card),
            "the card {card:?} escaped a {size:?} window"
        );
    }
    Ok(())
}

#[test]
fn clicking_a_nav_row_moves_to_that_page() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let size = open_and_settle(&ctx, &mut app, super::modal::ModalPage::Views);

    let target = probe(&TEST_MODAL_NAV_RECTS)
        .iter()
        .rev()
        .find(|(page, _)| *page == super::modal::ModalPage::About)
        .map(|(_, rect)| rect.center())
        .context("the About row should render a click target")?;

    render_modal_at(&ctx, &mut app, pointer_button(target, true), size);
    render_modal_at(&ctx, &mut app, pointer_button(target, false), size);

    assert_eq!(app.modal, Some(super::modal::ModalPage::About));
    Ok(())
}

#[test]
fn escape_unwinds_one_layer_at_a_time() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_one_file();
    app.open_modal(super::modal::ModalPage::Maintenance);
    app.pending_windows_tool = Some(4);

    // The confirmation goes first. Closing both at once would answer a
    // question the user was still being asked.
    super::modal::dismiss_top(&mut app);
    assert_eq!(app.pending_windows_tool, None);
    assert_eq!(app.modal, Some(super::modal::ModalPage::Maintenance));

    super::modal::dismiss_top(&mut app);
    assert_eq!(app.modal, None);
}

#[test]
fn the_delete_key_cannot_queue_a_second_delete_while_one_is_being_confirmed() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    app.selected_path = Some(vec![0]);
    app.request_delete_selected(false);
    assert_eq!(
        app.pending_delete.as_ref().map(|pending| pending.permanent),
        Some(false),
        "the first delete should be queued"
    );

    // Shift+Del while the confirmation is up used to reach the shortcut
    // handler and replace the queued delete with a *permanent* one,
    // underneath a card still showing the recycle-bin wording.
    let _ = ctx.run(
        raw_input(vec![egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }]),
        |ctx| super::handle_shortcuts(&mut app, ctx),
    );

    assert_eq!(
        app.pending_delete.as_ref().map(|pending| pending.permanent),
        Some(false),
        "the pending delete should not have been replaced"
    );
}

#[test]
fn picking_a_theme_repaints_the_window_in_it() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_one_file();
    let before = app.palette.panel;
    app.set_theme("light-modern");

    assert_eq!(app.theme_id, "light-modern");
    assert!(!app.palette.mode.is_dark(), "a light theme should be light");
    assert_ne!(app.palette.panel, before);

    // An id that no longer exists must not leave the app unpainted.
    app.set_theme("a-theme-that-was-removed");
    assert_eq!(
        app.palette.panel,
        themes::Palette::default().panel,
        "an unknown theme should fall back to the default palette"
    );
}

#[test]
fn every_destructive_tool_is_marked_before_it_is_clicked() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_TOOL_ROW_MARKERS).clear();
    open_and_settle(&ctx, &mut app, super::modal::ModalPage::Maintenance);

    let markers = probe(&TEST_TOOL_ROW_MARKERS);
    for (index, tool) in crate::wintools::TOOLS.iter().enumerate() {
        let drawn = markers
            .iter()
            .rev()
            .find(|(drawn_index, _, _)| *drawn_index == index)
            .context("every tool should render a row")?;
        assert_eq!(
            drawn.1, tool.destructive,
            "{} renders with the wrong severity",
            tool.name
        );
    }
    Ok(())
}

#[test]
fn a_tool_report_outlives_the_status_line_that_announced_it() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_one_file();
    app.tool_log.push(crate::gui::app::ToolOutcome {
        tool: "Analyze Component Store".to_string(),
        summary: "dism completed successfully".to_string(),
        detail: "Actual Size of Component Store : 6.21 GB".to_string(),
        failed: false,
    });
    // The status bar is transient by design; the report is the thing the
    // tool was run to produce, and it used to be discarded alongside it.
    app.status = Some("Scanning…".to_string());

    let kept = app.tool_log.last().map(|entry| entry.detail.as_str());
    assert_eq!(kept, Some("Actual Size of Component Store : 6.21 GB"));
}

#[test]
fn maintenance_rows_stay_row_sized_and_do_not_overlap() -> anyhow::Result<()> {
    /// Two lines of text, an optional chip line, and padding. A row that
    /// exceeds this is not a row any more.
    const MAX_ROW_HEIGHT: f32 = 150.0;

    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_TOOL_ROW_MARKERS).clear();
    open_and_settle(&ctx, &mut app, super::modal::ModalPage::Maintenance);

    // The bug this pins: a right-to-left region inside a top-aligned
    // horizontal claims the whole remaining height of the scroll area, so
    // the first row swallowed the page and left its button stranded at
    // the bottom of it.
    let markers = probe(&TEST_TOOL_ROW_MARKERS);
    let mut rows: Vec<_> = markers
        .iter()
        .rev()
        .take(crate::wintools::TOOLS.len())
        .map(|(index, _, rect)| (*index, *rect))
        .collect();
    assert_eq!(
        rows.len(),
        crate::wintools::TOOLS.len(),
        "every tool should render a row"
    );
    for (index, rect) in &rows {
        let name = crate::wintools::TOOLS
            .get(*index)
            .map(|tool| tool.name)
            .unwrap_or("?");
        assert!(
            rect.height() <= MAX_ROW_HEIGHT,
            "the {name} row is {:.0}px tall — it has swallowed the page",
            rect.height()
        );
        assert!(rect.height() > 0.0, "the {name} row has no height");
    }

    rows.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
    for pair in rows.windows(2) {
        let [(_, above), (_, below)] = pair else {
            continue;
        };
        assert!(
            above.bottom() <= below.top() + 0.5,
            "rows overlap: {above:?} runs into {below:?}"
        );
    }
    Ok(())
}

#[test]
fn page_boxes_share_one_left_and_right_edge_inside_the_card() -> anyhow::Result<()> {
    /// Boxes are meant to be flush, so the tolerance is for float noise,
    /// not for a design that nearly lines up.
    const EDGE_TOLERANCE: f32 = 0.5;

    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_TOOL_ROW_MARKERS).clear();
    open_and_settle(&ctx, &mut app, super::modal::ModalPage::Maintenance);

    let card = *probe(&TEST_MODAL_CARD_RECTS)
        .last()
        .context("the card should have painted")?;
    let markers = probe(&TEST_TOOL_ROW_MARKERS);
    let rows: Vec<_> = markers
        .iter()
        .rev()
        .take(crate::wintools::TOOLS.len())
        .map(|(index, _, rect)| (*index, *rect))
        .collect();
    let (_, first) = rows.first().context("every tool should render a row")?;

    // The bug this pins: each row was a `Frame` that shrank to fit its own
    // description, so every box stopped at a different x and the action
    // buttons stepped raggedly down the page.
    for (index, rect) in &rows {
        let name = crate::wintools::TOOLS
            .get(*index)
            .map(|tool| tool.name)
            .unwrap_or("?");
        assert!(
            (rect.left() - first.left()).abs() <= EDGE_TOLERANCE,
            "the {name} row starts at {:.1}, not the shared {:.1}",
            rect.left(),
            first.left()
        );
        assert!(
            (rect.right() - first.right()).abs() <= EDGE_TOLERANCE,
            "the {name} row ends at {:.1}, not the shared {:.1}",
            rect.right(),
            first.right()
        );
    }

    // And that shared edge has to sit inside the card, with air on both
    // sides rather than flush against the border.
    assert!(
        first.left() > card.left() && first.right() < card.right(),
        "the content edge {:?} is not inside the card {card:?}",
        (first.left(), first.right())
    );
    assert!(
        first.width() > (card.width() - super::modal::NAV_WIDTH) * 0.6,
        "the boxes only span {:.0}px of a {:.0}px page — they are not reaching the edge",
        first.width(),
        card.width() - super::modal::NAV_WIDTH
    );
    Ok(())
}

#[test]
fn a_long_page_scrolls_instead_of_growing_past_the_card() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    // Appearance carries the whole theme catalog, so it is the page most
    // able to overrun its own card.
    let size = egui::vec2(900.0, 620.0);
    app.open_modal(super::modal::ModalPage::Appearance);
    probe(&TEST_MODAL_CARD_RECTS).clear();
    probe(&TEST_THEME_ROW_RECTS).clear();
    settle_modal(&ctx, &mut app, size);

    let card = *probe(&TEST_MODAL_CARD_RECTS)
        .last()
        .context("the card should have painted")?;
    let rows = probe(&TEST_THEME_ROW_RECTS);
    assert!(!rows.is_empty(), "the theme picker should render rows");

    // The list is longer than the card, so some rows legitimately lay out
    // past the fold and are clipped. What must not happen is the *first*
    // one starting outside the card, which is what an unbounded scroll
    // area does once its content has pushed the viewport around.
    let (id, first) = rows.first().context("at least one theme row")?;
    assert!(
        first.top() >= card.top() && first.top() < card.bottom(),
        "the first theme row ({id}) at {first:?} is not inside the card {card:?}"
    );
    assert!(
        first.right() <= card.right(),
        "theme rows run past the right edge of the card"
    );
    Ok(())
}

// ------------------------------------------------- hover and motion

#[test]
fn a_hover_highlight_ramps_rather_than_switching() {
    // The point of the shared helper is that no control in the window
    // reaches full strength in one frame. Testing the ramp itself says
    // that once rather than once per widget: a control that forgot to
    // route through it would still look highlighted in any screenshot a
    // test could take.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let id = egui::Id::new("hover_ramp_probe");

    // One frame at rest first. The first observation of an animation
    // returns whatever it was asked for — that is what keeps a control
    // that appears already-selected from sliding into place — so a ramp
    // has to start from a state egui has actually seen.
    let _ = ctx.run(raw_input(Vec::new()), |ctx| {
        let _ = hover_t(ctx, id, false);
    });

    let mut rising = Vec::new();
    for _ in 0..14 {
        let _ = ctx.run(raw_input(Vec::new()), |ctx| {
            rising.push(hover_t(ctx, id, true))
        });
    }
    let mut falling = Vec::new();
    for _ in 0..14 {
        let _ = ctx.run(raw_input(Vec::new()), |ctx| {
            falling.push(hover_t(ctx, id, false))
        });
    }

    assert!(
        rising.first().is_some_and(|t| *t < 1.0),
        "the hover was fully lit one frame in, which is a switch, not a fade: {rising:?}"
    );
    assert_eq!(
        rising.last(),
        Some(&1.0),
        "the hover never reached full strength: {rising:?}"
    );
    assert!(
        rising.windows(2).filter(|w| w[0] < w[1]).count() >= 3,
        "the hover did not climb over several frames: {rising:?}"
    );
    // Front-loaded, not linear: most of the highlight has to arrive in
    // the first frames or the control looks like it is lagging the
    // pointer even though it is on its way.
    assert!(
        rising.first().is_some_and(|t| *t > 0.2),
        "the hover crawls out of the gate rather than answering: {rising:?}"
    );
    assert_eq!(
        falling.last(),
        Some(&0.0),
        "the hover never faded back out: {falling:?}"
    );
    assert!(
        falling.iter().any(|t| *t > 0.0 && *t < 1.0),
        "the hover snapped off instead of fading: {falling:?}"
    );
    // The ramp also has to finish inside the budget the constant
    // promises, or the highlight is still catching up with a pointer that
    // has already moved on.
    let frames_to_settle = (HOVER_SECONDS * 60.0).ceil() as usize + 2;
    assert!(
        rising.len() > frames_to_settle,
        "the ramp was measured over fewer frames than it takes to finish"
    );
}

#[test]
fn the_tree_chevron_turns_instead_of_swapping_glyphs() -> anyhow::Result<()> {
    // One chevron that rotates, not two that swap. A swap states the new
    // state and says nothing about how the rows below it just moved.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    // The root starts expanded, so settle there first. An animation asked
    // for a value it has never held returns that value immediately, and
    // measuring the first frame would be measuring nothing.
    for _ in 0..10 {
        render_directory(&ctx, &mut app, raw_input(Vec::new()));
    }
    probe(&TEST_CHEVRON_TURNS).clear();
    render_directory(&ctx, &mut app, raw_input(Vec::new()));
    let expanded = *probe(&TEST_CHEVRON_TURNS)
        .first()
        .context("the root row should draw an expand toggle")?;
    assert!(
        (expanded - 0.25).abs() < 0.001,
        "an expanded row should hold a quarter turn, not {expanded}"
    );

    app.toggle_expanded(&[]);
    probe(&TEST_CHEVRON_TURNS).clear();
    for _ in 0..14 {
        render_directory(&ctx, &mut app, raw_input(Vec::new()));
    }
    let turns = probe(&TEST_CHEVRON_TURNS);
    assert!(
        turns.iter().any(|t| *t > 0.001 && *t < 0.249),
        "the chevron cut straight from open to closed: {turns:?}"
    );
    assert_eq!(
        turns.last(),
        Some(&0.0),
        "the chevron never finished turning back: {turns:?}"
    );
    Ok(())
}

#[test]
fn the_treemap_marks_and_names_the_tile_under_the_pointer() -> anyhow::Result<()> {
    // The treemap is one painter with no widget per tile, so egui offers
    // it no hover of any kind. Until this was hit-tested by hand, the
    // only way to find out what a rectangle stood for was to click it and
    // watch the tree jump somewhere else.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    for _ in 0..3 {
        render_treemap(&ctx, &mut app, raw_input(Vec::new()));
    }

    let tile = app
        .treemap_tiles
        .iter()
        .find(|tile| tile.index_path == vec![0])
        .map(|tile| {
            egui::Rect::from_min_size(egui::pos2(tile.x, tile.y), egui::vec2(tile.w, tile.h))
        })
        .context("the one scanned file should have a tile")?;

    probe(&TEST_TREEMAP_HOVER).clear();
    render_treemap(&ctx, &mut app, raw_input(pointer_move(tile.center())));
    let (path, rect) = {
        let hovered = probe(&TEST_TREEMAP_HOVER);
        hovered
            .last()
            .context("hovering a tile should record it")?
            .clone()
    };
    assert_eq!(path, vec![0], "the deepest tile under the pointer wins");
    assert!(
        rect.contains(tile.center()),
        "the marked tile {rect:?} is not the one under the pointer"
    );

    // And nothing at all once the pointer leaves, so the outline cannot
    // be left behind on a tile the pointer has moved off.
    probe(&TEST_TREEMAP_HOVER).clear();
    render_treemap(
        &ctx,
        &mut app,
        raw_input(pointer_move(egui::pos2(-50.0, -50.0))),
    );
    assert!(
        probe(&TEST_TREEMAP_HOVER).is_empty(),
        "a tile stayed marked with the pointer off the panel"
    );
    Ok(())
}

// ------------------------------------------------------------ margins

#[test]
fn menu_bar_highlights_are_square_and_fill_the_bar() {
    // A rounded pill under a top-level menu name reads as a button
    // dropped into the strip rather than as the bar responding. The app
    // rounds every other widget by 6, so the bar has to override it — and
    // it has to do that on the child Ui inside `menu::bar`, because
    // `set_menu_style` discards anything set on the way in.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_MENU_BAR_RECTS).clear();
    probe(&TEST_MENU_BAR_ROUNDING).clear();
    let bar_top = std::cell::Cell::new(0.0_f32);
    let bar_bottom = std::cell::Cell::new(0.0_f32);
    let _ = ctx.run(raw_input_at_width(Vec::new(), 1280.0), |ctx| {
        apply_style(ctx, app.palette);
        bar_top.set(ctx.screen_rect().top());
        draw_menu_bar(&mut app, ctx);
        // Whatever the menu bar panel did not claim starts here, so this
        // is the bar's own bottom edge.
        bar_bottom.set(ctx.available_rect().top());
    });

    let roundings = probe(&TEST_MENU_BAR_ROUNDING);
    assert_eq!(roundings.len(), 7, "expected one reading per menu");
    for (label, rounding) in roundings.iter() {
        assert_eq!(
            *rounding, 0.0,
            "the {label} menu would paint a {rounding}px rounded hover background"
        );
    }

    // Square is half of it. A highlight that fills the bar is what stops
    // it reading as a floating chip, and that comes from the panel frame
    // claiming no vertical margin of its own.
    let bar_height = bar_bottom.get() - bar_top.get();
    assert!(bar_height > 0.0, "the menu bar claimed no height");
    for (label, rect) in probe(&TEST_MENU_BAR_RECTS).iter() {
        assert!(
            rect.height() >= bar_height - 2.0,
            "the {label} highlight is {:.1}px tall inside a {bar_height:.1}px bar, \
             so it floats with a gap above and below it",
            rect.height()
        );
    }
}

#[test]
fn both_kinds_of_table_header_start_their_text_in_the_same_place() -> anyhow::Result<()> {
    // Two widgets paint column names: the sortable one the directory and
    // extension tables use, and the plain one the flat lists use. The
    // plain one asked for `add_space(7.0)` and then a label — and
    // `add_space` does not consume the item spacing before the next
    // widget, so its text landed 8px right of the sortable one's. Two
    // tables on screen together had their headings in two columns.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    probe(&TEST_TABLE_HEADER_TEXT).clear();
    let _ = ctx.run(raw_input(Vec::new()), |ctx| {
        apply_style(ctx, themes::Palette::default());
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                sortable_header(ui, "Size", None);
                table_header_label(ui, "Size");
            });
        });
    });

    let insets = probe(&TEST_TABLE_HEADER_TEXT);
    assert_eq!(
        insets.len(),
        2,
        "both headers should have painted: {insets:?}"
    );
    let sortable = insets.first().context("the sortable header")?.1;
    let plain = insets.last().context("the plain header")?.1;
    assert!(
        (sortable - plain).abs() < 0.5,
        "column names start at {sortable} in a sortable header and {plain} in a plain one"
    );
    Ok(())
}

#[test]
fn a_file_row_lines_its_icon_up_with_the_folders_beside_it() -> anyhow::Result<()> {
    // A folder row leads with an expand toggle; a file row leaves a gap
    // where one would be. Getting that gap right is fiddlier than it
    // looks, because `add_space` advances the cursor without becoming an
    // item — so the widget after one is *not* given the row's item
    // spacing, and a gap of just the toggle's width leaves every file
    // 8px short of the folders it sits between.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_a_folder_beside_a_file();
    probe(&TEST_TREE_NAME_ICONS).clear();
    for _ in 0..4 {
        render_directory(&ctx, &mut app, raw_input(Vec::new()));
    }

    let icons = probe(&TEST_TREE_NAME_ICONS);
    // Siblings, so the same depth, so the same column. Depth is the only
    // thing allowed to move an icon sideways.
    let folder = icons
        .iter()
        .rev()
        .find(|(path, is_dir, _)| *is_dir && path.as_slice() == [0])
        .context("the sub-folder row should paint an icon")?;
    let file = icons
        .iter()
        .rev()
        .find(|(path, is_dir, _)| !*is_dir && path.as_slice() == [1])
        .context("the sibling file row should paint an icon")?;

    assert!(
        (folder.2.left() - file.2.left()).abs() < 0.5,
        "the folder icon starts at {:.1} and its sibling file's at {:.1}",
        folder.2.left(),
        file.2.left()
    );
    Ok(())
}

#[test]
fn every_pane_rules_off_its_heading_at_the_same_inset() -> anyhow::Result<()> {
    // The treemap pane, the extension pane, and the search view each drew
    // their own heading rule with hand-picked spacing on either side: 3
    // and 5 in one, 6 and 5 in the next. They share one helper now, and
    // what that has to guarantee is that the rules line up with each
    // other when two panes are open at once.
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_SECTION_RULE_RECTS).clear();
    apply_style(&ctx, app.palette);
    let _ = ctx.run(raw_input_at_width(Vec::new(), 1000.0), |ctx| {
        egui::TopBottomPanel::bottom("treemap_pane")
            .default_height(240.0)
            .frame(super::theme::panel_frame())
            .show(ctx, |ui| draw_treemap(&mut app, ui));
        egui::CentralPanel::default()
            .frame(super::theme::panel_frame())
            .show(ctx, |ui| draw_extension_list(&mut app, ui));
    });

    let rules = probe(&TEST_SECTION_RULE_RECTS);
    assert_eq!(rules.len(), 2, "both panes should rule off their heading");
    let (first, second) = (rules[0], rules[1]);
    assert!(
        (first.left() - second.left()).abs() < 0.5,
        "the two pane rules start at {:.1} and {:.1}, so the panels inset their \
         content by different amounts",
        first.left(),
        second.left()
    );
    assert!(
        (first.right() - second.right()).abs() < 0.5,
        "the two pane rules end at {:.1} and {:.1}",
        first.right(),
        second.right()
    );
    // And that inset is the one shared panel padding, not whatever each
    // pane happened to be handed.
    assert!(
        (first.left() - PAD).abs() < 0.5,
        "the rule starts at {:.1}, not the {PAD}px panel padding",
        first.left()
    );
    Ok(())
}

// ----------------------------------------------------------- scrollbars

/// Minimum perceptual-lightness gap, in L*, between a scrollbar handle
/// and a surface it can be drawn on top of.
///
/// Larger than the gap two adjacent *layers* need: a layer boundary is a
/// large area with an edge, while a handle is a nine-pixel strip that has
/// to be findable at a glance.
const HANDLE_SEPARATION: f32 = 6.0;

/// Every surface a scrollbar handle can end up sitting on.
///
/// All four are real: the directory table scrolls on `app`, the modal
/// page body on `panel`, the theme list inside its group box on `raised`,
/// and a bar can be dragged while the row under it is hovered.
fn scrollable_surfaces(p: &themes::Palette) -> [(&'static str, Color32); 4] {
    [
        ("app", p.app),
        ("panel", p.panel),
        ("raised", p.raised),
        ("hover", p.hover),
    ]
}

#[test]
fn a_scrollbar_handle_is_never_the_color_of_what_it_scrolls() {
    // The bug this pins: egui takes the handle color from
    // `widgets.inactive.bg_fill`, and that was set to `raised` — the
    // exact color of the card the theme list scrolls inside. The bar was
    // painted every frame and could not be seen at all.
    for spec in themes::themes() {
        let p = themes::Palette::from_spec(spec);
        let name = &spec.name;
        for (surface_name, surface) in scrollable_surfaces(&p) {
            let gap = (perceptual_lightness(p.control) - perceptual_lightness(surface)).abs();
            assert!(
                gap >= HANDLE_SEPARATION,
                "{name}: the scrollbar handle is {gap:.1} L* from {surface_name} — \
                 it disappears against it"
            );
            assert_ne!(
                p.control, surface,
                "{name}: the scrollbar handle is exactly {surface_name}"
            );
        }
        // Hover has to be a visible change from rest, or the bar gives no
        // feedback that it is the thing under the pointer.
        let hover_step =
            (perceptual_lightness(p.control_hover) - perceptual_lightness(p.control)).abs();
        assert!(
            hover_step >= 4.0,
            "{name}: the handle only changes {hover_step:.1} L* on hover"
        );
    }
}

#[test]
fn the_style_points_scrollbars_at_the_control_color_not_a_surface() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Checked through `apply_style` rather than against the palette alone,
    // because the defect was in the wiring: the palette had a usable color
    // all along and the style handed egui the surface instead.
    for spec in themes::themes() {
        let palette = themes::Palette::from_spec(spec);
        let ctx = egui::Context::default();
        apply_style(&ctx, palette);
        let visuals = ctx.style().visuals.clone();
        let name = &spec.name;

        assert_eq!(
            visuals.widgets.inactive.bg_fill, palette.control,
            "{name}: the resting handle does not come from the control color"
        );
        assert_eq!(
            visuals.widgets.hovered.bg_fill, palette.control_hover,
            "{name}: the hovered handle does not come from the control color"
        );
        assert_ne!(
            visuals.widgets.inactive.bg_fill, visuals.panel_fill,
            "{name}: the handle is the panel it sits on"
        );
        assert_ne!(
            visuals.widgets.inactive.bg_fill, palette.raised,
            "{name}: the handle is the card it sits on"
        );
        // Buttons keep the surface color. `bg_fill` and `weak_bg_fill`
        // being the same value is what caused this, so the two staying
        // apart is the invariant, not an incidental detail.
        assert_eq!(
            visuals.widgets.inactive.weak_bg_fill, palette.raised,
            "{name}: separating the handle color has changed how buttons look"
        );
    }
}

#[test]
fn every_scroll_area_shares_one_style_and_takes_its_own_space() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    apply_style(&ctx, themes::Palette::default());
    let scroll = ctx.style().spacing.scroll;
    let expected = scroll_style();

    // Solid, not floating. A floating bar is invisible until the pointer
    // is already over it and overlays the content when it appears, so a
    // table with columns past its edge looks like it has simply lost them.
    assert!(!scroll.floating, "scrollbars must not be floating");
    assert!(
        scroll.allocated_width() > 0.0,
        "a solid scrollbar has to reserve its own space, or it covers the content"
    );
    assert_eq!(scroll.bar_width, expected.bar_width);
    assert_eq!(scroll.handle_min_length, expected.handle_min_length);
    // A handle shorter than this is unusable with a pointer, and a long
    // list drives it toward zero.
    assert!(
        scroll.handle_min_length >= 24.0,
        "a {}px minimum handle is too small to grab",
        scroll.handle_min_length
    );

    // The style is the only place any of the five scroll areas is
    // configured, so switching theme must not disturb it.
    apply_style(&ctx, themes::palette_for("light-modern"));
    let after = ctx.style().spacing.scroll;
    assert_eq!(after.bar_width, expected.bar_width);
    assert_eq!(after.floating, expected.floating);
    assert_eq!(after.handle_min_length, expected.handle_min_length);
}

#[test]
fn the_modal_reserves_room_for_its_scrollbar() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    probe(&TEST_TOOL_ROW_MARKERS).clear();
    open_and_settle(&ctx, &mut app, super::modal::ModalPage::Maintenance);

    let card = *probe(&TEST_MODAL_CARD_RECTS)
        .last()
        .context("the card should have painted")?;
    let markers = probe(&TEST_TOOL_ROW_MARKERS);
    let (_, _, row) = markers.last().context("a tool row should have painted")?;

    // Content has to stop short of the card edge by at least the bar's
    // own width, or the bar is painted over the boxes it scrolls.
    let bar = apply_style_and_bar_width(&ctx, app.palette);
    let right_gap = card.right() - row.right();
    assert!(
        right_gap >= bar,
        "content ends {right_gap:.1}px from the card edge but the scrollbar needs {bar:.1}px"
    );
    Ok(())
}

fn apply_style_and_bar_width(ctx: &egui::Context, palette: themes::Palette) -> f32 {
    apply_style(ctx, palette);
    ctx.style().spacing.scroll.allocated_width()
}
