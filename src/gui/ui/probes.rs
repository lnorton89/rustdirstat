//! Test-only geometry probes.
//!
//! An immediate-mode UI has no retained widget tree to query after
//! the fact, so the drawing code records where it actually put
//! things into the statics below and the tests assert against
//! that. This is what lets a test click a real rendered row rather
//! than a coordinate someone guessed.

use crate::gui::app::{DirectoryColumn, ExtensionColumn, FileView};
use crate::gui::icons::Icon;
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

/// `(content width, visible width)` of the directory table's horizontal
/// scroll area. Content wider than the viewport is precisely the
/// condition that puts a scrollbar there, and it is not something the
/// painted row rects reveal — they sit at the table's minimum width
/// whether that width is reachable or merely clipped off the edge.
#[cfg(test)]
pub(super) static TEST_DIRECTORY_SCROLL: std::sync::Mutex<Vec<(f32, f32)>> =
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
