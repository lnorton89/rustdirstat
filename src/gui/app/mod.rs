// ============================================================================
// Module:       gui::app
// Description:  GuiApp itself and the state groups it holds; each concern's
//               operations live in a submodule beside it.
//
// Dependencies: eframe::egui, trash (delete to recycle bin); crate::{model,
//               duplicates, color, search, top_files}
// ============================================================================

//! `GuiApp`: the state the desktop window owns — the scanned tree, the
//! view state over it, and the caches derived from both.
//!
//! The operations over that state are *not* here. One file holding every
//! field and every transition means the blast radius of a change is the
//! whole file, and the same layout bugs kept coming back because a fix in
//! one method could not be reasoned about without reading the rest. Each
//! concern now owns its state and the methods that move it:
//!
//! - [`rows`] — the flattened row list, its cache, expansion, selection
//! - [`treemap`] — the tile cache and zoom
//! - [`scan`] — starting, replacing and retiring a tree
//! - [`tools`] — deleting, emptying, maintenance tools, duplicates
//! - [`extensions`] — the extension breakdown and column order
//!
//! Rust allows an `impl GuiApp` block per module, so a method sits with
//! the state it touches while `GuiApp` stays one type. What remains here
//! is the struct, its state groups, and the `eframe::App` loop.
//!
//! The window is immediate mode: `gui::ui::draw` rebuilds it in full
//! every frame. A scan of a whole drive is roughly nine million nodes, so
//! anything O(tree) on a draw path freezes the window outright. Derived
//! data therefore lives in caches here — `refresh_visible_rows`,
//! `refresh_treemap` — keyed off observed state through `RowKey` and
//! `TreemapKey` rather than invalidated by hand. A new field that affects
//! rows or tiles has to join the matching key, or the cache will go on
//! serving results that no longer match what is on screen.
//!
//! Freeing a tree is tree-sized too, which is why it goes through
//! `drop_in_background` rather than happening on the UI thread.
//!
//! State is grouped by the view that owns it (`SearchState`,
//! `ToolsState`, `ViewOptions`); a new field belongs in its group rather
//! than flat on `GuiApp`.

use crate::color::Category;
use crate::duplicates::DupGroup;
use crate::model::SortMode;
use crate::model::{category_for_name, Node, Tree};
use crate::util::human_bytes;
use crate::{search, top_files};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::treemap_layout;
use super::ui::{ModalPage, Palette};

mod extensions;
mod rows;
mod scan;
mod tools;
mod treemap;

pub(in crate::gui) use extensions::*;
pub(in crate::gui) use rows::*;
pub(in crate::gui) use scan::*;
pub(in crate::gui) use tools::*;
pub(in crate::gui) use treemap::*;

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
pub(in crate::gui) enum Backdrop {
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
    pub(in crate::gui) fn texture(&self) -> Option<&egui::TextureHandle> {
        match self {
            Backdrop::Ready(texture) => Some(texture),
            Backdrop::Idle | Backdrop::Requested { .. } | Backdrop::Unavailable => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui) enum PaneOrientation {
    Horizontal,
    Vertical,
}

impl PaneOrientation {
    pub(in crate::gui) fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Treemap below",
            Self::Vertical => "Treemap right",
        }
    }
    pub(in crate::gui) fn toggle(&mut self) {
        *self = match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        };
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui) enum FileView {
    AllFiles,
    LargestFiles,
    DuplicateFiles,
    SearchResults,
}

impl FileView {
    pub(in crate::gui) fn label(self) -> &'static str {
        match self {
            Self::AllFiles => "All Files",
            Self::LargestFiles => "Largest Files",
            Self::DuplicateFiles => "Duplicate Files",
            Self::SearchResults => "Search Results",
        }
    }
}

