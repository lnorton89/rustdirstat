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

pub(in crate::gui) fn valid_prefix(tree: &Tree, requested: &[usize]) -> Vec<usize> {
    let mut valid = Vec::new();
    let mut node = &tree.root;
    for &idx in requested {
        let Some(child) = node.children.get(idx) else {
            break;
        };
        valid.push(idx);
        node = child;
    }
    valid
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
        let progress = Arc::new(crate::scanner::Progress::default());
        let worker_progress = Arc::clone(&progress);
        let display = crate::util::display_path(&root);
        let (tx, rx) = mpsc::channel();
        let physical = self.use_physical;
        std::thread::spawn(move || {
            let result = crate::scanner::scan(&root, Some(worker_progress.as_ref()))
                .map_err(|error| error.to_string())
                .map(|tree| {
                    let extensions = collect_extension_rows(&tree.root, physical);
                    let largest_files = top_files::top_k(&tree.root, 200);
                    ScanOutcome {
                        tree,
                        extensions,
                        largest_files,
                    }
                });
            let _ = tx.send(result);
        });
        self.scan_progress = Some(progress);
        self.scan_rx = Some(rx);
        self.scan_resets_workspace = reset_workspace;
        self.status = Some(format!("Scanning {display}…"));
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
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("The scan worker stopped unexpectedly".to_string()))
            }
        });
        if let Some(result) = scan_result {
            self.scan_rx = None;
            self.scan_progress = None;
            match result {
                Ok(outcome) => {
                    let reset = self.scan_resets_workspace;
                    let ScanOutcome {
                        tree,
                        extensions,
                        largest_files,
                    } = outcome;
                    self.replace_tree(tree);
                    if reset {
                        self.reset_workspace();
                    } else {
                        self.zoom_path = valid_prefix(self.tree.as_ref(), &self.zoom_path);
                        self.selected_path = self
                            .selected_path
                            .as_ref()
                            .map(|path| valid_prefix(self.tree.as_ref(), path));
                    }
                    // Already computed on the scan thread; see
                    // `ScanOutcome`. Zooming still recomputes the
                    // extension rows, but that is a subtree and a
                    // deliberate act, not something that lands on the
                    // user unbidden.
                    self.extensions = extensions;
                    self.sort_extensions();
                    self.largest_files = largest_files;
                    self.search.results.clear();
                    self.duplicate_groups.clear();
                    self.status = Some("Scan complete".to_string());
                }
                Err(error) => self.status = Some(format!("Scan failed: {error}")),
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
                    // Says so when the search was cut short. "12
                    // duplicate group(s)" reads as the whole answer.
                    self.status = Some(if scan.skipped > 0 {
                        format!(
                            "{groups} duplicate group(s) — {} file(s) not checked,                              the candidate limit was reached",
                            scan.skipped
                        )
                    } else {
                        format!("{groups} duplicate group(s)")
                    });
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
