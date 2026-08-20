// ============================================================================
// Module:       top_files
// Description:  The k largest files anywhere in a subtree, streamed through a
//               bounded min-heap rather than collected and sorted.
//
// Dependencies: std::collections::BinaryHeap; crate::model::Node
// ============================================================================

//! Finds the `k` largest files anywhere in a subtree without ever
//! materializing the full file list — a directory can hold millions of
//! files, so this streams through with a bounded min-heap (O(k) memory,
//! O(n log k) time) instead of collecting everything and sorting.
//!
//! Front-end agnostic, and used by both: the terminal's "biggest files"
//! view and the window's Largest Files pane are the same call.
//!
//! The walk is iterative for the reason every walk in this crate is —
//! depth is whatever the user pointed at, and a recursive descent puts
//! it on the call stack.

use crate::color::Category;
use crate::model::Node;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::SystemTime;

pub struct TopFile {
    /// Indices from the directory being browsed down to this file.
    pub index_path: Vec<usize>,
    pub name: String,
    pub size: u64,
    /// Selection ("top k") is always by logical size — the two rarely
    /// disagree enough to matter for "what's biggest", and it keeps a size
    /// toggle from needing to redo the whole search. This is just for
    /// display when physical size is what's being shown.
    pub physical_size: u64,
    pub modified: Option<SystemTime>,
    pub category: Option<Category>,
}

struct Entry(TopFile);

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.0.size == other.0.size
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.size.cmp(&other.0.size)
    }
}

/// The `k` largest files anywhere in `node`'s subtree, largest first.
pub fn top_k(node: &Node, k: usize) -> Vec<TopFile> {
    let mut heap: BinaryHeap<Reverse<Entry>> = BinaryHeap::with_capacity(k + 1);
    let mut path = Vec::new();
    visit(node, &mut path, &mut heap, k);
    let mut out: Vec<TopFile> = heap.into_iter().map(|Reverse(e)| e.0).collect();
    out.sort_by_key(|b| Reverse(b.size));
    out
}

/// One directory still being walked, and how far through its children
/// the walk has got.
///
/// Frames cost depth, not breadth: a stack of `(path, node)` pairs would
/// hold a `PathBuf`-sized allocation per *pending sibling*, which for a
/// directory with millions of entries is the memory problem this module
/// exists to avoid, in a different shape.
struct Frame<'a> {
    node: &'a Node,
    next: usize,
}

fn visit(root: &Node, path: &mut Vec<usize>, heap: &mut BinaryHeap<Reverse<Entry>>, k: usize) {
    let mut stack = vec![Frame {
        node: root,
        next: 0,
    }];

    while let Some(top) = stack.len().checked_sub(1) {
        let Some(frame) = stack.get_mut(top) else {
            break;
        };
        let Some(child) = frame.node.children.get(frame.next) else {
            stack.pop();
            // Not for the root frame: it never pushed a segment of its
            // own, because `path` is relative to it.
            if !stack.is_empty() {
                path.pop();
            }
            continue;
        };
        let index = frame.next;
        frame.next += 1;

        path.push(index);
        if child.is_dir {
            // Leave `path` extended; the frame just pushed owns that
            // segment and pops it when it runs out of children.
            stack.push(Frame {
                node: child,
                next: 0,
            });
        } else {
            heap.push(Reverse(Entry(TopFile {
                index_path: path.clone(),
                name: child.name.clone(),
                size: child.size,
                physical_size: child.physical_size,
                modified: child.modified,
                category: child.category,
            })));
            if heap.len() > k {
                heap.pop();
            }
            path.pop();
        }
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;
    use crate::model::fixtures::*;

    /// The depth the stack-overflow test uses. Comfortably past what
    /// a real filesystem allows, and far past what the recursive
    /// version survived in a debug build.
    const DEEP: usize = 60_000;

    /// The walk does not put the tree depth on the call stack.
    ///
    /// It used to call itself once per directory level, and depth is the
    /// user choice, not ours — a chain like this overflowed the stack and
    /// took the process with it.
    #[test]
    fn a_tree_far_deeper_than_the_stack_is_still_walked() {
        let root = dir("root", vec![deep_chain(DEEP, 4096)]);
        let found = top_k(&root, 10);
        assert_eq!(found.len(), 1, "the one buried file should be found");
        let Some(first) = found.first() else { return };
        assert_eq!(first.name, "buried.bin");
        assert_eq!(
            first.index_path.len(),
            DEEP + 2,
            "the index path should name every level down to the file"
        );
    }

    /// Largest first, capped at `k`, with each hit index path actually
    /// leading to it.
    ///
    /// The iterative walk owns `path` across the whole traversal rather
    /// than getting a fresh one per frame, so a mispaired push and pop
    /// would corrupt the paths of everything after it — silently, since
    /// the sizes would still be right.
    #[test]
    fn the_biggest_files_come_back_in_order_and_their_paths_lead_to_them() {
        let root = dir(
            "root",
            vec![
                file("small.bin", 1),
                dir(
                    "a",
                    vec![
                        file("big.bin", 900),
                        dir("deep", vec![file("mid.bin", 500)]),
                    ],
                ),
                dir("b", vec![file("biggest.bin", 1000)]),
                file("tiny.bin", 2),
            ],
        );

        let found = top_k(&root, 3);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["biggest.bin", "big.bin", "mid.bin"]);

        for hit in &found {
            let landed = follow(&root, &hit.index_path);
            assert!(
                landed.is_some(),
                "the index path for {} ran off the tree",
                hit.name
            );
            assert_eq!(
                landed.map(|n| n.name.as_str()),
                Some(hit.name.as_str()),
                "{:?} does not lead to {}",
                hit.index_path,
                hit.name
            );
        }
    }

    /// An empty directory contributes nothing and does not disturb the
    /// path bookkeeping of the siblings after it.
    #[test]
    fn empty_directories_do_not_disturb_the_walk() {
        let root = dir(
            "root",
            vec![
                dir("empty", vec![]),
                dir("also_empty", vec![dir("still_empty", vec![])]),
                file("only.bin", 7),
            ],
        );
        let found = top_k(&root, 5);
        assert_eq!(found.len(), 1);
        let Some(first) = found.first() else { return };
        assert_eq!(
            first.index_path,
            vec![2],
            "the file is the third child, so nothing before it should have \
             left a segment behind"
        );
    }
}
