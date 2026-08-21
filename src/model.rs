// ============================================================================
// Module:       model
// Description:  The scanned filesystem hierarchy every other module reads, the
//               index-path addressing that stands in for storing paths, and
//               the sort order both front ends list siblings in.
//
// Dependencies: serde (SortMode is persisted); crate::color::Category
// ============================================================================

//! The scanned filesystem hierarchy: [`Node`], [`Tree`], the index
//! paths that stand in for storing a path per node, and [`SortMode`] —
//! the order siblings are listed in, which lives here because both front
//! ends and the persisted config all need it.
//!
//! Two properties drive the shape of everything here, and both follow
//! from one fact — a real volume is millions of nodes.
//!
//! Nodes do not store their own path. A `PathBuf` per node duplicates its
//! entire ancestor chain and dominates memory on a large scan, so a node
//! keeps only its own `name` and [`Tree::path_for`] rebuilds the rest
//! from child indices on demand. Selections are therefore `Vec<usize>`
//! index paths, not paths.
//!
//! Directory aggregates — size, counts, per-category totals — are rolled
//! up bottom-up during the scan and kept current as entries are deleted,
//! so answering "how big is this and what is in it" is a field read
//! rather than a walk. `Node`'s `Drop` is iterative for the same reason
//! nothing else here recurses: depth is user-supplied, and a tree-sized
//! recursion puts it on the call stack.

use crate::color::Category;
use std::ffi::{OsStr, OsString};
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
/// `name` is an `OsString`, not a `String`, and that is load-bearing:
/// Unix filenames are byte strings, not necessarily UTF-8, and the lossy
/// conversion (`to_string_lossy`) that would turn `OsString` into
/// `String` replaces invalid byte sequences with U+FFFD. Two distinct
/// real names can collapse onto the same replacement character, so a
/// `String` name cannot reliably reconstruct a path that reaches the
/// filesystem — and this is an application that deletes and moves what
/// it scans. The tree keeps the exact bytes; lossy conversion happens
/// only at display boundaries (labels, sort keys, search matching), never
/// on a path handed to the OS.
///
/// For directories, `size`, `file_count`, `dir_count`, and `ext_totals` are
/// aggregates of every descendant, computed bottom-up during scanning and
/// kept up to date as entries are removed (see `App::confirm_delete`) — so
/// nothing needs to re-walk the subtree just to answer "how big is this and
/// what's in it", which is what makes browsing a huge tree stay responsive.
#[derive(Debug, Clone)]
pub struct Node {
    /// The entry's exact filesystem name, as bytes. Display boundaries
    /// convert with `to_string_lossy()`; anything that touches the
    /// filesystem uses this `OsString` unchanged.
    pub name: OsString,
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
    /// The file's filesystem identity — (device, inode) on Unix, where it
    /// comes free out of the scan's `stat()`. Every hard link to the
    /// same file shares one, which is what lets duplicate detection
    /// distinguish "two copies" from "two names for one file" (deleting
    /// an alias frees nothing until the last one goes). `None` on
    /// platforms that would need an extra per-file syscall, and for
    /// directories.
    pub file_id: Option<crate::platform::FileId>,
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

/// The order sibling nodes are listed in.
///
/// Lives here rather than in a front end because both of them offer the
/// same six orders over the same nodes, and [`crate::config`] persists
/// the choice — which meant the scanning core had to reach up into the
/// TUI for the type. `sort_nodes` is here for the same reason: the two
/// front ends each had their own copy of the match, and they had drifted
/// (the terminal's ignored `physical` and always ordered by logical
/// size, so toggling to on-disk sizes reordered nothing).
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortMode {
    SizeDesc,
    SizeAsc,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
}

impl SortMode {
    /// The next order in the cycle the terminal's `s` key steps through.
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::ModifiedDesc,
            SortMode::ModifiedDesc => SortMode::ModifiedAsc,
            SortMode::ModifiedAsc => SortMode::SizeDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size desc",
            SortMode::SizeAsc => "size asc",
            SortMode::NameAsc => "name asc",
            SortMode::NameDesc => "name desc",
            SortMode::ModifiedDesc => "newest first",
            SortMode::ModifiedAsc => "oldest first",
        }
    }
}

