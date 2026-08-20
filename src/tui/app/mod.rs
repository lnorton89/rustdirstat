// ============================================================================
// Module:       tui::app
// Description:  App: the terminal front end's state, grouped by the view that
//               owns it, and the action dispatch every key and click funnels
//               into.
//
// Dependencies: crossterm (key codes), trash (delete to recycle bin), anyhow;
//               crate::{model, config, stats}
// ============================================================================

//! `App`: the terminal front end's state, and the action dispatch that
//! every key and click funnels into.
//!
//! State is grouped by the view that owns it — `SearchState`,
//! `DuplicatesState`, `MoveState`, `WinToolsState` — rather than sitting
//! flat on `App`. A new field belongs in its group; forty-odd flat fields
//! prefixed `search_` and `duplicate_` is what that convention replaced.
//!
//! Keyboard and mouse both resolve to an `Action` rather than each
//! carrying its own copy of the behaviour, so the two cannot drift apart.
//!
//! Destructive confirmations answer only to the keys they advertise. An
//! unrecognised key leaves the prompt standing rather than counting as a
//! cancel — treating everything else as a cancel meant the next
//! keystroke, aimed at the dialog, landed in the file list instead.

use crate::color::Category;
use crate::config::Config;
use crate::model::{Node, SortMode, Tree};
use crate::search::{self, SearchHit};
use crate::stats::{self, ExtStat};
use crate::top_files::{self, TopFile};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod input;
mod navigation;
mod operations;
mod views;

// Re-exported so the rest of the front end still names these at
// `tui::app::`, as it did when this was one file.
pub(in crate::tui) use input::ClickZone;

/// Every user-triggerable operation, so keyboard and mouse input can share
/// one code path instead of duplicating behavior.
///
/// `Debug` so a failing test can name the action it dispatched.
#[derive(Clone, Debug)]
pub(in crate::tui) enum Action {
    Up,
    Down,
    OpenSelected,
    Back,
    CycleSort,
    ToggleTreemap,
    RequestDelete,
    RequestDeletePermanent,
    OpenInFileManager,
    OpenItem,
    CopyPath,
    StartMove,
    ToggleProperties,
    ToggleWinTools,
    SelectWinTool(usize),
    ConfirmWinTool,
    CancelWinTool,
    Quit,
    ToggleHighlight(Category),
    ClearHighlight,
    SelectRow(usize),
    NavigateTo(Vec<usize>),
    ConfirmDelete,
    ConfirmEmpty,
    CancelDelete,
    Refresh,
    ToggleTopFiles,
    ToggleHelp,
    ExportReport,
    StartFilter,
    StartSubtreeSearch,
    ToggleDetails,
    GrowTreemap,
    ShrinkTreemap,
    StartResize,
    TogglePhysicalSize,
    ToggleDuplicates,
    ExportCsv,
}

/// One row of the flattened duplicate-groups view: either a group header
/// (not itself navigable — just a size/count label) or a member file
/// (navigable, like a search hit).
pub(in crate::tui) enum DupRow {
    Header { size: u64, count: usize },
    Member { index_path: Vec<usize> },
}

pub(in crate::tui) struct PendingDelete {
    pub orig_idx: usize,
    pub name: String,
    pub permanent: bool,
    /// Whether the target is a directory — the delete-confirm popup only
    /// offers an "Empty" (keep the folder, delete its contents) option
    /// when this is true.
    pub is_dir: bool,
}

/// Text-entry state for "Move to" (`M`) — the destination folder,
/// entered the same way the search and filter prompts are.
#[derive(Default)]
pub(in crate::tui) struct MoveState {
    pub entry_mode: bool,
    pub destination: String,
}

/// The Windows system-maintenance tools menu (`T`) — present on every
/// platform (see `wintools`'s module doc for why), just reporting every
/// entry as unavailable off Windows rather than not existing.
#[derive(Default)]
pub(in crate::tui) struct WinToolsState {
    pub visible: bool,
    pub selected: usize,
    /// Set while a destructive tool waits on a yes/no confirmation,
    /// before `wintools::run` is actually called.
    pub pending: Option<usize>,
}

/// The recursive name search: the query being typed, and the results.
///
/// Distinct from the quick `/` filter, which only narrows the current
/// directory's direct children. `entry_mode` is the query text-entry
/// state, `visible` is the results view.
#[derive(Default)]
pub(in crate::tui) struct SearchState {
    pub query: String,
    pub entry_mode: bool,
    pub visible: bool,
    pub results: Vec<SearchHit>,
    pub truncated: bool,
    pub error: Option<String>,
}

