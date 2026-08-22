// ============================================================================
// Module:       gui::ui::pages
// Description:  The contents of each modal page, and the two destructive-
//               action confirmations that layer above them.
//
// Dependencies: eframe::egui; crate::gui::app::GuiApp, super::{modal, themes,
//               widgets}
// ============================================================================

//! The contents of each modal page, and the two confirmations that
//! layer above them.
//!
//! Every one of these used to be its own `egui::Window` with its own
//! title bar, its own `show_*` flag, and its own idea of how wide it
//! should be. They are pages of one card now — see [`super::modal`] for
//! the shell they are drawn into, and for why they can link to each
//! other.
//!
//! Nothing here creates a window, sizes itself, or scrolls: the shell
//! owns all three. A page draws into the `Ui` it is handed, top to
//! bottom, and lets the card's scroll area worry about whether it fits.

use crate::gui::app::{GuiApp, PaneOrientation};
use crate::gui::icons::Icon;
use crate::util::{human_bytes, thousands};
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Stroke, Vec2};

use super::modal::{
    callout, confirm_card, fill_width, page_link, separator, ConfirmKind, ModalPage, Tone,
};
#[cfg(test)]
use super::probes::*;
use super::theme::*;
use super::themes;
use super::widgets::*;

pub(super) fn draw_page(app: &mut GuiApp, ui: &mut egui::Ui, page: ModalPage) {
    match page {
        ModalPage::Locations => draw_locations(app, ui),
        ModalPage::Appearance => draw_appearance(app, ui),
        ModalPage::Layout => draw_layout(app, ui),
        ModalPage::Views => draw_views(app, ui),
        ModalPage::Cleanups => draw_cleanups(app, ui),
        ModalPage::Maintenance => draw_maintenance(app, ui),
        ModalPage::Guide => draw_guide(ui),
        ModalPage::About => draw_about(app, ui),
    }
}

/// A titled block of related controls.
fn group(ui: &mut egui::Ui, icon: Icon, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let palette = palette();
    Frame::NONE
        .fill(palette.raised)
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(Margin::same(px(SPACE_MD)))
        .stroke(Stroke::new(1.0_f32, palette.border))
        .show(ui, |ui| {
            fill_width(ui);
            ui.horizontal(|ui| {
                paint_inline_icon(ui, icon, 16.0, palette.accent);
                ui.label(RichText::new(title).strong());
            });
            ui.add_space(SPACE_SM);
            add_contents(ui);
        });
    ui.add_space(SPACE_MD);
}

/// A sentence that ends by sending the reader to another page.
fn see_also(ui: &mut egui::Ui, app: &mut GuiApp, lead: &str, page: ModalPage) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(lead).color(palette().secondary_text));
        if page_link(ui, page) {
            app.modal = Some(page);
        }
        ui.label(RichText::new(".").color(palette().secondary_text));
    });
}

// ------------------------------------------------------------ Appearance

/// Visible height of the theme list before it scrolls on its own.
const THEME_LIST_HEIGHT: f32 = 330.0;

fn draw_appearance(app: &mut GuiApp, ui: &mut egui::Ui) {
    let mut chosen: Option<&'static str> = None;
    group(ui, Icon::Palette, "Theme", |ui| {
        ui.label(
            RichText::new(
                "Applies immediately. Drop a .toml file into the themes folder \
                 beside the config file to add your own.",
            )
            .color(palette().secondary_text),
        );
        ui.add_space(SPACE_MD);
        // Its own scroll, bounded. The catalog is long enough that
        // letting it run at full height pushes every other control on the
        // page past the bottom of the card.
        egui::ScrollArea::vertical()
            .id_salt("theme_list")
            .max_height(THEME_LIST_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for mode in [themes::ThemeMode::Dark, themes::ThemeMode::Light] {
                    let specs: Vec<_> = themes::themes()
                        .iter()
                        .filter(|spec| spec.mode == mode)
                        .collect();
                    if specs.is_empty() {
                        continue;
                    }
                    ui.label(
                        RichText::new(if mode.is_dark() { "Dark" } else { "Light" })
                            .small()
                            .color(palette().secondary_text),
                    );
                    ui.add_space(SPACE_XS);
                    for spec in specs {
                        let selected = spec.id == app.theme_id;
                        if theme_row(ui, selected, spec).clicked() {
                            chosen = Some(&spec.id);
                        }
                    }
                    ui.add_space(SPACE_MD);
                }
            });
    });
    if let Some(id) = chosen {
        app.set_theme(id);
    }
    group(ui, Icon::App, "Treemap", |ui| {
        ui.checkbox(&mut app.view.grid, "Grid lines between tiles");
        ui.checkbox(&mut app.view.labels, "File labels on large tiles");
        ui.checkbox(
            &mut app.view.free_space,
            "Free-space tile for whole-drive scans",
        );
    });
    see_also(
        ui,
        app,
        "To hide the treemap entirely, see",
        ModalPage::Views,
    );
}

