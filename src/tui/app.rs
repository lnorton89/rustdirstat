use super::search::{self, SearchHit};
use super::top_files::{self, TopFile};
use crate::color::Category;
use crate::config::Config;
use crate::duplicates::DupGroup;
use crate::model::{Node, Tree};
use crate::stats::{self, ExtStat};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortMode {
    SizeDesc,
    SizeAsc,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::ModifiedDesc,
            SortMode::ModifiedDesc => SortMode::ModifiedAsc,
            SortMode::ModifiedAsc => SortMode::SizeDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size desc",
            SortMode::SizeAsc => "size asc",
            SortMode::NameAsc => "name asc",
            SortMode::NameDesc => "name desc",
            SortMode::ModifiedDesc => "newest first",
            SortMode::ModifiedAsc => "oldest first",
        }
    }
}

/// Every user-triggerable operation, so keyboard and mouse input can share
/// one code path instead of duplicating behavior.
#[derive(Clone)]
pub enum Action {
    Up,
    Down,
    OpenSelected,
    Back,
    CycleSort,
    ToggleTreemap,
    RequestDelete,
    RequestDeletePermanent,
    OpenInFileManager,
    Quit,
    ToggleHighlight(Category),
    ClearHighlight,
    SelectRow(usize),
    NavigateTo(Vec<usize>),
    ConfirmDelete,
    CancelDelete,
    Refresh,
    ToggleTopFiles,
    ToggleHelp,
    ExportReport,
    StartFilter,
    StartSubtreeSearch,
    ToggleDetails,
    GrowTreemap,
    ShrinkTreemap,
    StartResize,
    TogglePhysicalSize,
    ToggleDuplicates,
}

/// One row of the flattened duplicate-groups view: either a group header
/// (not itself navigable — just a size/count label) or a member file
/// (navigable, like a search hit).
pub enum DupRow {
    Header { size: u64, count: usize },
    Member { index_path: Vec<usize> },
}

/// A screen region registered during the last draw that maps a mouse click
/// to an `Action`. Rebuilt every frame in `ui::draw`.
#[derive(Clone)]
pub struct ClickZone {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub action: Action,
}

