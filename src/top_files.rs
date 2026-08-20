// ============================================================================
// Module:       tui::top_files
// Description:  The k largest files anywhere in a subtree, streamed through a
//               bounded min-heap rather than collected and sorted.
//
// Dependencies: std::collections::BinaryHeap; crate::model::Node
// ============================================================================

//! Finds the `k` largest files anywhere in a subtree without ever
//! materializing the full file list — a directory can hold millions of
//! files, so this streams through with a bounded min-heap (O(k) memory,
//! O(n log k) time) instead of collecting everything and sorting.

use crate::color::Category;
use crate::model::Node;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::SystemTime;

pub(crate) struct TopFile {
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
pub(crate) fn top_k(node: &Node, k: usize) -> Vec<TopFile> {
    let mut heap: BinaryHeap<Reverse<Entry>> = BinaryHeap::with_capacity(k + 1);
    let mut path = Vec::new();
    visit(node, &mut path, &mut heap, k);
    let mut out: Vec<TopFile> = heap.into_iter().map(|Reverse(e)| e.0).collect();
    out.sort_by_key(|b| Reverse(b.size));
    out
}

fn visit(node: &Node, path: &mut Vec<usize>, heap: &mut BinaryHeap<Reverse<Entry>>, k: usize) {
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if child.is_dir {
            visit(child, path, heap, k);
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
        }
        path.pop();
    }
}
