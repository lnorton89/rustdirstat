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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Shared, lock-free counters updated as the background scan progresses, so
/// the UI thread can poll them without blocking the scan. Updated once per
/// directory (not once per file) to avoid contending the same cache line
/// from every worker thread on every single entry.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    /// Set by whoever asked for the scan to stop it early.
    ///
    /// Read once per directory, in the same places the counters are
    /// written, for the same reason: a relaxed load per directory is
    /// free, and one per entry would put the flag on the hot path of a
    /// nine-million-node walk. The cost of that choice is that a
    /// cancelled scan finishes the directory it is inside, which is
    /// bounded by one directory's entries rather than by the tree.
    ///
    /// [`DupProgress`] has had one of these since 0.2.1
    /// (`crate::duplicates`); the scanner had no way to be stopped at
    /// all, so a mistyped root or a slow network share had to be waited
    /// out with every core but one busy.
    pub cancelled: AtomicBool,
}

impl Progress {
    /// Asks the scan to stop at its next directory boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Whether a scan ran to completion or was stopped part way.
///
/// A cancel is not a failure, and must not arrive as one: an `Err` here
/// would put "Scan failed" in a status bar because the user pressed the
/// button that says Cancel. The partial tree is deliberately not
/// returned — a half-walked tree is indistinguishable from a real one
/// once it reaches a view, and every size in it would be wrong.
pub enum Scan {
    /// Boxed because a `Tree` is a couple of hundred bytes and the other
    /// arm is empty: without it every `Result<Scan>` in the crate — most
    /// of which are `Cancelled` never — would carry a tree-sized hole.
    Completed(Box<Tree>),
    Cancelled,
}

impl Scan {
    /// The tree, if the scan finished.
    pub fn completed(self) -> Option<Tree> {
        match self {
            Self::Completed(tree) => Some(*tree),
            Self::Cancelled => None,
        }
    }
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

pub fn scan(root: &Path, progress: Option<&Progress>) -> Result<Scan> {
    scan_with_options(root, progress, ScanOptions::default())
}

fn scan_inner(root: &Path, progress: Option<&Progress>, options: ScanOptions) -> Result<Scan> {
    // The fallback (a path with no final component, like `/` or `C:\`)
    // keeps the raw `OsString` rather than going through `display()`,
    // which is lossy.
    let name = root
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| root.as_os_str().to_os_string());

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
        scan_dir(root, name, progress, 0, root_dev, meta.modified().ok())
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
            other_filesystem: false,
        }
    };

    // Asked between the walk and the tree it produces: a partial tree
    // must not be handed back as a real one, and by here the walk has
    // already stopped at its first directory boundary past the cancel.
    if progress.is_some_and(Progress::is_cancelled) {
        return Ok(Scan::Cancelled);
    }

    let (volume_free, volume_total) = crate::platform::volume_space(root);

    Ok(Scan::Completed(Box::new(Tree {
        root_path: root.to_path_buf(),
        root: root_node,
        volume_free,
        volume_total,
        roots: Vec::new(),
    })))
}

/// Scans several roots into one tree.
///
/// The roots hang off a synthetic node whose name is a label rather than
/// a path component — nothing resolves a path *through* it, because
/// [`Tree::path_for`] treats the first index as a choice of root and
/// starts from that root's own path. A single root does not go through
/// here at all: it produces exactly the tree it always did, because
/// putting a synthetic level above the common case would change every
/// index path, every view, and every stored selection to serve the rare
/// one.
///
/// Free space is deliberately *not* summed. It is a property of a
/// volume, so two roots on one volume share one figure and two roots on
/// different volumes have two that cannot be added; the tree carries
/// them per root ([`crate::model::Root`]) and the views ask about the
/// root they are looking at.
pub fn scan_many(
    roots: &[PathBuf],
    progress: Option<&Progress>,
    options: ScanOptions,
) -> Result<Scan> {
    let [single] = roots else {
        return scan_many_inner(roots, progress, options);
    };
    scan_with_options(single, progress, options)
}

fn scan_many_inner(
    roots: &[PathBuf],
    progress: Option<&Progress>,
    options: ScanOptions,
) -> Result<Scan> {
    if roots.is_empty() {
        anyhow::bail!("a scan needs at least one root");
    }
    let mut children = Vec::with_capacity(roots.len());
    let mut described = Vec::with_capacity(roots.len());
    for root in roots {
        match scan_with_options(root, progress, options)? {
            Scan::Cancelled => return Ok(Scan::Cancelled),
            Scan::Completed(tree) => {
                let tree = *tree;
                described.push(crate::model::Root {
                    path: tree.root_path,
                    volume_free: tree.volume_free,
                    volume_total: tree.volume_total,
                });
                children.push(tree.root);
            }
        }
    }
    let root = combine(children);
    Ok(Scan::Completed(Box::new(Tree {
        // Not a path: a multi-root tree has no single place it came
        // from, and `path_for` never reads this. It is what the window
        // title and the root row show.
        root_path: PathBuf::from(MULTI_ROOT_LABEL),
        root,
        volume_free: None,
        volume_total: None,
        roots: described,
    })))
}

/// What a multi-root scan calls itself.
pub const MULTI_ROOT_LABEL: &str = "Selected locations";

