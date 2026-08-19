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

/// Caps how many same-size candidates get hashed — a pathological tree
/// (e.g. millions of empty-ish config files) could otherwise turn "find
/// duplicates" into "read the entire drive". Candidates beyond this are
/// dropped from consideration, largest groups first (see below), so this
/// only bites on trees with an implausible number of same-size files.
const MAX_CANDIDATES: usize = 200_000;

/// (index path from the tree root, absolute filesystem path) for one file.
type SizeCandidate = (Vec<usize>, PathBuf);

pub fn find_duplicates(tree: &Tree, progress: Option<&DupProgress>) -> Vec<DupGroup> {
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

    let mut candidates: Vec<(Vec<usize>, PathBuf, u64)> = Vec::new();
    'outer: for (size, files) in size_groups {
        for (idx, p) in files {
            if candidates.len() >= MAX_CANDIDATES {
                break 'outer;
            }
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
    groups
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
