// ============================================================================
// Module:       gui::ui::tests
// Description:  Interaction tests that drive the real drawing code through an
//               egui context and assert against the geometry it recorded.
//
// Dependencies: eframe::egui, anyhow; super::probes, crate::model::{Node,
//               Tree}
// ============================================================================

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
use crate::model::SortMode;
use crate::model::{Node, Tree};
use anyhow::Context;
use eframe::egui::{self, Color32};
use std::path::PathBuf;

static TEST_UI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Which `ext_totals` slot a file of this name is counted under.
///
/// Derived from the name the same way `file` below derives the node's
/// category, so the two cannot disagree — and so the fixtures do not have
/// to unwrap an `Option` they themselves just filled in.
fn category_index(name: &std::ffi::OsStr) -> usize {
    crate::model::category_for_name(name).index()
}

fn file(name: &str, size: u64) -> Node {
    let category = crate::model::category_for_name(std::ffi::OsStr::new(name));
    Node {
        name: std::ffi::OsString::from(name),
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
        file_id: None,
        other_filesystem: false,
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
            name: std::ffi::OsString::from("test-root"),
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
            file_id: None,
            other_filesystem: false,
        },
        volume_free: None,
        volume_total: None,
        roots: Vec::new(),
        hard_link_bytes: None,
    })
}

/// A root holding one folder and one file, so the two kinds of tree row
/// appear at the same depth and their layouts can be compared without a
/// depth indent standing between them.
fn app_with_a_folder_beside_a_file() -> GuiApp {
    let leaf = file("inside.bin", 64);
    let folder = Node {
        name: std::ffi::OsString::from("sub"),
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
        file_id: None,
        other_filesystem: false,
    };
    let sibling = file("beside.txt", 128);
    let mut totals = vec![(0, 0, 0); Category::COUNT];
    totals[category_index(std::ffi::OsStr::new("inside.bin"))] = (64, 64, 1);
    totals[category_index(std::ffi::OsStr::new("beside.txt"))] = (128, 128, 1);
    GuiApp::new(Tree {
        root_path: PathBuf::from("C:\\test-root"),
        root: Node {
            name: std::ffi::OsString::from("test-root"),
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
            file_id: None,
            other_filesystem: false,
        },
        volume_free: None,
        volume_total: None,
        roots: Vec::new(),
        hard_link_bytes: None,
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
            name: std::ffi::OsString::from("sortable-root"),
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
            file_id: None,
            other_filesystem: false,
        },
        volume_free: None,
        volume_total: None,
        roots: Vec::new(),
        hard_link_bytes: None,
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

/// Throws away a finished frame.
///
/// epaint 0.36 panics when a [`egui::FullOutput`] is dropped with texture
/// deltas still attached: a real renderer uploads them, and silently
/// discarding one is how a font atlas goes missing. These tests have no
/// renderer, so they say so explicitly rather than leaking the panic out
/// of whichever test happened to allocate a glyph first.
fn discard(mut output: egui::FullOutput) {
    output.textures_delta.clear();
}

fn raw_input_at_width(events: Vec<egui::Event>, width: f32) -> egui::RawInput {
    raw_input_sized(events, width, 500.0)
}

/// A frame at an explicit window size.
///
/// The 500px-tall default is fine for a test that renders one pane, but
/// the whole window does not fit in it: menu bar, toolbar, treemap,
/// extensions pane and status bar leave the file list with no rows on
/// screen at all, so anything that wants to click a row while the rest of
/// the window is drawn needs a realistic height.
fn raw_input_sized(events: Vec<egui::Event>, width: f32, height: f32) -> egui::RawInput {
    static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let frame = FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, height),
        )),
        events,
        time: Some(frame as f64 / 60.0),
        ..Default::default()
    }
}

/// A whole-window frame, big enough for every pane to have room.
fn window_input(events: Vec<egui::Event>) -> egui::RawInput {
    raw_input_sized(events, 1400.0, 900.0)
}

fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
    raw_input_at_width(events, 900.0)
}

fn render_directory(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| draw_directory_tree(app, ui));
    }));
}

fn render_extensions(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| draw_extension_list(app, ui));
    }));
}

fn render_largest(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| draw_largest_files(app, ui));
    }));
}

fn render_search(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| draw_search(app, ui));
    }));
}

fn render_duplicates(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| draw_duplicates(app, ui));
    }));
}

fn render_toolbar(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        draw_toolbar(app, ui);
    }));
}

/// Draws the treemap into a panel framed exactly as the real one is, so
/// the tiles land where they land in the window rather than at whatever
/// coordinates a bare `CentralPanel` happens to hand out.
fn render_treemap(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default()
            .frame(super::theme::panel_frame())
            .show(ui, |ui| draw_treemap(app, ui));
    }));
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
        let interaction = ctx.global_style().interaction.clone();
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
        discard(ctx.run_ui(raw_input_at_width(Vec::new(), 1400.0), |ui| {
            apply_style(ui.ctx(), app.palette);
            draw_toolbar(&mut app, ui);
        }));
    }
    assert!(
        !tooltip_is_showing(&ctx),
        "a tooltip is showing before the pointer has gone anywhere near a button"
    );

    // The first toolbar button sits just right of the app mark.
    let over_button = egui::pos2(150.0, 40.0);
    discard(ctx.run_ui(
        raw_input_at_width(pointer_move(over_button), 1400.0),
        |ui| {
            apply_style(ui.ctx(), app.palette);
            draw_toolbar(&mut app, ui);
        },
    ));
    // One further frame with no events at all: the pointer is now still,
    // which is precisely the state the old defaults never got a frame in.
    discard(ctx.run_ui(raw_input_at_width(Vec::new(), 1400.0), |ui| {
        apply_style(ui.ctx(), app.palette);
        draw_toolbar(&mut app, ui);
    }));

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
    assert!(!ctx.global_style().interaction.selectable_labels);
    assert!(!ctx.global_style().interaction.multi_widget_text_select);
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
        render_toolbar(&ctx, &mut app, raw_input_at_width(Vec::new(), 1600.0));
    }
    let search = probe(&TEST_VIEW_TAB_RECTS)
        .iter()
        .rev()
        .find(|(view, _)| *view == FileView::SearchResults)
        .map(|(_, rect)| rect.center())
        .context("the Search Results tab should render a click target")?;

    render_toolbar(
        &ctx,
        &mut app,
        raw_input_at_width(pointer_button(search, true), 1600.0),
    );
    render_toolbar(
        &ctx,
        &mut app,
        raw_input_at_width(pointer_button(search, false), 1600.0),
    );

    assert_eq!(app.file_view, FileView::SearchResults);
    Ok(())
}

#[test]
fn menu_icons_never_overlap_their_labels() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    probe(&TEST_ICON_MENU_LAYOUTS).clear();
    discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            icon_selectable_label(ui, true, Icon::Tree, "All files");
            icon_button(ui, true, Icon::Settings, "     Settings…");
            icon_button(ui, false, Icon::Duplicate, "     Duplicate Files");
        });
    }));

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
    discard(ctx.run_ui(raw_input_at_width(Vec::new(), 1280.0), |ui| {
        apply_style(ui.ctx(), app.palette);
        draw_menu_bar(&mut app, ui);
    }));

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
        ctx.fonts_mut(|fonts| {
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
    discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            menu_action(ui, true, Icon::FolderOpen, "Select folder…", "Ctrl+O");
            menu_action(ui, true, Icon::Refresh, "Rescan", "F5");
            menu_action(ui, true, Icon::Trash, "Delete to Recycle Bin", "Del");
            icon_button(ui, true, Icon::Export, "Export CSV…");
            menu_toggle(ui, &mut true, "Grid lines");
            menu_choice(ui, true, "Logical size");
        });
    }));

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