/// One theme in the picker: a live swatch of its own colors, so the list
/// can be read by eye rather than by name.
fn theme_row(ui: &mut egui::Ui, selected: bool, spec: &themes::ThemeSpec) -> egui::Response {
    const HEIGHT: f32 = 38.0;
    let palette = palette();
    let preview = themes::Palette::from_spec(spec);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), HEIGHT),
        egui::Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent_muted
        } else {
            hover_fill(ui, &response, Color32::TRANSPARENT, palette.hover)
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(7),
            fill,
            Stroke::NONE,
            egui::StrokeKind::Middle,
        );
        // The swatch shows the theme's own panel, accent, and two text
        // weights, painted in that theme rather than the active one —
        // which is the whole point of showing a swatch at all.
        let swatch = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 9.0, rect.center().y - 11.0),
            Vec2::new(46.0, 22.0),
        );
        ui.painter().rect(
            swatch,
            egui::CornerRadius::same(4),
            preview.panel,
            Stroke::new(1.0_f32, preview.border),
            egui::StrokeKind::Middle,
        );
        for (index, color) in [preview.accent, preview.primary_text, preview.secondary_text]
            .into_iter()
            .enumerate()
        {
            let dot = egui::Rect::from_center_size(
                egui::pos2(
                    swatch.left() + 11.0 + index as f32 * 12.0,
                    swatch.center().y,
                ),
                Vec2::splat(8.0),
            );
            ui.painter().rect_filled(dot, 2.0, color);
        }
        let color = if selected {
            palette.on_accent
        } else {
            palette.primary_text
        };
        ui.painter().text(
            egui::pos2(swatch.right() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &spec.name,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        if selected {
            Icon::Check.paint(
                ui.painter(),
                egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 16.0, rect.center().y),
                    Vec2::splat(14.0),
                ),
                color,
            );
        }
    }
    #[cfg(test)]
    probe(&TEST_THEME_ROW_RECTS).push((spec.id.clone(), rect));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- Layout

fn draw_layout(app: &mut GuiApp, ui: &mut egui::Ui) {
    group(ui, Icon::LayoutHorizontal, "Treemap position", |ui| {
        ui.radio_value(
            &mut app.view.orientation,
            PaneOrientation::Horizontal,
            "Horizontal — treemap below the lists",
        );
        ui.radio_value(
            &mut app.view.orientation,
            PaneOrientation::Vertical,
            "Vertical — treemap to the right",
        );
    });
    ui.label(
        RichText::new(
            "Every splitter can be dragged to zero, so a pane can be collapsed \
             without turning it off.",
        )
        .color(palette().secondary_text),
    );
    ui.add_space(SPACE_SM);
    see_also(ui, app, "To turn panes off outright, see", ModalPage::Views);
}

// ----------------------------------------------------------------- Views

fn draw_views(app: &mut GuiApp, ui: &mut egui::Ui) {
    group(ui, Icon::Tree, "Panes", |ui| {
        ui.checkbox(&mut app.view.extension_pane, "Extension list");
        ui.checkbox(&mut app.view.treemap, "Treemap");
    });
    group(ui, Icon::Settings, "Window chrome", |ui| {
        ui.checkbox(&mut app.view.toolbar, "Toolbar");
        ui.checkbox(&mut app.view.status_bar, "Status bar");
    });
    see_also(
        ui,
        app,
        "For where these panes sit relative to each other, see",
        ModalPage::Layout,
    );
    see_also(ui, app, "For what each pane does, see", ModalPage::Guide);
}

