mod color;
mod model;
mod report;
mod scanner;
mod stats;
mod tui;
mod util;

use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;

/// A cross-platform, terminal-based clone of WinDirStat for visualizing disk usage.
#[derive(Parser, Debug)]
#[command(name = "rustdirstat", version, about)]
struct Cli {
    /// Directory (or file) to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Print a plain-text report instead of launching the interactive TUI
    #[arg(short = 'n', long = "no-tui")]
    no_tui: bool,

    /// Number of top entries to show per directory in report mode
    #[arg(short = 't', long = "top", default_value_t = 20)]
    top: usize,

    /// Maximum depth to descend when producing the report
    #[arg(short = 'd', long = "depth", default_value_t = 2)]
    depth: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.path.exists() {
        bail!("path does not exist: {}", cli.path.display());
    }
    let root = cli.path.canonicalize().unwrap_or(cli.path.clone());

    if cli.no_tui {
        let tree = scanner::scan(&root, None)?;
        report::print_report(&tree, cli.top, cli.depth);
    } else {
        tui::run(root)?;
    }
    Ok(())
}