/// Renders the extensions pane in a resizable side panel and reports the
/// width the panel actually took.
fn extension_panel_width(
    ctx: &egui::Context,
    app: &mut GuiApp,
    screen: f32,
    events: Vec<egui::Event>,
) -> (f32, f32) {
    let mut rect = egui::Rect::ZERO;
    discard(ctx.run_ui(raw_input_at_width(events, screen), |ui| {
        let response = egui::Panel::right("test_extension_panel")
            .resizable(true)
            .min_size(0.0)
            .default_size(1200.0)
            .frame(panel_frame())
            .show(ui, |ui| draw_extension_list(app, ui));
        rect = response.response.rect;
    }));
    (rect.width(), rect.left())
}

/// The extensions pane takes the width it is given, and can be dragged
/// narrower.
///
/// A side panel stores *its content's* width as its own, so content that
/// overflows ratchets the panel wider and the divider will not come back.
/// Two things did that: the category chips are `egui::Frame`s, and a
/// frame measures itself against the space left on the line and then
/// allocates that, so in a wrapped row it overflows rather than wrapping
/// — nine chips pinned the pane at nearly the whole window. The
/// extension columns were `Column::auto()`, which makes the table's first
/// frame a sizing pass laid out unbounded, ratcheting it open on frame
/// one.
///
/// A realistic fixture on purpose: with three extensions and two
/// categories everything fits and nothing is pinned, which is why the
/// version of this test that used one passed throughout.
#[test]
fn the_extensions_pane_takes_the_width_it_is_given() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_many_extensions();
    apply_style(&ctx, app.palette);

    const SCREEN: f32 = 1900.0;
    const ASKED: f32 = 1200.0;

    let mut width = 0.0;
    let mut left = 0.0;
    for _ in 0..4 {
        (width, left) = extension_panel_width(&ctx, &mut app, SCREEN, Vec::new());
    }
    assert!(
        width <= ASKED + 1.0,
        "the pane settled at {width:.0}px when asked for {ASKED:.0}px, so its contents are \
         setting a floor the divider cannot be dragged past"
    );

    // Now drag the divider right, which narrows a right-hand panel.
    let grab = egui::pos2(left, 400.0);
    let target = egui::pos2(SCREEN - 300.0, 400.0);
    let _ = extension_panel_width(&ctx, &mut app, SCREEN, pointer_button(grab, true));
    for step in 1..=6 {
        let to = egui::pos2(grab.x + (target.x - grab.x) * step as f32 / 6.0, grab.y);
        let _ = extension_panel_width(&ctx, &mut app, SCREEN, pointer_move(to));
    }
    (width, _) = extension_panel_width(&ctx, &mut app, SCREEN, pointer_button(target, false));

    assert!(
        width <= 320.0,
        "after dragging the divider to leave 300px, the pane is still {width:.0}px wide"
    );
}

/// How far left of the next column's leading edge to grab.
///
/// `egui_extras` puts a column's resize line at the *next* column's left
/// edge and gives it `interaction.resize_grab_radius_side` of slack. One
/// pixel inside that is a reliable hit, and — unlike aiming at the line
/// itself — it cannot land on the next header, whose own click sense
/// covers its whole cell and would swallow the press.
///
/// Until egui 0.36 the handle sat one `item_spacing.x` right of the
/// *previous* cell instead, which is what this constant used to encode;
/// the gap between two cells is now two spacings wide, so the midpoint is
/// no longer the boundary.
const COLUMN_GRAB_INSET: f32 = 1.0;

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
    // The grab strip is on the *next* column's leading edge, so the
    // target is derived from the pair of cells rather than from a
    // spacing constant: the probe records every header in draw order, so
    // the entry after the last copy of `label` is the column to its
    // right.
    let rendered: Vec<(&'static str, egui::Rect)> = probe(headers).iter().copied().collect();
    let this = rendered
        .iter()
        .rposition(|(seen, _)| *seen == label)
        .map(|index| (index, rendered[index].1));
    assert!(
        this.is_some(),
        "{label} did not render, so it has no border"
    );
    let Some((index, cell)) = this else {
        return;
    };
    let next = rendered
        .iter()
        .skip(index + 1)
        .map(|(_, rect)| *rect)
        .find(|rect| rect.left() > cell.right());
    assert!(
        next.is_some(),
        "{label} is the last column rendered, so nothing borders it on the right"
    );
    let Some(next) = next else {
        return;
    };
    let edge = egui::pos2(next.left() - COLUMN_GRAB_INSET, cell.center().y);

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
        // The table reserves a gutter for its own vertical scrollbar, so
        // the columns legitimately stop short of the viewport by that
        // much. That strip is a scrollbar, not dead space.
        let gutter = ctx.global_style().spacing.scroll.allocated_width();
        assert!(
            row + gutter + 1.0 >= viewport,
            "in a {width:.0}px pane the table is {row:.0}px wide inside a {viewport:.0}px \
             viewport, leaving {:.0}px beside it that its scrollbar gutter ({gutter:.0}px)              does not account for",
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
        ctx.viewport_rect().width()
    );
    Ok(())
}

#[test]
fn clicking_a_rendered_extension_row_changes_highlight() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    // The extension rows are computed on a worker since the zoom-time
    // recomputation moved off the frame thread; wait for them through
    // the same poll path the frame update uses.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.extensions_pending() && std::time::Instant::now() < deadline {
        app.poll_background(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        !app.extensions_pending(),
        "the extension worker should finish"
    );
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
    app.search.query = "*".to_string();
    app.run_search();
    // The search runs on a worker now; wait for it rather than reading
    // results before they exist.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.search_running() && std::time::Instant::now() < deadline {
        app.poll_background(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!app.search_running(), "the search should finish");
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
        distinct_inodes: 1,
        files: vec![crate::duplicates::DupFile {
            index_path: vec![0],
            file_id: None,
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
    discard(ctx.run_ui(input, |ui| super::modal::draw_modal(app, ui.ctx())));
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
    app.tools.pending = Some(4);

    // The confirmation goes first. Closing both at once would answer a
    // question the user was still being asked.
    super::modal::dismiss_top(&mut app);
    assert_eq!(app.tools.pending, None);
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
    discard(ctx.run_ui(
        raw_input(vec![egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }]),
        |ctx| super::handle_shortcuts(&mut app, ctx),
    ));

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
    app.tools.log.push(crate::gui::app::ToolOutcome {
        tool: "Analyze Component Store".to_string(),
        summary: "dism completed successfully".to_string(),
        detail: "Actual Size of Component Store : 6.21 GB".to_string(),
        failed: false,
    });
    // The status bar is transient by design; the report is the thing the
    // tool was run to produce, and it used to be discarded alongside it.
    app.status = Some("Scanning…".to_string());

    let kept = app.tools.log.last().map(|entry| entry.detail.as_str());
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
    discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
        let _ = hover_t(ui.ctx(), id, false);
    }));

    let mut rising = Vec::new();
    for _ in 0..14 {
        discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
            rising.push(hover_t(ui.ctx(), id, true))
        }));
    }
    let mut falling = Vec::new();
    for _ in 0..14 {
        discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
            falling.push(hover_t(ui.ctx(), id, false))
        }));
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
    discard(ctx.run_ui(raw_input_at_width(Vec::new(), 1280.0), |ui| {
        apply_style(ui.ctx(), app.palette);
        bar_top.set(ui.ctx().viewport_rect().top());
        draw_menu_bar(&mut app, ui);
        // Whatever the menu bar panel did not claim starts here, so this
        // is the bar's own bottom edge.
        bar_bottom.set(ui.available_rect_before_wrap().top());
    }));

    let roundings = probe(&TEST_MENU_BAR_ROUNDING);
    assert_eq!(roundings.len(), 7, "expected one reading per menu");
    for (label, rounding) in roundings.iter() {
        assert_eq!(
            *rounding, 0,
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
    discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
        apply_style(ui.ctx(), themes::Palette::default());
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical(|ui| {
                sortable_header(ui, "Size", None, true);
                table_header_label(ui, "Size");
            });
        });
    }));

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
    discard(ctx.run_ui(raw_input_at_width(Vec::new(), 1000.0), |ui| {
        egui::Panel::bottom("treemap_pane")
            .default_size(240.0)
            .frame(super::theme::panel_frame())
            .show(ui, |ui| draw_treemap(&mut app, ui));
        egui::CentralPanel::default()
            .frame(super::theme::panel_frame())
            .show(ui, |ui| draw_extension_list(&mut app, ui));
    }));

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
        let visuals = ctx.global_style().visuals.clone();
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
    let scroll = ctx.global_style().spacing.scroll;
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
    let after = ctx.global_style().spacing.scroll;
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
    ctx.global_style().spacing.scroll.allocated_width()
}

