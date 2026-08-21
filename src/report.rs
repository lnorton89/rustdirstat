// ============================================================================
// Module:       report
// Description:  The plain-text report behind --no-tui: a depth- and count-
//               limited outline of a scanned tree.
//
// Dependencies: crate::model::Node, crate::stats, crate::util::human_bytes
// ============================================================================

//! The plain-text report behind `--no-tui`: a depth- and count-limited
//! outline of a scanned tree, for terminals, pipes, and CI logs.
//!
//! Limited on purpose, which is what separates it from
//! [`crate::csv_export`] — that dumps every node with no cap for other
//! tools to sort and filter, while this is meant to be read by a person.
//!
//! `max_depth` is an explicit parameter rather than something the walk
//! decides for itself, so the output stays bounded and a user-supplied
//! depth never reaches the call stack.

use crate::model::Node;
use crate::stats;
use crate::util::human_bytes;
use std::fmt::Write as _;

pub fn print_report(root_path: &std::path::Path, root: &Node, top: usize, max_depth: usize) {
    print!("{}", build_report(root_path, root, top, max_depth));
}

pub fn write_report_to_file(
    root_path: &std::path::Path,
    root: &Node,
    top: usize,
    max_depth: usize,
    out_path: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::write(out_path, build_report(root_path, root, top, max_depth))
}

fn build_report(root_path: &std::path::Path, root: &Node, top: usize, max_depth: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:>10}  {} ({} files, {} dirs)",
        human_bytes(root.size),
        root_path.display(),
        root.file_count,
        root.dir_count
    );
    if root.error {
        out.push_str("  <access denied>\n");
    }
    if root.unreadable_count > 0 {
        let _ = writeln!(
            out,
            "  warning: {} entries in this subtree could not be read and are excluded from the totals above",
            root.unreadable_count
        );
    }
    write_children(&mut out, root, 0, top, max_depth);

    out.push('\n');
    out.push_str("Extension breakdown:\n");
    let ext_stats = stats::extension_stats(root, false);
    let total = root.size.max(1);
    for stat in ext_stats.iter().take(top) {
        let pct = stat.size as f64 / total as f64 * 100.0;
        let _ = writeln!(
            out,
            "  {:>10}  {:>5.1}%  {:<14} {} files",
            human_bytes(stat.size),
            pct,
            stat.category.label(),
            stat.count
        );
    }
    // Matches write_children's "... and N more" above — a `--top` small
    // enough to also truncate this list (at most Category::COUNT rows, so
    // only ever with an unusually small --top) should say so here too,
    // rather than silently dropping categories with no indication
    // anywhere in the report that something was cut.
    if ext_stats.len() > top {
        let _ = writeln!(out, "  ... and {} more", ext_stats.len() - top);
    }
    out
}

