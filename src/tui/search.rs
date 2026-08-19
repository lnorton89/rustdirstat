//! Recursive name search across an entire subtree (distinct from the
//! quick '/' filter, which only narrows the current directory's direct
//! children). Supports glob (`*`, `?`) by default, or a full regex when
//! the query is prefixed with `re:`.

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

fn visit(
    node: &Node,
    re: &regex::Regex,
    path: &mut Vec<usize>,
    out: &mut Vec<SearchHit>,
    count: &mut usize,
) {
    for (i, child) in node.children.iter().enumerate() {
        if *count > MAX_RESULTS {
            return;
        }
        path.push(i);
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
            visit(child, re, path, out, count);
        }
        path.pop();
    }
}

/// Translates a shell-style glob (`*` = any run of characters, `?` = any
/// one character, everything else literal) into an equivalent regex.
fn glob_to_regex(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() * 2 + 2);
    out.push('^');
    for c in glob.chars() {
        match c {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '.' | '+' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push('$');
    out
}
