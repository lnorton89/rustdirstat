use super::app::{
    extension_label, size_label, ExtensionSortMode, FileView, GuiApp, PaneOrientation,
};
use super::icons::Icon;
use crate::color;
use crate::model::Node;
use crate::tui::SortMode;
use crate::util::{format_modified, human_bytes, thousands};
use eframe::egui::{
    self, Align, Color32, Frame, Layout, Margin, RichText, Sense, Stroke, TextStyle, Vec2,
};
use egui_extras::{Column, TableBuilder};

const PAD: f32 = 10.0;

pub fn draw(app: &mut GuiApp, ctx: &egui::Context) {
    apply_style(ctx);
    draw_menu_bar(app, ctx);
    if app.show_toolbar {
        draw_toolbar(app, ctx);
    }
    if app.show_status_bar {
        draw_status_bar(app, ctx);
    }
    draw_workspace(app, ctx);
    draw_delete_dialog(app, ctx);
    draw_properties_dialog(app, ctx);
    draw_settings_dialog(app, ctx);
    draw_windows_tools_dialog(app, ctx);
    draw_windows_tool_confirmation(app, ctx);
    draw_about_dialog(app, ctx);
    handle_shortcuts(app, ctx);
}

fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.menu_margin = Margin::same(8.0);
    style.spacing.indent = 18.0;
    // This is an application UI, not a document viewer. Selectable labels
    // steal pointer drags/clicks from table rows and make row selection feel
    // broken whenever the pointer lands on text.
    style.interaction.selectable_labels = false;
    style.visuals.panel_fill = Color32::from_rgb(25, 27, 32);
    style.visuals.extreme_bg_color = Color32::from_rgb(18, 20, 24);
    style.visuals.widgets.noninteractive.bg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(55, 59, 68));
    style.visuals.selection.bg_fill = Color32::from_rgb(45, 104, 190);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(126, 178, 255));
    for widgets in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.rounding = egui::Rounding::same(6.0);
    }
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    ctx.set_style(style);
}

fn panel_frame() -> Frame {
    Frame::none()
        .fill(Color32::from_rgb(25, 27, 32))
        .inner_margin(Margin::same(PAD))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 56, 65)))
}

fn draw_menu_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar")
        .frame(Frame::none().inner_margin(Margin::symmetric(8.0, 3.0)))
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if icon_button(
                        ui,
                        !app.is_busy(),
                        Icon::FolderOpen,
                        "     Select folder…     Ctrl+O",
                    )
                    .clicked()
                    {
                        choose_folder(app);
                        ui.close_menu();
                    }
                    if icon_button(
                        ui,
                        !app.is_busy(),
                        Icon::Refresh,
                        "     Rescan                F5",
                    )
                    .clicked()
                    {
                        refresh(app);
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::Export, "     Export CSV…").clicked() {
                        export_csv(app);
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::ExternalLink, "     Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if icon_button(
                        ui,
                        app.selected_path.is_some(),
                        Icon::Copy,
                        "     Copy path     Ctrl+C",
                    )
                    .clicked()
                    {
                        copy_path(app);
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Search, "     Search…        Ctrl+F").clicked()
                    {
                        app.file_view = FileView::SearchResults;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Cleanup", |ui| {
                    let selected = app.selected_path.is_some();
                    if icon_button(ui, selected, Icon::ExternalLink, "     Open").clicked() {
                        open_selected(app);
                        ui.close_menu();
                    }
                    if icon_button(ui, selected, Icon::Folder, "     Show in Explorer").clicked() {
                        reveal_selected(app);
                        ui.close_menu();
                    }
                    if icon_button(ui, selected, Icon::Info, "     Properties").clicked() {
                        app.show_properties = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(
                        ui,
                        selected,
                        Icon::Trash,
                        "     Delete to Recycle Bin     Del",
                    )
                    .clicked()
                    {
                        app.request_delete_selected(false);
                        ui.close_menu();
                    }
                    if icon_button(ui, selected, Icon::Trash, "     Delete permanently").clicked() {
                        app.request_delete_selected(true);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Treemap", |ui| {
                    if ui
                        .selectable_label(
                            app.orientation == PaneOrientation::Horizontal,
                            "Horizontal — below",
                        )
                        .clicked()
                    {
                        app.orientation = PaneOrientation::Horizontal;
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(
                            app.orientation == PaneOrientation::Vertical,
                            "Vertical — right",
                        )
                        .clicked()
                    {
                        app.orientation = PaneOrientation::Vertical;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.radio(!app.use_physical, "Logical size").clicked() {
                        app.use_physical = false;
                        app.refresh_extensions();
                    }
                    if ui.radio(app.use_physical, "Physical size").clicked() {
                        app.use_physical = true;
                        app.refresh_extensions();
                    }
                    ui.checkbox(&mut app.show_grid, "Grid lines");
                    ui.checkbox(&mut app.show_labels, "File labels");
                    ui.checkbox(&mut app.show_free_space, "Free space");
                    ui.separator();
                    if icon_button(ui, true, Icon::ZoomIn, "     Zoom in").clicked() {
                        app.zoom_in();
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::ZoomOut, "     Zoom out").clicked() {
                        app.zoom_out();
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Home, "     Reset zoom").clicked() {
                        app.reset_zoom();
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    view_menu_item(app, ui, FileView::AllFiles);
                    view_menu_item(app, ui, FileView::LargestFiles);
                    if icon_button(ui, !app.is_busy(), Icon::Duplicate, "     Duplicate Files")
                        .clicked()
                    {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                    view_menu_item(app, ui, FileView::SearchResults);
                    ui.separator();
                    ui.checkbox(&mut app.show_extension_view, "Extension list");
                    ui.checkbox(&mut app.show_treemap, "Treemap");
                    ui.checkbox(&mut app.show_toolbar, "Toolbar");
                    ui.checkbox(&mut app.show_status_bar, "Status bar");
                    ui.separator();
                    if icon_button(ui, true, Icon::Settings, "     Settings…").clicked() {
                        app.show_settings = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if icon_button(ui, true, Icon::Tools, "     Windows maintenance…").clicked() {
                        app.show_windows_tools = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::Duplicate, "     Find duplicate files").clicked()
                    {
                        app.find_duplicates();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if icon_button(ui, true, Icon::Help, "     WinDirStat view guide").clicked() {
                        app.show_about = true;
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Info, "     About RustDirStat").clicked() {
                        app.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
}

fn view_menu_item(app: &mut GuiApp, ui: &mut egui::Ui, view: FileView) {
    if icon_selectable_label(ui, app.file_view == view, view_icon(view), view.label()).clicked() {
        app.file_view = view;
        ui.close_menu();
    }
}

fn view_icon(view: FileView) -> Icon {
    match view {
        FileView::AllFiles => Icon::Tree,
        FileView::LargestFiles => Icon::Largest,
        FileView::DuplicateFiles => Icon::Duplicate,
        FileView::SearchResults => Icon::Search,
    }
}

fn draw_toolbar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("toolbar")
        .frame(Frame::none().inner_margin(Margin::symmetric(10.0, 7.0)))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                paint_inline_icon(ui, Icon::App, 20.0, Color32::from_rgb(104, 168, 255));
                ui.label(RichText::new("RustDirStat").strong().size(15.0));
                ui.separator();
                if tool_enabled(ui, !app.is_busy(), Icon::FolderOpen, "Select folder").clicked() {
                    choose_folder(app);
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Refresh, "Rescan (F5)").clicked() {
                    refresh(app);
                }
                ui.separator();
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::ExternalLink,
                    "Open selected",
                )
                .clicked()
                {
                    open_selected(app);
                }
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::Folder,
                    "Show in Explorer",
                )
                .clicked()
                {
                    reveal_selected(app);
                }
                if tool_enabled(ui, app.selected_path.is_some(), Icon::Copy, "Copy path").clicked()
                {
                    copy_path(app);
                }
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::Trash,
                    "Delete to Recycle Bin",
                )
                .clicked()
                {
                    app.request_delete_selected(false);
                }
                if tool_enabled(ui, app.selected_path.is_some(), Icon::Info, "Properties").clicked()
                {
                    app.show_properties = true;
                }
                ui.separator();
                if tool_enabled(
                    ui,
                    app.selected_path.is_some(),
                    Icon::ZoomIn,
                    "Zoom treemap to selection",
                )
                .clicked()
                {
                    app.zoom_in();
                }
                if tool_enabled(ui, !app.zoom_path.is_empty(), Icon::ZoomOut, "Zoom out").clicked()
                {
                    app.zoom_out();
                }
                if tool(ui, Icon::Home, "Reset treemap zoom").clicked() {
                    app.reset_zoom();
                }
                ui.separator();
                let orient = if app.orientation == PaneOrientation::Horizontal {
                    Icon::LayoutHorizontal
                } else {
                    Icon::LayoutVertical
                };
                if tool(
                    ui,
                    orient,
                    &format!("Layout: {} (click to switch)", app.orientation.label()),
                )
                .clicked()
                {
                    app.orientation.toggle();
                }
                if tool(ui, Icon::Settings, "Settings").clicked() {
                    app.show_settings = true;
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Tools, "Windows maintenance tools")
                    .clicked()
                {
                    app.show_windows_tools = true;
                }
                if ui.available_width() > 240.0 {
                    ui.separator();
                    ui.label(RichText::new(app.tree.root_path.display().to_string()).strong());
                }
            });
        });
}

fn tool(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    tool_enabled(ui, true, icon, tip)
}
fn tool_enabled(ui: &mut egui::Ui, enabled: bool, icon: Icon, tip: &str) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(Vec2::new(38.0, 34.0)),
    );
    let color = if enabled {
        ui.style().interact(&response).fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    icon.paint(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(18.0)),
        color,
    );
    response.on_hover_text(tip)
}

