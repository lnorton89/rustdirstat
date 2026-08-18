use crate::color::Category;
use crate::model::{category_for_name, Node, Tree};
use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared, lock-free counters updated as the background scan progresses, so
/// the UI thread can poll them without blocking the scan. Updated once per
/// directory (not once per file) to avoid contending the same cache line
/// from every worker thread on every single entry.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
}

/// Below this many entries, a directory's children are scanned on the
/// current thread instead of being handed to rayon — most directories in a
/// real filesystem are small, and spinning up parallel tasks for a
/// three-entry folder costs more in scheduling overhead than it saves.
const PAR_THRESHOLD: usize = 32;

pub fn scan(root: &Path, progress: Option<&Progress>) -> Result<Tree> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let meta = std::fs::symlink_metadata(root)?;
    let root_node = if meta.is_dir() {
        scan_dir(root, name, progress)
    } else {
        if let Some(p) = progress {
            p.files.fetch_add(1, Ordering::Relaxed);
            p.bytes.fetch_add(meta.len(), Ordering::Relaxed);
        }
        let category = Some(category_for_name(&name));
        Node {
            name,
            is_dir: false,
            is_symlink: meta.file_type().is_symlink(),
            size: meta.len(),
            file_count: 1,
            dir_count: 0,
            modified: meta.modified().ok(),
            children: vec![],
            error: false,
            category,
            ext_totals: vec![],
        }
    };

    Ok(Tree {
        root_path: root.to_path_buf(),
        root: root_node,
    })
}

fn scan_dir(path: &Path, name: String, progress: Option<&Progress>) -> Node {
    let dir_meta = std::fs::symlink_metadata(path).ok();

    let entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => {
            if let Some(p) = progress {
                p.dirs.fetch_add(1, Ordering::Relaxed);
            }
            return Node {
                name,
                is_dir: true,
                is_symlink: false,
                size: 0,
                file_count: 0,
                dir_count: 0,
                modified: dir_meta.and_then(|m| m.modified().ok()),
                children: vec![],
                error: true,
                category: None,
                ext_totals: vec![(0u64, 0u64); Category::COUNT],
            };
        }
    };

    let mut local_files = 0u64;
    let mut local_bytes = 0u64;

    let scan_one =
        |entry: std::fs::DirEntry, local_files: &mut u64, local_bytes: &mut u64| -> Option<Node> {
            let meta = entry.metadata().ok()?;
            let ft = meta.file_type();
            let ename = entry.file_name().to_string_lossy().to_string();

            if ft.is_symlink() {
                *local_files += 1;
                Some(Node {
                    name: ename,
                    is_dir: false,
                    is_symlink: true,
                    size: 0,
                    file_count: 1,
                    dir_count: 0,
                    modified: meta.modified().ok(),
                    children: vec![],
                    error: false,
                    category: None,
                    ext_totals: vec![],
                })
            } else if ft.is_dir() {
                Some(scan_dir(&entry.path(), ename, progress))
            } else {
                *local_files += 1;
                *local_bytes += meta.len();
                Some(Node {
                    category: Some(category_for_name(&ename)),
                    name: ename,
                    is_dir: false,
                    is_symlink: false,
                    size: meta.len(),
                    file_count: 1,
                    dir_count: 0,
                    modified: meta.modified().ok(),
                    children: vec![],
                    error: false,
                    ext_totals: vec![],
                })
            }
        };

    let children: Vec<Node> = if entries.len() >= PAR_THRESHOLD {
        entries
            .into_par_iter()
            .filter_map(|entry| {
                let mut lf = 0;
                let mut lb = 0;
                let node = scan_one(entry, &mut lf, &mut lb);
                if let Some(p) = progress {
                    if lf > 0 {
                        p.files.fetch_add(lf, Ordering::Relaxed);
                        p.bytes.fetch_add(lb, Ordering::Relaxed);
                    }
                }
                node
            })
            .collect()
    } else {
        let nodes: Vec<Node> = entries
            .into_iter()
            .filter_map(|entry| scan_one(entry, &mut local_files, &mut local_bytes))
            .collect();
        if let Some(p) = progress {
            if local_files > 0 {
                p.files.fetch_add(local_files, Ordering::Relaxed);
                p.bytes.fetch_add(local_bytes, Ordering::Relaxed);
            }
        }
        nodes
    };

    if let Some(p) = progress {
        p.dirs.fetch_add(1, Ordering::Relaxed);
    }

    let mut size = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut ext_totals = vec![(0u64, 0u64); Category::COUNT];

    for c in &children {
        size += c.size;
        file_count += c.file_count;
        if c.is_dir {
            dir_count += c.dir_count + 1;
            for (i, &(s, n)) in c.ext_totals.iter().enumerate() {
                ext_totals[i].0 += s;
                ext_totals[i].1 += n;
            }
        } else if let Some(cat) = c.category {
            let i = cat.index();
            ext_totals[i].0 += c.size;
            ext_totals[i].1 += 1;
        }
    }

    Node {
        name,
        is_dir: true,
        is_symlink: false,
        size,
        file_count,
        dir_count,
        modified: dir_meta.and_then(|m| m.modified().ok()),
        children,
        error: false,
        category: None,
        ext_totals,
    }
}
