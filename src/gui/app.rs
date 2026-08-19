use crate::color::Category;
use crate::model::{Node, Tree};
use crate::stats::{self, ExtStat};
use crate::tui::SortMode;
use crate::util::human_bytes;
use eframe::egui;
use std::path::PathBuf;

use super::treemap_layout;

pub struct PendingDelete {
    pub orig_idx: usize,
    pub name: String,
    pub is_dir: bool,
    pub permanent: bool,
}

pub struct GuiApp {
    pub tree: Tree,
    /// Index path (into `Node::children`) from the tree root down to the
    /// directory currently being browsed — mirrors the TUI's
    /// `App::path_indices` exactly, including the same "no per-node path"
    /// rationale (see `model.rs`).
    pub path_indices: Vec<usize>,
    /// Original (unsorted) child index of the selected row, if any.
    pub selected: Option<usize>,
    pub sort: SortMode,
    pub use_physical: bool,
    pub ext_stats: Vec<ExtStat>,
    pub pending_delete: Option<PendingDelete>,
    pub status: Option<String>,
    pub highlighted_category: Option<Category>,
    pub show_properties: bool,
}

impl GuiApp {
    pub fn new(tree: Tree) -> Self {
        let mut app = Self {
            tree,
            path_indices: Vec::new(),
            selected: None,
            sort: SortMode::SizeDesc,
            use_physical: false,
            ext_stats: Vec::new(),
            pending_delete: None,
            status: None,
            highlighted_category: None,
            show_properties: false,
        };
        app.refresh_ext_stats();
        app
    }

    pub fn current_node(&self) -> &Node {
        self.tree.node_for(&self.path_indices)
    }

    pub fn current_path(&self) -> PathBuf {
        self.tree.path_for(&self.path_indices)
    }

    /// Children of the current directory, sorted for display, paired with
    /// their index in the original (unsorted) `children` vec so
    /// navigation/deletion/selection stay stable regardless of sort order
    /// — same contract as the TUI's `App::display_children`.
    pub fn display_children(&self) -> Vec<(usize, &Node)> {
        let node = self.current_node();
        let mut v: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
        match self.sort {
            SortMode::SizeDesc => v.sort_by(|a, b| {
                b.1.effective_size(self.use_physical)
                    .cmp(&a.1.effective_size(self.use_physical))
            }),
            SortMode::SizeAsc => v.sort_by(|a, b| {
                a.1.effective_size(self.use_physical)
                    .cmp(&b.1.effective_size(self.use_physical))
            }),
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

    pub fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node(), self.use_physical);
    }

    /// Navigates into the given original child index of the currently
    /// browsed directory (double-click / Enter on a directory row).
    pub fn open_child(&mut self, orig_idx: usize) {
        let node = self.current_node();
        if let Some(child) = node.children.get(orig_idx) {
            if child.is_dir {
                self.path_indices.push(orig_idx);
                self.selected = None;
                self.refresh_ext_stats();
            }
        }
    }

    /// Navigates to an absolute index path (treemap tile click, which can
    /// land arbitrarily deep in the subtree, not just a direct child).
    pub fn navigate_to_absolute(&mut self, mut index_path: Vec<usize>) {
        if index_path.is_empty() {
            return;
        }
        let target = index_path.remove(index_path.len() - 1);
        self.path_indices = index_path;
        self.selected = Some(target);
        self.refresh_ext_stats();
    }

    pub fn go_up(&mut self) {
        if let Some(last) = self.path_indices.pop() {
            self.selected = Some(last);
            self.refresh_ext_stats();
        }
    }

    pub fn refresh_scan(&mut self) -> anyhow::Result<()> {
        let tree = crate::scanner::scan(&self.tree.root_path, None)?;
        // Clamp the browse path in case the rescanned tree is shallower
        // (something along the path was deleted/moved outside the app).
        let mut valid = Vec::new();
        let mut node = &tree.root;
        for &idx in &self.path_indices {
            if idx >= node.children.len() {
                break;
            }
            valid.push(idx);
            node = &node.children[idx];
        }
        self.path_indices = valid;
        self.tree = tree;
        self.selected = None;
        self.refresh_ext_stats();
        Ok(())
    }

    pub fn request_delete(&mut self, orig_idx: usize, permanent: bool) {
        if let Some(node) = self.current_node().children.get(orig_idx) {
            self.pending_delete = Some(PendingDelete {
                orig_idx,
                name: node.name.clone(),
                is_dir: node.is_dir,
                permanent,
            });
        }
    }

    pub fn confirm_delete(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);
        let target = self.tree.node_for(&full_index_path);
        let is_dir = target.is_dir;

        if pending.permanent {
            if is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        } else {
            trash::delete(&path).map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }

        let verb = if pending.permanent {
            "Permanently deleted"
        } else {
            "Moved to trash"
        };
        self.status = Some(format!("{verb}: {}", path.display()));
        // Simplest correct approach for a first pass: rescan rather than
        // patching the in-memory tree's aggregates in place (the TUI does
        // the latter for instant feedback on huge trees, but that's an
        // optimization to layer in later, not a correctness requirement).
        self.refresh_scan()
    }

    /// Deletes a directory's contents (each direct child moved to trash)
    /// while keeping the directory itself — same distinction as the TUI's
    /// `d`-then-`e` "empty" action, only ever offered for a directory
    /// (`PendingDelete::is_dir`).
    pub fn confirm_empty(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        if !pending.is_dir {
            return Ok(());
        }
        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);

        let children: Vec<std::path::PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        for child_path in children {
            trash::delete(&child_path)
                .map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }
        self.status = Some(format!("Emptied: {}", path.display()));
        self.refresh_scan()
    }

    pub fn treemap_tiles(&self, x: f32, y: f32, w: f32, h: f32) -> Vec<treemap_layout::Tile> {
        let free_space = if self.path_indices.is_empty() && self.tree.is_volume_root() {
            self.tree.volume_free
        } else {
            None
        };
        treemap_layout::build(
            self.current_node(),
            x,
            y,
            w,
            h,
            self.use_physical,
            free_space,
        )
    }
}

pub fn size_label(bytes: u64, physical: bool) -> String {
    let suffix = if physical { " (physical)" } else { "" };
    format!("{}{}", human_bytes(bytes), suffix)
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        super::ui::draw(self, ctx);
    }
}
