use crate::color::Category;
use crate::duplicates::DupGroup;
use crate::model::{category_for_name, Node, Tree};
use crate::tui::{search, top_files, SortMode};
use crate::util::human_bytes;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::treemap_layout;
use super::ui::{ModalPage, Palette};

/// The blurred snapshot painted behind an open modal.
///
/// Capturing it is a round trip through the renderer — a
/// [`egui::ViewportCommand::Screenshot`] this frame, an
/// [`egui::Event::Screenshot`] one or two frames later — so the modal
/// deliberately does not draw until the snapshot is in hand. Drawing it
/// first would put the modal *in* its own backdrop.
///
/// `Unavailable` is the honest state for a backend that never answers:
/// the modal still opens, over a plain scrim, which is what happens in
/// every test since there is no renderer behind `egui::Context::default`.
pub(super) enum Backdrop {
    Idle,
    /// Waiting on the reply. The counter is a deadline, not a retry — if
    /// no screenshot arrives within a couple of frames the modal must
    /// stop waiting and open regardless.
    Requested {
        frames_waited: u8,
    },
    Ready(egui::TextureHandle),
    Unavailable,
}

impl Backdrop {
    pub(super) fn texture(&self) -> Option<&egui::TextureHandle> {
        match self {
            Backdrop::Ready(texture) => Some(texture),
            Backdrop::Idle | Backdrop::Requested { .. } | Backdrop::Unavailable => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PaneOrientation {
    Horizontal,
    Vertical,
}

impl PaneOrientation {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Treemap below",
            Self::Vertical => "Treemap right",
        }
    }
    pub(super) fn toggle(&mut self) {
        *self = match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        };
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FileView {
    AllFiles,
    LargestFiles,
    DuplicateFiles,
    SearchResults,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum DirectoryColumn {
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
    pub(super) const DEFAULT_ORDER: [Self; 8] = [
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
pub(super) enum ExtensionColumn {
    Extension,
    Color,
    Description,
    Bytes,
    PercentBytes,
    Files,
}

impl ExtensionColumn {
    pub(super) const DEFAULT_ORDER: [Self; 6] = [
        Self::Extension,
        Self::Color,
        Self::Description,
        Self::Bytes,
        Self::PercentBytes,
        Self::Files,
    ];
}

impl FileView {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::AllFiles => "All Files",
            Self::LargestFiles => "Largest Files",
            Self::DuplicateFiles => "Duplicate Files",
            Self::SearchResults => "Search Results",
        }
    }
}

#[derive(Clone)]
pub(super) struct ExtensionRow {
    pub extension: String,
    pub category: Category,
    pub size: u64,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExtensionSortMode {
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
fn extension_color_sort_key(extension: &str) -> u32 {
    crate::color::extension_hue(extension) as u32
}

fn reorder_column<T: Copy + Eq>(columns: &mut Vec<T>, source: T, target: T) {
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

/// One row of the directory table, flattened out of the tree.
///
/// Lives here rather than with the table-painting code because it is
/// derived model state that gets cached across frames — see
/// [`GuiApp::refresh_visible_rows`].
#[derive(Clone)]
pub(super) struct TreeRow {
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
struct RowKey {
    tree: usize,
    sort: SortMode,
    physical: bool,
    expanded: u64,
}

/// Everything the treemap tile list depends on.
#[derive(PartialEq)]
struct TreemapKey {
    tree: usize,
    zoom_path: Vec<usize>,
    rect: [i32; 4],
    physical: bool,
    free_space: bool,
    /// Included because the strip comes from the font the renderer will
    /// draw with, and a font size change has to re-lay-out the tiles.
    label_strip: i32,
    /// Included so the reduced-detail layout used during a drag is
    /// replaced by the full one the moment the drag ends, rather than
    /// being mistaken for an up-to-date cache entry.
    max_tiles: usize,
}

/// Order-independent fingerprint of the expanded-directory set.
///
/// A `HashSet` has no stable iteration order, so the paths are folded
/// together commutatively. XOR alone would cancel a pair of equal
/// hashes, so a wrapping sum and the element count are mixed in too.
fn expanded_fingerprint(expanded: &HashSet<Vec<usize>>) -> u64 {
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

/// One finished maintenance tool, kept for the Maintenance page.
///
/// The status bar shows a single line that the next scan overwrites,
/// which is fine for "launched cleanmgr" and useless for a DISM report
/// the user ran specifically to read.
pub(super) struct ToolOutcome {
    pub tool: String,
    pub summary: String,
    pub detail: String,
    pub failed: bool,
}

pub(super) struct PendingDelete {
    pub index_path: Vec<usize>,
    pub name: String,
    pub is_dir: bool,
    pub permanent: bool,
}

/// Which parts of the window are showing, and how they are arranged.
///
/// These are the toggles the View menu offers and the ones saved to the
/// config between runs, so they travel together everywhere: the menu
/// writes them, the layout reads them, `to_config` persists them.
pub(super) struct ViewOptions {
    pub toolbar: bool,
    pub status_bar: bool,
    pub extension_pane: bool,
    pub treemap: bool,
    pub free_space: bool,
    pub grid: bool,
    pub labels: bool,
    pub orientation: PaneOrientation,
}

impl ViewOptions {
    /// Reads the toggles back out of a saved config, defaulting anything
    /// absent to shown.
    ///
    /// A pair with [`Self::to_config`], and separate from `GuiApp::new`
    /// so the two can be tested against each other. `new` loads the real
    /// config from disk, so nothing that goes through it can be checked
    /// without touching the user's own settings.
    fn from_config(config: &crate::config::Config) -> Self {
        Self {
            toolbar: config.gui_show_toolbar.unwrap_or(true),
            status_bar: config.gui_show_status_bar.unwrap_or(true),
            extension_pane: config.gui_show_extensions.unwrap_or(true),
            treemap: config.show_treemap.unwrap_or(true),
            free_space: config.gui_show_free_space.unwrap_or(true),
            grid: config.gui_show_grid.unwrap_or(true),
            labels: config.gui_show_labels.unwrap_or(true),
            orientation: match config.gui_orientation.as_deref() {
                Some("vertical") => PaneOrientation::Vertical,
                _ => PaneOrientation::Horizontal,
            },
        }
    }

    /// The inverse of [`Self::from_config`], as a partial config to be
    /// spread over the rest of the saved settings.
    fn to_config(&self) -> crate::config::Config {
        crate::config::Config {
            show_treemap: Some(self.treemap),
            gui_orientation: Some(match self.orientation {
                PaneOrientation::Horizontal => "horizontal".to_string(),
                PaneOrientation::Vertical => "vertical".to_string(),
            }),
            gui_show_extensions: Some(self.extension_pane),
            gui_show_toolbar: Some(self.toolbar),
            gui_show_status_bar: Some(self.status_bar),
            gui_show_free_space: Some(self.free_space),
            gui_show_grid: Some(self.grid),
            gui_show_labels: Some(self.labels),
            ..crate::config::Config::default()
        }
    }
}

/// The search box and its last results.
#[derive(Default)]
pub(super) struct SearchState {
    pub query: String,
    pub results: Vec<search::SearchHit>,
    pub error: Option<String>,
}

/// The Windows maintenance tools: what is queued, what is running, and
/// what has already run.
#[derive(Default)]
pub(super) struct ToolsState {
    /// Set while a tool waits on its confirmation.
    pub pending: Option<usize>,
    pub rx: Option<mpsc::Receiver<Result<crate::wintools::ToolOutput, String>>>,
    pub active_name: Option<String>,
    /// Index of the tool currently running, so its own row can show the
    /// spinner instead of the page closing out from under it.
    pub running: Option<usize>,
    pub log: Vec<ToolOutcome>,
}

pub(super) struct GuiApp {
    pub tree: Arc<Tree>,
    pub zoom_path: Vec<usize>,
    pub selected_path: Option<Vec<usize>>,
    pub expanded: HashSet<Vec<usize>>,
    pub sort: SortMode,
    pub directory_column_order: Vec<DirectoryColumn>,
    pub use_physical: bool,
    pub extensions: Vec<ExtensionRow>,
    pub extension_sort: ExtensionSortMode,
    pub extension_column_order: Vec<ExtensionColumn>,
    pub highlighted_extension: Option<String>,
    pub highlighted_category: Option<Category>,
    pub pending_delete: Option<PendingDelete>,
    pub status: Option<String>,
    /// The one modal page on screen, if any. Six independent `show_*`
    /// flags used to live here; they could all be true at once, and two
    /// of them opened the same window.
    pub modal: Option<ModalPage>,
    pub backdrop: Backdrop,
    pub theme_id: String,
    /// Resolved from `theme_id`. Cached rather than looked up per frame
    /// so a rename of the field cannot quietly put a table lookup on a
    /// draw path.
    pub palette: Palette,
    pub view: ViewOptions,
    pub file_view: FileView,
    pub largest_files: Vec<top_files::TopFile>,
    pub duplicate_groups: Vec<DupGroup>,
    pub search: SearchState,
    pub scan_progress: Option<Arc<crate::scanner::Progress>>,
    /// Directory rows currently on screen, rebuilt only when something
    /// they depend on changes rather than on every frame. On a
    /// whole-drive scan with a wide directory expanded, rebuilding this
    /// per frame means hundreds of thousands of string and path
    /// allocations per frame, which is enough on its own to stop the
    /// window responding to input.
    pub visible_rows: Vec<TreeRow>,
    visible_rows_key: Option<RowKey>,
    /// Treemap tiles for the current panel rect, cached on the same
    /// terms and for the same reason.
    /// System file-type icons, resolved lazily per extension and kept
    /// for the life of the process. See `gui::shell_icons`.
    pub shell_icons: super::shell_icons::ShellIcons,
    pub treemap_tiles: Vec<treemap_layout::Tile>,
    treemap_key: Option<TreemapKey>,
    scan_rx: Option<mpsc::Receiver<Result<ScanOutcome, String>>>,
    scan_resets_workspace: bool,
    duplicate_rx: Option<mpsc::Receiver<crate::duplicates::DupScan>>,
    pub tools: ToolsState,
}

impl GuiApp {
    pub(super) fn loading(root: PathBuf) -> Self {
        let mut app = Self::new(Tree::placeholder(root.clone()));
        app.start_scan(root, true);
        app
    }

    pub(super) fn new(tree: Tree) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(Vec::new());
        let config = crate::config::load();
        let theme_id = config
            .gui_theme
            .clone()
            .unwrap_or_else(|| super::ui::default_theme_id().to_string());
        let mut app = Self {
            tree: Arc::new(tree),
            zoom_path: Vec::new(),
            selected_path: None,
            expanded,
            sort: config.sort.unwrap_or(SortMode::SizeDesc),
            directory_column_order: DirectoryColumn::DEFAULT_ORDER.to_vec(),
            use_physical: config.use_physical.unwrap_or(false),
            extensions: Vec::new(),
            extension_sort: ExtensionSortMode::BytesDesc,
            extension_column_order: ExtensionColumn::DEFAULT_ORDER.to_vec(),
            highlighted_extension: None,
            highlighted_category: None,
            pending_delete: None,
            status: None,
            modal: None,
            backdrop: Backdrop::Idle,
            theme_id: theme_id.clone(),
            palette: super::ui::palette_for(&theme_id),
            view: ViewOptions::from_config(&config),
            file_view: FileView::AllFiles,
            largest_files: Vec::new(),
            duplicate_groups: Vec::new(),
            search: SearchState::default(),
            scan_progress: None,
            visible_rows: Vec::new(),
            visible_rows_key: None,
            shell_icons: super::shell_icons::ShellIcons::default(),
            treemap_tiles: Vec::new(),
            treemap_key: None,
            scan_rx: None,
            scan_resets_workspace: false,
            duplicate_rx: None,
            tools: ToolsState::default(),
        };
        app.refresh_extensions();
        app.refresh_largest_files();
        app
    }

    pub(super) fn zoom_node(&self) -> &Node {
        self.tree.node_for(&self.zoom_path)
    }
    pub(super) fn zoom_fs_path(&self) -> PathBuf {
        self.tree.path_for(&self.zoom_path)
    }
    pub(super) fn selected_node(&self) -> Option<&Node> {
        self.selected_path.as_deref().map(|p| self.tree.node_for(p))
    }
    pub(super) fn selected_fs_path(&self) -> Option<PathBuf> {
        self.selected_path.as_deref().map(|p| self.tree.path_for(p))
    }

    pub(super) fn select_path(&mut self, path: Vec<usize>) {
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

    pub(super) fn toggle_expanded(&mut self, path: &[usize]) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_vec());
        }
    }

    pub(super) fn expand_ancestors(&mut self, path: &[usize]) {
        self.expanded.insert(Vec::new());
        for len in 1..path.len() {
            self.expanded.insert(path[..len].to_vec());
        }
    }

    pub(super) fn refresh_extensions(&mut self) {
        self.extensions = collect_extension_rows(self.zoom_node(), self.use_physical);
        self.sort_extensions();
    }

    pub(super) fn sort_extensions(&mut self) {
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

    pub(super) fn reorder_directory_column(
        &mut self,
        source: DirectoryColumn,
        target: DirectoryColumn,
    ) {
        reorder_column(&mut self.directory_column_order, source, target);
    }

    pub(super) fn reorder_extension_column(
        &mut self,
        source: ExtensionColumn,
        target: ExtensionColumn,
    ) {
        reorder_column(&mut self.extension_column_order, source, target);
    }

    pub(super) fn refresh_largest_files(&mut self) {
        self.largest_files = top_files::top_k(&self.tree.root, 200);
    }

    pub(super) fn run_search(&mut self) {
        let outcome = search::search(&self.tree.root, &self.search.query);
        self.search.results = outcome.hits;
        self.search.error = outcome.error;
        self.file_view = FileView::SearchResults;
        self.status = Some(if outcome.truncated {
            "Search capped at 2,000 results".to_string()
        } else {
            format!("{} search result(s)", self.search.results.len())
        });
    }

    pub(super) fn find_duplicates(&mut self) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        self.status = Some("Finding duplicate files…".to_string());
        self.file_view = FileView::DuplicateFiles;
        let tree = Arc::clone(&self.tree);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::duplicates::find_duplicates(tree.as_ref(), None));
        });
        self.duplicate_rx = Some(rx);
    }

    pub(super) fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        if !index_path.is_empty() {
            self.select_path(index_path);
            self.file_view = FileView::AllFiles;
        }
    }

    pub(super) fn zoom_in(&mut self) {
        let Some(path) = self.selected_path.clone() else {
            return;
        };
        let node = self.tree.node_for(&path);
        self.zoom_path = if node.is_dir {
            path
        } else {
            path[..path.len().saturating_sub(1)].to_vec()
        };
        self.refresh_extensions();
    }
    pub(super) fn zoom_out(&mut self) {
        if !self.zoom_path.is_empty() {
            self.zoom_path.pop();
            self.refresh_extensions();
        }
    }
    pub(super) fn reset_zoom(&mut self) {
        self.zoom_path.clear();
        self.refresh_extensions();
    }

    pub(super) fn refresh_scan(&mut self) -> anyhow::Result<()> {
        self.start_scan(self.tree.root_path.clone(), false);
        Ok(())
    }

    pub(super) fn open_folder(&mut self, root: &Path) -> anyhow::Result<()> {
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

    pub(super) fn is_busy(&self) -> bool {
        self.scan_rx.is_some() || self.duplicate_rx.is_some() || self.tools.rx.is_some()
    }

    pub(super) fn busy_text(&self) -> Option<String> {
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

    pub(super) fn duplicate_running(&self) -> bool {
        self.duplicate_rx.is_some()
    }

    /// Switches theme and resolves the new palette once, here, rather
    /// than leaving `theme_id` and `palette` free to disagree.
    pub(super) fn set_theme(&mut self, id: &str) {
        self.theme_id = id.to_string();
        self.palette = super::ui::palette_for(id);
    }

    pub(super) fn open_modal(&mut self, page: ModalPage) {
        self.modal = Some(page);
    }

    pub(super) fn request_windows_tool(&mut self, index: usize) {
        let Some(tool) = crate::wintools::TOOLS.get(index) else {
            return;
        };
        if tool.destructive {
            self.tools.pending = Some(index);
        } else {
            self.start_windows_tool(index);
        }
    }

    pub(super) fn confirm_windows_tool(&mut self) {
        if let Some(index) = self.tools.pending.take() {
            self.start_windows_tool(index);
        }
    }

    fn start_windows_tool(&mut self, index: usize) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        let Some(tool) = crate::wintools::TOOLS.get(index) else {
            return;
        };
        let root = self.tree.root_path.clone();
        let name = tool.name.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::wintools::run(index, &root));
        });
        self.tools.active_name = Some(name.clone());
        self.tools.running = Some(index);
        self.tools.rx = Some(rx);
        self.status = Some(format!("Running {name}…"));
        // The page stays open. A DISM run takes minutes, and closing the
        // window that started it left a one-line status bar as the only
        // sign anything was happening.
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
    fn release_tree(&mut self) {
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

    pub(super) fn poll_background(&mut self, ctx: &egui::Context) {
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

    /// Picks up the reply to the screenshot the modal asked for.
    ///
    /// Read from the raw event list rather than from a helper because
    /// `Event::Screenshot` is not something egui surfaces any other way;
    /// it arrives one or two frames after the request, which is exactly
    /// why the modal does not paint until it lands. See `ui::modal`.
    fn collect_backdrop(&mut self, ctx: &egui::Context) {
        if !matches!(self.backdrop, Backdrop::Requested { .. }) {
            return;
        }
        let captured = ctx.input(|i| {
            i.raw.events.iter().rev().find_map(|event| {
                // `if let` rather than a match with a wildcard arm:
                // `egui::Event` has fifteen other variants and none of
                // them will ever be interesting here, so listing them
                // to satisfy an exhaustiveness lint would be noise.
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(Arc::clone(image))
                } else {
                    None
                }
            })
        });
        if let Some(image) = captured {
            self.backdrop = super::ui::install_backdrop(ctx, &image, self.palette.mode.is_dark());
        }
    }

    fn save_preferences(&self) {
        crate::config::save(&crate::config::Config {
            sort: Some(self.sort),
            use_physical: Some(self.use_physical),
            gui_theme: Some(self.theme_id.clone()),
            ..self.view.to_config()
        });
    }

    pub(super) fn request_delete_selected(&mut self, permanent: bool) {
        let Some(index_path) = self.selected_path.clone() else {
            return;
        };
        // The first row represents scan context, not a cleanup target.
        // Never let selecting it queue deletion of a whole drive/root.
        if index_path.is_empty() {
            self.status = Some("The scan root cannot be deleted from this view".to_string());
            return;
        }
        let node = self.tree.node_for(&index_path);
        self.pending_delete = Some(PendingDelete {
            index_path,
            name: node.name.clone(),
            is_dir: node.is_dir,
            permanent,
        });
    }

    pub(super) fn confirm_delete(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        // Belt and braces with the clearing in `replace_tree`: confirm
        // the indices still lead to the item the user actually chose
        // before handing a path to `remove_dir_all`. Deleting the wrong
        // thing is not an error worth being clever about recovering
        // from.
        let Some(node) = self.tree.try_node_for(&pending.index_path) else {
            self.status = Some(format!(
                "{} is no longer where it was; nothing was deleted",
                pending.name
            ));
            return Ok(());
        };
        if node.name != pending.name || node.is_dir != pending.is_dir {
            self.status = Some(format!(
                "{} moved since it was selected; nothing was deleted",
                pending.name
            ));
            return Ok(());
        }
        let path = self.tree.path_for(&pending.index_path);
        if pending.permanent {
            if pending.is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        } else {
            trash::delete(&path).map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }
        self.status = Some(format!(
            "{}: {}",
            if pending.permanent {
                "Permanently deleted"
            } else {
                "Moved to Recycle Bin"
            },
            path.display()
        ));
        self.selected_path = None;
        self.refresh_scan()
    }

    pub(super) fn confirm_empty(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        if !pending.is_dir {
            return Ok(());
        }
        let path = self.tree.path_for(&pending.index_path);
        for child in std::fs::read_dir(&path)?.filter_map(Result::ok) {
            trash::delete(child.path())
                .map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }
        self.status = Some(format!("Emptied: {}", path.display()));
        self.refresh_scan()
    }

    /// Rebuilds [`Self::treemap_tiles`] if the panel rect or anything the
    /// layout depends on has changed since the last frame, and otherwise
    /// leaves the existing tiles in place.
    pub(super) fn refresh_treemap(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label_strip: f32,
        interactive: bool,
    ) {
        let max_tiles = if interactive {
            treemap_layout::MAX_TILES_INTERACTIVE
        } else {
            treemap_layout::MAX_TILES
        };
        let show_free_space =
            self.view.free_space && self.zoom_path.is_empty() && self.tree.is_volume_root();
        let free_space = if show_free_space {
            self.tree.volume_free
        } else {
            None
        };
        let key = TreemapKey {
            tree: Arc::as_ptr(&self.tree) as usize,
            zoom_path: self.zoom_path.clone(),
            // Rounded to whole pixels because that is the resolution the
            // layout itself quantizes to: a sub-pixel change in the
            // splitter position cannot change a single tile.
            rect: [
                x.round() as i32,
                y.round() as i32,
                w.round() as i32,
                h.round() as i32,
            ],
            physical: self.use_physical,
            free_space: free_space.is_some(),
            label_strip: label_strip.ceil() as i32,
            max_tiles,
        };
        if self.treemap_key.as_ref() == Some(&key) {
            return;
        }

        let mut tiles = treemap_layout::build(
            self.zoom_node(),
            &treemap_layout::LayoutRequest {
                x,
                y,
                width: w,
                height: h,
                use_physical: self.use_physical,
                free_space,
                label_strip,
                max_tiles,
            },
        );
        for tile in &mut tiles {
            if tile.is_node() {
                let mut absolute = self.zoom_path.clone();
                absolute.extend_from_slice(&tile.index_path);
                tile.index_path = absolute;
            }
        }
        self.treemap_tiles = tiles;
        self.treemap_key = Some(key);
    }

    /// Rebuilds [`Self::visible_rows`] if the tree, sort order, size mode,
    /// or expanded set has changed since the last frame.
    pub(super) fn refresh_visible_rows(&mut self) {
        let key = RowKey {
            tree: Arc::as_ptr(&self.tree) as usize,
            sort: self.sort,
            physical: self.use_physical,
            expanded: expanded_fingerprint(&self.expanded),
        };
        if self.visible_rows_key.as_ref() == Some(&key) {
            return;
        }

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
        self.visible_rows = rows;
        self.visible_rows_key = Some(key);
    }
}

fn push_tree_rows(
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

pub(super) fn sort_nodes(nodes: &mut [(usize, &Node)], sort: SortMode, physical: bool) {
    match sort {
        SortMode::SizeDesc => nodes.sort_by(|a, b| {
            b.1.effective_size(physical)
                .cmp(&a.1.effective_size(physical))
        }),
        SortMode::SizeAsc => nodes.sort_by(|a, b| {
            a.1.effective_size(physical)
                .cmp(&b.1.effective_size(physical))
        }),
        SortMode::NameAsc => nodes.sort_by_key(|a| a.1.name.to_lowercase()),
        SortMode::NameDesc => nodes.sort_by_key(|b| std::cmp::Reverse(b.1.name.to_lowercase())),
        SortMode::ModifiedDesc => nodes.sort_by_key(|b| std::cmp::Reverse(b.1.modified)),
        SortMode::ModifiedAsc => nodes.sort_by_key(|a| a.1.modified),
    }
}

/// A finished scan, with the two whole-tree summaries already computed.
///
/// Both used to be derived on the UI thread the instant the tree landed.
/// `top_k` and the extension roll-up each walk every node, so on a large
/// drive that was seconds of frozen window at exactly the moment the scan
/// appeared to finish — the app looked like it hung and then snapped into
/// place. The scan thread has the tree first and nothing else to do with
/// it, so it does this work before handing anything over.
pub(super) struct ScanOutcome {
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
fn drop_in_background<T: Send + 'static>(value: T) {
    // If the spawn itself fails, `spawn` drops the closure — and with it
    // the value — right here, which is the correct fallback.
    let _ = std::thread::Builder::new()
        .name("rustdirstat-reclaim".to_owned())
        .spawn(move || drop(value));
}

fn valid_prefix(tree: &Tree, requested: &[usize]) -> Vec<usize> {
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

/// What [`extension_label`] returns for a file with no extension.
pub(super) const NO_EXTENSION_LABEL: &str = "[no extension]";

pub(super) fn extension_label(name: &str) -> String {
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
pub(super) fn collect_extension_rows(node: &Node, physical: bool) -> Vec<ExtensionRow> {
    let mut by_ext: HashMap<String, (Category, u64, u64)> = HashMap::new();
    collect_extensions(node, physical, &mut by_ext);
    by_ext
        .into_iter()
        .map(|(extension, (category, size, count))| ExtensionRow {
            extension,
            category,
            size,
            count,
        })
        .collect()
}

fn collect_extensions(
    node: &Node,
    physical: bool,
    out: &mut HashMap<String, (Category, u64, u64)>,
) {
    for child in &node.children {
        if child.is_dir {
            collect_extensions(child, physical, out);
        } else {
            let extension = extension_label(&child.name);
            let category = child
                .category
                .unwrap_or_else(|| category_for_name(&child.name));
            let entry = out.entry(extension).or_insert((category, 0, 0));
            entry.1 = entry.1.saturating_add(child.effective_size(physical));
            entry.2 = entry.2.saturating_add(1);
        }
    }
}

pub(super) fn size_label(bytes: u64, physical: bool) -> String {
    format!(
        "{}{}",
        human_bytes(bytes),
        if physical { " (physical)" } else { "" }
    )
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background(ctx);
        super::ui::draw(self, ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_preferences();
        // Preferences are the only thing that has to survive the process,
        // and they are on disk by now. Everything else is a cache of the
        // filesystem, so the scanned tree is handed off instead of being
        // walked and freed on the way out — on a whole-drive scan that
        // teardown is the difference between the window vanishing at once
        // and it sitting there unresponsive while millions of allocations
        // are returned to an allocator that is about to be discarded
        // wholesale anyway.
        self.release_tree();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extension_label, Category, FileView, GuiApp, PaneOrientation, PendingDelete, ViewOptions,
    };
    use crate::tui::SortMode;
    use std::time::{Duration, Instant};

    #[test]
    fn extension_labels_match_windirstat_style() {
        assert_eq!(extension_label("Movie.MKV"), ".mkv");
        assert_eq!(extension_label("README"), "[no extension]");
        assert_eq!(extension_label("archive.tar.gz"), ".gz");
    }

    /// A scratch directory no other test can collide with.
    ///
    /// The counter is the part that matters. Tests run in parallel, and a
    /// clock alone is not enough to separate them: `SystemTime::now` has
    /// coarse enough resolution on Windows that two calls can return the
    /// same value, at which point both tests share a directory and
    /// whichever finishes first deletes it out from under the other. That
    /// surfaced as an intermittent `PermissionDenied` from
    /// `remove_dir_all` on Windows CI.
    fn test_dir(name: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "rustdirstat_gui_{}_{}_{}",
            std::process::id(),
            name,
            unique
        ))
    }

    fn wait_for_background(app: &mut GuiApp) {
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.is_busy() && Instant::now() < deadline {
            app.poll_background(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!app.is_busy(), "background operation timed out");
    }

    /// The index path of a visible row, by name.
    ///
    /// Returns an error rather than unwrapping so a test that names a row
    /// which is not there says which row, instead of reporting a bare
    /// `None`.
    fn row_path(app: &GuiApp, name: &str) -> anyhow::Result<Vec<usize>> {
        app.visible_rows
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.path.clone())
            .ok_or_else(|| anyhow::anyhow!("{name} should be a visible row"))
    }

    #[test]
    fn folder_changes_scan_without_blocking_the_caller() -> anyhow::Result<()> {
        let first = test_dir("first");
        let second = test_dir("second");
        std::fs::create_dir_all(&first)?;
        std::fs::create_dir_all(&second)?;
        std::fs::write(first.join("old.txt"), b"old")?;
        std::fs::write(second.join("new.txt"), b"new")?;

        let mut app = GuiApp::new(crate::scanner::scan(&first, None)?);
        app.open_folder(&second)?;
        assert!(app.is_busy());
        assert_eq!(app.tree.root_path, first);
        wait_for_background(&mut app);
        assert_eq!(app.tree.root_path, second);

        std::fs::remove_dir_all(first)?;
        std::fs::remove_dir_all(second)?;
        Ok(())
    }

    #[test]
    fn initial_gui_shell_opens_before_scan_finishes() -> anyhow::Result<()> {
        let dir = test_dir("initial");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("payload.dat"), vec![7_u8; 4096])?;

        let mut app = GuiApp::loading(dir.clone());
        assert!(app.is_busy());
        assert_eq!(app.tree.root.size, 0);
        wait_for_background(&mut app);
        assert_eq!(app.tree.root.size, 4096);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn duplicate_hashing_runs_in_background() -> anyhow::Result<()> {
        let dir = test_dir("duplicates");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("one.bin"), b"same bytes")?;
        std::fs::write(dir.join("two.bin"), b"same bytes")?;

        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);
        app.find_duplicates();
        assert!(app.is_busy());
        wait_for_background(&mut app);
        assert_eq!(app.duplicate_groups.len(), 1);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    fn nested_test_tree() -> anyhow::Result<std::path::PathBuf> {
        let dir = test_dir("cache");
        std::fs::create_dir_all(dir.join("alpha"))?;
        std::fs::create_dir_all(dir.join("beta"))?;
        std::fs::write(dir.join("alpha/small.bin"), vec![1_u8; 16])?;
        std::fs::write(dir.join("alpha/large.bin"), vec![2_u8; 4096])?;
        std::fs::write(dir.join("beta/only.bin"), vec![3_u8; 512])?;
        Ok(dir)
    }

    /// The row cache is keyed off observed state rather than invalidated
    /// by hand, so what needs proving is that every input actually
    /// reaches the key — a missed one would leave the table painting a
    /// stale tree while the app looked like it was ignoring input.
    #[test]
    fn cached_rows_refresh_whenever_an_input_changes() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);

