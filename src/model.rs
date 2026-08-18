use crate::color::Category;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A single file or directory in the scanned tree.
///
/// Nodes deliberately do **not** store their own absolute path — on a huge
/// tree (millions of entries), a full `PathBuf` per node dominates both
/// scan time and memory, since every node's path duplicates its entire
/// ancestor chain. Instead each node stores only its own `name`; the full
/// path is reconstructed on demand from `Tree::path_for` by walking down
/// from the root, which is needed only for the handful of operations that
/// actually touch the filesystem (open, delete), not for every node.
///
/// For directories, `size`, `file_count`, `dir_count`, and `ext_totals` are
/// aggregates of every descendant, computed bottom-up during scanning and
/// kept up to date as entries are removed (see `App::confirm_delete`) — so
/// nothing needs to re-walk the subtree just to answer "how big is this and
/// what's in it", which is what makes browsing a huge tree stay responsive.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub file_count: u64,
    /// Number of directory descendants (not including this node itself).
    pub dir_count: u64,
    pub modified: Option<SystemTime>,
    pub children: Vec<Node>,
    /// Set when the directory could not be read (e.g. permission denied).
    pub error: bool,
    /// `None` for directories. For files, the category their extension
    /// falls into — computed once at scan time.
    pub category: Option<Category>,
    /// Per-category (size, count) totals across every file in this node's
    /// subtree. Empty for files (no heap allocation); length
    /// `Category::COUNT` for directories.
    pub ext_totals: Vec<(u64, u64)>,
}

/// A scanned tree plus the absolute path its root corresponds to.
pub struct Tree {
    pub root_path: PathBuf,
    pub root: Node,
}

impl Tree {
    /// Reconstruct the absolute path of the node reached by following
    /// `index_path` (child indices, root to leaf) from the root.
    pub fn path_for(&self, index_path: &[usize]) -> PathBuf {
        let mut path = self.root_path.clone();
        let mut node = &self.root;
        for &idx in index_path {
            node = &node.children[idx];
            path.push(&node.name);
        }
        path
    }

    pub fn node_for(&self, index_path: &[usize]) -> &Node {
        let mut node = &self.root;
        for &idx in index_path {
            node = &node.children[idx];
        }
        node
    }
}

pub fn category_for_name(name: &str) -> Category {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    crate::color::category_for_ext(ext)
}
