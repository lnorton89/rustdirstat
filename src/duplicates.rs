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
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
}

pub struct DupGroup {
    pub size: u64,
    pub files: Vec<DupFile>,
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
}

/// Caps how many same-size candidates get hashed — a pathological tree
/// (e.g. millions of empty-ish config files) could otherwise turn "find
/// duplicates" into "read the entire drive". Candidates beyond this are
/// dropped from consideration, largest groups first (see below), so this
/// only bites on trees with an implausible number of same-size files.
const MAX_CANDIDATES: usize = 200_000;

/// (index path from the tree root, absolute filesystem path) for one file.
type SizeCandidate = (Vec<usize>, PathBuf);

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
    let mut candidates: Vec<(Vec<usize>, PathBuf, u64)> = Vec::new();
    let mut skipped = 0_usize;
    for (size, files) in size_groups {
        if candidates.len().saturating_add(files.len()) > cap {
            skipped = skipped.saturating_add(files.len());
            continue;
        }
        for (idx, p) in files {
            candidates.push((idx, p, size));
        }
    }
    if let Some(p) = progress {
        p.candidates_total
            .store(candidates.len() as u64, Ordering::Relaxed);
    }

    let hashed: Vec<(blake3::Hash, Vec<usize>, u64)> = candidates
        .into_par_iter()
        .filter_map(|(idx, path, size)| {
            if let Some(p) = progress {
                if p.cancelled.load(Ordering::Relaxed) {
                    return None;
                }
            }
            let result = hash_file(&path).ok().map(|h| (h, idx, size));
            if let Some(p) = progress {
                p.hashed.fetch_add(1, Ordering::Relaxed);
            }
            result
        })
        .collect();

    let mut by_hash: HashMap<blake3::Hash, DupGroup> = HashMap::new();
    for (hash, index_path, size) in hashed {
        let group = by_hash.entry(hash).or_insert_with(|| DupGroup {
            size,
            files: vec![],
        });
        group.files.push(DupFile { index_path });
    }

    let mut groups: Vec<DupGroup> = by_hash
        .into_values()
        .filter(|g| g.files.len() > 1)
        .collect();
    // Same determinism concern as the size_groups sort above — `by_hash`
    // is also a `HashMap`, so ties on wasted space need an explicit,
    // deterministic tiebreaker rather than inheriting random iteration
    // order. Size and file count resolve all but a true tie on every
    // displayed stat; the first file's index path (unique per file)
    // resolves the rest, so the final order never depends on hashing.
    groups.sort_by(|a, b| {
        let wasted_a = a.size * (a.files.len() as u64 - 1);
        let wasted_b = b.size * (b.files.len() as u64 - 1);
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
    DupScan { groups, skipped }
}

fn collect_by_size(
    node: &Node,
    path: &mut PathBuf,
    index_path: &mut Vec<usize>,
    out: &mut HashMap<u64, Vec<SizeCandidate>>,
) {
    for (i, child) in node.children.iter().enumerate() {
        index_path.push(i);
        path.push(&child.name);
        if child.is_dir {
            collect_by_size(child, path, index_path, out);
        } else if !child.is_symlink && child.size > 0 {
            out.entry(child.size)
                .or_default()
                .push((index_path.clone(), path.clone()));
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
        // Named with the pid and a counter: two tests sharing a temp
        // directory delete it out from under each other, which showed up
        // as an intermittent PermissionDenied on Windows CI.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("rustdirstat_dupes_{}_{unique}", std::process::id()));
        let root = root.as_path();
        let _ = fs::remove_dir_all(root);
        fs::create_dir_all(root)?;

        // Two groups of identical files, of two distinct sizes, so the
        // size prefilter puts them in separate buckets.
        for (folder, size, count) in [("big", 64_usize, 5_usize), ("small", 32, 3)] {
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
}
