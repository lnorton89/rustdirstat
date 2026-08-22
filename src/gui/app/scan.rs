// ============================================================================
// Module:       gui::app::scan
// Description:  Starting, replacing and retiring a scanned tree, including the
//               off-thread teardown a drive-sized tree needs.
//
// Dependencies: crate::scanner; std::sync::mpsc; super::GuiApp
// ============================================================================

//! Starting, replacing and retiring a scanned tree.
//!
//! A scan arrives on a background thread as a `ScanOutcome`, already
//! carrying the derived tables so the UI thread never computes them. The
//! outgoing tree is freed on a background thread too: a drive-sized tree
//! takes long enough to drop that doing it inline stalls the window.

use super::*;

/// A finished scan, with the two whole-tree summaries already computed.
///
/// Both used to be derived on the UI thread the instant the tree landed.
/// `top_k` and the extension roll-up each walk every node, so on a large
/// drive that was seconds of frozen window at exactly the moment the scan
/// appeared to finish — the app looked like it hung and then snapped into
/// place. The scan thread has the tree first and nothing else to do with
/// it, so it does this work before handing anything over.
pub(in crate::gui) struct ScanOutcome {
    tree: Tree,
    extensions: Vec<ExtensionRow>,
    largest_files: Vec<top_files::TopFile>,
}

/// What a scan thread sends back.
///
/// `Cancelled` is its own arm rather than an `Err`: the user asked for
/// this, and reporting "Scan failed" because they pressed Cancel is the
/// kind of wrong that erodes trust in every other message the status bar
/// prints.
pub(in crate::gui) enum ScanMessage {
    Done(Box<ScanOutcome>),
    Cancelled,
    Failed(String),
}

/// Hands a value off to a detached thread to be dropped there.
///
/// Freeing a scanned tree is not cheap: it walks every node and returns
/// millions of individual allocations — one per name, one per child list,
/// one per directory's category totals — to the allocator. A whole-drive
/// scan is over a second of pure teardown even with every page resident,
/// and far worse once the working set has been paged out, because the
/// allocator has to fault all of it back in just to release it. Doing
/// that on the UI thread is what made rescanning a large drive hitch and
/// made closing the window look like a hang.
///
/// Nothing observable depends on *when* the memory comes back, so it can
/// happen off to the side. At process exit the reclaim thread is killed
/// wherever it happens to be, which is fine — the OS releases the whole
/// address space regardless, and that is the point: the teardown is work
/// nobody is waiting for.
pub(in crate::gui) fn drop_in_background<T: Send + 'static>(value: T) {
    // If the spawn itself fails, `spawn` drops the closure — and with it
    // the value — right here, which is the correct fallback.
    let _ = std::thread::Builder::new()
        .name("rustdirstat-reclaim".to_owned())
        .spawn(move || drop(value));
}

/// Where the user is, identified by the sequence of raw names from the
/// root down — the *identity* of a place, as opposed to the `Vec<usize>`
/// index path that only means anything against the tree it was taken
/// from.
///
/// Carried across a refresh scan. Filesystem enumeration order is not
/// stable between scans, so the index a directory held last time may now
/// name a different directory — restoring selection by index would
/// silently land the user on a different filesystem object. Names are
/// the identity; index paths are re-derived from them against the new
/// tree.
pub(in crate::gui) struct RestoreState {
    zoom: Vec<std::ffi::OsString>,
    selected: Option<Vec<std::ffi::OsString>>,
    expanded: Vec<Vec<std::ffi::OsString>>,
}

/// The name components along `index_path`, from the root down.
///
/// `None` only if the path runs off the end of the tree, which means the
/// view was already pointing somewhere invalid.
fn capture_identity(tree: &Tree, index_path: &[usize]) -> Option<Vec<std::ffi::OsString>> {
    let mut components = Vec::new();
    let mut node = &tree.root;
    for &idx in index_path {
        node = node.children.get(idx)?;
        components.push(node.name.clone());
    }
    Some(components)
}