// ------------------------------------------------------------- Locations

/// Pick what to scan: any number of drives, or a folder.
///
/// WinDirStat opens on this choice — one folder, one drive, several
/// drives, or all of them — and it was the last thing here that only
/// accepted one answer. The volumes come from `platform::volumes`, which
/// offers fixed and removable drives only; anything else is reachable
/// through the folder button, which is the honest place for "somewhere
/// this list does not know about".
fn draw_locations(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    let scanned = app.scanned_roots();
    group(ui, Icon::Folder, "Currently scanned", |ui| {
        for root in &scanned {
            ui.label(crate::util::display_path(root));
        }
    });
    ui.add_space(SPACE_MD);

    let volumes = crate::platform::volumes();
    group(ui, Icon::Tree, "Drives", |ui| {
        if volumes.is_empty() {
            ui.label(
                RichText::new("No local drives were found to offer.").color(palette.secondary_text),
            );
            return;
        }
        for volume in &volumes {
            let mut ticked = app.tools.selected_locations.contains(&volume.path);
            let used = match (volume.total, volume.free) {
                (Some(total), Some(free)) => format!(
                    "{} of {} used",
                    human_bytes(total.saturating_sub(free)),
                    human_bytes(total)
                ),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                if ui.checkbox(&mut ticked, &volume.label).changed() {
                    if ticked {
                        app.tools.selected_locations.push(volume.path.clone());
                    } else {
                        app.tools
                            .selected_locations
                            .retain(|path| path != &volume.path);
                    }
                }
                ui.label(RichText::new(used).color(palette.secondary_text));
            });
        }
        ui.add_space(SPACE_SM);
        ui.horizontal(|ui| {
            if ui.button("Select all").clicked() {
                app.tools.selected_locations = volumes.iter().map(|v| v.path.clone()).collect();
            }
            if ui.button("Clear").clicked() {
                app.tools.selected_locations.clear();
            }
        });
    });
    ui.add_space(SPACE_MD);

    group(ui, Icon::FolderOpen, "Scan", |ui| {
        let count = app.tools.selected_locations.len();
        let label = match count {
            0 => "Scan selected drives".to_string(),
            1 => "Scan 1 drive".to_string(),
            n => format!("Scan {n} drives together"),
        };
        ui.horizontal(|ui| {
            let scan = ui.add_enabled(count > 0 && !app.is_busy(), egui::Button::new(label));
            #[cfg(test)]
            probe(&TEST_LOCATION_SCAN_RECTS).push(scan.rect);
            if scan.clicked() {
                let picked = app.tools.selected_locations.clone();
                app.open_locations(picked);
                app.close_modal();
            }
            if ui
                .add_enabled(!app.is_busy(), egui::Button::new("Choose folder…"))
                .clicked()
            {
                super::actions::choose_folder(app);
                app.close_modal();
            }
        });
        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(
                "Several drives scan into one tree, each as a top-level entry. \
                 Free space is reported per drive, never added together.",
            )
            .color(palette.secondary_text),
        );
    });
}

// ----------------------------------------------------------- Maintenance

/// Indices of the tools that open an interactive Windows utility, as
/// opposed to running an operation directly. The split is what lets the
/// page say which kind the reader is about to get, since "opens a wizard
/// you can cancel" and "starts deleting now" deserve different framing.
const LAUNCHERS: [usize; 4] = [0, 1, 2, 6];

fn draw_maintenance(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    let supported = cfg!(windows);
    if !supported {
        // Above the list, not below it. It used to sit under ten cards
        // whose buttons were still live, so the first thing that told you
        // the feature was unavailable was a failure message after a click.
        callout(
            ui,
            Tone::Warning,
            Icon::Warning,
            "These are Windows utilities. Nothing on this page can run on this system.",
        );
        ui.add_space(SPACE_MD);
    }

    let mut requested = None;
    for (heading, launchers) in [("Open a Windows tool", true), ("Run an operation", false)] {
        ui.label(RichText::new(heading).strong());
        ui.label(
            RichText::new(if launchers {
                "Opens the real utility, which keeps its own confirmation and undo."
            } else {
                "Runs here and reports back. Anything destructive asks first."
            })
            .color(palette.secondary_text),
        );
        ui.add_space(SPACE_SM);
        for (index, tool) in crate::wintools::TOOLS.iter().enumerate() {
            if LAUNCHERS.contains(&index) != launchers {
                continue;
            }
            if tool_row(app, ui, index, tool, supported) {
                requested = Some(index);
            }
        }
        ui.add_space(SPACE_MD);
    }
    if let Some(index) = requested {
        app.request_windows_tool(index);
    }

    if !app.tools.log.is_empty() {
        separator(ui);
        ui.add_space(SPACE_MD);
        ui.label(RichText::new("Results").strong());
        ui.add_space(SPACE_SM);
        for entry in app.tools.log.iter().rev() {
            result_row(ui, entry);
        }
    }
}

