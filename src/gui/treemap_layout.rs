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
//! squarify algorithm — only the traversal policy around it differs.
//!
//! # Why the traversal is level-order, not depth-first
//!
//! Any real drive produces far more tiles than are worth drawing, so the
//! walk needs a budget. Spending that budget depth-first is what a naive
//! recursion does, and it is badly wrong: the first top-level directory
//! visited descends all the way to its leaves and consumes the entire
//! budget before its siblings are ever reached, so on a large volume the
//! right-hand side of the treemap is never emitted at all and renders as
//! blank panel. Walking level by level instead makes complete coverage an
//! invariant rather than a coincidence — a level is either drawn in full
//! or not started — and spends whatever budget is left on the largest
//! visible areas, where extra detail is actually legible, instead of on
//! whichever subtree happened to sort first.

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
    /// True for the synthetic "N more items" tile standing in for the
    /// children past `MAX_CHILDREN_PER_LEVEL`. It occupies their combined
    /// area but maps to no single node, so it is not selectable.
    pub is_aggregate: bool,
    /// See `tui::nested::TreemapItem::can_label` — same reasoning: a tile
    /// that recursed into children without reserving a label row must not
    /// draw one, or the children painted after it cover the tile's own
    /// name (or the name covers them, depending on draw order).
    pub can_label: bool,
}

impl Tile {
    /// Whether this tile stands for a real node the user can click.
    pub fn is_node(&self) -> bool {
        !self.is_free_space && !self.is_aggregate
    }
}

const MAX_DEPTH: u32 = 32;
/// A tile has to be at least this many pixels on a side to reserve a
/// label row for its own name before recursing into children — well
/// above the ~14px a single line of UI text needs, so a reserved row is
/// never narrower than what will actually render into it.
const MIN_LABEL_PX: f32 = 18.0;
/// Expansion stops once a tile's pixel area drops below this — a tile
/// smaller than a handful of pixels on a side can't show anything a user
/// could distinguish anyway, and without a floor a deeply-fanned-out
/// subtree (thousands of tiny files) would recurse effectively forever.
const MIN_RECURSE_AREA_PX: f32 = 9.0;
/// Children past this many are folded into one aggregate tile rather than
/// dropped. Dropping them would silently inflate the survivors: the
/// squarify pass normalizes against the sizes it is handed, so discarding
/// the tail makes the remaining tiles expand to fill area that isn't
/// theirs, and the treemap stops being an honest picture of the bytes.
const MAX_CHILDREN_PER_LEVEL: usize = 80;
/// Soft ceiling on emitted tiles, checked *between* levels and never
/// inside one, so that any level which starts also finishes.
const MAX_TILES: usize = 24_000;

/// One directory queued to be subdivided into the rect it was given.
struct Pending<'a> {
    node: &'a Node,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    depth: u32,
    index_path: Vec<usize>,
}

impl Pending<'_> {
    fn area(&self) -> f32 {
        self.w * self.h
    }

    /// Upper bound on the tiles that expanding this node can emit: it
    /// cannot exceed the child count, and it cannot exceed the pixel area
    /// either, since sub-pixel rects round away to nothing and are skipped.
    fn tile_cost(&self) -> usize {
        let children = self.node.children.len().min(MAX_CHILDREN_PER_LEVEL) + 1;
        children.min(self.area().max(0.0) as usize + 1)
    }
}

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
    let mut level = vec![Pending {
        node,
        x,
        y,
        w: width,
        h: height,
        depth: 0,
        index_path: Vec::new(),
    }];

    while !level.is_empty() && out.len() < MAX_TILES {
        let mut next = Vec::new();
        for pending in &level {
            expand(pending, use_physical, free_space, &mut out, &mut next);
        }
        prioritize(&mut next, out.len());
        level = next;
    }
    out
}

