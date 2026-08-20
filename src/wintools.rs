//! Windows-only system-maintenance tools (Disk Cleanup, component store
//! cleanup, defrag, shadow copies, ...) — the kind of thing WinDirStat
//! itself links out to from its "Clean up" menu. These have no equivalent
//! on other platforms (no universal "shrink the WinSxS folder" or "manage
//! shadow copies" concept exists outside NTFS/Windows), so this module is
//! genuinely Windows-only. That's different from *hiding* the feature
//! category on other platforms: the menu itself still exists everywhere
//! (see `tui::app`/`tui::ui`), it just reports every tool as unavailable
//! off Windows rather than not existing at all — a cross-platform app can
//! still carry a platform-specific feature subset without pretending that
//! subset doesn't exist for everyone else.
//!
//! Every tool here launches the real, official Windows utility (Disk
//! Cleanup, the Optimize Drives GUI, an interactive CHKDSK console, ...)
//! rather than scripting the underlying operation directly wherever an
//! interactive tool exists — that keeps any actual confirmation prompts
//! (and any elevation prompts) with the tool that owns them, instead of
//! this app silently running something destructive on someone's behalf.
//! Where no interactive tool exists (DISM component cleanup, shadow copy
//! management), `destructive` tools are gated behind a confirmation
//! prompt in the UI before `run` is ever called.

pub struct WinTool {
    pub name: &'static str,
    pub description: &'static str,
    /// Whether this needs an explicit confirmation prompt before running
    /// — true for anything that isn't just launching an interactive tool
    /// with its own confirmation/undo built in.
    pub destructive: bool,
    /// Whether what it does cannot be walked back afterwards.
    ///
    /// Deliberately narrower than `destructive`: component-store cleanup
    /// warrants a confirmation but is routine maintenance, while
    /// deleting every shadow copy on a volume is not. Carrying only the
    /// one flag meant the confirmation prompt had to say the same thing
    /// about both, which made it simultaneously too alarming for one and
    /// not alarming enough for the other.
    pub irreversible: bool,
    /// Whether it fails without an elevated session. Worth saying up
    /// front: the failure is otherwise a raw `exit code: 5` from a tool
    /// the user did not choose to run directly.
    pub needs_admin: bool,
}

/// What a finished tool reports back.
///
/// `detail` exists because "Analyze Component Store" is a *reporting*
/// tool — its entire purpose is telling the user how much WinSxS could
/// reclaim — and a run that discards its own stdout in favour of
/// "completed successfully" answers nothing.
pub struct ToolOutput {
    pub summary: String,
    pub detail: String,
}

pub const TOOLS: &[WinTool] = &[
    WinTool {
        name: "Disk Cleanup",
        description: "Open the built-in Disk Cleanup wizard (cleanmgr.exe).",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name: "Programs and Features",
        description: "Open the \"uninstall a program\" control panel.",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name: "Defragment and Optimize Drives",
        description: "Open the drive optimization tool (dfrgui.exe).",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name: "Analyze Component Store",
        description: "DISM: report how much space the WinSxS folder could reclaim, without changing anything.",
        destructive: false,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name: "Clean Up Component Store",
        description: "DISM: remove superseded component versions from WinSxS. Routine maintenance, safe to run.",
        destructive: true,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name: "Reset Base (aggressive cleanup)",
        description: "DISM /ResetBase: also removes the ability to uninstall currently installed updates.",
        destructive: true,
        irreversible: true,
        needs_admin: true,
    },
    WinTool {
        name: "Check Disk",
        description: "Open an interactive Check Disk (chkdsk) session for this volume in its own console.",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name: "Create Restore Point",
        description: "vssadmin: create a new shadow copy (System Restore point) of this volume.",
        destructive: false,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name: "Delete All Shadow Copies",
        description: "vssadmin: permanently remove every shadow copy on this volume. Cannot be undone.",
        destructive: true,
        irreversible: true,
        needs_admin: true,
    },
    WinTool {
        name: "Empty Recycle Bin",
        description: "Permanently empty the Recycle Bin on every drive. Cannot be undone.",
        destructive: true,
        irreversible: true,
        needs_admin: false,
    },
];