/// Returns whether the row's action button was clicked.
///
/// Laid out as two explicitly sized columns rather than a `horizontal`
/// with a right-to-left region on the end. That arrangement looks
/// equivalent and is not: a right-to-left child of a top-aligned
/// horizontal claims the *whole* remaining height of its parent, which
/// inside a scroll area is the rest of the page — so a single row grew
/// to fill the pane and stranded its own button at the bottom of it.
/// Giving each column a width and a zero desired height lets both size
/// to their content, which is what a row is.
fn tool_row(
    app: &GuiApp,
    ui: &mut egui::Ui,
    index: usize,
    tool: &crate::wintools::WinTool,
    supported: bool,
) -> bool {
    /// Width reserved for the action button, so buttons line up down the
    /// list instead of tracking each description's ragged right edge.
    const ACTION_COLUMN: f32 = 132.0;
    const SEVERITY_BAR: f32 = 4.0;

    let palette = palette();
    let running = app.tools.running == Some(index);
    let mut clicked = false;
    let frame = Frame::NONE
        .fill(palette.raised)
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(Margin::symmetric(px(SPACE_MD), px(SPACE_MD)))
        .stroke(Stroke::new(1.0_f32, palette.border))
        .show(ui, |ui| {
            fill_width(ui);
            let full = ui.available_width();
            let text_column = (full - ACTION_COLUMN - 12.0).max(80.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(text_column, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        // `allocate_ui_with_layout` claims only the space
                        // the content actually used, so without this the
                        // column — and the button after it — sat at a
                        // different x on every row.
                        ui.set_min_width(text_column);
                        ui.label(RichText::new(tool.name).strong());
                        ui.label(RichText::new(tool.description).color(palette.secondary_text));
                        if tool.irreversible || tool.needs_admin {
                            ui.add_space(SPACE_XS);
                            ui.horizontal(|ui| {
                                if tool.irreversible {
                                    chip(ui, palette.danger, "Cannot be undone");
                                }
                                if tool.needs_admin {
                                    chip(ui, palette.warning, "Needs administrator");
                                }
                            });
                        }
                    },
                );
                ui.allocate_ui_with_layout(
                    Vec2::new(ACTION_COLUMN, 0.0),
                    Layout::right_to_left(Align::Min),
                    |ui| {
                        ui.set_min_width(ACTION_COLUMN);
                        if running {
                            ui.spinner();
                            ui.label(RichText::new("Running…").color(palette.secondary_text));
                            return;
                        }
                        let label = if tool.destructive {
                            "Review…"
                        } else {
                            "Launch"
                        };
                        let enabled = supported && !app.is_busy();
                        let button = ui.add_enabled(enabled, egui::Button::new(label));
                        clicked = button.clicked();
                        if !supported {
                            button.on_disabled_hover_text("This tool exists only on Windows.");
                        } else if app.is_busy() {
                            button.on_disabled_hover_text(
                                "Another background operation is already running.",
                            );
                        }
                    },
                );
            });
        });

    // The severity edge is painted over the finished frame rather than
    // allocated inside it, so it is exactly as tall as the row turned out
    // to be — a spacer of a guessed height was what forced every row to
    // the same wrong size.
    let row = frame.response.rect;
    if tool.destructive {
        let mut edge = row;
        edge.max.x = row.min.x + SEVERITY_BAR;
        ui.painter().rect_filled(
            edge,
            egui::CornerRadius {
                nw: 8,
                sw: 8,
                ne: 0,
                se: 0,
            },
            palette.danger,
        );
    }
    #[cfg(test)]
    probe(&TEST_TOOL_ROW_MARKERS).push((index, tool.destructive, row));
    ui.add_space(SPACE_SM);
    clicked
}

