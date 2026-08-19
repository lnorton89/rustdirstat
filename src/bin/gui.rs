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
