use super::top_files::{self, TopFile};
use crate::color::Category;
use crate::model::{Node, Tree};
use crate::stats::{self, ExtStat};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
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
    StartSearch,
    ToggleDetails,
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
    pub show_help: bool,
    pub refresh_requested: bool,
    /// Show file/dir counts and modified dates in the list — off by
    /// default to keep each row to the essentials (bar, size, name).
    pub detailed: bool,
}

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
            show_help: false,
            refresh_requested: false,
            detailed: false,
        };
        app.refresh_ext_stats();
        app
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

    /// If the "biggest files" flat view is active, jump browsing to the
    /// currently selected file's actual parent directory (and select it
    /// there), then leave the flat view — so every action that operates on
    /// "the selected row" (delete, open, Enter) works the same regardless
    /// of which view found that row.
    fn exit_top_files_if_needed(&mut self) {
        if !self.show_top_files {
            return;
        }
        if let Some(tf) = self.top_files_cache.get(self.selected) {
            let idx_path = tf.index_path.clone();
            self.navigate_to(idx_path);
        }
        self.show_top_files = false;
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

        let action = match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Action::OpenSelected,
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => Action::Back,
            KeyCode::Char('s') => Action::CycleSort,
            KeyCode::Char('t') => Action::ToggleTreemap,
            KeyCode::Char('d') => Action::RequestDelete,
            KeyCode::Char('D') => Action::RequestDeletePermanent,
            KeyCode::Char('o') => Action::OpenInFileManager,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('f') => Action::ToggleTopFiles,
            KeyCode::Char('e') => Action::ExportReport,
            KeyCode::Char('m') => Action::ToggleDetails,
            KeyCode::Char('?') => Action::ToggleHelp,
            KeyCode::Char('/') => Action::StartSearch,
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

    fn request_delete(&mut self, permanent: bool) {
        self.exit_top_files_if_needed();
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
                } else {
                    self.display_children().len()
                };
                if self.selected + 1 < len {
                    self.selected += 1;
                }
            }
            Action::OpenSelected => {
                self.exit_top_files_if_needed();
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
            Action::RequestDelete => self.request_delete(false),
            Action::RequestDeletePermanent => self.request_delete(true),
            Action::OpenInFileManager => {
                self.exit_top_files_if_needed();
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
                self.show_top_files = !self.show_top_files;
                self.selected = 0;
                if self.show_top_files {
                    self.refresh_top_files();
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::ExportReport => self.export_report(),
            Action::StartSearch => {
                self.filter_mode = true;
                self.filter.clear();
                self.selected = 0;
            }
            Action::ToggleDetails => self.detailed = !self.detailed,
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

    fn confirm_delete(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);
        let target = self.tree.node_for(&full_index_path);

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
        subtract_totals(n, size, file_count, dir_count_delta, &removed_ext);
        for &idx in &self.path_indices {
            n = &mut n.children[idx];
            subtract_totals(n, size, file_count, dir_count_delta, &removed_ext);
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

fn subtract_totals(n: &mut Node, size: u64, file_count: u64, dir_count: u64, ext: &RemovedExt) {
    n.size -= size;
    n.file_count -= file_count;
    n.dir_count -= dir_count;
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
