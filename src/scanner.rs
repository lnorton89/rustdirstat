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
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    scan_with_options(root, progress, ScanOptions::default())
}

fn scan_inner(root: &Path, progress: Option<&Progress>, options: ScanOptions) -> Result<Tree> {
    let name = root
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from(root.display().to_string()));

    let meta = std::fs::symlink_metadata(root)?;
    // The device the root lives on. A scan is one filesystem's worth of
    // bytes compared against that filesystem's free space, so entries on
    // other devices (mount points, /proc, /sys, network shares) are left
    // out by default unless `options.same_filesystem_only` is off.
    #[cfg(unix)]
    let root_dev = if options.same_filesystem_only {
        use std::os::unix::fs::MetadataExt;
        Some(meta.dev())
    } else {
        None
    };
    #[cfg(not(unix))]
    let root_dev = {
        let _ = options;
        None
    };

    let root_node = if meta.is_dir() {
        scan_dir(root, name, progress, 0, root_dev)
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
            physical_size: crate::platform::physical_size(&meta, root),
            file_count: 1,
            dir_count: 0,
            modified: meta.modified().ok(),
            children: vec![],
            error: false,
            category,
            ext_totals: vec![],
            unreadable_count: 0,
            file_id: crate::platform::file_id(&meta),
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

/// Whether entries outside `root_dev`'s device are included in the scan.
///
/// The default `scan` keeps the walk on the root's filesystem: a scan
/// answers "what is filling this volume", and a volume's free-space
/// reference only means anything against that one volume — bytes from
/// `/mnt/other`, a USB stick, or a network mount compared against the
/// root volume's free space is a category error, and descending into
/// `/proc` or `/sys` is a pathological walk. `--cross-filesystems` turns
/// the guard off for people who want one merged view.
#[derive(Clone, Copy)]
pub struct ScanOptions {
    pub same_filesystem_only: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            same_filesystem_only: true,
        }
    }
}

/// [`scan`] with non-default options.
pub fn scan_with_options(
    root: &Path,
    progress: Option<&Progress>,
    options: ScanOptions,
) -> Result<Tree> {
    match scan_pool() {
        Some(pool) => pool.install(|| scan_inner(root, progress, options)),
        None => scan_inner(root, progress, options),
    }
}