/// The duplicate-files view and the last scan's results.
#[derive(Default)]
pub(in crate::tui) struct DuplicatesState {
    pub scan_requested: bool,
    pub visible: bool,
    pub rows: Vec<DupRow>,
    pub group_count: usize,
    pub total_wasted: u64,
    /// Set when there were more groups than the view will list.
    pub truncated: bool,
    /// Files the scan never hashed, because it hit its candidate limit.
    /// Shown, so "no more duplicates" is not confused with "we stopped
    /// looking".
    pub skipped: usize,
}

pub(in crate::tui) struct App {
    pub tree: Tree,
    /// Indices (into each level's original, unsorted `children` vec) from
    /// the root down to the directory currently being browsed.
    pub path_indices: Vec<usize>,
    pub selected: usize,
    pub sort: SortMode,
    pub show_treemap: bool,
    pub pending_delete: Option<PendingDelete>,
    pub message: Option<String>,
    pub ext_stats: Vec<ExtStat>,
    pub should_quit: bool,
    pub highlighted_category: Option<Category>,
    /// Clickable regions from the most recent frame; consumed by mouse clicks.
    pub click_zones: Vec<ClickZone>,
    last_click: Option<(usize, Instant)>,
    pub filter: String,
    pub filter_mode: bool,
    pub show_top_files: bool,
    pub top_files_cache: Vec<TopFile>,
    /// Recursive subtree search (distinct from `filter`, which only
    /// narrows the current directory's direct children): `search_mode` is
    /// the query text-entry state, `show_search` is the results view.
    pub search: SearchState,
    pub show_help: bool,
    pub refresh_requested: bool,
    /// Show file/dir counts and modified dates in the list — off by
    /// default to keep each row to the essentials (bar, size, name).
    pub detailed: bool,
    /// Size the list/treemap/header show and use for proportions: physical
    /// (on-disk, allocated) when true, logical (apparent) when false.
    pub use_physical: bool,
    /// Width of the treemap panel as a percentage of the body area.
    pub treemap_split: u16,
    /// Set while the divider between the list and treemap panels is being
    /// mouse-dragged; `body_x`/`body_width` (recorded each frame) let a
    /// drag position translate into a split percentage.
    pub resizing_treemap: bool,
    body_x: u16,
    body_width: u16,
    /// Set by `Action::ToggleDuplicates` and consumed by the browse loop in
    /// `tui::mod`, which runs the actual (background-threaded) scan and
    /// its own progress screen — hashing file content can take a while on
    /// a large tree, which doesn't fit the instant request/dispatch model
    /// every other action uses.
    pub duplicates: DuplicatesState,
    pub move_to: MoveState,
    pub show_properties: bool,
    pub wintools: WinToolsState,
}

const TREEMAP_SPLIT_MIN: u16 = 20;
const TREEMAP_SPLIT_MAX: u16 = 75;
const TREEMAP_SPLIT_STEP: u16 = 5;

const TOP_FILES_LIMIT: usize = 500;

impl App {
    pub(in crate::tui) fn new(tree: Tree) -> Self {
        let mut app = Self {
            tree,
            path_indices: vec![],
            selected: 0,
            sort: SortMode::SizeDesc,
            show_treemap: true,
            pending_delete: None,
            message: None,
            ext_stats: vec![],
            should_quit: false,
            highlighted_category: None,
            click_zones: vec![],
            last_click: None,
            filter: String::new(),
            filter_mode: false,
            show_top_files: false,
            top_files_cache: vec![],
            search: SearchState::default(),
            show_help: false,
            refresh_requested: false,
            detailed: false,
            use_physical: false,
            treemap_split: 45,
            resizing_treemap: false,
            body_x: 0,
            body_width: 0,
            duplicates: DuplicatesState::default(),
            move_to: MoveState::default(),
            show_properties: false,
            wintools: WinToolsState::default(),
        };
        app.refresh_ext_stats();
        app
    }