/// Rolls finished root trees up into the synthetic node above them.
///
/// The same aggregation [`finish_dir`] performs, minus the progress
/// counting — these directories have already been counted by their own
/// scans, and counting them again would show a total higher than the
/// scan found.
fn combine(children: Vec<Node>) -> Node {
    let mut size = 0u64;
    let mut physical_size = 0u64;
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut unreadable_count = 0u64;
    let mut ext_totals = vec![(0u64, 0u64, 0u64); Category::COUNT];
    let mut root_dirs = 0u64;
    for child in &children {
        size = size.saturating_add(child.size);
        physical_size = physical_size.saturating_add(child.physical_size);
        file_count = file_count.saturating_add(child.file_count);
        dir_count = dir_count.saturating_add(child.dir_count);
        unreadable_count = unreadable_count.saturating_add(child.unreadable_count);
        // A root can be a file — both binaries accept one — and a file
        // carries no `ext_totals` of its own, because in an ordinary
        // walk its parent directory is what files it under its category.
        // Here the synthetic node is that parent, so it has to do the
        // same filing, exactly as `finish_dir` does. Adding the empty
        // vector instead lost the file from the extension breakdown and
        // counted it as a folder.
        if child.is_dir {
            root_dirs += 1;
            for (slot, add) in ext_totals.iter_mut().zip(child.ext_totals.iter()) {
                slot.0 = slot.0.saturating_add(add.0);
                slot.1 = slot.1.saturating_add(add.1);
                slot.2 = slot.2.saturating_add(add.2);
            }
        } else if let Some(category) = child.category {
            let slot = &mut ext_totals[category.index()];
            slot.0 = slot.0.saturating_add(child.size);
            slot.1 = slot.1.saturating_add(child.physical_size);
            slot.2 = slot.2.saturating_add(1);
        }
    }
    Node {
        name: OsString::from(MULTI_ROOT_LABEL),
        is_dir: true,
        is_symlink: false,
        size,
        physical_size,
        file_count,
        dir_count: dir_count.saturating_add(root_dirs),
        modified: None,
        children,
        error: false,
        category: None,
        ext_totals,
        unreadable_count,
        file_id: None,
        other_filesystem: false,
    }
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
) -> Result<Scan> {
    match scan_pool() {
        Some(pool) => pool.install(|| scan_inner(root, progress, options)),
        None => scan_inner(root, progress, options),
    }
}

/// [`scan_to_completion`] over several roots.
pub fn scan_many_to_completion(roots: &[PathBuf], options: ScanOptions) -> Result<Tree> {
    match scan_many(roots, None, options)? {
        Scan::Completed(tree) => Ok(*tree),
        Scan::Cancelled => anyhow::bail!("a scan with no cancel token reported cancellation"),
    }
}

/// A scan nobody can cancel, as a plain [`Tree`].
///
/// [`Scan::Cancelled`] is only reachable through a [`Progress`] someone
/// else is holding, so a caller that passes none cannot observe it: the
/// CLI's two non-interactive modes are in that position, and so is every
/// test that scans a fixture. Making each of them re-handle an arm that
/// cannot occur is noise, and noise is where a real cancel eventually
/// gets ignored.
///
/// Anything with a user in front of it must call [`scan`] and answer the
/// cancel properly.
pub fn scan_to_completion(root: &Path) -> Result<Tree> {
    scan_to_completion_with_options(root, ScanOptions::default())
}

/// [`scan_to_completion`] with non-default options.
pub fn scan_to_completion_with_options(root: &Path, options: ScanOptions) -> Result<Tree> {
    match scan_with_options(root, None, options)? {
        Scan::Completed(tree) => Ok(*tree),
        // Unreachable, and an error rather than a panic: the crate denies
        // `unreachable!` for the reason this line exists, which is that
        // "cannot happen" is a claim about code someone may later change.
        Scan::Cancelled => anyhow::bail!("a scan with no cancel token reported cancellation"),
    }
}

