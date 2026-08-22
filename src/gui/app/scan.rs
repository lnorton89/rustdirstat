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
    /// One finished top-level child, published while the rest of the
    /// scan is still running. The window attaches it immediately, which
    /// is what makes the tree fill in rather than appear at the end.
    Child(Box<Node>),
    Done(Box<ScanOutcome>),
    Cancelled,
    Failed(String),
}

/// The two whole-tree summaries, accumulated child by child.
///
/// They used to be computed from the finished tree on the scan thread,
/// which a streaming scan cannot do: it no longer *has* the tree, having
/// handed every child to the window as it went. So each child is summed
/// on its way out — the same total work, still off the UI thread, still
/// arriving complete with `Done`.
struct Summaries {
    extensions: HashMap<String, ExtensionRow>,
    largest: Vec<top_files::TopFile>,
    physical: bool,
    /// How many children have been published, which is also the index the
    /// next one will have once the window attaches it — index paths from
    /// `top_k` are relative to the child they were found in, and have to
    /// be made relative to the root.
    published: usize,
}

impl Summaries {
    fn new(physical: bool) -> Self {
        Self {
            extensions: HashMap::new(),
            largest: Vec::new(),
            physical,
            published: 0,
        }
    }

    /// Folds one about-to-be-published child in, and returns the index it
    /// will occupy.
    fn add(&mut self, child: &Node) -> usize {
        let index = self.published;
        self.published += 1;

        for row in collect_extension_rows(child, self.physical) {
            let slot = self
                .extensions
                .entry(row.extension.clone())
                .or_insert_with(|| ExtensionRow {
                    extension: row.extension.clone(),
                    category: row.category,
                    size: 0,
                    count: 0,
                });
            slot.size = slot.size.saturating_add(row.size);
            slot.count = slot.count.saturating_add(row.count);
        }

        // A child's own top-k merged into the running one: the largest
        // files overall are among the largest of each part, so keeping
        // `TOP_FILES` per merge is exact rather than an approximation.
        for mut file in top_files::top_k(child, TOP_FILES) {
            let mut absolute = Vec::with_capacity(file.index_path.len() + 1);
            absolute.push(index);
            absolute.extend_from_slice(&file.index_path);
            file.index_path = absolute;
            self.largest.push(file);
        }
        self.largest
            .sort_by_key(|file| std::cmp::Reverse(file.size));
        self.largest.truncate(TOP_FILES);
        index
    }

    fn finish(self) -> (Vec<ExtensionRow>, Vec<top_files::TopFile>) {
        (self.extensions.into_values().collect(), self.largest)
    }
}

/// How many of the largest files the Largest Files view holds.
const TOP_FILES: usize = 200;

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

/// How many published children one frame will attach.
///
/// Attaching is O(1), but a scan of a directory with a thousand
/// top-level entries can publish faster than the window draws, and a
/// frame that attaches all of them is a frame that missed its budget.
/// The rest wait for the next one, a sixtieth of a second later.
const MAX_CHILDREN_PER_FRAME: usize = 64;

/// How often a hidden window wakes to collect background work.
///
/// Slow on purpose: there is nothing to draw, so this only has to keep a
/// finished scan from sitting undelivered until the window is restored.
const HIDDEN_POLL: std::time::Duration = std::time::Duration::from_millis(100);

impl GuiApp {
    pub(in crate::gui) fn loading(roots: Vec<PathBuf>) -> Self {
        // The placeholder tree is what the window draws until the first
        // scan lands, so it is labelled with whichever root the title bar
        // would name — the single one, or the multi-root label.
        let label = match roots.as_slice() {
            [single] => single.clone(),
            _ => PathBuf::from(crate::scanner::MULTI_ROOT_LABEL),
        };
        let mut app = Self::new(Tree::placeholder(label));
        app.start_scan(roots, true);
        app
    }

    pub(in crate::gui) fn refresh_scan(&mut self) -> anyhow::Result<()> {
        // A rescan repeats *what was scanned*, which for a multi-root
        // tree is the list of roots rather than the label standing in for
        // them. `root_path` is not a path at all in that case.
        self.start_scan(self.scanned_roots(), false);
        Ok(())
    }

    /// The paths the current tree was built from.
    pub(in crate::gui) fn scanned_roots(&self) -> Vec<PathBuf> {
        if self.tree.roots.is_empty() {
            return vec![self.tree.root_path.clone()];
        }
        self.tree
            .roots
            .iter()
            .map(|root| root.path.clone())
            .collect()
    }