/// The fresh index path for `identity` against `tree`, stopping at the
/// deepest component that still exists.
///
/// Matching is by name, not position — sibling *order* is exactly what
/// is not stable between scans, and position is what a stale `Vec<usize>`
/// would have answered about.
///
/// Forgiving by design: a place that vanished resolves to the place that
/// still exists nearest it. That is the right answer for the *zoom*
/// level — the user is looking at a region, and the region's nearest
/// surviving ancestor is still that region's neighborhood — and for
/// `expanded` directories. It is the wrong answer for a *selection*:
/// see [`resolve_identity_exact`].
fn resolve_identity(tree: &Tree, identity: &[std::ffi::OsString]) -> Vec<usize> {
    let mut resolved = Vec::new();
    let mut node = &tree.root;
    for component in identity {
        let Some((idx, _)) = node
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| child.name == *component)
        else {
            break;
        };
        resolved.push(idx);
        node = &node.children[idx];
    }
    resolved
}

/// [`resolve_identity`] that refuses to shorten: `None` unless every
/// component still exists.
///
/// Selection is an exact claim — "this item" — so a file that vanished
/// between scans must not silently become its parent directory, which is
/// what the forgiving form would do. Landing on the parent looks like
/// the selection surviving when the item it named is gone.
fn resolve_identity_exact(tree: &Tree, identity: &[std::ffi::OsString]) -> Option<Vec<usize>> {
    let mut resolved = Vec::new();
    let mut node = &tree.root;
    for component in identity {
        let (idx, child) = node
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| child.name == *component)?;
        resolved.push(idx);
        node = child;
    }
    Some(resolved)
}

impl GuiApp {
    pub(in crate::gui) fn loading(root: PathBuf) -> Self {
        let mut app = Self::new(Tree::placeholder(root.clone()));
        app.start_scan(root, true);
        app
    }

    pub(in crate::gui) fn refresh_scan(&mut self) -> anyhow::Result<()> {
        self.start_scan(self.tree.root_path.clone(), false);
        Ok(())
    }

    pub(in crate::gui) fn open_folder(&mut self, root: &Path) -> anyhow::Result<()> {
        self.start_scan(root.to_path_buf(), true);
        Ok(())
    }

