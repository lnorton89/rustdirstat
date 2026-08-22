// ============================================================================
// Module:       gui::ui::chrome
// Description:  The frame around the workspace: menu bar, toolbar, and status
//               bar.
//
// Dependencies: eframe::egui; crate::gui::app::GuiApp, super::{actions, theme,
//               widgets}
// ============================================================================

//! The frame around the workspace: menu bar, toolbar, status bar.

use super::modal::ModalPage;
use crate::gui::app::{size_label, FileView, GuiApp, PaneOrientation};
use crate::gui::icons::Icon;
use crate::util::thousands;
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Vec2};

use crate::i18n::tr;

use super::actions::*;
use super::theme::*;
use super::widgets::*;

/// Padding inside each top-level menu name, and the gap between them.
///
/// The bar is the tightest strip in the window, and egui's own menu style
/// is built for a compact look that leaves the names running into each
/// other. `menu_bar_names_are_clearly_separated` pins the resulting
/// on-screen gap so this cannot silently regress again.
pub(super) const MENU_BAR_BUTTON_PADDING: Vec2 = Vec2::new(12.0, 9.0);
pub(super) const MENU_BAR_ITEM_GAP: f32 = 10.0;

/// A menu bar's highlight is square and fills the bar.
///
/// The rest of the window rounds its widgets by 6, which under a
/// top-level menu name paints a pill floating in the middle of the bar —
/// it reads as a button someone dropped there rather than as the bar
/// responding. Squaring it, and letting it run the full height of the
/// strip, is what every desktop menu bar does and is why they read as
/// bars.
const MENU_BAR_ROUNDING: egui::CornerRadius = egui::CornerRadius::ZERO;
/// Floors the test measures against. Deliberately below what is
/// configured above, so ordinary tuning does not trip them, but far above
/// what egui's `set_menu_style` leaves behind (2px padding, 0 gap) — the
/// state this whole arrangement exists to prevent.
#[cfg(test)]
pub(super) const MENU_BAR_MIN_SIDE_PADDING: f32 = 8.0;
#[cfg(test)]
pub(super) const MENU_BAR_MIN_GAP: f32 = 6.0;

/// A top-level menu name, recording where it landed so the layout test
/// can measure the real gaps rather than trusting the spacing settings to
/// have survived egui's own menu styling.
fn menu_bar_button<R>(
    ui: &mut egui::Ui,
    label: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    #[cfg(test)]
    super::probes::probe(&super::probes::TEST_MENU_BAR_ROUNDING).push((
        label.to_owned(),
        ui.visuals().widgets.hovered.corner_radius.nw,
    ));
    let response = ui.menu_button(label, add_contents);
    #[cfg(test)]
    super::probes::probe(&super::probes::TEST_MENU_BAR_RECTS)
        .push((label.to_owned(), response.response.rect));
    response
}

/// The bar's own style: egui's menu defaults, then what this bar needs on
/// top of them.
///
/// It is handed to [`egui::containers::menu::MenuBar::style`] rather than
/// applied to the `Ui` inside the bar, because the bar applies its style
/// modifier *after* entering its own scope — the same ordering problem the
/// old `set_menu_style` had, solved by the API instead of worked around.
/// Note that a modifier replaces the default rather than adding to it,
/// which is why this calls `menu_style` first: without that line the bar
/// silently loses egui's own menu defaults.
fn menu_bar_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = MENU_BAR_BUTTON_PADDING;
    style.spacing.item_spacing.x = MENU_BAR_ITEM_GAP;
    let widgets = &mut style.visuals.widgets;
    for state in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = MENU_BAR_ROUNDING;
    }
}

