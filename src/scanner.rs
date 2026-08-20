// ============================================================================
// Module:       scanner
// Description:  The parallel filesystem walk that builds a Tree, rolling each
//               directory's aggregates up bottom-up as it goes.
//
// Dependencies: rayon (dedicated scan pool), anyhow; crate::model::{Node,
//               Tree}, crate::platform
// ============================================================================

//! The parallel filesystem walk that builds a [`Tree`], rolling each
//! directory's aggregates up bottom-up as it goes.
//!
//! This is the one place in the crate that recurses over something
//! tree-sized, and it recurses through rayon rather than the call stack —
//! bounded by what a real path can express, which is the only reason it
//! is acceptable here and nowhere else.
//!
//! Two tuning decisions are worth knowing before changing anything.
//! Directories below `PAR_THRESHOLD` entries are walked on the current
//! thread, because scheduling parallel tasks for a three-entry folder
//! costs more than it saves and most directories are small. And the scan
//! runs on a dedicated pool one thread short of the machine: rayon's
//! global pool takes every core, which left the UI thread fighting the
//! scan for one and made dragging a splitter stutter for the whole of it.
//!
//! Entries that cannot be read are omitted from every total and counted
//! in `unreadable_count` instead, so a partial scan is distinguishable
//! from a small one.

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

