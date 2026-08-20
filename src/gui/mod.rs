//! Native desktop GUI front end (egui/eframe), reusing the same scanning,
//! model, and treemap-layout core as the TUI. WinDirStat is a desktop
//! app — a terminal UI can approximate its look with block characters and
//! ANSI colors, but it can never actually have real resizable panes,
//! smooth pixel-level treemap shading, or native dialogs, all of which
//! this front end has instead.

mod app;
mod icons;
mod shell_icons;
mod treemap_layout;
mod ui;

use anyhow::Result;
use std::path::PathBuf;

pub fn run(root: PathBuf) -> Result<()> {
    // Open the native shell immediately; the initial scan runs on the same
    // background path as later rescans so a large drive never looks like a
    // failed launch while the process is busy walking the filesystem.
    let gui_app = app::GuiApp::loading(root);

    let options = native_options();

    eframe::run_native(
        "RustDirStat",
        options,
        Box::new(|_cc| Ok(Box::new(gui_app))),
    )
    .map_err(|e| anyhow::anyhow!("GUI failed: {e}"))
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([720.0, 480.0])
            .with_icon(icons::app_icon()),
        // wgpu avoids glutin's rigid framebuffer-config selection on Linux
        // and can use Vulkan or OpenGL according to what the system exposes.
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_gui_uses_wgpu_instead_of_glutin() {
        assert_eq!(native_options().renderer, eframe::Renderer::Wgpu);
    }
}
