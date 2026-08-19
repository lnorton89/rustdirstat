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
            physical_size: crate::platform::physical_size(&meta),
            file_count: 1,
            dir_count: 0,
            modified: meta.modified().ok(),
            children: vec![],
            error: false,
            category,
            ext_totals: vec![],
            unreadable_count: 0,
        }
    };

    let (volume_free, volume_total) = crate::platform::volume_space(root);

    Ok(Tree {
        root_path: root.to_path_buf(),
        root: root_node,
        volume_free,
        volume_total,
    })
}

fn scan_dir(path: &Path, name: String, progress: Option<&Progress>) -> Node {
    let dir_meta = std::fs::symlink_metadata(path).ok();

    // `read_dir` itself can fail (permission denied, etc.) — that's the
    // existing `error` flag. But the *iterator it returns* can also yield
    // individual `Err`s partway through (a race with something deleting an
    // entry, a flaky mount) without the whole listing failing. Silently
    // dropping those via `.filter_map(|e| e.ok())` would make a partial
    // listing look identical to a complete one, so they're counted instead
    // of discarded.
    let mut entries: Vec<std::fs::DirEntry> = Vec::new();
    let mut own_unreadable = 0u64;
    match std::fs::read_dir(path) {
        Ok(rd) => {
            for entry in rd {
                match entry {
                    Ok(e) => entries.push(e),
                    Err(_) => own_unreadable += 1,
                }
            }
        }
        Err(_) => {
            if let Some(p) = progress {
                p.dirs.fetch_add(1, Ordering::Relaxed);
            }
            return Node {
                name,
                is_dir: true,
                is_symlink: false,
                size: 0,
                physical_size: 0,
                file_count: 0,
                dir_count: 0,
                modified: dir_meta.and_then(|m| m.modified().ok()),
                children: vec![],
                error: true,
                category: None,
                ext_totals: vec![(0u64, 0u64, 0u64); Category::COUNT],
                unreadable_count: 1,
            };
        }
    };

    let mut local_files = 0u64;
    let mut local_bytes = 0u64;
    // Shared across both the parallel and sequential branches below — per-
    // entry metadata failures are rare enough that contending one atomic
    // isn't a concern, and it's the simplest way to count them from either
    // branch without threading a second kind of accumulator through both.
    let entry_unreadable = AtomicU64::new(0);

    let scan_one =
        |entry: std::fs::DirEntry, local_files: &mut u64, local_bytes: &mut u64| -> Option<Node> {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    entry_unreadable.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            };
            let ft = meta.file_type();
            let ename = entry.file_name().to_string_lossy().to_string();

            if ft.is_symlink() {
                *local_files += 1;
                Some(Node {
                    name: ename,
                    is_dir: false,
                    is_symlink: true,
                    size: 0,
                    physical_size: 0,
                    file_count: 1,
                    dir_count: 0,
                    modified: meta.modified().ok(),
                    children: vec![],
                    error: false,
                    category: None,
                    ext_totals: vec![],
                    unreadable_count: 0,
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
                    physical_size: crate::platform::physical_size(&meta),
                    file_count: 1,
                    dir_count: 0,
                    modified: meta.modified().ok(),
                    children: vec![],
                    error: false,
                    ext_totals: vec![],
                    unreadable_count: 0,
                })
            }
        };

    let children: Vec<Node> = if entries.len() >= PAR_THRESHOLD {
        // `fold` accumulates files/bytes per rayon work chunk (each chunk
        // covers many entries, processed on one worker thread without
        // touching the shared atomics), and `reduce` merges chunks
        // pairwise — so the whole directory's entries still add up to
        // exactly one or two `fetch_add` calls each, same as the
        // sequential branch below, rather than one per file. A version of
        // this that reset a per-entry counter inside the `filter_map`
        // closure (so every single file did its own `fetch_add`) would
        // silently contradict `Progress`'s own "updated once per
        // directory" doc comment and reintroduce the very atomic
        // contention that comment says was avoided.
        let (nodes, (lf, lb)) = entries
            .into_par_iter()
            .fold(
                || (Vec::new(), (0u64, 0u64)),
                |(mut nodes, (mut lf, mut lb)), entry| {
                    if let Some(node) = scan_one(entry, &mut lf, &mut lb) {
                        nodes.push(node);
                    }
                    (nodes, (lf, lb))
                },
            )
            .reduce(
                || (Vec::new(), (0u64, 0u64)),
                |(mut nodes_a, (lf_a, lb_a)), (nodes_b, (lf_b, lb_b))| {
                    nodes_a.extend(nodes_b);
                    (nodes_a, (lf_a + lf_b, lb_a + lb_b))
                },
            );
        if let Some(p) = progress {
            if lf > 0 {
                p.files.fetch_add(lf, Ordering::Relaxed);
                p.bytes.fetch_add(lb, Ordering::Relaxed);
            }
        }
        nodes
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
    own_unreadable += entry_unreadable.load(Ordering::Relaxed);

    if let Some(p) = progress {
        p.dirs.fetch_add(1, Ordering::Relaxed);
    }

    let mut size = 0u64;
    let mut physical_size = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut unreadable_count = own_unreadable;
    let mut ext_totals = vec![(0u64, 0u64, 0u64); Category::COUNT];

    for c in &children {
        size += c.size;
        physical_size += c.physical_size;
        file_count += c.file_count;
        unreadable_count += c.unreadable_count;
        if c.is_dir {
            dir_count += c.dir_count + 1;
            for (i, &(s, p, n)) in c.ext_totals.iter().enumerate() {
                ext_totals[i].0 += s;
                ext_totals[i].1 += p;
                ext_totals[i].2 += n;
            }
        } else if let Some(cat) = c.category {
            let i = cat.index();
            ext_totals[i].0 += c.size;
            ext_totals[i].1 += c.physical_size;
            ext_totals[i].2 += 1;
        }
    }

    Node {
        name,
        is_dir: true,
        is_symlink: false,
        size,
        physical_size,
        file_count,
        dir_count,
        modified: dir_meta.and_then(|m| m.modified().ok()),
        children,
        error: false,
        category: None,
        ext_totals,
        unreadable_count,
    }
}
