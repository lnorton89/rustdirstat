// ============================================================================
// Module:       tui::app::navigation
// Description:  Moving around the tree: where the browser is, what it lists, and
//               restoring a position across a rescan.
//
// Dependencies: crate::model; super::App
// ============================================================================

//! Moving around the tree: where the browser currently is, what it
//! lists there, and how a position is restored across a rescan.

use super::*;

impl App {
    /// Restore browsing to whatever directory `target` was pointing at
    /// before a rescan, matching by path since node indices aren't stable
    /// across scans. Falls back to the root if it can't be found (e.g. the
    /// directory was deleted).
    pub(in crate::tui) fn restore_path(&mut self, target: &std::path::Path) {
        let mut node = &self.tree.root;
        let mut indices = Vec::new();
        let mut current = self.tree.root_path.clone();
        if current == target {
            self.path_indices = indices;
            self.selected = 0;
            self.refresh_ext_stats();
            return;
        }
        loop {
            let mut found = None;
            for (i, c) in node.children.iter().enumerate() {
                let candidate = current.join(&c.name);
                if target == candidate || target.starts_with(&candidate) {
                    found = Some((i, c, candidate));
                    break;
                }
            }
            match found {
                Some((i, c, candidate)) => {
                    indices.push(i);
                    node = c;
                    current = candidate;
                    if current == target {
                        break;
                    }
                }
                None => break,
            }
        }
        self.path_indices = indices;
        self.selected = 0;
        self.refresh_ext_stats();
    }

    pub(in crate::tui) fn current_node(&self) -> &Node {
        // Forgiving: the current directory is where the user is, and a
        // stale position should still show the place it now points at.
        self.tree.deepest_valid_node(&self.path_indices)
    }

    pub(in crate::tui) fn current_path(&self) -> PathBuf {
        self.tree.deepest_valid_path(&self.path_indices)
    }

    /// Children of the current directory, filtered by the active search
    /// term and sorted for display, paired with their index in the
    /// original (unsorted) `children` vec so navigation and deletion stay
    /// stable regardless of sort/filter.
    pub(in crate::tui) fn display_children(&self) -> Vec<(usize, &Node)> {
        let node = self.current_node();
        let mut v: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            v.retain(|(_, n)| n.name.to_string_lossy().to_lowercase().contains(&f));
        }
        // `use_physical`, not always-logical: this used to sort by
        // `size` whatever the view was showing, so pressing `p` swapped
        // every number in the list without reordering a single row.
        crate::model::sort_nodes(&mut v, self.sort, self.use_physical);
        v
    }

    pub(in crate::tui) fn on_filter_changed(&mut self) {
        self.selected = 0;
        if self.show_top_files {
            self.refresh_top_files();
        }
    }

    /// If the "biggest files" or search-results flat view is active, jump
    /// browsing to the currently selected entry's actual parent directory
    /// (and select it there), then leave the flat view — so every action
    /// that operates on "the selected row" (delete, open, Enter) works the
    /// same regardless of which view found that row.
    pub(in crate::tui) fn exit_flat_view_if_needed(&mut self) {
        if self.show_top_files {
            if let Some(tf) = self.top_files_cache.get(self.selected) {
                let idx_path = tf.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.show_top_files = false;
        } else if self.search.visible {
            if let Some(hit) = self.search.results.get(self.selected) {
                let idx_path = hit.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.search.visible = false;
        } else if self.duplicates.visible {
            // Unlike the top-files/search rows above, not every row here is
            // a navigable item — group headers are rows too. Landing on one
            // and leaving `selected` unchanged would carry a duplicates-list
            // row index into the browse view's unrelated child list, so any
            // non-member row resets it instead of leaving it stale.
            match self.duplicates.rows.get(self.selected) {
                Some(DupRow::Member { index_path }) => {
                    let idx_path = index_path.clone();
                    self.navigate_to_absolute(idx_path);
                }
                _ => self.selected = 0,
            }
            self.duplicates.visible = false;
        }
    }

    /// Jump the browser to the item identified by `index_path` (as produced
    /// by the recursive treemap or the biggest-files view), landing on its
    /// parent directory with the item itself selected.
    pub(in crate::tui) fn navigate_to(&mut self, mut index_path: Vec<usize>) {
        if index_path.is_empty() {
            return;
        }
        let target = index_path.remove(index_path.len() - 1);
        self.path_indices.extend(index_path);
        self.selected = 0;
        self.refresh_ext_stats();
        if let Some(pos) = self
            .display_children()
            .iter()
            .position(|(idx, _)| *idx == target)
        {
            self.selected = pos;
        }
    }

    /// Like `navigate_to`, but for an `index_path` rooted at the whole
    /// tree rather than the currently browsed directory — needed for
    /// duplicate results, which are found by scanning from `tree.root`,
    /// not from `current_node()` the way search/top-files results are.
    pub(in crate::tui) fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        self.path_indices.clear();
        self.navigate_to(index_path);
    }
}
