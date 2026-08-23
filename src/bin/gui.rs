// ============================================================================
// Module:       rustdirstat-gui (binary crate root)
// Description:  Command-line entry point for the desktop build; validates the
//               requested path and hands it to the GUI front end.
//
// Dependencies: clap (argument parsing), anyhow; rustdirstat::gui
// ============================================================================

// No console window behind the GUI in a release build. Windows gives a
// console subsystem binary a terminal whether or not it writes to one,
// so launching the app from Explorer opened an empty black window beside
// it that stayed for the session.
//
// Debug builds keep the console on purpose: it is where panics and
// `println!` go while developing, and a GUI-subsystem binary discards
// both silently.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Entry point for `rustdirstat-gui`, the desktop build.
//!
//! Deliberately thin: it validates and canonicalises the requested path,
//! then hands off to `rustdirstat::gui::run`. The window, the event loop,
//! and the scan all belong to the library, so the two binaries differ
//! only in which front end they start.

use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;

/// rustdirstat's desktop GUI — a WinDirStat clone with a real resizable
/// window, native dialogs, and a smooth-shaded treemap.
#[derive(Parser, Debug)]
#[command(name = "rustdirstat-gui", version, about)]
struct Cli {
    /// Directories (or files) to scan. Several open as one tree, each as
    /// a top-level entry — the same choice the Locations page offers.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Every path is checked before the window opens, so a typo is a
    // dialog rather than an empty tree the user has to interpret.
    let mut roots = Vec::with_capacity(cli.paths.len());
    for path in &cli.paths {
        if !path.exists() {
            return fail(&format!("Path does not exist:\n{}", path.display()));
        }
        roots.push(path.canonicalize().unwrap_or_else(|_| path.clone()));
    }
    if let Err(error) = rustdirstat::gui::run(roots) {
        return fail(&error.to_string());
    }
    Ok(())
}

/// Reports a startup failure somewhere the user will actually see it.
///
/// A release build is a GUI-subsystem binary, so it has no console:
/// anything written to stderr goes nowhere, and a bad path would look
/// like the program simply refusing to start. A dialog is the only
/// channel that works whether it was launched from Explorer or a
/// terminal — and for a desktop app it is the right one either way.
///
/// The error is still returned, so a caller that *is* watching (a debug
/// build, a shell that checks the exit status) sees it too.
fn fail(message: &str) -> Result<()> {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("RustDirStat")
        .set_description(message)
        .show();
    bail!("{message}")
}
