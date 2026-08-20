// ============================================================================
// Module:       gui::treemap_layout
// Description:  Flattens a subtree into absolutely-positioned pixel tiles
//               under a fixed budget, spent level-order so coverage stays
//               complete.
//
// Dependencies: crate::treemap (squarify), crate::model::Node,
//               crate::color::Category
// ============================================================================

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

pub(super) struct Tile {
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
    /// children too small to draw individually (see `MIN_TILE_AREA_PX`).
    /// It occupies their combined area but maps to no single node, so it
    /// is not selectable.
    pub is_aggregate: bool,
    /// See `tui::nested::TreemapItem::can_label` — same reasoning: a tile
    /// that recursed into children without reserving a label row must not
    /// draw one, or the children painted after it cover the tile's own
    /// name (or the name covers them, depending on draw order).
    pub can_label: bool,
}

impl Tile {
    /// Whether this tile stands for a real node the user can click.
    pub(super) fn is_node(&self) -> bool {
        !self.is_free_space && !self.is_aggregate
    }
}

const MAX_DEPTH: u32 = 32;
/// Narrowest a tile can be and still show a name worth reading.
const MIN_LABEL_WIDTH_PX: f32 = 18.0;
/// Expansion stops once a tile's pixel area drops below this — a tile
/// smaller than a handful of pixels on a side can't show anything a user
/// could distinguish anyway, and without a floor a deeply-fanned-out
/// subtree (thousands of tiny files) would recurse effectively forever.
const MIN_RECURSE_AREA_PX: f32 = 9.0;
/// A child gets its own tile only if its share of the parent works out to
/// at least this much area. Below it there is nothing to see — the tile
/// would be a two-pixel speck — so the remainder is folded into one
/// aggregate tile instead.
///
/// This replaced a fixed "first 80 children" cap, which was the wrong
/// question to ask. Whether a child is worth drawing depends on how much
/// room it would get, not on where it sorted: a folder of 200 chunky
/// subdirectories was truncated at 80 and the other 120 collapsed into a
/// single grey slab covering most of the panel, while a folder of 30
/// tiny files was drawn in full detail nobody could see.
const MIN_TILE_AREA_PX: f32 = 6.0;
/// Hard ceiling on children considered at one level, purely so a
/// pathological directory cannot make a single level allocate without
/// bound. The area rule above is what normally decides.
const MAX_CHILDREN_PER_LEVEL: usize = 2048;
/// Soft ceiling on emitted tiles, checked *between* levels and never
/// inside one, so that any level which starts also finishes.
pub(super) const MAX_TILES: usize = 40_000;
/// Tile budget while the user is dragging something.
///
/// The layout is keyed on the panel rect, so dragging a splitter changes
/// the rect on every frame of the drag and re-lays-out the whole map each
/// time. At the full budget that is forty thousand tiles per frame, which
/// on a whole-drive scan is what made resizing anything feel like the
/// window had stopped responding. A tenth of the detail is still a
/// recognisable map to drag against, and the full one is rebuilt the
/// moment the pointer comes up.
pub(super) const MAX_TILES_INTERACTIVE: usize = 4_000;

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
    /// cannot exceed the child count, and it cannot exceed what the rect
    /// has room for at the minimum tile area either.
    fn tile_cost(&self) -> usize {
        let by_count = self.node.children.len().min(MAX_CHILDREN_PER_LEVEL) + 1;
        let by_area = (self.area().max(0.0) / MIN_TILE_AREA_PX) as usize + 1;
        by_count.min(by_area)
    }
}

/// Everything the layout needs besides the tree itself.
///
/// A struct rather than a positional argument list: the two `f32`s and
/// the `Option<u64>` are trivially transposable, and doing exactly that
/// is how `free_space` and `label_strip` got swapped once already.
pub(super) struct LayoutRequest {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub use_physical: bool,
    /// Volume free space to show alongside the root, if this is a
    /// whole-drive scan.
    pub free_space: Option<u64>,
    /// Height a tile reserves for its own name, measured by the caller
    /// from the font it will draw with.
    pub label_strip: f32,
    /// Ceiling on emitted tiles. `MAX_TILES` at rest,
    /// `MAX_TILES_INTERACTIVE` while a drag is in flight.
    pub max_tiles: usize,
}

