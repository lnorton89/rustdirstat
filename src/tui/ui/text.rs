// ============================================================================
// Module:       tui::ui::text
// Description:  Width-aware text trimming, measured in terminal cells rather
//               than chars so wide glyphs count as the two columns they
//               occupy.
//
// Dependencies: unicode-width; super (the shared drawing imports)
// ============================================================================

//! Width-aware text trimming. Terminal cells are counted with
//! `unicode-width` rather than by `char`, so wide glyphs measure as the
//! two columns they actually occupy.

use super::*;

/// Truncates `s` to at most `max` *terminal columns* — not characters.
/// A CJK character, most emoji, and other "wide" codepoints render as 2
/// columns, so measuring/cutting by `chars().count()` can let a label
/// through that's actually up to 2x wider than the caller asked for,
/// overflowing whatever fixed-width box (a treemap tile, the header line)
/// it was sized to fit inside.
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1; // reserve 1 column for the "…"
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > budget {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// Truncates `s` to at most `max` terminal columns by cutting out the
/// middle and joining the head/tail with an ellipsis — for a filesystem
/// path, the drive/volume prefix and the leaf (innermost) directory name
/// are usually the most identifying parts, so keeping both ends and
/// losing the middle preserves more useful information than `truncate`'s
/// plain trailing ellipsis would. Column-width-aware for the same reason
/// as `truncate` above — a path can legitimately contain wide characters.
pub(super) fn truncate_middle(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let budget = max - 1;
    let head_budget = budget.div_ceil(2);
    let tail_budget = budget / 2;
    let chars: Vec<char> = s.chars().collect();

    let mut head = String::new();
    let mut w = 0usize;
    for &ch in &chars {
        let cw = ch.width().unwrap_or(0);
        if w + cw > head_budget {
            break;
        }
        head.push(ch);
        w += cw;
    }

    let mut tail = String::new();
    let mut w = 0usize;
    for &ch in chars.iter().rev() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > tail_budget {
            break;
        }
        tail.insert(0, ch);
        w += cw;
    }

    format!("{head}…{tail}")
}

/// Greedy word-wrap into lines of at most `width` characters. Used
/// instead of `Paragraph::wrap` wherever a popup needs to know its own
/// wrapped row count in advance — to place click zones exactly where
/// text will land (as `draw_wintools_popup` does per-tool) or to size a
/// button row below variable-length text (the confirm popups) — since
/// computing that independently of whatever `Paragraph`'s own wrapping
/// decides would drift out of sync with what's actually on screen.
pub(super) fn wrap_text(s: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![s.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0usize;
    for word in s.split(' ') {
        let word_w = word.width();
        let candidate_w = if current.is_empty() {
            word_w
        } else {
            current_w + 1 + word_w
        };
        if candidate_w > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current_w = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_w += 1;
        }
        current.push_str(word);
        current_w += word_w;
    }
    lines.push(current);
    lines
}
