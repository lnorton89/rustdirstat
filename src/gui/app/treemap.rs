// ============================================================================
// Module:       gui::app::treemap
// Description:  The treemap's tile cache and the zoom deciding what it covers.
//
// Dependencies: super::{treemap_layout, GuiApp}
// ============================================================================

//! The treemap's tile cache and the zoom that decides what it covers.

use super::*;

/// Everything the treemap tile list depends on.
#[derive(PartialEq)]
pub(in crate::gui) struct TreemapKey {
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

impl GuiApp {
    pub(in crate::gui) fn zoom_node(&self) -> &Node {
        // Forgiving: the zoom level is display state, and a stale path
        // should still show the place it now points at.
        self.tree.deepest_valid_node(&self.zoom_path)
    }

    pub(in crate::gui) fn zoom_fs_path(&self) -> PathBuf {
        self.tree.deepest_valid_path(&self.zoom_path)
    }

    pub(in crate::gui) fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        if !index_path.is_empty() {
            self.select_path(index_path);
            self.file_view = FileView::AllFiles;
        }
    }

    pub(in crate::gui) fn zoom_in(&mut self) {
        let Some(path) = self.selected_path.clone() else {
            return;
        };
        let Some(node) = self.tree.node_for(&path) else {
            return;
        };
        self.zoom_path = if node.is_dir {
            path
        } else {
            path[..path.len().saturating_sub(1)].to_vec()
        };
        self.refresh_extensions();
    }

    pub(in crate::gui) fn zoom_out(&mut self) {
        if !self.zoom_path.is_empty() {
            self.zoom_path.pop();
            self.refresh_extensions();
        }
    }

    pub(in crate::gui) fn reset_zoom(&mut self) {
        self.zoom_path.clear();
        self.refresh_extensions();
    }

    /// Rebuilds [`Self::treemap_tiles`] if the panel rect or anything the
    /// layout depends on has changed since the last frame, and otherwise
    /// leaves the existing tiles in place.
    pub(in crate::gui) fn refresh_treemap(
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

        #[cfg(test)]
        crate::gui::ui::probes::TEST_TREEMAP_REBUILDS
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
}
