//! A squarified treemap layout (Bruls, Huizing, van Wijk), operating on
//! integer terminal cells. Given a list of relative sizes and a target
//! width/height, returns one rectangle per input, in the same order, tiling
//! the target area with minimal-aspect-ratio rectangles.

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy)]
struct FRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

pub fn layout(values: &[u64], width: u16, height: u16) -> Vec<Rect> {
    if values.is_empty() || width == 0 || height == 0 {
        return values.iter().map(|_| Rect { x: 0, y: 0, w: 0, h: 0 }).collect();
    }
    let total: f64 = values.iter().map(|&v| v.max(1) as f64).sum();
    let area = width as f64 * height as f64;
    let scaled: Vec<f64> = values.iter().map(|&v| (v.max(1) as f64 / total) * area).collect();

    let mut out = Vec::with_capacity(values.len());
    squarify(&scaled, FRect { x: 0.0, y: 0.0, w: width as f64, h: height as f64 }, &mut out);

    out.into_iter()
        .map(|r| {
            let x = r.x.round().clamp(0.0, width as f64) as u16;
            let y = r.y.round().clamp(0.0, height as f64) as u16;
            let w = r.w.round().max(0.0).min(width as f64 - x as f64) as u16;
            let h = r.h.round().max(0.0).min(height as f64 - y as f64) as u16;
            Rect { x, y, w, h }
        })
        .collect()
}

fn squarify(values: &[f64], rect: FRect, out: &mut Vec<FRect>) {
    if values.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    if values.len() == 1 {
        out.push(rect);
        return;
    }

    let side = rect.w.min(rect.h);
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

    if rect.w >= rect.h {
        let strip_w = (row_sum / rect.h).min(rect.w);
        let mut cy = rect.y;
        for &v in row {
            let ih = rect.h * (v / row_sum);
            out.push(FRect { x: rect.x, y: cy, w: strip_w, h: ih });
            cy += ih;
        }
        squarify(
            &values[i..],
            FRect { x: rect.x + strip_w, y: rect.y, w: (rect.w - strip_w).max(0.0), h: rect.h },
            out,
        );
    } else {
        let strip_h = (row_sum / rect.w).min(rect.h);
        let mut cx = rect.x;
        for &v in row {
            let iw = rect.w * (v / row_sum);
            out.push(FRect { x: cx, y: rect.y, w: iw, h: strip_h });
            cx += iw;
        }
        squarify(
            &values[i..],
            FRect { x: rect.x, y: rect.y + strip_h, w: rect.w, h: (rect.h - strip_h).max(0.0) },
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
