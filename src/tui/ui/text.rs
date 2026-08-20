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

#[cfg(test)]
mod tests {
    use super::*;

    /// A name with wide glyphs in it. Each CJK character occupies two
    /// terminal columns, which is the whole reason these functions
    /// measure width instead of counting `char`s.
    const WIDE: &str = "画像フォルダ";

    /// Nothing ever comes back wider than it was asked for.
    ///
    /// This is the property the whole module exists for: a label that
    /// overflows its box does not wrap in a terminal, it overwrites the
    /// cell next to it. Counting `char`s instead of columns lets a
    /// CJK name through at twice the width asked for.
    #[test]
    fn nothing_is_ever_wider_than_the_budget() {
        let inputs = [
            "short",
            "a rather long directory name.tar.gz",
            WIDE,
            "mixed 日本語 and latin",
            "",
            "…",
        ];
        for text in inputs {
            for max in 0..=40 {
                let cut = truncate(text, max);
                assert!(
                    cut.width() <= max,
                    "truncate({text:?}, {max}) = {cut:?} at {} columns",
                    cut.width()
                );
                let middle = truncate_middle(text, max);
                assert!(
                    middle.width() <= max.max(1),
                    "truncate_middle({text:?}, {max}) = {middle:?} at {} columns",
                    middle.width()
                );
            }
        }
    }

    /// Text that already fits is returned untouched — no ellipsis bolted
    /// onto something that never needed one.
    #[test]
    fn text_that_fits_is_left_alone() {
        for text in ["short", WIDE, "", "exactly"] {
            let width = text.width();
            assert_eq!(truncate(text, width), text, "truncate at exact width");
            assert_eq!(
                truncate(text, width + 5),
                text,
                "truncate with room to spare"
            );
            assert_eq!(truncate_middle(text, width), text, "middle at exact width");
        }
    }

    /// Truncation says it happened, and keeps a prefix of the original.
    #[test]
    fn a_shortened_string_says_so_and_keeps_its_start() {
        let text = "documents-and-settings";
        let cut = truncate(text, 10);
        assert!(cut.ends_with('…'), "{cut:?} was cut but does not say so");
        let kept: String = cut.chars().take_while(|c| *c != '…').collect();
        assert!(
            text.starts_with(&kept),
            "{cut:?} is not a prefix of {text:?}"
        );
        assert!(
            !kept.is_empty(),
            "10 columns has room for more than the ellipsis"
        );
    }

    /// The middle form keeps both ends, which is the point of it: for a
    /// path, the volume and the leaf are the identifying parts.
    #[test]
    fn the_middle_form_keeps_both_ends() {
        let path = "C:/Users/Lawrence/Documents/Dev/rustdirstat/target/debug";
        let cut = truncate_middle(path, 30);
        assert!(cut.width() <= 30, "{cut:?} is {} columns", cut.width());
        assert!(cut.contains('…'), "{cut:?} should show where it was cut");
        assert!(cut.starts_with("C:/"), "the volume should survive: {cut:?}");
        assert!(cut.ends_with("debug"), "the leaf should survive: {cut:?}");
    }

    /// Wrapping never emits a line wider than the width it was given, and
    /// never loses a word.
    #[test]
    fn wrapping_respects_the_width_and_keeps_every_word() {
        let text = "Delete this folder and everything inside it, permanently";
        for width in [10, 16, 24, 40, 200] {
            let lines = wrap_text(text, width);
            for line in &lines {
                assert!(
                    line.width() <= width || !line.contains(' '),
                    "a {width}-column wrap produced {line:?} at {} columns",
                    line.width()
                );
            }
            let rejoined: Vec<&str> = lines
                .iter()
                .flat_map(|line| line.split_whitespace())
                .collect();
            let original: Vec<&str> = text.split_whitespace().collect();
            assert_eq!(
                rejoined, original,
                "wrapping at {width} lost or reordered words"
            );
        }
    }

    /// A degenerate width does not hang or panic.
    #[test]
    fn wrapping_at_a_useless_width_still_terminates() {
        for width in [0, 1] {
            let lines = wrap_text("some words here", width);
            assert!(
                !lines.is_empty(),
                "wrapping at {width} should still produce something"
            );
        }
    }
}