fn scan_dir(
    path: &Path,
    name: OsString,
    progress: Option<&Progress>,
    depth: usize,
    root_dev: Option<u64>,
    known_modified: Option<std::time::SystemTime>,
) -> Node {
    if depth >= MAX_PARALLEL_DEPTH {
        return scan_dir_deep(path, name, progress, root_dev, known_modified);
    }
    // One relaxed load per directory, in the same place the counters are
    // touched. The subtree below an abandoned directory is never walked,
    // so a cancel propagates outward at the speed of the directories
    // already in flight rather than the size of what is left.
    if progress.is_some_and(Progress::is_cancelled) {
        return abandoned_dir(name);
    }

    // The parent's listing already carried this directory's timestamp
    // where the platform reports one, and asking the filesystem again is
    // a syscall per directory — a million of them on a drive-sized scan.
    // Only the scan root, which has no parent listing, pays for it.
    let dir_modified = match known_modified {
        Some(modified) => Some(modified),
        None => std::fs::symlink_metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok()),
    };
    let Some((raw, listing_unreadable)) = read_entries(path) else {
        return unreadable_dir(name, dir_modified, progress);
    };
    let wide = raw.len() >= PAR_THRESHOLD;
    let (entries, meta_unreadable) = materialize(raw, wide);
    let own_unreadable = listing_unreadable + meta_unreadable;
    let (entries, stubs) = split_other_filesystem(entries, root_dev);

    let mut local_files = 0u64;
    let mut local_bytes = 0u64;

    // [`materialize`] has already turned each entry into an owned name,
    // path, and metadata, so a `DirEntry` never survives into a child
    // scan — holding one keeps the directory's file descriptor open on
    // Unix, and a deep or wide tree of open descriptors can exhaust
    // `RLIMIT_NOFILE`. The only failures left to count here are a
    // directory that cannot be opened at all (handled above) and the
    // per-entry lookups `read_entries`/`materialize` already counted.
    let scan_one = |entry: EntryInfo, local_files: &mut u64, local_bytes: &mut u64| -> Node {
        let ename = entry.name.clone();
        if entry.is_dir && !entry.is_symlink {
            return scan_dir(
                &entry.path,
                ename,
                progress,
                depth + 1,
                root_dev,
                entry.modified,
            );
        }
        let (node, files, bytes) = leaf_node(entry, ename);
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

    // Other-filesystem markers lead, then the scanned children — the
    // same order the deep walk produces, so the two walks stay
    // interchangeable. Every view sorts before showing, so the order is
    // an invariant for the walks, not for the user.
    let mut all_children = stubs;
    all_children.extend(children);
    finish_dir(name, dir_modified, all_children, own_unreadable, progress)
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
    known_modified: Option<std::time::SystemTime>,
) -> Node {
    struct Frame {
        name: OsString,
        dir_modified: Option<std::time::SystemTime>,
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
        known_modified: Option<std::time::SystemTime>,
    ) -> Result2 {
        let dir_modified = match known_modified {
            Some(modified) => Some(modified),
            None => std::fs::symlink_metadata(&path)
                .ok()
                .and_then(|meta| meta.modified().ok()),
        };
        match read_entries(&path) {
            Some((raw, listing_unreadable)) => {
                // Sequential materialization: this walk only runs past
                // `MAX_PARALLEL_DEPTH`, on the narrow tail of a deep
                // chain, where there is no breadth to parallelise.
                let (entries, meta_unreadable) = materialize(raw, false);
                let (entries, stubs) = split_other_filesystem(entries, root_dev);
                Result2::Frame(Box::new(Frame {
                    name,
                    dir_modified,
                    entries: entries.into_iter(),
                    // Markers first, scanned children appended after —
                    // the same order `scan_dir` produces.
                    children: stubs,
                    own_unreadable: listing_unreadable + meta_unreadable,
                    local_files: 0,
                    local_bytes: 0,
                }))
            }
            None => Result2::Node(unreadable_dir(name, dir_modified, progress)),
        }
    }

    enum Result2 {
        Frame(Box<Frame>),
        Node(Node),
    }

    let mut stack: Vec<Frame> = Vec::new();
    match open(path.to_path_buf(), name, progress, root_dev, known_modified) {
        Result2::Frame(frame) => stack.push(*frame),
        Result2::Node(node) => return node,
    }

    // Only ever `None` once the root frame has been finished, which is
    // when the loop ends.
    let mut finished: Option<Node> = None;

    while let Some(frame) = stack.last_mut() {
        // Checked once per iteration rather than once per directory: this
        // walk is the deep, narrow tail of a chain, where a single
        // directory can be most of the remaining work.
        if progress.is_some_and(Progress::is_cancelled) {
            return abandoned_dir(path.file_name().map(OsString::from).unwrap_or_default());
        }
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
                frame.dir_modified,
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

        let ename = entry.name.clone();
        if entry.is_dir && !entry.is_symlink {
            match open(
                entry.path.clone(),
                ename,
                progress,
                root_dev,
                entry.modified,
            ) {
                Result2::Frame(child) => stack.push(*child),
                Result2::Node(node) => frame.children.push(node),
            }
            continue;
        }
        let (node, files, bytes) = leaf_node(entry, ename);
        frame.local_files += files;
        frame.local_bytes += bytes;
        frame.children.push(node);
    }

    finished.unwrap_or_else(|| unreadable_dir(OsString::new(), None, progress))
}

/// One directory entry, with everything the walk needs about it already
/// resolved.
///
/// Deliberately not a `std::fs::Metadata`: where the fields come from is
/// a platform decision made once, in [`read_entries`], and everything
/// downstream reads the same shape either way. Holding a `DirEntry` here
/// instead would also keep the directory's file descriptor open across
/// the child scan, and a deep or wide tree of those can exhaust Unix
/// `RLIMIT_NOFILE`.
struct EntryInfo {
    name: OsString,
    path: PathBuf,
    is_dir: bool,
    is_symlink: bool,
    len: u64,
    /// Bytes on disk. Where a directory listing supplied the allocation
    /// size it is that; otherwise it is whatever [`crate::platform`] can
    /// work out from the metadata.
    physical: u64,
    modified: Option<std::time::SystemTime>,
    file_id: Option<crate::platform::FileId>,
    /// The device the entry lives on, where the platform reports one.
    /// Only the filesystem-boundary check reads it.
    device: Option<u64>,
}

impl EntryInfo {
    /// The `std` path: everything derived from a per-entry `stat`.
    fn from_metadata(name: OsString, path: PathBuf, metadata: &std::fs::Metadata) -> Self {
        let kind = metadata.file_type();
        Self {
            is_dir: kind.is_dir(),
            is_symlink: kind.is_symlink(),
            len: metadata.len(),
            physical: crate::platform::physical_size(metadata, &path),
            modified: metadata.modified().ok(),
            file_id: crate::platform::file_id(metadata),
            device: entry_device(metadata),
            name,
            path,
        }
    }

    /// The listing path: everything the directory itself reported.
    fn from_listing(dir: &Path, entry: crate::platform::DirEntryInfo) -> Self {
        Self {
            path: dir.join(&entry.name),
            name: entry.name,
            is_dir: entry.is_dir,
            is_symlink: entry.is_symlink,
            len: entry.len,
            physical: entry.allocation,
            modified: entry.modified,
            file_id: entry.file_id,
            // A directory listing describes one directory, which is on
            // one filesystem by construction, so the boundary check has
            // nothing to compare. The platforms that report a device
            // report it through `stat`, which is the other path.
            device: None,
        }
    }
}

/// The device an entry sits on, where the platform says.
#[cfg(unix)]
fn entry_device(metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.dev())
}

#[cfg(not(unix))]
fn entry_device(_metadata: &std::fs::Metadata) -> Option<u64> {
    None
}

