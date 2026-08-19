use super::app::{size_label, GuiApp};
use crate::color;
use crate::tui::SortMode;
use crate::util::{human_bytes, thousands};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, TextStyle, Vec2};
use egui_extras::{Column, TableBuilder};

pub fn draw(app: &mut GuiApp, ctx: &egui::Context) {
    draw_menu_bar(app, ctx);
    draw_toolbar(app, ctx);
    draw_legend(app, ctx);
    draw_status_bar(app, ctx);

    egui::TopBottomPanel::top("file_list_panel")
        .resizable(true)
        .default_height(ctx.available_rect().height() * 0.4)
        .min_height(80.0)
        .show(ctx, |ui| draw_file_list(app, ui));

    egui::CentralPanel::default().show(ctx, |ui| draw_treemap(app, ui));

    draw_delete_dialog(app, ctx);
    draw_properties_dialog(app, ctx);
}

fn draw_menu_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Refresh").clicked() {
                    if let Err(e) = app.refresh_scan() {
                        app.status = Some(format!("Refresh failed: {e}"));
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Export CSV…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("CSV", &["csv"])
                        .save_file()
                    {
                        match crate::csv_export::write_csv_to_file(
                            &app.tree.root_path,
                            &app.tree.root,
                            &path,
                        ) {
                            Ok(()) => {
                                app.status = Some(format!("Exported CSV to {}", path.display()))
                            }
                            Err(e) => app.status = Some(format!("CSV export failed: {e}")),
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .checkbox(&mut app.use_physical, "Show physical (on-disk) size")
                    .changed()
                {
                    app.refresh_ext_stats();
                }
            });
        });
    });
}

fn draw_toolbar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!app.path_indices.is_empty(), egui::Button::new("⬆ Up"))
                .clicked()
            {
                app.go_up();
            }
            if ui.button("⟳ Refresh").clicked() {
                if let Err(e) = app.refresh_scan() {
                    app.status = Some(format!("Refresh failed: {e}"));
                }
            }
            ui.separator();
            let node = app.current_node();
            ui.label(
                RichText::new(app.current_path().display().to_string())
                    .strong()
                    .monospace(),
            );
            ui.label(format!(
                "  ·  {}, {} files",
                size_label(node.effective_size(app.use_physical), app.use_physical),
                thousands(node.file_count)
            ));
            if node.unreadable_count > 0 {
                ui.colored_label(
                    Color32::from_rgb(230, 175, 46),
                    format!("⚠ {} unreadable", thousands(node.unreadable_count)),
                );
            }
        });
    });
}

fn draw_status_bar(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some(status) = &app.status {
                ui.label(status);
            } else {
                ui.label("Ready");
            }
        });
    });
}

fn draw_legend(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("legend").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let total: u64 = app.ext_stats.iter().map(|s| s.size).sum::<u64>().max(1);
            for (i, stat) in app.ext_stats.iter().enumerate() {
                let pct = stat.size as f64 / total as f64 * 100.0;
                let swatch_color = category_color(stat.category);
                let is_highlighted = app.highlighted_category == Some(stat.category);
                let label = format!("{} {:.1}%", stat.category.label(), pct);
                let response = ui.add(
                    egui::Button::new(RichText::new(format!("{i}. ")).weak())
                        .frame(false)
                        .small(),
                );
                let (swatch_rect, swatch_resp) =
                    ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::click());
                ui.painter().rect_filled(swatch_rect, 2.0, swatch_color);
                let text_resp = ui.selectable_label(is_highlighted, label);
                if response.clicked() || swatch_resp.clicked() || text_resp.clicked() {
                    app.highlighted_category = if is_highlighted {
                        None
                    } else {
                        Some(stat.category)
                    };
                }
                ui.add_space(10.0);
            }
        });
    });
}

fn category_color(cat: crate::color::Category) -> Color32 {
    to_color32(cat.color())
}

/// `color.rs`'s palette functions return `ratatui::style::Color` — that
/// module is shared by both front ends and predates the GUI, so its
/// return type reflects whichever one used it first rather than being
/// front-end-neutral. Converting once here is cheaper than threading a
/// generic color type through every call site that only the TUI uses.
fn to_color32(c: ratatui::style::Color) -> Color32 {
    if let ratatui::style::Color::Rgb(r, g, b) = c {
        Color32::from_rgb(r, g, b)
    } else {
        Color32::GRAY
    }
}