/// Subdivides one pending rect, emitting a tile per child and queueing the
/// directories among them for the next level. Deliberately takes no budget
/// argument: a level that starts has to finish, or the parent's rect is
/// left partly uncovered.
fn expand<'a>(
    pending: &Pending<'a>,
    use_physical: bool,
    free_space: Option<u64>,
    out: &mut Vec<Tile>,
    next: &mut Vec<Pending<'a>>,
) {
    let (x, y, w, h, depth) = (pending.x, pending.y, pending.w, pending.h, pending.depth);
    // Free space is a property of the volume, so it joins the layout only
    // at the root — never inside a subdirectory.
    let free_space = if depth == 0 { free_space } else { None };

    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if pending.node.children.is_empty() && free_space.is_none() {
        return;
    }

    let mut children: Vec<(usize, &Node)> = pending.node.children.iter().enumerate().collect();
    children.sort_by(|a, b| {
        b.1.effective_size(use_physical)
            .cmp(&a.1.effective_size(use_physical))
    });
    let overflow_count = children.len().saturating_sub(MAX_CHILDREN_PER_LEVEL);
    let overflow_size: u64 = children
        .iter()
        .skip(MAX_CHILDREN_PER_LEVEL)
        .map(|(_, c)| c.effective_size(use_physical).max(1))
        .sum();
    children.truncate(MAX_CHILDREN_PER_LEVEL);

    let mut sizes: Vec<u64> = children
        .iter()
        .map(|(_, c)| c.effective_size(use_physical).max(1))
        .collect();
    if overflow_count > 0 {
        sizes.push(overflow_size.max(1));
    }
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

    let overflow_index = children.len();
    let free_space_index = overflow_index + usize::from(overflow_count > 0);

    for (i, r) in rects.iter().enumerate() {
        if r.w == 0 || r.h == 0 {
            continue;
        }
        let (rx, ry, rw, rh) = (x + r.x as f32, y + r.y as f32, r.w as f32, r.h as f32);

        if free_space.is_some() && i == free_space_index {
            out.push(Tile {
                x: rx,
                y: ry,
                w: rw,
                h: rh,
                is_dir: false,
                depth,
                name: "Free space".to_string(),
                category: None,
                index_path: Vec::new(),
                is_free_space: true,
                is_aggregate: false,
                can_label: true,
            });
            continue;
        }

        if overflow_count > 0 && i == overflow_index {
            out.push(Tile {
                x: rx,
                y: ry,
                w: rw,
                h: rh,
                is_dir: false,
                depth,
                name: format!("{overflow_count} more items"),
                category: None,
                index_path: Vec::new(),
                is_free_space: false,
                is_aggregate: true,
                can_label: true,
            });
            continue;
        }

        let Some(&(orig_idx, child)) = children.get(i) else {
            continue;
        };

        let area = rw * rh;
        let expands = child.is_dir
            && !child.children.is_empty()
            && depth + 1 < MAX_DEPTH
            && area >= MIN_RECURSE_AREA_PX;
        let reserve_label = rw >= MIN_LABEL_PX && rh >= MIN_LABEL_PX;

        let mut child_path = pending.index_path.clone();
        child_path.push(orig_idx);

        out.push(Tile {
            x: rx,
            y: ry,
            w: rw,
            h: rh,
            is_dir: child.is_dir,
            depth,
            name: child.name.clone(),
            category: child.category,
            index_path: child_path.clone(),
            is_free_space: false,
            is_aggregate: false,
            can_label: !expands || reserve_label,
        });

        if expands {
            let (iy, ih) = if reserve_label {
                (ry + MIN_LABEL_PX * 0.8, rh - MIN_LABEL_PX * 0.8)
            } else {
                (ry, rh)
            };
            next.push(Pending {
                node: child,
                x: rx,
                y: iy,
                w: rw,
                h: ih,
                depth: depth + 1,
                index_path: child_path,
            });
        }
    }
}