/// Truncation fits the width it is given, and keeps as much as fits.
///
/// The "keeps as much as fits" half is the one that matters: it is what
/// an off-by-one at the cut point breaks, and the reason this is not
/// just an "is it short enough" check. The implementation reads glyph
/// positions out of a single galley rather than laying the string out
/// again for every character it removes, which it used to do once per
/// visible treemap tile per frame.
#[test]
fn a_truncated_label_fits_its_width_and_keeps_everything_that_does() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    apply_style(&ctx, themes::Palette::default());
    discard(ctx.run_ui(raw_input(Vec::new()), |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let painter = ui.painter().clone();
            let font = egui::TextStyle::Small.resolve(ui.style());
            let width_of = |text: &str| {
                painter
                    .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
                    .rect
                    .width()
            };

            let name = "a-rather-long-file-name-that-will-not-fit.tar.gz";
            let full = width_of(name);

            // Wide enough for the whole thing: returned untouched, with
            // no ellipsis bolted on.
            assert_eq!(
                truncate_for_width(name, full + 10.0, &painter, ui),
                name,
                "a name that fits should come back unchanged"
            );

            for max_w in [full * 0.75, full * 0.5, full * 0.25, 12.0, 1.0] {
                let cut = truncate_for_width(name, max_w, &painter, ui);
                if cut.is_empty() {
                    continue;
                }
                assert!(
                    cut.ends_with('…'),
                    "{cut:?} was shortened but does not say so"
                );
                assert!(
                    width_of(&cut) <= max_w,
                    "{cut:?} lays out at {}px, past the {max_w}px it was given",
                    width_of(&cut)
                );

                let kept: String = cut.chars().take(cut.chars().count() - 1).collect();
                assert!(
                    name.starts_with(&kept),
                    "{cut:?} is not a prefix of the original name"
                );

                // One more character would not have fit. Without this,
                // returning just "…" passes every assertion above.
                let next = name.chars().nth(kept.chars().count());
                if let Some(next) = next {
                    let longer = format!("{kept}{next}…");
                    assert!(
                        width_of(&longer) > max_w,
                        "{cut:?} stopped early — {longer:?} also fits in {max_w}px"
                    );
                }
            }
        });
    }));
}

/// A pane that was once wider does not keep a scrollbar it no longer
/// needs.
///
/// Widening the window and narrowing it again used to leave the table at
/// its widest, with a horizontal scrollbar over space it had given back.
/// The cause was ours, not `egui_extras`: a column heading allocated its
/// whole cell, so the table recorded that width as the column's content
/// width — and for the `remainder()` column that becomes a floor it can
/// never shrink below. Headings now paint and sense across the full cell
/// while allocating only what their text needs.
///
/// One `Context` across all three sizes on purpose: the bug only exists
/// as a memory of having once been wider.
#[test]
fn a_table_gives_back_width_when_its_pane_shrinks() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    let mut render_at = |width: f32| {
        probe(&TEST_DIRECTORY_SCROLL).clear();
        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), width));
        }
        probe(&TEST_DIRECTORY_SCROLL)
            .last()
            .copied()
            .unwrap_or_default()
    };

    let (content, viewport) = render_at(1400.0);
    assert!(
        content <= viewport + 1.0,
        "a fresh 1400px pane should not scroll: {content:.0}px of content in {viewport:.0}px"
    );

    render_at(1500.0);

    let (content, viewport) = render_at(1400.0);
    assert!(
        content <= viewport + 1.0,
        "after being 1500px wide, a 1400px pane still reports {content:.0}px of content \
         in {viewport:.0}px of viewport — the table kept width it no longer has"
    );
}

/// A tree with a realistic spread of extensions and categories, so the
/// extension pane has as much in it as it does after a real scan.
fn app_with_many_extensions() -> GuiApp {
    let names = [
        "a.rlib", "b.rmeta", "c.bin", "d.pdb", "e.o", "f.exe", "g.dll", "h.d", "i", "j.html",
        "k.json", "l.woff2", "m.rs", "n.lib", "o.png", "p.jpg", "q.txt", "r.md", "s.toml",
        "t.lock", "u.zip", "v.mp4", "w.wav", "x.csv", "y.pdf", "z.docx", "aa.xlsx", "bb.iso",
        "cc.tar", "dd.gz", "ee.7z", "ff.svg", "gg.ico",
    ];
    let mut totals = vec![(0, 0, 0); Category::COUNT];
    let mut children = Vec::new();
    for (n, name) in names.iter().enumerate() {
        let size = ((n as u64) + 1) * 1_000_000;
        let index = category_index(std::ffi::OsStr::new(name));
        totals[index].0 += size;
        totals[index].1 += size;
        totals[index].2 += 1;
        children.push(file(name, size));
    }
    let total: u64 = children.iter().map(|c| c.size).sum();
    GuiApp::new(Tree {
        root_path: PathBuf::from("C:\\test-root"),
        volume_free: None,
        volume_total: None,
        root: Node {
            name: std::ffi::OsString::from("test-root"),
            is_dir: true,
            is_symlink: false,
            size: total,
            physical_size: total,
            file_count: names.len() as u64,
            dir_count: 0,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: totals,
            unreadable_count: 0,
            file_id: None,
            other_filesystem: false,
        },
        roots: Vec::new(),
        hard_link_bytes: None,
    })
}

/// Every column stays on screen however narrow the pane gets.
///
/// A narrow pane used to drop all but Name, Size and % of total. That is
/// the wrong trade for this table: the columns are the reason to look at
/// it, and one that vanishes cannot be scrolled to, resized, or even
/// known to exist — it simply looks like the app lost them. Narrowing
/// now scrolls instead, which is what the horizontal scroll area is for.
#[test]
fn no_column_disappears_when_the_pane_narrows() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    let expected: Vec<&'static str> = app
        .directory_column_order
        .iter()
        .map(|column| directory_column_label(*column))
        .collect();

    // Well below the 760px that used to trigger the compact set, and on
    // down to a pane narrower than a single column.
    for width in [1400.0, 900.0, 700.0, 500.0, 300.0, 120.0] {
        probe(&TEST_DIRECTORY_HEADER_RECTS).clear();
        probe(&TEST_DIRECTORY_SCROLL).clear();
        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), width));
        }

        let headers = probe(&TEST_DIRECTORY_HEADER_RECTS);
        let drawn: Vec<&str> = headers
            .iter()
            .rev()
            .take(expected.len())
            .rev()
            .map(|(label, _)| *label)
            .collect();
        assert_eq!(
            drawn, expected,
            "at {width:.0}px the table drew {drawn:?} instead of every column"
        );

        // And the ones past the edge are reachable rather than clipped.
        let (content, viewport) = probe(&TEST_DIRECTORY_SCROLL)
            .last()
            .copied()
            .unwrap_or_default();
        if content > viewport + 1.0 {
            continue;
        }
        assert!(
            width > 700.0,
            "at {width:.0}px the columns cannot all fit in {viewport:.0}px, so the table \
             should report more content than viewport and give a scrollbar — it reported \
             {content:.0}px"
        );
    }
}

