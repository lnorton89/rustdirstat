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

use super::search::{self, SearchHit};
use super::top_files::{self, TopFile};
use crate::color::Category;
use crate::config::Config;
use crate::model::{Node, Tree};
use crate::stats::{self, ExtStat};
use anyhow::Result;
use crossterm::event::KeyCode;
use std::cmp::Reverse;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SortMode {
    SizeDesc,
    SizeAsc,
    NameAsc,
    NameDesc,
    ModifiedDesc,
    ModifiedAsc,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::ModifiedDesc,
            SortMode::ModifiedDesc => SortMode::ModifiedAsc,
            SortMode::ModifiedAsc => SortMode::SizeDesc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size desc",
            SortMode::SizeAsc => "size asc",
            SortMode::NameAsc => "name asc",
            SortMode::NameDesc => "name desc",
            SortMode::ModifiedDesc => "newest first",
            SortMode::ModifiedAsc => "oldest first",
        }
    }
}

/// Every user-triggerable operation, so keyboard and mouse input can share
/// one code path instead of duplicating behavior.
#[derive(Clone)]
pub(super) enum Action {
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
pub(super) enum DupRow {
    Header { size: u64, count: usize },
    Member { index_path: Vec<usize> },
}

/// A screen region registered during the last draw that maps a mouse click
/// to an `Action`. Rebuilt every frame in `ui::draw`.
#[derive(Clone)]
pub(super) struct ClickZone {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub action: Action,
}

impl ClickZone {
    pub(super) fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

pub(super) struct PendingDelete {
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
pub(super) struct MoveState {
    pub entry_mode: bool,
    pub destination: String,
}

/// The Windows system-maintenance tools menu (`T`) — present on every
/// platform (see `wintools`'s module doc for why), just reporting every
/// entry as unavailable off Windows rather than not existing.
#[derive(Default)]
pub(super) struct WinToolsState {
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
pub(super) struct SearchState {
    pub query: String,
    pub entry_mode: bool,
    pub visible: bool,
    pub results: Vec<SearchHit>,
    pub truncated: bool,
    pub error: Option<String>,
}

/// The duplicate-files view and the last scan's results.
#[derive(Default)]
pub(super) struct DuplicatesState {
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

pub(super) struct App {
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
    pub(super) fn new(tree: Tree) -> Self {
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
    pub(super) fn apply_config(&mut self, cfg: &Config) {
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
    pub(super) fn to_config(&self) -> Config {
        Config {
            sort: Some(self.sort),
            show_treemap: Some(self.show_treemap),
            treemap_split: Some(self.treemap_split),
            detailed: Some(self.detailed),
            use_physical: Some(self.use_physical),
            ..Config::default()
        }
    }

    /// Restore browsing to whatever directory `target` was pointing at
    /// before a rescan, matching by path since node indices aren't stable
    /// across scans. Falls back to the root if it can't be found (e.g. the
    /// directory was deleted).
    pub(super) fn restore_path(&mut self, target: &std::path::Path) {
        let mut node = &self.tree.root;
        let mut indices = Vec::new();
        let mut current = self.tree.root_path.clone();
        if current == target {
            self.path_indices = indices;
            self.selected = 0;
            self.refresh_ext_stats();
            return;
        }
        loop {
            let mut found = None;
            for (i, c) in node.children.iter().enumerate() {
                let candidate = current.join(&c.name);
                if target == candidate || target.starts_with(&candidate) {
                    found = Some((i, c, candidate));
                    break;
                }
            }
            match found {
                Some((i, c, candidate)) => {
                    indices.push(i);
                    node = c;
                    current = candidate;
                    if current == target {
                        break;
                    }
                }
                None => break,
            }
        }
        self.path_indices = indices;
        self.selected = 0;
        self.refresh_ext_stats();
    }

    pub(super) fn current_node(&self) -> &Node {
        self.tree.node_for(&self.path_indices)
    }

    pub(super) fn current_path(&self) -> PathBuf {
        self.tree.path_for(&self.path_indices)
    }

    /// Children of the current directory, filtered by the active search
    /// term and sorted for display, paired with their index in the
    /// original (unsorted) `children` vec so navigation and deletion stay
    /// stable regardless of sort/filter.
    pub(super) fn display_children(&self) -> Vec<(usize, &Node)> {
        let node = self.current_node();
        let mut v: Vec<(usize, &Node)> = node.children.iter().enumerate().collect();
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            v.retain(|(_, n)| n.name.to_lowercase().contains(&f));
        }
        match self.sort {
            SortMode::SizeDesc => v.sort_by_key(|b| Reverse(b.1.size)),
            SortMode::SizeAsc => v.sort_by_key(|a| a.1.size),
            SortMode::NameAsc => v.sort_by_key(|a| a.1.name.to_lowercase()),
            SortMode::NameDesc => v.sort_by_key(|b| Reverse(b.1.name.to_lowercase())),
            SortMode::ModifiedDesc => v.sort_by_key(|b| Reverse(b.1.modified)),
            SortMode::ModifiedAsc => v.sort_by_key(|a| a.1.modified),
        }
        v
    }

    fn refresh_ext_stats(&mut self) {
        self.ext_stats = stats::extension_stats(self.current_node(), self.use_physical);
    }

    fn refresh_top_files(&mut self) {
        let mut out = top_files::top_k(self.current_node(), TOP_FILES_LIMIT);
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            out.retain(|t| t.name.to_lowercase().contains(&f));
        }
        self.top_files_cache = out;
    }

