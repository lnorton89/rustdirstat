// ============================================================================
// Module:       tui::ui::treemap
// Description:  The TUI treemap widget, the cushion shading that gives tiles
//               their rounded look, and the splitter handle beside it.
//
// Dependencies: ratatui; super (the shared drawing imports),
//               crate::tui::nested
// ============================================================================

//! The TUI treemap: its widget, the cushion shading that gives tiles
//! their rounded look, and the splitter handle beside it.

use super::*;

/// A permanently-visible "grab here" bar over the panel divider — bright
/// accent color, distinct from the ordinary panel border either side of it.
pub(super) fn draw_resize_handle(f: &mut Frame, x: u16, y: u16, height: u16) {
    if height == 0 {
        return;
    }
    let mid = height / 2;
    let lines: Vec<Line> = (0..height)
        .map(|row| {
            // A short dotted grip glyph at the vertical center reads as
            // "drag" more clearly than a plain solid line would.
            let glyph = if row == mid || row == mid.saturating_sub(1) || row == mid + 1 {
                "┃"
            } else {
                "│"
            };
            Line::from(Span::styled(
                glyph,
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines),
        Rect {
            x,
            y,
            width: 1,
            height,
        },
    );
}

pub(super) fn draw_treemap(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::border_type())
        .border_style(theme::panel_border(false))
        .title(Span::styled(
            " Treemap — click a tile to jump to it  ·  drag the left edge to resize ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.click_zones.push(ClickZone {
        x: area.x,
        y: area.y,
        w: area.width,
        h: 1,
        action: Action::ToggleTreemap,
    });

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Free space only makes sense relative to the whole volume, so it's
    // only shown when the scan root IS the volume root — not for every
    // scan just because you happen to be browsing at its top (a small
    // subfolder's content compared against gigabytes of unrelated free
    // space elsewhere on the drive would swamp the treemap with a
    // free-space tile representing almost the entire area), and not
    // injected into subfolder views either, where it wouldn't correspond
    // to anything real.
    let free_space = if app.path_indices.is_empty() && app.tree.is_volume_root() {
        app.tree.volume_free
    } else {
        None
    };
    let items = nested::build(
        app.current_node(),
        inner.x,
        inner.y,
        inner.width,
        inner.height,
        app.use_physical,
        free_space,
    );
    let selected_orig = app
        .display_children()
        .get(app.selected)
        .map(|(idx, _)| *idx);

    let widget = TreemapWidget {
        items: &items,
        selected_orig,
        highlighted: app.highlighted_category,
    };
    f.render_widget(widget, inner);

    for item in &items {
        if item.is_free_space {
            continue; // not a real entry — nothing to navigate to
        }
        app.click_zones.push(ClickZone {
            x: item.x,
            y: item.y,
            w: item.w,
            h: item.h,
            action: Action::NavigateTo(item.index_path.clone()),
        });
    }
}

pub(super) struct TreemapWidget<'a> {
    items: &'a [TreemapItem],
    selected_orig: Option<usize>,
    highlighted: Option<Category>,
}