use super::actions::handle_shortcuts;
use super::modal::modal_is_open;
use super::modal::ModalPage;

/// Feeds one key press through the shortcut handler.
fn press(ctx: &egui::Context, app: &mut GuiApp, key: egui::Key, modifiers: egui::Modifiers) {
    let mut input = raw_input(Vec::new());
    // The modifier state `handle_shortcuts` reads is
    // `InputState::modifiers`, and egui only updates that from an
    // explicit `ModifiersChanged` event — not from the modifiers
    // attached to a key event. Without this line every Ctrl+key pressed
    // here arrived as a bare key, and nothing noticed: the one test that
    // pressed Ctrl+F was asserting a modal *swallowed* it, which is true
    // either way.
    input.events.push(egui::Event::ModifiersChanged(modifiers));
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    discard(ctx.run_ui(input, |ui| handle_shortcuts(app, ui.ctx())));
}

/// A modal is modal for the keyboard too.
///
/// Without the guard, pressing Del while an "are you sure" was on screen
/// queued a *second* delete behind the one being confirmed, and F5 could
/// swap the tree out from under a pending deletion's index path — both
/// of which act on an index path that no longer means what it did.
///
/// This is the GUI's counterpart to the TUI rule that a destructive
/// confirmation answers only to the keys it offers.
#[test]
fn shortcuts_do_nothing_while_a_modal_is_open() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    app.select_path(vec![0]);

    // Queue a delete, which is what puts the confirmation up.
    app.request_delete_selected(false);
    assert!(
        app.pending_delete.is_some(),
        "the fixture should have something queued to confirm"
    );
    app.open_modal(ModalPage::Maintenance);
    assert!(
        modal_is_open(&app),
        "the confirmation should count as an open modal"
    );

    let before_view = app.file_view;
    let before_sort = app.sort;

    // Every destructive or state-changing shortcut the menus advertise,
    // minus Ctrl+O — that one opens a native folder picker, which would
    // block a test rather than fail it.
    let plain = egui::Modifiers::NONE;
    let ctrl = egui::Modifiers::CTRL;
    for (key, modifiers) in [
        (egui::Key::F5, plain),
        (egui::Key::Delete, plain),
        (egui::Key::Delete, egui::Modifiers::SHIFT),
        (egui::Key::F, ctrl),
        (egui::Key::C, ctrl),
        (egui::Key::Home, plain),
        (egui::Key::Plus, plain),
        (egui::Key::Minus, plain),
    ] {
        press(&ctx, &mut app, key, modifiers);
    }

    assert!(
        app.pending_delete.is_some(),
        "the queued delete should still be waiting on its confirmation, not replaced or \
         cancelled by a keystroke aimed at the dialog"
    );
    assert!(
        modal_is_open(&app),
        "no shortcut should have closed the modal out from under the user"
    );
    assert_eq!(
        app.file_view, before_view,
        "a shortcut changed the view while a modal was up"
    );
    assert!(
        app.sort == before_sort,
        "a shortcut changed the sort while a modal was up"
    );
    assert!(
        app.scan_progress.is_none(),
        "F5 started a rescan while a deletion was waiting to be confirmed — the tree it \
         is queued against would be swapped out from under it"
    );
}

/// Escape is the exception: it dismisses the modal, and only that.
#[test]
fn escape_closes_the_modal_it_is_aimed_at() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();

    app.open_modal(ModalPage::Maintenance);
    assert!(modal_is_open(&app));

    press(&ctx, &mut app, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!modal_is_open(&app), "Escape should dismiss the open modal");
}

/// The extension table keeps every column and scrolls to reach them.
///
/// Same rule as the file list: a column that vanishes on a narrow pane
/// cannot be scrolled to, resized, or even known to exist. This is the
/// half that a screenshot cannot answer — whether the columns past the
/// pane edge are *reachable* or merely clipped, which is the difference
/// between a narrow pane and a broken one.
#[test]
fn the_extension_table_scrolls_rather_than_dropping_columns() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_many_extensions();
    apply_style(&ctx, app.palette);

    let expected: Vec<&'static str> = app
        .extension_column_order
        .iter()
        .map(|column| extension_column_label(*column))
        .collect();

    // 340px is roughly the pane in the treemap-below layout, where the
    // table genuinely does not fit.
    for width in [900.0, 520.0, 340.0, 200.0] {
        probe(&TEST_EXTENSION_HEADER_RECTS).clear();
        probe(&TEST_EXTENSION_SCROLL).clear();
        for _ in 0..4 {
            render_extensions(&ctx, &mut app, raw_input_at_width(Vec::new(), width));
        }

        let headers = probe(&TEST_EXTENSION_HEADER_RECTS);
        let drawn: Vec<&str> = headers
            .iter()
            .rev()
            .take(expected.len())
            .rev()
            .map(|(label, _)| *label)
            .collect();
        assert_eq!(
            drawn, expected,
            "at {width:.0}px the table drew {drawn:?} instead of every column"
        );

        let (content, viewport) = probe(&TEST_EXTENSION_SCROLL)
            .last()
            .copied()
            .unwrap_or_default();
        assert!(
            content >= viewport - 1.0,
            "at {width:.0}px the table reports {content:.0}px of content in a \
             {viewport:.0}px viewport, leaving dead space beside it"
        );
        if width <= 520.0 {
            assert!(
                content > viewport + 1.0,
                "at {width:.0}px the columns cannot all fit, so the table should report \
                 more content than viewport and give a scrollbar to reach them — it \
                 reported {content:.0}px in {viewport:.0}px"
            );
        }
    }
}

/// No module in `gui::ui` may declare a `const` that shadows one of the
/// theme's.
///
/// Every module here glob-imports `theme`, and a glob import is the
/// weakest binding there is: any `const` of the same name declared in the
/// importing module wins silently, with no warning and no error. A
/// function-local one wins for the length of that function *only*, so the
/// same identifier can mean two different numbers a few lines apart.
/// `nav_row` had exactly that — `PAD` was 10.0 inside it and 12.0
/// everywhere else in `modal.rs`.
///
/// The names are read back out of `theme.rs` rather than listed here, so
/// adding a constant to the scale extends this check for free.
#[test]
fn no_local_const_shadows_the_theme_scale() -> anyhow::Result<()> {
    let ui_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("gui")
        .join("ui");

    let theme_source = std::fs::read_to_string(ui_dir.join("theme.rs"))
        .context("reading theme.rs to collect the names it exports")?;
    let exported: Vec<&str> = theme_source
        .lines()
        .filter_map(|line| line.strip_prefix("pub(super) const "))
        .filter_map(|rest| rest.split(':').next())
        .map(str::trim)
        .collect();

    assert!(
        exported.len() >= 4,
        "expected theme.rs to export the spacing scale, found {exported:?} — has the declaration style changed?"
    );

    let mut shadowed = Vec::new();
    for entry in std::fs::read_dir(&ui_dir).context("listing src/gui/ui")? {
        let path = entry.context("reading an entry of src/gui/ui")?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        // theme.rs is where these are declared, not a place they leak into.
        if file == "theme.rs" {
            continue;
        }

        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Checked at every indentation on purpose: a module-level
            // `const` shadows the glob for the whole file, which is worse
            // than a function-local one, not better.
            let bare = trimmed
                .strip_prefix("pub(crate) ")
                .or_else(|| trimmed.strip_prefix("pub(super) "))
                .or_else(|| trimmed.strip_prefix("pub "))
                .unwrap_or(trimmed);
            let Some(rest) = bare.strip_prefix("const ") else {
                continue;
            };
            let declared = rest.split(':').next().unwrap_or_default().trim();
            if exported.contains(&declared) {
                shadowed.push(format!("{file}:{} declares `const {declared}`", index + 1));
            }
        }
    }

    assert!(
        shadowed.is_empty(),
        "these shadow a theme constant of the same name, so the identifier means one thing there and another everywhere else: {shadowed:?}. Rename the local one after what it actually measures."
    );
    Ok(())
}

