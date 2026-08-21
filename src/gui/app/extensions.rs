// ============================================================================
// Module:       gui::app::extensions
// Description:  The extension breakdown, the flat file views, and the column
//               order shared by both tables.
//
// Dependencies: crate::{color, stats}; super::GuiApp
// ============================================================================

//! The extension breakdown, the flat file views, and the column order
//! shared by both tables.

use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::gui) enum DirectoryColumn {
    Name,
    Size,
    SubtreePercentage,
    PercentTotal,
    Files,
    Subdirs,
    LastChange,
    Attributes,
}

impl DirectoryColumn {
    pub(in crate::gui) const DEFAULT_ORDER: [Self; 8] = [
        Self::Name,
        Self::Size,
        Self::SubtreePercentage,
        Self::PercentTotal,
        Self::Files,
        Self::Subdirs,
        Self::LastChange,
        Self::Attributes,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::gui) enum ExtensionColumn {
    Extension,
    Color,
    Description,
    Bytes,
    PercentBytes,
    Files,
}

impl ExtensionColumn {
    pub(in crate::gui) const DEFAULT_ORDER: [Self; 6] = [
        Self::Extension,
        Self::Color,
        Self::Description,
        Self::Bytes,
        Self::PercentBytes,
        Self::Files,
    ];
}

#[derive(Clone)]
pub(in crate::gui) struct ExtensionRow {
    pub extension: String,
    pub category: Category,
    pub size: u64,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui) enum ExtensionSortMode {
    ExtensionAsc,
    ExtensionDesc,
    ColorAsc,
    ColorDesc,
    DescriptionAsc,
    DescriptionDesc,
    BytesDesc,
    BytesAsc,
    PercentDesc,
    PercentAsc,
    FilesDesc,
    FilesAsc,
}

/// Sorting the extension table by its Color column orders rows by the
/// hue they are actually painted at, so it has to be the same hue.
pub(in crate::gui) fn extension_color_sort_key(extension: &str) -> u32 {
    crate::color::extension_hue(extension) as u32
}

pub(in crate::gui) fn reorder_column<T: Copy + Eq>(columns: &mut Vec<T>, source: T, target: T) {
    if source == target {
        return;
    }
    let Some(source_index) = columns.iter().position(|column| *column == source) else {
        return;
    };
    columns.remove(source_index);
    let Some(target_index) = columns.iter().position(|column| *column == target) else {
        columns.insert(source_index.min(columns.len()), source);
        return;
    };
    columns.insert(target_index, source);
}

pub(in crate::gui) fn extension_label(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| format!(".{}", s.to_ascii_lowercase()))
        .unwrap_or_else(|| NO_EXTENSION_LABEL.to_string())
}

/// One row per distinct extension anywhere under `node`.
///
/// Free of `GuiApp` so the scan thread can call it before the tree is
/// ever handed over — see [`ScanOutcome`].
pub(in crate::gui) fn collect_extension_rows(node: &Node, physical: bool) -> Vec<ExtensionRow> {
    // Never cancelled, so the walk always completes and this is `Some`;
    // the fallback keeps the function total rather than being reachable.
    collect_extension_rows_unless(node, physical, None).unwrap_or_default()
}

/// [`collect_extension_rows`] for the zoom worker: `None` — no partial
/// answer — if `cancel` was raised mid-walk.
pub(in crate::gui) fn collect_extension_rows_unless(
    node: &Node,
    physical: bool,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<Vec<ExtensionRow>> {
    let mut by_ext: HashMap<String, (Category, u64, u64)> = HashMap::new();
    if !collect_extensions(node, physical, cancel, &mut by_ext) {
        return None;
    }
    Some(
        by_ext
            .into_iter()
            .map(|(extension, (category, size, count))| ExtensionRow {
                extension,
                category,
                size,
                count,
            })
            .collect(),
    )
}
/// Aggregates every file in `node`'s subtree into the extension table.
///
/// Iterative for the reason every walk here is: this runs when the zoom
/// changes, and a zoomed subtree is as deep as the user's filesystem, so
/// a recursion would put a user-chosen depth on the call stack.
///
/// Returns `false` if `cancel` was raised mid-walk — the walk quits on
/// the spot and `out` is partial, so a cancelled aggregation must be
/// discarded, not displayed. Cancellation exists because each zoom
/// spawns a worker walking the whole zoomed subtree; without it, zooming
/// rapidly through a drive-sized tree stacks a full multi-million-node
/// walk per superseded zoom, all running to completion for answers
/// nobody will see.
pub(in crate::gui) fn collect_extensions(
    node: &Node,
    physical: bool,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    out: &mut HashMap<String, (Category, u64, u64)>,
) -> bool {
    let mut pending = vec![node];
    while let Some(current) = pending.pop() {
        if let Some(flag) = cancel {
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                return false;
            }
        }
        for child in &current.children {
            if child.is_dir {
                pending.push(child);
            } else {
                // Display-only territory: the legend's labels. The raw
                // bytes stay on the node; `category_for_name` takes them
                // as-is.
                let extension = extension_label(&child.name.to_string_lossy());
                let category = child
                    .category
                    .unwrap_or_else(|| category_for_name(&child.name));
                let entry = out.entry(extension).or_insert((category, 0, 0));
                entry.1 = entry.1.saturating_add(child.effective_size(physical));
                entry.2 = entry.2.saturating_add(1);
            }
        }
    }
    true
}

pub(in crate::gui) fn size_label(bytes: u64, physical: bool) -> String {
    format!(
        "{}{}",
        human_bytes(bytes),
        if physical { " (physical)" } else { "" }
    )
}

