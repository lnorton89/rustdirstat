// ============================================================================
// Module:       treemap
// Description:  The squarified treemap layout (Bruls, Huizing, van Wijk) over
//               an abstract integer grid, shared by both front ends.
//
// Dependencies: None; std only.
// ============================================================================

//! A squarified treemap layout (Bruls, Huizing, van Wijk), operating on
//! an abstract integer grid — terminal cells for the TUI, pixels for the
//! GUI. Given a list of relative sizes and a target width/height, returns
//! one rectangle per input, in the same order, tiling the target area with
//! minimal-aspect-ratio rectangles. Shared by both front ends rather than
//! duplicated, since the layout math itself has nothing to do with which
//! one is rendering the result.

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Tracked as edges (`x0..x1`, `y0..y1`) rather than `x, y, w, h` during
/// layout — see the comment in `layout()`'s final rounding step for why
/// that's the part that actually matters.
#[derive(Debug, Clone, Copy)]
struct FRect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl FRect {
    fn w(&self) -> f64 {
        self.x1 - self.x0
    }
    fn h(&self) -> f64 {
        self.y1 - self.y0
    }
}

pub fn layout(values: &[u64], width: u16, height: u16) -> Vec<Rect> {
    if values.is_empty() || width == 0 || height == 0 {
        return values
            .iter()
            .map(|_| Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            })
            .collect();
    }
    let total: f64 = values.iter().map(|&v| v.max(1) as f64).sum();
    let area = width as f64 * height as f64;
    let scaled: Vec<f64> = values
        .iter()
        .map(|&v| (v.max(1) as f64 / total) * area)
        .collect();

    let mut out = Vec::with_capacity(values.len());
    squarify(
        &scaled,
        FRect {
            x0: 0.0,
            y0: 0.0,
            x1: width as f64,
            y1: height as f64,
        },
        &mut out,
    );

    // Round each rectangle's *edges* to integer cell boundaries, rather
    // than rounding its width/height independently of its position.
    // Adjacent siblings within a strip — and a strip's far edge against
    // the next strip's near edge — are built from the exact same float
    // coordinate (`cy`/`cx` handed forward unchanged, see `squarify`
    // below), so rounding that shared edge value once and deriving each
    // side's width/height from the rounded pair guarantees neighboring
    // tiles always meet at an exact integer boundary. Rounding width and
    // position separately (the previous approach) doesn't: round(a) +
    // round(b) isn't always round(a + b), so two tiles that shared an
    // exact float edge could round to leave a gap cell between them (an
    // uncolored sliver with no tile drawn into it) or to overlap by a
    // cell (the later-drawn tile's paint silently covering the earlier
    // one). Confirmed empirically — fuzzing the old code over random
    // value sets and target sizes produced a gap or overlap in roughly
    // half of all trials, including the simplest cases (three equal-size
    // items in a narrow column).
    out.into_iter()
        .map(|r| {
            let x0 = r.x0.round().clamp(0.0, width as f64);
            let y0 = r.y0.round().clamp(0.0, height as f64);
            let x1 = r.x1.round().clamp(x0, width as f64);
            let y1 = r.y1.round().clamp(y0, height as f64);
            Rect {
                x: x0 as u16,
                y: y0 as u16,
                w: (x1 - x0) as u16,
                h: (y1 - y0) as u16,
            }
        })
        .collect()
}

fn squarify(values: &[f64], rect: FRect, out: &mut Vec<FRect>) {
    let w = rect.w();
    let h = rect.h();
    if values.is_empty() || w <= 0.0 || h <= 0.0 {
        return;
    }
    if values.len() == 1 {
        out.push(rect);
        return;
    }

    let side = w.min(h);
    let mut i = 1;
    while i < values.len() {
        let r_i = worst_ratio(&values[0..i], side);
        let r_i1 = worst_ratio(&values[0..i + 1], side);
        if r_i1 <= r_i {
            i += 1;
        } else {
            break;
        }
    }

    let row = &values[0..i];
    let row_sum: f64 = row.iter().sum();

    if w >= h {
        let strip_w = (row_sum / h).min(w);
        let strip_x1 = rect.x0 + strip_w;
        let mut cy = rect.y0;
        for &v in row {
            let y1 = cy + h * (v / row_sum);
            out.push(FRect {
                x0: rect.x0,
                y0: cy,
                x1: strip_x1,
                y1,
            });
            cy = y1;
        }
        squarify(
            &values[i..],
            FRect {
                x0: strip_x1,
                y0: rect.y0,
                x1: rect.x1,
                y1: rect.y1,
            },
            out,
        );
    } else {
        let strip_h = (row_sum / w).min(h);
        let strip_y1 = rect.y0 + strip_h;
        let mut cx = rect.x0;
        for &v in row {
            let x1 = cx + w * (v / row_sum);
            out.push(FRect {
                x0: cx,
                y0: rect.y0,
                x1,
                y1: strip_y1,
            });
            cx = x1;
        }
        squarify(
            &values[i..],
            FRect {
                x0: rect.x0,
                y0: strip_y1,
                x1: rect.x1,
                y1: rect.y1,
            },
            out,
        );
    }
}

