// ============================================================================
// Module:       gui::ui::probes
// Description:  Test-only geometry probes: the rects the drawing code actually
//               painted, recorded so tests can click real coordinates.
//
// Dependencies: eframe::egui, std::sync::Mutex (the probe statics are global)
// ============================================================================

//! Test-only geometry probes.
//!
//! An immediate-mode UI has no retained widget tree to query after
//! the fact, so the drawing code records where it actually put
//! things into the statics below and the tests assert against
//! that. This is what lets a test click a real rendered row rather
//! than a coordinate someone guessed.

use crate::gui::app::{DirectoryColumn, ExtensionColumn, FileView};
use crate::gui::icons::Icon;
use crate::gui::ui::modal::ModalPage;
use eframe::egui::{self};
use std::sync::{Mutex, MutexGuard};

/// Takes a probe lock, recovering from poisoning rather than propagating
/// it.
///
/// These statics are written from inside the drawing code, so a test that
/// fails part-way through a frame leaves its probe poisoned. Propagating
/// that would turn one real failure into a cascade of unrelated ones in
/// every later test that touches the same probe, which buries the actual
/// result. Recorded geometry from an aborted frame is stale, not unsound
/// — every test clears the probe before rendering anyway.
pub(super) fn probe<T>(cell: &'static Mutex<Vec<T>>) -> MutexGuard<'static, Vec<T>> {
    cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(super) static TEST_MENU_BAR_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

/// Corner radius of the hover background egui would paint under a
/// top-level menu name. The app's global widget rounding is 6, and a
/// menu bar has to override it — a rounded pill under a menu name reads
/// as a floating button rather than as part of the bar.
#[cfg(test)]
pub(super) static TEST_MENU_BAR_ROUNDING: std::sync::Mutex<Vec<(String, u8)>> =
    std::sync::Mutex::new(Vec::new());

/// `(label, x of the first glyph)` for every table column header, of
/// either kind. Two widgets paint these and they have to agree.
#[cfg(test)]
pub(super) static TEST_TABLE_HEADER_TEXT: std::sync::Mutex<Vec<(String, f32)>> =
    std::sync::Mutex::new(Vec::new());

/// The rule under each pane's heading. What the test wants from it is not
/// where it is but that the air above and below it is the same in every
/// pane, which is only visible as geometry.
#[cfg(test)]
pub(super) static TEST_SECTION_RULE_RECTS: std::sync::Mutex<Vec<egui::Rect>> =
    std::sync::Mutex::new(Vec::new());

/// `(index path, is_dir, icon rect)` for the folder or file glyph in a
/// tree row's name cell. A file leaves a gap where a folder's expand
/// toggle would be, and the two have to come out the same width — which
/// nothing but the painted geometry can show.
#[cfg(test)]
pub(super) static TEST_TREE_NAME_ICONS: std::sync::Mutex<Vec<(Vec<usize>, bool, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

/// How far the tree's expand chevron is through its quarter turn, per
/// row. A quarter turn is what distinguishes an expanded row from a
/// collapsed one now that both draw the same glyph.
#[cfg(test)]
pub(super) static TEST_CHEVRON_TURNS: std::sync::Mutex<Vec<f32>> =
    std::sync::Mutex::new(Vec::new());

/// `(index path, rect)` of the treemap tile under the pointer. Nothing
/// else records it: the treemap is one big painter with no widget per
/// tile, so hover is resolved by hit-testing rather than by egui.
#[cfg(test)]
pub(super) static TEST_TREEMAP_HOVER: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

/// `(content width, visible width)` of the directory table's horizontal
/// scroll area. Content wider than the viewport is precisely the
/// condition that puts a scrollbar there, and it is not something the
/// painted row rects reveal — they sit at the table's minimum width
/// whether that width is reachable or merely clipped off the edge.
#[cfg(test)]
pub(super) static TEST_DIRECTORY_SCROLL: std::sync::Mutex<Vec<(f32, f32)>> =
    std::sync::Mutex::new(Vec::new());

/// The extension table's (content width, viewport width), for the same
/// reason as the directory table's above: whether a column past the pane
/// edge can be *reached* is not visible in the row rects.
#[cfg(test)]
pub(super) static TEST_EXTENSION_SCROLL: std::sync::Mutex<Vec<(f32, f32)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_DIRECTORY_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_DIRECTORY_CELL_COLUMNS: std::sync::Mutex<
    Vec<(Vec<usize>, DirectoryColumn)>,
> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_DIRECTORY_HEADER_RECTS: std::sync::Mutex<Vec<(&'static str, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_DIRECTORY_HEADER_ICONS: std::sync::Mutex<Vec<(&'static str, Option<Icon>)>> =
    std::sync::Mutex::new(Vec::new());

/// What one rendered menu row actually occupied on screen, so the layout
/// tests can assert against real geometry instead of trusting the widget
/// code to agree with itself.
#[cfg(test)]
pub(super) struct MenuItemLayout {
    pub(super) label: String,
    pub(super) row: egui::Rect,
    pub(super) icon: egui::Rect,
    pub(super) text: egui::Rect,
    pub(super) shortcut: Option<egui::Rect>,
}

#[cfg(test)]
pub(super) static TEST_MENU_ITEM_LAYOUTS: std::sync::Mutex<Vec<MenuItemLayout>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_ICON_MENU_LAYOUTS: std::sync::Mutex<
    Vec<(String, egui::Rect, egui::Rect, egui::Rect)>,
> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_EXTENSION_ROW_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_EXTENSION_CELL_COLUMNS: std::sync::Mutex<Vec<(String, ExtensionColumn)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_EXTENSION_TEXT_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_EXTENSION_HEADER_RECTS: std::sync::Mutex<Vec<(&'static str, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_EXTENSION_HEADER_ICONS: std::sync::Mutex<Vec<(&'static str, Option<Icon>)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_LARGEST_ROW_RECTS: std::sync::Mutex<Vec<(usize, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_SEARCH_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_DUPLICATE_ROW_RECTS: std::sync::Mutex<Vec<(Vec<usize>, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_VIEW_TAB_RECTS: std::sync::Mutex<Vec<(FileView, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

/// The full-window scrim behind an open modal. Its presence is what
/// proves the modal is actually modal rather than merely on top.
#[cfg(test)]
pub(super) static TEST_MODAL_SCRIM_RECTS: std::sync::Mutex<Vec<egui::Rect>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_MODAL_CARD_RECTS: std::sync::Mutex<Vec<egui::Rect>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_MODAL_NAV_RECTS: std::sync::Mutex<Vec<(ModalPage, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) static TEST_THEME_ROW_RECTS: std::sync::Mutex<Vec<(String, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());

/// `(tool index, destructive, row rect)` for each maintenance row, so a
/// test can check that severity is actually painted rather than merely
/// stored on the tool.
#[cfg(test)]
pub(super) static TEST_TOOL_ROW_MARKERS: std::sync::Mutex<Vec<(usize, bool, egui::Rect)>> =
    std::sync::Mutex::new(Vec::new());