fn compact_icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(20.0)),
    );
    let color = ui.style().interact(&response).fg_stroke.color;
    icon.paint(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), Vec2::splat(12.0)),
        color,
    );
    response.on_hover_text(tip)
}

fn paint_inline_icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    icon.paint(ui.painter(), rect.shrink(1.0), color);
}

fn icon_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    icon_menu_item(ui, true, selected, icon, label)
}

fn icon_button(ui: &mut egui::Ui, enabled: bool, icon: Icon, label: &str) -> egui::Response {
    icon_menu_item(ui, enabled, false, icon, label)
}

fn icon_menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    selected: bool,
    icon: Icon,
    label: &str,
) -> egui::Response {
    const ICON_SIZE: f32 = 15.0;
    const ICON_TEXT_GAP: f32 = 8.0;
    let label = label.trim_start();
    ui.add_enabled_ui(enabled, |ui| {
        let galley = egui::WidgetText::from(label).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );
        let padding = ui.spacing().button_padding;
        let desired = Vec2::new(
            padding.x * 2.0 + ICON_SIZE + ICON_TEXT_GAP + galley.size().x,
            (padding.y * 2.0 + galley.size().y)
                .max(28.0)
                .max(ui.spacing().interact_size.y),
        );
        let (rect, response) = ui.allocate_at_least(desired, Sense::click());
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + padding.x + ICON_SIZE * 0.5, rect.center().y),
            Vec2::splat(ICON_SIZE),
        );
        let text_rect = egui::Rect::from_min_size(
            egui::pos2(
                icon_rect.right() + ICON_TEXT_GAP,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley.size(),
        );

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact_selectable(&response, selected);
            if selected || response.hovered() || response.highlighted() || response.has_focus() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                );
            }
            let color = if enabled {
                visuals.text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            icon.paint(ui.painter(), icon_rect, color);
            ui.painter().galley(text_rect.min, galley, color);
        }

        #[cfg(test)]
        TEST_ICON_MENU_LAYOUTS
            .lock()
            .unwrap()
            .push((label.to_owned(), rect, icon_rect, text_rect));
        response
    })
    .inner
}

fn icon_heading(ui: &mut egui::Ui, icon: Icon, label: &str) {
    ui.horizontal(|ui| {
        paint_inline_icon(ui, icon, 19.0, Color32::from_rgb(104, 168, 255));
        ui.heading(label);
    });
}

fn draw_status_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .frame(Frame::none().inner_margin(Margin::symmetric(10.0, 4.0)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if app.is_busy() {
                    ui.spinner();
                } else {
                    paint_inline_icon(ui, Icon::Info, 14.0, ui.visuals().weak_text_color());
                }
                ui.label(
                    app.busy_text()
                        .as_deref()
                        .or(app.status.as_deref())
                        .unwrap_or("Ready"),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let node = app.zoom_node();
                    ui.label(format!(
                        "{} files · {} folders · {}",
                        thousands(node.file_count),
                        thousands(node.dir_count),
                        size_label(node.effective_size(app.use_physical), app.use_physical)
                    ));
                });
            });
        });
}

fn draw_workspace(app: &mut GuiApp, ctx: &egui::Context) {
    match (app.show_treemap, app.orientation) {
        (true, PaneOrientation::Horizontal) => {
            egui::TopBottomPanel::bottom("treemap_horizontal")
                .resizable(true)
                .default_height(280.0)
                .min_height(0.0)
                .frame(panel_frame())
                .show(ctx, |ui| draw_treemap(app, ui));
            draw_upper_workspace(app, ctx, true);
        }
        (true, PaneOrientation::Vertical) => {
            egui::SidePanel::right("treemap_vertical")
                .resizable(true)
                .default_width(ctx.available_rect().width() * 0.48)
                .min_width(0.0)
                .frame(panel_frame())
                .show(ctx, |ui| draw_treemap(app, ui));
            draw_upper_workspace(app, ctx, false);
        }
        (false, _) => {
            draw_upper_workspace(app, ctx, app.orientation == PaneOrientation::Horizontal)
        }
    }
}

fn draw_upper_workspace(app: &mut GuiApp, ctx: &egui::Context, extension_on_right: bool) {
    if app.show_extension_view {
        if extension_on_right {
            egui::SidePanel::right("extension_right")
                .resizable(true)
                .default_width(330.0)
                .min_width(0.0)
                .frame(panel_frame())
                .show(ctx, |ui| draw_extension_list(app, ui));
        } else {
            egui::TopBottomPanel::bottom("extension_bottom")
                .resizable(true)
                .default_height(220.0)
                .min_height(0.0)
                .frame(panel_frame())
                .show(ctx, |ui| draw_extension_list(app, ui));
        }
    }
    egui::CentralPanel::default()
        .frame(panel_frame())
        .show(ctx, |ui| draw_file_area(app, ui));
}

fn draw_file_area(app: &mut GuiApp, ui: &mut egui::Ui) {
    if let Some(message) = app.busy_text() {
        Frame::none()
            .fill(Color32::from_rgb(31, 49, 72))
            .rounding(egui::Rounding::same(7.0))
            .inner_margin(Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(message).strong());
                    ui.label(RichText::new("You can keep browsing the current scan.").weak());
                });
            });
        ui.add_space(6.0);
    }
    ui.horizontal(|ui| {
        for view in [
            FileView::AllFiles,
            FileView::LargestFiles,
            FileView::DuplicateFiles,
            FileView::SearchResults,
        ] {
            let clicked =
                icon_selectable_label(ui, app.file_view == view, view_icon(view), view.label())
                    .clicked();
            if clicked {
                if view == FileView::DuplicateFiles && app.duplicate_groups.is_empty() {
                    app.find_duplicates();
                } else {
                    app.file_view = view;
                }
            }
        }
    });
    ui.add_space(4.0);
    match app.file_view {
        FileView::AllFiles => draw_directory_tree(app, ui),
        FileView::LargestFiles => draw_largest_files(app, ui),
        FileView::DuplicateFiles => draw_duplicates(app, ui),
        FileView::SearchResults => draw_search(app, ui),
    }
}