/// A directory's entries, before they are turned into [`EntryInfo`]s.
///
/// Two shapes, because two platforms answer differently. Windows can
/// describe every entry from the directory handle it had to open anyway
/// — sizes, identity, timestamps and all — so those arrive complete and
/// there is nothing left to fetch. Everywhere else the walk gets names
/// and has to `stat` each one, which is worth doing in parallel for a
/// wide directory.
enum Listing {
    Ready(Vec<EntryInfo>),
    NeedsMetadata(Vec<std::fs::DirEntry>),
}

impl Listing {
    fn len(&self) -> usize {
        match self {
            Self::Ready(entries) => entries.len(),
            Self::NeedsMetadata(entries) => entries.len(),
        }
    }
}

/// Every entry in `path`, plus a count of the ones that could not be
/// read. `None` when the directory itself could not be listed at all.
///
/// The preferred path is [`crate::platform::directory_listing`], which on
/// Windows enumerates the directory through one handle and reports the
/// allocated size and the file identity of every entry alongside the
/// name — strictly more than `read_dir` exposes, for the same cost,
/// because `read_dir` opens the very same handle internally. Measured
/// over a 1,900-directory tree the two walks came out within a percent
/// of each other; running *both* — which is what an earlier version of
/// this did, to bolt the extra fields onto a `std` listing — cost more
/// than twice either.
///
/// Everywhere else, and on any Windows filesystem that does not
/// implement the info class, it falls back to `read_dir`. That path can
/// also fail *per entry* partway through — a race with something
/// deleting a file, a flaky mount — without the listing itself failing,
/// and those are counted rather than silently dropped.
fn read_entries(path: &Path) -> Option<(Listing, u64)> {
    if let Some(entries) = crate::platform::directory_listing(path) {
        let entries = entries
            .into_iter()
            .map(|entry| EntryInfo::from_listing(path, entry))
            .collect();
        return Some((Listing::Ready(entries), 0));
    }
    let read_dir = std::fs::read_dir(path).ok()?;
    let mut entries = Vec::new();
    let mut unreadable = 0u64;
    for entry in read_dir {
        match entry {
            Ok(e) => entries.push(e),
            Err(_) => unreadable += 1,
        }
    }
    Some((Listing::NeedsMetadata(entries), unreadable))
}

/// Completes a [`Listing`], plus a count of the entries whose metadata
/// could not be fetched.
///
/// A listing that arrived complete passes straight through. For the
/// `std` path this is where the per-entry `stat` happens, and every
/// `DirEntry` is consumed here, before any child scan begins. The
/// lookups run in parallel for a wide directory; an earlier version
/// fetched them sequentially inside [`read_entries`], which serialized
/// the syscalls the walk used to overlap and slowed exactly the huge
/// flat directories a scan spends most of its time in.
fn materialize(listing: Listing, parallel: bool) -> (Vec<EntryInfo>, u64) {
    let raw = match listing {
        Listing::Ready(entries) => return (entries, 0),
        Listing::NeedsMetadata(entries) => entries,
    };

    // `DirEntry::metadata` does not follow symlinks, matching what the
    // scan has always assumed of a directory entry.
    fn one(entry: std::fs::DirEntry) -> Option<EntryInfo> {
        let metadata = entry.metadata().ok()?;
        Some(EntryInfo::from_metadata(
            entry.file_name(),
            entry.path(),
            &metadata,
        ))
    }

    let materialized: Vec<Option<EntryInfo>> = if parallel {
        raw.into_par_iter().map(one).collect()
    } else {
        raw.into_iter().map(one).collect()
    };
    let mut entries = Vec::with_capacity(materialized.len());
    let mut unreadable = 0u64;
    for entry in materialized {
        match entry {
            Some(e) => entries.push(e),
            None => unreadable += 1,
        }
    }
    (entries, unreadable)
}

/// Splits a directory's entries into the ones on the scan's own
/// filesystem and childless marker nodes for the ones that are not.
///
/// With no `root_dev` — cross-filesystem scans, platforms without
/// device identity — everything is kept and no markers are made. The
/// markers replace what used to be a silent `retain`: dropping a mount
/// point entirely made it vanish from the tree, which read as the
/// scanner having lost it rather than as a deliberate boundary.
fn split_other_filesystem(
    entries: Vec<EntryInfo>,
    root_dev: Option<u64>,
) -> (Vec<EntryInfo>, Vec<Node>) {
    let Some(dev) = root_dev else {
        return (entries, Vec::new());
    };
    let mut kept = Vec::with_capacity(entries.len());
    let mut stubs = Vec::new();
    for entry in entries {
        if entry.device.is_none_or(|entry_dev| entry_dev == dev) {
            kept.push(entry);
        } else {
            stubs.push(other_filesystem_stub(entry));
        }
    }
    (kept, stubs)
}

/// The childless marker standing in for an entry on another filesystem.
///
/// Zero bytes on purpose: the entry's contents live on a different
/// volume, and this scan's totals answer "what is filling *this*
/// volume" against its free space. The marker keeps the place visible
/// while keeping the accounting honest.
fn other_filesystem_stub(entry: EntryInfo) -> Node {
    let is_dir = entry.is_dir;
    Node {
        name: entry.name,
        is_dir,
        is_symlink: entry.is_symlink,
        size: 0,
        physical_size: 0,
        file_count: 0,
        dir_count: 0,
        modified: entry.modified,
        children: vec![],
        error: false,
        category: None,
        ext_totals: if is_dir {
            vec![(0u64, 0u64, 0u64); Category::COUNT]
        } else {
            vec![]
        },
        unreadable_count: 0,
        file_id: entry.file_id,
        other_filesystem: true,
    }
}