impl ClickZone {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

pub struct PendingDelete {
    pub orig_idx: usize,
    pub name: String,
    pub permanent: bool,
}

pub struct App {
    pub tree: Tree,
    /// Indices (into each level's original, unsorted `children` vec) from
    /// the root down to the directory currently being browsed.
    pub path_indices: Vec<usize>,
    pub selected: usize,
    pub sort: SortMode,
    pub show_treemap: bool,
    pub pending_delete: Option<PendingDelete>,
    pub message: Option<String>,
    pub ext_stats: Vec<ExtStat>,
    pub should_quit: bool,
    pub highlighted_category: Option<Category>,
    /// Clickable regions from the most recent frame; consumed by mouse clicks.
    pub click_zones: Vec<ClickZone>,
    last_click: Option<(usize, Instant)>,
    pub filter: String,
    pub filter_mode: bool,
    pub show_top_files: bool,
    pub top_files_cache: Vec<TopFile>,
    /// Recursive subtree search (distinct from `filter`, which only
    /// narrows the current directory's direct children): `search_mode` is
    /// the query text-entry state, `show_search` is the results view.
    pub search_query: String,
    pub search_mode: bool,
    pub show_search: bool,
    pub search_results: Vec<SearchHit>,
    pub search_truncated: bool,
    pub search_error: Option<String>,
    pub show_help: bool,
    pub refresh_requested: bool,
    /// Show file/dir counts and modified dates in the list — off by
    /// default to keep each row to the essentials (bar, size, name).
    pub detailed: bool,
    /// Size the list/treemap/header show and use for proportions: physical
    /// (on-disk, allocated) when true, logical (apparent) when false.
    pub use_physical: bool,
    /// Width of the treemap panel as a percentage of the body area.
    pub treemap_split: u16,
    /// Set while the divider between the list and treemap panels is being
    /// mouse-dragged; `body_x`/`body_width` (recorded each frame) let a
    /// drag position translate into a split percentage.
    pub resizing_treemap: bool,
    body_x: u16,
    body_width: u16,
    /// Set by `Action::ToggleDuplicates` and consumed by the browse loop in
    /// `tui::mod`, which runs the actual (background-threaded) scan and
    /// its own progress screen — hashing file content can take a while on
    /// a large tree, which doesn't fit the instant request/dispatch model
    /// every other action uses.
    pub duplicate_scan_requested: bool,
    pub show_duplicates: bool,
    pub duplicate_rows: Vec<DupRow>,
    pub duplicate_group_count: usize,
    pub duplicate_total_wasted: u64,
    pub duplicate_truncated: bool,
}

const TREEMAP_SPLIT_MIN: u16 = 20;
const TREEMAP_SPLIT_MAX: u16 = 75;
const TREEMAP_SPLIT_STEP: u16 = 5;

const TOP_FILES_LIMIT: usize = 500;

impl App {
    pub fn new(tree: Tree) -> Self {
        let mut app = Self {
            tree,
            path_indices: vec![],
            selected: 0,
            sort: SortMode::SizeDesc,
            show_treemap: true,
            pending_delete: None,
            message: None,
            ext_stats: vec![],
            should_quit: false,
            highlighted_category: None,
            click_zones: vec![],
            last_click: None,
            filter: String::new(),
            filter_mode: false,
            show_top_files: false,
            top_files_cache: vec![],
            search_query: String::new(),
            search_mode: false,
            show_search: false,
            search_results: vec![],
            search_truncated: false,
            search_error: None,
            show_help: false,
            refresh_requested: false,
            detailed: false,
            use_physical: false,
            treemap_split: 45,
            resizing_treemap: false,
            body_x: 0,
            body_width: 0,
            duplicate_scan_requested: false,
            show_duplicates: false,
            duplicate_rows: vec![],
            duplicate_group_count: 0,
            duplicate_total_wasted: 0,
            duplicate_truncated: false,
        };
        app.refresh_ext_stats();
        app
    }

    /// Applies persisted preferences loaded at startup. Every field is
    /// optional, so a first run (no config file yet) or a config missing a
    /// newer field just leaves that setting at its built-in default.
    pub fn apply_config(&mut self, cfg: &Config) {
        if let Some(sort) = cfg.sort {
            self.sort = sort;
        }
        if let Some(show_treemap) = cfg.show_treemap {
            self.show_treemap = show_treemap;
        }
        if let Some(split) = cfg.treemap_split {
            self.treemap_split = split.clamp(TREEMAP_SPLIT_MIN, TREEMAP_SPLIT_MAX);
        }
        if let Some(detailed) = cfg.detailed {
            self.detailed = detailed;
        }
        if let Some(use_physical) = cfg.use_physical {
            self.use_physical = use_physical;
        }
    }

    /// Snapshots the preferences worth persisting across runs. Deliberately
    /// excludes anything tied to this specific scan (browse location,
    /// filters, search state, highlighted category) — those are session
    /// state, not preferences, and restoring them on an unrelated future
    /// scan would be surprising rather than helpful.
    pub fn to_config(&self) -> Config {
        Config {
            sort: Some(self.sort),
            show_treemap: Some(self.show_treemap),
            treemap_split: Some(self.treemap_split),
            detailed: Some(self.detailed),
            use_physical: Some(self.use_physical),
        }
    }

