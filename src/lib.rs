// ============================================================================
// Module:       rustdirstat (library crate root)
// Description:  Module graph for the shared scanning core and the two front
//               ends built on top of it.
//
// Dependencies: None; declarations only. See the individual modules.
// ============================================================================

//! A WinDirStat clone: one scanning core with two front ends over it.
//!
//! [`scanner`] walks a directory tree into the [`model::Tree`] that every
//! other module reads, and [`treemap`] turns a list of sizes into
//! rectangles. Neither knows which front end is asking. [`tui`] renders
//! through ratatui, [`gui`] through egui/eframe, and the two share
//! [`color`], [`stats`], [`util`], [`search`], [`top_files`], and the
//! layout maths rather than each carrying a copy — a file has to be the
//! same colour in both, and duplicated colour tables were how it stopped
//! being.
//!
//! See `docs/ARCHITECTURE.md` for the module map, and `docs/PERFORMANCE.md`
//! before touching anything on a per-frame path.

#[cfg(test)]
mod header_check;

pub mod brand;
pub mod cleanups;
pub mod color;
pub mod config;
pub mod csv_export;
pub mod duplicates;
pub mod gui;
pub mod i18n;
pub mod model;
pub mod platform;
pub mod report;
pub mod scanner;
pub mod search;
pub mod stats;
pub mod top_files;
pub mod treemap;
pub mod tui;
pub mod util;
pub mod wintools;