    fn on_filter_changed(&mut self) {
        self.selected = 0;
        if self.show_top_files {
            self.refresh_top_files();
        }
    }

    /// If the "biggest files" or search-results flat view is active, jump
    /// browsing to the currently selected entry's actual parent directory
    /// (and select it there), then leave the flat view — so every action
    /// that operates on "the selected row" (delete, open, Enter) works the
    /// same regardless of which view found that row.
    fn exit_flat_view_if_needed(&mut self) {
        if self.show_top_files {
            if let Some(tf) = self.top_files_cache.get(self.selected) {
                let idx_path = tf.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.show_top_files = false;
        } else if self.search.visible {
            if let Some(hit) = self.search.results.get(self.selected) {
                let idx_path = hit.index_path.clone();
                self.navigate_to(idx_path);
            }
            self.search.visible = false;
        } else if self.duplicates.visible {
            // Unlike the top-files/search rows above, not every row here is
            // a navigable item — group headers are rows too. Landing on one
            // and leaving `selected` unchanged would carry a duplicates-list
            // row index into the browse view's unrelated child list, so any
            // non-member row resets it instead of leaving it stale.
            match self.duplicates.rows.get(self.selected) {
                Some(DupRow::Member { index_path }) => {
                    let idx_path = index_path.clone();
                    self.navigate_to_absolute(idx_path);
                }
                _ => self.selected = 0,
            }
            self.duplicates.visible = false;
        }
    }

    /// Caps how many duplicate groups are turned into list rows — the
    /// underlying scan can find far more than is sensible to hand to
    /// ratatui's list widget every frame; the most-impactful groups (by
    /// wasted space) are already sorted first, so this only ever drops the
    /// long tail of smaller groups.
    const MAX_DUPLICATE_DISPLAY_GROUPS: usize = 500;

    pub(super) fn set_duplicate_results(&mut self, scan: crate::duplicates::DupScan) {
        let groups = scan.groups;
        self.duplicates.skipped = scan.skipped;
        self.duplicates.group_count = groups.len();
        self.duplicates.truncated = groups.len() > Self::MAX_DUPLICATE_DISPLAY_GROUPS;
        self.duplicates.total_wasted = groups
            .iter()
            .map(|g| g.size * (g.files.len() as u64 - 1))
            .sum();

        let mut rows = Vec::new();
        for group in groups.into_iter().take(Self::MAX_DUPLICATE_DISPLAY_GROUPS) {
            rows.push(DupRow::Header {
                size: group.size,
                count: group.files.len(),
            });
            for f in group.files {
                rows.push(DupRow::Member {
                    index_path: f.index_path,
                });
            }
        }
        self.duplicates.rows = rows;
        self.duplicates.visible = true;
        self.search.visible = false;
        self.show_top_files = false;
        self.selected = 0;
    }

    fn run_subtree_search(&mut self) {
        let outcome = search::search(self.current_node(), &self.search.query);
        self.search.error = outcome.error;
        self.search.truncated = outcome.truncated;
        self.search.results = outcome.hits;
        self.search.visible = true;
        self.search.entry_mode = false;
        self.selected = 0;
    }

    // Every match below is over `crossterm::event::KeyCode`, which has
    // dozens of variants (function keys, media keys, modifier keys, and
    // more added each release). Enumerating them to satisfy
    // `wildcard_enum_match_arm` would be unreadable and would break on
    // every crossterm upgrade, and "any other key does nothing here" is
    // the correct and complete handling. The lint earns its keep on our
    // own enums, not on a foreign keyboard model.
    #[expect(clippy::wildcard_enum_match_arm, reason = "see the comment above")]
    pub(super) fn handle_key(&mut self, code: KeyCode) -> Result<()> {
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        if self.show_properties {
            self.show_properties = false;
            return Ok(());
        }
        if self.wintools.pending.is_some() {
            // Same rule as the delete confirmation below: this dialog
            // offers `[Y]es` and `[N]o`, and a key it does not offer
            // leaves it alone rather than dismissing it.
            let action = if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                Action::ConfirmWinTool
            } else if matches!(
                code,
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc
            ) {
                Action::CancelWinTool
            } else {
                return Ok(());
            };
            return self.dispatch(action);
        }
        if self.wintools.visible {
            match code {
                KeyCode::Esc | KeyCode::Char('T') => self.wintools.visible = false,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.wintools.selected = self.wintools.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.wintools.selected + 1 < crate::wintools::TOOLS.len() {
                        self.wintools.selected += 1;
                    }
                }
                KeyCode::Enter => {
                    return self.dispatch(Action::SelectWinTool(self.wintools.selected))
                }
                _ => {}
            }
            return Ok(());
        }
        if let Some(pending) = &self.pending_delete {
            // Only the keys the dialog actually offers do anything. It
            // used to cancel on *every* other key, so an arrow key, a
            // function key, or a modifier arriving on its own dismissed
            // the confirmation — and the next keystroke, meant for the
            // dialog, went to the file list instead.
            let action = if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                Action::ConfirmDelete
            } else if pending.is_dir && matches!(code, KeyCode::Char('e') | KeyCode::Char('E')) {
                Action::ConfirmEmpty
            } else if matches!(
                code,
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc
            ) {
                Action::CancelDelete
            } else {
                return Ok(());
            };
            return self.dispatch(action);
        }
        if self.filter_mode {
            match code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filter_mode = false;
                    self.on_filter_changed();
                }
                KeyCode::Enter => self.filter_mode = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.on_filter_changed();
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.on_filter_changed();
                }
                _ => {}
            }
            return Ok(());
        }
        if self.search.entry_mode {
            match code {
                KeyCode::Esc => self.search.entry_mode = false,
                KeyCode::Enter => self.run_subtree_search(),
                KeyCode::Backspace => {
                    self.search.query.pop();
                }
                KeyCode::Char(c) => self.search.query.push(c),
                _ => {}
            }
            return Ok(());
        }
        if self.move_to.entry_mode {
            match code {
                KeyCode::Esc => self.move_to.entry_mode = false,
                KeyCode::Enter => self.perform_move(),
                KeyCode::Backspace => {
                    self.move_to.destination.pop();
                }
                KeyCode::Char(c) => self.move_to.destination.push(c),
                _ => {}
            }
            return Ok(());
        }

