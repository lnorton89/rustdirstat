// ============================================================================
// Module:       gui::ui::pages
// Description:  The contents of each modal page, and the two destructive-
//               action confirmations that layer above them.
//
// Dependencies: eframe::egui; crate::gui::{app::GuiApp, flags}, super::{modal,
//               themes, widgets}
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
use crate::gui::flags;
use crate::gui::icons::Icon;
use crate::i18n::{tr, tr_with};
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
fn group(
    ui: &mut egui::Ui,
    icon: Icon,
    title: impl AsRef<str>,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
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
                ui.label(RichText::new(title.as_ref()).strong());
            });
            ui.add_space(SPACE_SM);
            add_contents(ui);
        });
    ui.add_space(SPACE_MD);
}

/// A sentence that ends by sending the reader to another page.
fn see_also(ui: &mut egui::Ui, app: &mut GuiApp, lead: impl AsRef<str>, page: ModalPage) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new(lead.as_ref()).color(palette().secondary_text));
        if page_link(ui, page) {
            app.modal = Some(page);
        }
        ui.label(RichText::new(".").color(palette().secondary_text));
    });
}

// ------------------------------------------------------------ Appearance

/// Visible height of the theme list before it scrolls on its own.
const THEME_LIST_HEIGHT: f32 = 330.0;

/// The language picker.
///
/// The list is [`i18n::available`], which is the twelve catalogues
/// compiled into the binary plus whatever `.toml` files the user has
/// dropped into `lang/` beside the config. Both sources are offered the
/// same way on purpose: a translation is content, and content that needs
/// a compiler to add is a translation nobody writes — so a file appearing
/// in that folder is all it takes to extend this list.
///
/// It used to be a row of bare tags, and a tag is the one label a reader
/// cannot use: someone looking for their own language is looking for the
/// word they call it by. Each entry leads with a flag, to be found by eye
/// before it is read, and names itself in its own language with the
/// English name beneath — the tag stays, small, because it is also the
/// name of the file in the folder named below.
fn draw_language(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    let available = crate::i18n::available();
    let current = crate::i18n::language();
    let mut wrote: Option<String> = None;
    group(
        ui,
        Icon::Globe,
        crate::i18n::tr("settings.language"),
        |ui| {
            // Its own scroll, bounded, for the reason the theme catalog
            // below has one: a dozen languages laid out two to a line is
            // three hundred pixels of page, and everything under it — the
            // whole theme picker — starts below the fold. The number of
            // catalogues is also the user's to decide, so an unbounded
            // grid is a page whose length someone else controls.
            egui::ScrollArea::vertical()
                .id_salt("language_list")
                .max_height(LANGUAGE_LIST_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // The grid re-fits the card rather than wrapping a fixed
                    // tile width: the card is a proportion of the window, so a
                    // fixed column count would strand empty space on a wide
                    // screen and overflow the pane on a narrow one.
                    let width = ui.available_width();
                    let columns =
                        ((width + SPACE_SM) / (LANGUAGE_TILE_MIN_WIDTH + SPACE_SM)).floor();
                    let columns = (columns as usize).max(1);
                    let tile = ((width - SPACE_SM * (columns - 1) as f32) / columns as f32).floor();
                    for row in available.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = SPACE_SM;
                            for tag in row {
                                if language_tile(ui, &current == tag, tag, tile).clicked()
                                    && &current != tag
                                {
                                    crate::i18n::set_language(tag);
                                }
                            }
                        });
                        ui.add_space(SPACE_SM);
                    }
                });
            ui.add_space(SPACE_XS);
            ui.label(
                RichText::new(crate::i18n::tr_with(
                    "settings.language.folder",
                    &[(
                        "path",
                        &crate::i18n::catalog_dir()
                            .map(|dir| crate::util::display_path(&dir))
                            .unwrap_or_default(),
                    )],
                ))
                .color(palette.secondary_text),
            );
            ui.add_space(SPACE_SM);
            // The catalogue is compiled into the binary and ships in no
            // package, so without this an installed build has no way to
            // obtain the file the sentence above tells you to copy.
            if ui
                .button(crate::i18n::tr("settings.language.write_template"))
                .on_hover_text(crate::i18n::tr("settings.language.write_template_hint"))
                .clicked()
            {
                wrote = Some(match crate::i18n::write_starter_catalogue() {
                    Ok(path) => crate::i18n::tr_with(
                        "settings.language.written",
                        &[("path", &crate::util::display_path(&path))],
                    ),
                    Err(error) => crate::i18n::tr_with(
                        "settings.language.write_failed",
                        &[("error", &error.to_string())],
                    ),
                });
            }
        },
    );
    if let Some(message) = wrote {
        app.status = Some(message);
    }
}

