//! Structured CSV export of a scanned tree, for scripting/spreadsheet use
//! — distinct from the human-oriented text report (`report.rs`), which is
//! depth- and count-limited for readability. This dumps every node with no
//! limit, one row per file/directory, so the output can be sorted,
//! filtered, or diffed by other tools the way WinDirStat's own CSV export
//! is meant to be used.
//!
//! "No limit" is the whole point of the format, which is why the writing
//! is streamed and the walk is iterative. A drive-sized scan is millions
//! of nodes: building the text up in a `String` first meant holding the
//! entire export — comfortably past a gigabyte — in memory before a
//! single byte reached the disk, and recursing per directory put the
//! tree's depth on the call stack.

use crate::model::Node;
use crate::util::format_modified;
use std::io::{BufWriter, Write};
use std::path::Path;

const HEADER: &str = "path,type,size,physical_size,files,dirs,modified,unreadable\n";

pub fn write_csv_to_file(root_path: &Path, root: &Node, out_path: &Path) -> std::io::Result<()> {
    let file = std::fs::File::create(out_path)?;
    let mut out = BufWriter::new(file);
    write_csv(&mut out, root_path, root)?;
    // Explicitly, rather than leaving it to `Drop`: a `BufWriter` that
    // fails to flush on drop has nowhere to report it, so the export
    // would look like it succeeded with its tail missing.
    out.flush()
}

/// Writes the whole tree in the same order the recursive version did:
/// each node, then its children, depth first.
fn write_csv<W: Write>(out: &mut W, root_path: &Path, root: &Node) -> std::io::Result<()> {
    out.write_all(HEADER.as_bytes())?;

    // One frame per directory being walked, holding how far through its
    // children we are, plus a single path buffer pushed and popped in
    // step with them. Frames cost depth, not breadth — a stack of
    // (path, node) pairs would be back to holding a `PathBuf` per
    // pending sibling, which for a directory with millions of entries is
    // the memory problem again in a different shape.
    struct Frame<'a> {
        node: &'a Node,
        next: usize,
    }

    let mut path = root_path.to_path_buf();
    write_row(out, &path, root)?;
    let mut stack = vec![Frame {
        node: root,
        next: 0,
    }];

    while let Some(top) = stack.len().checked_sub(1) {
        let Some(frame) = stack.get_mut(top) else {
            break;
        };
        let node = frame.node;
        let Some(child) = node.children.get(frame.next) else {
            stack.pop();
            // Not for the root: its segment is `root_path` itself, which
            // nothing here pushed.
            if !stack.is_empty() {
                path.pop();
            }
            continue;
        };
        frame.next += 1;

        path.push(&child.name);
        write_row(out, &path, child)?;
        if child.is_dir && !child.children.is_empty() {
            // Leave `path` extended; the frame owns that segment now.
            stack.push(Frame {
                node: child,
                next: 0,
            });
        } else {
            path.pop();
        }
    }
    Ok(())
}

fn write_row<W: Write>(out: &mut W, path: &Path, node: &Node) -> std::io::Result<()> {
    let kind = if node.is_dir {
        "dir"
    } else if node.is_symlink {
        "symlink"
    } else {
        "file"
    };
    writeln!(
        out,
        "{},{kind},{},{},{},{},{},{}",
        csv_field(&path.display().to_string()),
        node.size,
        node.physical_size,
        node.file_count,
        node.dir_count,
        csv_field(&format_modified(node.modified)),
        node.unreadable_count
    )
}

/// Quotes a field only when it contains something that would otherwise be
/// ambiguous in CSV (a comma, quote, newline, or carriage return) — file
/// names containing any of those are rare but not disallowed by any
/// filesystem this app targets, so this can't be skipped. `\r` needs the
/// same treatment as `\n`: an unquoted bare CR is still a row terminator
/// to plenty of real-world CSV readers (Excel among them), so leaving one
/// unescaped can split a row and corrupt everything parsed after it.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Node;

    fn node(name: &str, is_dir: bool, children: Vec<Node>) -> Node {
        Node {
            name: name.to_owned(),
            is_dir,
            is_symlink: false,
            size: 1,
            physical_size: 1,
            file_count: u64::from(!is_dir),
            dir_count: 0,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: Vec::new(),
            unreadable_count: 0,
        }
    }

    fn export(root_path: &str, root: &Node) -> String {
        let mut out = Vec::new();
        let result = write_csv(&mut out, Path::new(root_path), root);
        assert!(
            result.is_ok(),
            "the export should not fail writing to a Vec"
        );
        String::from_utf8(out).unwrap_or_default()
    }

    #[test]
    fn every_node_is_written_once_in_depth_first_order() {
        let tree = node(
            "root",
            true,
            vec![
                node(
                    "a",
                    true,
                    vec![node("a1.txt", false, vec![]), node("a2.txt", false, vec![])],
                ),
                node("b.txt", false, vec![]),
            ],
        );
        let csv = export("/base", &tree);
        let paths: Vec<&str> = csv
            .lines()
            .skip(1)
            .filter_map(|line| line.split(',').next())
            .collect();
        let expected = [
            "/base",
            "/base/a",
            "/base/a/a1.txt",
            "/base/a/a2.txt",
            "/base/b.txt",
        ];
        let normalised: Vec<String> = paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert_eq!(
            normalised, expected,
            "paths should come out depth first, each exactly once"
        );
    }

    /// A tree deeper than the call stack would take.
    ///
    /// The walk used to recurse once per directory level, so a deep
    /// enough tree overflowed the stack and took the process down —
    /// during an export, after the scan the user had already waited for.
    #[test]
    fn a_very_deep_tree_exports_without_exhausting_the_stack() {
        // Deeper than any path a filesystem can actually express — an
        // NTFS path caps at 32,767 characters, so even single-character
        // directory names run out well before this.
        const DEPTH: usize = 10_000;
        let mut deepest = node("leaf.txt", false, vec![]);
        for i in 0..DEPTH {
            deepest = node(&format!("d{i}"), true, vec![deepest]);
        }
        let csv = export("/base", &deepest);
        assert_eq!(
            csv.lines().count(),
            // header + the root + one per level below it + the leaf
            1 + 1 + DEPTH,
            "every level should be written exactly once"
        );
    }

    #[test]
    fn names_needing_quotes_get_them() {
        let tree = node(
            "root",
            true,
            vec![
                node("comma,name.txt", false, vec![]),
                node("quote\"", false, vec![]),
            ],
        );
        let csv = export("/base", &tree);
        assert!(
            csv.contains("\"/base/comma,name.txt\"") || csv.contains("\"/base\\comma,name.txt\""),
            "a name containing a comma must be quoted, got:\n{csv}"
        );
        assert!(
            csv.contains("\"\""),
            "an embedded quote must be doubled, got:\n{csv}"
        );
    }
}