impl GuiApp {
    pub(in crate::gui) fn refresh_extensions(&mut self) {
        // Off the frame thread: this walks the whole zoom subtree, which
        // on a drive-sized scan is millions of nodes. The previous rows
        // stay on screen until the new ones land — percentages are
        // computed against the displayed rows, so a stale set is still
        // self-consistent — rather than freezing the window for the
        // walk. The worker holds its own `Arc` to the tree, so it is
        // safe alongside a rescan; `poll_background` drops the receiver
        // when the tree is replaced, so a stale result cannot clobber
        // the fresh scan-time rows.
        //
        // A superseded worker is told to stop, not just ignored: its
        // receiver is replaced below, so its answer could never land,
        // and letting it walk millions of nodes to completion anyway is
        // a full scan's worth of CPU per rapid zoom click.
        self.cancel_extension_worker();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.extensions_cancel = Some(Arc::clone(&cancel));
        let tree = Arc::clone(&self.tree);
        let zoom_path = self.zoom_path.clone();
        let physical = self.use_physical;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Forgiving: a slightly stale zoom path (the tree changed
            // under us) still has a nearest node to describe.
            let node = tree.deepest_valid_node(&zoom_path);
            if let Some(rows) = collect_extension_rows_unless(node, physical, Some(&cancel)) {
                let _ = tx.send(rows);
            }
        });
        self.extensions_rx = Some(rx);
    }

    /// Stops any zoom-time extension worker still walking and forgets
    /// its receiver, so a superseded or obsolete answer neither arrives
    /// nor keeps burning a core.
    pub(in crate::gui) fn cancel_extension_worker(&mut self) {
        if let Some(flag) = self.extensions_cancel.take() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.extensions_rx = None;
    }

    /// Whether a zoom-time extension recomputation is still running —
    /// used by tests to wait for the worker.
    #[cfg(test)]
    pub(in crate::gui) fn extensions_pending(&self) -> bool {
        self.extensions_rx.is_some()
    }

    pub(in crate::gui) fn sort_extensions(&mut self) {
        let by_extension = |a: &ExtensionRow, b: &ExtensionRow| {
            a.extension.to_lowercase().cmp(&b.extension.to_lowercase())
        };
        match self.extension_sort {
            ExtensionSortMode::ExtensionAsc => self.extensions.sort_by(by_extension),
            ExtensionSortMode::ExtensionDesc => self.extensions.sort_by(|a, b| by_extension(b, a)),
            ExtensionSortMode::ColorAsc => self.extensions.sort_by(|a, b| {
                extension_color_sort_key(&a.extension)
                    .cmp(&extension_color_sort_key(&b.extension))
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::ColorDesc => self.extensions.sort_by(|a, b| {
                extension_color_sort_key(&b.extension)
                    .cmp(&extension_color_sort_key(&a.extension))
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::DescriptionAsc => self.extensions.sort_by(|a, b| {
                a.category
                    .label()
                    .cmp(b.category.label())
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::DescriptionDesc => self.extensions.sort_by(|a, b| {
                b.category
                    .label()
                    .cmp(a.category.label())
                    .then_with(|| by_extension(a, b))
            }),
            ExtensionSortMode::BytesDesc => self
                .extensions
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::BytesAsc => self
                .extensions
                .sort_by(|a, b| a.size.cmp(&b.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::PercentDesc => self
                .extensions
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::PercentAsc => self
                .extensions
                .sort_by(|a, b| a.size.cmp(&b.size).then_with(|| by_extension(a, b))),
            ExtensionSortMode::FilesDesc => self
                .extensions
                .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| by_extension(a, b))),
            ExtensionSortMode::FilesAsc => self
                .extensions
                .sort_by(|a, b| a.count.cmp(&b.count).then_with(|| by_extension(a, b))),
        }
    }

    pub(in crate::gui) fn reorder_directory_column(
        &mut self,
        source: DirectoryColumn,
        target: DirectoryColumn,
    ) {
        reorder_column(&mut self.directory_column_order, source, target);
    }

    pub(in crate::gui) fn reorder_extension_column(
        &mut self,
        source: ExtensionColumn,
        target: ExtensionColumn,
    ) {
        reorder_column(&mut self.extension_column_order, source, target);
    }

    pub(in crate::gui) fn refresh_largest_files(&mut self) {
        self.largest_files = top_files::top_k(&self.tree.root, 200);
    }

    /// Whether a search is still running — used by tests to wait for the
    /// worker instead of reading results before they exist.
    #[cfg(test)]
    pub(in crate::gui) fn search_running(&self) -> bool {
        self.search_rx.is_some()
    }

    pub(in crate::gui) fn run_search(&mut self) {
        let query = self.search.query.clone();
        if query.is_empty() {
            self.search.results.clear();
            self.search.error = None;
            self.search_rx = None;
            self.file_view = FileView::SearchResults;
            // Not left at whatever it was: an in-flight search set
            // "Searching…", and with its receiver dropped nothing else
            // would ever overwrite that.
            self.status = None;
            return;
        }
        // Off the frame thread, like the scan and duplicate workers: a
        // search walks every node in the tree, and on a whole-drive scan
        // that is ~10M regex matches — a synchronous call froze the
        // window for as long as the search took. The worker holds its own
        // `Arc` to the tree, so it is safe to run alongside a rescan; if
        // a new tree lands first its result is dropped with the receiver.
        let tree = Arc::clone(&self.tree);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(search::search(&tree.root, &query));
        });
        self.search_rx = Some(rx);
        self.search.results.clear();
        self.search.error = None;
        self.file_view = FileView::SearchResults;
        self.status = Some("Searching…".to_string());
    }
}
