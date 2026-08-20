// ============================================================================
// Module:       rustdirstat-gui (binary crate root)
// Description:  Command-line entry point for the desktop build; validates the
//               requested path and hands it to the GUI front end.
//
// Dependencies: clap (argument parsing), anyhow; rustdirstat::gui
// ============================================================================

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
    /// Directory (or file) to scan
    #[arg(default_value = ".")]
    path: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.path.exists() {
        bail!("path does not exist: {}", cli.path.display());
    }
    let root = cli.path.canonicalize().unwrap_or(cli.path.clone());
    rustdirstat::gui::run(root)
}