/// Visible height of the language grid before it scrolls on its own —
/// three rows of tiles, whatever the card's width makes a row.
const LANGUAGE_LIST_HEIGHT: f32 = 160.0;

/// Height of one entry in the language picker: two lines of text, and a
/// flag with enough room around it to be told from the one below.
const LANGUAGE_TILE_HEIGHT: f32 = 44.0;

/// The narrowest a language entry may be laid out at before the grid
/// drops a column. Wide enough for `Português do Brasil`, which is the
/// longest name the table holds.
const LANGUAGE_TILE_MIN_WIDTH: f32 = 196.0;

/// How tall a flag is drawn in the picker. Its width follows from
/// [`flags::ASPECT`], so every flag is the same rectangle whatever it
/// holds — which is what makes a column of them scannable.
const FLAG_HEIGHT: f32 = 15.0;

/// How far each of a tile's two lines sits from its centre. One number
/// for both, so the pair is centred as a block.
const LANGUAGE_LINE_OFFSET: f32 = 9.0;

/// One language in the picker: its flag, its own name for itself, and
/// the English name and tag under that.
///
/// Laid out by hand into an exactly-allocated rect rather than composed
/// from labels, for the reason every other tiled control here is: a
/// `Frame` measures itself against the line it is on and *then* allocates
/// what it measured, so a row of them overflows the pane instead of
/// wrapping. `width` is the column the caller worked out from the space
/// it has, and the tile never asks for more than that.
fn language_tile(ui: &mut egui::Ui, selected: bool, tag: &str, width: f32) -> egui::Response {
    let palette = palette();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, LANGUAGE_TILE_HEIGHT), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.accent_muted
        } else {
            hover_fill(ui, &response, Color32::TRANSPARENT, palette.hover)
        };
        let stroke = if selected || response.has_focus() {
            Stroke::new(1.0_f32, palette.accent)
        } else {
            Stroke::new(1.0_f32, palette.border)
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(7),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );

        let flag_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + SPACE_MD, rect.center().y - FLAG_HEIGHT * 0.5),
            Vec2::new(FLAG_HEIGHT * flags::ASPECT, FLAG_HEIGHT),
        );
        match flags::flag(tag) {
            Some(flag) => flag.paint(ui.painter(), flag_rect, palette.border),
            // A tag with no flag is an ordinary case, not a broken one —
            // somebody's own catalogue, or a language whose flag could not
            // be told from another at this size. It takes the same
            // rectangle either way, so the names stay in one column.
            None => Icon::Globe.paint(
                ui.painter(),
                egui::Rect::from_center_size(flag_rect.center(), Vec2::splat(FLAG_HEIGHT)),
                palette.secondary_text,
            ),
        }

        let (title, subtitle) = match crate::i18n::language_name(tag) {
            // `English` written under `English` is a line that says
            // nothing, so there the tag stands on its own.
            Some(name) if name.english == name.endonym => {
                (name.endonym.to_string(), tag.to_string())
            }
            Some(name) => (
                name.endonym.to_string(),
                format!("{} · {tag}", name.english),
            ),
            // Nothing in the table: the tag is all there is to show, and
            // it is exactly what this picker showed for every language
            // before there was a table.
            None => (tag.to_string(), String::new()),
        };
        let color = if selected {
            palette.on_accent
        } else {
            palette.primary_text
        };
        let left = flag_rect.right() + SPACE_SM;
        // A selected tile ends in a checkmark, so the text stops short of
        // where that will be — on every tile, selected or not, or the name
        // would shift sideways as the selection moved.
        let room = (rect.right() - SPACE_MD - FLAG_HEIGHT - SPACE_SM - left).max(0.0);
        // Truncated by egui rather than by `truncate_for_width`, which
        // measures in the small text style — these two lines are set in
        // two different styles, and a cut measured in the wrong one is no
        // cut at all.
        let line = |ui: &egui::Ui, text: &str, style: egui::TextStyle| {
            egui::WidgetText::from(text.to_string()).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                room,
                style,
            )
        };
        let title = line(ui, &title, egui::TextStyle::Body);
        let title_y = if subtitle.is_empty() {
            rect.center().y
        } else {
            rect.center().y - LANGUAGE_LINE_OFFSET
        };
        ui.painter().galley(
            egui::pos2(left, title_y - title.size().y * 0.5),
            title,
            color,
        );
        if !subtitle.is_empty() {
            let subtitle = line(ui, &subtitle, egui::TextStyle::Small);
            let y = rect.center().y + LANGUAGE_LINE_OFFSET - subtitle.size().y * 0.5;
            ui.painter().galley(
                egui::pos2(left, y),
                subtitle,
                if selected {
                    palette.on_accent
                } else {
                    palette.secondary_text
                },
            );
        }
        if selected {
            Icon::Check.paint(
                ui.painter(),
                egui::Rect::from_center_size(
                    egui::pos2(rect.right() - SPACE_MD - FLAG_HEIGHT * 0.5, rect.center().y),
                    Vec2::splat(FLAG_HEIGHT),
                ),
                color,
            );
        }
    }
    #[cfg(test)]
    probe(&TEST_LANGUAGE_TILE_RECTS).push((tag.to_string(), rect));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn draw_appearance(app: &mut GuiApp, ui: &mut egui::Ui) {
    draw_language(app, ui);
    ui.add_space(SPACE_MD);
    let mut chosen: Option<&'static str> = None;
    group(ui, Icon::Palette, tr("page.appearance.theme"), |ui| {
        ui.label(RichText::new(tr("page.appearance.theme.blurb")).color(palette().secondary_text));
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
                        RichText::new(if mode.is_dark() {
                            tr("page.appearance.dark")
                        } else {
                            tr("page.appearance.light")
                        })
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
    group(ui, Icon::App, tr("label.treemap"), |ui| {
        ui.checkbox(&mut app.view.grid, tr("page.appearance.grid"));
        ui.checkbox(&mut app.view.labels, tr("page.appearance.labels"));
        ui.checkbox(&mut app.view.free_space, tr("page.appearance.free_space"));
    });
    see_also(ui, app, tr("page.appearance.hide_hint"), ModalPage::Views);
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
    group(
        ui,
        Icon::LayoutHorizontal,
        tr("page.layout.position"),
        |ui| {
            ui.radio_value(
                &mut app.view.orientation,
                PaneOrientation::Horizontal,
                tr("page.layout.horizontal"),
            );
            ui.radio_value(
                &mut app.view.orientation,
                PaneOrientation::Vertical,
                tr("page.layout.vertical"),
            );
        },
    );
    ui.label(RichText::new(tr("page.layout.splitter_hint")).color(palette().secondary_text));
    ui.add_space(SPACE_SM);
    see_also(ui, app, tr("page.layout.off_hint"), ModalPage::Views);
}

// ----------------------------------------------------------------- Views

fn draw_views(app: &mut GuiApp, ui: &mut egui::Ui) {
    group(ui, Icon::Tree, "Panes", |ui| {
        ui.checkbox(&mut app.view.extension_pane, tr("label.extension_list"));
        ui.checkbox(&mut app.view.treemap, tr("label.treemap"));
    });
    group(ui, Icon::Settings, tr("page.views.chrome"), |ui| {
        ui.checkbox(&mut app.view.toolbar, tr("label.toolbar"));
        ui.checkbox(&mut app.view.status_bar, tr("label.status_bar"));
    });
    see_also(ui, app, tr("page.views.layout_hint"), ModalPage::Layout);
    see_also(ui, app, tr("page.views.guide_hint"), ModalPage::Guide);
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
    group(ui, Icon::Folder, tr("page.locations.current"), |ui| {
        for root in &scanned {
            ui.label(crate::util::display_path(root));
        }
    });
    ui.add_space(SPACE_MD);

    let volumes = crate::platform::volumes();
    group(ui, Icon::Tree, tr("page.locations.drives"), |ui| {
        if volumes.is_empty() {
            ui.label(RichText::new(tr("page.locations.none")).color(palette.secondary_text));
            return;
        }
        for volume in &volumes {
            let mut ticked = app.tools.selected_locations.contains(&volume.path);
            let used = match (volume.total, volume.free) {
                (Some(total), Some(free)) => tr_with(
                    "page.locations.used",
                    &[
                        ("used", &human_bytes(total.saturating_sub(free))),
                        ("total", &human_bytes(total)),
                    ],
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
            if ui.button(tr("page.locations.select_all")).clicked() {
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
            0 => tr("page.locations.scan_selected").to_string(),
            1 => tr("page.locations.scan_one").to_string(),
            n => tr_with("page.locations.scan_many", &[("count", &n.to_string())]),
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
                .add_enabled(
                    !app.is_busy(),
                    egui::Button::new(tr("page.locations.choose_folder")),
                )
                .clicked()
            {
                super::actions::choose_folder(app);
                app.close_modal();
            }
        });
        ui.add_space(SPACE_XS);
        ui.label(RichText::new(tr("page.locations.multi_root")).color(palette.secondary_text));
    });

    // A scan option, so it lives with the page about what a scan covers
    // rather than with the ones about how the window looks. Without it
    // the window could never turn hard-link measurement on under
    // Windows, where it is off by default — so the figure it produces was
    // built, translated and unreachable on the platform this app is most
    // used on.
    group(ui, Icon::Info, tr("page.locations.scan_options"), |ui| {
        let mut counting = app.scan_options.count_hard_links;
        if ui
            .checkbox(&mut counting, tr("page.locations.count_hard_links"))
            .changed()
        {
            app.scan_options.count_hard_links = counting;
        }
        ui.add_space(SPACE_XS);
        ui.label(
            RichText::new(tr("page.locations.count_hard_links.blurb"))
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
            tr("page.maintenance.unsupported"),
        );
        ui.add_space(SPACE_MD);
    }

    let mut requested = None;
    for (heading, launchers) in [
        (tr("page.maintenance.open_tool"), true),
        (tr("page.maintenance.run_operation"), false),
    ] {
        ui.label(RichText::new(heading).strong());
        ui.label(
            RichText::new(if launchers {
                tr("page.maintenance.open_blurb")
            } else {
                tr("page.maintenance.run_blurb")
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
        ui.label(RichText::new(tr("page.maintenance.results")).strong());
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
                        ui.label(RichText::new(tool.name()).strong());
                        ui.label(RichText::new(tool.description()).color(palette.secondary_text));
                        if tool.irreversible || tool.needs_admin {
                            ui.add_space(SPACE_XS);
                            ui.horizontal(|ui| {
                                if tool.irreversible {
                                    chip(ui, palette.danger, tr("page.maintenance.irreversible"));
                                }
                                if tool.needs_admin {
                                    chip(ui, palette.warning, tr("page.maintenance.needs_admin"));
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
                            ui.label(
                                RichText::new(tr("page.maintenance.running"))
                                    .color(palette.secondary_text),
                            );
                            return;
                        }
                        let label = if tool.destructive {
                            tr("page.maintenance.review")
                        } else {
                            tr("page.maintenance.launch")
                        };
                        let enabled = supported && !app.is_busy();
                        let button = ui.add_enabled(enabled, egui::Button::new(label));
                        clicked = button.clicked();
                        if !supported {
                            button.on_disabled_hover_text(tr("page.maintenance.windows_only"));
                        } else if app.is_busy() {
                            button.on_disabled_hover_text(crate::i18n::tr("status.busy"));
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
fn chip(ui: &mut egui::Ui, tone: Color32, text: impl AsRef<str>) {
    Frame::NONE
        .fill(blend(palette().raised, tone, 0.18))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(Margin::symmetric(px(SPACE_SM), px(SPACE_XS)))
        .show(ui, |ui| {
            ui.label(RichText::new(text.as_ref()).small().color(tone));
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
    group(ui, Icon::Tree, tr("view.all_files"), |ui| {
        ui.label(tr("page.guide.all_files"));
    });
    group(ui, Icon::Extensions, tr("page.guide.extensions"), |ui| {
        ui.label(tr("page.guide.extensions.body"));
    });
    group(ui, Icon::App, tr("label.treemap"), |ui| {
        ui.label(tr("page.guide.treemap.body"));
    });
    group(ui, Icon::Largest, tr("page.guide.other_views"), |ui| {
        ui.label(tr("page.guide.other_views.body"));
    });
    ui.label(RichText::new(tr("page.guide.footnote")).color(palette.secondary_text));
}

fn draw_about(app: &mut GuiApp, ui: &mut egui::Ui) {
    let palette = palette();
    ui.horizontal(|ui| {
        paint_inline_brand(ui, 40.0);
        ui.add_space(SPACE_SM);
        ui.vertical(|ui| {
            ui.label(RichText::new("RustDirStat").heading().strong());
            ui.label(
                RichText::new(tr_with(
                    "page.about.version",
                    &[("version", env!("CARGO_PKG_VERSION"))],
                ))
                .color(palette.secondary_text),
            );
        });
    });
    ui.add_space(SPACE_MD);
    group(ui, Icon::Info, tr("page.about.what"), |ui| {
        ui.label(tr("page.about.what.body"));
    });
    group(ui, Icon::Folder, tr("page.about.current_scan"), |ui| {
        let node = app.zoom_node();
        egui::Grid::new("about_scan_grid")
            .num_columns(2)
            .spacing(Vec2::new(18.0, 9.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Root").color(palette.secondary_text));
                ui.label(crate::util::display_path(&app.tree.root_path));
                ui.end_row();
                ui.label(RichText::new(tr("page.about.files")).color(palette.secondary_text));
                ui.label(thousands(node.file_count));
                ui.end_row();
                ui.label(RichText::new(tr("page.about.folders")).color(palette.secondary_text));
                ui.label(thousands(node.dir_count));
                ui.end_row();
            });
    });
    see_also(ui, app, tr("page.about.guide_hint"), ModalPage::Guide);
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
            tr("page.cleanups.none"),
            tr("page.cleanups.none.body"),
        );
        ui.add_space(SPACE_MD);
    }

    group(ui, Icon::Tools, tr("page.cleanups.yours"), |ui| {
        if cleanups.is_empty() {
            ui.label(RichText::new(tr("page.cleanups.empty")).color(palette.secondary_text));
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
            ui.label(RichText::new(tr("page.cleanups.select_first")).color(palette.secondary_text));
        }
    });

    ui.add_space(SPACE_MD);
    group(ui, Icon::Info, tr("page.cleanups.how"), |ui| {
        ui.label(RichText::new(tr("page.cleanups.how.body1")).color(palette.secondary_text));
        ui.add_space(SPACE_XS);
        ui.label(RichText::new(tr("page.cleanups.how.body2")).color(palette.secondary_text));
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
                    tr("confirm.delete.permanent.title")
                } else {
                    tr("confirm.delete.trash.title")
                })
                .heading()
                .strong(),
            );
        });
        ui.add_space(SPACE_SM);
        ui.label(RichText::new(&name).strong());
        ui.add_space(SPACE_SM);
        if permanent {
            callout(ui, Tone::Danger, Icon::Warning, tr("confirm.cannot_undo"));
        } else {
            ui.label(RichText::new(tr("confirm.delete.restorable")).color(palette.secondary_text));
        }
        if is_dir {
            ui.add_space(SPACE_SM);
            ui.label(RichText::new(tr("confirm.delete.empty_hint")).color(palette.secondary_text));
        }
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr("confirm.cancel")).clicked() {
                cancel = true;
            }
            let delete = if permanent {
                danger_button(ui, tr("confirm.delete_permanently"))
            } else {
                accent_button(ui, tr("confirm.move_to_bin"))
            };
            if delete.clicked() {
                confirm = true;
            }
            if is_dir && ui.button(tr("confirm.empty_folder")).clicked() {
                empty = true;
            }
        });
    });
    if confirm {
        if let Err(e) = app.confirm_delete() {
            app.status = Some(tr_with(
                "status.delete_failed",
                &[("error", &e.to_string())],
            ));
        }
    }
    if empty {
        if let Err(e) = app.confirm_empty() {
            app.status = Some(tr_with("status.empty_failed", &[("error", &e.to_string())]));
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
            ui.label(
                RichText::new(tr_with("confirm.tool.question", &[("name", &name)]))
                    .heading()
                    .strong(),
            );
        });
        ui.add_space(SPACE_SM);
        ui.label(RichText::new(tr("confirm.tool.blurb")).color(palette.secondary_text));
        ui.add_space(SPACE_SM);
        // Monospaced and selectable-looking: this is the text the user is
        // being asked about, not a description of it.
        ui.label(RichText::new(&preview).monospace());
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr("confirm.cancel")).clicked() {
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
            ui.label(RichText::new(tool.name()).heading().strong());
        });
        ui.add_space(SPACE_MD);
        // The tool's own words, not a generic warning. The generic one
        // was simultaneously stronger than the truth for routine cleanup
        // and weaker than it for deleting every shadow copy.
        ui.label(tool.description());
        ui.add_space(SPACE_MD);
        if tool.irreversible {
            callout(ui, Tone::Danger, Icon::Warning, tr("confirm.cannot_undo"));
            ui.add_space(SPACE_SM);
        }
        if tool.needs_admin {
            callout(ui, Tone::Warning, Icon::Info, tr("confirm.tool.admin"));
        }
        ui.add_space(SPACE_LG);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr("confirm.cancel")).clicked() {
                cancel = true;
            }
            let run = if tool.irreversible {
                danger_button(ui, tr("confirm.tool.run_anyway"))
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
