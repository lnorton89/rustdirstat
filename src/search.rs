// ============================================================================
// Module:       search
// Description:  Recursive name search across a whole subtree: glob by default,
//               full regex behind an re: prefix.
//
// Dependencies: regex; crate::model::Node, crate::color::Category
// ============================================================================

//! Name search across an entire subtree (distinct from the terminal's
//! quick '/' filter, which only narrows the current directory's direct
//! children). Supports glob (`*`, `?`) by default, or a full regex when
//! the query is prefixed with `re:`.
//!
//! Front-end agnostic, and used by both.
//!
//! The walk itself is iterative. "Recursive" in the description above is
//! about what it searches, not how — depth is whatever the user pointed
//! at, and a recursive descent would put it on the call stack.

use crate::color::Category;
use crate::model::Node;
use regex::RegexBuilder;
use std::time::SystemTime;

pub struct SearchHit {
    /// Indices from the directory being browsed down to this entry.
    pub index_path: Vec<usize>,
    pub is_dir: bool,
    pub size: u64,
    pub physical_size: u64,
    pub modified: Option<SystemTime>,
    pub category: Option<Category>,
}

/// Caps how many matches are collected — a broad pattern against a huge
/// tree could otherwise match millions of entries; past this the search
/// stops early rather than building an unusably long, memory-heavy result
/// list. `SearchOutcome::truncated` tells the UI to say so.
const MAX_RESULTS: usize = 2000;

pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
    pub error: Option<String>,
}

pub fn search(node: &Node, query: &str) -> SearchOutcome {
    let (pattern, is_regex) = match query.strip_prefix("re:") {
        Some(rest) => (rest, true),
        None => (query, false),
    };
    if pattern.is_empty() {
        return SearchOutcome {
            hits: vec![],
            truncated: false,
            error: None,
        };
    }

    let regex_source = if is_regex {
        pattern.to_string()
    } else {
        glob_to_regex(pattern)
    };
    let re = match RegexBuilder::new(&regex_source)
        .case_insensitive(true)
        .build()
    {
        Ok(re) => re,
        Err(e) => {
            return SearchOutcome {
                hits: vec![],
                truncated: false,
                error: Some(format!(
                    "invalid {}: {e}",
                    if is_regex { "regex" } else { "pattern" }
                )),
            }
        }
    };

    let mut hits = Vec::new();
    let mut path = Vec::new();
    let mut count = 0usize;
    visit(node, &re, &mut path, &mut hits, &mut count);
    SearchOutcome {
        hits,
        truncated: count > MAX_RESULTS,
        error: None,
    }
}

/// One directory still being walked, and how far through its children
/// the walk has got. See the note on the identical type in
/// [`crate::top_files`] for why the frame holds an index rather than a
/// path.
struct Frame<'a> {
    node: &'a Node,
    next: usize,
}

fn visit(
    root: &Node,
    re: &regex::Regex,
    path: &mut Vec<usize>,
    out: &mut Vec<SearchHit>,
    count: &mut usize,
) {
    let mut stack = vec![Frame {
        node: root,
        next: 0,
    }];

    while let Some(top) = stack.len().checked_sub(1) {
        if *count > MAX_RESULTS {
            return;
        }
        let Some(frame) = stack.get_mut(top) else {
            break;
        };
        let Some(child) = frame.node.children.get(frame.next) else {
            stack.pop();
            // Not for the root frame: it never pushed a segment of its
            // own, because `path` is relative to it.
            if !stack.is_empty() {
                path.pop();
            }
            continue;
        };
        let index = frame.next;
        frame.next += 1;

        path.push(index);
        // Pre-order, as the recursive form was: a directory that matches
        // is recorded before anything inside it.
        if re.is_match(&child.name) {
            *count += 1;
            if out.len() < MAX_RESULTS {
                out.push(SearchHit {
                    index_path: path.clone(),
                    is_dir: child.is_dir,
                    size: child.size,
                    physical_size: child.physical_size,
                    modified: child.modified,
                    category: child.category,
                });
            }
        }
        if child.is_dir {
            // Leave `path` extended; the frame just pushed owns that
            // segment and pops it when it runs out of children.
            stack.push(Frame {
                node: child,
                next: 0,
            });
        } else {
            path.pop();
        }
    }
}

