// ============================================================================
// Module:       duplicates
// Description:  Exact-duplicate file detection: a same-size prefilter, then
//               BLAKE3 hashing of whatever survives it.
//
// Dependencies: blake3 (content hashing), rayon (parallel hashing);
//               crate::model::{Node, Tree}
// ============================================================================

//! Duplicate file detection: files with a matching size are hashed
//! (BLAKE3) and grouped so exact-duplicate groups can be shown, the way
//! WinDirStat's Duplicate Files view does. Two-phase because hashing is
//! comparatively expensive — a same-size prefilter (free, since sizes are
//! already known from the scan) rules out the overwhelming majority of
//! files before any file content is actually read from disk.

use crate::model::{Node, Tree};
use crate::platform::FileId;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

#[derive(Default)]
pub struct DupProgress {
    /// Set once the cheap same-size prefilter finishes and hashing starts.
    pub candidates_total: AtomicU64,
    pub hashed: AtomicU64,
    /// Checked between candidates so a user cancel stops new hashing
    /// promptly instead of running the whole candidate list to completion
    /// (hashing file content is the expensive part; the size-prefilter
    /// phase that runs before this is checked is fast enough not to need
    /// its own cancellation point).
    pub cancelled: AtomicBool,
}

pub struct DupFile {
    pub index_path: Vec<usize>,
    /// The file's identity — (device, inode) on Unix. Hard links to the
    /// same file share one, which is what lets a group of same-content
    /// files be told apart from one file with many names.
    pub file_id: Option<FileId>,
}

pub struct DupGroup {
    pub size: u64,
    pub files: Vec<DupFile>,
    /// How many distinct filesystem objects the group holds. Hard links
    /// to one inode count once, not once per pathname; on platforms
    /// without file identity this falls back to `files.len()`. This is
    /// the number reclaimable space is computed from — deleting all but
    /// one hard link frees nothing until the last link goes, so aliases
    /// are not spare copies.
    pub distinct_inodes: usize,
}

impl DupGroup {
    /// The bytes actually reclaimable by deleting copies: size times the
    /// copies beyond the first, where hard-link aliases are not copies.
    pub fn reclaimable(&self) -> u64 {
        self.size
            .saturating_mul(self.distinct_inodes.saturating_sub(1) as u64)
    }
}

/// The result of a duplicate scan, and what it had to leave out.
pub struct DupScan {
    pub groups: Vec<DupGroup>,
    /// Files never hashed because `MAX_CANDIDATES` was reached.
    ///
    /// Reported rather than dropped in silence: "no more duplicates"
    /// and "we stopped looking" are very different answers to give
    /// someone deciding what to delete.
    pub skipped: usize,
    /// Files that could not be hashed at all — they disappeared, became
    /// unreadable, or errored mid-read. Like `skipped`, they are counted
    /// rather than silently absent, so the result is not presented as
    /// complete when it was not.
    pub read_failures: usize,
}

/// Caps how many same-size candidates get hashed — a pathological tree
/// (e.g. millions of empty-ish config files) could otherwise turn "find
/// duplicates" into "read the entire drive". Candidates beyond this are
/// dropped from consideration, largest groups first (see below), so this
/// only bites on trees with an implausible number of same-size files.
const MAX_CANDIDATES: usize = 200_000;

/// (index path from the tree root, absolute filesystem path, identity) for
/// one file.
type SizeCandidate = (Vec<usize>, PathBuf, Option<FileId>);

pub fn find_duplicates(tree: &Tree, progress: Option<&DupProgress>) -> DupScan {
    find_duplicates_capped(tree, progress, MAX_CANDIDATES)
}

