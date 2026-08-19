//! Flattens a directory's *entire* subtree (not just its direct children)
//! into absolutely-positioned rectangles, recursing the squarified treemap
//! layout into each directory's own allotted rectangle. This is what makes
//! the treemap show real internal structure instead of one flat blob for
//! any directory that dominates its parent.

use crate::color::Category;
use crate::model::Node;
use crate::treemap;
use crate::treemap::Rect;

pub struct TreemapItem {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub is_dir: bool,
    pub depth: u16,
    pub name: String,
    /// `None` means "directory" (directories aren't `Category`s).
    pub category: Option<Category>,
    /// Indices (original, unsorted child order) from the directory being
    /// browsed down to this item — enough to reconstruct navigation state.
    /// Empty for the synthetic free-space tile, which doesn't correspond
    /// to a real filesystem entry (`App::navigate_to` already no-ops on an
    /// empty path, so it's simply not clickable-to-navigate).
    pub index_path: Vec<usize>,
    /// True only for the synthetic "free space on this volume" tile —
    /// never a real file or directory.
    pub is_free_space: bool,
    /// False only for a directory that recursed into its children without
    /// reserving a row for its own label (tile too small to spare one) —
    /// the render side must not draw a label there even if the tile would
    /// otherwise be wide/tall enough on its own, since it would just be
    /// painted over by (or paint over) the children occupying that same
    /// space. True for files and for any directory that either didn't
    /// recurse or reserved its own row, where there's nothing to conflict
    /// with.
    pub can_label: bool,
}

// Any directory with room for at least one sub-tile keeps recursing into
// its actual files. A size *threshold* on recursion (an earlier approach)
// leaves huge swaths of the map as flat, undifferentiated directory-
// colored blocks whenever the tree is directory-heavy near the top (true
// of most real filesystems — you pass through many folders before
// reaching the files that actually take up space), even though there's
// plenty of room to keep going. The only thing gated by size now is
// whether a tile is legible enough to bother with a text label —
// recursion itself only stops when a tile is too small to hold even one
// child cell, when MAX_DEPTH is hit (a sanity backstop, not a real limit),
// or when MIN_RECURSE_AREA is hit.
//
// MIN_RECURSE_AREA matters more than it looks: MAX_ITEMS is a *global*
// counter checked in traversal order (children sorted largest-first,
// fully recursed depth-first before the next sibling starts). Without a
// per-tile floor, a single branch with a huge, deeply-fanned-out subtree
// (e.g. a cache directory with tens of thousands of tiny files) keeps
// subdividing all the way down to 1-cell slivers — each level change only
// swaps which leaf owns that one screen cell, but still costs a full
// recursive call and an item slot. On a real multi-million-file drive
// that alone can exhaust the entire item budget before a large *sibling*
// directory (equally deserving of subdivision) gets to recurse at all,
// leaving it rendered as a single flat, uninformative block — this was
// visibly reproduced on a real Windows scan where a `Documents` subtree
// next to a large `AppData` subtree came out as one solid slab. Stopping
// recursion once a tile's own area drops below a couple of cells (further
// subdivision of a literal single character cell can't show anything new
// anyway) keeps per-branch cost roughly proportional to that branch's own
// screen footprint, so one huge fanout can no longer starve everything
// drawn after it.
const MAX_DEPTH: u16 = 24;
// Matches the `item.w >= 6` legibility gate in ui.rs's render code — see
// the comment on `reserve_label` below for why these can't drift apart.
const MIN_LABEL_W: u16 = 6;
const MIN_LABEL_H: u16 = 2;
const MIN_RECURSE_AREA: u32 = 2;
const MAX_CHILDREN_PER_LEVEL: usize = 60;
const MAX_ITEMS: usize = 3000;

/// `free_space`, when given, adds one extra synthetic tile at the top
/// level only — sized alongside the real children so "used vs. free" is
/// visible at a glance, the way WinDirStat's own treemap does for a
/// whole-volume scan. It's not part of `node`'s children and never
/// recursed into.
pub fn build(
    node: &Node,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    use_physical: bool,
    free_space: Option<u64>,
) -> Vec<TreemapItem> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    recurse(
        node,
        Rect {
            x,
            y,
            w: width,
            h: height,
        },
        0,
        use_physical,
        free_space,
        &mut path,
        &mut out,
    );
    out
}

fn recurse(
    node: &Node,
    area: Rect,
    depth: u16,
    use_physical: bool,
    free_space: Option<u64>,
    index_path: &mut Vec<usize>,
    out: &mut Vec<TreemapItem>,
) {
    if area.w == 0 || area.h == 0 || out.len() >= MAX_ITEMS {
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
    let rects = treemap::layout(&sizes, area.w, area.h);

    for (i, r) in rects.iter().enumerate() {
        if r.w == 0 || r.h == 0 || out.len() >= MAX_ITEMS {
            continue;
        }

        if i >= children.len() {
            // The synthetic free-space tile — always last, since it was
            // appended last to `sizes` above.
            out.push(TreemapItem {
                x: area.x + r.x,
                y: area.y + r.y,
                w: r.w,
                h: r.h,
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

        let area_cells = u32::from(r.w) * u32::from(r.h);
        let recurses = child.is_dir && depth + 1 < MAX_DEPTH && area_cells >= MIN_RECURSE_AREA;
        // Only reserve a row for the directory's own label if the tile is
        // big enough for that label to actually render (matches the w>=6
        // render-time gate in ui.rs — MIN_LABEL_W here isn't an
        // independent guess, since a mismatch either wastes a row
        // reserved for a label that never gets drawn, or lets a label get
        // drawn into a row children then recurse into and paint over)
        // — otherwise give the whole tile to its children instead of
        // wasting a row on text nobody could read anyway.
        let reserve_label = r.w >= MIN_LABEL_W && r.h >= MIN_LABEL_H;

        out.push(TreemapItem {
            x: area.x + r.x,
            y: area.y + r.y,
            w: r.w,
            h: r.h,
            is_dir: child.is_dir,
            depth,
            name: child.name.clone(),
            category: child.category,
            index_path: index_path.clone(),
            is_free_space: false,
            can_label: !recurses || reserve_label,
        });

        if recurses {
            let inner = if reserve_label {
                Rect {
                    x: area.x + r.x,
                    y: area.y + r.y + 1,
                    w: r.w,
                    h: r.h - 1,
                }
            } else {
                Rect {
                    x: area.x + r.x,
                    y: area.y + r.y,
                    w: r.w,
                    h: r.h,
                }
            };
            // Free space is only ever shown once, at the top level.
            recurse(child, inner, depth + 1, use_physical, None, index_path, out);
        }

        index_path.pop();
    }
}