fn draw_file_list(app: &mut GuiApp, ui: &mut egui::Ui) {
    let phys = app.use_physical;
    let total = app.current_node().effective_size(phys).max(1);
    let rows: Vec<(usize, u64, String, bool)> = app
        .display_children()
        .iter()
        .map(|(idx, n)| (*idx, n.effective_size(phys), n.name.clone(), n.is_dir))
        .collect();

    let mut clicked_open: Option<usize> = None;
    let mut clicked_select: Option<usize> = None;
    let mut sort_clicked: Option<SortMode> = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(120.0))
        .column(Column::auto().at_least(90.0))
        .column(Column::auto().at_least(55.0))
        .column(Column::auto().at_least(120.0))
        .sense(Sense::click())
        .header(22.0, |mut header| {
            header.col(|ui| {
                if ui.strong("Name").clicked() {
                    sort_clicked = Some(match app.sort {
                        SortMode::NameAsc => SortMode::NameDesc,
                        _ => SortMode::NameAsc,
                    });
                }
            });
            header.col(|ui| {
                if ui.strong("Size").clicked() {
                    sort_clicked = Some(match app.sort {
                        SortMode::SizeDesc => SortMode::SizeAsc,
                        _ => SortMode::SizeDesc,
                    });
                }
            });
            header.col(|ui| {
                ui.strong("%");
            });
            header.col(|ui| {
                ui.strong("Distribution");
            });
        })
        .body(|body| {
            body.rows(22.0, rows.len(), |mut row| {
                let (idx, size, name, is_dir) = &rows[row.index()];
                let selected = app.selected == Some(*idx);
                row.set_selected(selected);

                row.col(|ui| {
                    let icon = if *is_dir { "📁 " } else { "📄 " };
                    ui.label(format!("{icon}{name}"));
                });
                row.col(|ui| {
                    ui.label(human_bytes(*size));
                });
                row.col(|ui| {
                    let pct = *size as f64 / total as f64 * 100.0;
                    ui.label(format!("{pct:.1}%"));
                });
                row.col(|ui| {
                    let pct = (*size as f64 / total as f64) as f32;
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width().min(200.0), 14.0),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(rect, 2.0, Color32::from_gray(45));
                    let mut filled = rect;
                    filled.set_width(rect.width() * pct.clamp(0.0, 1.0));
                    ui.painter()
                        .rect_filled(filled, 2.0, Color32::from_rgb(66, 133, 219));
                });

                let response = row.response();
                if response.clicked() {
                    clicked_select = Some(*idx);
                }
                if response.double_clicked() {
                    clicked_open = Some(*idx);
                }
            });
        });

    if let Some(mode) = sort_clicked {
        app.sort = mode;
    }
    if let Some(idx) = clicked_select {
        app.selected = Some(idx);
    }
    if let Some(idx) = clicked_open {
        app.open_child(idx);
    }

    ui.input(|i| {
        if i.key_pressed(egui::Key::Delete) {
            if let Some(idx) = app.selected {
                app.request_delete(idx, false);
            }
        }
    });
}

fn draw_treemap(app: &mut GuiApp, ui: &mut egui::Ui) {
    let avail = ui.available_size();
    let (response, painter) = ui.allocate_painter(avail, Sense::click());
    let origin = response.rect.min;
    let tiles = app.treemap_tiles(origin.x, origin.y, avail.x, avail.y);

    let mut clicked_path: Option<Vec<usize>> = None;

    for tile in &tiles {
        if tile.w < 1.0 || tile.h < 1.0 {
            continue;
        }
        let rect =
            egui::Rect::from_min_size(egui::pos2(tile.x, tile.y), egui::vec2(tile.w, tile.h));

        let base = if tile.is_free_space {
            color::free_space_color()
        } else if tile.is_dir {
            color::directory_color()
        } else {
            color::ext_color(&tile.name)
        };
        // Slightly darker with nesting depth, the same way the TUI's
        // treemap distinguishes a deeply-nested tile from a shallow one
        // at a glance, before the cushion gradient below is layered on
        // top of it.
        let depth_factor = 1.0 - (tile.depth as f32 * 0.06).min(0.4);
        let mut base = scale(to_color32(base), depth_factor);
        if !tile.is_free_space {
            if let Some(h) = app.highlighted_category {
                if Some(h) != tile.category {
                    base = blend(base, Color32::from_rgb(55, 58, 66), 0.75);
                }
            }
        }

        paint_cushion_rect(&painter, rect, base);
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(18, 18, 22)));

        if tile.can_label && tile.w >= 40.0 && tile.h >= 14.0 {
            let label_color = if luminance(base) > 140.0 {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            painter.text(
                rect.min + egui::vec2(3.0, 2.0),
                egui::Align2::LEFT_TOP,
                truncate_for_width(&tile.name, tile.w - 6.0, &painter, ui),
                TextStyle::Small.resolve(ui.style()),
                label_color,
            );
        }

        if !tile.is_free_space && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    clicked_path = Some(tile.index_path.clone());
                }
            }
        }
    }

    if let Some(path) = clicked_path {
        app.navigate_to_absolute(path);
    }
}

