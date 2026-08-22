// ============================================================================
// Module:       gui::app::tools
// Description:  Operations that reach outside the app: deleting, emptying,
//               Windows maintenance tools, and the duplicate scan.
//
// Dependencies: crate::{duplicates, util, wintools}; super::GuiApp
// ============================================================================

//! Operations that change something outside the app: deleting files,
//! emptying folders, running a Windows maintenance tool, and the
//! duplicate scan.
//!
//! Each is queued as a pending confirmation first — nothing here acts on
//! the filesystem without one.

use super::*;

/// One finished maintenance tool, kept for the Maintenance page.
///
/// The status bar shows a single line that the next scan overwrites,
/// which is fine for "launched cleanmgr" and useless for a DISM report
/// the user ran specifically to read.
pub(in crate::gui) struct ToolOutcome {
    pub tool: String,
    pub summary: String,
    pub detail: String,
    pub failed: bool,
}

pub(in crate::gui) struct PendingDelete {
    pub index_path: Vec<usize>,
    /// The raw filesystem name, kept `OsString` so the stale-name check
    /// in `confirm_delete` compares identity rather than the lossy
    /// display of it.
    pub name: std::ffi::OsString,
    pub is_dir: bool,
    pub permanent: bool,
}

/// The Windows maintenance tools: what is queued, what is running, and
/// what has already run.
#[derive(Default)]
pub(in crate::gui) struct ToolsState {
    /// Set while a tool waits on its confirmation.
    pub pending: Option<usize>,
    pub rx: Option<mpsc::Receiver<Result<crate::wintools::ToolOutput, String>>>,
    pub active_name: Option<String>,
    /// Index of the tool currently running, so its own row can show the
    /// spinner instead of the page closing out from under it.
    pub running: Option<usize>,
    pub log: Vec<ToolOutcome>,
    /// What the Locations page has ticked, which is not what is being
    /// scanned until the button is pressed. Kept on the app rather than
    /// in the page because the page is redrawn from scratch every frame
    /// and has nowhere of its own to remember a selection.
    pub selected_locations: Vec<std::path::PathBuf>,
}

impl GuiApp {
    pub(in crate::gui) fn find_duplicates(&mut self) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        self.status = Some("Finding duplicate files…".to_string());
        self.file_view = FileView::DuplicateFiles;
        let tree = Arc::clone(&self.tree);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::duplicates::find_duplicates(tree.as_ref(), None));
        });
        self.duplicate_rx = Some(rx);
    }

    pub(in crate::gui) fn duplicate_running(&self) -> bool {
        self.duplicate_rx.is_some()
    }

    pub(in crate::gui) fn request_windows_tool(&mut self, index: usize) {
        let Some(tool) = crate::wintools::TOOLS.get(index) else {
            return;
        };
        if tool.destructive {
            self.tools.pending = Some(index);
        } else {
            self.start_windows_tool(index);
        }
    }

    pub(in crate::gui) fn confirm_windows_tool(&mut self) {
        if let Some(index) = self.tools.pending.take() {
            self.start_windows_tool(index);
        }
    }

    fn start_windows_tool(&mut self, index: usize) {
        if self.is_busy() {
            self.status = Some("Another background operation is already running".to_string());
            return;
        }
        let Some(tool) = crate::wintools::TOOLS.get(index) else {
            return;
        };
        let root = self.tree.root_path.clone();
        let name = tool.name.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::wintools::run(index, &root));
        });
        self.tools.active_name = Some(name.clone());
        self.tools.running = Some(index);
        self.tools.rx = Some(rx);
        self.status = Some(format!("Running {name}…"));
        // The page stays open. A DISM run takes minutes, and closing the
        // window that started it left a one-line status bar as the only
        // sign anything was happening.
    }

    pub(in crate::gui) fn request_delete_selected(&mut self, permanent: bool) {
        let Some(index_path) = self.selected_path.clone() else {
            return;
        };
        // The first row represents scan context, not a cleanup target.
        // Never let selecting it queue deletion of a whole drive/root.
        if index_path.is_empty() {
            self.status = Some("The scan root cannot be deleted from this view".to_string());
            return;
        }
        // Exact: a delete queued against a path that no longer resolves
        // must not silently describe whatever now sits at those indices.
        let Some(node) = self.tree.node_for(&index_path) else {
            self.status = Some("That item is no longer there".to_string());
            return;
        };
        self.pending_delete = Some(PendingDelete {
            index_path,
            name: node.name.clone(),
            is_dir: node.is_dir,
            permanent,
        });
    }

    pub(in crate::gui) fn confirm_delete(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        // Belt and braces with the clearing in `replace_tree`: confirm
        // the indices still lead to the item the user actually chose
        // before handing a path to `remove_dir_all`. Deleting the wrong
        // thing is not an error worth being clever about recovering
        // from.
        let Some(node) = self.tree.node_for(&pending.index_path) else {
            self.status = Some(format!(
                "{} is no longer where it was; nothing was deleted",
                pending.name.to_string_lossy()
            ));
            return Ok(());
        };
        if node.name != pending.name || node.is_dir != pending.is_dir {
            self.status = Some(format!(
                "{} moved since it was selected; nothing was deleted",
                pending.name.to_string_lossy()
            ));
            return Ok(());
        }
        let Some(path) = self.tree.path_for(&pending.index_path) else {
            self.status = Some(format!(
                "{} is no longer where it was; nothing was deleted",
                pending.name.to_string_lossy()
            ));
            return Ok(());
        };
        if pending.permanent {
            if pending.is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        } else {
            trash::delete(&path).map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }
        self.status = Some(format!(
            "{}: {}",
            if pending.permanent {
                "Permanently deleted"
            } else {
                "Moved to Recycle Bin"
            },
            path.display()
        ));
        self.selected_path = None;
        self.refresh_scan()
    }

    pub(in crate::gui) fn confirm_empty(&mut self) -> anyhow::Result<()> {
        let Some(pending) = self.pending_delete.take() else {
            return Ok(());
        };
        if !pending.is_dir {
            return Ok(());
        }
        let Some(path) = self.tree.path_for(&pending.index_path) else {
            self.status = Some(format!(
                "{} is no longer where it was; nothing was emptied",
                pending.name.to_string_lossy()
            ));
            return Ok(());
        };
        // Enumerate-fully-then-delete, shared with the TUI: an
        // incomplete listing aborts before anything is deleted, and a
        // mid-way failure is reported as the partial emptying it is.
        // Either way the folder is rescanned so the tree matches what
        // actually went to the trash.
        match crate::util::empty_directory_to_trash(&path)? {
            crate::util::EmptyOutcome::Incomplete { unreadable } => {
                self.status = Some(format!(
                    "Nothing was emptied: {unreadable} entr{} could not be read ({})",
                    if unreadable == 1 { "y" } else { "ies" },
                    path.display()
                ));
            }
            crate::util::EmptyOutcome::Partial {
                done,
                total,
                first_error,
            } => {
                let failed = total - done;
                self.status = Some(match first_error {
                    Some(error) => format!(
                        "Emptied {done} of {total} items in {}; {failed} could not be \
                         moved to trash ({error})",
                        path.display()
                    ),
                    None => format!(
                        "Emptied {done} of {total} items in {}; {failed} could not be \
                         moved to trash",
                        path.display()
                    ),
                });
            }
            crate::util::EmptyOutcome::Emptied { .. } => {
                self.status = Some(format!("Emptied: {}", path.display()));
            }
        }
        self.refresh_scan()
    }
}