/// No literal spacing anywhere in `src/gui`.
///
/// The scale only buys a shared column if everything is on it, and the
/// modal spent a long time not being: about forty hand-picked values,
/// `add_space(6.0)` through `add_space(28.0)`, plus off-scale `Margin`s
/// and a `SPACE_XS + 1.0`. They are all snapped now, and this is what
/// stops the next one from being added.
///
/// `0.0` is allowed: an absent margin is not a step on the scale, and
/// rounding it up to `SPACE_XS` would put a gap where the design wants
/// none.
#[test]
fn no_literal_spacing_survives_in_the_gui() -> anyhow::Result<()> {
    let add_space = regex::Regex::new(r"ui\.add_space\(\s*\d+\.\d+")?;
    let margin = regex::Regex::new(r"Margin::(?:same|symmetric)\(([^)]*)\)")?;
    let number = regex::Regex::new(r"\d+\.\d+")?;

    let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("gui")];
    let mut offenders = Vec::new();

    // Iterative for the same reason everything else here is, even though
    // a source tree is nothing like tree-sized.
    while let Some(dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("listing {}", dir.display()))?
        {
            let path = entry.context("reading a directory entry")?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let file = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            for (index, line) in text.lines().enumerate() {
                let trimmed = line.trim_start();
                // Doc comments quote these calls when explaining them.
                if trimmed.starts_with("//") {
                    continue;
                }
                let at = index + 1;
                if add_space.is_match(trimmed) {
                    offenders.push(format!("{file}:{at} add_space with a literal"));
                }
                for caught in margin.captures_iter(trimmed) {
                    let Some(args) = caught.get(1) else {
                        continue;
                    };
                    for found in number.find_iter(args.as_str()) {
                        if found.as_str() != "0.0" {
                            offenders.push(format!("{file}:{at} Margin with a literal"));
                        }
                    }
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "spacing must come from SPACE_XS / SPACE_SM / SPACE_MD / SPACE_LG, not a number picked by eye: {offenders:?}"
    );
    Ok(())
}

// ------------------------------------------------------- scan cancelling

/// Renders the file area, which is where the busy banner and its cancel
/// button live.
fn render_file_area(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    apply_style(ctx, app.palette);
    discard(ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| super::draw_file_area(app, ui));
    }));
}

/// The banner offers a way out of a scan, and the button is wired to it.
///
/// Driven through the recorded rect rather than by calling `cancel_scan`,
/// because the thing worth pinning is that the *control* reaches the
/// cancel — a scan of a whole volume with no visible way to stop it is
/// the state this sprint existed to remove.
#[test]
fn the_busy_banner_can_stop_a_running_scan() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let (_worker, progress) = app.pretend_scan_is_running();

    probe(&TEST_SCAN_CANCEL_RECTS).clear();
    render_file_area(&ctx, &mut app, raw_input(Vec::new()));
    let button = probe(&TEST_SCAN_CANCEL_RECTS).last().copied();
    let Some(button) = button else {
        anyhow::bail!("a running scan drew no cancel button");
    };

    assert!(
        !progress
            .cancelled
            .load(std::sync::atomic::Ordering::Relaxed),
        "nothing has been clicked yet"
    );
    render_file_area(
        &ctx,
        &mut app,
        raw_input(pointer_button(button.center(), true)),
    );
    render_file_area(
        &ctx,
        &mut app,
        raw_input(pointer_button(button.center(), false)),
    );

    assert!(
        progress
            .cancelled
            .load(std::sync::atomic::Ordering::Relaxed),
        "clicking Cancel scan did not reach the scan"
    );
    Ok(())
}

/// Esc stops a scan when nothing is open in front of it.
#[test]
fn escape_stops_a_running_scan() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let (_worker, progress) = app.pretend_scan_is_running();

    press(&ctx, &mut app, egui::Key::Escape, egui::Modifiers::NONE);

    assert!(
        progress
            .cancelled
            .load(std::sync::atomic::Ordering::Relaxed),
        "Escape with nothing open should stop the scan"
    );
    Ok(())
}

/// A cancelled scan is not a failed one, and leaves the tree alone.
///
/// The status line matters more than it looks: "Scan failed" in answer to
/// a button labelled Cancel teaches the user to distrust every other
/// message the status bar prints.
#[test]
fn a_cancelled_scan_keeps_the_tree_and_says_so() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let before = app.tree.root_path.clone();
    let (worker, _progress) = app.pretend_scan_is_running();

    worker.send(crate::gui::app::ScanMessage::Cancelled)?;
    app.poll_background(&ctx);

    assert_eq!(
        app.tree.root_path, before,
        "a cancelled scan must leave the tree that was already on screen"
    );
    let status = app.status.clone().unwrap_or_default();
    assert!(
        status.contains("cancel") || status.contains("Cancel"),
        "the status said {status:?} rather than reporting the cancel"
    );
    assert!(
        !status.contains("failed"),
        "a cancel was reported as a failure: {status:?}"
    );
    assert!(
        !app.scan_is_running(),
        "the scan should be finished with once its message arrives"
    );
    Ok(())
}

// ------------------------------------------------------- the frame budget

/// A tree far bigger than any window can show, with a realistic shape.
///
/// `breadth.pow(depth)` leaves, so 12^5 is a quarter of a million nodes —
/// the order of a real drive — while the root still has only twelve
/// children, which is what a real drive looks like from the top. A
/// fixture that put a quarter of a million children *under the root*
/// would be measuring a case the app never sees.
fn wide_deep_tree(breadth: usize, depth: usize) -> Node {
    fn level(breadth: usize, depth: usize, at: usize) -> Node {
        if depth == 0 {
            return crate::model::fixtures::file(&format!("leaf{at}.bin"), (at as u64 + 1) * 1_024);
        }
        let children: Vec<Node> = (0..breadth)
            .map(|index| level(breadth, depth - 1, index))
            .collect();
        crate::model::fixtures::dir(&format!("d{at}"), children)
    }
    level(breadth, depth, 0)
}

fn app_with_big_tree() -> GuiApp {
    let root = wide_deep_tree(12, 5);
    let mut app = GuiApp::new(Tree {
        root_path: std::path::PathBuf::from("big"),
        root,
        volume_free: None,
        volume_total: None,
        roots: Vec::new(),
        hard_link_bytes: None,
    });
    app.expanded.insert(Vec::new());
    app
}

/// One whole window frame, the way the app draws it.
fn render_window(ctx: &egui::Context, app: &mut GuiApp, input: egui::RawInput) {
    discard(ctx.run_ui(input, |ui| super::draw(app, ui)));
}

fn rebuild_counts() -> (u64, u64) {
    use std::sync::atomic::Ordering;
    (
        TEST_ROW_REBUILDS.load(Ordering::Relaxed),
        TEST_TREEMAP_REBUILDS.load(Ordering::Relaxed),
    )
}