fn scan_dir(
    path: &Path,
    name: OsString,
    progress: Option<&Progress>,
    depth: usize,
    root_dev: Option<u64>,
) -> Node {
    if depth >= MAX_PARALLEL_DEPTH {
        return scan_dir_deep(path, name, progress, root_dev);
    }

    let dir_meta = std::fs::symlink_metadata(path).ok();
    let Some((mut entries, own_unreadable)) = read_entries(path) else {
        return unreadable_dir(name, dir_meta, progress);
    };
    if let Some(dev) = root_dev {
        entries.retain(|entry| same_filesystem(dev, &entry.metadata));
    }

    let mut local_files = 0u64;
    let mut local_bytes = 0u64;

    // [`read_entries`] has already materialized each entry's name, path,
    // and metadata, so a `DirEntry` never survives into a child scan —
    // holding one keeps the directory's file descriptor open on Unix,
    // and a deep or wide tree of open descriptors can exhaust
    // `RLIMIT_NOFILE`. The only failures left to count here are a
    // directory that cannot be opened at all (handled above) and a
    // per-entry metadata lookup, which `read_entries` counts.
    let scan_one = |entry: EntryInfo, local_files: &mut u64, local_bytes: &mut u64| -> Node {
        let ename = entry.name;
        if entry.metadata.file_type().is_dir() {
            return scan_dir(&entry.path, ename, progress, depth + 1, root_dev);
        }
        let (node, files, bytes) = leaf_node(&entry.metadata, ename, &entry.path);
        *local_files += files;
        *local_bytes += bytes;
        node
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
                    nodes.push(scan_one(entry, &mut lf, &mut lb));
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
            .map(|entry| scan_one(entry, &mut local_files, &mut local_bytes))
            .collect();
        if let Some(p) = progress {
            if local_files > 0 {
                p.files.fetch_add(local_files, Ordering::Relaxed);
                p.bytes.fetch_add(local_bytes, Ordering::Relaxed);
            }
        }
        nodes
    };

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
fn scan_dir_deep(
    path: &Path,
    name: OsString,
    progress: Option<&Progress>,
    root_dev: Option<u64>,
) -> Node {
    struct Frame {
        name: OsString,
        dir_meta: Option<std::fs::Metadata>,
        entries: std::vec::IntoIter<EntryInfo>,
        children: Vec<Node>,
        own_unreadable: u64,
        local_files: u64,
        local_bytes: u64,
    }

    fn open(
        path: PathBuf,
        name: OsString,
        progress: Option<&Progress>,
        root_dev: Option<u64>,
    ) -> Result2 {
        let dir_meta = std::fs::symlink_metadata(&path).ok();
        match read_entries(&path) {
            Some((mut entries, own_unreadable)) => {
                if let Some(dev) = root_dev {
                    entries.retain(|entry| same_filesystem(dev, &entry.metadata));
                }
                Result2::Frame(Box::new(Frame {
                    name,
                    dir_meta,
                    entries: entries.into_iter(),
                    children: Vec::new(),
                    own_unreadable,
                    local_files: 0,
                    local_bytes: 0,
                }))
            }
            None => Result2::Node(unreadable_dir(name, dir_meta, progress)),
        }
    }

    enum Result2 {
        Frame(Box<Frame>),
        Node(Node),
    }

    let mut stack: Vec<Frame> = Vec::new();
    match open(path.to_path_buf(), name, progress, root_dev) {
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

        let ename = entry.name;
        if entry.metadata.file_type().is_dir() {
            match open(entry.path, ename, progress, root_dev) {
                Result2::Frame(child) => stack.push(*child),
                Result2::Node(node) => frame.children.push(node),
            }
            continue;
        }
        let (node, files, bytes) = leaf_node(&entry.metadata, ename, &entry.path);
        frame.local_files += files;
        frame.local_bytes += bytes;
        frame.children.push(node);
    }

    finished.unwrap_or_else(|| unreadable_dir(OsString::new(), None, progress))
}

/// Whether an entry is on the device the scan started on.
///
/// `root_dev` is always `None` on platforms without an inode model, where
/// the guard is simply off.
#[cfg(unix)]
fn same_filesystem(root_dev: u64, meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.dev() == root_dev
}

#[cfg(not(unix))]
fn same_filesystem(_root_dev: u64, _meta: &std::fs::Metadata) -> bool {
    true
}

/// What the scanner actually keeps of a directory entry.
///
/// Deliberately not a [`std::fs::DirEntry`]: Rust documents that a Unix
/// `DirEntry` holds an internal reference to the directory it came from,
/// so retaining entries while recursively scanning their siblings keeps
/// one directory file descriptor open per entry — a deep or wide tree can
/// exhaust `RLIMIT_NOFILE`. The name, path, and metadata are materialized
/// here and the `DirEntry` is dropped before any child scan begins.
struct EntryInfo {
    name: OsString,
    path: PathBuf,
    metadata: std::fs::Metadata,
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
/// The same applies to an entry whose metadata cannot be fetched — it too
/// is counted rather than dropped.
fn read_entries(path: &Path) -> Option<(Vec<EntryInfo>, u64)> {
    let read_dir = std::fs::read_dir(path).ok()?;
    let mut entries = Vec::new();
    let mut unreadable = 0u64;
    for entry in read_dir {
        let Ok(e) = entry else {
            unreadable += 1;
            continue;
        };
        // `DirEntry::metadata` does not follow symlinks, matching what
        // the scan has always assumed of a directory entry.
        let Ok(metadata) = e.metadata() else {
            unreadable += 1;
            continue;
        };
        entries.push(EntryInfo {
            name: e.file_name(),
            path: e.path(),
            metadata,
        });
    }
    Some((entries, unreadable))
}

/// The node standing in for a directory that could not be opened at all.
fn unreadable_dir(
    name: OsString,
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
        file_id: None,
    }
}

/// A non-directory entry, with what it contributes to its parent's
/// running file and byte counts. `path` is only for platforms whose
/// physical size needs the real path (Windows) — Unix reads it straight
/// off the metadata.
fn leaf_node(meta: &std::fs::Metadata, name: OsString, path: &Path) -> (Node, u64, u64) {
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
                file_id: crate::platform::file_id(meta),
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
            physical_size: crate::platform::physical_size(meta, path),
            file_count: 1,
            dir_count: 0,
            modified: meta.modified().ok(),
            children: vec![],
            error: false,
            ext_totals: vec![],
            unreadable_count: 0,
            file_id: crate::platform::file_id(meta),
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
    name: OsString,
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
        file_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;
    use std::fs;

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
                node.name.to_string_lossy(),
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
        let root = scratch_dir("scan", "agree");
        let _ = fs::remove_dir_all(&root);
        build_fixture(&root)?;

        let parallel = scan_dir(&root, OsString::from("root"), None, 0, None);
        let deep = scan_dir_deep(&root, OsString::from("root"), None, None);

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

    /// A name that is not valid UTF-8 survives the scan and reconstructs
    /// to the exact bytes — the whole point of `OsString` names.
    ///
    /// `to_string_lossy` replaces invalid byte sequences with U+FFFD, so
    /// a `String` name could never round-trip: the real file `a\xFFb` and
    /// a file literally named `a\u{FFFD}b` would collapse onto the same
    /// string, and an operation aimed at one could target the other.
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_name_round_trips_through_scan_and_path_for() -> std::io::Result<()> {
        use std::os::unix::ffi::OsStrExt;

        let root = scratch_dir("scan", "nonutf8");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        // The literal bytes a\xFFb — a name no UTF-8 string can express.
        let bad_name = std::ffi::OsStr::from_bytes(b"a\xFFb");
        fs::write(root.join(bad_name), b"payload")?;

        let node = scan_dir(&root, OsString::from("root"), None, 0, None);
        let Some(child) = node.children.first() else {
            return Err(std::io::Error::other("fixture file was not scanned"));
        };
        assert_eq!(
            child.name.as_os_str().as_bytes(),
            b"a\xFFb",
            "the node must keep the exact bytes, not the lossy replacement"
        );

        let tree = Tree {
            root_path: root.clone(),
            root: node,
            volume_free: None,
            volume_total: None,
        };
        assert_eq!(
            tree.path_for(&[0]),
            Some(root.join(bad_name)),
            "the reconstructed path must be the exact real path"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// Two distinct names whose lossy displays are identical stay two
    /// distinct nodes with two distinct filesystem paths.
    #[cfg(unix)]
    #[test]
    fn names_that_collide_when_lossy_stay_distinct() -> std::io::Result<()> {
        use std::os::unix::ffi::OsStrExt;

        let root = scratch_dir("scan", "collide");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        // `a\xFF` (invalid UTF-8) and `a\u{FFFD}` (a real replacement
        // character in a valid name) are different files that print
        // identically after a lossy conversion.
        let invalid = std::ffi::OsStr::from_bytes(b"a\xFF");
        fs::write(root.join(invalid), b"one")?;
        fs::write(root.join("a\u{FFFD}"), b"two")?;

        let node = scan_dir(&root, OsString::from("root"), None, 0, None);
        assert_eq!(
            node.children.len(),
            2,
            "both files must be scanned as separate entries"
        );
        let displays: Vec<String> = node
            .children
            .iter()
            .map(|c| c.name.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            displays,
            ["a\u{FFFD}".to_string(), "a\u{FFFD}".to_string()],
            "the two names are indistinguishable once lossy — which is \
             exactly why the model must not store them lossily"
        );
        assert_ne!(
            node.children[0].name, node.children[1].name,
            "the raw names must still differ"
        );

        let tree = Tree {
            root_path: root.clone(),
            root: node,
            volume_free: None,
            volume_total: None,
        };
        let first = tree
            .path_for(&[0])
            .ok_or_else(|| std::io::Error::other("the first path must resolve"))?;
        let second = tree
            .path_for(&[1])
            .ok_or_else(|| std::io::Error::other("the second path must resolve"))?;
        assert_ne!(first, second, "each path must reach its own file");
        assert!(first.exists(), "the first path must be real");
        assert!(second.exists(), "the second path must be real");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// A scan reaches the bottom of a chain deeper than the parallel
    /// walk may recurse.
    #[test]
    fn a_chain_deeper_than_the_recursion_limit_is_still_scanned() -> std::io::Result<()> {
        let root = scratch_dir("scan", "deep");
        let _ = fs::remove_dir_all(&root);
        let mut chain = root.clone();
        for _ in 0..(MAX_PARALLEL_DEPTH * 3) {
            chain = chain.join("d");
        }
        fs::create_dir_all(&chain)?;
        fs::write(chain.join("bottom.bin"), vec![b'z'; 7])?;

        let tree = scan_dir(&root, OsString::from("root"), None, 0, None);
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
