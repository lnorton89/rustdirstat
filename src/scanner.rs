use crate::model::Node;
use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared, lock-free counters updated as the background scan progresses, so
/// the UI thread can poll them without blocking the scan.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
}

pub fn scan(root: &Path, progress: Option<&Progress>) -> Result<Node> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let meta = std::fs::symlink_metadata(root)?;
    if meta.is_dir() {
        Ok(scan_dir(root, name, progress))
    } else {
        // A single file was passed instead of a directory.
        if let Some(p) = progress {
            p.files.fetch_add(1, Ordering::Relaxed);
            p.bytes.fetch_add(meta.len(), Ordering::Relaxed);
        }
        Ok(Node {
            name,
            path: root.to_path_buf(),
            is_dir: false,
            is_symlink: meta.file_type().is_symlink(),
            size: meta.len(),
            file_count: 1,
            children: vec![],
            error: false,
        })
    }
}

fn scan_dir(path: &Path, name: String, progress: Option<&Progress>) -> Node {
    if let Some(p) = progress {
        p.dirs.fetch_add(1, Ordering::Relaxed);
    }

    let entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => {
            return Node {
                name,
                path: path.to_path_buf(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                file_count: 0,
                children: vec![],
                error: true,
            };
        }
    };

    let children: Vec<Node> = entries
        .into_par_iter()
        .filter_map(|entry| {
            // DirEntry::metadata does not follow symlinks on any platform,
            // so this is safe from symlink cycles by construction.
            let meta = entry.metadata().ok()?;
            let ft = meta.file_type();
            let p = entry.path();
            let ename = entry.file_name().to_string_lossy().to_string();

            if ft.is_symlink() {
                if let Some(pr) = progress {
                    pr.files.fetch_add(1, Ordering::Relaxed);
                }
                Some(Node {
                    name: ename,
                    path: p,
                    is_dir: false,
                    is_symlink: true,
                    size: 0,
                    file_count: 1,
                    children: vec![],
                    error: false,
                })
            } else if ft.is_dir() {
                Some(scan_dir(&p, ename, progress))
            } else {
                if let Some(pr) = progress {
                    pr.files.fetch_add(1, Ordering::Relaxed);
                    pr.bytes.fetch_add(meta.len(), Ordering::Relaxed);
                }
                Some(Node {
                    name: ename,
                    path: p,
                    is_dir: false,
                    is_symlink: false,
                    size: meta.len(),
                    file_count: 1,
                    children: vec![],
                    error: false,
                })
            }
        })
        .collect();

    let size = children.iter().map(|c| c.size).sum();
    let file_count = children.iter().map(|c| c.file_count).sum();

    Node {
        name,
        path: path.to_path_buf(),
        is_dir: true,
        is_symlink: false,
        size,
        file_count,
        children,
        error: false,
    }
}