    /// Restore browsing to whatever directory `target` was pointing at
    /// before a rescan, matching by path since node indices aren't stable
    /// across scans. Falls back to the root if it can't be found (e.g. the
    /// directory was deleted).
    pub fn restore_path(&mut self, target: &std::path::Path) {
        let mut node = &self.tree.root;
        let mut indices = Vec::new();
        let mut current = self.tree.root_path.clone();
        if current == target {
            self.path_indices = indices;
            self.selected = 0;
            self.refresh_ext_stats();
            return;
        }
        loop {
            let mut found = None;
            for (i, c) in node.children.iter().enumerate() {
                let candidate = current.join(&c.name);
                if target == candidate || target.starts_with(&candidate) {
                    found = Some((i, c, candidate));
                    break;
                }
            }
            match found {
                Some((i, c, candidate)) => {
                    indices.push(i);
                    node = c;
                    current = candidate;
                    if current == target {
                        break;
                    }
                }
                None => break,
            }
        }
        self.path_indices = indices;
        self.selected = 0;
        self.refresh_ext_stats();
    }

    pub fn current_node(&self) -> &Node {
        self.tree.node_for(&self.path_indices)
    }

    pub fn current_path(&self) -> PathBuf {
        self.tree.path_for(&self.path_indices)
    }

    /// Children of the current directory, filtered by the active search
    /// term and sorted for display, paired with their index in the
    /// original (unsorted) `children` vec so navigation and deletion stay
    /// stable regardless of sort/filter.
    pub fn display_children(&self) -> Vec<(usize, &Node)> {
        let node = self.current_node();
        let mut v: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            v.retain(|(_, n)| n.name.to_lowercase().contains(&f));
        }
        match self.sort {
            SortMode::SizeDesc => v.sort_by(|a, b| b.1.size.cmp(&a.1.size)),
            SortMode::SizeAsc => v.sort_by(|a, b| a.1.size.cmp(&b.1.size)),
            SortMode::NameAsc => {
                v.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
            }
            SortMode::NameDesc => {
                v.sort_by(|a, b| b.1.name.to_lowercase().cmp(&a.1.name.to_lowercase()))
            }
            SortMode::ModifiedDesc => v.sort_by(|a, b| b.1.modified.cmp(&a.1.modified)),
            SortMode::ModifiedAsc => v.sort_by(|a, b| a.1.modified.cmp(&b.1.modified)),
        }
        v
    }

    fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node());
    }

    fn refresh_top_files(&mut self) {
        let mut out = top_files::top_k(self.current_node(), TOP_FILES_LIMIT);
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            out.retain(|t| t.name.to_lowercase().contains(&f));
        }
        self.top_files_cache = out;
    }

    fn on_filter_changed(&mut self) {
        self.selected = 0;
        if self.show_top_files {
            self.refresh_top_files();
        }
    }

    /// If the "biggest files" or search-results flat view is active, jump
    /// browsing to the currently selected entry's actual parent directory
    /// (and select it there), then leave the flat view — so every action
    /// that operates on "the selected row" (delete, open, Enter) works the
    /// same regardless of which view found that row.
    fn exit_flat_view_if_needed(&mut self) {
        if self.show_top_files {
            if let Some(tf) = self.top_files_cache.get(self.selected) {
                let idx_path = tf.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.show_top_files = false;
        } else if self.show_search {
            if let Some(hit) = self.search_results.get(self.selected) {
                let idx_path = hit.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.show_search = false;
        } else if self.show_duplicates {
            if let Some(DupRow::Member { index_path }) = self.duplicate_rows.get(self.selected) {
                let idx_path = index_path.clone();
                self.navigate_to_absolute(idx_path);
            }
            self.show_duplicates = false;
        }
    }

    /// Caps how many duplicate groups are turned into list rows — the
    /// underlying scan can find far more than is sensible to hand to
    /// ratatui's list widget every frame; the most-impactful groups (by
    /// wasted space) are already sorted first, so this only ever drops the
    /// long tail of smaller groups.
    const MAX_DUPLICATE_DISPLAY_GROUPS: usize = 500;

    pub fn set_duplicate_results(&mut self, groups: Vec<DupGroup>) {
        self.duplicate_group_count = groups.len();
        self.duplicate_truncated = groups.len() > Self::MAX_DUPLICATE_DISPLAY_GROUPS;
        self.duplicate_total_wasted = groups
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
        self.duplicate_rows = rows;
        self.show_duplicates = true;
        self.show_search = false;
        self.show_top_files = false;
        self.selected = 0;
    }

    fn run_subtree_search(&mut self) {
        let outcome = search::search(self.current_node(), &self.search_query);
        self.search_error = outcome.error;
        self.search_truncated = outcome.truncated;
        self.search_results = outcome.hits;
        self.show_search = true;
        self.search_mode = false;
        self.selected = 0;
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Result<()> {
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        if self.pending_delete.is_some() {
            let action = if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                Action::ConfirmDelete
            } else {
                Action::CancelDelete
            };
            return self.dispatch(action);
        }
        if self.filter_mode {
            match code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filter_mode = false;
                    self.on_filter_changed();
                }
                KeyCode::Enter => self.filter_mode = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.on_filter_changed();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.on_filter_changed();
                }
                _ => {}
            }
            return Ok(());
        }
        if self.search_mode {
            match code {
                KeyCode::Esc => self.search_mode = false,
                KeyCode::Enter => self.run_subtree_search(),
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(c) => self.search_query.push(c),
                _ => {}
            }
            return Ok(());
        }

        let action = match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Action::OpenSelected,
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => Action::Back,
            KeyCode::Char('s') => Action::CycleSort,
            KeyCode::Char('t') => Action::ToggleTreemap,
            KeyCode::Char('[') => Action::ShrinkTreemap,
            KeyCode::Char(']') => Action::GrowTreemap,
            KeyCode::Char('d') => Action::RequestDelete,
            KeyCode::Char('D') => Action::RequestDeletePermanent,
            KeyCode::Char('o') => Action::OpenInFileManager,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('f') => Action::ToggleTopFiles,
            KeyCode::Char('e') => Action::ExportReport,
            KeyCode::Char('m') => Action::ToggleDetails,
            KeyCode::Char('p') => Action::TogglePhysicalSize,
            KeyCode::Char('?') => Action::ToggleHelp,
            KeyCode::Char('/') => Action::StartFilter,
            KeyCode::Char('S') => Action::StartSubtreeSearch,
            KeyCode::Char('u') => Action::ToggleDuplicates,
            KeyCode::Char('0') => Action::ClearHighlight,
            KeyCode::Char(c @ '1'..='9') => {
                let idx = c.to_digit(10).unwrap() as usize - 1;
                match self.ext_stats.get(idx) {
                    Some(stat) => Action::ToggleHighlight(stat.category),
                    None => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        self.dispatch(action)
    }

    /// Look up whatever click zone (if any) contains `(x, y)` — the most
    /// recently drawn zone wins, so popups drawn last take priority.
    pub fn handle_click(&mut self, x: u16, y: u16) -> Result<()> {
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        if let Some(zone) = self.click_zones.iter().rev().find(|z| z.contains(x, y)) {
            let action = zone.action.clone();
            self.dispatch(action)?;
        }
        Ok(())
    }

    /// Recorded every frame by `ui::draw` so a mouse drag position can be
    /// translated into a split percentage.
    pub fn set_body_area(&mut self, x: u16, width: u16) {
        self.body_x = x;
        self.body_width = width;
    }

    /// Called on `MouseEventKind::Drag` while `resizing_treemap` is set.
    pub fn handle_drag(&mut self, x: u16) {
        if !self.resizing_treemap || self.body_width == 0 {
            return;
        }
        let list_w = x.saturating_sub(self.body_x).min(self.body_width);
        let list_pct = (u32::from(list_w) * 100 / u32::from(self.body_width)) as u16;
        let treemap_pct = 100u16.saturating_sub(list_pct);
        self.treemap_split = treemap_pct.clamp(TREEMAP_SPLIT_MIN, TREEMAP_SPLIT_MAX);
    }

    pub fn end_drag(&mut self) {
        self.resizing_treemap = false;
    }

    fn request_delete(&mut self, permanent: bool) {
        self.exit_flat_view_if_needed();
        if let Some((idx, node)) = self.display_children().get(self.selected) {
            self.pending_delete = Some(PendingDelete {
                orig_idx: *idx,
                name: node.name.clone(),
                permanent,
            });
        }
    }

    pub fn dispatch(&mut self, action: Action) -> Result<()> {
        if self.pending_delete.is_some() {
            match action {
                Action::ConfirmDelete => self.confirm_delete()?,
                _ => self.pending_delete = None,
            }
            return Ok(());
        }

        self.message = None;
        match action {
            Action::Up => self.selected = self.selected.saturating_sub(1),
            Action::Down => {
                let len = if self.show_top_files {
                    self.top_files_cache.len()
                } else if self.show_search {
                    self.search_results.len()
                } else if self.show_duplicates {
                    self.duplicate_rows.len()
                } else {
                    self.display_children().len()
                };
                if self.selected + 1 < len {
                    self.selected += 1;
                }
            }
            Action::OpenSelected => {
                self.exit_flat_view_if_needed();
                let target = self
                    .display_children()
                    .get(self.selected)
                    .map(|(idx, n)| (*idx, n.is_dir));
                if let Some((idx, is_dir)) = target {
                    if is_dir {
                        self.path_indices.push(idx);
                        self.selected = 0;
                        self.refresh_ext_stats();
                    }
                }
            }
            Action::Back => {
                if !self.path_indices.is_empty() {
                    self.path_indices.pop();
                    self.selected = 0;
                    self.refresh_ext_stats();
                }
            }
            Action::CycleSort => self.sort = self.sort.next(),
            Action::ToggleTreemap => self.show_treemap = !self.show_treemap,
            Action::ShrinkTreemap => {
                self.treemap_split = self
                    .treemap_split
                    .saturating_sub(TREEMAP_SPLIT_STEP)
                    .max(TREEMAP_SPLIT_MIN);
            }
            Action::GrowTreemap => {
                self.treemap_split =
                    (self.treemap_split + TREEMAP_SPLIT_STEP).min(TREEMAP_SPLIT_MAX);
            }
            Action::StartResize => self.resizing_treemap = true,
            Action::RequestDelete => self.request_delete(false),
            Action::RequestDeletePermanent => self.request_delete(true),
            Action::OpenInFileManager => {
                self.exit_flat_view_if_needed();
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let mut target = self.current_path();
                    target.push(&node.name);
                    let target = if node.is_dir {
                        target
                    } else {
                        target.parent().map(|p| p.to_path_buf()).unwrap_or(target)
                    };
                    if let Err(e) = crate::util::open_in_file_manager(&target) {
                        self.message = Some(format!("Failed to open file manager: {e}"));
                    }
                }
            }
            Action::Quit => self.should_quit = true,
            Action::ToggleHighlight(cat) => {
                self.highlighted_category = if self.highlighted_category == Some(cat) {
                    None
                } else {
                    Some(cat)
                };
            }
            Action::ClearHighlight => self.highlighted_category = None,
            Action::SelectRow(idx) => {
                if self.show_top_files {
                    if let Some(tf) = self.top_files_cache.get(idx) {
                        let idx_path = tf.index_path.clone();
                        self.navigate_to(idx_path);
                    }
                    self.show_top_files = false;
                    return Ok(());
                }
                if self.show_search {
                    if let Some(hit) = self.search_results.get(idx) {
                        let idx_path = hit.index_path.clone();
                        self.navigate_to(idx_path);
                    }
                    self.show_search = false;
                    return Ok(());
                }
                if self.show_duplicates {
                    match self.duplicate_rows.get(idx) {
                        Some(DupRow::Member { index_path }) => {
                            let idx_path = index_path.clone();
                            self.navigate_to_absolute(idx_path);
                            self.show_duplicates = false;
                        }
                        Some(DupRow::Header { .. }) => self.selected = idx,
                        None => {}
                    }
                    return Ok(());
                }
                let now = Instant::now();
                let is_double_click = matches!(
                    self.last_click,
                    Some((last_idx, t)) if last_idx == idx && now.duration_since(t) < Duration::from_millis(450)
                );
                let len = self.display_children().len();
                if idx < len {
                    self.selected = idx;
                }
                if is_double_click {
                    self.last_click = None;
                    self.dispatch(Action::OpenSelected)?;
                } else {
                    self.last_click = Some((idx, now));
                }
            }
            Action::NavigateTo(path) => self.navigate_to(path),
            Action::Refresh => self.refresh_requested = true,
            Action::ToggleTopFiles => {
                self.show_search = false;
                self.show_duplicates = false;
                self.show_top_files = !self.show_top_files;
                self.selected = 0;
                if self.show_top_files {
                    self.refresh_top_files();
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::ExportReport => self.export_report(),
            Action::StartFilter => {
                self.filter_mode = true;
                self.filter.clear();
                self.selected = 0;
            }
            Action::StartSubtreeSearch => {
                if self.show_search {
                    self.show_search = false;
                } else {
                    self.show_top_files = false;
                    self.show_duplicates = false;
                    self.search_mode = true;
                    self.search_query.clear();
                    self.selected = 0;
                }
            }
            Action::ToggleDetails => self.detailed = !self.detailed,
            Action::TogglePhysicalSize => self.use_physical = !self.use_physical,
            Action::ToggleDuplicates => {
                if self.show_duplicates {
                    self.show_duplicates = false;
                } else {
                    self.show_search = false;
                    self.show_top_files = false;
                    self.duplicate_scan_requested = true;
                }
            }
            Action::ConfirmDelete | Action::CancelDelete => {}
        }
        Ok(())
    }

    fn export_report(&mut self) {
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

    /// Jump the browser to the item identified by `index_path` (as produced
    /// by the recursive treemap or the biggest-files view), landing on its
    /// parent directory with the item itself selected.
    fn navigate_to(&mut self, mut index_path: Vec<usize>) {
        if index_path.is_empty() {
            return;
        }
        let target = index_path.remove(index_path.len() - 1);
        self.path_indices.extend(index_path);
        self.selected = 0;
        self.refresh_ext_stats();
        if let Some(pos) = self
            .display_children()
            .iter()
            .position(|(idx, _)| *idx == target)
        {
            self.selected = pos;
        }
    }

    /// Like `navigate_to`, but for an `index_path` rooted at the whole
    /// tree rather than the currently browsed directory — needed for
    /// duplicate results, which are found by scanning from `tree.root`,
    /// not from `current_node()` the way search/top-files results are.
    fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        self.path_indices.clear();
        self.navigate_to(index_path);
    }

    fn confirm_delete(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);
        let target = self.tree.node_for(&full_index_path);

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
        n.children.remove(pending.orig_idx);

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
}

enum RemovedExt {
    File(Option<Category>),
    Dir(Vec<(u64, u64)>),
}

fn subtract_totals(
    n: &mut Node,
    size: u64,
    physical_size: u64,
    file_count: u64,
    dir_count: u64,
    unreadable_count: u64,
    ext: &RemovedExt,
) {
    n.size -= size;
    n.physical_size -= physical_size;
    n.file_count -= file_count;
    n.dir_count -= dir_count;
    n.unreadable_count -= unreadable_count;
    match ext {
        RemovedExt::File(Some(cat)) => {
            let i = cat.index();
            n.ext_totals[i].0 -= size;
            n.ext_totals[i].1 -= 1;
        }
        RemovedExt::File(None) => {}
        RemovedExt::Dir(totals) => {
            for (i, &(s, c)) in totals.iter().enumerate() {
                n.ext_totals[i].0 -= s;
                n.ext_totals[i].1 -= c;
            }
        }
    }
}