/// A window nobody is touching does no tree-sized work at all.
///
/// This is the property the whole cache design exists for, and it is
/// invisible from the outside: a cache that misses every frame looks
/// exactly like one that hits, until the tree is nine million nodes and
/// the window stops responding. Both caches are keyed off observed state
/// (`RowKey`, `TreemapKey`), so a field that affects rows or tiles and is
/// missing from its key shows up here as a rebuild that should not have
/// happened.
#[test]
fn a_still_window_rebuilds_neither_rows_nor_tiles() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_big_tree();

    // Settle first. A window takes a few frames to reach a steady state:
    // the panels resolve their default sizes, and the treemap lays out
    // once at the reduced tile budget it uses while something is moving
    // and once more at full quality when nothing is
    // (`a_drag_lays_out_far_less_than_a_settled_frame`). The claim here
    // is about the frames after that, which is every frame a user spends
    // reading the window.
    for _ in 0..8 {
        render_window(&ctx, &mut app, raw_input(Vec::new()));
    }
    let (rows_before, tiles_before) = rebuild_counts();
    for _ in 0..8 {
        render_window(&ctx, &mut app, raw_input(Vec::new()));
    }
    let (rows_after, tiles_after) = rebuild_counts();

    assert_eq!(
        rows_after - rows_before,
        0,
        "the row list was rebuilt on a frame where nothing changed"
    );
    assert_eq!(
        tiles_after - tiles_before,
        0,
        "the treemap was laid out again on a frame where nothing changed"
    );
}

/// What a frame paints is bounded by the window, not by the tree.
///
/// A quarter-million-node tree draws the same handful of rows and a
/// tile count bounded by the panel's area, because rows are virtualized
/// by the table and tiles are budgeted by `MIN_TILE_AREA_PX`. Without
/// both, the frame cost would grow with the scan and 120 FPS would be a
/// property of small directories only.
#[test]
fn a_frame_over_a_huge_tree_draws_only_what_fits() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_big_tree();
    let nodes = app.tree.root.file_count + app.tree.root.dir_count;
    assert!(
        nodes > 200_000,
        "the fixture is meant to dwarf the window: {nodes} nodes"
    );

    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    for _ in 0..2 {
        render_window(&ctx, &mut app, raw_input(Vec::new()));
    }
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    render_window(&ctx, &mut app, raw_input(Vec::new()));

    let painted = probe(&TEST_DIRECTORY_ROW_RECTS).len();
    assert!(
        painted > 0 && painted < 200,
        "{painted} rows were painted for a {nodes}-node tree; a frame must \
         cost what the window shows, not what the scan found"
    );
    assert!(
        app.treemap_tiles.len() <= crate::gui::treemap_layout::MAX_TILES_INTERACTIVE,
        "the treemap laid out {} tiles, past its own budget",
        app.treemap_tiles.len()
    );
}

/// The median frame is inside the budget.
///
/// Median rather than mean or max on purpose: one scheduler preemption
/// on a shared CI runner must not fail a build, and a real regression
/// moves the middle of the distribution rather than one sample. The
/// budget itself is `theme::FRAME_BUDGET`, scaled by
/// `DEBUG_FRAME_BUDGET_FACTOR` because tests run unoptimized.
fn median_frame_time(ctx: &egui::Context, app: &mut GuiApp, frames: usize) -> std::time::Duration {
    // Warm up: the first frames allocate the galley cache, lay out panels
    // and fill both caches, and none of that repeats.
    for _ in 0..4 {
        render_window(ctx, app, raw_input(Vec::new()));
    }
    let mut samples = Vec::with_capacity(frames);
    for _ in 0..frames {
        let started = std::time::Instant::now();
        render_window(ctx, app, raw_input(Vec::new()));
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    samples.get(samples.len() / 2).copied().unwrap_or_default()
}

fn frame_budget() -> std::time::Duration {
    let budget = super::theme::FRAME_BUDGET;
    if cfg!(debug_assertions) {
        budget * super::theme::DEBUG_FRAME_BUDGET_FACTOR
    } else {
        budget
    }
}

#[test]
fn a_frame_over_a_huge_tree_fits_the_budget() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_big_tree();

    let median = median_frame_time(&ctx, &mut app, 30);
    let budget = frame_budget();
    assert!(
        median <= budget,
        "the median frame took {median:?} against a {budget:?} budget"
    );
}

/// And it still fits while a scan is running.
///
/// This is the sprint's actual claim. The scan pool leaves a core free
/// and the workers share nothing with the UI thread but three atomics,
/// so a walk in flight should cost the frame nothing — but that is an
/// argument, and this is a measurement. The scan is a real one over a
/// real temporary tree, and the frames are rendered while it walks.
#[test]
fn frames_stay_inside_the_budget_while_a_scan_runs() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = crate::util::scratch_dir("gui", "frame_budget_scan");
    // Wide and shallow: enough entries that the walk takes long enough to
    // overlap the frames below, without a fixture that takes longer to
    // build than the test takes to run.
    for folder in 0..24 {
        let dir = root.join(format!("d{folder}"));
        std::fs::create_dir_all(&dir)?;
        for index in 0..64 {
            std::fs::write(dir.join(format!("f{index}.bin")), vec![b'x'; 512])?;
        }
    }

    let ctx = egui::Context::default();
    let mut app = app_with_big_tree();
    for _ in 0..4 {
        render_window(&ctx, &mut app, raw_input(Vec::new()));
    }

    // The worker walks the fixture over and over until the render loop
    // has what it needs. One pass over a fixture small enough to build
    // inside a test finishes in a couple of frames, which would measure
    // almost nothing; looping keeps a real scan — the same rayon pool,
    // the same atomics, the same allocation churn — running underneath
    // every sample.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stop = std::sync::Arc::clone(&stop);
    let scan_root = root.clone();
    let handle = std::thread::spawn(move || -> anyhow::Result<u32> {
        let mut passes = 0;
        while !worker_stop.load(std::sync::atomic::Ordering::Relaxed) {
            let progress = crate::scanner::Progress::default();
            crate::scanner::scan(&scan_root, Some(&progress))?;
            passes += 1;
        }
        Ok(passes)
    });

    let mut samples = Vec::with_capacity(30);
    for _ in 0..30 {
        let started = std::time::Instant::now();
        render_window(&ctx, &mut app, raw_input(Vec::new()));
        samples.push(started.elapsed());
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let Ok(passes) = handle.join() else {
        anyhow::bail!("the scan thread panicked");
    };
    let passes = passes?;

    assert!(
        passes >= 1,
        "the scan thread never completed a pass, so nothing was measured under load"
    );
    samples.sort_unstable();
    let median = samples.get(samples.len() / 2).copied().unwrap_or_default();
    let budget = frame_budget();
    assert!(
        median <= budget,
        "with a scan running the median frame took {median:?} against a {budget:?} budget"
    );
    Ok(())
}

/// A busy app asks for the next frame, not for one in 33 milliseconds.
///
/// `request_repaint_after(33ms)` caps the window at 30 FPS for the whole
/// of a scan however much headroom the machine has, which is the one way
/// to miss a 120 FPS target without any frame being slow.
#[test]
fn a_busy_app_asks_for_the_next_frame_immediately() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let (_worker, _progress) = app.pretend_scan_is_running();

    let output = ctx.run_ui(raw_input(Vec::new()), |ui| {
        app.poll_background(ui.ctx());
        super::draw(&mut app, ui);
    });
    let delay = output
        .viewport_output
        .values()
        .map(|v| v.repaint_delay)
        .min();
    discard(output);

    let Some(delay) = delay else {
        return;
    };
    assert!(
        delay <= std::time::Duration::from_millis(1),
        "a scan in flight asked for the next frame in {delay:?}, which caps the \
         window well under its 120 FPS target"
    );
}

// -------------------------------------------------- the properties window