/// Runs the tool at `index` against the volume containing `volume_path`.
/// Launchers (Disk Cleanup, Programs and Features, Optimize Drives,
/// interactive Check Disk) are spawned detached, matching how
/// `util::open_in_file_manager` already launches OS tools elsewhere in
/// this app — we don't wait on or capture output from something meant to
/// stay open and be used interactively. The DISM/vssadmin commands run
/// and wait, since their result (and whether they even started —
/// commonly they need an elevated/admin session) is the only feedback the
/// user gets, there being no window of their own to show it in.
#[cfg(windows)]
pub fn run(index: usize, volume_path: &std::path::Path) -> Result<ToolOutput, String> {
    use std::process::Command;

    let volume = volume_root_arg(volume_path);

    let spawn_detached = |program: &str, args: &[&str]| -> Result<ToolOutput, String> {
        Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ToolOutput {
                summary: format!("Launched {program}"),
                detail: String::new(),
            })
            .map_err(|e| format!("Failed to launch {program}: {e}"))
    };

    let run_and_wait = |program: &str, args: &[&str]| -> Result<ToolOutput, String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("Failed to run {program}: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            // Keep stdout. For the analyze-only tools it *is* the result,
            // and the one-line summary above it says nothing they were
            // run to find out.
            Ok(ToolOutput {
                summary: format!("{program} completed successfully"),
                detail: stdout.trim().to_string(),
            })
        } else {
            let detail = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            Err(format!(
                "{program} exited with {}: {}",
                output.status,
                detail.trim()
            ))
        }
    };

    match index {
        0 => spawn_detached("cleanmgr", &[]),
        1 => spawn_detached("control", &["appwiz.cpl"]),
        2 => spawn_detached("dfrgui", &[]),
        3 => run_and_wait(
            "dism",
            &["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"],
        ),
        4 => run_and_wait(
            "dism",
            &["/Online", "/Cleanup-Image", "/StartComponentCleanup"],
        ),
        5 => run_and_wait(
            "dism",
            &[
                "/Online",
                "/Cleanup-Image",
                "/StartComponentCleanup",
                "/ResetBase",
            ],
        ),
        6 => spawn_detached("cmd", &["/k", "chkdsk", &volume]),
        7 => run_and_wait("vssadmin", &["create", "shadow", &format!("/for={volume}")]),
        8 => run_and_wait(
            "vssadmin",
            &["delete", "shadows", "/for", &volume, "/all", "/quiet"],
        ),
        // Clear-RecycleBin is a modern PowerShell cmdlet (Windows 10+) —
        // simpler and less error-prone than hand-writing the SHEmptyRecycleBinW
        // FFI call for what's ultimately a one-shot, non-interactive operation.
        9 => run_and_wait(
            "powershell",
            &["-NoProfile", "-Command", "Clear-RecycleBin -Force"],
        ),
        _ => Err("Unknown tool".to_string()),
    }
}

#[cfg(windows)]
fn volume_root_arg(path: &std::path::Path) -> String {
    path.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_else(|| "C:".to_string())
}

#[cfg(not(windows))]
pub fn run(_index: usize, _volume_path: &std::path::Path) -> Result<ToolOutput, String> {
    Err("This tool is only available on Windows.".to_string())
}

#[cfg(test)]
mod tests {
    use super::TOOLS;

    /// `irreversible` is the stronger claim of the two, so a tool that
    /// cannot be undone must also be one the UI stops to confirm. The UI
    /// reads them independently — this is what keeps the pair coherent.
    #[test]
    fn every_irreversible_tool_is_also_confirmed() {
        for tool in TOOLS {
            assert!(
                !tool.irreversible || tool.destructive,
                "{} is irreversible but would run without a confirmation",
                tool.name
            );
        }
    }

    #[test]
    fn the_analyze_only_tool_changes_nothing() {
        let analyze = TOOLS.iter().find(|t| t.name == "Analyze Component Store");
        assert!(analyze.is_some(), "the analyze tool should exist");
        assert!(
            analyze.is_some_and(|t| !t.destructive && !t.irreversible),
            "a tool that only reports must not be gated as destructive"
        );
    }
}
