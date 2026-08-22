// ============================================================================
// Module:       rustdirstat (binary crate root)
// Description:  Command-line entry point for the terminal build; parses
//               arguments and dispatches to one of the three output modes.
//
// Dependencies: clap (argument parsing), anyhow; crate::{scanner, tui, report,
//               csv_export}
// ============================================================================

//! Entry point for `rustdirstat`, the terminal build.
//!
//! Three mutually exclusive modes: the interactive TUI (the default), a
//! plain-text report (`--no-tui`), and a full CSV export (`--csv`). The
//! two non-interactive modes exist so the same scan can be piped into
//! something else — a terminal UI is no use at all from a script.

use anyhow::{bail, Result};
use clap::Parser;
use rustdirstat::{csv_export, report, scanner, tui};
use std::path::PathBuf;

/// A cross-platform clone of WinDirStat for visualizing disk usage. Run
/// with no flags to launch the terminal UI (`rustdirstat-gui` for the
/// desktop GUI instead).
#[derive(Parser, Debug)]
#[command(name = "rustdirstat", version, about)]
struct Cli {
    /// Directories (or files) to scan. Several are scanned into one
    /// tree, with each one as a top-level entry — the terminal
    /// equivalent of WinDirStat opening on several drives at once.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Print a plain-text report instead of launching the interactive TUI.
    /// Mutually exclusive with `--csv` — the two non-interactive modes
    /// disagree about what the scan is for, so the combination is refused
    /// rather than silently giving one precedence.
    #[arg(short = 'n', long = "no-tui", conflicts_with = "csv")]
    no_tui: bool,

    /// Number of top entries to show per directory in report mode
    #[arg(short = 't', long = "top", default_value_t = 20)]
    top: usize,

    /// Maximum depth to descend when producing the report
    #[arg(short = 'd', long = "depth", default_value_t = 2)]
    depth: usize,

    /// Scan and write a full CSV export (one row per file/directory) to
    /// this path instead of launching the TUI or printing a text report
    #[arg(long = "csv", value_name = "PATH")]
    csv: Option<PathBuf>,

    /// Descend into other filesystems (mount points, /proc, network
    /// shares) instead of staying on the scanned path's own filesystem.
    /// Unix only: Windows reports no device identity to the scanner, so
    /// there the walk always crosses volume boundaries (junctions) and
    /// this flag changes nothing
    #[arg(long = "cross-filesystems")]
    cross_filesystems: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Every path is checked before any of them is scanned: finding out
    // that the third argument was a typo after two drives have been
    // walked is a poor trade for one `exists` call each.
    let mut roots = Vec::with_capacity(cli.paths.len());
    for path in &cli.paths {
        if !path.exists() {
            bail!("path does not exist: {}", path.display());
        }
        roots.push(path.canonicalize().unwrap_or_else(|_| path.clone()));
    }
    let options = scanner::ScanOptions {
        same_filesystem_only: !cli.cross_filesystems,
    };

    if let Some(csv_path) = &cli.csv {
        let tree = scanner::scan_many_to_completion(&roots, options)?;
        csv_export::write_tree_csv(&tree, csv_path)?;
    } else if cli.no_tui {
        let tree = scanner::scan_many_to_completion(&roots, options)?;
        report::print_tree_report(&tree, cli.top, cli.depth);
    } else {
        tui::run(roots, options)?;
    }
    Ok(())
}