#[derive(Clone)]
struct TreeRow {
    path: Vec<usize>,
    depth: usize,
    name: String,
    is_dir: bool,
    size: u64,
    parent_size: u64,
    files: u64,
    dirs: u64,
    modified: Option<std::time::SystemTime>,
    unreadable: u64,
    symlink: bool,
}

enum RowAction {
    Open,
    Reveal,
    CopyPath,
    Zoom,
    Properties,
    Delete,
}

#[cfg(test)]
static TEST_DIRECTORY_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_DIRECTORY_HEADER_RECTS: std::sync::Mutex<Vec<(&'static str, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_DIRECTORY_HEADER_ICONS: std::sync::Mutex<Vec<(&'static str, Option<Icon>)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_ICON_MENU_LAYOUTS: std::sync::Mutex<Vec<(String, egui::Rect, egui::Rect, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_EXTENSION_ROW_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_EXTENSION_TEXT_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_EXTENSION_HEADER_RECTS: std::sync::Mutex<Vec<(&'static str, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_EXTENSION_HEADER_ICONS: std::sync::Mutex<Vec<(&'static str, Option<Icon>)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_LARGEST_ROW_RECTS: std::sync::Mutex<Vec<(usize, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_SEARCH_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
#[cfg(test)]
static TEST_DUPLICATE_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

fn sortable_header(
    ui: &mut egui::Ui,
    label: &'static str,
    direction: Option<Icon>,
) -> egui::Response {
    const ICON_SIZE: f32 = 12.0;
    const ICON_GAP: f32 = 5.0;
    let galley = egui::WidgetText::from(RichText::new(label).strong()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        TextStyle::Button,
    );
    let size = ui.available_size_before_wrap();
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let text_pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
        let color = if direction.is_some() {
            Color32::from_rgb(104, 168, 255)
        } else {
            ui.style().interact(&response).text_color()
        };
        ui.painter().galley(text_pos, galley.clone(), color);
        if let Some(icon) = direction {
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(
                    text_pos.x + galley.size().x + ICON_GAP + ICON_SIZE * 0.5,
                    rect.center().y,
                ),
                Vec2::splat(ICON_SIZE),
            );
            icon.paint(ui.painter(), icon_rect, color);
        }
    }
    #[cfg(test)]
    TEST_DIRECTORY_HEADER_ICONS
        .lock()
        .unwrap()
        .push((label, direction));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Sort by {label}"))
}

fn visible_tree_rows(app: &GuiApp) -> Vec<TreeRow> {
    let mut out = Vec::new();
    let root_name = app.tree.root_path.display().to_string();
    push_tree_rows(
        &app.tree.root,
        Vec::new(),
        0,
        app.tree.root.effective_size(app.use_physical).max(1),
        root_name,
        app,
        &mut out,
    );
    out
}

fn push_tree_rows(
    node: &Node,
    path: Vec<usize>,
    depth: usize,
    parent_size: u64,
    display_name: String,
    app: &GuiApp,
    out: &mut Vec<TreeRow>,
) {
    out.push(TreeRow {
        path: path.clone(),
        depth,
        name: display_name,
        is_dir: node.is_dir,
        size: node.effective_size(app.use_physical),
        parent_size,
        files: node.file_count,
        dirs: node.dir_count,
        modified: node.modified,
        unreadable: node.unreadable_count,
        symlink: node.is_symlink,
    });
    if !node.is_dir || !app.expanded.contains(&path) {
        return;
    }
    let mut children: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
    sort_nodes(&mut children, app.sort, app.use_physical);
    let node_size = node.effective_size(app.use_physical).max(1);
    for (idx, child) in children {
        let mut child_path = path.clone();
        child_path.push(idx);
        push_tree_rows(
            child,
            child_path,
            depth + 1,
            node_size,
            child.name.clone(),
            app,
            out,
        );
    }
}

fn sort_nodes(nodes: &mut [(usize, &Node)], sort: SortMode, physical: bool) {
    match sort {
        SortMode::SizeDesc => nodes.sort_by(|a, b| {
            b.1.effective_size(physical)
                .cmp(&a.1.effective_size(physical))
        }),
        SortMode::SizeAsc => nodes.sort_by(|a, b| {
            a.1.effective_size(physical)
                .cmp(&b.1.effective_size(physical))
        }),
        SortMode::NameAsc => nodes.sort_by_key(|a| a.1.name.to_lowercase()),
        SortMode::NameDesc => nodes.sort_by_key(|b| std::cmp::Reverse(b.1.name.to_lowercase())),
        SortMode::ModifiedDesc => nodes.sort_by_key(|b| std::cmp::Reverse(b.1.modified)),
        SortMode::ModifiedAsc => nodes.sort_by_key(|a| a.1.modified),
    }
}