    /// Applies persisted preferences loaded at startup. Every field is
    /// optional, so a first run (no config file yet) or a config missing a
    /// newer field just leaves that setting at its built-in default.
    pub(in crate::tui) fn apply_config(&mut self, cfg: &Config) {
        if let Some(sort) = cfg.sort {
            self.sort = sort;
        }
        if let Some(show_treemap) = cfg.show_treemap {
            self.show_treemap = show_treemap;
        }
        if let Some(split) = cfg.treemap_split {
            self.treemap_split = split.clamp(TREEMAP_SPLIT_MIN, TREEMAP_SPLIT_MAX);
        }
        if let Some(detailed) = cfg.detailed {
            self.detailed = detailed;
        }
        if let Some(use_physical) = cfg.use_physical {
            self.use_physical = use_physical;
        }
    }

    /// Snapshots the preferences worth persisting across runs. Deliberately
    /// excludes anything tied to this specific scan (browse location,
    /// filters, search state, highlighted category) — those are session
    /// state, not preferences, and restoring them on an unrelated future
    /// scan would be surprising rather than helpful.
    pub(in crate::tui) fn to_config(&self) -> Config {
        Config {
            sort: Some(self.sort),
            show_treemap: Some(self.show_treemap),
            treemap_split: Some(self.treemap_split),
            detailed: Some(self.detailed),
            use_physical: Some(self.use_physical),
            ..Config::default()
        }
    }