fn write_children(out: &mut String, node: &Node, depth: usize, top: usize, max_depth: usize) {
    if depth >= max_depth {
        return;
    }
    let mut children: Vec<&Node> = node.children.iter().collect();
    children.sort_by_key(|b| std::cmp::Reverse(b.size));
    let total = node.size.max(1);
    let indent = "  ".repeat(depth + 1);

    for (i, child) in children.iter().enumerate() {
        if i >= top {
            let _ = writeln!(out, "{}... and {} more", indent, children.len() - top);
            break;
        }
        let pct = child.size as f64 / total as f64 * 100.0;
        let suffix = if child.is_dir {
            "/"
        } else if child.is_symlink {
            "@"
        } else {
            ""
        };
        let err = if child.error { " <access denied>" } else { "" };
        let warn = if !child.error && child.unreadable_count > 0 {
            format!(" <{} unreadable>", child.unreadable_count)
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "{}{:>10}  {:>5.1}%  {}{}{}{}",
            indent,
            human_bytes(child.size),
            pct,
            child.name.to_string_lossy(),
            suffix,
            err,
            warn
        );
        if child.is_dir {
            write_children(out, child, depth + 1, top, max_depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Category;
    use std::path::Path;

    fn file(name: &str, size: u64) -> Node {
        Node {
            name: std::ffi::OsString::from(name),
            is_dir: false,
            is_symlink: false,
            size,
            physical_size: size,
            file_count: 1,
            dir_count: 0,
            modified: None,
            children: Vec::new(),
            error: false,
            category: Some(Category::NoExtension),
            ext_totals: Vec::new(),
            unreadable_count: 0,
            file_id: None,
            other_filesystem: false,
        }
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let size = children.iter().map(|c| c.size).sum();
        let mut totals = vec![(0_u64, 0_u64, 0_u64); Category::COUNT];
        if let Some(slot) = totals.get_mut(Category::NoExtension.index()) {
            slot.0 = size;
            slot.1 = size;
            slot.2 = children.iter().map(|c| c.file_count).sum();
        }
        Node {
            name: std::ffi::OsString::from(name),
            is_dir: true,
            is_symlink: false,
            size,
            physical_size: size,
            file_count: children.iter().map(|c| c.file_count).sum(),
            dir_count: children.iter().filter(|c| c.is_dir).count() as u64,
            modified: None,
            children,
            error: false,
            category: None,
            ext_totals: totals,
            unreadable_count: 0,
            file_id: None,
            other_filesystem: false,
        }
    }

    /// A chain `d0/d1/d2/.../leaf.bin`, so depth is easy to reason about.
    fn chain(depth: usize) -> Node {
        let mut node = file("leaf.bin", 100);
        for level in (0..depth).rev() {
            node = dir(&format!("d{level}"), vec![node]);
        }
        node
    }

    #[test]
    fn the_depth_limit_stops_where_it_says() {
        let tree = chain(8);
        let shallow = build_report(Path::new("/root"), &tree, 20, 2);
        // `d0` is the root itself, so the listing starts at its child.
        assert!(
            shallow.contains("d1/"),
            "the root's own child should be listed:\n{shallow}"
        );
        assert!(
            shallow.contains("d2/"),
            "a depth of 2 covers two levels:\n{shallow}"
        );
        assert!(!shallow.contains("d3/"), "and stops there:\n{shallow}");

        let deep = build_report(Path::new("/root"), &tree, 20, 8);
        assert!(
            deep.contains("d6/"),
            "a depth of 8 should reach further than a depth of 2:\n{deep}"
        );
    }

    /// Truncating the list says so, rather than quietly showing fewer.
    ///
    /// A report that lists five of forty directories without mentioning
    /// the other thirty-five is worse than one that lists none: it looks
    /// complete.
    #[test]
    fn a_truncated_listing_says_how_many_it_left_out() {
        let children: Vec<Node> = (0..40)
            .map(|i| {
                dir(
                    &format!("sub{i:02}"),
                    vec![file("f.bin", (i as u64 + 1) * 100)],
                )
            })
            .collect();
        let tree = dir("root", children);

        let report = build_report(Path::new("/root"), &tree, 5, 2);
        assert!(
            report.contains("and 35 more"),
            "the report should say what it left out:\n{report}"
        );

        // And the five it kept are the biggest, not the first five.
        assert!(
            report.contains("sub39"),
            "the largest subdirectory should be among those kept:\n{report}"
        );
        assert!(
            !report.contains("sub00"),
            "the smallest should have been dropped:\n{report}"
        );
    }

    /// A listing that fits is not annotated as though it were cut.
    #[test]
    fn a_complete_listing_is_not_marked_as_truncated() {
        let tree = dir(
            "root",
            vec![
                dir("a", vec![file("f.bin", 100)]),
                dir("b", vec![file("g.bin", 200)]),
            ],
        );
        let report = build_report(Path::new("/root"), &tree, 20, 2);
        assert!(
            !report.contains("more"),
            "nothing was left out, so nothing should say so:\n{report}"
        );
    }

    /// The header states the totals the scan found.
    #[test]
    fn the_header_carries_the_totals() {
        let tree = dir(
            "root",
            vec![dir("a", vec![file("f.bin", 1024), file("g.bin", 1024)])],
        );
        let report = build_report(Path::new("/root"), &tree, 20, 2);
        let header = report.lines().next().unwrap_or_default();
        assert!(header.contains("2 files"), "header: {header}");
        assert!(header.contains("1 dir"), "header: {header}");
        assert!(header.contains("KB"), "sizes should be readable: {header}");
    }
}