fn draw_directory_tree(app: &mut GuiApp, ui: &mut egui::Ui) {
    let rows = visible_tree_rows(app);
    let total = app.tree.root.effective_size(app.use_physical).max(1);
    let mut select = None;
    let mut toggle = None;
    let mut open = None;
    let mut sort = None;
    let mut row_action: Option<(RowAction, Vec<usize>)> = None;
    let compact = ui.available_width() < 760.0;
    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .vscroll(true)
        .sense(Sense::click())
        .cell_layout(Layout::left_to_right(Align::Center))
        // The flexible column must not persist a stale drag width: it absorbs
        // every pixel left after the content-sized columns are measured.
        .column(
            Column::remainder()
                .at_least(220.0)
                .clip(true)
                .resizable(false),
        )
        .column(Column::auto().range(90.0..=125.0).clip(true));
    table = if compact {
        table.column(Column::auto().range(70.0..=100.0).clip(true))
    } else {
        table
            .column(Column::auto().range(150.0..=240.0).clip(true))
            .column(Column::auto().range(70.0..=105.0).clip(true))
            .column(Column::auto().range(60.0..=95.0).clip(true))
            .column(Column::auto().range(60.0..=95.0).clip(true))
            .column(Column::auto().range(110.0..=175.0).clip(true))
            .column(Column::auto().range(70.0..=105.0).clip(true))
    };
    table
        .header(28.0, |mut h| {
            h.col(|ui| {
                let response = sortable_header(
                    ui,
                    "Name",
                    match app.sort {
                        SortMode::NameAsc => Some(Icon::ChevronUp),
                        SortMode::NameDesc => Some(Icon::ChevronDown),
                        _ => None,
                    },
                );
                #[cfg(test)]
                TEST_DIRECTORY_HEADER_RECTS
                    .lock()
                    .unwrap()
                    .push(("Name", response.rect));
                if response.clicked() {
                    sort = Some(if app.sort == SortMode::NameAsc {
                        SortMode::NameDesc
                    } else {
                        SortMode::NameAsc
                    });
                }
            });
            h.col(|ui| {
                let response = sortable_header(
                    ui,
                    "Size",
                    match app.sort {
                        SortMode::SizeAsc => Some(Icon::ChevronUp),
                        SortMode::SizeDesc => Some(Icon::ChevronDown),
                        _ => None,
                    },
                );
                #[cfg(test)]
                TEST_DIRECTORY_HEADER_RECTS
                    .lock()
                    .unwrap()
                    .push(("Size", response.rect));
                if response.clicked() {
                    sort = Some(if app.sort == SortMode::SizeDesc {
                        SortMode::SizeAsc
                    } else {
                        SortMode::SizeDesc
                    });
                }
            });
            if compact {
                h.col(|ui| {
                    ui.strong("% total");
                });
            } else {
                h.col(|ui| {
                    ui.strong("Subtree percentage");
                });
                h.col(|ui| {
                    ui.strong("% of total");
                });
                h.col(|ui| {
                    ui.strong("Files");
                });
                h.col(|ui| {
                    ui.strong("Subdirs");
                });
                h.col(|ui| {
                    let response = sortable_header(
                        ui,
                        "Last change",
                        match app.sort {
                            SortMode::ModifiedAsc => Some(Icon::ChevronUp),
                            SortMode::ModifiedDesc => Some(Icon::ChevronDown),
                            _ => None,
                        },
                    );
                    #[cfg(test)]
                    TEST_DIRECTORY_HEADER_RECTS
                        .lock()
                        .unwrap()
                        .push(("Last change", response.rect));
                    if response.clicked() {
                        sort = Some(if app.sort == SortMode::ModifiedDesc {
                            SortMode::ModifiedAsc
                        } else {
                            SortMode::ModifiedDesc
                        });
                    }
                });
                h.col(|ui| {
                    ui.strong("Attributes");
                });
            }
        })
        .body(|body| {
            body.rows(27.0, rows.len(), |mut row| {
                let item = &rows[row.index()];
                row.set_selected(app.selected_path.as_ref() == Some(&item.path));
                row.col(|ui| {
                    ui.add_space(item.depth as f32 * 18.0);
                    if item.is_dir {
                        let expanded = app.expanded.contains(&item.path);
                        let chevron = if expanded {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        };
                        if compact_icon_button(
                            ui,
                            chevron,
                            if expanded { "Collapse" } else { "Expand" },
                        )
                        .clicked()
                        {
                            toggle = Some(item.path.clone());
                        }
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
                });
                row.col(|ui| {
                    ui.label(human_bytes(item.size));
                });
                if compact {
                    row.col(|ui| {
                        ui.label(format!("{:.1}%", item.size as f64 / total as f64 * 100.0));
                    });
                } else {
                    row.col(|ui| {
                        percentage_bar(ui, item.size as f32 / item.parent_size.max(1) as f32);
                    });
                    row.col(|ui| {
                        ui.label(format!("{:.1}%", item.size as f64 / total as f64 * 100.0));
                    });
                    row.col(|ui| {
                        ui.label(thousands(item.files));
                    });
                    row.col(|ui| {
                        ui.label(thousands(item.dirs));
                    });
                    row.col(|ui| {
                        ui.label(format_modified(item.modified));
                    });
                    row.col(|ui| {
                        ui.label(if item.unreadable > 0 {
                            "D !"
                        } else if item.symlink {
                            "L"
                        } else if item.is_dir {
                            "D"
                        } else {
                            "A"
                        });
                    });
                }
                let response = row.response();
                #[cfg(test)]
                TEST_DIRECTORY_ROW_RECTS
                    .lock()
                    .unwrap()
                    .push((item.path.clone(), response.rect));
                response.context_menu(|ui| {
                    ui.set_min_width(180.0);
                    if icon_button(ui, true, Icon::ExternalLink, "     Open").clicked() {
                        row_action = Some((RowAction::Open, item.path.clone()));
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Folder, "     Show in Explorer").clicked() {
                        row_action = Some((RowAction::Reveal, item.path.clone()));
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Copy, "     Copy path").clicked() {
                        row_action = Some((RowAction::CopyPath, item.path.clone()));
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, true, Icon::ZoomIn, "     Zoom treemap here").clicked() {
                        row_action = Some((RowAction::Zoom, item.path.clone()));
                        ui.close_menu();
                    }
                    if icon_button(ui, true, Icon::Info, "     Properties").clicked() {
                        row_action = Some((RowAction::Properties, item.path.clone()));
                        ui.close_menu();
                    }
                    ui.separator();
                    if icon_button(ui, !item.path.is_empty(), Icon::Trash, "     Delete…").clicked()
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

fn percentage_bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(180.0), 14.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(43, 47, 56));
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter()
        .rect_filled(fill, 2.0, Color32::from_rgb(66, 133, 219));
}

fn draw_extension_list(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        paint_inline_icon(ui, Icon::Extensions, 19.0, Color32::from_rgb(104, 168, 255));
        ui.heading("Extensions");
        if ui.small_button("Clear highlight").clicked() {
            app.highlighted_extension = None;
            app.highlighted_category = None;
        }
    });
    let total = app.extensions.iter().map(|e| e.size).sum::<u64>().max(1);
    let rows = app.extensions.clone();
    let mut selected = None;
    let mut sort = None;
    TableBuilder::new(ui)
        .striped(true)
        .vscroll(true)
        .resizable(true)
        .sense(Sense::click())
        .column(Column::exact(24.0))
        .column(
            Column::remainder()
                .at_least(80.0)
                .clip(true)
                .resizable(false),
        )
        .column(Column::auto().range(65.0..=120.0).clip(true))
        .column(Column::auto().range(85.0..=145.0).clip(true))
        .column(Column::auto().range(48.0..=80.0).clip(true))
        .header(26.0, |mut h| {
            h.col(|_| {});
            h.col(|ui| {
                let direction = match app.extension_sort {
                    ExtensionSortMode::ExtensionAsc => Some(Icon::ChevronUp),
                    ExtensionSortMode::ExtensionDesc => Some(Icon::ChevronDown),
                    _ => None,
                };
                let response = sortable_header(ui, "Extension", direction);
                #[cfg(test)]
                {
                    TEST_EXTENSION_HEADER_RECTS
                        .lock()
                        .unwrap()
                        .push(("Extension", response.rect));
                    TEST_EXTENSION_HEADER_ICONS
                        .lock()
                        .unwrap()
                        .push(("Extension", direction));
                }
                if response.clicked() {
                    sort = Some(if app.extension_sort == ExtensionSortMode::ExtensionAsc {
                        ExtensionSortMode::ExtensionDesc
                    } else {
                        ExtensionSortMode::ExtensionAsc
                    });
                }
            });
            h.col(|ui| {
                let direction = match app.extension_sort {
                    ExtensionSortMode::ColorAsc => Some(Icon::ChevronUp),
                    ExtensionSortMode::ColorDesc => Some(Icon::ChevronDown),
                    _ => None,
                };
                let response = sortable_header(ui, "Color", direction);
                #[cfg(test)]
                {
                    TEST_EXTENSION_HEADER_RECTS
                        .lock()
                        .unwrap()
                        .push(("Color", response.rect));
                    TEST_EXTENSION_HEADER_ICONS
                        .lock()
                        .unwrap()
                        .push(("Color", direction));
                }
                if response.clicked() {
                    sort = Some(if app.extension_sort == ExtensionSortMode::ColorAsc {
                        ExtensionSortMode::ColorDesc
                    } else {
                        ExtensionSortMode::ColorAsc
                    });
                }
            });
            h.col(|ui| {
                let direction = match app.extension_sort {
                    ExtensionSortMode::BytesAsc => Some(Icon::ChevronUp),
                    ExtensionSortMode::BytesDesc => Some(Icon::ChevronDown),
                    _ => None,
                };
                let response = sortable_header(ui, "Bytes", direction);
                #[cfg(test)]
                {
                    TEST_EXTENSION_HEADER_RECTS
                        .lock()
                        .unwrap()
                        .push(("Bytes", response.rect));
                    TEST_EXTENSION_HEADER_ICONS
                        .lock()
                        .unwrap()
                        .push(("Bytes", direction));
                }
                if response.clicked() {
                    sort = Some(if app.extension_sort == ExtensionSortMode::BytesDesc {
                        ExtensionSortMode::BytesAsc
                    } else {
                        ExtensionSortMode::BytesDesc
                    });
                }
            });
            h.col(|ui| {
                let direction = match app.extension_sort {
                    ExtensionSortMode::FilesAsc => Some(Icon::ChevronUp),
                    ExtensionSortMode::FilesDesc => Some(Icon::ChevronDown),
                    _ => None,
                };
                let response = sortable_header(ui, "Files", direction);
                #[cfg(test)]
                {
                    TEST_EXTENSION_HEADER_RECTS
                        .lock()
                        .unwrap()
                        .push(("Files", response.rect));
                    TEST_EXTENSION_HEADER_ICONS
                        .lock()
                        .unwrap()
                        .push(("Files", direction));
                }
                if response.clicked() {
                    sort = Some(if app.extension_sort == ExtensionSortMode::FilesDesc {
                        ExtensionSortMode::FilesAsc
                    } else {
                        ExtensionSortMode::FilesDesc
                    });
                }
            });
        })
        .body(|body| {
            body.rows(25.0, rows.len(), |mut row| {
                let ext = &rows[row.index()];
                row.set_selected(app.highlighted_extension.as_ref() == Some(&ext.extension));
                row.col(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
                    ui.painter()
                        .rect_filled(r, 1.0, extension_color(&ext.extension));
                });
                row.col(|ui| {
                    let _response = ui.label(&ext.extension);
                    #[cfg(test)]
                    TEST_EXTENSION_TEXT_RECTS
                        .lock()
                        .unwrap()
                        .push((ext.extension.clone(), _response.rect));
                });
                row.col(|ui| {
                    ui.label(ext.category.label());
                });
                row.col(|ui| {
                    ui.label(format!(
                        "{}  ({:.1}%)",
                        human_bytes(ext.size),
                        ext.size as f64 / total as f64 * 100.0
                    ));
                });
                row.col(|ui| {
                    ui.label(thousands(ext.count));
                });
                let response = row.response();
                #[cfg(test)]
                TEST_EXTENSION_ROW_RECTS
                    .lock()
                    .unwrap()
                    .push((ext.extension.clone(), response.rect));
                if response.clicked() {
                    selected = Some((ext.extension.clone(), ext.category));
                }
            })
        });
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

fn draw_largest_files(app: &mut GuiApp, ui: &mut egui::Ui) {
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
        .header(28.0, |mut h| {
            h.col(|ui| {
                ui.strong("Path");
            });
            h.col(|ui| {
                ui.strong("Size");
            });
            h.col(|ui| {
                ui.strong("Last change");
            });
        })
        .body(|body| {
            body.rows(27.0, app.largest_files.len(), |mut row| {
                let file = &app.largest_files[row.index()];
                let path = file.index_path.clone();
                row.set_selected(app.selected_path.as_ref() == Some(&path));
                row.col(|ui| {
                    ui.label(app.tree.path_for(&path).display().to_string());
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
                #[cfg(test)]
                TEST_LARGEST_ROW_RECTS
                    .lock()
                    .unwrap()
                    .push((row.index(), response.rect));
                if response.clicked() {
                    selected = Some(path);
                }
            })
        });
    if let Some(path) = selected {
        app.select_path(path);
    }
}

fn draw_search(app: &mut GuiApp, ui: &mut egui::Ui) {
    let mut run = false;
    ui.horizontal(|ui| {
        ui.label("Name pattern:");
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.search_query)
                .hint_text("*.iso or re:^archive-")
                .desired_width(300.0),
        );
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            run = true;
        }
        if icon_button(ui, true, Icon::Search, "     Search").clicked() {
            run = true;
        }
    });
    if run {
        app.run_search();
    }
    if let Some(error) = &app.search_error {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
    let mut selected = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for hit in &app.search_results {
            let path = hit.index_path.clone();
            let response = ui.selectable_label(
                app.selected_path.as_ref() == Some(&path),
                format!(
                    "{}    {}",
                    app.tree.path_for(&path).display(),
                    human_bytes(if app.use_physical {
                        hit.physical_size
                    } else {
                        hit.size
                    })
                ),
            );
            #[cfg(test)]
            TEST_SEARCH_ROW_RECTS
                .lock()
                .unwrap()
                .push((path.clone(), response.rect));
            if response.clicked() {
                selected = Some(path);
            }
        }
    });
    if let Some(path) = selected {
        app.select_path(path);
    }
}

fn draw_duplicates(app: &mut GuiApp, ui: &mut egui::Ui) {
    if app.duplicate_groups.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            if app.duplicate_running() {
                ui.spinner();
                ui.heading("Finding duplicate files…");
                ui.label("Files with matching sizes are being hashed in the background.");
            } else {
                paint_inline_icon(ui, Icon::Duplicate, 46.0, Color32::from_rgb(104, 168, 255));
                ui.heading("No duplicate groups found");
                ui.label("Scan the current tree for files with identical content.");
            }
            if !app.duplicate_running()
                && icon_button(ui, true, Icon::Duplicate, "     Scan for duplicates").clicked()
            {
                app.find_duplicates();
            }
        });
        return;
    }
    let mut selected = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
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
                        app.tree.path_for(&file.index_path).display().to_string(),
                    );
                    #[cfg(test)]
                    TEST_DUPLICATE_ROW_RECTS
                        .lock()
                        .unwrap()
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