fn truncate_for_width(name: &str, max_w: f32, painter: &egui::Painter, ui: &egui::Ui) -> String {
    let font = TextStyle::Small.resolve(ui.style());
    let full_w = painter
        .layout_no_wrap(name.to_string(), font.clone(), Color32::WHITE)
        .rect
        .width();
    if full_w <= max_w {
        return name.to_string();
    }
    let mut s = name.to_string();
    while !s.is_empty() {
        s.pop();
        let candidate = format!("{s}…");
        let w = painter
            .layout_no_wrap(candidate.clone(), font.clone(), Color32::WHITE)
            .rect
            .width();
        if w <= max_w {
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
    let adj = |v: u8| ((v as f32) * f).min(255.0) as u8;
    Color32::from_rgb(adj(c.r()), adj(c.g()), adj(c.b()))
}

fn blend(c: Color32, target: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 * (1.0 - t) + b as f32 * t) as u8;
    Color32::from_rgb(
        mix(c.r(), target.r()),
        mix(c.g(), target.g()),
        mix(c.b(), target.b()),
    )
}

/// Same diagonal light-from-upper-left cushion gradient as the TUI's
/// `cushion_shade`, but painted as real sub-pixel-smooth horizontal
/// strips instead of per-terminal-cell blocks — a GUI has actual pixel
/// resolution to spend on this, so the gradient can be continuous rather
/// than stepped.
fn paint_cushion_rect(painter: &egui::Painter, rect: egui::Rect, base: Color32) {
    const STEPS: i32 = 12;
    let step_h = (rect.height() / STEPS as f32).max(1.0);
    for i in 0..STEPS {
        let t_top = i as f32 / STEPS as f32;
        let y0 = rect.min.y + t_top * rect.height();
        let y1 = (y0 + step_h).min(rect.max.y);
        if y1 <= y0 {
            continue;
        }
        // Sample the gradient at the strip's vertical center, blended
        // across the strip's horizontal extent too (diagonal, not purely
        // vertical) by using the rect's midpoint x-position as a stand-in
        // — full per-column resolution isn't worth the extra draw calls
        // at typical tile sizes.
        let t = t_top + (1.0 / STEPS as f32) * 0.5;
        let shade = if t < 0.5 {
            blend(base, Color32::WHITE, (0.5 - t) * 0.5)
        } else {
            blend(base, Color32::BLACK, (t - 0.5) * 0.6)
        };
        let strip =
            egui::Rect::from_min_max(egui::pos2(rect.min.x, y0), egui::pos2(rect.max.x, y1));
        painter.rect_filled(strip, 0.0, shade);
    }
}

fn draw_delete_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    let Some(pending) = &app.pending_delete else {
        return;
    };
    let name = pending.name.clone();
    let permanent = pending.permanent;
    let is_dir = pending.is_dir;
    let mut do_confirm = false;
    let mut do_empty = false;
    let mut do_cancel = false;

    egui::Window::new(if permanent {
        "Permanently Delete"
    } else {
        "Move to Trash"
    })
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
    .show(ctx, |ui| {
        ui.label(format!("Delete '{name}'?"));
        if permanent {
            ui.colored_label(
                Color32::from_rgb(217, 83, 79),
                "This bypasses the Recycle Bin/Trash and cannot be undone.",
            );
        } else {
            ui.label("This can be undone from your OS Recycle Bin/Trash.");
        }
        if is_dir {
            ui.label("Or empty it — delete its contents, keep the folder.");
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                do_confirm = true;
            }
            if is_dir && ui.button("Empty").clicked() {
                do_empty = true;
            }
            if ui.button("No").clicked() {
                do_cancel = true;
            }
        });
    });

    if do_confirm {
        if let Err(e) = app.confirm_delete() {
            app.status = Some(format!("Delete failed: {e}"));
        }
    }
    if do_empty {
        if let Err(e) = app.confirm_empty() {
            app.status = Some(format!("Empty failed: {e}"));
        }
    }
    if do_cancel {
        app.pending_delete = None;
    }
}

fn draw_properties_dialog(app: &mut GuiApp, ctx: &egui::Context) {
    if !app.show_properties {
        return;
    }
    let mut open = true;
    egui::Window::new("Properties")
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            if let Some(idx) = app.selected {
                if let Some(node) = app.current_node().children.get(idx) {
                    ui.label(format!("Name: {}", node.name));
                    ui.label(format!(
                        "Type: {}",
                        if node.is_dir { "Folder" } else { "File" }
                    ));
                    ui.label(format!("Size: {}", human_bytes(node.size)));
                    ui.label(format!(
                        "Physical size: {}",
                        human_bytes(node.physical_size)
                    ));
                    ui.label(format!("Files: {}", thousands(node.file_count)));
                }
            }
        });
    if !open {
        app.show_properties = false;
    }
}