/// Trims the next level to what the remaining tile budget can pay for,
/// largest visible area first. Dropping a pending expansion only costs
/// detail — the directory still renders as its own solid tile — so this
/// can never open a hole in the map, which is exactly what stopping
/// part-way through a level would do.
fn prioritize(next: &mut Vec<Pending<'_>>, emitted: usize) {
    let mut budget = MAX_TILES.saturating_sub(emitted);
    if next.iter().map(Pending::tile_cost).sum::<usize>() <= budget {
        return;
    }
    next.sort_by(|a, b| b.area().total_cmp(&a.area()));
    next.retain(|pending| {
        let cost = pending.tile_cost();
        if cost <= budget {
            budget -= cost;
            true
        } else {
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, size: u64) -> Node {
        Node {
            name: name.to_string(),
            is_dir: false,
            is_symlink: false,
            size,
            physical_size: size,
            file_count: 1,
            dir_count: 0,
            modified: None,
            children: Vec::new(),
            error: false,
            category: None,
            ext_totals: Vec::new(),
            unreadable_count: 0,
        }
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let size = children.iter().map(|c| c.size).sum();
        Node {
            name: name.to_string(),
            is_dir: true,
            is_symlink: false,
            size,
            physical_size: size,
            file_count: children.iter().map(|c| c.file_count).sum(),
            dir_count: children.iter().filter(|c| c.is_dir).count() as u64,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: vec![(0, 0, 0); Category::COUNT],
            unreadable_count: 0,
        }
    }

    /// A drive-shaped tree: a handful of very large top-level directories,
    /// each of them deeply nested and widely fanned out.
    fn drive_shaped(depth: u32, fanout: usize, unit: u64) -> Node {
        fn build_node(depth: u32, fanout: usize, unit: u64, label: &str) -> Node {
            if depth == 0 {
                return leaf(&format!("{label}.dat"), unit.max(1));
            }
            let children = (0..fanout)
                .map(|i| {
                    build_node(
                        depth - 1,
                        fanout,
                        unit * (fanout - i) as u64,
                        &format!("{label}_{i}"),
                    )
                })
                .collect();
            dir(label, children)
        }
        build_node(depth, fanout, unit, "root")
    }

    /// Total area of the shallowest tiles, which between them are supposed
    /// to tile the whole panel exactly once.
    fn top_level_coverage(tiles: &[Tile], w: f32, h: f32) -> f32 {
        let covered: f32 = tiles
            .iter()
            .filter(|t| t.depth == 0)
            .map(|t| t.w * t.h)
            .sum();
        covered / (w * h)
    }

    #[test]
    fn a_tree_too_big_for_the_budget_still_covers_the_whole_panel() {
        // Regression test for the depth-first budget bug: the leftmost
        // subtree used to consume every tile before its siblings were
        // reached, leaving the right-hand side of a large volume's treemap
        // as bare panel background.
        let root = drive_shaped(5, 6, 1024);
        let (w, h) = (1900.0_f32, 420.0_f32);
        let tiles = build(&root, 0.0, 0.0, w, h, false, Some(root.size / 2));

        assert!(
            top_level_coverage(&tiles, w, h) > 0.99,
            "top level covered only {:.1}% of the panel",
            top_level_coverage(&tiles, w, h) * 100.0
        );
        let rightmost = tiles
            .iter()
            .filter(|t| t.depth == 0)
            .fold(0.0_f32, |acc, t| acc.max(t.x + t.w));
        assert!(
            rightmost >= w - 1.0,
            "treemap stopped at x={rightmost} instead of reaching {w}"
        );
    }

    #[test]
    fn every_level_is_emitted_whole_or_not_at_all() {
        let root = drive_shaped(5, 6, 1024);
        let tiles = build(&root, 0.0, 0.0, 1900.0, 420.0, false, None);
        // A parent that was expanded must be covered by its children, so
        // for every depth present, the deepest present depth is the only
        // one allowed to be partial. Checking monotonic depth ordering is
        // enough to prove the walk was level-order.
        let mut previous = 0;
        for tile in &tiles {
            assert!(
                tile.depth >= previous,
                "tiles are not in level order: {} after {}",
                tile.depth,
                previous
            );
            previous = tile.depth;
        }
    }

    #[test]
    fn overflow_children_keep_their_area_instead_of_inflating_the_rest() {
        // 200 children of equal size: the 80 that get their own tile are
        // 40% of the bytes, so they must get ~40% of the area and the
        // aggregate tile the rest. Dropping the tail would give them 100%.
        let children = (0..200).map(|i| leaf(&format!("f{i}.bin"), 1000)).collect();
        let root = dir("root", children);
        let (w, h) = (800.0_f32, 600.0_f32);
        let tiles = build(&root, 0.0, 0.0, w, h, false, None);

        let aggregate: Vec<_> = tiles.iter().filter(|t| t.is_aggregate).collect();
        assert_eq!(aggregate.len(), 1, "expected exactly one aggregate tile");
        assert_eq!(aggregate[0].name, "120 more items");

        let aggregate_share = aggregate[0].w * aggregate[0].h / (w * h);
        assert!(
            (aggregate_share - 0.6).abs() < 0.02,
            "aggregate tile took {:.1}% of the area, expected ~60%",
            aggregate_share * 100.0
        );
        assert!(!aggregate[0].is_node(), "aggregate tiles are not clickable");
    }

    #[test]
    fn free_space_only_appears_at_the_root() {
        let root = dir(
            "root",
            vec![dir("sub", vec![leaf("a.bin", 4000), leaf("b.bin", 4000)])],
        );
        let tiles = build(&root, 0.0, 0.0, 400.0, 300.0, false, Some(8000));
        let free: Vec<_> = tiles.iter().filter(|t| t.is_free_space).collect();
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].depth, 0);
        assert!(!free[0].is_node(), "free space is not clickable");
    }

    #[test]
    fn deeper_tiles_are_emitted_after_the_parents_they_paint_over() {
        let root = drive_shaped(4, 5, 4096);
        let tiles = build(&root, 0.0, 0.0, 900.0, 700.0, false, None);
        for (i, tile) in tiles.iter().enumerate() {
            if tile.index_path.len() < 2 {
                continue;
            }
            let parent = &tile.index_path[..tile.index_path.len() - 1];
            let parent_index = tiles.iter().position(|t| t.index_path == parent);
            if let Some(parent_index) = parent_index {
                assert!(
                    parent_index < i,
                    "child at {i} painted before its parent at {parent_index}"
                );
            }
        }
    }

    #[test]
    fn an_empty_panel_produces_no_tiles() {
        let root = dir("root", vec![leaf("a.bin", 10)]);
        assert!(build(&root, 0.0, 0.0, 0.0, 100.0, false, None).is_empty());
        assert!(build(&root, 0.0, 0.0, 100.0, 0.0, false, None).is_empty());
    }
}