/// Sorts `nodes` in place, each paired with its index in the parent's
/// unsorted `children` — which is what makes navigation and deletion
/// stable no matter how the view is ordered.
///
/// `physical` picks which size the size orders compare, so the list is
/// ordered by the number it is displaying.
pub fn sort_nodes(nodes: &mut [(usize, &Node)], sort: SortMode, physical: bool) {
    match sort {
        SortMode::SizeDesc => nodes.sort_by(|a, b| {
            b.1.effective_size(physical)
                .cmp(&a.1.effective_size(physical))
        }),
        SortMode::SizeAsc => nodes.sort_by(|a, b| {
            a.1.effective_size(physical)
                .cmp(&b.1.effective_size(physical))
        }),
        // Sort keys are presentation, not identity: names are compared
        // through the same lossy view a person sees, preserving the
        // Unicode case-folding for valid UTF-8 while keeping the two
        // name orders defined over the same `OsString` the nodes hold.
        SortMode::NameAsc => nodes.sort_by_key(|a| a.1.name.to_string_lossy().to_lowercase()),
        SortMode::NameDesc => {
            nodes.sort_by_key(|b| std::cmp::Reverse(b.1.name.to_string_lossy().to_lowercase()))
        }
        SortMode::ModifiedDesc => nodes.sort_by_key(|b| std::cmp::Reverse(b.1.modified)),
        SortMode::ModifiedAsc => nodes.sort_by_key(|a| a.1.modified),
    }
}

/// Iterative pre-order walk shared by the tree-sized passes: search and
/// top-files both traverse a whole subtree visiting every node, and both
/// need an index path to report back. There used to be two copies of the
/// same frame discipline, and the comment that matters — "not for the
/// root frame" — was written twice and read zero times.
///
/// `root` itself is never visited and its index is never pushed: `path`
/// is relative to it, so callers that report paths start with the
/// directory they were handed. Every directory is descended into — every
/// consumer of this walk visits whole subtrees. A visitor that wants out
/// early (search, once its result cap is exceeded) returns
/// [`WalkControl::Stop`] and the walk quits on the spot; a walk over a
/// drive-sized tree must be able to bail.
///
/// One frame per open directory, on the heap: a stack of `(path, node)`
/// pairs would hold a `PathBuf`-sized allocation per *pending sibling*,
/// which for a directory with millions of entries is the memory problem
/// the iterative form exists to avoid, in a different shape.
pub(crate) fn walk_preorder<'a>(
    root: &'a Node,
    mut visit: impl FnMut(&'a Node, &mut Vec<usize>) -> WalkControl,
) {
    struct Frame<'a> {
        node: &'a Node,
        next: usize,
    }

    let mut path = Vec::new();
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
        if visit(child, &mut path) == WalkControl::Stop {
            return;
        }
        if child.is_dir {
            // Leave `path` extended; the frame just pushed owns that
            // segment and pops it when it runs out of children.
            stack.push(Frame {
                node: child,
                next: 0,
            });
        } else {
            path.pop();
        }
    }
}

/// What a [`walk_preorder`] visitor asks the walk to do after its node.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkControl {
    Continue,
    Stop,
}

/// Frees a subtree without recursing once per level.
///
/// The derived drop walks `children` recursively, so a deep enough tree
/// overflows the stack and takes the process down — while *freeing*
/// memory, which is not a failure anyone expects to have to handle. The
/// GUI already moves this work off the UI thread (`drop_in_background`),
/// but that only changes which thread's stack runs out.
///
/// Costs one allocation for a whole tree, not one per node: the
/// outermost drop takes its children and drains them here, so by the
/// time any node below it is dropped its own `children` is already empty
/// and it returns immediately. That matters — a drive-sized scan is
/// millions of nodes, and this runs on every one of them.
impl Drop for Node {
    fn drop(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let mut pending = vec![std::mem::take(&mut self.children)];
        while let Some(mut level) = pending.pop() {
            while let Some(mut node) = level.pop() {
                let children = std::mem::take(&mut node.children);
                if !children.is_empty() {
                    pending.push(children);
                }
                // `node` drops here with no children of its own, so the
                // early return above ends it.
            }
        }
    }
}

impl Tree {
    /// An empty tree standing in for a real one: what the GUI shows while
    /// the first scan is still running, and what a scanned tree is swapped
    /// out for when it is being retired.
    pub fn placeholder(root_path: PathBuf) -> Self {
        let name = root_path
            .file_name()
            .map(OsString::from)
            .unwrap_or_else(|| OsString::from(root_path.display().to_string()));
        let is_dir = root_path.is_dir();
        Self {
            root: Node {
                name,
                is_dir,
                is_symlink: false,
                size: 0,
                physical_size: 0,
                file_count: 0,
                dir_count: 0,
                modified: None,
                children: Vec::new(),
                error: false,
                category: None,
                ext_totals: if is_dir {
                    vec![(0, 0, 0); Category::COUNT]
                } else {
                    Vec::new()
                },
                unreadable_count: 0,
                file_id: None,
            },
            root_path,
            volume_free: None,
            volume_total: None,
        }
    }

