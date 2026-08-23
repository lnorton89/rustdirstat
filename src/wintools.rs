// ============================================================================
// Module:       wintools
// Description:  Windows-only system-maintenance tools, launched as the real OS
//               utilities; reports every tool as unavailable elsewhere.
//
// Dependencies: windows-sys (ShellExecuteW, for elevation),
//               std::process::Command
// ============================================================================

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
    /// Catalogue keys, not text: this table is shown by both front ends,
    /// so it is one of the places a language setting has to reach. The
    /// keys are literals here so the catalogue tests can see them.
    pub name_key: &'static str,
    pub description_key: &'static str,
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

impl WinTool {
    /// What the menus call this tool, in the active language.
    pub fn name(&self) -> String {
        crate::i18n::tr(self.name_key)
    }

    /// What it does, in the active language.
    pub fn description(&self) -> String {
        crate::i18n::tr(self.description_key)
    }
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
        name_key: "wintool.disk_cleanup",
        description_key: "wintool.disk_cleanup.desc",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name_key: "wintool.programs",
        description_key: "wintool.programs.desc",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name_key: "wintool.defragment",
        description_key: "wintool.defragment.desc",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name_key: "wintool.analyze_store",
        description_key: "wintool.analyze_store.desc",
        destructive: false,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name_key: "wintool.clean_store",
        description_key: "wintool.clean_store.desc",
        destructive: true,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name_key: "wintool.reset_base",
        description_key: "wintool.reset_base.desc",
        destructive: true,
        irreversible: true,
        needs_admin: true,
    },
    WinTool {
        name_key: "wintool.chkdsk",
        description_key: "wintool.chkdsk.desc",
        destructive: false,
        irreversible: false,
        needs_admin: false,
    },
    WinTool {
        name_key: "wintool.restore_point",
        description_key: "wintool.restore_point.desc",
        destructive: false,
        irreversible: false,
        needs_admin: true,
    },
    WinTool {
        name_key: "wintool.delete_shadows",
        description_key: "wintool.delete_shadows.desc",
        destructive: true,
        irreversible: true,
        needs_admin: true,
    },
    WinTool {
        name_key: "wintool.empty_bin",
        description_key: "wintool.empty_bin.desc",
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
                summary: crate::i18n::tr_with("tool.launched", &[("program", program)]),
                detail: String::new(),
            })
            .map_err(|e| {
                crate::i18n::tr_with(
                    "tool.launch_failed",
                    &[("program", program), ("error", &e.to_string())],
                )
            })
    };

    let run_and_wait = |program: &str, args: &[&str]| -> Result<ToolOutput, String> {
        let output = Command::new(program).args(args).output().map_err(|e| {
            crate::i18n::tr_with(
                "tool.run_failed",
                &[("program", program), ("error", &e.to_string())],
            )
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            // Keep stdout. For the analyze-only tools it *is* the result,
            // and the one-line summary above it says nothing they were
            // run to find out.
            Ok(ToolOutput {
                summary: crate::i18n::tr_with("tool.completed", &[("program", program)]),
                detail: stdout.trim().to_string(),
            })
        } else {
            let detail = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            Err(crate::i18n::tr_with(
                "tool.exited",
                &[
                    ("program", program),
                    ("status", &output.status.to_string()),
                    ("detail", detail.trim()),
                ],
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
            &[
                "delete",
                "shadows",
                &format!("/for={volume}"),
                "/all",
                "/quiet",
            ],
        ),
        9 => empty_recycle_bin(),
        _ => Err(crate::i18n::tr("tool.unknown")),
    }
}

/// Empties the Recycle Bin for every drive.
///
/// This used to shell out to `powershell -Command "Clear-RecycleBin
/// -Force"`, on the reasoning that a cmdlet beats hand-written FFI for a
/// one-shot operation. In practice the cmdlet is the more fragile of the
/// two: it costs a PowerShell startup, it is refused outright under a
/// restrictive execution policy, and it did not exist before Windows 10.
/// `SHEmptyRecycleBinW` is one documented shell call with no interpreter
/// in front of it, and `Win32_UI_Shell` was already a dependency for the
/// file-type icons.
///
/// A null window handle and a null root path mean "no owner window, all
/// drives". Confirmation, progress and sound UI are all suppressed: this
/// runs behind the app's own confirmation, and a second dialog appearing
/// from nowhere reads as the app having lost control of it.
#[cfg(windows)]
fn empty_recycle_bin() -> Result<ToolOutput, String> {
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    // SAFETY: both pointers are null, which this call documents as "no
    // owner window" and "every drive" rather than as pointers it will
    // dereference. The flags are the documented `SHERB_*` constants.
    let hresult = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };

    match hresult {
        S_OK => Ok(ToolOutput {
            summary: crate::i18n::tr("tool.bin_emptied"),
            detail: String::new(),
        }),
        // An already-empty bin reports `S_FALSE`, and telling someone who
        // asked for an empty Recycle Bin, and now has one, that it failed
        // is not useful.
        S_FALSE => Ok(ToolOutput {
            summary: crate::i18n::tr("tool.bin_already_empty"),
            detail: String::new(),
        }),
        other => Err(crate::i18n::tr_with(
            "tool.bin_failed",
            &[("code", &format!("0x{:08X}", other as u32))],
        )),
    }
}

#[cfg(windows)]
const S_OK: i32 = 0;

/// Returned by `SHEmptyRecycleBinW` when there was nothing to delete.
#[cfg(windows)]
const S_FALSE: i32 = 1;

/// The `C:` a maintenance tool wants, from whatever path the scan holds.
///
/// This used to hand back the first component's text, which for the
/// canonicalized paths the app actually carries is the *prefix* —
/// `\\?\C:`. `chkdsk \\?\C:` and `vssadmin ... /for=\\?\C:` both refuse
/// that: the extended-length form is understood by the file APIs and by
/// nothing else, which is the same reason `util::display_path` exists.
/// So the drive letter is read off the prefix rather than printed with
/// it, and the two spellings of a disk prefix — `C:\` and `\\?\C:\` —
/// answer identically.
///
/// A UNC path has no drive letter at all and never will; the fallback
/// stays `C:` because these tools have to be given something, and the
/// tool itself is the right place for that argument to be rejected.
#[cfg(windows)]
fn volume_root_arg(path: &std::path::Path) -> String {
    use std::path::{Component, Prefix};
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                format!("{}:", char::from(letter).to_ascii_uppercase())
            }
            // A share, a device namespace, or a verbatim path that names
            // no drive. None of these is a volume `chkdsk` or `vssadmin`
            // can be pointed at, and the fallback is the same as having
            // no prefix at all.
            Prefix::Verbatim(_)
            | Prefix::VerbatimUNC(..)
            | Prefix::DeviceNS(_)
            | Prefix::UNC(..) => "C:".to_string(),
        },
        // A relative path, or one that begins at the root of the current
        // drive — neither names a volume either.
        _ => "C:".to_string(),
    }
}

