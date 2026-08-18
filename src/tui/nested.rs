//! Flattens a directory's *entire* subtree (not just its direct children)
//! into absolutely-positioned rectangles, recursing the squarified treemap
//! layout into each directory's own allotted rectangle. This is what makes
//! the treemap show real internal structure instead of one flat blob for
//! any directory that dominates its parent.

use super::treemap;
use super::treemap::Rect;
use crate::color::Category;
use crate::model::Node;

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
    pub index_path: Vec<usize>,
}

const MAX_DEPTH: u16 = 6;
const MIN_RECURSE_W: u16 = 4;
const MIN_RECURSE_H: u16 = 3;
const MAX_CHILDREN_PER_LEVEL: usize = 128;
const MAX_ITEMS: usize = 4000;

pub fn build(node: &Node, x: u16, y: u16, width: u16, height: u16) -> Vec<TreemapItem> {
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
        &mut path,
        &mut out,
    );
    out
}

fn recurse(
    node: &Node,
    area: Rect,
    depth: u16,
    index_path: &mut Vec<usize>,
    out: &mut Vec<TreemapItem>,
) {
    if area.w == 0 || area.h == 0 || out.len() >= MAX_ITEMS || node.children.is_empty() {
        return;
    }

    let mut children: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
    children.sort_by(|a, b| b.1.size.cmp(&a.1.size));
    children.truncate(MAX_CHILDREN_PER_LEVEL);

    let sizes: Vec<u64> = children.iter().map(|(_, c)| c.size.max(1)).collect();
    let rects = treemap::layout(&sizes, area.w, area.h);

    for ((orig_idx, child), r) in children.iter().zip(rects.iter()) {
        if r.w == 0 || r.h == 0 || out.len() >= MAX_ITEMS {
            continue;
        }
        index_path.push(*orig_idx);

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
        });

        if child.is_dir && depth + 1 < MAX_DEPTH && r.w >= MIN_RECURSE_W && r.h >= MIN_RECURSE_H {
            // Reserve the rectangle's top row for the directory's own label.
            let inner = Rect {
                x: area.x + r.x,
                y: area.y + r.y + 1,
                w: r.w,
                h: r.h - 1,
            };
            recurse(child, inner, depth + 1, index_path, out);
        }

        index_path.pop();
    }
}