/// Translates a shell-style glob into an equivalent regex.
///
/// Supports the four things people actually type:
///
/// - `*` — any run of characters
/// - `?` — any one character
/// - `[abc]`, `[a-z]`, `[!0-9]` — a character class, negated with `!` or `^`
/// - `{jpg,png}` — alternatives, and they nest
///
/// A backslash escapes the next character, so `\[` matches a literal
/// bracket. Anything else is literal.
///
/// `[...]` and `{...}` used to be escaped into literals, so `*.{jpg,png}`
/// matched nothing and said nothing about why — the pattern is valid, so
/// there was no error to report, only an empty result list. An
/// unterminated `[` or `{` still falls back to a literal rather than
/// becoming an error: half-typed patterns are the normal state of a
/// search box.
fn glob_to_regex(glob: &str) -> String {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = String::with_capacity(glob.len() * 2 + 2);
    out.push('^');
    let mut i = 0;
    let mut open_groups = 0_usize;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                out.push_str(".*");
                i += 1;
            }
            '?' => {
                out.push('.');
                i += 1;
            }
            '\\' if i + 1 < chars.len() => {
                push_literal(&mut out, chars[i + 1]);
                i += 2;
            }
            '[' => match class_end(&chars, i) {
                Some(end) => {
                    push_class(&mut out, &chars[i + 1..end]);
                    i = end + 1;
                }
                None => {
                    push_literal(&mut out, '[');
                    i += 1;
                }
            },
            '{' if group_end(&chars, i).is_some() => {
                out.push_str("(?:");
                open_groups += 1;
                i += 1;
            }
            ',' if open_groups > 0 => {
                out.push('|');
                i += 1;
            }
            '}' if open_groups > 0 => {
                out.push(')');
                open_groups -= 1;
                i += 1;
            }
            c => {
                push_literal(&mut out, c);
                i += 1;
            }
        }
    }
    out.push('$');
    out
}