    pub(in crate::gui) fn open_folder(&mut self, root: &Path) -> anyhow::Result<()> {
        self.start_scan(vec![root.to_path_buf()], true);
        Ok(())
    }

    /// Scans several places at once, as one tree.
    ///
    /// An empty list is a no-op with a status line rather than an error:
    /// it means the picker was confirmed with nothing ticked, which is a
    /// slip, not a failure.
    pub(in crate::gui) fn open_locations(&mut self, roots: Vec<PathBuf>) {
        if roots.is_empty() {
            self.status = Some("Pick at least one location to scan".to_string());
            return;
        }
        self.start_scan(roots, true);
    }

    fn start_scan(&mut self, roots: Vec<PathBuf>, reset_workspace: bool) {
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
        // Remembered now, because the first published child is what
        // swaps the previous tree out and by then the request is gone.
        self.pending_root = match roots.as_slice() {
            [single] => single.clone(),
            _ => PathBuf::from(crate::scanner::MULTI_ROOT_LABEL),
        };
        self.pending_children.clear();
        self.pending_finish = None;
        self.live_scan = false;
        let progress = Arc::new(crate::scanner::Progress::default());
        let worker_progress = Arc::clone(&progress);
        let display = match roots.as_slice() {
            [single] => crate::util::display_path(single),
            many => format!("{} locations", many.len()),
        };
        let (tx, rx) = mpsc::channel();
        let physical = self.use_physical;
        std::thread::spawn(move || {
            // The sink runs on whichever worker thread finished the
            // child. It sums the child into the summaries and hands it
            // straight to the window, so nothing tree-sized is left for
            // the UI thread to do when the scan lands — which was the
            // whole reason those summaries were computed here in the
            // first place.
            let summaries = std::sync::Mutex::new(Summaries::new(physical));
            let publish = |node: Node| {
                if let Ok(mut summaries) = summaries.lock() {
                    summaries.add(&node);
                }
                let _ = tx.send(ScanMessage::Child(Box::new(node)));
            };
            let scanned = crate::scanner::scan_many_streaming(
                &roots,
                Some(worker_progress.as_ref()),
                crate::scanner::ScanOptions::default(),
                &publish,
            );
            let message = match scanned {
                Ok(crate::scanner::Scan::Completed(tree)) => {
                    let (extensions, largest_files) = match summaries.into_inner() {
                        Ok(summaries) => summaries.finish(),
                        Err(_) => (Vec::new(), Vec::new()),
                    };
                    ScanMessage::Done(Box::new(ScanOutcome {
                        tree: *tree,
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
        // As `start_scan` would have left it: the live tree needs
        // somewhere to be rooted before the first child lands.
        self.pending_root = self.tree.root_path.clone();
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

    /// Adopts the finished shell onto the tree the window has been
    /// filling in.
    ///
    /// Deferred if a background worker still holds a clone of the tree —
    /// the same reason `drain_pending_children` defers — and retried on
    /// the next poll. Until then the window shows totals accumulated from
    /// the children themselves, which differ from the shell's only by the
    /// root's own unreadable count.
    fn finish_live_tree(&mut self, shell: Box<Tree>) {
        self.drain_pending_children();
        let Some(tree) = Arc::get_mut(&mut self.tree) else {
            self.pending_finish = Some(shell);
            return;
        };
        let shell = *shell;
        let children = std::mem::take(&mut tree.root.children);
        let mut root = shell.root;
        root.children = children;
        tree.root = root;
        tree.root_path = shell.root_path;
        tree.volume_free = shell.volume_free;
        tree.volume_total = shell.volume_total;
        tree.roots = shell.roots;
        self.tree_generation = self.tree_generation.wrapping_add(1);
        self.live_scan = false;
    }

    /// Attaches one published child to the tree being built.
    ///
    /// Mutation in place rather than a rebuild: a tree is the one thing
    /// in this app that must never be copied per frame, and pushing a
    /// child plus folding its totals in is O(1) whatever the tree already
    /// holds. `Arc::get_mut` is what makes it safe — it succeeds only
    /// while the window is the sole owner, and any frame where a worker
    /// still holds a clone simply defers the child to the next one.
    ///
    /// The caches key off the `Arc`'s address, which does *not* change
    /// under an in-place mutation, so `tree_generation` is what tells
    /// them the rows and tiles they hold are stale. A field that affects
    /// rows or tiles has to be in `RowKey`/`TreemapKey`; this is one.
    fn attach_child(&mut self, child: Node) {
        if !self.live_scan {
            // The first child of a scan replaces whatever was on screen.
            // Until it arrives the previous tree stays browsable, which
            // is why the swap happens here rather than when the scan
            // starts: an empty window between the two would be a worse
            // trade than a moment of the old one.
            self.replace_tree(Tree::live_shell(self.pending_root.clone()));
            self.reset_workspace();
            self.live_scan = true;
        }
        self.pending_children.push(child);
        self.drain_pending_children();
    }

    /// Moves whatever has been published into the live tree, if the
    /// window currently owns it exclusively.
    fn drain_pending_children(&mut self) {
        if self.pending_children.is_empty() {
            return;
        }
        let Some(tree) = Arc::get_mut(&mut self.tree) else {
            // A background worker is still holding a clone. Nothing is
            // lost: the children wait here and land on a later frame.
            return;
        };
        let mut totals = crate::scanner::Totals::from_node(&tree.root);
        for child in self.pending_children.drain(..) {
            totals.add(&child);
            tree.root.children.push(child);
        }
        totals.write_into(&mut tree.root);
        self.tree_generation = self.tree_generation.wrapping_add(1);
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
        // Anything a previous frame could not attach, because a worker
        // held a clone of the tree at the time.
        self.drain_pending_children();
        if let Some(shell) = self.pending_finish.take() {
            self.finish_live_tree(shell);
        }
        // Children first: a scan publishes them as it goes, and every one
        // of them is a folder appearing in the window while the rest of
        // the drive is still being walked. Bounded per frame so a scan
        // that finds a thousand top-level entries cannot spend a frame
        // attaching them — `attach_child` is O(1), but a thousand of
        // anything inside one frame is a stutter.
        let mut scan_result = None;
        for _ in 0..MAX_CHILDREN_PER_FRAME {
            let next = self.scan_rx.as_ref().and_then(|rx| match rx.try_recv() {
                Ok(message) => Some(message),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(ScanMessage::Failed(
                    "The scan worker stopped unexpectedly".to_string(),
                )),
            });
            match next {
                Some(ScanMessage::Child(child)) => self.attach_child(*child),
                Some(other) => {
                    scan_result = Some(other);
                    break;
                }
                None => break,
            }
        }
        if let Some(result) = scan_result {
            self.scan_rx = None;
            self.scan_progress = None;
            match result {
                // A `Child` is handled in the loop above; reaching here
                // with one would mean the loop stopped draining, so it is
                // put back rather than dropped on the floor.
                ScanMessage::Child(child) => self.attach_child(*child),
                ScanMessage::Done(outcome) => {
                    let reset = self.scan_resets_workspace;
                    let ScanOutcome {
                        tree,
                        extensions,
                        largest_files,
                    } = *outcome;
                    // The children are already on screen; what arrives
                    // here is the shell that owns their totals, the
                    // volume figures and the root list. Adopting it in
                    // place keeps every index path the window has been
                    // handing out valid, which moving the children into
                    // a second tree would not.
                    if self.live_scan {
                        self.finish_live_tree(Box::new(tree));
                    } else {
                        self.replace_tree(tree);
                    }
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
            // Every frame the display will take, not a timer. The app
            // targets 120 FPS *during* a scan (`theme::FRAME_BUDGET`),
            // and a repaint request on a 33 ms timer caps it at 30 no
            // matter how much headroom the machine has — the counters in
            // the banner tick in visible steps and a splitter drag moves
            // in stages. eframe paces the actual presentation against
            // vsync, so asking for the next frame immediately means "as
            // fast as this display runs", not a spin.
            //
            // Unless nothing is being presented. A minimized window has
            // no vsync to pace it, and an unpaced request-repaint loop
            // burns the one core the scan pool deliberately left free —
            // slowing the very scan that is being waited on, to animate
            // counters nobody can see. Hidden, it falls back to a slow
            // heartbeat that only has to keep the worker polled.
            let hidden = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
            if hidden {
                ctx.request_repaint_after(HIDDEN_POLL);
            } else {
                ctx.request_repaint();
            }
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
            roots: Vec::new(),
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
            roots: Vec::new(),
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
            roots: Vec::new(),
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
            roots: Vec::new(),
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
