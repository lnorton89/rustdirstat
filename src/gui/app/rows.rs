// ============================================================================
// Module:       gui::app::rows
// Description:  The flattened row list the directory table draws, and the
//               state deciding its shape: what is expanded, selected, sorted.
//
// Dependencies: crate::model (tree walk); super::GuiApp
// ============================================================================

//! The flattened row list the directory table draws, and the state
//! that decides its shape: what is expanded, what is selected, how it is
//! sorted.
//!
//! The cache is keyed off observed state rather than invalidated by
//! hand, and debug builds check the cache against a fresh build whenever
//! it claims a hit — see `cached_rows_are_current`.

use super::*;

/// One row of the directory table, flattened out of the tree.
///
/// Lives here rather than with the table-painting code because it is
/// derived model state that gets cached across frames — see
/// [`GuiApp::refresh_visible_rows`].
#[derive(Clone)]
pub(in crate::gui) struct TreeRow {
    pub path: Vec<usize>,
    pub depth: usize,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub parent_size: u64,
    pub files: u64,
    pub dirs: u64,
    pub modified: Option<std::time::SystemTime>,
    pub unreadable: u64,
    pub symlink: bool,
}

impl TreeRow {
    /// Field-by-field equality, used only by the cache check.
    ///
    /// Written as an exhaustive destructure rather than `#[derive(PartialEq)]`
    /// so that adding a field to `TreeRow` fails to compile here until
    /// someone decides whether a change in it should count as the rows
    /// having changed.
    ///
    /// Debug-only, like its one caller: the cache check it serves does
    /// not run in release, and the crate denies dead code, so a release
    /// build fails on it otherwise.
    #[cfg(debug_assertions)]
    fn same_as(&self, other: &Self) -> bool {
        let Self {
            path,
            depth,
            name,
            is_dir,
            size,
            parent_size,
            files,
            dirs,
            modified,
            unreadable,
            symlink,
        } = self;
        path == &other.path
            && depth == &other.depth
            && name == &other.name
            && is_dir == &other.is_dir
            && size == &other.size
            && parent_size == &other.parent_size
            && files == &other.files
            && dirs == &other.dirs
            && modified == &other.modified
            && unreadable == &other.unreadable
            && symlink == &other.symlink
    }
}

/// Everything the flattened row list depends on.
///
/// This is compared against per frame rather than invalidated by hand at
/// each mutation site. The set of things that can change the row list is
/// large and spread across the UI code — every expand/collapse, every
/// sort click, the size-mode toggle, and every rescan — so hand-written
/// invalidation is one missed call away from painting a stale tree, a bug
/// that would look like the app ignoring input. Deriving the key from
/// observable state instead cannot go stale by construction.
#[derive(PartialEq)]
pub(in crate::gui) struct RowKey {
    tree: usize,
    sort: SortMode,
    physical: bool,
    expanded: u64,
}

/// Order-independent fingerprint of the expanded-directory set.
///
/// A `HashSet` has no stable iteration order, so the paths are folded
/// together commutatively. XOR alone would cancel a pair of equal
/// hashes, so a wrapping sum and the element count are mixed in too.
pub(in crate::gui) fn expanded_fingerprint(expanded: &HashSet<Vec<usize>>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut xor = 0_u64;
    let mut sum = 0_u64;
    for path in expanded {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        let hash = hasher.finish();
        xor ^= hash;
        sum = sum.wrapping_add(hash);
    }
    xor ^ sum.rotate_left(17) ^ (expanded.len() as u64)
}

pub(in crate::gui) fn push_tree_rows(
    node: &Node,
    path: Vec<usize>,
    depth: usize,
    parent_size: u64,
    display_name: String,
    app: &GuiApp,
    out: &mut Vec<TreeRow>,
) {
    out.push(TreeRow {
        path: path.clone(),
        depth,
        name: display_name,
        is_dir: node.is_dir,
        size: node.effective_size(app.use_physical),
        parent_size,
        files: node.file_count,
        dirs: node.dir_count,
        modified: node.modified,
        unreadable: node.unreadable_count,
        symlink: node.is_symlink,
    });
    if !node.is_dir || !app.expanded.contains(&path) {
        return;
    }
    let mut children: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
    sort_nodes(&mut children, app.sort, app.use_physical);
    let node_size = node.effective_size(app.use_physical).max(1);
    for (idx, child) in children {
        let mut child_path = path.clone();
        child_path.push(idx);
        push_tree_rows(
            child,
            child_path,
            depth + 1,
            node_size,
            child.name.clone(),
            app,
            out,
        );
    }
}