/// How a menu answers a click.
///
/// egui 0.32 made `CloseOnClick` the default, which would close the menu
/// under a size choice or a view toggle — the three `menu_toggle` rows and
/// the logical/physical pair are deliberately flippable several at a time,
/// and every row that *should* dismiss the menu says so with `ui.close()`.
/// So the bar asks for the older behaviour explicitly rather than
/// inheriting a default that contradicts the menus it holds.
fn menu_config() -> egui::containers::menu::MenuConfig {
    egui::containers::menu::MenuConfig::new()
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

pub(super) fn draw_menu_bar(app: &mut GuiApp, ui: &mut egui::Ui) {
    egui::Panel::top("menu_bar")
        .frame(
            Frame::NONE
                .fill(palette().app)
                // No vertical margin: the highlight under a menu name is
                // the button's own background, so anything the frame adds
                // above and below shows as a gap the highlight cannot
                // reach — which is what made it look like a floating pill
                // instead of part of the bar. The height comes from the
                // button padding below instead.
                // No stroke either: since egui 0.31 a frame's stroke
                // is padding, which would inset the bar's content and
                // put the gap back. The rule under the bar is the
                // panel's own separator line.
                .inner_margin(Margin::symmetric(px(SPACE_XS), 0)),
        )
        .show(ui, |ui| {
            let ctx = ui.ctx().clone();
            egui::containers::menu::MenuBar::new()
                .style(menu_bar_style)
                .config(menu_config())
                .ui(ui, |ui| {
                    menu_bar_button(ui, &tr("menu.file"), |ui| {
                        if menu_action(
                            ui,
                            !app.is_busy(),
                            Icon::FolderOpen,
                            &tr("menu.file.select_folder"),
                            "Ctrl+O",
                        )
                        .clicked()
                        {
                            choose_folder(app);
                            ui.close();
                        }
                        if menu_action(
                            ui,
                            !app.is_busy(),
                            Icon::Refresh,
                            &tr("menu.file.rescan"),
                            "F5",
                        )
                        .clicked()
                        {
                            refresh(app);
                            ui.close();
                        }
                        ui.separator();
                        if icon_button(ui, true, Icon::Export, &tr("menu.file.export_csv"))
                            .clicked()
                        {
                            export_csv(app);
                            ui.close();
                        }
                        ui.separator();
                        if icon_button(ui, true, Icon::ExternalLink, &tr("menu.file.exit"))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    menu_bar_button(ui, &tr("menu.edit"), |ui| {
                        if menu_action(
                            ui,
                            app.selected_path.is_some(),
                            Icon::Copy,
                            &tr("menu.edit.copy_path"),
                            "Ctrl+C",
                        )
                        .clicked()
                        {
                            copy_path(app);
                            ui.close();
                        }
                        if menu_action(ui, true, Icon::Search, &tr("menu.edit.search"), "Ctrl+F")
                            .clicked()
                        {
                            app.file_view = FileView::SearchResults;
                            ui.close();
                        }
                    });
                    menu_bar_button(ui, &tr("menu.cleanup"), |ui| {
                        let selected = app.selected_path.is_some();
                        if icon_button(ui, selected, Icon::ExternalLink, &tr("menu.cleanup.open"))
                            .clicked()
                        {
                            open_selected(app);
                            ui.close();
                        }
                        if icon_button(ui, selected, Icon::Folder, &tr("menu.cleanup.reveal"))
                            .clicked()
                        {
                            reveal_selected(app);
                            ui.close();
                        }
                        if icon_button(ui, selected, Icon::Info, &tr("menu.cleanup.properties"))
                            .clicked()
                        {
                            app.toggle_properties();
                            ui.close();
                        }
                        ui.separator();
                        if menu_action(ui, selected, Icon::Trash, &tr("menu.cleanup.delete"), "Del")
                            .clicked()
                        {
                            app.request_delete_selected(false);
                            ui.close();
                        }
                        if menu_action(
                            ui,
                            selected,
                            Icon::Trash,
                            &tr("menu.cleanup.delete_permanent"),
                            "Shift+Del",
                        )
                        .clicked()
                        {
                            app.request_delete_selected(true);
                            ui.close();
                        }
                    });
                    menu_bar_button(ui, &tr("menu.treemap"), |ui| {
                        if menu_choice(
                            ui,
                            app.view.orientation == PaneOrientation::Horizontal,
                            &tr("menu.treemap.horizontal"),
                        )
                        .clicked()
                        {
                            app.view.orientation = PaneOrientation::Horizontal;
                            ui.close();
                        }
                        if menu_choice(
                            ui,
                            app.view.orientation == PaneOrientation::Vertical,
                            &tr("menu.treemap.vertical"),
                        )
                        .clicked()
                        {
                            app.view.orientation = PaneOrientation::Vertical;
                            ui.close();
                        }
                        ui.separator();
                        if menu_choice(ui, !app.use_physical, &tr("menu.treemap.logical")).clicked()
                        {
                            app.use_physical = false;
                            app.refresh_extensions();
                        }
                        if menu_choice(ui, app.use_physical, &tr("menu.treemap.physical")).clicked()
                        {
                            app.use_physical = true;
                            app.refresh_extensions();
                        }
                        ui.separator();
                        menu_toggle(ui, &mut app.view.grid, &tr("menu.treemap.grid"));
                        menu_toggle(ui, &mut app.view.labels, &tr("menu.treemap.labels"));
                        menu_toggle(ui, &mut app.view.free_space, &tr("menu.treemap.free_space"));
                        ui.separator();
                        if menu_action(ui, true, Icon::ZoomIn, &tr("menu.treemap.zoom_in"), "+")
                            .clicked()
                        {
                            app.zoom_in();
                            ui.close();
                        }
                        if menu_action(ui, true, Icon::ZoomOut, &tr("menu.treemap.zoom_out"), "-")
                            .clicked()
                        {
                            app.zoom_out();
                            ui.close();
                        }
                        if menu_action(ui, true, Icon::Home, &tr("menu.treemap.reset_zoom"), "Home")
                            .clicked()
                        {
                            app.reset_zoom();
                            ui.close();
                        }
                    });
                    menu_bar_button(ui, &tr("menu.view"), |ui| {
                        view_menu_item(app, ui, FileView::AllFiles);
                        view_menu_item(app, ui, FileView::LargestFiles);
                        if icon_button(
                            ui,
                            !app.is_busy(),
                            Icon::Duplicate,
                            &tr("view.duplicate_files"),
                        )
                        .clicked()
                        {
                            app.find_duplicates();
                            ui.close();
                        }
                        view_menu_item(app, ui, FileView::SearchResults);
                        ui.separator();
                        menu_toggle(
                            ui,
                            &mut app.view.extension_pane,
                            &tr("menu.view.extensions"),
                        );
                        menu_toggle(ui, &mut app.view.treemap, &tr("menu.view.treemap"));
                        menu_toggle(ui, &mut app.view.toolbar, &tr("menu.view.toolbar"));
                        menu_toggle(ui, &mut app.view.status_bar, &tr("menu.view.status_bar"));
                        ui.separator();
                        if icon_button(ui, true, Icon::Palette, &tr("menu.view.appearance"))
                            .clicked()
                        {
                            app.open_modal(ModalPage::Appearance);
                            ui.close();
                        }
                        if icon_button(ui, true, Icon::Settings, &tr("menu.view.settings"))
                            .clicked()
                        {
                            app.open_modal(ModalPage::Views);
                            ui.close();
                        }
                    });
                    menu_bar_button(ui, &tr("menu.tools"), |ui| {
                        if icon_button(ui, true, Icon::Tools, &tr("menu.tools.maintenance"))
                            .clicked()
                        {
                            app.open_modal(ModalPage::Maintenance);
                            ui.close();
                        }
                        ui.separator();
                        if icon_button(ui, true, Icon::Duplicate, &tr("menu.tools.duplicates"))
                            .clicked()
                        {
                            app.find_duplicates();
                            ui.close();
                        }
                    });
                    menu_bar_button(ui, &tr("menu.help"), |ui| {
                        // These used to open the same window as each other.
                        if icon_button(ui, true, Icon::Help, &tr("menu.help.guide")).clicked() {
                            app.open_modal(ModalPage::Guide);
                            ui.close();
                        }
                        if icon_button(ui, true, Icon::Info, &tr("menu.help.about")).clicked() {
                            app.open_modal(ModalPage::About);
                            ui.close();
                        }
                    });
                });
        });
}

pub(super) fn view_menu_item(app: &mut GuiApp, ui: &mut egui::Ui, view: FileView) {
    if icon_selectable_label(ui, app.file_view == view, view_icon(view), &view.label()).clicked() {
        app.file_view = view;
        ui.close();
    }
}

pub(super) fn draw_toolbar(app: &mut GuiApp, ui: &mut egui::Ui) {
    egui::Panel::top("toolbar")
        .frame(
            Frame::NONE
                .fill(palette().panel)
                .inner_margin(Margin::symmetric(px(PAD), px(SPACE_SM))),
        )
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Toolbar buttons are icon-only, so they need more air
                // between them than text controls do to stay readable as
                // separate targets rather than one strip of glyphs.
                ui.spacing_mut().item_spacing = Vec2::new(6.0, SPACE_SM);
                paint_inline_brand(ui, 20.0);
                ui.add_space(SPACE_XS);
                ui.label(RichText::new("RustDirStat").strong().size(15.0));
                toolbar_separator(ui);
                if tool_enabled(ui, !app.is_busy(), Icon::FolderOpen, "Select folder").clicked() {
                    choose_folder(app);
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Refresh, "Rescan (F5)").clicked() {
                    refresh(app);
                }
                toolbar_separator(ui);
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
                    app.toggle_properties();
                }
                toolbar_separator(ui);
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
                toolbar_separator(ui);
                let orient = if app.view.orientation == PaneOrientation::Horizontal {
                    Icon::LayoutHorizontal
                } else {
                    Icon::LayoutVertical
                };
                if tool(
                    ui,
                    orient,
                    &format!("Layout: {} (click to switch)", app.view.orientation.label()),
                )
                .clicked()
                {
                    app.view.orientation.toggle();
                }
                if tool(ui, Icon::Palette, "Appearance and theme").clicked() {
                    app.open_modal(ModalPage::Appearance);
                }
                if tool(ui, Icon::Settings, "Settings").clicked() {
                    app.open_modal(ModalPage::Views);
                }
                if tool_enabled(ui, !app.is_busy(), Icon::Tools, "Windows maintenance tools")
                    .clicked()
                {
                    app.open_modal(ModalPage::Maintenance);
                }
                if ui.available_width() > 240.0 {
                    toolbar_separator(ui);
                    Frame::NONE
                        .fill(palette().app)
                        .corner_radius(egui::CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(px(SPACE_SM), px(SPACE_XS)))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(crate::util::display_name(&app.tree.root_path))
                                    .color(palette().secondary_text),
                            );
                        });
                }
                // The four-way view selector sits here, right of the
                // current folder, instead of on its own strip at the top
                // of the file area. It is the same control; only the
                // click handling for Duplicate Files has to travel with
                // it, because that tab still starts a scan on first use.
                toolbar_separator(ui);
                for view in [
                    FileView::AllFiles,
                    FileView::LargestFiles,
                    FileView::DuplicateFiles,
                    FileView::SearchResults,
                ] {
                    if view_tab(ui, app.file_view == view, view).clicked() {
                        if view == FileView::DuplicateFiles && app.duplicate_groups.is_empty() {
                            app.find_duplicates();
                        } else {
                            app.file_view = view;
                        }
                    }
                }
            });
        });
}

