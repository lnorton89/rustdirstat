use crate::model::Node;
use crate::stats::{self, ExtStat};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::PathBuf;

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
            SortMode::NameAsc => v.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())),
            SortMode::NameDesc => v.sort_by(|a, b| b.1.name.to_lowercase().cmp(&a.1.name.to_lowercase())),
        }
        v
    }

    fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node());
    }

    pub fn handle_key(&mut self, code: KeyCode) -> Result<()> {
        if self.pending_delete.is_some() {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.confirm_delete()?,
                _ => self.pending_delete = None,
            }
            return Ok(());
        }

        self.message = None;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = self.display_children().len();
                if self.selected + 1 < len {
                    self.selected += 1;
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                let target = self.display_children().get(self.selected).map(|(idx, n)| (*idx, n.is_dir));
                if let Some((idx, is_dir)) = target {
                    if is_dir {
                        self.path_indices.push(idx);
                        self.selected = 0;
                        self.refresh_ext_stats();
                    }
                }
            }
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                if !self.path_indices.is_empty() {
                    self.path_indices.pop();
                    self.selected = 0;
                    self.refresh_ext_stats();
                }
            }
            KeyCode::Char('s') => self.sort = self.sort.next(),
            KeyCode::Char('t') => self.show_treemap = !self.show_treemap,
            KeyCode::Char('d') => {
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    self.pending_delete = Some(node.path.clone());
                }
            }
            KeyCode::Char('o') => {
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let target = if node.is_dir {
                        node.path.clone()
                    } else {
                        node.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| node.path.clone())
                    };
                    if let Err(e) = crate::util::open_in_file_manager(&target) {
                        self.message = Some(format!("Failed to open file manager: {e}"));
                    }
                }
            }
            _ => {}
        }
        Ok(())
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