// The ordering itself lives on the model, beside the nodes it orders —
// the terminal front end sorts the same six ways over the same type, and
// keeping two copies of the match is what let them drift apart.
pub(in crate::gui) use crate::model::sort_nodes;

impl GuiApp {
    pub(in crate::gui) fn selected_node(&self) -> Option<&Node> {
        self.selected_path.as_deref().map(|p| self.tree.node_for(p))
    }

    pub(in crate::gui) fn selected_fs_path(&self) -> Option<PathBuf> {
        self.selected_path.as_deref().map(|p| self.tree.path_for(p))
    }

    pub(in crate::gui) fn select_path(&mut self, path: Vec<usize>) {
        self.expand_ancestors(&path);
        self.selected_path = Some(path.clone());
        let selected = self.tree.node_for(&path);
        if !selected.is_dir {
            self.highlighted_extension = Some(extension_label(&selected.name));
            self.highlighted_category = selected.category;
        } else {
            self.highlighted_extension = None;
            self.highlighted_category = None;
        }
    }

    pub(in crate::gui) fn toggle_expanded(&mut self, path: &[usize]) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_vec());
        }
    }

    pub(in crate::gui) fn expand_ancestors(&mut self, path: &[usize]) {
        self.expanded.insert(Vec::new());
        for len in 1..path.len() {
            self.expanded.insert(path[..len].to_vec());
        }
    }

    /// Rebuilds [`Self::visible_rows`] if the tree, sort order, size mode,
    /// or expanded set has changed since the last frame.
    pub(in crate::gui) fn refresh_visible_rows(&mut self) {
        let key = RowKey {
            tree: Arc::as_ptr(&self.tree) as usize,
            sort: self.sort,
            physical: self.use_physical,
            expanded: expanded_fingerprint(&self.expanded),
        };
        if self.visible_rows_key.as_ref() == Some(&key) {
            debug_assert!(
                self.cached_rows_are_current(),
                "the row cache reported a hit but the rows it holds are not the rows \
                 this state produces — some field the row list depends on is missing \
                 from `RowKey`"
            );
            return;
        }

        self.visible_rows = self.build_visible_rows();
        self.visible_rows_key = Some(key);
    }

    /// Builds the flattened row list from current state, ignoring the
    /// cache entirely.
    fn build_visible_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let root_name = crate::util::display_path(&self.tree.root_path);
        push_tree_rows(
            &self.tree.root,
            Vec::new(),
            0,
            self.tree.root.effective_size(self.use_physical).max(1),
            root_name,
            self,
            &mut rows,
        );
        rows
    }

    /// Whether the cached rows match what current state would produce.
    ///
    /// `RowKey` is compared against per frame rather than invalidated by
    /// hand, which cannot go stale for the fields it *contains* — but
    /// nothing stopped a new `GuiApp` field affecting the row list and
    /// never being added to the key, and the symptom of that is the
    /// table quietly showing rows from before the change. Debug builds
    /// therefore check the cache against a fresh build whenever it
    /// claims a hit, which turns "silently stale" into a failed
    /// assertion the first time anyone runs the app or the tests.
    ///
    /// Bounded by row count, not tree size: the row list only covers
    /// expanded directories, so this is normally a few hundred rows. The
    /// cap is there for the case where someone expands a directory with
    /// a million entries in it, where a per-frame rebuild would be felt.
    #[cfg(debug_assertions)]
    fn cached_rows_are_current(&self) -> bool {
        const VERIFY_LIMIT: usize = 5_000;
        if self.visible_rows.len() > VERIFY_LIMIT {
            return true;
        }
        let fresh = self.build_visible_rows();
        fresh.len() == self.visible_rows.len()
            && fresh
                .iter()
                .zip(&self.visible_rows)
                .all(|(a, b)| a.same_as(b))
    }

    #[cfg(not(debug_assertions))]
    fn cached_rows_are_current(&self) -> bool {
        true
    }
}