        let action = match code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => Action::OpenSelected,
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => Action::Back,
            KeyCode::Char('s') => Action::CycleSort,
            KeyCode::Char('t') => Action::ToggleTreemap,
            KeyCode::Char('[') => Action::ShrinkTreemap,
            KeyCode::Char(']') => Action::GrowTreemap,
            KeyCode::Char('d') => Action::RequestDelete,
            KeyCode::Char('D') => Action::RequestDeletePermanent,
            KeyCode::Char('o') => Action::OpenItem,
            KeyCode::Char('O') => Action::OpenInFileManager,
            KeyCode::Char('y') => Action::CopyPath,
            KeyCode::Char('M') => Action::StartMove,
            KeyCode::Char('i') => Action::ToggleProperties,
            KeyCode::Char('T') => Action::ToggleWinTools,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('f') => Action::ToggleTopFiles,
            KeyCode::Char('e') => Action::ExportReport,
            KeyCode::Char('E') => Action::ExportCsv,
            KeyCode::Char('m') => Action::ToggleDetails,
            KeyCode::Char('p') => Action::TogglePhysicalSize,
            KeyCode::Char('?') => Action::ToggleHelp,
            KeyCode::Char('/') => Action::StartFilter,
            KeyCode::Char('S') => Action::StartSubtreeSearch,
            KeyCode::Char('u') => Action::ToggleDuplicates,
            KeyCode::Char('0') => Action::ClearHighlight,
            KeyCode::Char(c @ '1'..='9') => {
                // The pattern already restricts `c` to '1'..='9', so the
                // digit conversion cannot fail — but the crate denies
                // `unwrap`, and an `unwrap` that is only correct because
                // of a guard several lines away is exactly the kind that
                // rots when the guard is edited.
                let Some(digit) = c.to_digit(10) else {
                    return Ok(());
                };
                let idx = digit as usize - 1;
                match self.ext_stats.get(idx) {
                    Some(stat) => Action::ToggleHighlight(stat.category),
                    None => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        self.dispatch(action)
    }

    /// Look up whatever click zone (if any) contains `(x, y)` — the most
    /// recently drawn zone wins, so popups drawn last take priority.
    pub(super) fn handle_click(&mut self, x: u16, y: u16) -> Result<()> {
        if self.show_help {
            self.show_help = false;
            return Ok(());
        }
        if self.show_properties {
            self.show_properties = false;
            return Ok(());
        }
        if let Some(zone) = self.click_zones.iter().rev().find(|z| z.contains(x, y)) {
            let action = zone.action.clone();
            self.dispatch(action)?;
        }
        Ok(())
    }

    /// Recorded every frame by `ui::draw` so a mouse drag position can be
    /// translated into a split percentage.
    pub(super) fn set_body_area(&mut self, x: u16, width: u16) {
        self.body_x = x;
        self.body_width = width;
    }

    /// Called on `MouseEventKind::Drag` while `resizing_treemap` is set.
    pub(super) fn handle_drag(&mut self, x: u16) {
        if !self.resizing_treemap || self.body_width == 0 {
            return;
        }
        let list_w = x.saturating_sub(self.body_x).min(self.body_width);
        let list_pct = (u32::from(list_w) * 100 / u32::from(self.body_width)) as u16;
        let treemap_pct = 100u16.saturating_sub(list_pct);
        self.treemap_split = treemap_pct.clamp(TREEMAP_SPLIT_MIN, TREEMAP_SPLIT_MAX);
    }

    pub(super) fn end_drag(&mut self) {
        self.resizing_treemap = false;
    }

    fn request_delete(&mut self, permanent: bool) {
        self.exit_flat_view_if_needed();
        if let Some((idx, node)) = self.display_children().get(self.selected) {
            self.pending_delete = Some(PendingDelete {
                orig_idx: *idx,
                name: node.name.clone(),
                permanent,
                is_dir: node.is_dir,
            });
        }
    }

    pub(super) fn dispatch(&mut self, action: Action) -> Result<()> {
        if self.pending_delete.is_some() {
            // The wildcard is the point here, and its default is the safe
            // one: while a delete is awaiting confirmation, only an
            // explicit confirmation goes through and literally anything
            // else cancels. Listing the other forty-odd actions would
            // invert that -- a newly added action would fall through to
            // no arm at all and have to be handled by hand, when what it
            // should do is cancel.
            #[expect(clippy::wildcard_enum_match_arm, reason = "see the comment above")]
            match action {
                Action::ConfirmDelete => self.confirm_delete()?,
                Action::ConfirmEmpty => self.confirm_empty()?,
                _ => self.pending_delete = None,
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

    fn export_report(&mut self) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("rustdirstat-report-{secs}.txt");
        let path = self.current_path();
        match crate::report::write_report_to_file(
            &path,
            self.current_node(),
            50,
            4,
            std::path::Path::new(&filename),
        ) {
            Ok(()) => self.message = Some(format!("Report written to {filename}")),
            Err(e) => self.message = Some(format!("Failed to write report: {e}")),
        }
    }

    fn export_csv(&mut self) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("rustdirstat-export-{secs}.csv");
        let path = self.current_path();
        match crate::csv_export::write_csv_to_file(
            &path,
            self.current_node(),
            std::path::Path::new(&filename),
        ) {
            Ok(()) => self.message = Some(format!("CSV written to {filename}")),
            Err(e) => self.message = Some(format!("Failed to write CSV: {e}")),
        }
    }

    fn run_wintool(&mut self, idx: usize) {
        let Some(tool) = crate::wintools::TOOLS.get(idx) else {
            return;
        };
        match crate::wintools::run(idx, &self.tree.root_path) {
            // The TUI has one status line and no scrollback panel to put
            // a report in, so a tool's `detail` is folded onto the end of
            // its summary rather than dropped — truncated by the status
            // line if it does not fit, which still beats discarding the
            // one thing an analyze-only tool was run to produce.
            Ok(output) if output.detail.is_empty() => self.message = Some(output.summary),
            Ok(output) => {
                let detail = output.detail.replace(char::is_whitespace, " ");
                self.message = Some(format!("{}: {}", output.summary, detail.trim()));
            }
            Err(e) => self.message = Some(format!("{}: {e}", tool.name)),
        }
        self.wintools.visible = false;
    }

    /// Moves the selected item to the folder (or exact path) typed into
    /// the move prompt. On success, triggers a full rescan rather than
    /// patching totals in place the way delete does — unlike a delete,
    /// the destination might land inside the currently scanned tree too
    /// (or on a different volume entirely), and correctly reflecting
    /// either case without a real re-scan isn't worth the complexity for
    /// an action this infrequent.
    fn perform_move(&mut self) {
        self.move_to.entry_mode = false;
        let dest_input = self.move_to.destination.trim().to_string();
        if dest_input.is_empty() {
            return;
        }
        let Some((orig_idx, name)) = self
            .display_children()
            .get(self.selected)
            .map(|(idx, n)| (*idx, n.name.clone()))
        else {
            return;
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(orig_idx);
        let source = self.tree.path_for(&full_index_path);

        let dest_base = std::path::PathBuf::from(&dest_input);
        let dest = if dest_base.is_dir() {
            dest_base.join(&name)
        } else {
            dest_base
        };

        match crate::util::move_path(&source, &dest) {
            Ok(()) => {
                self.message = Some(format!("Moved to {}", dest.display()));
                self.refresh_requested = true;
            }
            Err(e) => self.message = Some(format!("Move failed: {e}")),
        }
    }

    /// Jump the browser to the item identified by `index_path` (as produced
    /// by the recursive treemap or the biggest-files view), landing on its
    /// parent directory with the item itself selected.
    fn navigate_to(&mut self, mut index_path: Vec<usize>) {
        if index_path.is_empty() {
            return;
        }
        let target = index_path.remove(index_path.len() - 1);
        self.path_indices.extend(index_path);
        self.selected = 0;
        self.refresh_ext_stats();
        if let Some(pos) = self
            .display_children()
            .iter()
            .position(|(idx, _)| *idx == target)
        {
            self.selected = pos;
        }
    }

    /// Like `navigate_to`, but for an `index_path` rooted at the whole
    /// tree rather than the currently browsed directory — needed for
    /// duplicate results, which are found by scanning from `tree.root`,
    /// not from `current_node()` the way search/top-files results are.
    fn navigate_to_absolute(&mut self, index_path: Vec<usize>) {
        self.path_indices.clear();
        self.navigate_to(index_path);
    }

    fn confirm_delete(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);
        let target = self.tree.node_for(&full_index_path);

        let unreadable_delta = target.unreadable_count;
        let physical_delta = target.physical_size;
        let (is_dir, size, file_count, dir_count_delta, removed_ext) = if target.is_dir {
            (
                true,
                target.size,
                target.file_count,
                target.dir_count + 1,
                RemovedExt::Dir(target.ext_totals.clone()),
            )
        } else {
            (false, target.size, 1, 0, RemovedExt::File(target.category))
        };

        if pending.permanent {
            if is_dir {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        } else {
            trash::delete(&path).map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }

        let mut n = &mut self.tree.root;
        subtract_totals(
            n,
            size,
            physical_delta,
            file_count,
            dir_count_delta,
            unreadable_delta,
            &removed_ext,
        );
        for &idx in &self.path_indices {
            n = &mut n.children[idx];
            subtract_totals(
                n,
                size,
                physical_delta,
                file_count,
                dir_count_delta,
                unreadable_delta,
                &removed_ext,
            );
        }
        n.children.remove(pending.orig_idx);

        let len = self.display_children().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.refresh_ext_stats();
        if self.show_top_files {
            self.refresh_top_files();
        }
        let verb = if pending.permanent {
            "Permanently deleted"
        } else {
            "Moved to trash"
        };
        self.message = Some(format!("{verb}: {}", path.display()));
        Ok(())
    }

    /// Deletes a directory's contents (each direct child moved to trash)
    /// while keeping the directory itself, unlike `confirm_delete` which
    /// removes the node from its parent entirely. The node's own
    /// aggregates are zeroed out in place rather than the node being
    /// removed, so it still shows up afterward — just empty.
    fn confirm_empty(&mut self) -> Result<()> {
        let pending = match self.pending_delete.take() {
            Some(p) => p,
            None => return Ok(()),
        };
        if !pending.is_dir {
            return Ok(());
        }

        let mut full_index_path = self.path_indices.clone();
        full_index_path.push(pending.orig_idx);
        let path = self.tree.path_for(&full_index_path);
        let target = self.tree.node_for(&full_index_path);

        let unreadable_delta = target.unreadable_count;
        let physical_delta = target.physical_size;
        let size = target.size;
        let file_count = target.file_count;
        let dir_count_delta = target.dir_count;
        let removed_ext = RemovedExt::Dir(target.ext_totals.clone());

        let children: Vec<std::path::PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        for child_path in children {
            trash::delete(&child_path)
                .map_err(|e| anyhow::anyhow!("failed to move to trash: {e}"))?;
        }

        let mut n = &mut self.tree.root;
        subtract_totals(
            n,
            size,
            physical_delta,
            file_count,
            dir_count_delta,
            unreadable_delta,
            &removed_ext,
        );
        for &idx in &self.path_indices {
            n = &mut n.children[idx];
            subtract_totals(
                n,
                size,
                physical_delta,
                file_count,
                dir_count_delta,
                unreadable_delta,
                &removed_ext,
            );
        }
        let node = &mut n.children[pending.orig_idx];
        node.size = 0;
        node.physical_size = 0;
        node.file_count = 0;
        node.dir_count = 0;
        node.unreadable_count = 0;
        node.ext_totals = vec![(0u64, 0u64, 0u64); Category::COUNT];
        node.children.clear();

        let len = self.display_children().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.refresh_ext_stats();
        if self.show_top_files {
            self.refresh_top_files();
        }
        self.message = Some(format!("Emptied: {}", path.display()));
        Ok(())
    }
}

enum RemovedExt {
    File(Option<Category>),
    Dir(Vec<(u64, u64, u64)>),
}

fn subtract_totals(
    n: &mut Node,
    size: u64,
    physical_size: u64,
    file_count: u64,
    dir_count: u64,
    unreadable_count: u64,
    ext: &RemovedExt,
) {
    n.size -= size;
    n.physical_size -= physical_size;
    n.file_count -= file_count;
    n.dir_count -= dir_count;
    n.unreadable_count -= unreadable_count;
    match ext {
        RemovedExt::File(Some(cat)) => {
            let i = cat.index();
            n.ext_totals[i].0 -= size;
            n.ext_totals[i].1 -= physical_size;
            n.ext_totals[i].2 -= 1;
        }
        RemovedExt::File(None) => {}
        RemovedExt::Dir(totals) => {
            for (i, &(s, p, c)) in totals.iter().enumerate() {
                n.ext_totals[i].0 -= s;
                n.ext_totals[i].1 -= p;
                n.ext_totals[i].2 -= c;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