    /// Reconstruct the absolute path of the node reached by following
    /// `index_path` (child indices, root to leaf) from the root.
    ///
    /// Exact: `None` if the path runs off the end of the tree. An index
    /// path only means anything against the tree it was taken from, and
    /// the tree can be replaced under a held path by a rescan — the
    /// exact form is the one a destructive operation can trust, because
    /// the alternative (truncating to the deepest node that exists) is
    /// how a stale path used to resolve to a *different directory* than
    /// the one the user pointed at. Display code that would rather show
    /// the deepest reachable place than nothing uses
    /// [`Self::deepest_valid_path`]. It used to index `children`
    /// directly and panic instead — the crate denies `panic!`, but `[]`
    /// walks straight past that.
    pub fn path_for(&self, index_path: &[usize]) -> Option<PathBuf> {
        let mut path = self.root_path.clone();
        let mut node = &self.root;
        for &idx in index_path {
            let child = node.children.get(idx)?;
            node = child;
            path.push(&node.name);
        }
        Some(path)
    }

    /// The node `index_path` leads to, or `None` if it runs off the end
    /// of the tree. Exact, like [`Self::path_for`] — see its doc for why
    /// the forgiving form lives under an awkward name instead.
    pub fn node_for(&self, index_path: &[usize]) -> Option<&Node> {
        let mut node = &self.root;
        for &idx in index_path {
            node = node.children.get(idx)?;
        }
        Some(node)
    }

    /// The longest prefix of `index_path` that exists in this tree.
    ///
    /// The forgiving primitive the exact lookups refuse to be: where
    /// [`Self::node_for`]/[`Self::path_for`] answer `None`, this answers
    /// the deepest place the path still reaches. Display code uses it so
    /// a stale path shows the thing it now points at rather than
    /// nothing; mutation code must not, for exactly the reason
    /// [`Self::path_for`] spells out.
    pub fn valid_prefix(&self, index_path: &[usize]) -> Vec<usize> {
        let mut valid = Vec::new();
        let mut node = &self.root;
        for &idx in index_path {
            let Some(child) = node.children.get(idx) else {
                break;
            };
            valid.push(idx);
            node = child;
        }
        valid
    }

    /// Forgiving: the node `index_path` leads to, or the deepest one
    /// that exists. Display and navigation only — a mutation that
    /// resolved through this would act on a different directory than the
    /// one the user pointed at. See [`Self::valid_prefix`].
    pub fn deepest_valid_node(&self, index_path: &[usize]) -> &Node {
        let prefix = self.valid_prefix(index_path);
        // The prefix is valid by construction, so this is always `Some`;
        // the fallback exists to keep the function total, not because it
        // can be reached.
        self.node_for(&prefix).unwrap_or(&self.root)
    }

    /// Forgiving: the path of the deepest node `index_path` reaches.
    pub fn deepest_valid_path(&self, index_path: &[usize]) -> PathBuf {
        let prefix = self.valid_prefix(index_path);
        self.path_for(&prefix)
            .unwrap_or_else(|| self.root_path.clone())
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

/// The category a file's name falls into, computed once at scan time.
///
/// Takes the raw `OsStr` rather than a lossy string so classification does
/// not depend on the same U+FFFD collision that would be unsafe for path
/// reconstruction — a non-UTF-8 extension simply has no `to_str`, so it
/// yields `NoExtension`/`Other` instead of a guess.
pub fn category_for_name(name: &OsStr) -> Category {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    crate::color::category_for_ext(ext)
}

/// Builders for the small hand-made trees the tests across this crate
/// assert against.
///
/// Every test module that needed a `Node` used to declare its own `file`
/// and `dir` — fourteen near-identical lines each, listing every field
/// so that adding one to `Node` broke each copy separately.
#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// A file whose on-disk size matches its logical size.
    pub fn file(name: &str, size: u64) -> Node {
        file_sized(name, size, size)
    }

    /// A file whose logical and on-disk sizes differ — sparse and
    /// compressed files are the reason `physical_size` exists.
    pub fn file_sized(name: &str, size: u64, physical_size: u64) -> Node {
        Node {
            name: OsString::from(name),
            is_dir: false,
            is_symlink: false,
            size,
            physical_size,
            file_count: 1,
            dir_count: 0,
            modified: None,
            children: Vec::new(),
            error: false,
            category: None,
            ext_totals: Vec::new(),
            unreadable_count: 0,
            file_id: None,
        }
    }

    /// A directory whose aggregates are rolled up from `children`, the
    /// way the scanner would have left them — including an `ext_totals`
    /// of the documented length, which is what separates a directory
    /// from a file here.
    pub fn dir(name: &str, children: Vec<Node>) -> Node {
        Node {
            name: OsString::from(name),
            is_dir: true,
            is_symlink: false,
            size: children.iter().map(|c| c.size).sum(),
            physical_size: children.iter().map(|c| c.physical_size).sum(),
            file_count: children.iter().map(|c| c.file_count).sum(),
            dir_count: children.iter().filter(|c| c.is_dir).count() as u64,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: vec![(0, 0, 0); Category::COUNT],
            unreadable_count: 0,
            file_id: None,
        }
    }

    /// A single chain `depth` directories deep with one file at the
    /// bottom, for the tests that check a walk does not put the tree's
    /// depth on the call stack.
    pub fn deep_chain(depth: usize, leaf_size: u64) -> Node {
        let mut node = dir("bottom", vec![file("buried.bin", leaf_size)]);
        for level in 0..depth {
            node = dir(&format!("d{level}"), vec![node]);
        }
        node
    }

    /// Follows an index path from `root`, or `None` if it runs off the
    /// tree — the check that an index path actually leads where the
    /// thing that produced it claimed.
    pub fn follow<'a>(root: &'a Node, index_path: &[usize]) -> Option<&'a Node> {
        let mut node = root;
        for &index in index_path {
            node = node.children.get(index)?;
        }
        Some(node)
    }
}

