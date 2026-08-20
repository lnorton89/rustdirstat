// ============================================================================
// Module:       tui::app::views
// Description:  The alternate views over a scan: extension stats, largest files,
//               subtree search, and duplicate groups.
//
// Dependencies: crate::{stats, tui::search, tui::top_files}; super::App
// ============================================================================

//! The alternate views over a scan — extension stats, largest files,
//! subtree search results, duplicate groups — and the caches behind
//! them.

use super::*;

impl App {
    /// Caps how many duplicate groups are turned into list rows — the
    /// underlying scan can find far more than is sensible to hand to
    /// ratatui's list widget every frame; the most-impactful groups (by
    /// wasted space) are already sorted first, so this only ever drops the
    /// long tail of smaller groups.
    pub(in crate::tui) const MAX_DUPLICATE_DISPLAY_GROUPS: usize = 500;

    pub(in crate::tui) fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node(), self.use_physical);
    }

    pub(in crate::tui) fn refresh_top_files(&mut self) {
        let mut out = top_files::top_k(self.current_node(), TOP_FILES_LIMIT);
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            out.retain(|t| t.name.to_lowercase().contains(&f));
        }
        self.top_files_cache = out;
    }

    pub(in crate::tui) fn set_duplicate_results(&mut self, scan: crate::duplicates::DupScan) {
        let groups = scan.groups;
        self.duplicates.skipped = scan.skipped;
        self.duplicates.group_count = groups.len();
        self.duplicates.truncated = groups.len() > Self::MAX_DUPLICATE_DISPLAY_GROUPS;
        self.duplicates.total_wasted = groups
            .iter()
            .map(|g| g.size * (g.files.len() as u64 - 1))
            .sum();

        let mut rows = Vec::new();
        for group in groups.into_iter().take(Self::MAX_DUPLICATE_DISPLAY_GROUPS) {
            rows.push(DupRow::Header {
                size: group.size,
                count: group.files.len(),
            });
            for f in group.files {
                rows.push(DupRow::Member {
                    index_path: f.index_path,
                });
            }
        }
        self.duplicates.rows = rows;
        self.duplicates.visible = true;
        self.search.visible = false;
        self.show_top_files = false;
        self.selected = 0;
    }

    pub(in crate::tui) fn run_subtree_search(&mut self) {
        let outcome = search::search(self.current_node(), &self.search.query);
        self.search.error = outcome.error;
        self.search.truncated = outcome.truncated;
        self.search.results = outcome.hits;
        self.search.visible = true;
        self.search.entry_mode = false;
        self.selected = 0;
    }
}
