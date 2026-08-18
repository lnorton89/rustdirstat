use crate::model::Node;
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
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::SizeDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size desc",
            SortMode::SizeAsc => "size asc",
            SortMode::NameAsc => "name asc",
            SortMode::NameDesc => "name desc",
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
    OpenInFileManager,
    Quit,
    ToggleHighlight(String),
    ClearHighlight,
    SelectRow(usize),
    NavigateTo(Vec<usize>),
    ConfirmDelete,
    CancelDelete,
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

pub struct App {
    pub root: Node,
    /// Indices (into each level's original, unsorted `children` vec) from
    /// the root down to the directory currently being browsed.
    pub path_indices: Vec<usize>,
    pub selected: usize,
    pub sort: SortMode,
    pub show_treemap: bool,
    pub pending_delete: Option<PathBuf>,
    pub message: Option<String>,
    pub ext_stats: Vec<ExtStat>,
    pub should_quit: bool,
    pub highlighted_category: Option<String>,
    /// Clickable regions from the most recent frame; consumed by mouse clicks.
    pub click_zones: Vec<ClickZone>,
    last_click: Option<(usize, Instant)>,
}

impl App {
    pub fn new(root: Node) -> Self {
        let mut app = Self {
            root,
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
        };
        app.refresh_ext_stats();
        app
    }

    pub fn current_node(&self) -> &Node {
        let mut node = &self.root;
        for &idx in &self.path_indices {
            node = &node.children[idx];
        }
        node
    }

    /// Children of the current directory, sorted for display, paired with
    /// their index in the original (unsorted) `children` vec so navigation
    /// and deletion stay stable regardless of sort order.
    pub fn display_children(&self) -> Vec<(usize, &Node)> {
        let node = self.current_node();
        let mut v: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
        match self.sort {
            SortMode::SizeDesc => v.sort_by(|a, b| b.1.size.cmp(&a.1.size)),
            SortMode::SizeAsc => v.sort_by(|a, b| a.1.size.cmp(&b.1.size)),
            SortMode::NameAsc => {
                v.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
            }
            SortMode::NameDesc => {
                v.sort_by(|a, b| b.1.name.to_lowercase().cmp(&a.1.name.to_lowercase()))
            }
        }
        v
    }

    fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node());
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Result<()> {
        if self.pending_delete.is_some() {
            let action = if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                Action::ConfirmDelete
            } else {
                Action::CancelDelete
            };
            return self.dispatch(action);
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
            KeyCode::Char('o') => Action::OpenInFileManager,
            KeyCode::Char('0') => Action::ClearHighlight,
            KeyCode::Char(c @ '1'..='9') => {
                let idx = c.to_digit(10).unwrap() as usize - 1;
                match self.ext_stats.get(idx) {
                    Some(stat) => Action::ToggleHighlight(stat.category.clone()),
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
        if let Some(zone) = self.click_zones.iter().rev().find(|z| z.contains(x, y)) {
            let action = zone.action.clone();
            self.dispatch(action)?;
        }
        Ok(())
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
                let len = self.display_children().len();
                if self.selected + 1 < len {
                    self.selected += 1;
                }
            }
            Action::OpenSelected => {
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
            Action::RequestDelete => {
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    self.pending_delete = Some(node.path.clone());
                }
            }
            Action::OpenInFileManager => {
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let target = if node.is_dir {
                        node.path.clone()
                    } else {
                        node.path
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| node.path.clone())
                    };
                    if let Err(e) = crate::util::open_in_file_manager(&target) {
                        self.message = Some(format!("Failed to open file manager: {e}"));
                    }
                }
            }
            Action::Quit => self.should_quit = true,
            Action::ToggleHighlight(cat) => {
                self.highlighted_category =
                    if self.highlighted_category.as_deref() == Some(cat.as_str()) {
                        None
                    } else {
                        Some(cat)
                    };
            }
            Action::ClearHighlight => self.highlighted_category = None,
            Action::SelectRow(idx) => {
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
            Action::ConfirmDelete | Action::CancelDelete => {}
        }
        Ok(())
    }

    /// Jump the browser to the item identified by `index_path` (as produced
    /// by the recursive treemap), landing on its parent directory with the
    /// item itself selected.
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
        let path = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let found = {
            let disp = self.display_children();
            disp.iter()
                .find(|(_, n)| n.path == path)
                .map(|(idx, n)| (*idx, n.is_dir, n.size, n.file_count))
        };

        if let Some((orig_idx, is_dir, size, count)) = found {
            if is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }

            let mut n = &mut self.root;
            n.size -= size;
            n.file_count -= count;
            for &idx in &self.path_indices {
                n = &mut n.children[idx];
                n.size -= size;
                n.file_count -= count;
            }
            n.children.remove(orig_idx);

            let len = self.display_children().len();
            if self.selected >= len {
                self.selected = len.saturating_sub(1);
            }
            self.refresh_ext_stats();
            self.message = Some(format!("Deleted {}", path.display()));
        } else {
            self.message = Some("Item no longer exists".to_string());
        }
        Ok(())
    }
}
