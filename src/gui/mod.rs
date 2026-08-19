//! Native desktop GUI front end (egui/eframe), reusing the same scanning,
//! model, and treemap-layout core as the TUI. WinDirStat is a desktop
//! app — a terminal UI can approximate its look with block characters and
//! ANSI colors, but it can never actually have real resizable panes,
//! smooth pixel-level treemap shading, or native dialogs, all of which
//! this front end has instead.

mod app;
mod treemap_layout;
mod ui;

use anyhow::Result;
use std::path::PathBuf;

pub fn run(root: PathBuf) -> Result<()> {
    let tree = crate::scanner::scan(&root, None)?;
    let gui_app = app::GuiApp::new(tree);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "rustdirstat",
        options,
        Box::new(|_cc| Ok(Box::new(gui_app))),
    )
    .map_err(|e| anyhow::anyhow!("GUI failed: {e}"))
}
