// ============================================================================
// Module:       tui::app::operations
// Description:  Everything that changes something outside the app: deleting,
//               emptying, moving, exporting, and the Windows maintenance tools.
//
// Dependencies: trash, crate::{csv_export, report, util, wintools}; super::App
// ============================================================================

//! Everything that reaches outside the app: deleting, emptying,
//! moving, exporting, and running a Windows maintenance tool.
//!
//! A queued deletion is resolved through `try_node_for` before anything
//! is removed. An index path only means something against the tree it
//! came from, and the infallible lookup answers about the deepest node
//! that exists — which for a delete would mean acting on the parent.

use super::*;

/// Shown when a queued deletion names something the tree no longer has.
///
/// The usual way to get one is a rescan between queuing the delete and
/// confirming it: index paths are only meaningful against the tree they
/// came from.
pub(in crate::tui) const STALE_TARGET: &str =
    "That item is no longer in the scan — rescan and try again";

pub(in crate::tui) enum RemovedExt {
    File(Option<Category>),
    Dir(Vec<(u64, u64, u64)>),
}

pub(in crate::tui) fn subtract_totals(
    n: &mut Node,
    size: u64,
    physical_size: u64,
    file_count: u64,
    dir_count: u64,
    unreadable_count: u64,
    ext: &RemovedExt,
) {
    // Saturating throughout. These aggregates are maintained by hand as
    // deletions come in, so a disagreement between them and the delta
    // being applied is possible in a way it is not for a freshly scanned
    // tree — and plain `-=` turns that into an underflow panic in debug
    // and a wrapped, enormous total in release. Clamping at zero is
    // wrong by whatever the discrepancy was; the alternatives are wrong
    // by 18 quintillion or not running at all.
    n.size = n.size.saturating_sub(size);
    n.physical_size = n.physical_size.saturating_sub(physical_size);
    n.file_count = n.file_count.saturating_sub(file_count);
    n.dir_count = n.dir_count.saturating_sub(dir_count);
    n.unreadable_count = n.unreadable_count.saturating_sub(unreadable_count);
    match ext {
        RemovedExt::File(Some(cat)) => {
            if let Some(total) = n.ext_totals.get_mut(cat.index()) {
                total.0 = total.0.saturating_sub(size);
                total.1 = total.1.saturating_sub(physical_size);
                total.2 = total.2.saturating_sub(1);
            }
        }
        RemovedExt::File(None) => {}
        RemovedExt::Dir(totals) => {
            for (i, &(s, p, c)) in totals.iter().enumerate() {
                let Some(total) = n.ext_totals.get_mut(i) else {
                    break;
                };
                total.0 = total.0.saturating_sub(s);
                total.1 = total.1.saturating_sub(p);
                total.2 = total.2.saturating_sub(c);
            }
        }
    }
}

impl App {
    pub(in crate::tui) fn request_delete(&mut self, permanent: bool) {
        self.exit_flat_view_if_needed();
        if let Some((idx, node)) = self.display_children().get(self.selected) {
            self.pending_delete = Some(PendingDelete {
                orig_idx: *idx,
                name: node.name.clone(),
                permanent,
                is_dir: node.is_dir,
            });
        }
    }

