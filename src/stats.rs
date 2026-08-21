// ============================================================================
// Module:       stats
// Description:  The per-category size and count breakdown for a subtree, read
//               straight out of the totals the scan already rolled up.
//
// Dependencies: crate::model::Node, crate::color::Category
// ============================================================================

//! The per-category size and count breakdown for a subtree.
//!
//! A direct read of the totals [`crate::scanner`] already rolled up
//! bottom-up, not a fresh walk — which is what keeps it instant at the
//! root of a whole drive.
//!
//! [`ExtStat`] carries a single already-resolved `size` rather than both
//! the logical and physical totals, so every consumer of it — the
//! legend's percentages, its sort order — automatically agrees with
//! whichever mode the rest of the screen is showing, instead of being a
//! second place that can drift out of sync with the physical-size toggle.

use crate::color::Category;
use crate::model::Node;

pub struct ExtStat {
    pub category: Category,
    /// The size to actually show — logical or physical, whichever
    /// `extension_stats` was asked for. Kept as a single already-resolved
    /// field (rather than both totals) so every consumer (the legend's
    /// percentages, its sort order) automatically agrees with whichever
    /// mode the rest of the screen — header, file list, treemap — is
    /// currently showing, instead of a second place that could drift out
    /// of sync with the `use_physical` toggle.
    pub size: u64,
    pub count: u64,
}

/// Extension/category breakdown for `node`'s subtree. This is a direct read
/// of the totals precomputed bottom-up at scan time (see `scanner::scan_dir`)
/// — no re-walking the subtree, so it stays instant even for a directory
/// with millions of descendants.
pub fn extension_stats(node: &Node, use_physical: bool) -> Vec<ExtStat> {
    let mut v: Vec<ExtStat> = Category::ALL
        .iter()
        .enumerate()
        .filter_map(|(i, &category)| {
            let (logical, physical, count) = *node.ext_totals.get(i)?;
            if count > 0 {
                Some(ExtStat {
                    category,
                    size: if use_physical { physical } else { logical },
                    count,
                })
            } else {
                None
            }
        })
        .collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.size));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory node carrying the per-category totals the scan rolls
    /// up. `pairs` is (category, logical, physical, count).
    fn node_with(pairs: &[(Category, u64, u64, u64)]) -> Node {
        let mut totals = vec![(0_u64, 0_u64, 0_u64); Category::COUNT];
        for &(category, logical, physical, count) in pairs {
            let slot = category.index();
            if let Some(entry) = totals.get_mut(slot) {
                entry.0 += logical;
                entry.1 += physical;
                entry.2 += count;
            }
        }
        Node {
            name: std::ffi::OsString::from("dir"),
            is_dir: true,
            is_symlink: false,
            size: pairs.iter().map(|p| p.1).sum(),
            physical_size: pairs.iter().map(|p| p.2).sum(),
            file_count: pairs.iter().map(|p| p.3).sum(),
            dir_count: 0,
            modified: None,
            children: Vec::new(),
            error: false,
            category: None,
            ext_totals: totals,
            unreadable_count: 0,
            file_id: None,
            other_filesystem: false,
        }
    }

    /// Only categories that actually have files show up.
    ///
    /// A legend listing every category with nothing in it is nine rows of
    /// zeroes, and the whole point of the panel is what is filling the
    /// disk.
    #[test]
    fn categories_with_no_files_are_left_out() {
        let node = node_with(&[(Category::Source, 500, 500, 5), (Category::Images, 0, 0, 0)]);
        let stats = extension_stats(&node, false);
        assert_eq!(stats.len(), 1, "only the category with files should appear");
        assert_eq!(stats.first().map(|s| s.category), Some(Category::Source));
    }

    /// The size reported follows the logical/physical toggle, because
    /// every consumer reads this one field.
    #[test]
    fn the_reported_size_follows_the_size_mode() {
        // A small file on a large cluster: 100 bytes of content taking a
        // 4 KiB block.
        let node = node_with(&[(Category::Documents, 100, 4096, 1)]);

        let logical = extension_stats(&node, false);
        assert_eq!(logical.first().map(|s| s.size), Some(100));

        let physical = extension_stats(&node, true);
        assert_eq!(physical.first().map(|s| s.size), Some(4096));

        // The count is the same either way — only the size mode changes.
        assert_eq!(
            logical.first().map(|s| s.count),
            physical.first().map(|s| s.count)
        );
    }

    /// Biggest first, and by the size mode actually in use.
    ///
    /// Sorting by the logical total while displaying physical ones would
    /// put the legend in an order its own numbers contradict.
    #[test]
    fn stats_are_sorted_by_the_size_being_shown() {
        // Physical order is the reverse of logical: many tiny files take
        // more space on disk than one slightly larger file.
        let node = node_with(&[
            (Category::Source, 900, 4_096, 1),
            (Category::Documents, 800, 40_960, 10),
        ]);

        let logical = extension_stats(&node, false);
        assert_eq!(
            logical.iter().map(|s| s.category).collect::<Vec<_>>(),
            vec![Category::Source, Category::Documents],
            "by logical size, Source is larger"
        );

        let physical = extension_stats(&node, true);
        assert_eq!(
            physical.iter().map(|s| s.category).collect::<Vec<_>>(),
            vec![Category::Documents, Category::Source],
            "by physical size, Documents is larger — the order must follow"
        );
    }

    /// A node with no totals at all (a file, or an unreadable directory)
    /// yields nothing rather than panicking on a short array.
    #[test]
    fn a_node_without_totals_yields_nothing() {
        let mut node = node_with(&[(Category::Source, 1, 1, 1)]);
        node.ext_totals = Vec::new();
        assert!(extension_stats(&node, false).is_empty());
    }
}