/// Index of the `]` closing the class opened at `open`, if there is one.
///
/// A `]` in the first position is a literal member of the class, the
/// same rule shells use, so `[]]` matches a single `]`.
fn class_end(chars: &[char], open: usize) -> Option<usize> {
    let mut i = open + 1;
    if matches!(chars.get(i), Some('!') | Some('^')) {
        i += 1;
    }
    if chars.get(i) == Some(&']') {
        i += 1;
    }
    while i < chars.len() {
        if chars[i] == ']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Index of the `}` closing the group opened at `open`, counting nested
/// braces so `{a,{b,c}}` closes at the outer one.
fn group_end(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (i, c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn push_class(out: &mut String, body: &[char]) {
    out.push('[');
    let body = match body.first() {
        Some('!') | Some('^') => {
            out.push('^');
            &body[1..]
        }
        _ => body,
    };
    for &c in body {
        // `-` is left alone so ranges keep working. The rest are escaped
        // because the `regex` crate gives them meaning *inside* a class:
        // `[` opens a nested one, `&&` intersects, `~~` differences.
        if matches!(c, '\\' | ']' | '^' | '[' | '&' | '~') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(']');
}

fn push_literal(out: &mut String, c: char) {
    if matches!(
        c,
        '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
    ) {
        out.push('\\');
    }
    out.push(c);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Matches the way the search box does: case-insensitively, anchored.
    fn matches(glob: &str, name: &str) -> bool {
        let source = glob_to_regex(glob);
        let built = RegexBuilder::new(&source).case_insensitive(true).build();
        assert!(
            built.is_ok(),
            "{glob:?} translated to {source:?}, which is not a valid regex"
        );
        built.map(|re| re.is_match(name)).unwrap_or(false)
    }

    #[test]
    fn stars_and_question_marks_still_mean_what_they_did() {
        assert!(matches("*.iso", "ubuntu.iso"));
        assert!(!matches("*.iso", "ubuntu.iso.part"));
        assert!(matches("photo?.jpg", "photo1.jpg"));
        assert!(!matches("photo?.jpg", "photo12.jpg"));
        assert!(matches("*", "anything at all"));
        // Anchored at both ends: a glob describes the whole name.
        assert!(!matches("iso", "ubuntu.iso"));
        // And case does not matter, the same as the search box.
        assert!(matches("*.ISO", "ubuntu.iso"));
    }

    /// Brace alternation. `*.{jpg,png}` used to match nothing at all,
    /// with no error to explain it.
    #[test]
    fn braces_offer_alternatives() {
        assert!(matches("*.{jpg,png}", "holiday.jpg"));
        assert!(matches("*.{jpg,png}", "holiday.png"));
        assert!(!matches("*.{jpg,png}", "holiday.gif"));
        assert!(matches("report.{doc,docx}", "report.docx"));
        // Nested, closing on the outer brace.
        assert!(matches("*.{jpg,{tar,tar.gz}}", "archive.tar.gz"));
        assert!(matches("*.{jpg,{tar,tar.gz}}", "archive.tar"));
        assert!(!matches("*.{jpg,{tar,tar.gz}}", "archive.zip"));
    }

    /// Character classes, including ranges and negation.
    #[test]
    fn classes_match_one_character_from_a_set() {
        assert!(matches("photo[123].jpg", "photo2.jpg"));
        assert!(!matches("photo[123].jpg", "photo4.jpg"));
        assert!(matches("file[a-f].txt", "filec.txt"));
        assert!(!matches("file[a-f].txt", "filez.txt"));
        assert!(matches("log[!0-9].txt", "logx.txt"));
        assert!(!matches("log[!0-9].txt", "log7.txt"));
        // `^` negates too, the way a shell accepts either spelling.
        assert!(matches("log[^0-9].txt", "logx.txt"));
        assert!(!matches("log[^0-9].txt", "log7.txt"));
        // A class matches exactly one character.
        assert!(!matches("photo[123].jpg", "photo12.jpg"));
    }

    /// Anything the translation cannot make sense of stays literal
    /// rather than becoming an error. A search box spends most of its
    /// time holding a half-typed pattern.
    #[test]
    fn unfinished_and_escaped_patterns_stay_literal() {
        // No closing bracket or brace: the opener is just a character.
        assert!(matches("draft[1.txt", "draft[1.txt"));
        assert!(matches("draft{1.txt", "draft{1.txt"));
        assert!(matches("[", "["));
        assert!(matches("{", "{"));
        // Escaped, so it means itself even though it is well-formed.
        assert!(matches(r"photo\[1\].jpg", "photo[1].jpg"));
        assert!(!matches(r"photo\[1\].jpg", "photo1.jpg"));
        // Regex metacharacters carry no meaning of their own.
        assert!(matches("a+b.txt", "a+b.txt"));
        assert!(!matches("a+b.txt", "aab.txt"));
        assert!(matches("(x).txt", "(x).txt"));
        // `]` first in a class is a literal member, as in a shell.
        assert!(matches("end[]].txt", "end].txt"));
    }
}

#[cfg(test)]
mod walk_tests {
    use super::*;
    use crate::model::fixtures::*;

    /// The depth the stack-overflow test uses. Comfortably past what
    /// a real filesystem allows, and far past what the recursive
    /// version survived in a debug build.
    const DEEP: usize = 60_000;

    /// The walk does not put the tree depth on the call stack.
    ///
    /// It used to call itself once per directory level, and depth is the
    /// user choice, not ours — searching a chain like this overflowed the
    /// stack and took the process with it.
    #[test]
    fn a_tree_far_deeper_than_the_stack_is_still_searched() {
        let root = dir("root", vec![deep_chain(DEEP, 1)]);
        let outcome = search(&root, "buried*");
        assert!(outcome.error.is_none(), "the pattern is valid");
        assert_eq!(outcome.hits.len(), 1, "the one buried file should match");
        let Some(hit) = outcome.hits.first() else {
            return;
        };
        assert_eq!(hit.index_path.len(), DEEP + 2);
    }

    /// Matches come back in pre-order — a directory before its contents —
    /// and every index path leads to the entry it describes.
    ///
    /// The iterative walk owns `path` across the whole traversal, so a
    /// mispaired push and pop would corrupt every path after it without
    /// changing the set of names that matched.
    #[test]
    fn matches_are_pre_order_and_their_paths_lead_to_them() {
        let root = dir(
            "root",
            vec![
                dir(
                    "logs",
                    vec![file("a.log", 1), dir("old", vec![file("b.log", 2)])],
                ),
                file("notes.txt", 3),
                dir("empty", vec![]),
                file("c.log", 4),
            ],
        );

        let outcome = search(&root, "*log*");
        let mut names = Vec::new();
        for hit in &outcome.hits {
            let landed = follow(&root, &hit.index_path);
            assert!(landed.is_some(), "an index path ran off the tree");
            let Some(node) = landed else { return };
            assert_eq!(
                node.is_dir, hit.is_dir,
                "{} was recorded as the wrong kind",
                node.name
            );
            names.push(node.name.clone());
        }
        assert_eq!(
            names,
            ["logs", "a.log", "b.log", "c.log"],
            "the directory should be listed before what is inside it"
        );
    }

    /// A broad pattern stops at the cap and says that it did.
    #[test]
    fn a_pattern_that_matches_everything_stops_at_the_cap() {
        let files: Vec<Node> = (0..MAX_RESULTS + 500)
            .map(|i| file(&format!("f{i}.bin"), 1))
            .collect();
        let outcome = search(&dir("root", files), "*");
        assert_eq!(outcome.hits.len(), MAX_RESULTS);
        assert!(outcome.truncated, "the caller has to be able to say so");
    }
}