/// The thread pool the scan runs on, deliberately one thread short of
/// the machine.
///
/// Rayon's global pool sizes itself to every available core, and a scan
/// saturates all of them. On a large tree that leaves the UI thread
/// fighting the scan for a core, which is what made dragging a splitter
/// or a window stutter while a scan was running — the frame was ready,
/// there was just nowhere to run it. Giving the scan everything *but*
/// one core costs a few percent of scan throughput and buys back a
/// responsive window for the whole of it.
///
/// Falls back to the global pool if a dedicated one cannot be built;
/// a slightly stuttery scan is much better than no scan.
fn scan_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1);
        rayon::ThreadPoolBuilder::new()
            .num_threads(cores.saturating_sub(1).max(1))
            .thread_name(|index| format!("rustdirstat-scan-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

pub fn scan(root: &Path, progress: Option<&Progress>) -> Result<Tree> {
    match scan_pool() {
        Some(pool) => pool.install(|| scan_inner(root, progress)),
        None => scan_inner(root, progress),
    }
}

fn scan_inner(root: &Path, progress: Option<&Progress>) -> Result<Tree> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let meta = std::fs::symlink_metadata(root)?;
    let root_node = if meta.is_dir() {
        scan_dir(root, name, progress, 0)
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

/// How many directory levels the parallel walk may recurse through
/// before it hands off to [`scan_dir_deep`].
///
/// The recursion below is rayon's fork-join, and unwinding it into a
/// work queue would mean rebuilding that parallelism by hand — it is
/// what makes a full-drive scan fast. But recursion depth here is the
/// depth of the user's filesystem, which is theirs to choose, not ours.
///
/// So the two are separated: near the root, where a tree is wide and
/// parallelism is worth having, the walk forks as it always did. Past
/// this depth it switches to an iterative walk on one thread. Very
/// little is lost — a directory chain deep enough to reach this is
/// narrow, which is *why* it is deep, and a narrow chain has no breadth
/// to parallelise. What is gained is a hard ceiling on stack depth that
/// does not depend on what the user points the scanner at.
const MAX_PARALLEL_DEPTH: usize = 64;

fn scan_dir(path: &Path, name: String, progress: Option<&Progress>, depth: usize) -> Node {
    if depth >= MAX_PARALLEL_DEPTH {
        return scan_dir_deep(path, name, progress);
    }

    let dir_meta = std::fs::symlink_metadata(path).ok();
    let Some((entries, mut own_unreadable)) = read_entries(path) else {
        return unreadable_dir(name, dir_meta, progress);
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
            let ename = entry.file_name().to_string_lossy().to_string();
            if meta.file_type().is_dir() {
                return Some(scan_dir(&entry.path(), ename, progress, depth + 1));
            }
            let (node, files, bytes) = leaf_node(&meta, ename);
            *local_files += files;
            *local_bytes += bytes;
            Some(node)
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

    finish_dir(name, dir_meta, children, own_unreadable, progress)
}

/// The same walk as [`scan_dir`], iteratively and on one thread.
///
/// Reached only past [`MAX_PARALLEL_DEPTH`], so this is the tail of a
/// deep, narrow chain. One frame per level being walked, on the heap,
/// which can simply be large.
///
/// Directories are finished bottom-up: a frame whose entries are
/// exhausted is aggregated by the same [`finish_dir`] the recursive
/// walk uses and handed to its parent's child list, so the two produce
/// identical trees.
fn scan_dir_deep(path: &Path, name: String, progress: Option<&Progress>) -> Node {
    struct Frame {
        path: std::path::PathBuf,
        name: String,
        dir_meta: Option<std::fs::Metadata>,
        entries: std::vec::IntoIter<std::fs::DirEntry>,
        children: Vec<Node>,
        own_unreadable: u64,
        local_files: u64,
        local_bytes: u64,
    }

    fn open(path: std::path::PathBuf, name: String, progress: Option<&Progress>) -> Result2 {
        let dir_meta = std::fs::symlink_metadata(&path).ok();
        match read_entries(&path) {
            Some((entries, own_unreadable)) => Result2::Frame(Box::new(Frame {
                path,
                name,
                dir_meta,
                entries: entries.into_iter(),
                children: Vec::new(),
                own_unreadable,
                local_files: 0,
                local_bytes: 0,
            })),
            None => Result2::Node(unreadable_dir(name, dir_meta, progress)),
        }
    }

    enum Result2 {
        Frame(Box<Frame>),
        Node(Node),
    }

    let mut stack: Vec<Frame> = Vec::new();
    match open(path.to_path_buf(), name, progress) {
        Result2::Frame(frame) => stack.push(*frame),
        Result2::Node(node) => return node,
    }

    // Only ever `None` once the root frame has been finished, which is
    // when the loop ends.
    let mut finished: Option<Node> = None;

    while let Some(frame) = stack.last_mut() {
        let Some(entry) = frame.entries.next() else {
            // Every child accounted for: aggregate and hand upward.
            let Some(frame) = stack.pop() else {
                break;
            };
            if let Some(p) = progress {
                if frame.local_files > 0 {
                    p.files.fetch_add(frame.local_files, Ordering::Relaxed);
                    p.bytes.fetch_add(frame.local_bytes, Ordering::Relaxed);
                }
            }
            let node = finish_dir(
                frame.name,
                frame.dir_meta,
                frame.children,
                frame.own_unreadable,
                progress,
            );
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => finished = Some(node),
            }
            continue;
        };

        let Ok(meta) = entry.metadata() else {
            frame.own_unreadable += 1;
            continue;
        };
        let ename = entry.file_name().to_string_lossy().to_string();
        if meta.file_type().is_dir() {
            let child_path = frame.path.join(&ename);
            match open(child_path, ename, progress) {
                Result2::Frame(child) => stack.push(*child),
                Result2::Node(node) => frame.children.push(node),
            }
            continue;
        }
        let (node, files, bytes) = leaf_node(&meta, ename);
        frame.local_files += files;
        frame.local_bytes += bytes;
        frame.children.push(node);
    }

    finished.unwrap_or_else(|| unreadable_dir(String::new(), None, progress))
}

/// Every entry in `path`, plus a count of the ones that could not be
/// read. `None` when the directory itself could not be opened.
///
/// `read_dir` can fail outright (permission denied, etc.) — that's the
/// `None`. But the *iterator it returns* can also yield individual
/// `Err`s partway through (a race with something deleting an entry, a
/// flaky mount) without the whole listing failing. Silently dropping
/// those via `.filter_map(|e| e.ok())` would make a partial listing look
/// identical to a complete one, so they're counted instead of discarded.
fn read_entries(path: &Path) -> Option<(Vec<std::fs::DirEntry>, u64)> {
    let read_dir = std::fs::read_dir(path).ok()?;
    let mut entries = Vec::new();
    let mut unreadable = 0u64;
    for entry in read_dir {
        match entry {
            Ok(e) => entries.push(e),
            Err(_) => unreadable += 1,
        }
    }
    Some((entries, unreadable))
}

/// The node standing in for a directory that could not be opened at all.
fn unreadable_dir(
    name: String,
    dir_meta: Option<std::fs::Metadata>,
    progress: Option<&Progress>,
) -> Node {
    if let Some(p) = progress {
        p.dirs.fetch_add(1, Ordering::Relaxed);
    }
    Node {
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
    }
}

/// A non-directory entry, with what it contributes to its parent's
/// running file and byte counts.
fn leaf_node(meta: &std::fs::Metadata, name: String) -> (Node, u64, u64) {
    if meta.file_type().is_symlink() {
        return (
            Node {
                name,
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
            },
            1,
            0,
        );
    }
    (
        Node {
            category: Some(category_for_name(&name)),
            name,
            is_dir: false,
            is_symlink: false,
            size: meta.len(),
            physical_size: crate::platform::physical_size(meta),
            file_count: 1,
            dir_count: 0,
            modified: meta.modified().ok(),
            children: vec![],
            error: false,
            ext_totals: vec![],
            unreadable_count: 0,
        },
        1,
        meta.len(),
    )
}

/// Rolls a directory's finished children up into the directory itself.
///
/// Shared by both walks, so the parallel and iterative paths cannot
/// drift into aggregating differently.
fn finish_dir(
    name: String,
    dir_meta: Option<std::fs::Metadata>,
    children: Vec<Node>,
    own_unreadable: u64,
    progress: Option<&Progress>,
) -> Node {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "rustdirstat_scan_{}_{name}_{unique}",
            std::process::id()
        ))
    }

    /// A tree with something in it for each branch of the walk: a
    /// directory wide enough to go through rayon, a chain deeper than
    /// the parallel walk is allowed to recurse, files of assorted sizes
    /// and extensions, and an empty directory.
    fn build_fixture(root: &Path) -> std::io::Result<()> {
        fs::create_dir_all(root)?;

        let wide = root.join("wide");
        fs::create_dir_all(&wide)?;
        for i in 0..(PAR_THRESHOLD + 8) {
            fs::write(wide.join(format!("f{i}.txt")), vec![b'x'; i + 1])?;
        }

        // Past MAX_PARALLEL_DEPTH, so a normal scan is forced to hand
        // off to the iterative walk partway down.
        let mut chain = root.join("deep");
        for _ in 0..(MAX_PARALLEL_DEPTH + 16) {
            chain = chain.join("d");
        }
        fs::create_dir_all(&chain)?;
        fs::write(chain.join("bottom.bin"), vec![b'z'; 64])?;

        fs::create_dir_all(root.join("empty"))?;

        let small = root.join("small");
        fs::create_dir_all(&small)?;
        fs::write(small.join("a.rs"), b"fn main() {}")?;
        fs::write(small.join("b.mkv"), vec![b'v'; 4096])?;
        fs::write(small.join("noext"), b"x")?;
        Ok(())
    }

    /// Flattens a tree to a canonical string: one line per node, sorted
    /// by name at each level, carrying everything the UI reads.
    ///
    /// Iterative, because the trees under test are deeper than the walk
    /// that built them was allowed to recurse.
    fn canonical(root: &Node) -> String {
        let mut out = String::new();
        let mut stack = vec![(root, 0_usize)];
        while let Some((node, depth)) = stack.pop() {
            out.push_str(&"  ".repeat(depth));
            out.push_str(&format!(
                "{} dir={} link={} size={} phys={} files={} dirs={} unreadable={} err={}",
                node.name,
                node.is_dir,
                node.is_symlink,
                node.size,
                node.physical_size,
                node.file_count,
                node.dir_count,
                node.unreadable_count,
                node.error,
            ));
            for (i, &(s, p, n)) in node.ext_totals.iter().enumerate() {
                if s != 0 || p != 0 || n != 0 {
                    out.push_str(&format!(" ext{i}=({s},{p},{n})"));
                }
            }
            out.push('\n');

            // Sorted, so the two walks are compared on content rather
            // than on whatever order the filesystem listed entries in.
            // Reversed on push so they come back off in order.
            let mut children: Vec<&Node> = node.children.iter().collect();
            children.sort_by(|a, b| b.name.cmp(&a.name));
            for child in children {
                stack.push((child, depth + 1));
            }
        }
        out
    }

    /// The parallel walk and the deep walk must produce the same tree.
    ///
    /// `scan_dir` forks through rayon near the root and hands off to
    /// `scan_dir_deep` past `MAX_PARALLEL_DEPTH`, so the two run against
    /// the same filesystem on every real scan and any disagreement about
    /// sizes, counts or category totals would show up as a tree that
    /// changes shape depending how deep the thing being scanned sits.
    #[test]
    fn the_parallel_and_deep_walks_agree() -> std::io::Result<()> {
        let root = scratch("agree");
        let _ = fs::remove_dir_all(&root);
        build_fixture(&root)?;

        let parallel = scan_dir(&root, "root".to_string(), None, 0);
        let deep = scan_dir_deep(&root, "root".to_string(), None);

        assert_eq!(
            canonical(&parallel),
            canonical(&deep),
            "the two walks disagree about the same tree"
        );

        // And the totals are actually right, so agreeing on the same
        // wrong answer cannot pass.
        let expected_files: u64 = (PAR_THRESHOLD + 8) as u64 + 1 + 3;
        assert_eq!(
            parallel.file_count, expected_files,
            "every file in the fixture should be counted"
        );
        assert_eq!(
            parallel.unreadable_count, 0,
            "nothing in the fixture is unreadable"
        );

        let wide_bytes: u64 = (1..=(PAR_THRESHOLD + 8) as u64).sum();
        let expected_bytes = wide_bytes + 64 + 12 + 4096 + 1;
        assert_eq!(parallel.size, expected_bytes, "total bytes");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// A scan reaches the bottom of a chain deeper than the parallel
    /// walk may recurse.
    #[test]
    fn a_chain_deeper_than_the_recursion_limit_is_still_scanned() -> std::io::Result<()> {
        let root = scratch("deep");
        let _ = fs::remove_dir_all(&root);
        let mut chain = root.clone();
        for _ in 0..(MAX_PARALLEL_DEPTH * 3) {
            chain = chain.join("d");
        }
        fs::create_dir_all(&chain)?;
        fs::write(chain.join("bottom.bin"), vec![b'z'; 7])?;

        let tree = scan_dir(&root, "root".to_string(), None, 0);
        assert_eq!(tree.file_count, 1, "the file at the bottom should be found");
        assert_eq!(tree.size, 7, "and its bytes counted");
        assert_eq!(
            tree.dir_count,
            (MAX_PARALLEL_DEPTH * 3) as u64,
            "every level should be counted exactly once"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
