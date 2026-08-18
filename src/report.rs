use crate::model::Node;
use crate::stats;
use crate::util::human_bytes;

pub fn print_report(root: &Node, top: usize, max_depth: usize) {
    println!(
        "{:>10}  {} ({} files)",
        human_bytes(root.size),
        root.path.display(),
        root.file_count
    );
    if root.error {
        println!("  <access denied>");
    }
    print_children(root, 0, top, max_depth);

    println!();
    println!("Extension breakdown:");
    let ext_stats = stats::extension_stats(root);
    let total = root.size.max(1);
    for stat in ext_stats.iter().take(top) {
        let pct = stat.size as f64 / total as f64 * 100.0;
        println!(
            "  {:>10}  {:>5.1}%  {:<14} {} files",
            human_bytes(stat.size),
            pct,
            stat.category,
            stat.count
        );
    }
}

fn print_children(node: &Node, depth: usize, top: usize, max_depth: usize) {
    if depth >= max_depth {
        return;
    }
    let mut children: Vec<&Node> = node.children.iter().collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
    let total = node.size.max(1);
    let indent = "  ".repeat(depth + 1);

    for (i, child) in children.iter().enumerate() {
        if i >= top {
            println!("{}... and {} more", indent, children.len() - top);
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
        println!(
            "{}{:>10}  {:>5.1}%  {}{}{}",
            indent,
            human_bytes(child.size),
            pct,
            child.name,
            suffix,
            err
        );
        if child.is_dir {
            print_children(child, depth + 1, top, max_depth);
        }
    }
}