impl<'a> Widget for TreemapWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, item) in self.items.iter().enumerate() {
            if item.w == 0 || item.h == 0 {
                continue;
            }
            let base = if item.is_free_space {
                color::free_space_color()
            } else if item.is_dir {
                color::directory_color()
            } else {
                color::ext_color(&item.name)
            };
            // Cushion-style shading: darker with depth, alternated by
            // sibling index, so nested rectangles read as distinct tiles.
            let depth_factor = 1.0 - (item.depth as f32 * 0.09).min(0.55);
            let parity = if i % 2 == 0 { 1.0 } else { 0.88 };
            let mut bg = lighten(base, depth_factor * parity);
            if !item.is_free_space {
                if let Some(h) = self.highlighted {
                    if Some(h) != item.category {
                        // Blend toward theme::DIM rather than replacing
                        // the tile outright — a flat gray recreates the
                        // "everything looks the same" flatness that
                        // per-extension coloring was built to fix, for
                        // every non-matching tile at once. Keeping a
                        // little of the real hue means shape and rough
                        // color still read while the highlighted set
                        // clearly stands out.
                        bg = blend_toward(bg, theme::DIM, 0.75);
                    }
                }
            }

            // Per-cell "cushion" gradient on top of the tile's own flat
            // base color — a diagonal highlight near the upper-left corner
            // fading to a shadow near the lower-right, the light-from-one-
            // corner effect that gives WinDirStat's treemap its
            // recognizable slightly-3D look. Without this, every tile is a
            // single flat poster-paint rectangle; this is a cheap
            // approximation (no real lighting model, just a brightness
            // gradient) but reads the same way at terminal-cell
            // resolution.
            for yy in 0..item.h {
                for xx in 0..item.w {
                    let px = item.x + xx;
                    let py = item.y + yy;
                    if px >= area.x + area.width || py >= area.y + area.height {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((px, py)) {
                        cell.set_symbol(" ");
                        cell.set_bg(cushion_shade(bg, xx, yy, item.w, item.h));
                    }
                }
            }

            // Every tile gets a dark separator border so adjacent tiles of
            // the same (or similar) color still read as distinct pieces —
            // without this, same-category siblings visually merge into one
            // shapeless blob.
            draw_border(buf, area, item.x, item.y, item.w, item.h, theme::SHADOW);

            let is_selected = item.depth == 0
                && item.index_path.len() == 1
                && Some(item.index_path[0]) == self.selected_orig;
            if is_selected {
                draw_border(
                    buf,
                    area,
                    item.x,
                    item.y,
                    item.w,
                    item.h,
                    theme::SELECTED_BORDER,
                );
            }

            // A dense tree (a build output directory, node_modules, ...)
            // recurses into thousands of small tiles — that's real data,
            // not a bug, and each one still needs its own color to be
            // accurate. But labeling every single one, down to a 3-cell
            // sliver, turns the treemap into illegible fragments like
            // "bu…"/"qu…" that add noise without conveying anything —
            // worse than no label, since it reads as clutter rather than
            // as missing information. WinDirStat doesn't label tiles it
            // can't fit a real name into either; small tiles just show
            // their color. 6 cells is enough for a short full name (e.g.
            // "src/") or a handful of real characters before the ellipsis
            // — below that, skip the label rather than draw noise.
            if item.can_label && item.w >= 6 && item.h >= 1 {
                let label = truncate(&item.name, item.w as usize - 1);
                // Matches the cushion shade the fill loop above just wrote
                // into the label's own starting cell (top-left corner,
                // xx=0/yy=0) — using the flat `bg` here instead would draw
                // the label on a small rectangle of the tile's unshaded
                // color, visibly breaking the gradient right where text
                // sits.
                let label_bg = cushion_shade(bg, 0, 0, item.w, item.h);
                let style = Style::default().fg(contrast_fg(label_bg)).bg(label_bg);
                let style = if item.is_dir {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                };
                buf.set_string(item.x, item.y, &label, style);
            }
        }
    }
}

pub(super) fn draw_border(buf: &mut Buffer, area: Rect, x: u16, y: u16, w: u16, h: u16, c: Color) {
    if w == 0 || h == 0 {
        return;
    }
    for xx in 0..w {
        let px = x + xx;
        for &py in &[y, y + h.saturating_sub(1)] {
            if px < area.x + area.width && py < area.y + area.height {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_fg(c);
                }
            }
        }
    }
    for yy in 0..h {
        let py = y + yy;
        for &px in &[x, x + w.saturating_sub(1)] {
            if px < area.x + area.width && py < area.y + area.height {
                if let Some(cell) = buf.cell_mut((px, py)) {
                    cell.set_fg(c);
                }
            }
        }
    }
}

pub(super) fn lighten(c: Color, factor: f32) -> Color {
    if let Color::Rgb(r, g, b) = c {
        let f = factor.clamp(0.0, 1.5);
        let adj = |v: u8| ((v as f32) * f).min(255.0) as u8;
        Color::Rgb(adj(r), adj(g), adj(b))
    } else {
        c
    }
}

/// Mixes `c` toward `target` by `t` (0 = unchanged, 1 = fully `target`).
pub(super) fn blend_toward(c: Color, target: Color, t: f32) -> Color {
    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (c, target) {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t) as u8;
        Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
    } else {
        target
    }
}

/// Diagonal brightness gradient across one tile's cells — light near the
/// top-left corner (`x=0, y=0`), dark near the bottom-right — approximating
/// WinDirStat's "cushion" tile shading. `w`/`h` are the tile's own
/// dimensions, so this is independent of the tile's absolute screen
/// position.
pub(super) fn cushion_shade(base: Color, x: u16, y: u16, w: u16, h: u16) -> Color {
    let nx = if w > 1 {
        f32::from(x) / f32::from(w - 1)
    } else {
        0.0
    };
    let ny = if h > 1 {
        f32::from(y) / f32::from(h - 1)
    } else {
        0.0
    };
    let t = (nx + ny) / 2.0; // 0.0 at the top-left corner, 1.0 at bottom-right
                             // `blend_toward` only blends between two `Color::Rgb` values — passing
                             // the named `Color::White`/`Color::Black` variants instead would fail
                             // its pattern match and return the target color unchanged (flat white
                             // or flat black, not a gradient), so the endpoints are spelled out as
                             // Rgb here even though they're pure white/black.
    if t < 0.5 {
        blend_toward(base, Color::Rgb(255, 255, 255), (0.5 - t) * 0.5)
    } else {
        blend_toward(base, Color::Rgb(0, 0, 0), (t - 0.5) * 0.6)
    }
}

pub(super) fn contrast_fg(bg: Color) -> Color {
    if let Color::Rgb(r, g, b) = bg {
        let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        if luminance > 140.0 {
            Color::Black
        } else {
            Color::White
        }
    } else {
        Color::White
    }
}