#[cfg(test)]
mod sort_tests {
    use super::fixtures::*;
    use super::*;

    /// A size sort orders by the size the view is actually showing.
    ///
    /// The terminal had its own copy of this match that read `size`
    /// whatever `use_physical` was set to, so pressing `p` there swapped
    /// every number in the list without reordering a single row. The two
    /// front ends share one implementation now, and this pins the
    /// property that made them differ.
    #[test]
    fn a_size_sort_follows_the_size_being_displayed() {
        // Deliberately opposed: `sparse` is the larger file logically and
        // the smaller one on disk.
        let sparse = file_sized("sparse.img", 1_000_000, 4_096);
        let packed = file_sized("packed.bin", 500_000, 500_000);
        let nodes = [(0, &sparse), (1, &packed)];

        let mut logical = nodes.to_vec();
        sort_nodes(&mut logical, SortMode::SizeDesc, false);
        assert_eq!(
            logical
                .first()
                .map(|n| n.1.name.to_string_lossy().to_string()),
            Some("sparse.img".to_string()),
            "by logical size the sparse file is the bigger one"
        );

        let mut physical = nodes.to_vec();
        sort_nodes(&mut physical, SortMode::SizeDesc, true);
        assert_eq!(
            physical
                .first()
                .map(|n| n.1.name.to_string_lossy().to_string()),
            Some("packed.bin".to_string()),
            "by on-disk size the order reverses — this is what the terminal \
             front end used to get wrong"
        );
    }

    /// The index paired with each node is its position in the parent
    /// unsorted `children`, and sorting must not disturb it — every
    /// navigation and deletion is addressed by that index.
    #[test]
    fn sorting_keeps_each_node_paired_with_its_original_index() {
        let a = file("c.bin", 3);
        let b = file("a.bin", 1);
        let c = file("b.bin", 2);
        let mut nodes = vec![(0, &a), (1, &b), (2, &c)];

        sort_nodes(&mut nodes, SortMode::NameAsc, false);
        let pairs: Vec<(usize, String)> = nodes
            .iter()
            .map(|(i, n)| (*i, n.name.to_string_lossy().to_string()))
            .collect();
        assert_eq!(
            pairs,
            [
                (1, "a.bin".to_string()),
                (2, "b.bin".to_string()),
                (0, "c.bin".to_string())
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index path outliving the tree it was taken from must not take
    /// the process down with it, and must not quietly answer about a
    /// different directory.
    ///
    /// Both helpers used to index `children` directly. The crate denies
    /// `panic!`, but `[]` is not a `panic!` call and slipped past the
    /// lint, so a queued deletion or a restored selection surviving a
    /// rescan could crash the app outright.
    #[test]
    fn walking_past_the_end_of_the_tree_stops_rather_than_panicking() {
        let tree = Tree::placeholder(PathBuf::from("root"));

        // The root has no children at all, so every one of these indices
        // is past the end. The exact forms say so rather than inventing
        // an answer about the root or anything else.
        assert_eq!(tree.path_for(&[0]), None);
        assert_eq!(tree.path_for(&[3, 7, 11]), None);
        assert!(tree.node_for(&[0]).is_none());
        assert!(tree.node_for(&[3, 7, 11]).is_none());
        assert!(tree.node_for(&[]).is_some());

        // The forgiving forms exist, and are explicit about being
        // forgiving: the deepest node the stale path reaches is the
        // root.
        assert_eq!(tree.valid_prefix(&[0]), Vec::<usize>::new());
        assert_eq!(tree.deepest_valid_node(&[3, 7, 11]).name, tree.root.name);
        assert_eq!(tree.deepest_valid_path(&[3, 7, 11]), PathBuf::from("root"));
    }
}