/// Worst (largest) aspect ratio among rectangles that would result from
/// laying `row` out as a strip of fixed length `side`.
fn worst_ratio(row: &[f64], side: f64) -> f64 {
    if side <= 0.0 {
        return f64::INFINITY;
    }
    let row_sum: f64 = row.iter().sum();
    if row_sum <= 0.0 {
        return f64::INFINITY;
    }
    let a = row_sum / side;
    let mut worst = 1.0f64;
    for &v in row {
        let b = side * v / row_sum;
        let r = if a > b { a / b } else { b / a };
        if r > worst {
            worst = r;
        }
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Total area covered, counting any overlap twice — so a sum larger
    /// than the panel is itself evidence of overlap.
    fn covered(rects: &[Rect]) -> u32 {
        rects.iter().map(|r| u32::from(r.w) * u32::from(r.h)).sum()
    }

    fn overlaps(a: &Rect, b: &Rect) -> bool {
        let ax2 = u32::from(a.x) + u32::from(a.w);
        let ay2 = u32::from(a.y) + u32::from(a.h);
        let bx2 = u32::from(b.x) + u32::from(b.w);
        let by2 = u32::from(b.y) + u32::from(b.h);
        u32::from(a.x) < bx2 && u32::from(b.x) < ax2 && u32::from(a.y) < by2 && u32::from(b.y) < ay2
    }

    /// The three properties a treemap has to have, over a spread of
    /// shapes and value distributions.
    ///
    /// Asserted together over many inputs rather than as three separate
    /// hand-picked cases: the interesting failures are at particular
    /// aspect ratios and particular value spreads, and which ones those
    /// are is exactly what nobody knows in advance.
    #[test]
    fn tiles_tile_the_panel_without_gaps_or_overlaps() {
        let shapes = [
            (40_u16, 20_u16),
            (1, 50),
            (50, 1),
            (7, 13),
            (200, 120),
            (3, 3),
        ];
        let distributions: [&[u64]; 6] = [
            &[1],
            &[1, 1, 1, 1],
            &[100, 1, 1, 1, 1],
            &[5, 4, 3, 2, 1],
            &[1_000_000, 999_999, 2, 1],
            &[7; 32],
        ];

        for (width, height) in shapes {
            for values in distributions {
                let rects = layout(values, width, height);
                assert_eq!(
                    rects.len(),
                    values.len(),
                    "every value must get a rect ({width}x{height}, {values:?})"
                );

                for (i, r) in rects.iter().enumerate() {
                    assert!(
                        u32::from(r.x) + u32::from(r.w) <= u32::from(width)
                            && u32::from(r.y) + u32::from(r.h) <= u32::from(height),
                        "tile {i} at ({},{}) {}x{} escapes a {width}x{height} panel",
                        r.x,
                        r.y,
                        r.w,
                        r.h
                    );
                }

                for i in 0..rects.len() {
                    for j in (i + 1)..rects.len() {
                        let (a, b) = (&rects[i], &rects[j]);
                        if a.w == 0 || a.h == 0 || b.w == 0 || b.h == 0 {
                            continue;
                        }
                        assert!(
                            !overlaps(a, b),
                            "tiles {i} and {j} overlap ({width}x{height}, {values:?})"
                        );
                    }
                }

                // Every cell accounted for. Rounding is done on tile
                // *edges* precisely so neighbours cannot round into a gap
                // or an overlap, which is what this pins.
                assert_eq!(
                    covered(&rects),
                    u32::from(width) * u32::from(height),
                    "tiles should cover the whole {width}x{height} panel ({values:?})"
                );
            }
        }
    }

    /// A bigger value gets a bigger tile.
    ///
    /// Not exact proportionality — integer cells cannot deliver that —
    /// but the ordering has to hold, or the picture is lying about which
    /// directory is larger, which is the one thing the view exists to
    /// say.
    #[test]
    fn a_larger_value_never_gets_a_smaller_tile() {
        let values = [1_000_u64, 500, 250, 125, 60, 30, 15, 8, 4, 2, 1];
        let rects = layout(&values, 160, 100);
        let areas: Vec<u32> = rects
            .iter()
            .map(|r| u32::from(r.w) * u32::from(r.h))
            .collect();

        for i in 1..areas.len() {
            assert!(
                areas[i] <= areas[i - 1],
                "value {} got {}px but the larger value {} got only {}px",
                values[i],
                areas[i],
                values[i - 1],
                areas[i - 1]
            );
        }
    }

    /// Degenerate inputs produce empty rects rather than panicking or
    /// dividing by zero.
    #[test]
    fn empty_and_zero_sized_inputs_are_handled() {
        assert!(layout(&[], 10, 10).is_empty(), "no values, no rects");

        for (w, h) in [(0_u16, 10_u16), (10, 0), (0, 0)] {
            let rects = layout(&[1, 2, 3], w, h);
            assert_eq!(rects.len(), 3, "a rect per value even at {w}x{h}");
            assert!(
                rects.iter().all(|r| r.w == 0 || r.h == 0),
                "a zero-sized panel cannot hold a tile with area"
            );
        }

        // All-zero values must not divide by zero.
        let rects = layout(&[0, 0, 0], 20, 10);
        assert_eq!(rects.len(), 3);
    }
}