/// A small pill carrying one fact about a row.
fn chip(ui: &mut egui::Ui, tone: Color32, text: &str) {
    Frame::NONE
        .fill(blend(palette().raised, tone, 0.18))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(Margin::symmetric(px(SPACE_SM), px(SPACE_XS)))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(tone));
        });
}

fn result_row(ui: &mut egui::Ui, entry: &crate::gui::app::ToolOutcome) {
    let palette = palette();
    let tone = if entry.failed {
        palette.danger
    } else {
        palette.success
    };
    Frame::NONE
        .fill(palette.raised)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(Margin::same(px(SPACE_MD)))
        .stroke(Stroke::new(1.0_f32, palette.border))
        .show(ui, |ui| {
            fill_width(ui);
            ui.horizontal(|ui| {
                paint_inline_icon(
                    ui,
                    if entry.failed {
                        Icon::Warning
                    } else {
                        Icon::Check
                    },
                    14.0,
                    tone,
                );
                ui.label(RichText::new(&entry.tool).strong());
            });
            ui.label(RichText::new(&entry.summary).color(palette.secondary_text));
            if !entry.detail.is_empty() {
                ui.add_space(SPACE_SM);
                // Monospace, because a DISM report is a table and a
                // proportional font turns it back into prose.
                Frame::NONE
                    .fill(palette.app)
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(Margin::same(px(SPACE_SM)))
                    .show(ui, |ui| {
                        fill_width(ui);
                        ui.label(RichText::new(&entry.detail).monospace().small());
                    });
            }
        });
    ui.add_space(SPACE_SM);
}

// ------------------------------------------------------------ Guide/About

fn draw_guide(ui: &mut egui::Ui) {
    let palette = palette();
    group(ui, Icon::Tree, "All Files", |ui| {
        ui.label("The expandable, size-sorted directory tree. Selecting an item frames the same item in the treemap.");
    });
    group(ui, Icon::Extensions, "Extensions", |ui| {
        ui.label("Groups files by exact extension, with color, bytes, percentage, and count. Selecting a row highlights the matching treemap files.");
    });
    group(ui, Icon::App, "Treemap", |ui| {
        ui.label("Every file is an area proportional to its size. Directory areas nest, tiles take their extension's color, and clicking one selects the matching path in the tree.");
    });
    group(ui, Icon::Largest, "The other file views", |ui| {
        ui.label("Largest Files is a flat top-200 by size. Duplicate Files groups byte-identical files by hash. Search Results accepts glob patterns — * for any run of characters, ? for one, [a-z] for a character class and {jpg,png} for alternatives — and regular expressions when the query starts with re:.");
    });
    ui.label(
        RichText::new(
            "This mirrors WinDirStat's three coupled views: whatever is selected in one \
             is selected in all of them.",
        )
        .color(palette.secondary_text),
    );
}

fn draw_about(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    ui.horizontal(|ui| {
        paint_inline_brand(ui, 40.0);
        ui.add_space(SPACE_SM);
        ui.vertical(|ui| {
            ui.label(RichText::new("RustDirStat").heading().strong());
            ui.label(
                RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .color(palette.secondary_text),
            );
        });
    });
    ui.add_space(SPACE_MD);
    group(ui, Icon::Info, "What this is", |ui| {
        ui.label("A WinDirStat clone in Rust, with a terminal front end and this desktop one over a single scanning core.");
    });
    group(ui, Icon::Folder, "Current scan", |ui| {
        let node = app.zoom_node();
        egui::Grid::new("about_scan_grid")
            .num_columns(2)
            .spacing(Vec2::new(18.0, 9.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Root").color(palette.secondary_text));
                ui.label(crate::util::display_path(&app.tree.root_path));
                ui.end_row();
                ui.label(RichText::new("Files").color(palette.secondary_text));
                ui.label(thousands(node.file_count));
                ui.end_row();
                ui.label(RichText::new("Folders").color(palette.secondary_text));
                ui.label(thousands(node.dir_count));
                ui.end_row();
            });
    });
    see_also(ui, app, "For what each view does, see", ModalPage::Guide);
}

