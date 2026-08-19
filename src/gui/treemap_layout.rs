//! Flattens a directory's entire subtree into absolutely-positioned pixel
//! rectangles, the same way `tui::nested` does for terminal cells — see
//! that module's doc comment for why recursion happens down to whatever
//! unit is being tiled rather than stopping at direct children. This is a
//! separate implementation (not a shared one) because the two front ends
//! tile fundamentally different units at fundamentally different scales:
//! a terminal cell is ~1/6000th of a typical panel's area and needs a
//! coarse recursion floor and a character-count label gate, while a pixel
//! is a couple of orders of magnitude smaller still and needs its own
//! floor tuned so a real window doesn't recurse into millions of
//! sub-pixel tiles. Both call the same underlying `crate::treemap::layout`
//! squarify algorithm — only the recursion policy around it differs.

use crate::color::Category;
use crate::model::Node;
use crate::treemap;

pub struct Tile {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub is_dir: bool,
    pub depth: u32,
    pub name: String,
    pub category: Option<Category>,
    pub index_path: Vec<usize>,
    pub is_free_space: bool,
    /// See `tui::nested::TreemapItem::can_label` — same reasoning: a tile
    /// that recursed into children without reserving a label row must not
    /// draw one, or the children painted after it cover the tile's own
    /// name (or the name covers them, depending on draw order).
    pub can_label: bool,
}

const MAX_DEPTH: u32 = 32;
/// A tile has to be at least this many pixels on a side to reserve a
/// label row for its own name before recursing into children — well
/// above the ~14px a single line of UI text needs, so a reserved row is
/// never narrower than what will actually render into it.
const MIN_LABEL_PX: f32 = 18.0;
/// Recursion stops once a tile's pixel area drops below this — a tile
/// smaller than a handful of pixels on a side can't show anything a user
/// could distinguish anyway, and without a floor a deeply-fanned-out
/// subtree (thousands of tiny files) would recurse effectively forever.
const MIN_RECURSE_AREA_PX: f32 = 9.0;
const MAX_CHILDREN_PER_LEVEL: usize = 80;
const MAX_TILES: usize = 6000;

pub fn build(
    node: &Node,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    use_physical: bool,
    free_space: Option<u64>,
) -> Vec<Tile> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    recurse(
        node,
        x,
        y,
        width,
        height,
        0,
        use_physical,
        free_space,
        &mut path,
        &mut out,
    );
    out
}

#[allow(clippy::too_many_arguments)]
fn recurse(
    node: &Node,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    depth: u32,
    use_physical: bool,
    free_space: Option<u64>,
    index_path: &mut Vec<usize>,
    out: &mut Vec<Tile>,
) {
    if w <= 0.0 || h <= 0.0 || out.len() >= MAX_TILES {
        return;
    }
    if node.children.is_empty() && free_space.is_none() {
        return;
    }

    let mut children: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
    children.sort_by(|a, b| {
        b.1.effective_size(use_physical)
            .cmp(&a.1.effective_size(use_physical))
    });
    children.truncate(MAX_CHILDREN_PER_LEVEL);

    let mut sizes: Vec<u64> = children
        .iter()
        .map(|(_, c)| c.effective_size(use_physical).max(1))
        .collect();
    if let Some(fs) = free_space {
        sizes.push(fs.max(1));
    }
    // The shared squarify algorithm works in an abstract integer grid;
    // pixel dimensions comfortably fit u16 for any realistic window size,
    // and quantizing to whole pixels here (rather than threading f32
    // through the algorithm) is what makes the earlier gap/overlap fix in
    // `treemap::layout` actually apply — that fix guarantees adjacent
    // *integer* cell boundaries never disagree, which only matters if
    // this is the resolution being rounded to in the first place.
    let px_w = w.round().clamp(0.0, u16::MAX as f32) as u16;
    let px_h = h.round().clamp(0.0, u16::MAX as f32) as u16;
    let rects = treemap::layout(&sizes, px_w, px_h);

    for (i, r) in rects.iter().enumerate() {
        if r.w == 0 || r.h == 0 || out.len() >= MAX_TILES {
            continue;
        }
        let (rx, ry, rw, rh) = (x + r.x as f32, y + r.y as f32, r.w as f32, r.h as f32);

        if i >= children.len() {
            out.push(Tile {
                x: rx,
                y: ry,
                w: rw,
                h: rh,
                is_dir: false,
                depth,
                name: "Free space".to_string(),
                category: None,
                index_path: vec![],
                is_free_space: true,
                can_label: true,
            });
            continue;
        }

        let (orig_idx, child) = children[i];
        index_path.push(orig_idx);

        let area = rw * rh;
        let recurses = child.is_dir && depth + 1 < MAX_DEPTH && area >= MIN_RECURSE_AREA_PX;
        let reserve_label = rw >= MIN_LABEL_PX && rh >= MIN_LABEL_PX;

        out.push(Tile {
            x: rx,
            y: ry,
            w: rw,
            h: rh,
            is_dir: child.is_dir,
            depth,
            name: child.name.clone(),
            category: child.category,
            index_path: index_path.clone(),
            is_free_space: false,
            can_label: !recurses || reserve_label,
        });

        if recurses {
            let (iy, ih) = if reserve_label {
                (ry + MIN_LABEL_PX * 0.8, rh - MIN_LABEL_PX * 0.8)
            } else {
                (ry, rh)
            };
            recurse(
                child,
                rx,
                iy,
                rw,
                ih,
                depth + 1,
                use_physical,
                None,
                index_path,
                out,
            );
        }

        index_path.pop();
    }
}