        app.refresh_visible_rows();
        let collapsed = app.visible_rows.len();

        // Expanding a directory reveals its children.
        let alpha = row_path(&app, "alpha")?;
        app.toggle_expanded(&alpha);
        app.refresh_visible_rows();
        assert!(
            app.visible_rows.len() > collapsed,
            "expanding a directory did not add rows"
        );
        let expanded = app.visible_rows.len();

        // Sort order changes which child comes first.
        app.sort = SortMode::SizeDesc;
        app.refresh_visible_rows();
        let biggest_first = app.visible_rows[1].name.clone();
        app.sort = SortMode::SizeAsc;
        app.refresh_visible_rows();
        assert_ne!(
            biggest_first, app.visible_rows[1].name,
            "flipping the sort order did not reorder the rows"
        );
        assert_eq!(app.visible_rows.len(), expanded);

        // Collapsing again gets back to where it started.
        app.toggle_expanded(&alpha);
        app.refresh_visible_rows();
        assert_eq!(app.visible_rows.len(), collapsed);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn cached_rows_are_not_rebuilt_when_nothing_changed() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);

        app.refresh_visible_rows();
        let first = app.visible_rows.as_ptr();
        for _ in 0..8 {
            app.refresh_visible_rows();
        }
        assert_eq!(
            first,
            app.visible_rows.as_ptr(),
            "the row list was reallocated despite nothing changing, so it is \
             being rebuilt every frame"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn cached_treemap_follows_the_panel_rect_and_the_zoom() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);

        app.refresh_treemap(0.0, 0.0, 400.0, 300.0, 16.0, false);
        assert!(!app.treemap_tiles.is_empty());
        let stable = app.treemap_tiles.as_ptr();
        app.refresh_treemap(0.0, 0.0, 400.0, 300.0, 16.0, false);
        assert_eq!(
            stable,
            app.treemap_tiles.as_ptr(),
            "the treemap was re-laid-out for an unchanged rect"
        );

        // A different panel size has to produce a different layout.
        app.refresh_treemap(0.0, 0.0, 900.0, 200.0, 16.0, false);
        let widest = app
            .treemap_tiles
            .iter()
            .fold(0.0_f32, |acc, tile| acc.max(tile.x + tile.w));
        assert!(
            widest > 400.0,
            "tiles still fit the old 400px panel after it grew to 900px"
        );

        // So does zooming into a subdirectory.
        app.refresh_visible_rows();
        app.select_path(row_path(&app, "alpha")?);
        app.zoom_in();
        app.refresh_treemap(0.0, 0.0, 900.0, 200.0, 16.0, false);
        assert!(
            app.treemap_tiles
                .iter()
                .any(|tile| tile.name == "large.bin"),
            "zooming in did not re-lay-out the treemap for the new root"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Closing the window must not depend on walking the scanned tree, so
    /// the exit path hands it off rather than freeing it in place.
    #[test]
    fn releasing_the_tree_drops_everything_derived_from_it() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);
        app.refresh_visible_rows();
        app.refresh_treemap(0.0, 0.0, 400.0, 300.0, 16.0, false);
        assert!(!app.visible_rows.is_empty());
        assert!(!app.treemap_tiles.is_empty());
        assert!(!app.largest_files.is_empty());

        app.release_tree();

        assert!(app.visible_rows.is_empty());
        assert!(app.treemap_tiles.is_empty());
        assert!(app.largest_files.is_empty());
        assert!(app.tree.root.children.is_empty());
        assert_eq!(app.tree.root_path, dir);
        // The placeholder still has to answer the queries the UI makes
        // while the window tears down around it.
        assert_eq!(app.zoom_node().size, 0);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn scanning_a_new_folder_clears_the_previous_one_from_the_view() -> anyhow::Result<()> {
        let first = nested_test_tree()?;
        let second = test_dir("second_scan");
        std::fs::create_dir_all(&second)?;
        std::fs::write(second.join("other.bin"), vec![9_u8; 32])?;

        let mut app = GuiApp::new(crate::scanner::scan(&first, None)?);
        // Put the workspace into a thoroughly used state.
        app.refresh_visible_rows();
        let alpha = row_path(&app, "alpha")?;
        app.toggle_expanded(&alpha);
        app.select_path(alpha);
        app.highlighted_extension = Some(".bin".to_string());
        app.highlighted_category = Some(Category::Programs);
        app.search.query = "large".to_string();
        app.search.error = Some("stale".to_string());
        app.file_view = FileView::SearchResults;

        app.open_folder(&second)?;
        wait_for_background(&mut app);

        assert_eq!(app.tree.root_path, second);
        assert!(app.selected_path.is_none(), "selection outlived the scan");
        assert!(app.zoom_path.is_empty(), "zoom outlived the scan");
        assert_eq!(app.expanded.len(), 1, "expansion outlived the scan");
        assert_eq!(app.file_view, FileView::AllFiles);
        // These are what the previous fix missed: a highlight and a
        // search naming things that are not in the new tree at all.
        assert!(app.highlighted_extension.is_none(), "highlight outlived it");
        assert!(app.highlighted_category.is_none(), "highlight outlived it");
        assert!(app.search.query.is_empty(), "search query outlived it");
        assert!(app.search.error.is_none(), "search error outlived it");

        std::fs::remove_dir_all(first)?;
        std::fs::remove_dir_all(second)?;
        Ok(())
    }

    /// A queued deletion holds indices into the tree it was taken from.
    /// Letting one survive a rescan means confirming it deletes whatever
    /// now sits at those indices — a different file entirely.
    #[test]
    fn a_queued_deletion_does_not_survive_a_rescan() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);
        app.refresh_visible_rows();
        app.select_path(row_path(&app, "alpha")?);
        app.request_delete_selected(true);
        assert!(app.pending_delete.is_some(), "the delete should be queued");

        app.refresh_scan()?;
        wait_for_background(&mut app);
        assert!(
            app.pending_delete.is_none(),
            "a rescan left a deletion queued against indices into the old tree"
        );

        // And confirming a deletion whose target no longer matches is
        // refused outright rather than resolved against whatever is
        // there now.
        app.pending_delete = Some(PendingDelete {
            index_path: vec![0],
            name: "not-the-file-that-is-there".to_string(),
            is_dir: false,
            permanent: true,
        });
        app.confirm_delete()?;
        assert!(dir.join("alpha").exists(), "the wrong item was deleted");
        assert!(dir.join("beta").exists(), "the wrong item was deleted");

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn scan_root_cannot_be_queued_for_deletion() -> anyhow::Result<()> {
        let dir = test_dir("root_delete");
        std::fs::create_dir_all(&dir)?;
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None)?);
        app.select_path(Vec::new());
        app.request_delete_selected(false);
        assert!(app.pending_delete.is_none());
        assert!(dir.exists());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Every view toggle survives a save and reload.
    ///
    /// The toggles used to be eight loose fields on `GuiApp`, each wired
    /// to its own config key by hand in two separate places. Nothing
    /// checked that the two agreed, so a field read from the wrong key —
    /// or, once they were grouped into `ViewOptions`, quietly dropped from
    /// one side — would show up only as a setting that forgets itself
    /// between runs.
    #[test]
    fn view_toggles_survive_a_config_round_trip() {
        // Every flag flipped away from its default of `true`, so a
        // mapping that loses one falls back to `true` and is caught.
        let saved = ViewOptions {
            toolbar: false,
            status_bar: false,
            extension_pane: false,
            treemap: false,
            free_space: false,
            grid: false,
            labels: false,
            orientation: PaneOrientation::Vertical,
        };

        let restored = ViewOptions::from_config(&saved.to_config());

        assert!(!restored.toolbar, "toolbar");
        assert!(!restored.status_bar, "status_bar");
        assert!(!restored.extension_pane, "extension_pane");
        assert!(!restored.treemap, "treemap");
        assert!(!restored.free_space, "free_space");
        assert!(!restored.grid, "grid");
        assert!(!restored.labels, "labels");
        assert_eq!(restored.orientation, PaneOrientation::Vertical);

        // And the other way, so neither direction can be the one that
        // silently pins a value.
        let all_on = ViewOptions {
            toolbar: true,
            status_bar: true,
            extension_pane: true,
            treemap: true,
            free_space: true,
            grid: true,
            labels: true,
            orientation: PaneOrientation::Horizontal,
        };
        let restored = ViewOptions::from_config(&all_on.to_config());
        assert!(restored.toolbar && restored.status_bar && restored.extension_pane);
        assert!(restored.treemap && restored.free_space && restored.grid && restored.labels);
        assert_eq!(restored.orientation, PaneOrientation::Horizontal);
    }
}