/// Which parts of the window are showing, and how they are arranged.
///
/// These are the toggles the View menu offers and the ones saved to the
/// config between runs, so they travel together everywhere: the menu
/// writes them, the layout reads them, `to_config` persists them.
pub(in crate::gui) struct ViewOptions {
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
pub(in crate::gui) struct SearchState {
    pub query: String,
    pub results: Vec<search::SearchHit>,
    pub error: Option<String>,
}

pub(in crate::gui) struct GuiApp {
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
    scan_rx: Option<mpsc::Receiver<ScanMessage>>,
    scan_resets_workspace: bool,
    /// The user's zoom/selection/expansion captured as name identities
    /// when a refresh scan started, to be re-derived against the new
    /// tree when it lands. `None` for a fresh-folder scan, which resets
    /// the workspace anyway.
    restore: Option<scan::RestoreState>,
    duplicate_rx: Option<mpsc::Receiver<crate::duplicates::DupScan>>,
    /// A search in flight. Search is the one whole-tree walk that used
    /// to run on the frame thread — ten million nodes of regex matching
    /// froze the window — so it runs on a worker like scanning and
    /// duplicates do, and `poll_background` applies the result.
    search_rx: Option<mpsc::Receiver<crate::search::SearchOutcome>>,
    /// A zoom-time extension recomputation in flight. The scan already
    /// computes the *root* extension rows off the frame thread, but
    /// zooming into a subtree recomputes them for that subtree — a walk
    /// the size of the zoomed region, which on a drive-sized scan is
    /// millions of nodes. Same worker pattern as the search.
    extensions_rx: Option<mpsc::Receiver<Vec<ExtensionRow>>>,
    /// Raised to stop the worker behind `extensions_rx` mid-walk when
    /// its answer became obsolete — a newer zoom superseded it, or a
    /// rescan retired the tree it was walking.
    extensions_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub tools: ToolsState,
}

impl GuiApp {
    pub(in crate::gui) fn new(tree: Tree) -> Self {
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
            restore: None,
            duplicate_rx: None,
            search_rx: None,
            extensions_rx: None,
            extensions_cancel: None,
            tools: ToolsState::default(),
        };
        app.refresh_extensions();
        app.refresh_largest_files();
        app
    }

    /// Switches theme and resolves the new palette once, here, rather
    /// than leaving `theme_id` and `palette` free to disagree.
    pub(in crate::gui) fn set_theme(&mut self, id: &str) {
        self.theme_id = id.to_string();
        self.palette = super::ui::palette_for(id);
    }

    pub(in crate::gui) fn open_modal(&mut self, page: ModalPage) {
        self.modal = Some(page);
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
        let result = crate::config::save(&crate::config::Config {
            sort: Some(self.sort),
            use_physical: Some(self.use_physical),
            gui_theme: Some(self.theme_id.clone()),
            ..self.view.to_config()
        });
        // This runs from `on_exit` — the window is already going away,
        // so stderr is the only channel left. A quiet stderr line beats
        // the old behavior, which was every failed save looking exactly
        // like a successful one.
        if let Err(error) = result {
            eprintln!("rustdirstat: preferences were not saved: {error}");
        }
    }
}

/// What [`extension_label`] returns for a file with no extension.
pub(in crate::gui) const NO_EXTENSION_LABEL: &str = "[no extension]";