fn draw_treemap(app: &mut GuiApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        paint_inline_icon(ui, Icon::App, 19.0, Color32::from_rgb(104, 168, 255));
        ui.heading("Treemap");
        ui.label(RichText::new(app.zoom_fs_path().display().to_string()).weak());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label("Drag the splitter to resize all the way down");
        });
    });
    let avail = ui.available_size();
    if avail.x <= 1.0 || avail.y <= 1.0 {
        return;
    }
    let (response, painter) = ui.allocate_painter(avail, Sense::click());
    let tiles = app.treemap_tiles(response.rect.min.x, response.rect.min.y, avail.x, avail.y);
    let mut clicked = None;
    for tile in &tiles {
        if tile.w < 1.0 || tile.h < 1.0 {
            continue;
        }
        let rect =
            egui::Rect::from_min_size(egui::pos2(tile.x, tile.y), egui::vec2(tile.w, tile.h));
        let raw = if tile.is_free_space {
            to_color32(color::free_space_color())
        } else if tile.is_dir {
            to_color32(color::directory_color())
        } else {
            extension_color(&extension_label(&tile.name))
        };
        let mut base = scale(raw, 1.0 - (tile.depth as f32 * 0.05).min(0.35));
        if !tile.is_free_space {
            if let Some(ext) = &app.highlighted_extension {
                if tile.is_dir || extension_label(&tile.name) != *ext {
                    base = blend(base, Color32::from_rgb(49, 52, 60), 0.78);
                }
            } else if let Some(category) = app.highlighted_category {
                if tile.is_dir || tile.category != Some(category) {
                    base = blend(base, Color32::from_rgb(49, 52, 60), 0.78);
                }
            }
        }
        paint_cushion_rect(&painter, rect, base);
        if app.show_grid {
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0_f32, Color32::from_rgb(14, 15, 18)),
            );
        }
        if app.selected_path.as_ref() == Some(&tile.index_path) {
            painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(3.0_f32, Color32::WHITE));
        }
        if app.show_labels && tile.can_label && tile.w >= 48.0 && tile.h >= 16.0 {
            painter.text(
                rect.min + egui::vec2(4.0, 3.0),
                egui::Align2::LEFT_TOP,
                truncate_for_width(&tile.name, tile.w - 8.0, &painter, ui),
                TextStyle::Small.resolve(ui.style()),
                if luminance(base) > 145.0 {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                },
            );
        }
        if !tile.is_free_space
            && response.clicked()
            && response
                .interact_pointer_pos()
                .is_some_and(|p| rect.contains(p))
        {
            clicked = Some(tile.index_path.clone());
        }
    }
    if let Some(path) = clicked {
        app.navigate_to_absolute(path);
    }
}