/// A toolbar group divider. The extra air on either side is what makes
/// the icon-only buttons read as a few labelled groups rather than one
/// undifferentiated row of glyphs; a bare `ui.separator()` inherits the
/// same tight item spacing as the buttons and does not separate anything.
pub(super) fn toolbar_separator(ui: &mut egui::Ui) {
    ui.add_space(SPACE_XS);
    ui.separator();
    ui.add_space(SPACE_XS);
}

pub(super) fn draw_status_bar(app: &mut GuiApp, ui: &mut egui::Ui) {
    egui::Panel::bottom("status_bar")
        .frame(
            Frame::NONE
                .fill(palette().app)
                .inner_margin(Margin::symmetric(px(PAD), px(SPACE_XS))),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if app.is_busy() {
                    ui.spinner();
                } else {
                    paint_inline_icon(ui, Icon::Info, 14.0, palette().secondary_text);
                }
                ui.label(
                    app.busy_text()
                        .as_deref()
                        .or(app.status.as_deref())
                        .unwrap_or(&tr("status.ready")),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let node = app.zoom_node();
                    ui.label(crate::i18n::tr_with(
                        "status.scan_counts",
                        &[
                            ("files", &thousands(node.file_count)),
                            ("folders", &thousands(node.dir_count)),
                            (
                                "size",
                                &size_label(
                                    node.effective_size(app.use_physical),
                                    app.use_physical,
                                ),
                            ),
                        ],
                    ));
                    // How much of that total is the same bytes under two
                    // names. Shown rather than silently subtracted: the
                    // rows above add up to the figure beside it, and a
                    // total that disagreed with them would be its own
                    // kind of wrong. Only when there is something to say
                    // — on a volume with no hard links this is zero, and
                    // a permanent "0 B in hard links" is noise.
                    if let Some(shared) = app.hard_link_bytes() {
                        ui.label(
                            RichText::new(format!("· {} shared by hard links", size_label(shared, app.use_physical)))
                                .color(palette().secondary_text),
                        )
                        .on_hover_text(
                            "Counted once per name, the way the rows above do. This is how                              much of the total is the same bytes reached through more than                              one of them.",
                        );
                    }
                });
            });
        });
}