impl eframe::App for GuiApp {
    /// Everything that is not drawing.
    ///
    /// eframe 0.34 split the old `update` in two: `logic` runs once
    /// before each `ui`, and additionally while the window is hidden and
    /// something has asked for a repaint. Polling the background workers
    /// belongs on that side of the line — a finished scan has to be
    /// collected whether or not anyone is looking at the window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        super::ui::draw(self, ui);
    }

    fn on_exit(&mut self) {
        self.save_preferences();
        // A scan still walking has nowhere to deliver to. Telling it to
        // stop is not required for the process to exit — nothing joins it
        // — but a window that has visibly closed should not leave every
        // core but one busy for another minute.
        self.cancel_scan();
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
        extension_label, Category, FileView, GuiApp, Node, PaneOrientation, PendingDelete, Tree,
        ViewOptions,
    };
    use crate::model::SortMode;
    use crate::util::scratch_dir;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// A tree whose logical and physical sizes differ, so switching size
    /// mode changes what every row reads.
    fn app_with_differing_sizes() -> GuiApp {
        fn file(name: &str, size: u64, physical: u64) -> Node {
            Node {
                name: std::ffi::OsString::from(name),
                is_dir: false,
                is_symlink: false,
                size,
                physical_size: physical,
                file_count: 1,
                dir_count: 0,
                modified: None,
                children: Vec::new(),
                error: false,
                category: Some(Category::NoExtension),
                ext_totals: Vec::new(),
                unreadable_count: 0,
                file_id: None,
                other_filesystem: false,
            }
        }
        let mut totals = vec![(0, 0, 0); Category::COUNT];
        totals[Category::NoExtension.index()] = (100, 4096, 1);
        GuiApp::new(Tree {
            root_path: PathBuf::from("root"),
            volume_free: None,
            volume_total: None,
            root: Node {
                name: std::ffi::OsString::from("root"),
                is_dir: true,
                is_symlink: false,
                size: 100,
                physical_size: 4096,
                file_count: 1,
                dir_count: 0,
                modified: None,
                children: vec![file("sparse.bin", 100, 4096)],
                error: false,
                category: None,
                ext_totals: totals,
                unreadable_count: 0,
                file_id: None,
                other_filesystem: false,
            },
        })
    }

    /// Changing something the rows are drawn from rebuilds them.
    ///
    /// This is the test that drives the cache's own debug check: with a
    /// row input missing from `RowKey`, the second `refresh_visible_rows`
    /// takes the early return, the check compares the cached rows against
    /// a fresh build, and the mismatch trips the assertion. Without a
    /// test that actually flips an input, that guard never runs.
    #[test]
    fn switching_size_mode_rebuilds_the_row_cache() {
        let mut app = app_with_differing_sizes();
        app.expanded.insert(Vec::new());

        app.use_physical = false;
        app.refresh_visible_rows();
        let logical: Vec<u64> = app.visible_rows.iter().map(|row| row.size).collect();

        app.use_physical = true;
        app.refresh_visible_rows();
        let physical: Vec<u64> = app.visible_rows.iter().map(|row| row.size).collect();

        assert_ne!(
            logical, physical,
            "the rows should be rebuilt when the size mode changes, not served from cache"
        );
        assert!(
            physical.contains(&4096),
            "physical mode should show the allocated size: {physical:?}"
        );
    }

    /// Same again for sort order and for expanding a directory, so each
    /// field of `RowKey` is covered by something that exercises it.
    #[test]
    fn sorting_and_expanding_both_rebuild_the_row_cache() {
        let mut app = app_with_differing_sizes();
        app.expanded.insert(Vec::new());

        app.refresh_visible_rows();
        let before = app.visible_rows.len();

        app.expanded.clear();
        app.refresh_visible_rows();
        assert_ne!(
            app.visible_rows.len(),
            before,
            "collapsing the root should change how many rows there are"
        );

        app.expanded.insert(Vec::new());
        app.refresh_visible_rows();
        assert_eq!(
            app.visible_rows.len(),
            before,
            "and expanding it again should bring them back"
        );

        app.sort = SortMode::NameAsc;
        app.refresh_visible_rows();
        app.sort = SortMode::SizeDesc;
        app.refresh_visible_rows();
    }

    #[test]
    fn extension_labels_match_windirstat_style() {
        assert_eq!(extension_label("Movie.MKV"), ".mkv");
        assert_eq!(extension_label("README"), "[no extension]");
        assert_eq!(extension_label("archive.tar.gz"), ".gz");
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
        let first = scratch_dir("gui", "first");
        let second = scratch_dir("gui", "second");
        std::fs::create_dir_all(&first)?;
        std::fs::create_dir_all(&second)?;
        std::fs::write(first.join("old.txt"), b"old")?;
        std::fs::write(second.join("new.txt"), b"new")?;

        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&first)?);
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
        let dir = scratch_dir("gui", "initial");
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
        let dir = scratch_dir("gui", "duplicates");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("one.bin"), b"same bytes")?;
        std::fs::write(dir.join("two.bin"), b"same bytes")?;

        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
        app.find_duplicates();
        assert!(app.is_busy());
        wait_for_background(&mut app);
        assert_eq!(app.duplicate_groups.len(), 1);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    fn nested_test_tree() -> anyhow::Result<std::path::PathBuf> {
        let dir = scratch_dir("gui", "cache");
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
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);

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
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);

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
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);

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
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
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
        let second = scratch_dir("gui", "second_scan");
        std::fs::create_dir_all(&second)?;
        std::fs::write(second.join("other.bin"), vec![9_u8; 32])?;

        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&first)?);
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

    /// A refresh scan restores the browsing location by name identity,
    /// through the whole scan/poll pipeline, so the user stays where
    /// they were.
    #[test]
    fn a_refresh_scan_restores_selection_and_zoom() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
        app.refresh_visible_rows();
        let alpha = row_path(&app, "alpha")?;
        app.toggle_expanded(&alpha);
        app.select_path(alpha.clone());
        app.zoom_path = alpha.clone();

        app.refresh_scan()?;
        wait_for_background(&mut app);

        let selected = app
            .selected_path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("the selection should have been restored"))?;
        assert_eq!(
            selected, alpha,
            "the selection should survive a refresh scan"
        );
        assert_eq!(
            app.zoom_path, alpha,
            "the zoom should survive a refresh scan"
        );
        assert!(
            app.expanded.contains(&alpha),
            "the expansion should survive a refresh scan"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// A selection is an exact claim about a specific item, so one whose
    /// file vanished between scans must be dropped, not silently moved to
    /// its parent directory — landing on the parent would make the
    /// selection point at a different thing than the user chose.
    #[test]
    fn a_selection_whose_file_vanished_is_dropped_not_moved_to_its_parent() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
        app.refresh_visible_rows();
        let alpha = row_path(&app, "alpha")?;
        app.toggle_expanded(&alpha);
        app.refresh_visible_rows();
        let small = row_path(&app, "small.bin")?;
        app.select_path(small.clone());
        assert_eq!(app.selected_path.as_deref(), Some(&small[..]));

        // The file disappears from disk before the refresh scan runs.
        std::fs::remove_file(dir.join("alpha").join("small.bin"))?;
        app.refresh_scan()?;
        wait_for_background(&mut app);

        assert!(
            app.selected_path.is_none(),
            "a vanished selection must not resolve to its parent directory"
        );
        assert!(
            dir.join("alpha").exists(),
            "the parent still exists — the test must prove the selection \
             was dropped rather than moved to it"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// A refresh that starts while the view is already stale — the zoom
    /// path no longer resolves, so no name identity can be captured —
    /// must still not carry raw index paths onto the new tree
    /// unvalidated. Same semantics as the restore itself: placement
    /// truncates to what exists, selection is exact or dropped.
    #[test]
    fn a_stale_view_is_still_validated_when_no_identity_could_be_captured() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
        app.refresh_visible_rows();
        // Point the view somewhere the current tree cannot resolve, so
        // the capture step has nothing to capture.
        app.zoom_path = vec![97, 98];
        app.selected_path = Some(vec![97, 98, 99]);
        app.expanded.insert(vec![97]);

        app.refresh_scan()?;
        wait_for_background(&mut app);

        assert_eq!(
            app.zoom_path,
            Vec::<usize>::new(),
            "a stale zoom truncates to the deepest place that exists"
        );
        assert!(
            app.selected_path.is_none(),
            "a stale selection is dropped, not carried onto the new tree"
        );
        assert!(
            app.expanded
                .iter()
                .all(|path| app.tree.node_for(path).is_some()),
            "no expanded path may dangle into the new tree"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Zooming recomputes the extension breakdown for the zoomed subtree
    /// — on a worker, so zooming into a drive-sized subtree cannot freeze
    /// the window, and the rows that arrive describe the subtree the
    /// user actually zoomed into.
    #[test]
    fn zoom_recomputes_extensions_for_the_zoomed_subtree() -> anyhow::Result<()> {
        let dir = scratch_dir("gui", "ext_zoom");
        std::fs::create_dir_all(dir.join("alpha"))?;
        std::fs::create_dir_all(dir.join("beta"))?;
        std::fs::write(dir.join("alpha/one.bin"), vec![1_u8; 8])?;
        std::fs::write(dir.join("beta/two.txt"), vec![2_u8; 8])?;

        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
        app.refresh_visible_rows();
        let alpha = row_path(&app, "alpha")?;
        app.zoom_path = alpha;
        app.refresh_extensions();

        // The rows arrive on a worker; wait for them through the same
        // poll path the frame update uses.
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.extensions_pending() && Instant::now() < deadline {
            app.poll_background(&ctx);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !app.extensions_pending(),
            "the extension worker should finish"
        );
        let exts: Vec<String> = app.extensions.iter().map(|e| e.extension.clone()).collect();
        assert_eq!(
            exts,
            [".bin".to_string()],
            "zoomed into alpha — only its extension should remain"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// A queued deletion holds indices into the tree it was taken from.
    /// Letting one survive a rescan means confirming it deletes whatever
    /// now sits at those indices — a different file entirely.
    #[test]
    fn a_queued_deletion_does_not_survive_a_rescan() -> anyhow::Result<()> {
        let dir = nested_test_tree()?;
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
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
            name: std::ffi::OsString::from("not-the-file-that-is-there"),
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
        let dir = scratch_dir("gui", "root_delete");
        std::fs::create_dir_all(&dir)?;
        let mut app = GuiApp::new(crate::scanner::scan_to_completion(&dir)?);
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