    fn start_scan(&mut self, root: PathBuf, reset_workspace: bool) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        // Remember where the user is — by name identity, not index — so
        // a refresh scan can put them back even though enumeration order
        // may have changed. Opening a different folder resets everything
        // anyway, so only a refresh captures.
        self.restore = if reset_workspace {
            None
        } else {
            self.capture_restore_state()
        };
        let progress = Arc::new(crate::scanner::Progress::default());
        let worker_progress = Arc::clone(&progress);
        let display = crate::util::display_path(&root);
        let (tx, rx) = mpsc::channel();
        let physical = self.use_physical;
        std::thread::spawn(move || {
            let message = match crate::scanner::scan(&root, Some(worker_progress.as_ref())) {
                Ok(crate::scanner::Scan::Completed(tree)) => {
                    let tree = *tree;
                    let extensions = collect_extension_rows(&tree.root, physical);
                    let largest_files = top_files::top_k(&tree.root, 200);
                    ScanMessage::Done(Box::new(ScanOutcome {
                        tree,
                        extensions,
                        largest_files,
                    }))
                }
                Ok(crate::scanner::Scan::Cancelled) => ScanMessage::Cancelled,
                Err(error) => ScanMessage::Failed(error.to_string()),
            };
            let _ = tx.send(message);
        });
        self.scan_progress = Some(progress);
        self.scan_rx = Some(rx);
        self.scan_resets_workspace = reset_workspace;
        self.status = Some(format!("Scanning {display}…"));
    }

    /// Puts the app in the state it is in while a scan runs, without
    /// running one.
    ///
    /// A real scan of a test fixture finishes in microseconds, so a test
    /// that starts one and then tries to cancel it is a race it usually
    /// loses — and a test that only calls `cancel_scan` directly proves
    /// nothing about the button that is supposed to call it. This hands
    /// back the worker end of a scan that will never finish, so the
    /// cancel path can be driven through the real control at leisure.
    #[cfg(test)]
    pub(in crate::gui) fn pretend_scan_is_running(
        &mut self,
    ) -> (mpsc::Sender<ScanMessage>, Arc<crate::scanner::Progress>) {
        let progress = Arc::new(crate::scanner::Progress::default());
        let (tx, rx) = mpsc::channel();
        self.scan_progress = Some(Arc::clone(&progress));
        self.scan_rx = Some(rx);
        self.scan_resets_workspace = false;
        (tx, progress)
    }

    /// The current zoom, selection, and expansion, as name identities.
    fn capture_restore_state(&self) -> Option<RestoreState> {
        let zoom = capture_identity(&self.tree, &self.zoom_path)?;
        let selected = self
            .selected_path
            .as_deref()
            .and_then(|path| capture_identity(&self.tree, path));
        let expanded = self
            .expanded
            .iter()
            .filter_map(|path| capture_identity(&self.tree, path))
            .collect();
        Some(RestoreState {
            zoom,
            selected,
            expanded,
        })
    }

    /// Whether a scan is in flight right now.
    ///
    /// Narrower than [`Self::is_busy`], which also covers duplicate
    /// hashing and the Windows maintenance tools: only a scan answers to
    /// the scan cancel.
    pub(in crate::gui) fn scan_is_running(&self) -> bool {
        self.scan_rx.is_some()
    }

    /// Asks the running scan to stop.
    ///
    /// Returns immediately. The worker notices at its next directory
    /// boundary, sends [`ScanMessage::Cancelled`], and drops its partial
    /// tree on its own thread — waiting for any of that here would freeze
    /// the window for exactly as long as the operation being cancelled,
    /// which is the opposite of the point.
    pub(in crate::gui) fn cancel_scan(&mut self) {
        if let Some(progress) = &self.scan_progress {
            progress.cancel();
            self.status = Some("Cancelling the scan…".to_string());
        }
    }

    pub(in crate::gui) fn is_busy(&self) -> bool {
        self.scan_rx.is_some() || self.duplicate_rx.is_some() || self.tools.rx.is_some()
    }

    pub(in crate::gui) fn busy_text(&self) -> Option<String> {
        if let Some(progress) = &self.scan_progress {
            return Some(format!(
                "Scanning · {} files · {} folders · {}",
                progress.files.load(Ordering::Relaxed),
                progress.dirs.load(Ordering::Relaxed),
                human_bytes(progress.bytes.load(Ordering::Relaxed)),
            ));
        }
        self.duplicate_rx
            .as_ref()
            .map(|_| "Hashing duplicate candidates…".to_string())
            .or_else(|| {
                self.tools
                    .active_name
                    .as_ref()
                    .map(|name| format!("Running {name}…"))
            })
    }

    /// Swaps in a freshly scanned tree, retiring the old one off-thread.
    /// See [`drop_in_background`] for why the old tree is not just dropped
    /// where it stands.
    fn replace_tree(&mut self, tree: Tree) {
        // A queued deletion holds an *index path*, which only means
        // anything against the tree it was taken from. Carrying one
        // across a rescan would resolve it against the new tree and
        // delete whatever now happens to sit at those indices — with
        // `remove_dir_all` on the other end of it. Dropping it here, at
        // the one place trees are ever swapped, is what makes that
        // impossible rather than merely unlikely.
        self.pending_delete = None;
        drop_in_background(std::mem::replace(&mut self.tree, Arc::new(tree)));
    }

    /// Clears everything that describes *where the user was* in a tree,
    /// as opposed to the preferences that should outlive it.
    ///
    /// Scanning a different folder used to leave the extension highlight,
    /// the search box, and the active view pointing at the tree that had
    /// just been thrown away, so a fresh scan opened showing a
    /// highlighted extension that was not in it and a search query with
    /// no results.
    fn reset_workspace(&mut self) {
        self.zoom_path.clear();
        self.selected_path = None;
        self.expanded.clear();
        self.expanded.insert(Vec::new());
        self.file_view = FileView::AllFiles;
        self.highlighted_extension = None;
        self.highlighted_category = None;
        self.search.query.clear();
        self.search.error = None;
    }

    /// Gives up the scanned tree without paying to tear it down, for use
    /// on the way out of the process. Everything the UI derives from the
    /// tree is dropped alongside it, so nothing is left pointing at data
    /// that is no longer there.
    pub(in crate::gui) fn release_tree(&mut self) {
        self.visible_rows = Vec::new();
        self.visible_rows_key = None;
        self.treemap_tiles = Vec::new();
        self.treemap_key = None;
        self.largest_files = Vec::new();
        self.duplicate_groups = Vec::new();
        self.search.results = Vec::new();
        self.search_rx = None;
        // A zoom-time extension worker would answer about the tree just
        // retired; the scan recomputes rows for the new one, and the
        // worker is told to stop walking rather than left to finish.
        self.cancel_extension_worker();
        let root_path = self.tree.root_path.clone();
        drop_in_background(std::mem::replace(
            &mut self.tree,
            Arc::new(Tree::placeholder(root_path)),
        ));
    }

    pub(in crate::gui) fn poll_background(&mut self, ctx: &egui::Context) {
        self.collect_backdrop(ctx);
        let scan_result = self.scan_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(ScanMessage::Failed(
                "The scan worker stopped unexpectedly".to_string(),
            )),
        });
        if let Some(result) = scan_result {
            self.scan_rx = None;
            self.scan_progress = None;
            match result {
                ScanMessage::Done(outcome) => {
                    let reset = self.scan_resets_workspace;
                    let ScanOutcome {
                        tree,
                        extensions,
                        largest_files,
                    } = *outcome;
                    self.replace_tree(tree);
                    if reset {
                        self.reset_workspace();
                    } else {
                        // Restore by identity: names, not indices. Index
                        // paths only mean anything against the tree they
                        // were taken from, and this is a different tree.
                        if let Some(state) = self.restore.take() {
                            self.zoom_path = resolve_identity(&self.tree, &state.zoom);
                            // Selection is exact: a vanished item must not
                            // silently become its parent directory. If it
                            // is gone, it is gone.
                            self.selected_path = state
                                .selected
                                .and_then(|identity| resolve_identity_exact(&self.tree, &identity));
                            self.expanded = state
                                .expanded
                                .iter()
                                .map(|identity| resolve_identity(&self.tree, identity))
                                .collect();
                            self.expanded.insert(Vec::new());
                        } else {
                            // Capture can only fail when the view was
                            // already stale against the old tree — but
                            // stale index paths still must not be carried
                            // onto a brand-new tree unvalidated, which is
                            // the very bug restore-by-name replaced. Same
                            // semantics as the restore above: placement
                            // truncates, selection is exact or dropped.
                            self.zoom_path = self.tree.valid_prefix(&self.zoom_path);
                            self.selected_path = self
                                .selected_path
                                .take()
                                .filter(|path| self.tree.node_for(path).is_some());
                            self.expanded
                                .retain(|path| self.tree.node_for(path).is_some());
                            self.expanded.insert(Vec::new());
                        }
                    }
                    // Already computed on the scan thread; see
                    // `ScanOutcome`. Zooming still recomputes the
                    // extension rows, but that is a subtree and a
                    // deliberate act, not something that lands on the
                    // user unbidden.
                    self.extensions = extensions;
                    self.sort_extensions();
                    self.largest_files = largest_files;
                    // A search in flight answered about the tree that was
                    // just retired; its result is stale by construction.
                    self.search_rx = None;
                    self.search.results.clear();
                    // Same for a zoom-time extension worker: the rows
                    // below are the new tree's, and a late delivery must
                    // not clobber them — and the worker itself is walking
                    // a retired tree, so it is stopped, not just ignored.
                    self.cancel_extension_worker();
                    self.duplicate_groups.clear();
                    self.status = Some("Scan complete".to_string());
                }
                ScanMessage::Cancelled => {
                    // Nothing to restore *to*: the tree on screen is the
                    // one that was already there, untouched, and the
                    // capture taken when this scan started describes it.
                    self.restore = None;
                    self.status = Some("Scan cancelled".to_string());
                }
                ScanMessage::Failed(error) => {
                    self.restore = None;
                    self.status = Some(format!("Scan failed: {error}"));
                }
            }
        }

        let search_result = self.search_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(crate::search::SearchOutcome {
                hits: vec![],
                truncated: false,
                error: Some("The search worker stopped unexpectedly".to_string()),
            }),
        });
        if let Some(outcome) = search_result {
            self.search_rx = None;
            self.search.results = outcome.hits;
            self.search.error = outcome.error;
            self.status = Some(if outcome.truncated {
                "Search capped at 2,000 results".to_string()
            } else {
                format!("{} search result(s)", self.search.results.len())
            });
        }

        let extension_result = self
            .extensions_rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(rows) => Some(Ok(rows)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(())),
            });
        if let Some(result) = extension_result {
            self.extensions_rx = None;
            // The worker is done, one way or the other; its stop flag
            // has nothing left to stop.
            self.extensions_cancel = None;
            // A disconnected worker sent nothing; keep the rows already
            // showing rather than blanking the pane.
            if let Ok(rows) = result {
                self.extensions = rows;
                self.sort_extensions();
            }
        }

        let duplicate_result = self
            .duplicate_rx
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(groups) => Some(Ok(groups)),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("The duplicate worker stopped unexpectedly".to_string()))
                }
            });
        if let Some(result) = duplicate_result {
            self.duplicate_rx = None;
            match result {
                Ok(scan) => {
                    self.duplicate_groups = scan.groups;
                    let groups = self.duplicate_groups.len();
                    // Says so when the search was cut short or files could
                    // not be read. "N duplicate group(s)" reads as the
                    // whole answer when it was not.
                    let mut message = format!("{groups} duplicate group(s)");
                    if scan.skipped > 0 {
                        message.push_str(&format!(
                            " — {} file(s) not checked, the candidate limit was reached",
                            scan.skipped
                        ));
                    }
                    if scan.read_failures > 0 {
                        message.push_str(&format!(
                            " — {} file(s) could not be read",
                            scan.read_failures
                        ));
                    }
                    self.status = Some(message);
                }
                Err(error) => self.status = Some(error),
            }
        }

        let tool_result = self.tools.rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "The maintenance tool worker stopped unexpectedly".to_string(),
            )),
        });
        if let Some(result) = tool_result {
            self.tools.rx = None;
            let tool = self.tools.active_name.take().unwrap_or_default();
            self.tools.running = None;
            let outcome = match result {
                Ok(output) => ToolOutcome {
                    tool,
                    summary: output.summary,
                    detail: output.detail,
                    failed: false,
                },
                Err(error) => ToolOutcome {
                    tool,
                    summary: error,
                    detail: String::new(),
                    failed: true,
                },
            };
            self.status = Some(if outcome.failed {
                format!("Tool failed: {}", outcome.summary)
            } else {
                outcome.summary.clone()
            });
            self.tools.log.push(outcome);
        }

        if self.is_busy() {
            // ~30fps while work is in flight. At the previous 80ms the
            // window only redrew twelve times a second, so anything the
            // user did during a scan — dragging a splitter, moving the
            // window — moved in visible steps even when the machine had
            // capacity to spare.
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::{dir, file};

    /// Rescan restoration matches by name, not by index.
    ///
    /// The old code re-validated `Vec<usize>` index paths against the new
    /// tree, which only answers "do these indices still exist".
    /// Filesystem enumeration order is not stable between scans, so index
    /// 0 can be Downloads in one scan and Documents in the next — and the
    /// restore would silently land the user on the wrong directory. This
    /// pins that a location is captured as name components and resolved
    /// by name against the new tree.
    #[test]
    fn restore_resolves_by_name_across_a_sibling_reorder() -> anyhow::Result<()> {
        let before = Tree {
            root_path: PathBuf::from("root"),
            volume_free: None,
            volume_total: None,
            root: dir(
                "root",
                vec![
                    dir("Downloads", vec![file("a.bin", 1)]),
                    dir("Documents", vec![file("b.bin", 2)]),
                ],
            ),
        };
        // The user was looking at Downloads, which this scan happened to
        // enumerate first.
        let captured = capture_identity(&before, &[0])
            .ok_or_else(|| anyhow::anyhow!("index 0 should exist"))?;
        assert_eq!(captured, [std::ffi::OsString::from("Downloads")]);

        // The next scan of the same folder enumerated the siblings in
        // the other order: index 0 is now Documents, index 1 Downloads.
        let reordered = Tree {
            root_path: PathBuf::from("root"),
            volume_free: None,
            volume_total: None,
            root: dir(
                "root",
                vec![
                    dir("Documents", vec![file("b.bin", 2)]),
                    dir("Downloads", vec![file("a.bin", 1)]),
                ],
            ),
        };
        let restored = resolve_identity(&reordered, &captured);
        assert_eq!(
            restored,
            vec![1],
            "the identity must lead to Downloads, wherever the reorder put it"
        );
        assert_ne!(
            restored,
            vec![0],
            "index 0 is Documents now — the index-based restore the old code \
             performed would have silently landed on the wrong directory"
        );
        Ok(())
    }

    /// An identity whose name is gone entirely resolves to the deepest
    /// ancestor that still exists rather than to whatever now occupies
    /// the old index — for *placement* (zoom, expansion). A selection,
    /// which is an exact claim about a specific item, must instead be
    /// dropped: see the assertions on [`resolve_identity_exact`] below.
    #[test]
    fn a_vanished_name_restores_to_its_ancestor_not_the_index() -> anyhow::Result<()> {
        let before = Tree {
            root_path: PathBuf::from("root"),
            volume_free: None,
            volume_total: None,
            root: dir(
                "root",
                vec![
                    dir("alpha", vec![dir("inner", vec![file("x.bin", 1)])]),
                    dir("beta", vec![file("y.bin", 2)]),
                ],
            ),
        };
        let captured = capture_identity(&before, &[0, 0])
            .ok_or_else(|| anyhow::anyhow!("the path should exist"))?;
        assert_eq!(captured.len(), 2);

        // The rescan has a *different* directory sitting where `alpha`
        // used to be; `alpha` itself is gone.
        let after = Tree {
            root_path: PathBuf::from("root"),
            volume_free: None,
            volume_total: None,
            root: dir(
                "root",
                vec![
                    dir("replacement", vec![file("z.bin", 3)]),
                    dir("beta", vec![file("y.bin", 2)]),
                ],
            ),
        };
        // Zoom/expansion placement resolves to the nearest surviving
        // ancestor.
        let restored = resolve_identity(&after, &captured);
        assert_eq!(
            restored,
            vec![],
            "the deepest still-existing component is the root — restoring \
             to index 0 would have selected 'replacement'"
        );
        assert_ne!(restored, vec![0, 0]);

        // A selection, though, is exact: `alpha/inner` is gone, so there
        // is nothing to select. Falling back to the parent would make the
        // selection silently point at a different thing.
        assert_eq!(
            resolve_identity_exact(&after, &captured),
            None,
            "a vanished item must not resolve to its parent"
        );
        // A still-present identity resolves exactly, of course.
        let still_there = capture_identity(&before, &[1])
            .ok_or_else(|| anyhow::anyhow!("the path should exist"))?;
        assert_eq!(
            resolve_identity_exact(&after, &still_there).as_deref(),
            Some(&[1][..]),
            "beta is in the same place in both trees"
        );
        Ok(())
    }
}
