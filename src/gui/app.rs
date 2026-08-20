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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaneOrientation {
    Horizontal,
    Vertical,
}

impl PaneOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Treemap below",
            Self::Vertical => "Treemap right",
        }
    }
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        };
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileView {
    AllFiles,
    LargestFiles,
    DuplicateFiles,
    SearchResults,
}

impl FileView {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllFiles => "All Files",
            Self::LargestFiles => "Largest Files",
            Self::DuplicateFiles => "Duplicate Files",
            Self::SearchResults => "Search Results",
        }
    }
}

#[derive(Clone)]
pub struct ExtensionRow {
    pub extension: String,
    pub category: Category,
    pub size: u64,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtensionSortMode {
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

fn extension_color_sort_key(extension: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in extension.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash % 360
}

pub struct PendingDelete {
    pub index_path: Vec<usize>,
    pub name: String,
    pub is_dir: bool,
    pub permanent: bool,
}

pub struct GuiApp {
    pub tree: Arc<Tree>,
    pub zoom_path: Vec<usize>,
    pub selected_path: Option<Vec<usize>>,
    pub expanded: HashSet<Vec<usize>>,
    pub sort: SortMode,
    pub use_physical: bool,
    pub extensions: Vec<ExtensionRow>,
    pub extension_sort: ExtensionSortMode,
    pub highlighted_extension: Option<String>,
    pub highlighted_category: Option<Category>,
    pub pending_delete: Option<PendingDelete>,
    pub status: Option<String>,
    pub show_properties: bool,
    pub show_settings: bool,
    pub show_about: bool,
    pub show_windows_tools: bool,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub show_extension_view: bool,
    pub show_treemap: bool,
    pub show_free_space: bool,
    pub show_grid: bool,
    pub show_labels: bool,
    pub orientation: PaneOrientation,
    pub file_view: FileView,
    pub largest_files: Vec<top_files::TopFile>,
    pub duplicate_groups: Vec<DupGroup>,
    pub search_query: String,
    pub search_results: Vec<search::SearchHit>,
    pub search_error: Option<String>,
    pub scan_progress: Option<Arc<crate::scanner::Progress>>,
    scan_rx: Option<mpsc::Receiver<Result<Tree, String>>>,
    scan_resets_workspace: bool,
    duplicate_rx: Option<mpsc::Receiver<Vec<DupGroup>>>,
    pub pending_windows_tool: Option<usize>,
    tool_rx: Option<mpsc::Receiver<Result<String, String>>>,
    active_tool_name: Option<String>,
}

impl GuiApp {
    pub fn loading(root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| root.display().to_string());
        let is_dir = root.is_dir();
        let placeholder = Tree {
            root_path: root.clone(),
            root: Node {
                name,
                is_dir,
                is_symlink: false,
                size: 0,
                physical_size: 0,
                file_count: 0,
                dir_count: 0,
                modified: None,
                children: Vec::new(),
                error: false,
                category: None,
                ext_totals: if is_dir {
                    vec![(0, 0, 0); Category::COUNT]
                } else {
                    Vec::new()
                },
                unreadable_count: 0,
            },
            volume_free: None,
            volume_total: None,
        };
        let mut app = Self::new(placeholder);
        app.start_scan(root, true);
        app
    }

    pub fn new(tree: Tree) -> Self {
        let mut expanded = HashSet::new();
        expanded.insert(Vec::new());
        let config = crate::config::load();
        let mut app = Self {
            tree: Arc::new(tree),
            zoom_path: Vec::new(),
            selected_path: None,
            expanded,
            sort: config.sort.unwrap_or(SortMode::SizeDesc),
            use_physical: config.use_physical.unwrap_or(false),
            extensions: Vec::new(),
            extension_sort: ExtensionSortMode::BytesDesc,
            highlighted_extension: None,
            highlighted_category: None,
            pending_delete: None,
            status: None,
            show_properties: false,
            show_settings: false,
            show_about: false,
            show_windows_tools: false,
            show_toolbar: config.gui_show_toolbar.unwrap_or(true),
            show_status_bar: config.gui_show_status_bar.unwrap_or(true),
            show_extension_view: config.gui_show_extensions.unwrap_or(true),
            show_treemap: config.show_treemap.unwrap_or(true),
            show_free_space: config.gui_show_free_space.unwrap_or(true),
            show_grid: config.gui_show_grid.unwrap_or(true),
            show_labels: config.gui_show_labels.unwrap_or(true),
            orientation: match config.gui_orientation.as_deref() {
                Some("vertical") => PaneOrientation::Vertical,
                _ => PaneOrientation::Horizontal,
            },
            file_view: FileView::AllFiles,
            largest_files: Vec::new(),
            duplicate_groups: Vec::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            search_error: None,
            scan_progress: None,
            scan_rx: None,
            scan_resets_workspace: false,
            duplicate_rx: None,
            pending_windows_tool: None,
            tool_rx: None,
            active_tool_name: None,
        };
        app.refresh_extensions();
        app.refresh_largest_files();
        app
    }

    pub fn zoom_node(&self) -> &Node {
        self.tree.node_for(&self.zoom_path)
    }
    pub fn zoom_fs_path(&self) -> PathBuf {
        self.tree.path_for(&self.zoom_path)
    }
    pub fn selected_node(&self) -> Option<&Node> {
        self.selected_path.as_deref().map(|p| self.tree.node_for(p))
    }
    pub fn selected_fs_path(&self) -> Option<PathBuf> {
        self.selected_path.as_deref().map(|p| self.tree.path_for(p))
    }

    pub fn select_path(&mut self, path: Vec<usize>) {
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

    pub fn toggle_expanded(&mut self, path: &[usize]) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.to_vec());
        }
    }

    pub fn expand_ancestors(&mut self, path: &[usize]) {
        self.expanded.insert(Vec::new());
        for len in 1..path.len() {
            self.expanded.insert(path[..len].to_vec());
        }
    }

    pub fn refresh_extensions(&mut self) {
        let mut by_ext: HashMap<String, (Category, u64, u64)> = HashMap::new();
        collect_extensions(self.zoom_node(), self.use_physical, &mut by_ext);
        let rows: Vec<_> = by_ext
            .into_iter()
            .map(|(extension, (category, size, count))| ExtensionRow {
                extension,
                category,
                size,
                count,
            })
            .collect();
        self.extensions = rows;
        self.sort_extensions();
    }

    pub fn sort_extensions(&mut self) {
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

    pub fn refresh_largest_files(&mut self) {
        self.largest_files = top_files::top_k(&self.tree.root, 200);
    }

    pub fn run_search(&mut self) {
        let outcome = search::search(&self.tree.root, &self.search_query);
        self.search_results = outcome.hits;
        self.search_error = outcome.error;
        self.file_view = FileView::SearchResults;
        self.status = Some(if outcome.truncated {
            "Search capped at 2,000 results".to_string()
        } else {
            format!("{} search result(s)", self.search_results.len())
        });
    }

    pub fn find_duplicates(&mut self) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        self.status = Some("Finding duplicate files…".to_string());
        self.file_view = FileView::DuplicateFiles;
        let tree = Arc::clone(&self.tree);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let groups = crate::duplicates::find_duplicates(tree.as_ref(), None);
            let _ = tx.send(groups);
        });
        self.duplicate_rx = Some(rx);
    }

    pub fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        if !index_path.is_empty() {
            self.select_path(index_path);
            self.file_view = FileView::AllFiles;
        }
    }

    pub fn zoom_in(&mut self) {
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
    pub fn zoom_out(&mut self) {
        if !self.zoom_path.is_empty() {
            self.zoom_path.pop();
            self.refresh_extensions();
        }
    }
    pub fn reset_zoom(&mut self) {
        self.zoom_path.clear();
        self.refresh_extensions();
    }

    pub fn refresh_scan(&mut self) -> anyhow::Result<()> {
        self.start_scan(self.tree.root_path.clone(), false);
        Ok(())
    }

    pub fn open_folder(&mut self, root: &Path) -> anyhow::Result<()> {
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
        let display = root.display().to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::scanner::scan(&root, Some(worker_progress.as_ref()))
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        self.scan_progress = Some(progress);
        self.scan_rx = Some(rx);
        self.scan_resets_workspace = reset_workspace;
        self.status = Some(format!("Scanning {display}…"));
    }

    pub fn is_busy(&self) -> bool {
        self.scan_rx.is_some() || self.duplicate_rx.is_some() || self.tool_rx.is_some()
    }

    pub fn busy_text(&self) -> Option<String> {
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
                self.active_tool_name
                    .as_ref()
                    .map(|name| format!("Running {name}…"))
            })
    }

    pub fn duplicate_running(&self) -> bool {
        self.duplicate_rx.is_some()
    }

    pub fn request_windows_tool(&mut self, index: usize) {
        let Some(tool) = crate::wintools::TOOLS.get(index) else {
            return;
        };
        if tool.destructive {
            self.pending_windows_tool = Some(index);
        } else {
            self.start_windows_tool(index);
        }
    }

    pub fn confirm_windows_tool(&mut self) {
        if let Some(index) = self.pending_windows_tool.take() {
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
        self.active_tool_name = Some(name.clone());
        self.tool_rx = Some(rx);
        self.status = Some(format!("Running {name}…"));
        self.show_windows_tools = false;
    }

    pub fn poll_background(&mut self, ctx: &egui::Context) {
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
                Ok(tree) => {
                    let reset = self.scan_resets_workspace;
                    self.tree = Arc::new(tree);
                    if reset {
                        self.zoom_path.clear();
                        self.selected_path = None;
                        self.expanded.clear();
                        self.expanded.insert(Vec::new());
                        self.file_view = FileView::AllFiles;
                    } else {
                        self.zoom_path = valid_prefix(self.tree.as_ref(), &self.zoom_path);
                        self.selected_path = self
                            .selected_path
                            .as_ref()
                            .map(|path| valid_prefix(self.tree.as_ref(), path));
                    }
                    self.refresh_extensions();
                    self.refresh_largest_files();
                    self.search_results.clear();
                    self.duplicate_groups.clear();
                    self.status = Some(format!("Scan complete: {}", self.tree.root_path.display()));
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
                Ok(groups) => {
                    self.duplicate_groups = groups;
                    self.status = Some(format!(
                        "{} duplicate group(s)",
                        self.duplicate_groups.len()
                    ));
                }
                Err(error) => self.status = Some(error),
            }
        }

        let tool_result = self.tool_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "The maintenance tool worker stopped unexpectedly".to_string(),
            )),
        });
        if let Some(result) = tool_result {
            self.tool_rx = None;
            self.active_tool_name = None;
            self.status = Some(match result {
                Ok(message) => message,
                Err(error) => format!("Tool failed: {error}"),
            });
        }

        if self.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn save_preferences(&self) {
        crate::config::save(&crate::config::Config {
            sort: Some(self.sort),
            show_treemap: Some(self.show_treemap),
            use_physical: Some(self.use_physical),
            gui_orientation: Some(match self.orientation {
                PaneOrientation::Horizontal => "horizontal".to_string(),
                PaneOrientation::Vertical => "vertical".to_string(),
            }),
            gui_show_extensions: Some(self.show_extension_view),
            gui_show_toolbar: Some(self.show_toolbar),
            gui_show_status_bar: Some(self.show_status_bar),
            gui_show_free_space: Some(self.show_free_space),
            gui_show_grid: Some(self.show_grid),
            gui_show_labels: Some(self.show_labels),
            ..crate::config::Config::default()
        });
    }

    pub fn request_delete_selected(&mut self, permanent: bool) {
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

    pub fn confirm_delete(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
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

    pub fn confirm_empty(&mut self) -> anyhow::Result<()> {
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

    pub fn treemap_tiles(&self, x: f32, y: f32, w: f32, h: f32) -> Vec<treemap_layout::Tile> {
        let free_space =
            if self.show_free_space && self.zoom_path.is_empty() && self.tree.is_volume_root() {
                self.tree.volume_free
            } else {
                None
            };
        let mut tiles =
            treemap_layout::build(self.zoom_node(), x, y, w, h, self.use_physical, free_space);
        for tile in &mut tiles {
            if !tile.is_free_space {
                let mut absolute = self.zoom_path.clone();
                absolute.extend_from_slice(&tile.index_path);
                tile.index_path = absolute;
            }
        }
        tiles
    }
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

pub fn extension_label(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| format!(".{}", s.to_ascii_lowercase()))
        .unwrap_or_else(|| "[no extension]".to_string())
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

pub fn size_label(bytes: u64, physical: bool) -> String {
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
    }
}

#[cfg(test)]
mod tests {
    use super::{extension_label, GuiApp};
    use std::time::{Duration, Instant};

    #[test]
    fn extension_labels_match_windirstat_style() {
        assert_eq!(extension_label("Movie.MKV"), ".mkv");
        assert_eq!(extension_label("README"), "[no extension]");
        assert_eq!(extension_label("archive.tar.gz"), ".gz");
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rustdirstat_gui_{}_{}_{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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

    #[test]
    fn folder_changes_scan_without_blocking_the_caller() {
        let first = test_dir("first");
        let second = test_dir("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("old.txt"), b"old").unwrap();
        std::fs::write(second.join("new.txt"), b"new").unwrap();

        let mut app = GuiApp::new(crate::scanner::scan(&first, None).unwrap());
        app.open_folder(&second).unwrap();
        assert!(app.is_busy());
        assert_eq!(app.tree.root_path, first);
        wait_for_background(&mut app);
        assert_eq!(app.tree.root_path, second);

        std::fs::remove_dir_all(first).unwrap();
        std::fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn initial_gui_shell_opens_before_scan_finishes() {
        let dir = test_dir("initial");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("payload.dat"), vec![7_u8; 4096]).unwrap();

        let mut app = GuiApp::loading(dir.clone());
        assert!(app.is_busy());
        assert_eq!(app.tree.root.size, 0);
        wait_for_background(&mut app);
        assert_eq!(app.tree.root.size, 4096);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_hashing_runs_in_background() {
        let dir = test_dir("duplicates");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("one.bin"), b"same bytes").unwrap();
        std::fs::write(dir.join("two.bin"), b"same bytes").unwrap();

        let mut app = GuiApp::new(crate::scanner::scan(&dir, None).unwrap());
        app.find_duplicates();
        assert!(app.is_busy());
        wait_for_background(&mut app);
        assert_eq!(app.duplicate_groups.len(), 1);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_root_cannot_be_queued_for_deletion() {
        let dir = test_dir("root_delete");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = GuiApp::new(crate::scanner::scan(&dir, None).unwrap());
        app.select_path(Vec::new());
        app.request_delete_selected(false);
        assert!(app.pending_delete.is_none());
        assert!(dir.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
