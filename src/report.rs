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
    out
}

fn write_children(out: &mut String, node: &Node, depth: usize, top: usize, max_depth: usize) {
    if depth >= max_depth {
        return;
    }
    let mut children: Vec<&Node> = node.children.iter().collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
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
            child.name,
            suffix,
            err,
            warn
        );
        if child.is_dir {
            write_children(out, child, depth + 1, top, max_depth);
        }
    }
}