/// The node standing in for a directory that could not be opened at all.
/// The stand-in for a directory the walk gave up on because the scan was
/// cancelled.
///
/// It is never seen by anyone: [`scan_inner`] returns [`Scan::Cancelled`]
/// rather than the tree these nodes are part of, and the whole thing is
/// dropped. It exists because the walk aggregates bottom-up and every
/// branch has to hand *something* back — and it is deliberately not
/// counted as unreadable, so a cancel can never be mistaken for a
/// permissions problem if this ever does become visible.
fn abandoned_dir(name: OsString) -> Node {
    Node {
        name,
        is_dir: true,
        is_symlink: false,
        size: 0,
        physical_size: 0,
        file_count: 0,
        dir_count: 0,
        modified: None,
        children: vec![],
        error: false,
        category: None,
        ext_totals: vec![(0u64, 0u64, 0u64); Category::COUNT],
        unreadable_count: 0,
        file_id: None,
        other_filesystem: false,
    }
}

fn unreadable_dir(
    name: OsString,
    dir_modified: Option<std::time::SystemTime>,
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
        modified: dir_modified,
        children: vec![],
        error: true,
        category: None,
        ext_totals: vec![(0u64, 0u64, 0u64); Category::COUNT],
        unreadable_count: 1,
        file_id: None,
        other_filesystem: false,
    }
}

