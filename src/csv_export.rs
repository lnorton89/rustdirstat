//! Structured CSV export of a scanned tree, for scripting/spreadsheet use
//! — distinct from the human-oriented text report (`report.rs`), which is
//! depth- and count-limited for readability. This dumps every node with no
//! limit, one row per file/directory, so the output can be sorted,
//! filtered, or diffed by other tools the way WinDirStat's own CSV export
//! is meant to be used.

use crate::model::Node;
use crate::util::format_modified;
use std::path::{Path, PathBuf};

const HEADER: &str = "path,type,size,physical_size,files,dirs,modified,unreadable\n";

pub fn write_csv_to_file(root_path: &Path, root: &Node, out_path: &Path) -> std::io::Result<()> {
    let mut out = String::from(HEADER);
    write_row(&mut out, root_path, root);
    write_children(&mut out, root_path, root);
    std::fs::write(out_path, out)
}

fn write_row(out: &mut String, path: &Path, node: &Node) {
    let kind = if node.is_dir {
        "dir"
    } else if node.is_symlink {
        "symlink"
    } else {
        "file"
    };
    out.push_str(&csv_field(&path.display().to_string()));
    out.push(',');
    out.push_str(kind);
    out.push(',');
    out.push_str(&node.size.to_string());
    out.push(',');
    out.push_str(&node.physical_size.to_string());
    out.push(',');
    out.push_str(&node.file_count.to_string());
    out.push(',');
    out.push_str(&node.dir_count.to_string());
    out.push(',');
    out.push_str(&csv_field(&format_modified(node.modified)));
    out.push(',');
    out.push_str(&node.unreadable_count.to_string());
    out.push('\n');
}

fn write_children(out: &mut String, base: &Path, node: &Node) {
    for child in &node.children {
        let path: PathBuf = base.join(&child.name);
        write_row(out, &path, child);
        if child.is_dir {
            write_children(out, &path, child);
        }
    }
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