/// The inspector paints, and the window behind it keeps working.
///
/// This is the whole reason it left the modal. A modal answers a click by
/// dismissing itself and blocks every shortcut behind it; an inspector
/// that did either would be useless for the thing people actually do with
/// it, which is click through a tree and watch the numbers change.
#[test]
fn the_properties_window_leaves_the_app_usable() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_files();
    app.toggle_properties();
    // Parked to one side, which is both what the remembered-position path
    // does in a real session and what leaves rows uncovered to click in a
    // 900px test window.
    app.properties.pos = Some([1000.0, 30.0]);

    probe(&TEST_PROPERTIES_RECTS).clear();
    probe(&TEST_DIRECTORY_ROW_RECTS).clear();
    for _ in 0..2 {
        render_window(&ctx, &mut app, window_input(Vec::new()));
    }
    assert!(
        !probe(&TEST_PROPERTIES_RECTS).is_empty(),
        "an open inspector painted nothing"
    );
    assert!(
        !modal_is_open(&app),
        "the inspector must not read as a modal, or it blocks every shortcut"
    );

    // A row behind the window still takes a click. The inspector is
    // placed at the top-left by default, so pick a row well clear of it.
    //
    // Both probes are copied out and the guards dropped before the next
    // frame: the drawing code pushes into the same mutexes, so holding
    // one across a render deadlocks the test rather than failing it.
    let rows: Vec<(Vec<usize>, egui::Rect)> = probe(&TEST_DIRECTORY_ROW_RECTS)
        .iter()
        .map(|(path, rect)| (path.clone(), *rect))
        .collect();
    let window = probe(&TEST_PROPERTIES_RECTS).last().copied();
    let Some(window) = window else {
        anyhow::bail!("no inspector rect was recorded");
    };
    // A row is wider than the pane it scrolls in, so "does not overlap
    // the window" would rule every row out. What matters is that a point
    // *on* the row and clear of the window still reaches the app, so the
    // click lands left of the inspector rather than beside it.
    let target = rows.iter().find_map(|(path, rect)| {
        if path.is_empty() || !rect.is_finite() {
            return None;
        }
        let x = rect.center().x.min(window.left() - 24.0);
        let at = egui::pos2(x, rect.center().y);
        (rect.contains(at) && !window.contains(at)).then(|| (path.clone(), at))
    });
    let Some((path, at)) = target else {
        let seen: Vec<egui::Rect> = rows.iter().map(|(_, rect)| *rect).collect();
        anyhow::bail!("no row offered a point clear of the inspector at {window:?}: {seen:?}");
    };
    render_window(&ctx, &mut app, window_input(pointer_button(at, true)));
    render_window(&ctx, &mut app, window_input(pointer_button(at, false)));
    assert_eq!(
        app.selected_path.as_deref(),
        Some(path.as_slice()),
        "clicking a row behind the inspector did not select it"
    );
    Ok(())
}

/// Keyboard shortcuts keep working while it is open.
///
/// `handle_shortcuts` returns early for a modal on purpose — Del must not
/// queue a second delete behind a confirmation. An inspector is not that,
/// and the check that tells them apart is `modal_is_open`.
#[test]
fn shortcuts_still_work_while_the_inspector_is_open() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_files();
    app.toggle_properties();
    app.file_view = FileView::AllFiles;

    press(&ctx, &mut app, egui::Key::F, egui::Modifiers::CTRL);

    assert_eq!(
        app.file_view,
        FileView::SearchResults,
        "Ctrl+F was swallowed while the inspector was open"
    );
}

/// It follows the selection rather than capturing one.
#[test]
fn the_inspector_describes_whatever_is_selected_now() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_sortable_files();
    app.toggle_properties();

    app.selected_path = Some(vec![0]);
    let first = app
        .selected_node()
        .map(|node| node.name.to_string_lossy().to_string());
    app.selected_path = Some(vec![1]);
    let second = app
        .selected_node()
        .map(|node| node.name.to_string_lossy().to_string());

    assert!(
        first.is_some() && second.is_some() && first != second,
        "the fixture should offer two different items to describe: {first:?} vs {second:?}"
    );
}

/// The toggle is a toggle, and a closed inspector paints nothing.
#[test]
fn the_inspector_toggles_shut() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_files();

    app.toggle_properties();
    assert!(app.properties.open, "the first toggle should open it");
    app.toggle_properties();
    assert!(!app.properties.open, "the second toggle should close it");

    probe(&TEST_PROPERTIES_RECTS).clear();
    render_window(&ctx, &mut app, raw_input(Vec::new()));
    assert!(
        probe(&TEST_PROPERTIES_RECTS).is_empty(),
        "a closed inspector still painted"
    );
}

/// Where it was left is remembered, and a half-saved position is ignored.
#[test]
fn the_inspector_position_survives_a_config_round_trip() {
    let saved = crate::config::Config {
        gui_properties_open: Some(true),
        gui_properties_pos: Some(vec![320.0, 210.0]),
        ..crate::config::Config::default()
    };
    let restored = crate::gui::app::PropertiesWindow::from_config(&saved);
    assert!(
        restored.open,
        "the inspector was showing when the app closed"
    );
    assert_eq!(restored.pos, Some([320.0, 210.0]));

    // Half a position is not a position.
    let broken = crate::config::Config {
        gui_properties_pos: Some(vec![320.0]),
        ..crate::config::Config::default()
    };
    assert_eq!(
        crate::gui::app::PropertiesWindow::from_config(&broken).pos,
        None,
        "a malformed position must fall back to the default placement"
    );
}

// ------------------------------------------------------- the locations page

/// The Locations page starts a scan of everything ticked.
///
/// Driven through the recorded button rect, and through the page's own
/// selection state, so what is pinned is the wiring a user actually
/// touches: tick, press, and the app is scanning several roots as one
/// tree.
#[test]
fn the_locations_page_scans_what_is_ticked() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let first = crate::util::scratch_dir("gui", "locations_first");
    let second = crate::util::scratch_dir("gui", "locations_second");
    std::fs::create_dir_all(&first)?;
    std::fs::create_dir_all(&second)?;
    std::fs::write(first.join("a.bin"), *b"a")?;
    std::fs::write(second.join("b.bin"), *b"b")?;

    app.open_modal(super::modal::ModalPage::Locations);
    app.tools.selected_locations = vec![first.clone(), second.clone()];
    // The card waits on a screenshot that never arrives without a
    // renderer, so it paints nothing for the first few frames; see
    // `SETTLE_FRAMES`.
    for _ in 0..SETTLE_FRAMES {
        render_window(&ctx, &mut app, window_input(Vec::new()));
    }
    probe(&TEST_LOCATION_SCAN_RECTS).clear();
    render_window(&ctx, &mut app, window_input(Vec::new()));
    let button = probe(&TEST_LOCATION_SCAN_RECTS).last().copied();
    let Some(button) = button else {
        anyhow::bail!("the Locations page drew no scan button");
    };

    render_window(
        &ctx,
        &mut app,
        window_input(pointer_button(button.center(), true)),
    );
    render_window(
        &ctx,
        &mut app,
        window_input(pointer_button(button.center(), false)),
    );

    assert!(
        app.scan_is_running(),
        "pressing Scan should have started one"
    );
    assert!(
        !modal_is_open(&app),
        "and closed the page it was pressed on"
    );
    wait_for_background(&mut app, &ctx);

    assert!(
        app.tree.is_multi_root(),
        "two ticked locations should scan into one multi-root tree"
    );
    assert_eq!(app.scanned_roots().len(), 2);
    assert_eq!(
        app.tree.root.file_count,
        2,
        "one file from each location: {:?}",
        app.scanned_roots()
    );
    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
    Ok(())
}

/// Waits for the scan worker, polling the way the window does.
fn wait_for_background(app: &mut GuiApp, ctx: &egui::Context) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.is_busy() && std::time::Instant::now() < deadline {
        app.poll_background(ctx);
    }
}

