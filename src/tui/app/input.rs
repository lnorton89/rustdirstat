// ============================================================================
// Module:       tui::app::input
// Description:  Turning key presses and mouse events into `Action`s, including the
//               click zones the renderer registers each frame.
//
// Dependencies: crossterm (key and mouse events); super::{Action, App}
// ============================================================================

//! Turning key presses and mouse events into `Action`s.
//!
//! Both resolve to the same `Action`, so keyboard and mouse cannot
//! drift apart. Destructive confirmations answer only to the keys they
//! advertise — an unrecognised key leaves the prompt standing.

use super::*;

/// A screen region registered during the last draw that maps a mouse click
/// to an `Action`. Rebuilt every frame in `ui::draw`.
#[derive(Clone)]
pub(in crate::tui) struct ClickZone {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub action: Action,
}

impl ClickZone {
    pub(in crate::tui) fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

impl App {
    // Every match below is over `crossterm::event::KeyCode`, which has
    // dozens of variants (function keys, media keys, modifier keys, and
    // more added each release). Enumerating them to satisfy
    // `wildcard_enum_match_arm` would be unreadable and would break on
    // every crossterm upgrade, and "any other key does nothing here" is
    // the correct and complete handling. The lint earns its keep on our
    // own enums, not on a foreign keyboard model.
    #[expect(clippy::wildcard_enum_match_arm, reason = "see the comment above")]
    pub(in crate::tui) fn handle_key(&mut self, code: KeyCode) -> Result<()> {
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
    pub(in crate::tui) fn handle_click(&mut self, x: u16, y: u16) -> Result<()> {
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
    pub(in crate::tui) fn set_body_area(&mut self, x: u16, width: u16) {
        self.body_x = x;
        self.body_width = width;
    }

    /// Called on `MouseEventKind::Drag` while `resizing_treemap` is set.
    pub(in crate::tui) fn handle_drag(&mut self, x: u16) {
        if !self.resizing_treemap || self.body_width == 0 {
            return;
        }
        let list_w = x.saturating_sub(self.body_x).min(self.body_width);
        let list_pct = (u32::from(list_w) * 100 / u32::from(self.body_width)) as u16;
        let treemap_pct = 100u16.saturating_sub(list_pct);
        self.treemap_split = treemap_pct.clamp(TREEMAP_SPLIT_MIN, TREEMAP_SPLIT_MAX);
    }

    pub(in crate::tui) fn end_drag(&mut self) {
        self.resizing_treemap = false;
    }
}