/// [`find_duplicates`] with the cap spelled out, so a test can reach the
/// truncation path without building a tree of 200,000 files.
fn find_duplicates_capped(tree: &Tree, progress: Option<&DupProgress>, cap: usize) -> DupScan {
    let mut by_size: HashMap<u64, Vec<SizeCandidate>> = HashMap::new();
    let mut path = tree.root_path.clone();
    let mut index_path = Vec::new();
    collect_by_size(&tree.root, &mut path, &mut index_path, &mut by_size);

    // Bias the candidate cap toward groups with the most same-size files
    // first — those are both the most likely to contain real duplicates
    // and the ones a hard cap would otherwise starve unfairly.
    let mut size_groups: Vec<(u64, Vec<SizeCandidate>)> = by_size
        .into_iter()
        .filter(|(size, files)| *size > 0 && files.len() > 1)
        .collect();
    // Tie-broken by size (unique per group, since `by_size` is keyed by
    // it) rather than left to land in whatever order `HashMap::into_iter`
    // happened to produce — that order comes from `RandomState`'s
    // per-process random seed, so without a deterministic tiebreaker,
    // which same-file-count groups get truncated when `MAX_CANDIDATES` is
    // actually hit (not just their display order) could differ between
    // two runs of an identical scan.
    size_groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| b.0.cmp(&a.0)));

    // Whole size-groups only. Taking a group's first few files and
    // stopping mid-way through it used to be possible, and a partly
    // hashed group is worse than an absent one: hash three of five
    // identical files and the view reports a group of three, so someone
    // clearing duplicates is told there are two spare copies when there
    // are four. An absent group at least says nothing.
    let mut candidates: Vec<(Vec<usize>, PathBuf, u64, Option<FileId>)> = Vec::new();
    let mut skipped = 0_usize;
    for (size, files) in size_groups {
        if candidates.len().saturating_add(files.len()) > cap {
            skipped = skipped.saturating_add(files.len());
            continue;
        }
        for (idx, p, file_id) in files {
            candidates.push((idx, p, size, file_id));
        }
    }
    if let Some(p) = progress {
        p.candidates_total
            .store(candidates.len() as u64, Ordering::Relaxed);
    }

    // Failures are rare (a race with a delete, a permission edge case), so
    // one shared atomic is fine even with hashing parallel across workers.
    let read_failures = AtomicUsize::new(0);
    let hashed: Vec<(blake3::Hash, Vec<usize>, u64, Option<FileId>)> = candidates
        .into_par_iter()
        .filter_map(|(idx, path, size, file_id)| {
            if let Some(p) = progress {
                if p.cancelled.load(Ordering::Relaxed) {
                    return None;
                }
            }
            let result = match hash_file(&path) {
                Ok(h) => Some((h, idx, size, file_id)),
                Err(_) => {
                    read_failures.fetch_add(1, Ordering::Relaxed);
                    None
                }
            };
            if let Some(p) = progress {
                p.hashed.fetch_add(1, Ordering::Relaxed);
            }
            result
        })
        .collect();

    // Built through a side struct so the distinct-inode count can be
    // computed while files are still arriving, then folded into the
    // finished `DupGroup`.
    struct Builder {
        size: u64,
        files: Vec<DupFile>,
        ids: Vec<FileId>,
        missing_id: bool,
    }
    let mut by_hash: HashMap<blake3::Hash, Builder> = HashMap::new();
    for (hash, index_path, size, file_id) in hashed {
        let builder = by_hash.entry(hash).or_insert_with(|| Builder {
            size,
            files: vec![],
            ids: vec![],
            missing_id: false,
        });
        match file_id {
            Some(id) => builder.ids.push(id),
            None => builder.missing_id = true,
        }
        builder.files.push(DupFile {
            index_path,
            file_id,
        });
    }

    let mut groups: Vec<DupGroup> = Vec::new();
    for builder in by_hash.into_values() {
        if builder.files.len() < 2 {
            continue;
        }
        let distinct_inodes = if builder.missing_id {
            // Identity unavailable (Windows): each pathname counts as its
            // own file, as duplicate detection always has.
            builder.files.len()
        } else {
            builder.ids.iter().collect::<HashSet<_>>().len()
        };
        // Same content, one inode — hard links, not copies. Deleting
        // every path but one frees nothing, so claiming these as
        // reclaimable duplicates would be a lie.
        if distinct_inodes <= 1 {
            continue;
        }
        groups.push(DupGroup {
            size: builder.size,
            files: builder.files,
            distinct_inodes,
        });
    }
    // Same determinism concern as the size_groups sort above — `by_hash`
    // is also a `HashMap`, so ties on wasted space need an explicit,
    // deterministic tiebreaker rather than inheriting random iteration
    // order. Size and file count resolve all but a true tie on every
    // displayed stat; the first file's index path (unique per file)
    // resolves the rest, so the final order never depends on hashing.
    groups.sort_by(|a, b| {
        let wasted_a = a.reclaimable();
        let wasted_b = b.reclaimable();
        wasted_b
            .cmp(&wasted_a)
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| b.files.len().cmp(&a.files.len()))
            .then_with(|| {
                a.files
                    .first()
                    .map(|f| &f.index_path)
                    .cmp(&b.files.first().map(|f| &f.index_path))
            })
    });
    DupScan {
        groups,
        skipped,
        read_failures: read_failures.load(Ordering::Relaxed),
    }
}