/// The inspector can be dragged, and remembers where it was left.
///
/// The whole point of it being a window rather than a modal page. It is
/// positioned from `app.properties.pos` every frame, so if that were not
/// written back from the window's own rect a drag would spring straight
/// back to where it started.
#[test]
fn the_inspector_can_be_dragged() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_sortable_files();
    app.toggle_properties();
    app.properties.pos = Some([400.0, 200.0]);
    for _ in 0..3 {
        render_window(&ctx, &mut app, window_input(Vec::new()));
    }

    // The title strip, a little in from the window's top-left corner.
    let grab = egui::pos2(460.0, 210.0);
    render_window(&ctx, &mut app, window_input(pointer_button(grab, true)));
    for step in 1..=4 {
        let to = grab + egui::vec2(30.0 * step as f32 / 4.0, 20.0 * step as f32 / 4.0);
        render_window(&ctx, &mut app, window_input(pointer_move(to)));
    }
    let end = grab + egui::vec2(30.0, 20.0);
    render_window(&ctx, &mut app, window_input(pointer_button(end, false)));
    render_window(&ctx, &mut app, window_input(Vec::new()));

    let moved = app.properties.pos.unwrap_or_default();
    assert!(
        moved[0] > 420.0 && moved[1] > 210.0,
        "dragging the inspector left it at {moved:?}, which is where it started"
    );
    Ok(())
}

/// A maintenance tool never runs against a fabricated volume.
///
/// `wintools` takes the first component of the path it is given as the
/// volume, and a multi-root tree's `root_path` is the label standing in
/// for its roots — so the tools were being handed the first word of a UI
/// string. These are the most destructive things in the app.
#[test]
fn a_maintenance_tool_refuses_a_multi_root_scan_with_no_selection() {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app_with_one_file();
    app.tree = std::sync::Arc::new(Tree {
        root_path: std::path::PathBuf::from(crate::scanner::MULTI_ROOT_LABEL),
        root: Node {
            name: std::ffi::OsString::from(crate::scanner::MULTI_ROOT_LABEL),
            is_dir: true,
            is_symlink: false,
            size: 0,
            physical_size: 0,
            file_count: 0,
            dir_count: 1,
            modified: None,
            children: vec![file("only.txt", 1)],
            error: false,
            category: None,
            ext_totals: vec![(0, 0, 0); Category::COUNT],
            unreadable_count: 0,
            file_id: None,
            other_filesystem: false,
        },
        volume_free: None,
        volume_total: None,
        hard_link_bytes: None,
        roots: vec![crate::model::Root {
            path: std::path::PathBuf::from("C:\\"),
            volume_free: None,
            volume_total: None,
        }],
    });
    app.selected_path = None;

    app.request_windows_tool(0);
    app.confirm_windows_tool();

    assert!(
        !app.is_busy(),
        "no tool should have been started without a volume to point it at"
    );
    let status = app.status.clone().unwrap_or_default();
    assert!(
        status.contains("Select an item"),
        "the refusal should say what to do, got {status:?}"
    );
}

// ----------------------------------------------------------- live scans

/// A published folder is on screen before the scan finishes.
///
/// The point of the whole streaming path: a drive that takes a minute to
/// walk shows its first folders in the first second, rather than an empty
/// window and a spinner. Driven through the real message the scan worker
/// sends, so what is pinned is the window's half of that contract.
#[test]
fn a_published_child_appears_before_the_scan_finishes() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let (worker, _progress) = app.pretend_scan_is_running();

    worker.send(crate::gui::app::ScanMessage::Child(Box::new(dir_node(
        "Users", 4_096,
    ))))?;
    app.poll_background(&ctx);

    assert!(app.live_scan, "the first child starts the live tree");
    assert_eq!(
        app.tree.root.children.len(),
        1,
        "the published folder should be attached"
    );
    assert_eq!(app.tree.root.size, 4_096, "and counted");
    assert!(
        app.scan_is_running(),
        "with the scan still going — that is the whole point"
    );

    worker.send(crate::gui::app::ScanMessage::Child(Box::new(dir_node(
        "Windows", 8_192,
    ))))?;
    app.poll_background(&ctx);
    assert_eq!(app.tree.root.children.len(), 2);
    assert_eq!(
        app.tree.root.size, 12_288,
        "totals grow with each folder, not only at the end"
    );
    Ok(())
}

/// Attaching a child invalidates the caches that draw it.
///
/// A tree that grows in place keeps its address, and both caches key off
/// that address — so without the generation counter the window would
/// happily go on drawing the rows it had before the folder arrived, and
/// the live tree would be live in memory only.
#[test]
fn attaching_a_child_rebuilds_the_rows_and_tiles() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    let (worker, _progress) = app.pretend_scan_is_running();
    worker.send(crate::gui::app::ScanMessage::Child(Box::new(dir_node(
        "Users", 4_096,
    ))))?;
    app.poll_background(&ctx);
    for _ in 0..4 {
        render_window(&ctx, &mut app, window_input(Vec::new()));
    }
    let rows_before = app.visible_row_count();

    worker.send(crate::gui::app::ScanMessage::Child(Box::new(dir_node(
        "Windows", 8_192,
    ))))?;
    app.poll_background(&ctx);
    render_window(&ctx, &mut app, window_input(Vec::new()));

    assert!(
        app.visible_row_count() > rows_before,
        "the second folder should have reached the row list: {} rows before, {} after",
        rows_before,
        app.visible_row_count()
    );
    Ok(())
}

/// What the window ends up with is what a plain scan would have found.
///
/// The real risk of assembling a tree from published parts: the live one
/// and the finished one disagreeing. This runs an actual scan of a real
/// fixture through the window's own worker and message loop, then
/// compares the result against `scan_to_completion` over the same
/// directory.
#[test]
fn a_live_scan_ends_up_agreeing_with_a_plain_one() -> anyhow::Result<()> {
    let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = crate::util::scratch_dir("gui", "live_scan");
    for folder in ["one", "two", "three"] {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir)?;
        for index in 0..8 {
            std::fs::write(
                dir.join(format!("f{index}.bin")),
                vec![b'x'; 64 * (index + 1)],
            )?;
        }
    }
    std::fs::write(root.join("loose.txt"), vec![b'y'; 500])?;

    let expected = crate::scanner::scan_to_completion(&root)?;

    let ctx = egui::Context::default();
    let mut app = app_with_one_file();
    app.open_folder(&root)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.is_busy() && std::time::Instant::now() < deadline {
        app.poll_background(&ctx);
    }

    assert!(!app.live_scan, "the scan finished, so the tree is not live");
    assert_eq!(app.tree.root_path, expected.root_path);
    assert_eq!(
        app.tree.root.children.len(),
        expected.root.children.len(),
        "every top-level entry should have been published exactly once"
    );
    assert_eq!(app.tree.root.size, expected.root.size, "same bytes");
    assert_eq!(app.tree.root.file_count, expected.root.file_count);
    assert_eq!(app.tree.root.dir_count, expected.root.dir_count);
    assert_eq!(
        app.tree.root.ext_totals, expected.root.ext_totals,
        "and the same breakdown by category"
    );
    assert!(
        !app.extensions.is_empty(),
        "the extension rows are summed on the scan thread and should arrive with it"
    );
    assert!(!app.largest_files.is_empty(), "so should the largest files");

    // The largest file's index path must resolve in the assembled tree —
    // it was numbered by the worker against the order it published in.
    let Some(largest) = app.largest_files.first() else {
        anyhow::bail!("the fixture has files, so there is a largest one");
    };
    let Some(node) = app.tree.node_for(&largest.index_path) else {
        anyhow::bail!(
            "the largest file's path does not resolve: {:?} at {:?}",
            largest.name,
            largest.index_path
        );
    };
    assert_eq!(
        node.name.to_string_lossy(),
        largest.name,
        "and it should resolve to the file it names"
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A directory node with a known size, for the message-level tests.
fn dir_node(name: &str, size: u64) -> Node {
    Node {
        name: std::ffi::OsString::from(name),
        is_dir: true,
        is_symlink: false,
        size,
        physical_size: size,
        file_count: 1,
        dir_count: 0,
        modified: None,
        children: vec![file(&format!("{name}.bin"), size)],
        error: false,
        category: None,
        ext_totals: vec![(0, 0, 0); Category::COUNT],
        unreadable_count: 0,
        file_id: None,
        other_filesystem: false,
    }
}