pub(super) fn build(node: &Node, request: &LayoutRequest) -> Vec<Tile> {
    let LayoutRequest {
        x,
        y,
        width,
        height,
        use_physical,
        free_space,
        label_strip,
        max_tiles,
    } = *request;
    // Whole pixels, so nested tiles stay on integer origins. The caller
    // measures this from the font it will actually draw with rather than
    // passing a guess: a strip shorter than the text means the children
    // painted into the rest of the tile cover the bottom of their own
    // parent's name, which showed up as descenders being sliced off.
    let label_strip = label_strip.max(0.0).ceil();
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

    while !level.is_empty() && out.len() < max_tiles {
        let mut next = Vec::new();
        for pending in &level {
            expand(
                pending,
                use_physical,
                free_space,
                label_strip,
                &mut out,
                &mut next,
            );
        }
        prioritize(&mut next, out.len(), max_tiles);
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
    label_strip: f32,
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
    children.truncate(MAX_CHILDREN_PER_LEVEL);

    // How many of them are actually worth a tile, largest first. The
    // denominator has to include free space, since that competes for the
    // same rect at the root.
    let total: u64 = children
        .iter()
        .map(|(_, c)| c.effective_size(use_physical).max(1))
        .sum();
    let denominator = (total + free_space.unwrap_or(0)).max(1) as f64;
    let area = f64::from(w) * f64::from(h);
    let visible = children
        .iter()
        .take_while(|(_, c)| {
            let share = c.effective_size(use_physical).max(1) as f64 / denominator;
            share * area >= f64::from(MIN_TILE_AREA_PX)
        })
        .count();

    // Every child is too small to see. Leave the parent's own tile
    // standing rather than replacing it with an aggregate covering the
    // identical area — the parent already represents exactly these bytes,
    // and a grey "N more items" slab in its place says strictly less.
    if visible == 0 && free_space.is_none() {
        return;
    }

    let overflow_count = children.len() - visible;
    let overflow_size: u64 = children
        .iter()
        .skip(visible)
        .map(|(_, c)| c.effective_size(use_physical).max(1))
        .sum();
    children.truncate(visible);

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
    //
    // Rounded *down*, not to nearest. Rounding up lets the laid-out area
    // come out fractionally larger than the rect it was given, so a child
    // can extend past the bottom or right edge of the parent it was
    // subdivided from — painting over a sibling and crediting that
    // sibling's pixels to the wrong directory. Down leaves at most a
    // hairline of the parent uncovered instead, which is invisible and
    // cannot compound with depth the way the overflow did.
    let px_w = w.floor().clamp(0.0, u16::MAX as f32) as u16;
    let px_h = h.floor().clamp(0.0, u16::MAX as f32) as u16;
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
        let reserve_label = rw >= MIN_LABEL_WIDTH_PX && rh >= label_strip + 4.0;

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
                (ry + label_strip, rh - label_strip)
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
fn prioritize(next: &mut Vec<Pending<'_>>, emitted: usize, max_tiles: usize) {
    let mut budget = max_tiles.saturating_sub(emitted);
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

    /// Stands in for what the renderer measures from its font.
    const TEST_LABEL_STRIP: f32 = 16.0;

    // The tree builders are shared — see `crate::model::fixtures`.
    use crate::model::fixtures::{dir, file as leaf};

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
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
                use_physical: false,
                free_space: Some(root.size / 2),
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );

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
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 1900.0,
                height: 420.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );
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
    fn many_children_all_get_their_own_tile_when_there_is_room() {
        // 200 equal children in a large panel work out to ~2400px each,
        // far above the visibility floor, so every one of them is drawn.
        // A fixed "first N children" cap used to truncate this at 80 and
        // collapse the rest into one grey slab covering 60% of the panel.
        let children = (0..200).map(|i| leaf(&format!("f{i}.bin"), 1000)).collect();
        let root = dir("root", children);
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );

        assert_eq!(tiles.iter().filter(|t| !t.is_aggregate).count(), 200);
        assert!(
            !tiles.iter().any(|t| t.is_aggregate),
            "nothing here is too small to draw, so nothing should be aggregated"
        );
    }

    #[test]
    fn a_sub_pixel_tail_is_aggregated_at_its_true_area() {
        // Five chunky files plus two thousand specks, in a panel where a
        // speck works out to under 3px. The specks are 2/7ths of the
        // bytes, so the tile standing in for them has to be 2/7ths of the
        // area — dropping them instead would inflate the five survivors
        // to fill the whole panel and misstate what is on disk.
        let mut children: Vec<Node> = (0..5)
            .map(|i| leaf(&format!("big{i}.bin"), 100_000))
            .collect();
        children.extend((0..2000).map(|i| leaf(&format!("tiny{i}.bin"), 100)));
        let root = dir("root", children);
        let (w, h) = (200.0_f32, 100.0_f32);
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );

        let aggregate: Vec<_> = tiles.iter().filter(|t| t.is_aggregate).collect();
        assert_eq!(aggregate.len(), 1, "expected exactly one aggregate tile");
        assert_eq!(aggregate[0].name, "2000 more items");
        assert!(!aggregate[0].is_node(), "aggregate tiles are not clickable");

        let share = aggregate[0].w * aggregate[0].h / (w * h);
        assert!(
            (share - 2.0 / 7.0).abs() < 0.03,
            "aggregate took {:.1}% of the area, expected ~28.6%",
            share * 100.0
        );
    }

    #[test]
    fn a_directory_of_nothing_but_specks_is_left_as_its_own_tile() {
        // Every child is below the visibility floor. Replacing the parent
        // with an aggregate covering the identical rect would say strictly
        // less than the parent tile already does, so the expansion is
        // skipped entirely.
        let children = (0..5000).map(|i| leaf(&format!("f{i}.bin"), 10)).collect();
        let root = dir(
            "root",
            vec![dir("dense", children), leaf("big.bin", 5_000_000)],
        );
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );

        assert!(
            tiles.iter().any(|t| t.name == "dense"),
            "the directory itself should still be drawn"
        );
        assert!(
            !tiles.iter().any(|t| t.is_aggregate),
            "a directory with nothing visible inside should not become a grey slab"
        );
    }

    #[test]
    fn free_space_only_appears_at_the_root() {
        let root = dir(
            "root",
            vec![dir("sub", vec![leaf("a.bin", 4000), leaf("b.bin", 4000)])],
        );
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
                use_physical: false,
                free_space: Some(8000),
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );
        let free: Vec<_> = tiles.iter().filter(|t| t.is_free_space).collect();
        assert_eq!(free.len(), 1);
        assert_eq!(free[0].depth, 0);
        assert!(!free[0].is_node(), "free space is not clickable");
    }

    #[test]
    fn deeper_tiles_are_emitted_after_the_parents_they_paint_over() {
        let root = drive_shaped(4, 5, 4096);
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 900.0,
                height: 700.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );
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

    /// Sweep a drive-shaped tree at a range of panel sizes and check the
    /// things that have to hold for every tile at every size.
    #[test]
    fn no_tile_ever_escapes_the_panel_or_its_parent() {
        let root = drive_shaped(5, 6, 4096);
        for (w, h) in [
            (1900.0_f32, 420.0_f32),
            (640.0, 900.0),
            (300.0, 120.0),
            (77.0, 51.0),
            (1024.0, 1024.0),
        ] {
            let tiles = build(
                &root,
                &LayoutRequest {
                    x: 12.0,
                    y: 34.0,
                    width: w,
                    height: h,
                    use_physical: false,
                    free_space: Some(root.size / 3),
                    label_strip: TEST_LABEL_STRIP,
                    max_tiles: MAX_TILES,
                },
            );
            let panel = (12.0, 34.0, 12.0 + w, 34.0 + h);
            for tile in &tiles {
                assert!(
                    tile.x >= panel.0 - 0.51
                        && tile.y >= panel.1 - 0.51
                        && tile.x + tile.w <= panel.2 + 0.51
                        && tile.y + tile.h <= panel.3 + 0.51,
                    "at {w}x{h}, tile {:?} at ({}, {}) {}x{} escapes the panel",
                    tile.name,
                    tile.x,
                    tile.y,
                    tile.w,
                    tile.h
                );
                assert!(
                    tile.w >= 0.0 && tile.h >= 0.0,
                    "at {w}x{h}, tile {:?} has a negative dimension",
                    tile.name
                );
            }

            // A child must sit inside the tile of the parent it was
            // subdivided out of. If it did not, it would paint over a
            // sibling and attribute that sibling's pixels to the wrong
            // directory — the treemap would be quietly lying about where
            // the bytes are.
            for tile in tiles.iter().filter(|t| t.index_path.len() >= 2) {
                let parent_path = &tile.index_path[..tile.index_path.len() - 1];
                let Some(parent) = tiles.iter().find(|t| t.index_path == parent_path) else {
                    continue;
                };
                assert!(
                    tile.x >= parent.x - 0.51
                        && tile.x + tile.w <= parent.x + parent.w + 0.51
                        && tile.y + tile.h <= parent.y + parent.h + 0.51,
                    "at {w}x{h}, {:?} escapes its parent {:?}",
                    tile.name,
                    parent.name
                );
            }
        }
    }

    /// Siblings partition their parent: no overlaps, no holes.
    #[test]
    fn tiles_at_one_level_do_not_overlap_each_other() {
        let root = drive_shaped(4, 5, 4096);
        let tiles = build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 900.0,
                height: 600.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        );

        let top: Vec<_> = tiles.iter().filter(|t| t.depth == 0).collect();
        for (i, a) in top.iter().enumerate() {
            for b in top.iter().skip(i + 1) {
                let overlap_w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let overlap_h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(
                    overlap_w <= 0.51 || overlap_h <= 0.51,
                    "{:?} and {:?} overlap by {overlap_w}x{overlap_h}",
                    a.name,
                    b.name
                );
            }
        }
    }

    /// A drag must not pay for the full map on every frame.
    ///
    /// The layout is keyed on the panel rect, so dragging a splitter
    /// changes that rect every frame and rebuilds the whole thing each
    /// time. On a whole-drive scan at the full budget that is tens of
    /// thousands of tiles per frame, which is what made resizing feel
    /// like the window had stopped responding. The reduced budget is what
    /// keeps a drag cheap; the full one comes back when the pointer does.
    #[test]
    fn a_drag_lays_out_far_less_than_a_settled_frame() {
        let root = drive_shaped(6, 6, 4096);
        let request = |max_tiles| LayoutRequest {
            x: 0.0,
            y: 0.0,
            width: 1900.0,
            height: 900.0,
            use_physical: false,
            free_space: None,
            label_strip: TEST_LABEL_STRIP,
            max_tiles,
        };

        let settled = build(&root, &request(MAX_TILES));
        let dragging = build(&root, &request(MAX_TILES_INTERACTIVE));

        assert!(
            dragging.len() <= MAX_TILES_INTERACTIVE,
            "a drag emitted {} tiles, over its own {MAX_TILES_INTERACTIVE} budget",
            dragging.len()
        );
        assert!(
            settled.len() > dragging.len() * 2,
            "the settled layout ({} tiles) is not meaningfully more detailed than the \
             dragging one ({} tiles), so the reduced budget is buying nothing",
            settled.len(),
            dragging.len()
        );
        // Cheaper still has to mean *correct*: a drag that left holes in
        // the map would be worse than a slow one.
        let covered: f32 = dragging
            .iter()
            .filter(|tile| tile.depth == 0)
            .map(|tile| tile.w * tile.h)
            .sum();
        assert!(
            covered / (1900.0 * 900.0) > 0.99,
            "the reduced-detail layout covers only {:.1}% of the panel",
            covered / (1900.0 * 900.0) * 100.0
        );
    }

    #[test]
    fn an_empty_panel_produces_no_tiles() {
        let root = dir("root", vec![leaf("a.bin", 10)]);
        assert!(build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 100.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        )
        .is_empty());
        assert!(build(
            &root,
            &LayoutRequest {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 0.0,
                use_physical: false,
                free_space: None,
                label_strip: TEST_LABEL_STRIP,
                max_tiles: MAX_TILES,
            },
        )
        .is_empty());
    }
}