/// Buckets every file in the tree by size, iteratively.
///
/// One frame per directory being walked, with the path and index path
/// pushed and popped in step with them — so the depth of the tree being
/// scanned costs heap rather than call stack. It used to recurse per
/// level, which put a user-chosen depth on the stack in the one place
/// that walks a whole drive twice.
///
/// This is deliberately not [`crate::model::walk_preorder`]: the
/// candidates are hashed *after* the walk, so each one carries a real
/// `PathBuf` built here, and the shared walker hands out index paths
/// only.
fn collect_by_size(
    node: &Node,
    path: &mut PathBuf,
    index_path: &mut Vec<usize>,
    out: &mut HashMap<u64, Vec<SizeCandidate>>,
) {
    struct Frame<'a> {
        node: &'a Node,
        next: usize,
    }

    let mut stack = vec![Frame { node, next: 0 }];
    while let Some(top) = stack.len().checked_sub(1) {
        let Some(frame) = stack.get_mut(top) else {
            break;
        };
        let parent = frame.node;
        let Some(child) = parent.children.get(frame.next) else {
            stack.pop();
            // The root frame's segments were never pushed by this loop.
            if !stack.is_empty() {
                path.pop();
                index_path.pop();
            }
            continue;
        };
        let index = frame.next;
        frame.next += 1;

        index_path.push(index);
        path.push(&child.name);
        if child.is_dir {
            // Segments stay pushed; the new frame owns them.
            stack.push(Frame {
                node: child,
                next: 0,
            });
            continue;
        }
        if !child.is_symlink && child.size > 0 {
            out.entry(child.size).or_default().push((
                index_path.clone(),
                path.clone(),
                child.file_id,
            ));
        }
        path.pop();
        index_path.pop();
    }
}

fn hash_file(path: &std::path::Path) -> std::io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;
    use std::fs;

    /// A size-group is taken whole or not at all, and what was left out
    /// is reported.
    ///
    /// The cap used to stop mid-group, so a group of five identical
    /// files could be hashed three at a time and shown as a group of
    /// three. Someone clearing duplicates would be told there were two
    /// spare copies when there were four — and nothing anywhere said the
    /// search had been cut short.
    #[test]
    fn the_candidate_cap_drops_whole_groups_and_says_how_many() -> anyhow::Result<()> {
        let root = scratch_dir("dupes", "cap");
        let root = root.as_path();
        let _ = fs::remove_dir_all(root);
        fs::create_dir_all(root)?;

        // Two groups of identical files, of two distinct sizes, so the
        // size prefilter puts them in separate buckets.
        // Nested at different depths on purpose: the size prefilter
        // walks the whole tree, and a walk that only handled the top
        // level would still find both groups if they sat side by side.
        for (folder, size, count) in [("big", 64_usize, 5_usize), ("small/a/b/c", 32, 3)] {
            let sub = root.join(folder);
            fs::create_dir_all(&sub)?;
            for i in 0..count {
                fs::write(sub.join(format!("f{i}.bin")), vec![b'x'; size])?;
            }
        }

        let tree = crate::scanner::scan(root, None)?;

        // Room for everything: both groups come back, nothing skipped.
        let all = find_duplicates_capped(&tree, None, 100);
        assert_eq!(all.skipped, 0, "nothing should be dropped under a wide cap");
        assert_eq!(
            all.groups.len(),
            2,
            "both sets of identical files form a group"
        );

        // Room for the five-file group only. The three-file group is
        // dropped whole and counted, rather than half-hashed.
        let capped = find_duplicates_capped(&tree, None, 5);
        assert_eq!(
            capped.skipped, 3,
            "the group that did not fit should be reported, not silently dropped"
        );
        for group in &capped.groups {
            assert_eq!(
                group.files.len(),
                5,
                "a group was reported with {} of its files, which understates                  how many copies exist",
                group.files.len()
            );
        }
        fs::remove_dir_all(root)?;
        Ok(())
    }

    /// Two hard links to one file are not reported as duplicates: their
    /// content is identical, but deleting one frees nothing until the
    /// last link goes, so calling the pair "reclaimable copies" would be
    /// a lie. The same-content group must be dropped, while a genuine
    /// duplicate pair beside it still reports its real reclaimable bytes.
    #[cfg(unix)]
    #[test]
    fn hard_links_are_not_reported_as_reclaimable_duplicates() -> anyhow::Result<()> {
        let root = scratch_dir("hardlink", "two_names");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;

        // One file with two names, and a real pair of copies.
        fs::write(root.join("one.bin"), b"same bytes")?;
        fs::hard_link(root.join("one.bin"), root.join("two.bin"))?;
        fs::write(root.join("a.dat"), b"real duplicate")?;
        fs::write(root.join("b.dat"), b"real duplicate")?;

        let tree = crate::scanner::scan(&root, None)?;
        let scan = find_duplicates(&tree, None);
        assert_eq!(
            scan.groups.len(),
            1,
            "only the genuine duplicate pair may form a group"
        );
        let Some(group) = scan.groups.first() else {
            return Ok(());
        };
        assert_eq!(
            group.distinct_inodes, 2,
            "the group holds two distinct files, not two names for one"
        );
        assert_eq!(
            group.reclaimable(),
            group.size,
            "one spare copy of the duplicated file is reclaimable"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