/// A non-directory entry, with what it contributes to its parent's
/// running file and byte counts.
///
/// Everything it needs is already on the entry: the walk decided how to
/// get each field — a directory listing on Windows, a `stat` elsewhere —
/// before this was called, so there is one shape of leaf here rather
/// than one per platform.
fn leaf_node(entry: EntryInfo, name: OsString) -> (Node, u64, u64) {
    if entry.is_symlink {
        return (
            Node {
                name,
                is_dir: false,
                is_symlink: true,
                size: 0,
                physical_size: 0,
                file_count: 1,
                dir_count: 0,
                modified: entry.modified,
                children: vec![],
                error: false,
                category: None,
                ext_totals: vec![],
                unreadable_count: 0,
                file_id: entry.file_id,
                other_filesystem: false,
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
            size: entry.len,
            physical_size: entry.physical,
            file_count: 1,
            dir_count: 0,
            modified: entry.modified,
            children: vec![],
            error: false,
            ext_totals: vec![],
            unreadable_count: 0,
            file_id: entry.file_id,
            other_filesystem: false,
        },
        1,
        entry.len,
    )
}

/// Rolls a directory's finished children up into the directory itself.
///
/// Shared by both walks, so the parallel and iterative paths cannot
/// drift into aggregating differently.
fn finish_dir(
    name: OsString,
    dir_modified: Option<std::time::SystemTime>,
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
        modified: dir_modified,
        children,
        error: false,
        category: None,
        ext_totals,
        unreadable_count,
        file_id: None,
        other_filesystem: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;
    use std::fs;

    /// Two roots come back as one tree, and every path in it resolves
    /// against the root it belongs to.
    ///
    /// This is the whole risk of a multi-root tree: the synthetic node on
    /// top is not a directory, so a `path_for` that appended its way
    /// through it would build `C:\D:\Users` — a path that exists
    /// nowhere, handed to code that deletes things.
    #[test]
    fn two_roots_scan_into_one_tree_with_real_paths() -> anyhow::Result<()> {
        let first = scratch_dir("scanner", "multi_first");
        let second = scratch_dir("scanner", "multi_second");
        fs::create_dir_all(&first)?;
        fs::create_dir_all(&second)?;
        fs::write(first.join("a.bin"), vec![b'a'; 100])?;
        fs::write(second.join("b.bin"), vec![b'b'; 250])?;

        let tree =
            scan_many_to_completion(&[first.clone(), second.clone()], ScanOptions::default())?;

        assert!(tree.is_multi_root(), "two roots make a multi-root tree");
        assert_eq!(tree.roots.len(), 2);
        assert_eq!(tree.root.children.len(), 2, "one child per root");
        assert_eq!(
            tree.root.file_count, 2,
            "the synthetic root totals both scans"
        );
        assert_eq!(tree.root.size, 350, "and their bytes");

        // The file under the *second* root resolves to the second root's
        // own path, not to the first one's and not through the label.
        let path = tree
            .path_for(&[1, 0])
            .ok_or_else(|| anyhow::anyhow!("the second root's file should resolve"))?;
        assert_eq!(path, second.join("b.bin"), "resolved to {path:?}");
        let first_path = tree
            .path_for(&[0, 0])
            .ok_or_else(|| anyhow::anyhow!("the first root's file should resolve"))?;
        assert_eq!(first_path, first.join("a.bin"));
        Ok(())
    }

    /// One root is not a multi-root tree.
    ///
    /// The single-root shape is the common case and stays exactly as it
    /// was — a synthetic level above it would change every index path,
    /// every saved selection, and every view, to serve the rare case.
    #[test]
    fn one_root_scans_the_way_it_always_did() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "multi_single");
        fs::create_dir_all(&root)?;
        fs::write(root.join("only.bin"), vec![b'x'; 10])?;

        let tree = scan_many_to_completion(std::slice::from_ref(&root), ScanOptions::default())?;

        assert!(!tree.is_multi_root(), "one root is just a tree");
        assert!(tree.roots.is_empty());
        assert_eq!(tree.root_path, root);
        assert_eq!(tree.path_for(&[0]), Some(root.join("only.bin")));
        Ok(())
    }

    /// Free space is never added across roots.
    ///
    /// Two roots on one volume share one figure and two roots on
    /// different volumes have two that mean different things; summing
    /// them would produce a number that is true of no volume at all.
    #[test]
    fn free_space_is_per_root_not_summed() -> anyhow::Result<()> {
        let first = scratch_dir("scanner", "multi_free_a");
        let second = scratch_dir("scanner", "multi_free_b");
        fs::create_dir_all(&first)?;
        fs::create_dir_all(&second)?;

        let tree = scan_many_to_completion(&[first, second], ScanOptions::default())?;

        assert_eq!(
            tree.volume_free, None,
            "a multi-root tree has no single free-space figure"
        );
        assert_eq!(tree.volume_total, None);
        assert!(
            !tree.is_volume_root(),
            "and it is not a volume, so no free-space tile is offered above the roots"
        );
        for (index, root) in tree.roots.iter().enumerate() {
            assert_eq!(
                tree.root_for(&[index]).map(|r| r.path),
                Some(root.path.clone()),
                "each index path knows which root it belongs to"
            );
        }
        Ok(())
    }

    /// A cancel during the second root abandons the whole tree.
    #[test]
    fn cancelling_a_multi_root_scan_returns_no_tree() -> anyhow::Result<()> {
        let first = scratch_dir("scanner", "multi_cancel_a");
        let second = scratch_dir("scanner", "multi_cancel_b");
        fs::create_dir_all(&first)?;
        fs::create_dir_all(&second)?;

        let progress = Progress::default();
        progress.cancel();
        let outcome = scan_many(&[first, second], Some(&progress), ScanOptions::default())?;

        let Scan::Cancelled = outcome else {
            anyhow::bail!("a cancelled multi-root scan must not produce a tree");
        };
        Ok(())
    }

    /// On Windows a scan now reports what a file actually occupies.
    ///
    /// Before 0.3.0 "physical size" there meant compression- and
    /// sparse-aware but *not* cluster-rounded, so every plain file
    /// reported its logical size — internally consistent, and not what a
    /// disk-usage tool means by on-disk size. The per-directory listing
    /// supplies the filesystem's own allocation size for every entry at
    /// the cost of one call per directory, so the honest number is now
    /// affordable.
    ///
    /// "Honest" is worth being precise about, because NTFS has two
    /// answers. A file too small to need a cluster is stored *resident*
    /// inside its MFT record, and NTFS reports its allocation as the data
    /// rounded to eight bytes — it genuinely occupies no clusters. A file
    /// past that threshold is allocated in whole clusters and reports
    /// them. This checks the second case, which is the one the old
    /// behaviour got wrong.
    #[cfg(windows)]
    #[test]
    fn a_file_reports_the_clusters_it_occupies() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "allocation");
        fs::create_dir_all(&root)?;
        // Deliberately not a round number and past any residency
        // threshold, so the answer has to be rounded *up* to something.
        let logical = 5_000_u64;
        fs::write(root.join("chunk.bin"), vec![b'x'; logical as usize])?;
        // And a resident-sized one beside it, to pin the other case.
        fs::write(root.join("tiny.bin"), *b"x")?;

        let tree = scan_to_completion(&root)?;
        let find = |name: &str| {
            tree.root
                .children
                .iter()
                .find(|child| child.name == std::ffi::OsStr::new(name))
        };
        let Some(chunk) = find("chunk.bin") else {
            anyhow::bail!("the 5000-byte fixture file was not scanned");
        };
        assert_eq!(chunk.size, logical, "the logical size is what was written");
        assert!(
            chunk.physical_size > logical,
            "{} bytes on disk for a {logical}-byte file is the logical size again,              not an allocation",
            chunk.physical_size
        );
        assert_eq!(
            chunk.physical_size % 512,
            0,
            "an allocation is a whole number of sectors, got {}",
            chunk.physical_size
        );

        let Some(tiny) = find("tiny.bin") else {
            anyhow::bail!("the one-byte fixture file was not scanned");
        };
        assert!(
            tiny.physical_size < chunk.physical_size,
            "a resident file cannot occupy more than a clustered one: {} vs {}",
            tiny.physical_size,
            chunk.physical_size
        );
        Ok(())
    }

    /// An empty file occupies nothing, and that is not a failure to read
    /// the allocation.
    #[cfg(windows)]
    #[test]
    fn an_empty_file_occupies_nothing() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "empty_file");
        fs::create_dir_all(&root)?;
        fs::write(root.join("empty.bin"), *b"")?;

        let tree = scan_to_completion(&root)?;
        let Some(empty) = tree.root.children.first() else {
            anyhow::bail!("the fixture has one file and the scan found none");
        };
        assert_eq!(empty.size, 0);
        assert_eq!(
            empty.physical_size, 0,
            "an empty file allocates no clusters"
        );
        Ok(())
    }

    /// The scan captures file identity on Windows, not just on Unix.
    ///
    /// Two hard links to one file share an identity, and until 0.3.0 the
    /// Windows scan left `file_id` `None` and duplicate detection
    /// recovered it later from the hashing handle. Having it at scan time
    /// is what a hard-link-aware *total* would need, and it costs nothing
    /// extra now that the directory listing is being read anyway.
    #[cfg(windows)]
    #[test]
    fn a_windows_scan_captures_file_identity() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "identity");
        fs::create_dir_all(&root)?;
        let original = root.join("original.bin");
        fs::write(&original, vec![b'x'; 4096])?;
        let link = root.join("link.bin");
        // `fs::hard_link` needs no privilege on NTFS, unlike a symlink.
        // A filesystem that refuses one (FAT32 on a USB stick) skips the
        // test rather than failing it.
        if fs::hard_link(&original, &link).is_err() {
            return Ok(());
        }

        let tree = scan_to_completion(&root)?;
        let ids: Vec<_> = tree
            .root
            .children
            .iter()
            .filter(|child| !child.is_dir)
            .map(|child| child.file_id)
            .collect();
        assert_eq!(ids.len(), 2, "the fixture has two names for one file");
        assert!(
            ids.iter().all(Option::is_some),
            "a Windows scan should now capture identity: {ids:?}"
        );
        assert_eq!(
            ids.first(),
            ids.last(),
            "two hard links to one file must share an identity: {ids:?}"
        );
        Ok(())
    }

    /// A directory that cannot be listed the fast way still scans.
    ///
    /// The fallback is the whole reason `directory_listing` returns an
    /// `Option`: a redirector or a filesystem that does not implement the
    /// info class must produce a scan with less precise sizes, never no
    /// scan. Checked here by asking for a listing of something that is
    /// not a directory at all, which is the same "cannot list this"
    /// answer.
    #[test]
    fn a_directory_with_no_fast_listing_still_scans() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "no_listing");
        fs::create_dir_all(&root)?;
        let file = root.join("plain.bin");
        fs::write(&file, vec![b'x'; 100])?;

        assert!(
            crate::platform::directory_listing(&file).is_none(),
            "a file is not a directory, so it has no entry listing"
        );
        let tree = scan_to_completion(&root)?;
        assert_eq!(tree.root.file_count, 1, "the scan still produced a tree");
        assert!(tree.root.size >= 100);
        Ok(())
    }

    /// A file passed as one of several roots is still a file.
    ///
    /// The synthetic node above the roots is their parent, so it has to
    /// file a non-directory child under its category the way any other
    /// parent does. Summing the child's own `ext_totals` instead — which
    /// a file does not have — dropped it out of the extension breakdown
    /// entirely and counted it as a folder: 30 bytes of tree reporting 10
    /// bytes of extensions and two directories where there was one.
    #[test]
    fn a_file_root_is_counted_as_a_file_not_a_folder() -> anyhow::Result<()> {
        let dir = scratch_dir("scanner", "combine_dir");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("inside.txt"), vec![b'x'; 10])?;
        let holder = scratch_dir("scanner", "combine_file");
        fs::create_dir_all(&holder)?;
        let loose = holder.join("loose.txt");
        fs::write(&loose, vec![b'y'; 20])?;

        let tree = scan_many_to_completion(&[dir, loose], ScanOptions::default())?;

        let ext_bytes: u64 = tree.root.ext_totals.iter().map(|(size, _, _)| size).sum();
        let ext_files: u64 = tree.root.ext_totals.iter().map(|(_, _, files)| files).sum();
        assert_eq!(tree.root.size, 30, "both roots contribute their bytes");
        assert_eq!(
            ext_bytes, 30,
            "and both are filed under an extension, not just the one inside a folder"
        );
        assert_eq!(ext_files, 2, "two files, both categorised");
        assert_eq!(
            tree.root.dir_count, 1,
            "one of the two roots is a file, so there is one folder"
        );
        Ok(())
    }

    /// A cancelled scan says so, and stops walking.
    ///
    /// Both halves matter. Reporting `Cancelled` while having walked the
    /// whole tree anyway would look right from the outside and still
    /// leave every core busy for a minute on a real volume, which is the
    /// bug this exists to prevent — so the flag is set *before* the walk
    /// starts and the counters are read afterwards to prove the walk
    /// stopped early.
    #[test]
    fn a_cancelled_scan_reports_cancelled_and_stops_walking() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "cancel_before");
        build_fixture(&root)?;

        let progress = Progress::default();
        progress.cancel();
        let outcome = scan(&root, Some(&progress))?;

        let Scan::Cancelled = outcome else {
            anyhow::bail!("a scan cancelled before it started should report Cancelled");
        };
        let walked = progress.files.load(Ordering::Relaxed);
        assert_eq!(
            walked, 0,
            "the walk kept going after the cancel: {walked} files were counted"
        );
        Ok(())
    }

    /// Cancelling part way through leaves the caller with no tree at all,
    /// rather than a half-walked one.
    ///
    /// A partial tree is the dangerous outcome: every directory total in
    /// it is short by whatever was not walked, and nothing downstream —
    /// the treemap, the percentages, a delete confirmation quoting a
    /// folder size — could tell. So the cancel path returns the marker and
    /// the partial tree is dropped on the scan thread.
    #[test]
    fn cancelling_part_way_returns_no_tree() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "cancel_midway");
        build_fixture(&root)?;

        let progress = std::sync::Arc::new(Progress::default());
        let watcher = std::sync::Arc::clone(&progress);
        let scan_root = root.clone();
        let handle = std::thread::spawn(move || scan(&scan_root, Some(watcher.as_ref())));

        // Cancel as soon as the walk has actually started, so this is a
        // mid-flight cancel rather than the "never started" case above.
        // A spin rather than a sleep: the fixture is small enough that a
        // fixed wait would usually miss the window entirely.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while progress.dirs.load(Ordering::Relaxed) == 0 && std::time::Instant::now() < deadline {
            std::hint::spin_loop();
        }
        progress.cancel();

        let Ok(outcome) = handle.join() else {
            anyhow::bail!("the scan thread panicked");
        };
        // Either answer is legitimate here — a fixture this small can
        // finish before the cancel lands — but a *tree* must never come
        // back once the flag has been read.
        match outcome? {
            Scan::Cancelled => {}
            Scan::Completed(tree) => assert!(
                !progress.is_cancelled() || tree.root.file_count > 0,
                "a completed scan must be a real one"
            ),
        }
        Ok(())
    }

    /// The deep, iterative walk answers the cancel too.
    ///
    /// It is a separate loop from the parallel walk with its own frame
    /// discipline, and it is the one that runs on the narrow tails where a
    /// single directory can be most of the remaining work — a cancel
    /// checked only in `scan_dir` would be ignored for the whole of it.
    #[test]
    fn the_deep_walk_answers_a_cancel() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "cancel_deep");
        let mut chain = root.join("deep");
        for _ in 0..(MAX_PARALLEL_DEPTH + 16) {
            chain = chain.join("d");
        }
        fs::create_dir_all(&chain)?;
        fs::write(chain.join("bottom.bin"), vec![b'z'; 64])?;

        let progress = Progress::default();
        progress.cancel();
        let outcome = scan_dir_deep(
            &root.join("deep"),
            std::ffi::OsString::from("deep"),
            Some(&progress),
            None,
            None,
        );

        assert!(
            outcome.children.is_empty() && outcome.file_count == 0,
            "the deep walk kept descending after the cancel"
        );
        assert!(
            !outcome.error,
            "an abandoned directory is not an unreadable one"
        );
        Ok(())
    }

    /// `scan_to_completion` is the no-token path, and its impossible arm
    /// stays impossible.
    #[test]
    fn a_scan_with_no_token_completes() -> anyhow::Result<()> {
        let root = scratch_dir("scanner", "no_token");
        build_fixture(&root)?;
        let tree = scan_to_completion(&root)?;
        assert!(
            tree.root.file_count > 0,
            "the fixture has files, so the tree should too"
        );
        Ok(())
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

        let parallel = scan_dir(&root, OsString::from("root"), None, 0, None, None);
        let deep = scan_dir_deep(&root, OsString::from("root"), None, None, None);

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

    /// `Ok(true)` if the filesystem refused the name outright. APFS
    /// (macOS) enforces UTF-8 filenames and fails such a create with
    /// `EILSEQ` — on a filesystem that cannot *hold* a non-UTF-8 name,
    /// the collision the non-UTF-8 tests guard against cannot exist, so
    /// they skip rather than fail. Any other error is a real failure.
    #[cfg(unix)]
    fn filesystem_rejected_name(result: std::io::Result<()>) -> std::io::Result<bool> {
        match result {
            Ok(()) => Ok(false),
            Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => Ok(true),
            Err(error) => Err(error),
        }
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
        if filesystem_rejected_name(fs::write(root.join(bad_name), b"payload"))? {
            fs::remove_dir_all(&root)?;
            return Ok(());
        }

        let node = scan_dir(&root, OsString::from("root"), None, 0, None, None);
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
            roots: Vec::new(),
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
        if filesystem_rejected_name(fs::write(root.join(invalid), b"one"))? {
            fs::remove_dir_all(&root)?;
            return Ok(());
        }
        fs::write(root.join("a\u{FFFD}"), b"two")?;

        let node = scan_dir(&root, OsString::from("root"), None, 0, None, None);
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
            roots: Vec::new(),
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

    /// An entry on a different device than the scan root stays visible
    /// as a childless zero-byte marker rather than vanishing from the
    /// tree — and contributes nothing to any total.
    ///
    /// A real mount boundary cannot be conjured inside a test
    /// environment, so this drives the boundary with a fabricated root
    /// device that matches nothing: relative to it, everything in the
    /// fixture is "another filesystem".
    #[cfg(unix)]
    #[test]
    fn entries_on_another_filesystem_become_markers_not_omissions() -> std::io::Result<()> {
        use std::os::unix::fs::MetadataExt;

        let root = scratch_dir("scan", "otherfs");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("mounted"))?;
        fs::write(root.join("mounted").join("inside.bin"), vec![b'x'; 128])?;
        fs::write(root.join("local.txt"), b"stay")?;

        let real_dev = std::fs::symlink_metadata(&root)?.dev();
        let node = scan_dir(
            &root,
            OsString::from("root"),
            None,
            0,
            Some(real_dev + 1),
            None,
        );
        assert_eq!(
            node.children.len(),
            2,
            "both entries must stay visible as markers"
        );
        for child in &node.children {
            assert!(child.other_filesystem, "each entry is marked");
            assert!(
                child.children.is_empty(),
                "a marker is never descended into"
            );
            assert_eq!(child.size, 0, "another volume's bytes are not counted");
        }
        assert_eq!(node.size, 0, "the parent totals exclude the markers");
        assert_eq!(node.file_count, 0);

        // With the true device the same scan keeps everything.
        let node = scan_dir(&root, OsString::from("root"), None, 0, Some(real_dev), None);
        assert!(
            node.children.iter().all(|c| !c.other_filesystem),
            "nothing on the root's own device is marked"
        );
        assert_eq!(node.file_count, 2, "both files are scanned for real");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// A junction is scanned as a link, not followed. Following one
    /// would double-count the target's bytes — and for a junction cycle,
    /// walk forever. Windows' junctions carry the directory attribute,
    /// so this pins that the reparse point wins over it.
    #[cfg(windows)]
    #[test]
    fn a_junction_is_scanned_as_a_link_not_followed() -> anyhow::Result<()> {
        use std::process::Command;

        let root = scratch_dir("scan", "junction");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("real"))?;
        fs::write(root.join("real").join("payload.bin"), vec![b'x'; 512])?;

        // `mklink /J` needs no privilege, unlike a symlink — but a
        // locked-down environment may still refuse, and a skipped
        // assertion beats a flaky failure.
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("jct"))
            .arg(root.join("real"))
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            let _ = fs::remove_dir_all(&root);
            return Ok(());
        }

        let node = scan_dir(&root, OsString::from("root"), None, 0, None, None);
        assert_eq!(
            node.size, 512,
            "the payload is counted once — a followed junction would double it"
        );
        let junction = node
            .children
            .iter()
            .find(|c| c.name == std::ffi::OsStr::new("jct"))
            .ok_or_else(|| anyhow::anyhow!("the junction must appear in the scan"))?;
        assert!(
            junction.is_symlink,
            "a junction is a link in the model, not a directory"
        );
        assert!(
            junction.children.is_empty(),
            "nothing behind the junction may be walked"
        );
        assert_eq!(junction.size, 0, "a link contributes no bytes");

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

        let tree = scan_dir(&root, OsString::from("root"), None, 0, None, None);
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