    pub(in crate::tui) fn export_report(&mut self) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("rustdirstat-report-{secs}.txt");
        let path = self.current_path();
        match crate::report::write_report_to_file(
            &path,
            self.current_node(),
            50,
            4,
            std::path::Path::new(&filename),
        ) {
            Ok(()) => self.message = Some(format!("Report written to {filename}")),
            Err(e) => self.message = Some(format!("Failed to write report: {e}")),
        }
    }

    pub(in crate::tui) fn export_csv(&mut self) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("rustdirstat-export-{secs}.csv");
        let path = self.current_path();
        match crate::csv_export::write_csv_to_file(
            &path,
            self.current_node(),
            std::path::Path::new(&filename),
        ) {
            Ok(()) => self.message = Some(format!("CSV written to {filename}")),
            Err(e) => self.message = Some(format!("Failed to write CSV: {e}")),
        }
    }

    pub(in crate::tui) fn run_wintool(&mut self, idx: usize) {
        let Some(tool) = crate::wintools::TOOLS.get(idx) else {
            return;
        };
        match crate::wintools::run(idx, &self.tree.root_path) {
            // The TUI has one status line and no scrollback panel to put
            // a report in, so a tool's `detail` is folded onto the end of
            // its summary rather than dropped — truncated by the status
            // line if it does not fit, which still beats discarding the
            // one thing an analyze-only tool was run to produce.
            Ok(output) if output.detail.is_empty() => self.message = Some(output.summary),
            Ok(output) => {
                let detail = output.detail.replace(char::is_whitespace, " ");
                self.message = Some(format!("{}: {}", output.summary, detail.trim()));
            }
            Err(e) => self.message = Some(format!("{}: {e}", tool.name)),
        }
        self.wintools.visible = false;
    }

    /// Moves the selected item to the folder (or exact path) typed into
    /// the move prompt. On success, triggers a full rescan rather than
    /// patching totals in place the way delete does — unlike a delete,
    /// the destination might land inside the currently scanned tree too
    /// (or on a different volume entirely), and correctly reflecting
    /// either case without a real re-scan isn't worth the complexity for
    /// an action this infrequent.
    pub(in crate::tui) fn perform_move(&mut self) {
        self.move_to.entry_mode = false;
        let dest_input = self.move_to.destination.trim().to_string();
        if dest_input.is_empty() {
            return;
        }
        let Some((orig_idx, name)) = self
            .display_children()
            .get(self.selected)
            .map(|(idx, n)| (*idx, n.name.clone()))
        else {
            return;
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(orig_idx);
        let source = self.tree.path_for(&full_index_path);

        let dest_base = std::path::PathBuf::from(&dest_input);
        let dest = if dest_base.is_dir() {
            dest_base.join(&name)
        } else {
            dest_base
        };

        match crate::util::move_path(&source, &dest) {
            Ok(()) => {
                self.message = Some(format!("Moved to {}", dest.display()));
                self.refresh_requested = true;
            }
            Err(e) => self.message = Some(format!("Move failed: {e}")),
        }
    }

    pub(in crate::tui) fn confirm_delete(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        // Resolved fallibly, before anything is deleted. An index path
        // only means something against the tree it was taken from, and
        // `node_for` answers about the deepest node that does exist —
        // which for a delete would mean confirming against the *parent*
        // directory and removing that instead.
        let Some(target) = self.tree.try_node_for(&full_index_path) else {
            self.message = Some(STALE_TARGET.to_string());
            return Ok(());
        };
        let path = self.tree.path_for(&full_index_path);

        let unreadable_delta = target.unreadable_count;
        let physical_delta = target.physical_size;
        let (is_dir, size, file_count, dir_count_delta, removed_ext) = if target.is_dir {
            (
                true,
                target.size,
                target.file_count,
                target.dir_count + 1,
                RemovedExt::Dir(target.ext_totals.clone()),
            )
        } else {
            (false, target.size, 1, 0, RemovedExt::File(target.category))
        };

        if pending.permanent {
            if is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        } else {
            trash::delete(&path).map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }

        let mut n = &mut self.tree.root;
        subtract_totals(
            n,
            size,
            physical_delta,
            file_count,
            dir_count_delta,
            unreadable_delta,
            &removed_ext,
        );
        for &idx in &self.path_indices {
            // Bounds-checked, then indexed. `get_mut` would be the
            // natural spelling, but it returns a value whose borrow the
            // compiler cannot shorten across this loop, which stops `n`
            // being used at all afterwards; indexing is a place
            // expression and reborrows cleanly. The check above it is
            // what makes the index infallible rather than merely
            // unlikely to fail.
            if idx >= n.children.len() {
                break;
            }
            n = &mut n.children[idx];
            subtract_totals(
                n,
                size,
                physical_delta,
                file_count,
                dir_count_delta,
                unreadable_delta,
                &removed_ext,
            );
        }
        if pending.orig_idx < n.children.len() {
            n.children.remove(pending.orig_idx);
        }

        let len = self.display_children().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.refresh_ext_stats();
        if self.show_top_files {
            self.refresh_top_files();
        }
        let verb = if pending.permanent {
            "Permanently deleted"
        } else {
            "Moved to trash"
        };
        self.message = Some(format!("{verb}: {}", path.display()));
        Ok(())
    }

    /// Deletes a directory's contents (each direct child moved to trash)
    /// while keeping the directory itself, unlike `confirm_delete` which
    /// removes the node from its parent entirely. The node's own
    /// aggregates are zeroed out in place rather than the node being
    /// removed, so it still shows up afterward — just empty.
    pub(in crate::tui) fn confirm_empty(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };
        if !pending.is_dir {
            return Ok(());
        }

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        // Same as `confirm_delete`: a stale path must not resolve to the
        // nearest surviving ancestor and empty *that*.
        let Some(target) = self.tree.try_node_for(&full_index_path) else {
            self.message = Some(STALE_TARGET.to_string());
            return Ok(());
        };
        let path = self.tree.path_for(&full_index_path);

        let unreadable_delta = target.unreadable_count;
        let physical_delta = target.physical_size;
        let size = target.size;
        let file_count = target.file_count;
        let dir_count_delta = target.dir_count;
        let removed_ext = RemovedExt::Dir(target.ext_totals.clone());

        let children: Vec<std::path::PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        for child_path in children {
            trash::delete(&child_path)
                .map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }

        let mut n = &mut self.tree.root;
        subtract_totals(
            n,
            size,
            physical_delta,
            file_count,
            dir_count_delta,
            unreadable_delta,
            &removed_ext,
        );
        for &idx in &self.path_indices {
            // Bounds-checked, then indexed. `get_mut` would be the
            // natural spelling, but it returns a value whose borrow the
            // compiler cannot shorten across this loop, which stops `n`
            // being used at all afterwards; indexing is a place
            // expression and reborrows cleanly. The check above it is
            // what makes the index infallible rather than merely
            // unlikely to fail.
            if idx >= n.children.len() {
                break;
            }
            n = &mut n.children[idx];
            subtract_totals(
                n,
                size,
                physical_delta,
                file_count,
                dir_count_delta,
                unreadable_delta,
                &removed_ext,
            );
        }
        let Some(node) = n.children.get_mut(pending.orig_idx) else {
            return Ok(());
        };
        node.size = 0;
        node.physical_size = 0;
        node.file_count = 0;
        node.dir_count = 0;
        node.unreadable_count = 0;
        node.ext_totals = vec![(0u64, 0u64, 0u64); Category::COUNT];
        node.children.clear();

        let len = self.display_children().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.refresh_ext_stats();
        if self.show_top_files {
            self.refresh_top_files();
        }
        self.message = Some(format!("Emptied: {}", path.display()));
        Ok(())
    }
}
