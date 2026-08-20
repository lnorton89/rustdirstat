// ============================================================================
// Module:       brand
// Description:  The RustDirStat mark, defined once as geometry and colour and
//               rasterised on demand — window icon, in-app mark, README art.
//
// Dependencies: none
// ============================================================================

//! The application mark: a four-tile treemap in a rounded frame.
//!
//! The mark exists in three places — the window and taskbar icon, the
//! vector copy painted beside the product name inside the GUI, and the
//! PNGs the README shows — and all three are drawn from the [`TILES`]
//! table below rather than from three hand-matched copies. A mark that
//! disagrees with itself across the title bar, the About card and the
//! project page is the failure this is written to prevent.
//!
//! It lives at the crate root, not under `gui`, for the same reason
//! `color` does: it is not the desktop front end's private business.
//! `gui::icons` paints it as egui primitives, and the asset generator in
//! `examples/brand_assets.rs` rasterises it at whatever size a README or
//! a platform icon bundle wants.
//!
//! The colours are deliberately *not* theme colours. Everything else the
//! GUI paints comes from `palette()` and changes with the active theme,
//! because it is interface; the mark is identity, and an application
//! whose logo restyles itself under a dark theme does not have a logo.

/// The frame the tiles sit in, and what shows through the gutters
/// between them.
pub const FRAME: [u8; 3] = [115, 178, 255];

/// The tiles of the mark, as `(x0, y0, x1, y1, rgb)` fractions of the
/// interior box — so one table serves every size the mark is drawn at.
///
/// The layout is a squarified treemap in miniature: one tall tile, one
/// short tile beside it, and the split running the other way in the
/// second column. That alternation is what the real treemap does with a
/// directory of mixed sizes, and it is what stops the mark from reading
/// as a generic four-square grid.
pub const TILES: [(f32, f32, f32, f32, [u8; 3]); 4] = [
    (0.00, 0.00, 0.46, 0.54, [55, 129, 229]),
    (0.50, 0.00, 1.00, 0.34, [71, 194, 137]),
    (0.50, 0.38, 1.00, 1.00, [239, 168, 67]),
    (0.00, 0.58, 0.46, 1.00, [168, 92, 216]),
];

/// Corner radius of the frame, as a fraction of the mark's width.
pub const CORNER: f32 = 0.094;

/// Inset from the frame's edge to the tile box, as a fraction of the
/// mark's width. This is the visible width of the frame.
pub const INSET: f32 = 0.078;

/// Rasterises the mark at `size` × `size` as straight RGBA8.
///
/// Pixel coverage is sampled rather than snapped: a tile edge or a
/// rounded corner that falls between two pixels contributes partial
/// alpha, which is what keeps a 32-pixel icon from looking like it was
/// drawn with a brick. `size` of zero yields an empty buffer rather than
/// an error — there is nothing a caller could usefully do with a failure
/// here that it could not do with an empty image.
pub fn rgba(size: usize) -> Vec<u8> {
    const SAMPLES: usize = 4;

    let mut pixels = vec![0_u8; size * size * 4];
    let extent = size as f32;
    let radius = CORNER * extent;
    let inset = INSET * extent;
    let interior = extent - 2.0 * inset;

    for y in 0..size {
        for x in 0..size {
            // Accumulate premultiplied colour over the sub-samples, so a
            // pixel straddling two tiles ends up with the mix of both
            // rather than whichever one the centre happened to land in.
            let mut sum = [0.0_f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    if !inside_rounded_square(px, py, extent, radius) {
                        continue;
                    }
                    let color = tile_at((px - inset) / interior, (py - inset) / interior);
                    sum[0] += color[0] as f32;
                    sum[1] += color[1] as f32;
                    sum[2] += color[2] as f32;
                    sum[3] += 1.0;
                }
            }
            if sum[3] == 0.0 {
                continue;
            }
            let total = (SAMPLES * SAMPLES) as f32;
            let offset = (y * size + x) * 4;
            pixels[offset] = (sum[0] / sum[3]) as u8;
            pixels[offset + 1] = (sum[1] / sum[3]) as u8;
            pixels[offset + 2] = (sum[2] / sum[3]) as u8;
            pixels[offset + 3] = (255.0 * sum[3] / total) as u8;
        }
    }
    pixels
}

/// Whether a point lies inside the frame's rounded square.
fn inside_rounded_square(x: f32, y: f32, extent: f32, radius: f32) -> bool {
    // Distance from the corner arc's centre, for whichever corner this
    // point is nearest; a point outside the corner box is trivially in.
    let dx = (radius - x).max(x - (extent - radius)).max(0.0);
    let dy = (radius - y).max(y - (extent - radius)).max(0.0);
    dx * dx + dy * dy <= radius * radius
}

