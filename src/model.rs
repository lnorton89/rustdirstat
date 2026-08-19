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
    /// On-disk (allocated) size — for directories, the sum of descendants'.
    /// Can differ from `size` (the logical/apparent size) for compressed,
    /// sparse, or small-file-on-large-cluster cases. On platforms/files
    /// where this can't be determined, it falls back to `size`.
    pub physical_size: u64,
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
    /// Per-category (logical size, physical size, count) totals across
    /// every file in this node's subtree. Both size totals are tracked (not
    /// just logical) so the extension/category legend can honor the
    /// logical/physical toggle the same way the file list, header, and
    /// treemap already do — otherwise the legend's percentages silently
    /// stop matching the sizes the rest of the screen is showing whenever
    /// physical mode is on. Empty for files (no heap allocation); length
    /// `Category::COUNT` for directories.
    pub ext_totals: Vec<(u64, u64, u64)>,
    /// Count of filesystem entries in this node's subtree that were seen
    /// but couldn't be fully read — a directory listing that failed
    /// outright (also covered by `error`, counted here as 1), or an
    /// individual entry whose metadata lookup failed mid-listing (a race
    /// with something else deleting it, a permission edge case, a flaky
    /// mount). Those entries are silently omitted from every other total
    /// (`size`, `file_count`, ...), so without this there'd be no way to
    /// tell "this subtree is 40 KB" from "this subtree is 40 KB *and we
    /// couldn't read some of it*" — the two look identical otherwise.
    pub unreadable_count: u64,
}

/// A scanned tree plus the absolute path its root corresponds to.
pub struct Tree {
    pub root_path: PathBuf,
    pub root: Node,
    /// Free/total bytes on the volume containing `root_path`, if it could
    /// be determined. Volume-level info, not part of the file hierarchy —
    /// used only to draw a "free space" reference tile alongside the real
    /// content when browsing the scan root, the way WinDirStat does.
    pub volume_free: Option<u64>,
    pub volume_total: Option<u64>,
}

impl Node {
    pub fn effective_size(&self, physical: bool) -> u64 {
        if physical {
            self.physical_size
        } else {
            self.size
        }
    }
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

    /// True only when the scan root is an actual filesystem/volume root
    /// (`/` on Unix, `C:\` or `\\?\C:\` on Windows) rather than some
    /// subfolder. `volume_free`/`volume_total` are always the *whole
    /// volume's* numbers, which only mean anything relative to the
    /// scanned tree when the tree IS the whole volume — comparing a
    /// small subfolder's size against gigabytes of unrelated free space
    /// on the rest of the drive would swamp the treemap with a free-space
    /// tile representing almost the entire area, which is what WinDirStat
    /// avoids by only ever showing it for a whole-drive scan.
    pub fn is_volume_root(&self) -> bool {
        self.root_path.parent().is_none()
    }
}

pub fn category_for_name(name: &str) -> Category {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    crate::color::category_for_ext(ext)
}