    pub(in crate::tui) fn dispatch(&mut self, action: Action) -> Result<()> {
        if self.pending_delete.is_some() {
            // Spelled out rather than wildcarded. This used to be
            // `_ => self.pending_delete = None`, defended as "cancelling
            // is the safe default" — but the effect was that every one
            // of the actions below, and every action added later,
            // silently dismissed a delete prompt. That is the same
            // defect the key handler had, and the same rule applies: a
            // destructive confirmation answers only to the things it
            // offers.
            //
            // The lint stays on so a new `Action` cannot join the enum
            // without someone deciding which of these three groups it
            // belongs in.
            match action {
                Action::ConfirmDelete => self.confirm_delete()?,
                Action::ConfirmEmpty => self.confirm_empty()?,
                Action::CancelDelete => self.pending_delete = None,
                // Everything else leaves the prompt standing. A stray
                // click or keystroke aimed at the dialog must not land
                // on the file list behind it.
                Action::Up
                | Action::Down
                | Action::OpenSelected
                | Action::Back
                | Action::CycleSort
                | Action::ToggleTreemap
                | Action::RequestDelete
                | Action::RequestDeletePermanent
                | Action::OpenInFileManager
                | Action::OpenItem
                | Action::CopyPath
                | Action::StartMove
                | Action::ToggleProperties
                | Action::ToggleWinTools
                | Action::SelectWinTool(_)
                | Action::ConfirmWinTool
                | Action::CancelWinTool
                | Action::Quit
                | Action::ToggleHighlight(_)
                | Action::ClearHighlight
                | Action::SelectRow(_)
                | Action::NavigateTo(_)
                | Action::Refresh
                | Action::ToggleTopFiles
                | Action::ToggleHelp
                | Action::ExportReport
                | Action::StartFilter
                | Action::StartSubtreeSearch
                | Action::ToggleDetails
                | Action::GrowTreemap
                | Action::ShrinkTreemap
                | Action::StartResize
                | Action::TogglePhysicalSize
                | Action::ToggleDuplicates
                | Action::ExportCsv => {}
            }
            return Ok(());
        }

        self.message = None;
        match action {
            Action::Up => self.selected = self.selected.saturating_sub(1),
            Action::Down => {
                let len = if self.show_top_files {
                    self.top_files_cache.len()
                } else if self.search.visible {
                    self.search.results.len()
                } else if self.duplicates.visible {
                    self.duplicates.rows.len()
                } else {
                    self.display_children().len()
                };
                if self.selected + 1 < len {
                    self.selected += 1;
                }
            }
            Action::OpenSelected => {
                self.exit_flat_view_if_needed();
                let target = self
                    .display_children()
                    .get(self.selected)
                    .map(|(idx, n)| (*idx, n.is_dir));
                if let Some((idx, is_dir)) = target {
                    if is_dir {
                        self.path_indices.push(idx);
                        self.selected = 0;
                        self.refresh_ext_stats();
                    }
                }
            }
            Action::Back => {
                if !self.path_indices.is_empty() {
                    self.path_indices.pop();
                    self.selected = 0;
                    self.refresh_ext_stats();
                }
            }
            Action::CycleSort => self.sort = self.sort.next(),
            Action::ToggleTreemap => self.show_treemap = !self.show_treemap,
            Action::ShrinkTreemap => {
                self.treemap_split = self
                    .treemap_split
                    .saturating_sub(TREEMAP_SPLIT_STEP)
                    .max(TREEMAP_SPLIT_MIN);
            }
            Action::GrowTreemap => {
                self.treemap_split =
                    (self.treemap_split + TREEMAP_SPLIT_STEP).min(TREEMAP_SPLIT_MAX);
            }
            Action::StartResize => self.resizing_treemap = true,
            Action::RequestDelete => self.request_delete(false),
            Action::RequestDeletePermanent => self.request_delete(true),
            Action::OpenInFileManager => {
                self.exit_flat_view_if_needed();
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let mut target = self.current_path();
                    target.push(&node.name);
                    let target = if node.is_dir {
                        target
                    } else {
                        target.parent().map(|p| p.to_path_buf()).unwrap_or(target)
                    };
                    if let Err(e) = crate::util::open_in_file_manager(&target) {
                        self.message = Some(format!("Failed to open file manager: {e}"));
                    }
                }
            }
            Action::OpenItem => {
                self.exit_flat_view_if_needed();
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let mut target = self.current_path();
                    target.push(&node.name);
                    if let Err(e) = crate::util::open_path(&target) {
                        self.message = Some(format!("Failed to open: {e}"));
                    }
                }
            }
            Action::CopyPath => {
                self.exit_flat_view_if_needed();
                if let Some((_, node)) = self.display_children().get(self.selected) {
                    let mut target = self.current_path();
                    target.push(&node.name);
                    let text = target.display().to_string();
                    match crate::util::copy_to_clipboard(&text) {
                        Ok(()) => self.message = Some(format!("Copied path: {text}")),
                        Err(e) => self.message = Some(format!("Failed to copy path: {e}")),
                    }
                }
            }
            Action::StartMove => {
                self.exit_flat_view_if_needed();
                if self.display_children().get(self.selected).is_some() {
                    self.move_to.entry_mode = true;
                    self.move_to.destination.clear();
                }
            }
            Action::ToggleProperties => {
                self.exit_flat_view_if_needed();
                self.show_properties = !self.show_properties;
            }
            Action::ToggleWinTools => {
                self.wintools.visible = !self.wintools.visible;
                self.wintools.selected = 0;
            }
            Action::SelectWinTool(idx) => {
                if let Some(tool) = crate::wintools::TOOLS.get(idx) {
                    if tool.destructive {
                        self.wintools.pending = Some(idx);
                    } else {
                        self.run_wintool(idx);
                    }
                }
            }
            Action::ConfirmWinTool => {
                if let Some(idx) = self.wintools.pending.take() {
                    self.run_wintool(idx);
                }
            }
            Action::CancelWinTool => self.wintools.pending = None,
            Action::Quit => self.should_quit = true,
            Action::ToggleHighlight(cat) => {
                self.highlighted_category = if self.highlighted_category == Some(cat) {
                    None
                } else {
                    Some(cat)
                };
            }
            Action::ClearHighlight => self.highlighted_category = None,
            Action::SelectRow(idx) => {
                if self.show_top_files {
                    if let Some(tf) = self.top_files_cache.get(idx) {
                        let idx_path = tf.index_path.clone();
                        self.navigate_to(idx_path);
                    }
                    self.show_top_files = false;
                    return Ok(());
                }
                if self.search.visible {
                    if let Some(hit) = self.search.results.get(idx) {
                        let idx_path = hit.index_path.clone();
                        self.navigate_to(idx_path);
                    }
                    self.search.visible = false;
                    return Ok(());
                }
                if self.duplicates.visible {
                    match self.duplicates.rows.get(idx) {
                        Some(DupRow::Member { index_path }) => {
                            let idx_path = index_path.clone();
                            self.navigate_to_absolute(idx_path);
                            self.duplicates.visible = false;
                        }
                        Some(DupRow::Header { .. }) => self.selected = idx,
                        None => {}
                    }
                    return Ok(());
                }
                let now = Instant::now();
                let is_double_click = matches!(
                    self.last_click,
                    Some((last_idx, t)) if last_idx == idx && now.duration_since(t) < Duration::from_millis(450)
                );
                let len = self.display_children().len();
                if idx < len {
                    self.selected = idx;
                }
                if is_double_click {
                    self.last_click = None;
                    self.dispatch(Action::OpenSelected)?;
                } else {
                    self.last_click = Some((idx, now));
                }
            }
            Action::NavigateTo(path) => self.navigate_to(path),
            Action::Refresh => self.refresh_requested = true,
            Action::ToggleTopFiles => {
                self.search.visible = false;
                self.duplicates.visible = false;
                self.show_top_files = !self.show_top_files;
                self.selected = 0;
                if self.show_top_files {
                    self.refresh_top_files();
                }
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::ExportReport => self.export_report(),
            Action::ExportCsv => self.export_csv(),
            Action::StartFilter => {
                // Filtering only applies to the normal browse view (see
                // `display_children`) — search results and duplicate
                // groups aren't filtered by it at all, so typing here
                // while one of those flat views is open would be dead
                // input with a real side effect: `self.filter` still gets
                // set, and it stays active once the user leaves the flat
                // view, silently filtering whatever directory they land
                // in next by a string they never meant to apply there.
                // Closes the other flat views the same way
                // ToggleTopFiles/StartSubtreeSearch/ToggleDuplicates
                // already do, rather than navigating anywhere — starting
                // a filter isn't "act on the selected row", so it
                // shouldn't move the browse location the way
                // exit_flat_view_if_needed's callers do.
                self.search.visible = false;
                self.show_top_files = false;
                self.duplicates.visible = false;
                self.filter_mode = true;
                self.filter.clear();
                self.selected = 0;
            }
            Action::StartSubtreeSearch => {
                if self.search.visible {
                    self.search.visible = false;
                } else {
                    self.show_top_files = false;
                    self.duplicates.visible = false;
                    self.search.entry_mode = true;
                    self.search.query.clear();
                    self.selected = 0;
                }
            }
            Action::ToggleDetails => self.detailed = !self.detailed,
            Action::TogglePhysicalSize => {
                self.use_physical = !self.use_physical;
                self.refresh_ext_stats();
            }
            Action::ToggleDuplicates => {
                if self.duplicates.visible {
                    self.duplicates.visible = false;
                } else {
                    self.search.visible = false;
                    self.show_top_files = false;
                    self.duplicates.scan_requested = true;
                }
            }
            Action::ConfirmDelete | Action::ConfirmEmpty | Action::CancelDelete => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::operations::STALE_TARGET;
    use super::*;
    use std::path::PathBuf;

    /// An app browsing a root holding two files whose logical and
    /// on-disk sizes disagree in opposite directions.
    fn app_with_a_sparse_and_a_packed_file() -> App {
        use crate::model::fixtures::file_sized;
        let mut tree = Tree::placeholder(PathBuf::from("root"));
        tree.root.children = vec![
            file_sized("sparse.img", 1_000_000, 4_096),
            file_sized("packed.bin", 500_000, 500_000),
        ];
        App::new(tree)
    }

    /// Toggling to on-disk sizes reorders the list, rather than only
    /// changing the numbers in it.
    ///
    /// `display_children` had its own copy of the sort, and that copy
    /// read `size` regardless of `use_physical` — so pressing `p` swapped
    /// every size on screen and left the rows in logical order, which
    /// reads as the toggle having done nothing at all. Both front ends
    /// call `model::sort_nodes` now.
    #[test]
    fn switching_to_on_disk_sizes_reorders_the_file_list() {
        let mut app = app_with_a_sparse_and_a_packed_file();
        app.sort = SortMode::SizeDesc;

        app.use_physical = false;
        let logical: Vec<&str> = app
            .display_children()
            .iter()
            .map(|(_, n)| n.name.as_str())
            .collect();
        assert_eq!(
            logical,
            ["sparse.img", "packed.bin"],
            "by logical size the sparse file leads"
        );

        app.use_physical = true;
        let physical: Vec<&str> = app
            .display_children()
            .iter()
            .map(|(_, n)| n.name.as_str())
            .collect();
        assert_eq!(
            physical,
            ["packed.bin", "sparse.img"],
            "the list should be ordered by the size it is showing"
        );
    }

    fn app_awaiting_delete_confirmation(is_dir: bool) -> App {
        let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
        app.pending_delete = Some(PendingDelete {
            orig_idx: 0,
            name: "doomed".to_owned(),
            permanent: false,
            is_dir,
        });
        app
    }

    /// Only the three confirmation actions touch a pending delete.
    ///
    /// `dispatch` used to end its pending-delete guard with
    /// `_ => self.pending_delete = None`, so every other action — and
    /// every action added afterwards — quietly dismissed the prompt.
    /// A click landing anywhere behind the dialog took it down.
    #[test]
    fn only_the_confirmation_actions_answer_a_pending_delete() -> Result<()> {
        let ignored = [
            Action::Up,
            Action::Down,
            Action::Back,
            Action::CycleSort,
            Action::Refresh,
            Action::ToggleHelp,
            Action::SelectRow(3),
            Action::ToggleTreemap,
            Action::TogglePhysicalSize,
        ];
        for action in ignored {
            let mut app = app_awaiting_delete_confirmation(true);
            let name = format!("{action:?}");
            app.dispatch(action)?;
            assert!(
                app.pending_delete.is_some(),
                "{name} dismissed the delete prompt, but it is not one of the \
                 actions the prompt offers"
            );
        }

        let mut app = app_awaiting_delete_confirmation(true);
        app.dispatch(Action::CancelDelete)?;
        assert!(
            app.pending_delete.is_none(),
            "CancelDelete is what the dialog's No button sends, and must cancel"
        );
        Ok(())
    }

    /// A delete queued against a tree that no longer has that entry is
    /// refused, not applied to whatever now sits at those indices.
    ///
    /// `node_for` answers about the deepest node that exists, so a stale
    /// index path resolves to the target's *parent*. For a delete that
    /// is worse than a crash: the confirmation would read the parent
    /// directory's size and then remove it.
    #[test]
    fn a_delete_queued_against_a_vanished_entry_is_refused() -> Result<()> {
        let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
        // The placeholder root has no children at all, so index 0 is
        // already past the end.
        app.pending_delete = Some(PendingDelete {
            orig_idx: 0,
            name: "gone".to_owned(),
            permanent: true,
            is_dir: false,
        });

        app.dispatch(Action::ConfirmDelete)?;

        assert!(
            app.pending_delete.is_none(),
            "the prompt should be dismissed either way"
        );
        assert_eq!(
            app.message.as_deref(),
            Some(STALE_TARGET),
            "the user should be told the target is gone rather than something else being deleted"
        );
        assert_eq!(
            app.tree.root.children.len(),
            0,
            "nothing should have been removed from the tree"
        );
        Ok(())
    }

    /// Only the keys the confirmation dialog offers do anything to it.
    ///
    /// It used to treat every key other than Y (and E for folders) as a
    /// cancel, so an arrow key or a function key silently dismissed the
    /// dialog — and the keystroke the user then aimed at it went to the
    /// file list instead. The dialog itself only ever advertised
    /// `[Y]es`, `[E]mpty` and `[N]o`.

    #[test]
    fn a_key_the_delete_dialog_does_not_offer_leaves_it_standing() -> Result<()> {
        for code in [
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Tab,
            KeyCode::F(5),
            KeyCode::Char(' '),
            KeyCode::Char('x'),
        ] {
            let mut app = app_awaiting_delete_confirmation(true);
            app.handle_key(code)?;
            assert!(
                app.pending_delete.is_some(),
                "{code:?} dismissed the delete confirmation, but the dialog does not offer it"
            );
        }
        Ok(())
    }

    /// The Windows-tool confirmation guards a destructive action too,
    /// and had the same "any key cancels" shape.
    #[test]
    fn a_key_the_wintool_dialog_does_not_offer_leaves_it_standing() -> Result<()> {
        for code in [
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::F(5),
            KeyCode::Char('x'),
        ] {
            let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
            app.wintools.pending = Some(0);
            app.handle_key(code)?;
            assert!(
                app.wintools.pending.is_some(),
                "{code:?} dismissed the tool confirmation, but the dialog does not offer it"
            );
        }
        for code in [KeyCode::Char('n'), KeyCode::Esc] {
            let mut app = App::new(Tree::placeholder(PathBuf::from("root")));
            app.wintools.pending = Some(0);
            app.handle_key(code)?;
            assert!(
                app.wintools.pending.is_none(),
                "{code:?} should cancel the tool confirmation"
            );
        }
        Ok(())
    }

    #[test]
    fn the_delete_dialog_still_cancels_on_the_keys_it_offers() -> Result<()> {
        for code in [
            KeyCode::Char('n'),
            KeyCode::Char('N'),
            KeyCode::Char('q'),
            KeyCode::Esc,
        ] {
            let mut app = app_awaiting_delete_confirmation(true);
            app.handle_key(code)?;
            assert!(
                app.pending_delete.is_none(),
                "{code:?} should cancel the delete confirmation"
            );
        }
        Ok(())
    }
}