/// The colour at a point given in interior fractions — a tile's, or the
/// frame's where it falls in a gutter or outside the tile box entirely.
fn tile_at(x: f32, y: f32) -> [u8; 3] {
    for &(x0, y0, x1, y1, color) in &TILES {
        if x >= x0 && x < x1 && y >= y0 && y < y1 {
            return color;
        }
    }
    FRAME
}

#[cfg(test)]
mod tests {
    use super::{rgba, tile_at, FRAME, INSET, TILES};

    #[test]
    fn the_tiles_do_not_overlap_and_leave_gutters_between_them() {
        for (i, &(ax0, ay0, ax1, ay1, _)) in TILES.iter().enumerate() {
            assert!(
                ax0 < ax1 && ay0 < ay1,
                "tile {i} is inside out: {ax0},{ay0} to {ax1},{ay1}"
            );
            assert!(
                ax0 >= 0.0 && ay0 >= 0.0 && ax1 <= 1.0 && ay1 <= 1.0,
                "tile {i} leaves the interior box"
            );
            for &(bx0, by0, bx1, by1, _) in &TILES[..i] {
                let overlaps = ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1;
                assert!(!overlaps, "tile {i} overlaps an earlier tile");
            }
        }
    }

    #[test]
    fn every_tile_is_a_different_colour_from_the_frame_and_its_neighbours() {
        // Four tiles reading as four distinct areas is the whole content
        // of the mark; two that match would render it as three.
        for (i, &(_, _, _, _, color)) in TILES.iter().enumerate() {
            assert_ne!(color, FRAME, "tile {i} is the frame colour");
            for &(_, _, _, _, other) in &TILES[..i] {
                assert_ne!(color, other, "tile {i} repeats an earlier tile's colour");
            }
        }
    }

    #[test]
    fn a_gutter_shows_the_frame_rather_than_a_neighbouring_tile() {
        // The gutters are what separates the tiles; if a point between
        // two of them resolves to either one, they have merged.
        assert_eq!(tile_at(0.48, 0.20), FRAME, "the column gutter has closed");
        assert_eq!(tile_at(0.20, 0.56), FRAME, "the left row gutter has closed");
        assert_eq!(
            tile_at(0.75, 0.36),
            FRAME,
            "the right row gutter has closed"
        );
    }

    #[test]
    fn the_mark_is_opaque_in_the_middle_and_clear_at_the_corners() {
        const SIZE: usize = 64;
        let pixels = rgba(SIZE);
        assert_eq!(pixels.len(), SIZE * SIZE * 4);

        let alpha_at = |x: usize, y: usize| pixels[(y * SIZE + x) * 4 + 3];
        assert_eq!(
            alpha_at(SIZE / 2, SIZE / 2),
            255,
            "the centre is not opaque"
        );
        assert_eq!(alpha_at(0, 0), 0, "the rounded corner was not cut away");
        assert_eq!(alpha_at(SIZE - 1, SIZE - 1), 0, "the corner was not cut");
    }

    #[test]
    fn the_frame_is_visible_at_every_edge() {
        // A frame thin enough to disappear at the mark's smallest usable
        // size is not a frame, and the inset is what sets its width.
        const SIZE: usize = 32;
        let pixels = rgba(SIZE);
        let color_at = |x: usize, y: usize| {
            let offset = (y * SIZE + x) * 4;
            [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
        };
        let middle = SIZE / 2;
        for (x, y, edge) in [
            (middle, 1, "top"),
            (middle, SIZE - 2, "bottom"),
            (1, middle, "left"),
            (SIZE - 2, middle, "right"),
        ] {
            assert_eq!(color_at(x, y), FRAME, "the {edge} edge is not the frame");
        }
        // Measured rather than derived from INSET: what matters is how
        // many pixels of frame actually survive the rasteriser at the
        // mark's smallest usable size, not what the fraction implies.
        let frame_width = (0..SIZE)
            .take_while(|&x| color_at(x, middle) == FRAME)
            .count();
        assert!(
            frame_width >= 2,
            "the frame is {frame_width} pixels wide at {SIZE}"
        );
    }

    #[test]
    fn every_tile_survives_rasterisation_at_the_smallest_size() {
        // Sub-sampling averages colours, so a tile that is too small to
        // own a pixel outright vanishes into its neighbours. Each one
        // has to still be recognisably itself.
        const SIZE: usize = 32;
        let pixels = rgba(SIZE);
        let extent = SIZE as f32;
        let inset = INSET * extent;
        let interior = extent - 2.0 * inset;
        for (i, &(x0, y0, x1, y1, color)) in TILES.iter().enumerate() {
            let x = (inset + interior * (x0 + x1) / 2.0) as usize;
            let y = (inset + interior * (y0 + y1) / 2.0) as usize;
            let offset = (y * SIZE + x) * 4;
            assert_eq!(
                [pixels[offset], pixels[offset + 1], pixels[offset + 2]],
                color,
                "tile {i} is not its own colour at its own centre"
            );
        }
    }
}
