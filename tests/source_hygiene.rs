// ============================================================================
// Module:       source_hygiene (integration test)
// Description:  Guards the one class of string-literal mistake that compiles,
//               formats and lints cleanly while rendering visibly wrong.
//
// Dependencies: std::fs (walks src/ and tests/)
// ============================================================================

//! A multi-line string literal in Rust joins its lines with a trailing
//! `\`. Without it the newline *and the source indentation* become part
//! of the text:
//!
//! ```text
//! "This is how                              much of the total is..."
//! ```
//!
//! That is what the status bar's hard-link tooltip actually said, along
//! with both paragraphs of the Cleanups settings page, a message in the
//! terminal, and five test assertions. Nothing catches it: it is valid
//! Rust, `rustfmt` does not touch the inside of a literal, and clippy has
//! no opinion about prose. It only shows up when somebody looks at the
//! screen.
//!
//! So this looks instead. A run of six or more spaces in the middle of a
//! sentence is not something anyone types on purpose.

use std::path::{Path, PathBuf};

/// Shortest run of spaces treated as a mistake.
///
/// A collapsed continuation leaves at least the source indentation, which
/// in this codebase is never less than twelve columns inside a nested
/// literal. Six is comfortably below that and still far above anything
/// intentional — the widest deliberate gap in ordinary prose is the two
/// spaces some people put after a full stop.
const RUN: usize = 6;

#[test]
fn no_string_literal_carries_a_collapsed_line_continuation() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for dir in ["src", "tests", "examples"] {
        walk(&root.join(dir), &mut |path| {
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            for (index, line) in text.lines().enumerate() {
                if !suspicious(line) {
                    continue;
                }
                let name = path.strip_prefix(root).unwrap_or(path).display();
                offenders.push(format!("{name}:{}", index + 1));
            }
        })?;
    }
    assert!(
        offenders.is_empty(),
        "these lines hold a run of {RUN}+ spaces inside a sentence, which is what a \
         multi-line string literal looks like when it lost the trailing `\\` that \
         joins its lines: {offenders:#?}"
    );
    Ok(())
}

/// Whether one line of source looks like it holds a collapsed
/// continuation.
///
/// Deliberately narrow: the run has to sit *between two words*, so a
/// literal that pads a column (`"// Module:       color"`, the fixtures
/// `header_check` builds) is not flagged, and neither is leading or
/// trailing whitespace, which a literal may legitimately carry.
fn suspicious(line: &str) -> bool {
    // A line of source that is itself a comment cannot hold a broken
    // literal, and comments are where the deliberate column alignment
    // lives. A *literal* that opens with `//` is the same thing one
    // level down — `header_check` builds header banners out of them, and
    // the padding in `"// Module:       color"` is the point.
    let start = line.trim_start();
    if start.starts_with("//") || start.starts_with("\"//") {
        return false;
    }
    let bytes = line.as_bytes();
    let mut run = 0usize;
    let mut before = None;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b' ' {
            if run == 0 {
                before = index.checked_sub(1).and_then(|at| bytes.get(at)).copied();
            }
            run += 1;
            continue;
        }
        let ends_a_word = before.is_some_and(|b| b.is_ascii_alphanumeric() || b",;:.".contains(&b));
        if run >= RUN && ends_a_word && byte.is_ascii_alphanumeric() {
            return true;
        }
        run = 0;
    }
    false
}

/// Every `.rs` file under `dir`.
fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) -> anyhow::Result<()> {
    let mut pending: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            // A directory that is not there — `examples/` in a checkout
            // that has none — is not a failure.
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                visit(&path);
            }
        }
    }
    Ok(())
}