#[cfg(not(windows))]
pub fn run(_index: usize, _volume_path: &std::path::Path) -> Result<ToolOutput, String> {
    Err(crate::i18n::tr("tool.windows_only"))
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
                tool.name_key
            );
        }
    }

    /// A maintenance tool is given `C:`, whichever spelling the scan
    /// carries.
    ///
    /// The paths this app holds are canonicalized, and on Windows that
    /// means `\\?\C:\Users\...`. Taking the first component's text
    /// handed `chkdsk` and `vssadmin` the whole `\\?\C:` prefix, which
    /// neither accepts — so the two tools that take a volume failed on
    /// every scan the app has ever produced.
    #[cfg(windows)]
    #[test]
    fn a_volume_argument_is_a_drive_letter_however_the_path_is_spelled() {
        use std::path::Path;
        for path in [
            r"C:\Users\someone\Documents",
            r"\\?\C:\Users\someone\Documents",
            r"C:\",
            r"\\?\C:\",
        ] {
            assert_eq!(
                super::volume_root_arg(Path::new(path)),
                "C:",
                "{path} should name the volume as C:"
            );
        }
        assert_eq!(
            super::volume_root_arg(Path::new(r"\\?\d:\games")),
            "D:",
            "a lowercase drive letter is still that drive"
        );
    }

    #[test]
    fn the_analyze_only_tool_changes_nothing() {
        // By key, not by text: the text is now whatever language the
        // catalogue is in.
        let analyze = TOOLS.iter().find(|t| t.name_key == "wintool.analyze_store");
        assert!(analyze.is_some(), "the analyze tool should exist");
        assert!(
            analyze.is_some_and(|t| !t.destructive && !t.irreversible),
            "a tool that only reports must not be gated as destructive"
        );
    }
}