// -------------------------------------------------------------- Cleanups

/// The user's own commands, run against the selection.
///
/// The list comes from `app.cleanups`, which is re-read from the config
/// when this page is *opened* — never while drawing. A file read on the
/// frame path is exactly the kind of thing that turns a 120 FPS window
/// into a stuttering one, and the list only changes when someone edits
/// the file.
fn draw_cleanups(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    let cleanups = app.cleanups.clone();
    let selected = app.selected_fs_path();

    if cleanups.is_empty() {
        empty_state(
            ui,
            Icon::Tools,
            "No cleanups configured",
            "A cleanup is a command of your own, run against whatever is selected.              They live in the config file; nothing is set up by default.",
        );
        ui.add_space(SPACE_MD);
    }

    group(ui, Icon::Tools, "Your commands", |ui| {
        if cleanups.is_empty() {
            ui.label(
                RichText::new("Nothing to show until the config file has some.")
                    .color(palette.secondary_text),
            );
            return;
        }
        for (index, cleanup) in cleanups.iter().enumerate() {
            ui.horizontal(|ui| {
                let run = ui.add_enabled(
                    selected.is_some() && !app.is_busy(),
                    egui::Button::new(&cleanup.name),
                );
                #[cfg(test)]
                probe(&TEST_CLEANUP_RECTS).push((cleanup.name.clone(), run.rect));
                if run.clicked() {
                    app.request_cleanup(index);
                }
                ui.label(RichText::new(command_summary(cleanup)).color(palette.secondary_text));
            });
        }
        if selected.is_none() {
            ui.add_space(SPACE_SM);
            ui.label(
                RichText::new("Select an item first — a cleanup always acts on one thing.")
                    .color(palette.secondary_text),
            );
        }
    });

    ui.add_space(SPACE_MD);
    group(ui, Icon::Info, "How they work", |ui| {
        ui.label(
            RichText::new(
                "Each cleanup names a program and its arguments as a list.                  %p is the full path, %n the name, %d the containing folder,                  and %% a literal per cent.",
            )
            .color(palette.secondary_text),
        );
        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(
                "No shell is involved: arguments are passed as they are, so a file                  name containing spaces, quotes or semicolons is data rather than                  syntax. Cleanups ask before running unless their entry says otherwise.",
            )
            .color(palette.secondary_text),
        );
    });
}

/// A one-line rendering of what a cleanup will run, before substitution.
fn command_summary(cleanup: &crate::cleanups::Cleanup) -> String {
    if cleanup.args.is_empty() {
        return cleanup.program.clone();
    }
    format!("{} {}", cleanup.program, cleanup.args.join(" "))
}

// ---------------------------------------------------------- Confirmations

pub(super) fn draw_confirm(app: &mut GuiApp, ctx: &egui::Context, kind: ConfirmKind, opening: f32) {
    match kind {
        ConfirmKind::Delete => draw_delete_confirm(app, ctx, opening),
        ConfirmKind::WindowsTool(index) => draw_tool_confirm(app, ctx, index, opening),
        ConfirmKind::Cleanup => draw_cleanup_confirm(app, ctx, opening),
    }
}