fn handle_shortcuts(app: &mut GuiApp, ctx: &egui::Context) {
    let editing_text = ctx.wants_keyboard_input();
    let (refresh_key, delete_key, open_dialog, search, copy, up, down, left, right, enter) = ctx
        .input(|i| {
            (
                i.key_pressed(egui::Key::F5),
                i.key_pressed(egui::Key::Delete),
                i.modifiers.ctrl && i.key_pressed(egui::Key::O),
                i.modifiers.ctrl && i.key_pressed(egui::Key::F),
                i.modifiers.ctrl && i.key_pressed(egui::Key::C),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Enter),
            )
        });
    if refresh_key {
        refresh(app);
    }
    if delete_key && !editing_text {
        app.request_delete_selected(false);
    }
    if open_dialog {
        choose_folder(app);
    }
    if search {
        app.file_view = FileView::SearchResults;
    }
    if copy && !editing_text {
        copy_path(app);
    }
    if !editing_text && app.file_view == FileView::AllFiles {
        let rows = visible_tree_rows(app);
        let current = app
            .selected_path
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.path == selected));
        if (up || down) && !rows.is_empty() {
            let next = if up {
                current.unwrap_or(1).saturating_sub(1)
            } else {
                (current.unwrap_or(0) + 1).min(rows.len() - 1)
            };
            app.select_path(rows[next].path.clone());
        }
        if let Some(path) = app.selected_path.clone() {
            let is_dir = app.tree.node_for(&path).is_dir;
            if right && is_dir && !app.expanded.contains(&path) {
                app.toggle_expanded(&path);
            }
            if left {
                if is_dir && app.expanded.contains(&path) {
                    app.toggle_expanded(&path);
                } else if !path.is_empty() {
                    app.select_path(path[..path.len() - 1].to_vec());
                }
            }
            if enter {
                if is_dir {
                    app.toggle_expanded(&path);
                } else {
                    open_selected(app);
                }
            }
        }
    }
}

fn choose_folder(app: &mut GuiApp) {
    if let Some(path) = rfd::FileDialog::new()
        .set_directory(&app.tree.root_path)
        .pick_folder()
    {
        if let Err(e) = app.open_folder(&path) {
            app.status = Some(format!("Scan failed: {e}"));
        }
    }
}
fn refresh(app: &mut GuiApp) {
    if let Err(e) = app.refresh_scan() {
        app.status = Some(format!("Refresh failed: {e}"));
    }
}
fn open_selected(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        if let Err(e) = crate::util::open_path(&path) {
            app.status = Some(format!("Open failed: {e}"));
        }
    }
}
fn reveal_selected(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(&path).to_path_buf()
        };
        if let Err(e) = crate::util::open_in_file_manager(&target) {
            app.status = Some(format!("Explorer failed: {e}"));
        }
    }
}
fn copy_path(app: &mut GuiApp) {
    if let Some(path) = app.selected_fs_path() {
        let text = path.display().to_string();
        match crate::util::copy_to_clipboard(&text) {
            Ok(()) => app.status = Some(format!("Copied: {text}")),
            Err(e) => app.status = Some(format!("Copy failed: {e}")),
        }
    }
}
fn export_csv(app: &mut GuiApp) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name("rustdirstat.csv")
        .save_file()
    {
        match crate::csv_export::write_csv_to_file(&app.tree.root_path, &app.tree.root, &path) {
            Ok(()) => app.status = Some(format!("Exported: {}", path.display())),
            Err(e) => app.status = Some(format!("Export failed: {e}")),
        }
    }
}

fn draw_delete_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    let Some(pending) = &app.pending_delete else {
        return;
    };
    let (name, permanent, is_dir) = (pending.name.clone(), pending.permanent, pending.is_dir);
    let mut confirm = false;
    let mut empty = false;
    let mut cancel = false;
    egui::Window::new(if permanent {
        "Permanently delete"
    } else {
        "Move to Recycle Bin"
    })
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
    .show(ctx, |ui| {
        ui.set_min_width(420.0);
        icon_heading(ui, Icon::Trash, &format!("Delete “{name}”?"));
        ui.add_space(6.0);
        if permanent {
            ui.colored_label(Color32::LIGHT_RED, "This cannot be undone.");
        } else {
            ui.label("The item can be restored from the Recycle Bin.");
        }
        if is_dir {
            ui.label("Empty keeps the folder and removes its contents.");
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui
                .button(if permanent {
                    "Delete permanently"
                } else {
                    "Move to Recycle Bin"
                })
                .clicked()
            {
                confirm = true;
            }
            if is_dir && ui.button("Empty folder").clicked() {
                empty = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if confirm {
        if let Err(e) = app.confirm_delete() {
            app.status = Some(format!("Delete failed: {e}"));
        }
    }
    if empty {
        if let Err(e) = app.confirm_empty() {
            app.status = Some(format!("Empty failed: {e}"));
        }
    }
    if cancel {
        app.pending_delete = None;
    }
}

fn draw_properties_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_properties {
        return;
    }
    let mut open = true;
    egui::Window::new("Properties")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            icon_heading(ui, Icon::Info, "Item details");
            ui.add_space(6.0);
            if let (Some(node), Some(path)) = (app.selected_node(), app.selected_fs_path()) {
                egui::Grid::new("properties_grid")
                    .spacing(Vec2::new(18.0, 8.0))
                    .show(ui, |ui| {
                        property(ui, "Name", &node.name);
                        property(ui, "Path", &path.display().to_string());
                        property(ui, "Type", if node.is_dir { "Folder" } else { "File" });
                        property(ui, "Logical size", &human_bytes(node.size));
                        property(ui, "Physical size", &human_bytes(node.physical_size));
                        property(ui, "Files", &thousands(node.file_count));
                        property(ui, "Subdirectories", &thousands(node.dir_count));
                        property(ui, "Last change", &format_modified(node.modified));
                        property(ui, "Unreadable items", &thousands(node.unreadable_count));
                    });
            } else {
                ui.label("Select an item first.");
            }
        });
    if !open {
        app.show_properties = false;
    }
}
fn property(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}

fn draw_settings_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            icon_heading(ui, Icon::Settings, "Layout");
            ui.radio_value(
                &mut app.orientation,
                PaneOrientation::Horizontal,
                "Horizontal: treemap below the lists",
            );
            ui.radio_value(
                &mut app.orientation,
                PaneOrientation::Vertical,
                "Vertical: treemap to the right",
            );
            ui.add_space(10.0);
            icon_heading(ui, Icon::Tree, "Views");
            ui.checkbox(&mut app.show_extension_view, "Show extension list");
            ui.checkbox(&mut app.show_treemap, "Show treemap");
            ui.checkbox(&mut app.show_toolbar, "Show toolbar");
            ui.checkbox(&mut app.show_status_bar, "Show status bar");
            ui.add_space(10.0);
            icon_heading(ui, Icon::App, "Treemap");
            ui.checkbox(&mut app.show_grid, "Grid lines");
            ui.checkbox(&mut app.show_labels, "File labels");
            ui.checkbox(
                &mut app.show_free_space,
                "Free-space tile for whole-drive scans",
            );
        });
    if !open {
        app.show_settings = false;
    }
}