fn draw_delete_confirm(app: &mut GuiApp, ctx: &egui::Context, opening: f32) {
    let Some(pending) = &app.pending_delete else {
        return;
    };
    let (name, permanent, is_dir) = (
        pending.name.to_string_lossy().to_string(),
        pending.permanent,
        pending.is_dir,
    );
    let palette = palette();
    let mut confirm = false;
    let mut empty = false;
    let mut cancel = false;
    confirm_card(ctx, "confirm_delete", opening, |ui| {
        ui.horizontal(|ui| {
            paint_inline_icon(ui, Icon::Trash, 20.0, palette.danger);
            ui.add_space(SPACE_XS);
            ui.label(
                RichText::new(if permanent {
                    "Delete permanently?"
                } else {
                    "Move to the Recycle Bin?"
                })
                .heading()
                .strong(),
            );
        });
        ui.add_space(SPACE_SM);
        ui.label(RichText::new(&name).strong());
        ui.add_space(SPACE_SM);
        if permanent {
            callout(ui, Tone::Danger, Icon::Warning, "This cannot be undone.");
        } else {
            ui.label(
                RichText::new("The item can be restored from the Recycle Bin.")
                    .color(palette.secondary_text),
            );
        }
        if is_dir {
            ui.add_space(SPACE_SM);
            ui.label(
                RichText::new("Empty keeps the folder and removes only its contents.")
                    .color(palette.secondary_text),
            );
        }
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            let delete = if permanent {
                danger_button(ui, "Delete permanently")
            } else {
                accent_button(ui, "Move to Recycle Bin")
            };
            if delete.clicked() {
                confirm = true;
            }
            if is_dir && ui.button("Empty folder").clicked() {
                empty = true;
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

/// "This is the command, exactly as it will run."
///
/// The whole point of the card: a user-defined cleanup is a template plus
/// a selection, and neither of those is checkable on its own. What is
/// shown is the resolved argv — program and every argument after
/// substitution — because that is the thing that is actually about to
/// happen. See `docs/CLEANUPS_THREAT_MODEL.md`.
fn draw_cleanup_confirm(app: &mut GuiApp, ctx: &egui::Context, opening: f32) {
    let Some(pending) = &app.tools.pending_cleanup else {
        return;
    };
    let (name, preview) = (pending.name.clone(), pending.command.preview());
    let palette = palette();
    let mut confirm = false;
    let mut cancel = false;
    confirm_card(ctx, "confirm_cleanup", opening, |ui| {
        ui.horizontal(|ui| {
            paint_inline_icon(ui, Icon::Tools, 20.0, palette.accent);
            ui.add_space(SPACE_XS);
            ui.label(RichText::new(format!("Run {name}?")).heading().strong());
        });
        ui.add_space(SPACE_SM);
        ui.label(
            RichText::new("This runs a program you configured, with your privileges.")
                .color(palette.secondary_text),
        );
        ui.add_space(SPACE_SM);
        // Monospaced and selectable-looking: this is the text the user is
        // being asked about, not a description of it.
        ui.label(RichText::new(&preview).monospace());
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if accent_button(ui, "Run").clicked() {
                confirm = true;
            }
        });
    });
    if confirm {
        app.confirm_cleanup();
    }
    if cancel {
        app.tools.pending_cleanup = None;
    }
}

fn draw_tool_confirm(app: &mut GuiApp, ctx: &egui::Context, index: usize, opening: f32) {
    let Some(tool) = crate::wintools::TOOLS.get(index) else {
        app.tools.pending = None;
        return;
    };
    let palette = palette();
    let mut confirm = false;
    let mut cancel = false;
    confirm_card(ctx, "confirm_tool", opening, |ui| {
        ui.horizontal(|ui| {
            // Not a trash can for everything destructive. Component-store
            // cleanup reclaims space; it does not delete the user's
            // files, and dressing it as a deletion trains the reader to
            // ignore the icon on the one that is.
            let icon = if tool.irreversible {
                Icon::Warning
            } else {
                Icon::Tools
            };
            let tone = if tool.irreversible {
                palette.danger
            } else {
                palette.warning
            };
            paint_inline_icon(ui, icon, 20.0, tone);
            ui.add_space(SPACE_XS);
            ui.label(RichText::new(tool.name).heading().strong());
        });
        ui.add_space(SPACE_MD);
        // The tool's own words, not a generic warning. The generic one
        // was simultaneously stronger than the truth for routine cleanup
        // and weaker than it for deleting every shadow copy.
        ui.label(tool.description);
        ui.add_space(SPACE_MD);
        if tool.irreversible {
            callout(ui, Tone::Danger, Icon::Warning, "This cannot be undone.");
            ui.add_space(SPACE_SM);
        }
        if tool.needs_admin {
            callout(
                ui,
                Tone::Warning,
                Icon::Info,
                "Needs an elevated session. Without one it will fail immediately.",
            );
        }
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            let run = if tool.irreversible {
                danger_button(ui, "Run anyway")
            } else {
                accent_button(ui, "Run")
            };
            if run.clicked() {
                confirm = true;
            }
        });
    });
    if confirm {
        app.confirm_windows_tool();
    }
    if cancel {
        app.tools.pending = None;
    }
}