fn draw_windows_tools_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_windows_tools {
        return;
    }
    let mut open = true;
    let mut selected = None;
    egui::Window::new("Windows maintenance")
        .open(&mut open)
        .resizable(true)
        .default_width(590.0)
        .show(ctx, |ui| {
            icon_heading(ui, Icon::Tools, "Storage and system tools");
            ui.label(
                "Launch built-in Windows maintenance tools for the volume containing this scan.",
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(440.0)
                .show(ui, |ui| {
                    for (index, tool) in crate::wintools::TOOLS.iter().enumerate() {
                        Frame::none()
                            .fill(Color32::from_rgb(30, 33, 40))
                            .rounding(egui::Rounding::same(7.0))
                            .inner_margin(Margin::same(10.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(tool.name).strong());
                                        ui.label(RichText::new(tool.description).weak());
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let label = if tool.destructive {
                                            "Review…"
                                        } else {
                                            "Launch"
                                        };
                                        if ui
                                            .add_enabled(!app.is_busy(), egui::Button::new(label))
                                            .clicked()
                                        {
                                            selected = Some(index);
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
            if !cfg!(windows) {
                ui.colored_label(
                    Color32::LIGHT_YELLOW,
                    "These tools are available on Windows only.",
                );
            }
        });
    if let Some(index) = selected {
        app.request_windows_tool(index);
    }
    if !open {
        app.show_windows_tools = false;
    }
}

fn draw_windows_tool_confirmation(app: &mut GuiApp, ctx: &egui::Context) {
    let Some(index) = app.pending_windows_tool else {
        return;
    };
    let Some(tool) = crate::wintools::TOOLS.get(index) else {
        app.pending_windows_tool = None;
        return;
    };
    let (name, description) = (tool.name, tool.description);
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new("Confirm maintenance action")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(470.0);
            icon_heading(ui, Icon::Trash, name);
            ui.label(description);
            ui.add_space(8.0);
            ui.colored_label(
                Color32::LIGHT_RED,
                "This operation may remove data and cannot be undone.",
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Run action").clicked() {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if confirm {
        app.confirm_windows_tool();
    }
    if cancel {
        app.pending_windows_tool = None;
    }
}

fn draw_about_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }
    let mut open = true;
    egui::Window::new("WinDirStat views in RustDirStat")
        .open(&mut open)
        .resizable(true)
        .default_width(620.0)
        .show(ctx, |ui| {
            icon_heading(ui, Icon::Help, "Three coupled core views");
            ui.label("All Files is the expandable, size-sorted directory tree. Selecting an item frames the same item in the treemap.");
            ui.label("Extensions groups files by exact extension, with color, bytes, percentage, and count. Selecting a row highlights matching treemap files.");
            ui.label("Treemap represents every file as an area proportional to size, nests directory areas, uses extension colors, and selects the matching tree path when clicked.");
            ui.add_space(10.0);
            icon_heading(ui, Icon::Largest, "Additional file views");
            ui.label("Largest Files is a flat top-200 size view. Duplicate Files groups byte-identical files by hash. Search Results supports glob patterns and re: regular expressions.");
            ui.add_space(10.0);
            ui.label("All splitters are resizable to zero. The toolbar or Treemap menu switches between below/right orientations.");
        });
    if !open {
        app.show_about = false;
    }
}

fn extension_color(extension: &str) -> Color32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in extension.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hsv_to_rgb((hash % 360) as f32, 0.68, 0.88)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Color32 {
    let c = value * saturation;
    let x = c * (1.0 - (((hue / 60.0) % 2.0) - 1.0).abs());
    let m = value - c;
    let (r, g, b) = match hue {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color32::from_rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
fn to_color32(c: ratatui::style::Color) -> Color32 {
    if let ratatui::style::Color::Rgb(r, g, b) = c {
        Color32::from_rgb(r, g, b)
    } else {
        Color32::GRAY
    }
}
fn truncate_for_width(name: &str, max_w: f32, painter: &egui::Painter, ui: &egui::Ui) -> String {
    let font = TextStyle::Small.resolve(ui.style());
    if painter
        .layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
        .rect
        .width()
        <= max_w
    {
        return name.to_string();
    }
    let mut s = name.to_string();
    while !s.is_empty() {
        s.pop();
        let candidate = format!("{s}…");
        if painter
            .layout_no_wrap(candidate.clone(), font.clone(), Color32::WHITE)
            .rect
            .width()
            <= max_w
        {
            return candidate;
        }
    }
    String::new()
}
fn luminance(c: Color32) -> f32 {
    0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32
}
fn scale(c: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.5);
    Color32::from_rgb(
        ((c.r() as f32) * f).min(255.0) as u8,
        ((c.g() as f32) * f).min(255.0) as u8,
        ((c.b() as f32) * f).min(255.0) as u8,
    )
}
fn blend(c: Color32, target: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t) as u8;
    Color32::from_rgb(
        m(c.r(), target.r()),
        m(c.g(), target.g()),
        m(c.b(), target.b()),
    )
}
fn paint_cushion_rect(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    const STEPS: i32 = 12;
    let step = (rect.height() / STEPS as f32).max(1.0);
    for i in 0..STEPS {
        let t = i as f32 / STEPS as f32;
        let y0 = rect.min.y + t * rect.height();
        let y1 = (y0 + step).min(rect.max.y);
        if y1 <= y0 {
            continue;
        }
        let shade = if t < 0.5 {
            blend(base, Color32::WHITE, (0.5 - t) * 0.5)
        } else {
            blend(base, Color32::BLACK, (t - 0.5) * 0.6)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.min.x, y0), egui::pos2(rect.max.x, y1)),
            0.0,
            shade,
        );
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::color::Category;
    use crate::gui::app::ExtensionRow;
    use crate::model::{Node, Tree};
    use std::path::PathBuf;

    static TEST_UI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let index = child.category.unwrap().index();
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
            let index = child.category.unwrap().index();
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

    fn click_directory_header(ctx: &egui::Context, app: &mut GuiApp, label: &str) {
        TEST_DIRECTORY_HEADER_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_directory(ctx, app, raw_input(Vec::new()));
        }
        let position = TEST_DIRECTORY_HEADER_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(header, _)| *header == label)
            .map(|(_, rect)| egui::pos2(rect.right() - 4.0, rect.center().y))
            .unwrap_or_else(|| panic!("the rendered {label} header should expose a click target"));
        render_directory(ctx, app, raw_input(pointer_button(position, true)));
        render_directory(ctx, app, raw_input(pointer_button(position, false)));
    }

    fn rendered_child_order(ctx: &egui::Context, app: &mut GuiApp) -> Vec<usize> {
        TEST_DIRECTORY_ROW_RECTS.lock().unwrap().clear();
        render_directory(ctx, app, raw_input(Vec::new()));
        TEST_DIRECTORY_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(path, _)| path.first().copied())
            .collect()
    }

    fn latest_header_icon(label: &str) -> Option<Icon> {
        TEST_DIRECTORY_HEADER_ICONS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(header, _)| *header == label)
            .and_then(|(_, icon)| *icon)
    }

    fn click_extension_header(ctx: &egui::Context, app: &mut GuiApp, label: &str) {
        TEST_EXTENSION_HEADER_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_extensions(ctx, app, raw_input(Vec::new()));
        }
        let position = TEST_EXTENSION_HEADER_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(header, _)| *header == label)
            .map(|(_, rect)| egui::pos2(rect.right() - 4.0, rect.center().y))
            .unwrap_or_else(|| panic!("the rendered {label} header should expose a click target"));
        render_extensions(ctx, app, raw_input(pointer_button(position, true)));
        render_extensions(ctx, app, raw_input(pointer_button(position, false)));
    }

    fn rendered_extension_order(ctx: &egui::Context, app: &mut GuiApp) -> Vec<String> {
        TEST_EXTENSION_ROW_RECTS.lock().unwrap().clear();
        render_extensions(ctx, app, raw_input(Vec::new()));
        TEST_EXTENSION_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .map(|(extension, _)| extension.clone())
            .collect()
    }

    fn latest_extension_header_icon(label: &str) -> Option<Icon> {
        TEST_EXTENSION_HEADER_ICONS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(header, _)| *header == label)
            .and_then(|(_, icon)| *icon)
    }

    fn assert_extension_header_click(
        ctx: &egui::Context,
        app: &mut GuiApp,
        label: &str,
        expected_mode: ExtensionSortMode,
        expected_order: &[&str],
        expected_icon: Icon,
    ) {
        click_extension_header(ctx, app, label);
        assert_eq!(app.extension_sort, expected_mode);
        assert_eq!(rendered_extension_order(ctx, app), expected_order);
        assert_eq!(latest_extension_header_icon(label), Some(expected_icon));
    }

    #[test]
    fn clicking_directory_headers_changes_and_toggles_sort_order() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_sortable_files();

        render_directory(&ctx, &mut app, raw_input(Vec::new()));
        assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronDown));
        assert_eq!(latest_header_icon("Name"), None);
        assert_eq!(latest_header_icon("Last change"), None);

        click_directory_header(&ctx, &mut app, "Name");
        assert!(matches!(app.sort, SortMode::NameAsc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
        assert_eq!(latest_header_icon("Name"), Some(Icon::ChevronUp));
        assert_eq!(latest_header_icon("Size"), None);
        click_directory_header(&ctx, &mut app, "Name");
        assert!(matches!(app.sort, SortMode::NameDesc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
        assert_eq!(latest_header_icon("Name"), Some(Icon::ChevronDown));

        click_directory_header(&ctx, &mut app, "Size");
        assert!(matches!(app.sort, SortMode::SizeDesc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
        assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronDown));
        assert_eq!(latest_header_icon("Name"), None);
        click_directory_header(&ctx, &mut app, "Size");
        assert!(matches!(app.sort, SortMode::SizeAsc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
        assert_eq!(latest_header_icon("Size"), Some(Icon::ChevronUp));

        click_directory_header(&ctx, &mut app, "Last change");
        assert!(matches!(app.sort, SortMode::ModifiedDesc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![1, 2, 0]);
        assert_eq!(latest_header_icon("Last change"), Some(Icon::ChevronDown));
        assert_eq!(latest_header_icon("Size"), None);
        click_directory_header(&ctx, &mut app, "Last change");
        assert!(matches!(app.sort, SortMode::ModifiedAsc));
        assert_eq!(rendered_child_order(&ctx, &mut app), vec![0, 2, 1]);
        assert_eq!(latest_header_icon("Last change"), Some(Icon::ChevronUp));
    }

    #[test]
    fn extension_headers_sort_rendered_rows_and_show_direction() {
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

        assert_extension_header_click(
            &ctx,
            &mut app,
            "Extension",
            ExtensionSortMode::ExtensionAsc,
            &[".aaa", ".mmm", ".zzz"],
            Icon::ChevronUp,
        );
        assert_eq!(latest_extension_header_icon("Bytes"), None);
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Extension",
            ExtensionSortMode::ExtensionDesc,
            &[".zzz", ".mmm", ".aaa"],
            Icon::ChevronDown,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Color",
            ExtensionSortMode::ColorAsc,
            &[".mmm", ".aaa", ".zzz"],
            Icon::ChevronUp,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Color",
            ExtensionSortMode::ColorDesc,
            &[".zzz", ".aaa", ".mmm"],
            Icon::ChevronDown,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Bytes",
            ExtensionSortMode::BytesDesc,
            &[".zzz", ".mmm", ".aaa"],
            Icon::ChevronDown,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Bytes",
            ExtensionSortMode::BytesAsc,
            &[".aaa", ".mmm", ".zzz"],
            Icon::ChevronUp,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Files",
            ExtensionSortMode::FilesDesc,
            &[".aaa", ".mmm", ".zzz"],
            Icon::ChevronDown,
        );
        assert_extension_header_click(
            &ctx,
            &mut app,
            "Files",
            ExtensionSortMode::FilesAsc,
            &[".zzz", ".mmm", ".aaa"],
            Icon::ChevronUp,
        );
    }

    #[test]
    fn application_labels_do_not_capture_text_selection() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        apply_style(&ctx);
        assert!(!ctx.style().interaction.selectable_labels);
    }

    #[test]
    fn menu_icons_never_overlap_their_labels() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        TEST_ICON_MENU_LAYOUTS.lock().unwrap().clear();
        let _ = ctx.run(raw_input(Vec::new()), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                icon_selectable_label(ui, true, Icon::Tree, "All files");
                icon_button(ui, true, Icon::Settings, "     Settings…");
                icon_button(ui, false, Icon::Duplicate, "     Duplicate Files");
            });
        });

        let layouts = TEST_ICON_MENU_LAYOUTS.lock().unwrap();
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
    fn clicking_a_rendered_directory_row_changes_selection() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        TEST_DIRECTORY_ROW_RECTS.lock().unwrap().clear();
        // Table column widths settle over the first few immediate-mode
        // frames, just as they do while the native window is opening.
        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input(Vec::new()));
        }

        let child_row = TEST_DIRECTORY_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(path, _)| path == &[0])
            .map(|(_, rect)| egui::pos2(rect.center().x, rect.max.y - 3.0))
            .expect("the rendered child row should expose a response rectangle");
        render_directory(&ctx, &mut app, raw_input(pointer_button(child_row, true)));
        render_directory(&ctx, &mut app, raw_input(pointer_button(child_row, false)));

        assert_eq!(
            app.selected_path,
            Some(vec![0]),
            "row states: {:?}",
            *TEST_DIRECTORY_ROW_RECTS.lock().unwrap()
        );
    }

    #[test]
    fn directory_table_expands_to_fill_a_wider_pane() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        TEST_DIRECTORY_ROW_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 900.0));
        }
        let narrow_width = TEST_DIRECTORY_ROW_RECTS
            .lock()
            .unwrap()
            .last()
            .expect("directory row should render")
            .1
            .width();

        for _ in 0..4 {
            render_directory(&ctx, &mut app, raw_input_at_width(Vec::new(), 1500.0));
        }
        let wide_width = TEST_DIRECTORY_ROW_RECTS
            .lock()
            .unwrap()
            .last()
            .expect("directory row should render after resize")
            .1
            .width();

        assert!(
            wide_width > narrow_width + 500.0,
            "table should absorb pane growth: narrow={narrow_width}, wide={wide_width}, screen={}",
            ctx.screen_rect().width()
        );
    }

    #[test]
    fn clicking_a_rendered_extension_row_changes_highlight() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        TEST_EXTENSION_ROW_RECTS.lock().unwrap().clear();
        TEST_EXTENSION_TEXT_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_extensions(&ctx, &mut app, raw_input(Vec::new()));
        }
        let row_pos = TEST_EXTENSION_TEXT_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(extension, _)| extension == ".txt")
            .map(|(_, rect)| rect.center())
            .expect("the extension text should expose its rendered rectangle");
        render_extensions(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
        render_extensions(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
        assert_eq!(app.highlighted_extension.as_deref(), Some(".txt"));
    }

    #[test]
    fn clicking_a_rendered_largest_file_row_changes_selection() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        TEST_LARGEST_ROW_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_largest(&ctx, &mut app, raw_input(Vec::new()));
        }
        let row_pos = TEST_LARGEST_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(index, _)| *index == 0)
            .map(|(_, rect)| egui::pos2(rect.left() + 45.0, rect.center().y))
            .expect("the largest-file row should render");
        render_largest(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
        render_largest(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
        assert_eq!(app.selected_path, Some(vec![0]));
    }

    #[test]
    fn clicking_a_rendered_search_result_changes_selection() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        app.search_query = "*".to_string();
        app.run_search();
        TEST_SEARCH_ROW_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_search(&ctx, &mut app, raw_input(Vec::new()));
        }
        let row_pos = TEST_SEARCH_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(path, _)| path == &[0])
            .map(|(_, rect)| rect.center())
            .expect("the search result should render");
        render_search(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
        render_search(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
        assert_eq!(app.selected_path, Some(vec![0]));
    }

    #[test]
    fn clicking_a_rendered_duplicate_member_changes_selection() {
        let _test_guard = TEST_UI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut app = app_with_one_file();
        app.duplicate_groups = vec![crate::duplicates::DupGroup {
            size: 128,
            files: vec![crate::duplicates::DupFile {
                index_path: vec![0],
            }],
        }];
        TEST_DUPLICATE_ROW_RECTS.lock().unwrap().clear();
        for _ in 0..4 {
            render_duplicates(&ctx, &mut app, raw_input(Vec::new()));
        }
        let row_pos = TEST_DUPLICATE_ROW_RECTS
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(path, _)| path == &[0])
            .map(|(_, rect)| rect.center())
            .expect("the duplicate member should render");
        render_duplicates(&ctx, &mut app, raw_input(pointer_button(row_pos, true)));
        render_duplicates(&ctx, &mut app, raw_input(pointer_button(row_pos, false)));
        assert_eq!(app.selected_path, Some(vec![0]));
    }
}
