// ============================================================================
// Module:       changelog (example)
// Description:  Regenerates CHANGELOG.md from the git history, grouping
//               conventional-commit subjects under the release tag that
//               shipped them.
//
// Dependencies: anyhow; std::process::Command (shells out to git). No crates
//               beyond what the library already pulls in.
// ============================================================================

//! Regenerates `CHANGELOG.md` — run it with:
//!
//! ```sh
//! cargo run --example changelog
//! ```
//!
//! or check the committed file against the history without rewriting it:
//!
//! ```sh
//! cargo run --example changelog -- --check
//! ```
//!
//! The changelog is derived, not authored. Every entry is a
//! conventional-commit subject (`fix:`, `feat:`, `refactor:` …) resolved
//! against `git tag`, so the released sections cannot say something the
//! history does not. That is the whole point: a hand-maintained changelog
//! drifts silently, and the drift is only ever discovered on release day.
//!
//! Consequences worth knowing before you edit the file by hand:
//!
//! - **Released sections are rewritten on every run.** Prose added to one
//!   is lost the next time this runs. If a release needs a human summary,
//!   the place for it is the GitHub release body, which
//!   `.github/workflows/release.yml` owns.
//! - **`--check` compares released sections only.** The `Unreleased`
//!   section is deliberately exempt, because its content changes with
//!   every merge — and a squash or rebase rewrites the very hashes it
//!   would be pinned to. Gating a pull request on it would fail every
//!   pull request for no signal.
//!
//! - **A version tag no branch can reach is a hard error under `--check`.**
//!   Each release is computed as `<previous>..<tag>`, so a tag left pointing
//!   at an amended-away commit makes every range from there on wrong. See
//!   [`orphan_report`] for the symptom and the fix.
//!
//! An example rather than a test, for the same reason as `brand_assets`:
//! it writes into the source tree, which is not something `cargo test`
//! should ever do. It is still built by `cargo clippy --all-targets`, so
//! it cannot rot silently.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// The generated file, relative to the crate root.
const CHANGELOG: &str = "CHANGELOG.md";

/// Used to build the commit and compare links. Taken from `Cargo.toml`'s
/// `repository` field rather than guessed.
const REPO: &str = "https://github.com/lnorton89/rustdirstat";

/// Heading for commits that have not shipped in a tagged release yet.
const UNRELEASED: &str = "## [Unreleased]";

/// Field separator for `git log --format`. `%x1f` is the ASCII unit
/// separator, which cannot occur in a commit subject.
const SEP: char = '\u{1f}';

/// Conventional-commit types, in the order their sections appear within a
/// release, mapped to the heading each lands under.
///
/// Anything whose subject does not parse as a conventional commit falls to
/// `Other` — which is most of the pre-`v0.1.0` history, when this
/// repository had not adopted the convention yet.
const SECTIONS: &[(&str, &[&str])] = &[
    ("Added", &["feat"]),
    ("Fixed", &["fix"]),
    ("Performance", &["perf"]),
    ("Changed", &["refactor", "style"]),
    ("Documentation", &["docs"]),
    ("Internal", &["build", "ci", "chore"]),
    ("Tests", &["test"]),
];

/// Heading for breaking changes, which lead a release regardless of the
/// type they were committed under.
const BREAKING: &str = "Breaking changes";

/// Heading for subjects that are not conventional commits.
const OTHER: &str = "Other";

/// One commit, already parsed into the pieces a changelog line needs.
struct Commit {
    hash: String,
    short: String,
    /// The heading this commit belongs under.
    section: &'static str,
    /// The `(scope)` of a conventional commit, if it carried one.
    scope: Option<String>,
    /// The subject with the `type(scope):` prefix stripped.
    summary: String,
}

/// One tagged release and everything that landed since the previous tag.
struct Release {
    /// The tag as git spells it, e.g. `v0.2.0`.
    tag: String,
    /// The tag without its leading `v`, e.g. `0.2.0`.
    version: String,
    /// Committer date of the tagged commit, `YYYY-MM-DD`.
    date: String,
    /// The tag this release is compared against, if any.
    previous: Option<String>,
    commits: Vec<Commit>,
}

fn main() -> Result<()> {
    let mut check = false;
    for arg in std::env::args().skip(1) {
        if arg == "--check" {
            check = true;
        } else {
            bail!("unknown argument `{arg}`; usage: changelog [--check]");
        }
    }

    if !Path::new("Cargo.toml").is_file() {
        bail!("run this from the crate root, where Cargo.toml lives");
    }

    let releases = releases()?;
    let unreleased = match releases.first() {
        Some(latest) => commits(&format!("{}..HEAD", latest.tag))?,
        None => commits("HEAD")?,
    };
    let generated = render(&releases, &unreleased);

    // An orphaned tag makes every range below wrong, so it is reported
    // before any mismatch it would itself have caused.
    let orphaned = orphaned_tags(&releases)?;

    if check {
        if !orphaned.is_empty() {
            bail!("{}", orphan_report(&orphaned));
        }
        let existing = fs::read_to_string(CHANGELOG).with_context(|| {
            format!("{CHANGELOG} is missing; run `cargo run --example changelog`")
        })?;
        if released_only(&existing) == released_only(&generated) {
            println!("{CHANGELOG} matches the git history.");
            return Ok(());
        }
        bail!(
            "{CHANGELOG} is out of date with the git history.\n\
             Regenerate it with `cargo run --example changelog` and commit the result."
        );
    }

    // A warning rather than an error on this path: the file still gets
    // written, so you are not locked out of regenerating while you sort the
    // tag out. `--check` is where it becomes a build failure.
    if !orphaned.is_empty() {
        eprintln!("warning: {}", orphan_report(&orphaned));
    }

    fs::write(CHANGELOG, &generated).with_context(|| format!("failed to write {CHANGELOG}"))?;
    println!(
        "wrote {CHANGELOG}: {} release(s), {} unreleased commit(s).",
        releases.len(),
        unreleased.len()
    );
    Ok(())
}

/// Runs git and returns its stdout, turning a non-zero exit into an error
/// rather than an empty string that would silently produce a blank section.
fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("failed to run `git {}`", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git {}` failed: {}", args.join(" "), stderr.trim());
    }

    String::from_utf8(output.stdout)
        .with_context(|| format!("`git {}` printed invalid UTF-8", args.join(" ")))
}

/// Every version tag, newest release first.
fn releases() -> Result<Vec<Release>> {
    let mut tags: Vec<(String, (u64, u64, u64))> = Vec::new();
    for line in git(&["tag", "--list"])?.lines() {
        let tag = line.trim();
        if tag.is_empty() {
            continue;
        }
        if let Some(version) = parse_version(tag) {
            tags.push((tag.to_string(), version));
        }
    }

    // Oldest first, so each tag knows the one it follows.
    tags.sort_by_key(|(_, version)| *version);

    let mut releases = Vec::new();
    for (index, (tag, _)) in tags.iter().enumerate() {
        let previous = index
            .checked_sub(1)
            .and_then(|i| tags.get(i))
            .map(|(t, _)| t.clone());
        let range = match &previous {
            Some(prev) => format!("{prev}..{tag}"),
            // The first release reaches back to the root commit.
            None => tag.clone(),
        };
        releases.push(Release {
            version: tag.trim_start_matches('v').to_string(),
            date: tag_date(tag)?,
            previous,
            commits: commits(&range)?,
            tag: tag.clone(),
        });
    }

    // Newest first, which is the order a changelog is read in.
    releases.reverse();
    Ok(releases)
}

/// Every ref a tag is reachable from.
///
/// Empty means the tag points into history that no branch can reach — the
/// tagged commit was amended or rebased afterwards, and the tag was left
/// behind pointing at the discarded copy. `v0.2.0` was in exactly this
/// state until it was re-pointed; the symptom was its own release commit
/// showing up under `Unreleased`, because `v0.2.0..HEAD` spans a fork
/// rather than a range.
fn refs_containing(tag: &str) -> Result<Vec<String>> {
    let refs = git(&[
        "for-each-ref",
        "--contains",
        tag,
        "--format=%(refname)",
        "refs/heads",
        "refs/remotes",
    ])?;
    Ok(refs
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// Version tags pointing at unreachable commits.
///
/// A checkout with no branch refs at all cannot answer the question, so
/// this reports nothing rather than condemning every tag — better to skip
/// the check than to fail a build for the shape of its clone.
fn orphaned_tags(releases: &[Release]) -> Result<Vec<String>> {
    let all_refs = git(&[
        "for-each-ref",
        "--format=%(refname)",
        "refs/heads",
        "refs/remotes",
    ])?;
    if all_refs.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut orphaned = Vec::new();
    for release in releases {
        if refs_containing(&release.tag)?.is_empty() {
            orphaned.push(release.tag.clone());
        }
    }
    Ok(orphaned)
}

/// What to tell someone who has just grown an orphaned tag.
fn orphan_report(tags: &[String]) -> String {
    let list = tags.join(", ");
    format!(
        "{list} point(s) at a commit no branch can reach.\n\
         A tag was placed, then the commit under it was amended or rebased, so the tag \
         now describes history that was thrown away. Every range computed from it is wrong.\n\
         \n\
         Re-point it at the commit that actually shipped, then force-push:\n\
         \n\
         \x20   git tag -f -a <tag> <commit> -m \"rustdirstat <tag>\"\n\
         \x20   git push --force origin refs/tags/<tag>\n\
         \n\
         Pushing a `v*` tag re-triggers .github/workflows/release.yml, which ends in\n\
         `gh release upload --clobber`. If the release is already published and you do not\n\
         want its assets rebuilt, cancel the run: `gh run cancel <id>`."
    )
}

/// `v1.2.3` or `1.2.3` into a comparable tuple. Anything else is not a
/// release tag and is skipped rather than guessed at.
fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Committer date of the commit a tag points at. `^{commit}` dereferences
/// an annotated tag, so both tag styles give the same answer.
fn tag_date(tag: &str) -> Result<String> {
    let spec = format!("{tag}^{{commit}}");
    let date = git(&["log", "-1", "--format=%cd", "--date=short", &spec])?;
    Ok(date.trim().to_string())
}

/// Parsed commits in a revision range, newest first. Merges are excluded —
/// they carry no subject of their own worth listing.
fn commits(range: &str) -> Result<Vec<Commit>> {
    let format = format!("--format=%H{SEP}%h{SEP}%s");
    let log = git(&["log", "--no-merges", &format, range])?;

    let mut commits = Vec::new();
    for line in log.lines() {
        let mut fields = line.split(SEP);
        let (Some(hash), Some(short), Some(subject)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        commits.push(parse_subject(hash, short, subject));
    }
    Ok(commits)
}

/// Splits a `type(scope)!: summary` subject into its parts, falling back to
/// the `Other` section for anything that is not a conventional commit.
fn parse_subject(hash: &str, short: &str, subject: &str) -> Commit {
    let plain = Commit {
        hash: hash.to_string(),
        short: short.to_string(),
        section: OTHER,
        scope: None,
        summary: subject.trim().to_string(),
    };

    let Some(colon) = subject.find(':') else {
        return plain;
    };
    let (head, rest) = subject.split_at(colon);
    let summary = rest[1..].trim();
    if summary.is_empty() {
        return plain;
    }

    // A trailing `!` is the conventional-commit marker for a breaking change.
    let breaking = head.ends_with('!');
    let head = head.strip_suffix('!').unwrap_or(head);

    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => match scope.strip_suffix(')') {
            Some(scope) if !scope.is_empty() => (kind, Some(scope.to_string())),
            // `feat(unclosed: ...` is not a conventional commit.
            _ => return plain,
        },
        None => (head, None),
    };

    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_lowercase()) {
        return plain;
    }

    let section = if breaking {
        BREAKING
    } else {
        let mut found = OTHER;
        for (heading, kinds) in SECTIONS {
            if kinds.contains(&kind) {
                found = heading;
                break;
            }
        }
        found
    };

    Commit {
        hash: hash.to_string(),
        short: short.to_string(),
        section,
        scope,
        summary: summary.to_string(),
    }
}

/// The full file, preamble through link references.
fn render(releases: &[Release], unreleased: &[Commit]) -> String {
    let mut out = String::new();

    out.push_str("# Changelog\n\n");
    out.push_str("All notable changes to RustDirStat, newest first.\n\n");
    out.push_str(
        "**This file is generated.** It is rebuilt from the git history by\n\
         `cargo run --example changelog`, so released sections are rewritten in\n\
         place and edits to them do not survive. See `CONTRIBUTING.md`.\n\n",
    );
    out.push_str(
        "The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),\n\
         and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).\n",
    );

    out.push('\n');
    out.push_str(UNRELEASED);
    out.push('\n');
    if unreleased.is_empty() {
        out.push_str("\nNothing yet — the latest release is the tip of `main`.\n");
    } else {
        push_sections(&mut out, unreleased);
    }

    for release in releases {
        out.push_str(&format!("\n## [{}] - {}\n", release.version, release.date));
        if release.commits.is_empty() {
            out.push_str("\nNo commits recorded against this tag.\n");
        } else {
            push_sections(&mut out, &release.commits);
        }
    }

    push_links(&mut out, releases);
    out
}

/// The `### Heading` blocks for one release, in `SECTIONS` order, skipping
/// any heading nothing landed under.
fn push_sections(out: &mut String, commits: &[Commit]) {
    let mut order: Vec<&str> = vec![BREAKING];
    order.extend(SECTIONS.iter().map(|(heading, _)| *heading));
    order.push(OTHER);

    for heading in order {
        let matching: Vec<&Commit> = commits.iter().filter(|c| c.section == heading).collect();
        if matching.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {heading}\n\n"));
        for commit in matching {
            let scope = match &commit.scope {
                Some(scope) => format!("**{scope}:** "),
                None => String::new(),
            };
            out.push_str(&format!(
                "- {scope}{} ([`{}`]({REPO}/commit/{}))\n",
                commit.summary, commit.short, commit.hash
            ));
        }
    }
}

/// Markdown link references, so each heading above is a link to the diff
/// that produced it.
fn push_links(out: &mut String, releases: &[Release]) {
    out.push('\n');
    match releases.first() {
        Some(latest) => out.push_str(&format!(
            "[Unreleased]: {REPO}/compare/{}...HEAD\n",
            latest.tag
        )),
        None => out.push_str(&format!("[Unreleased]: {REPO}/commits/HEAD\n")),
    }

    for release in releases {
        match &release.previous {
            Some(previous) => out.push_str(&format!(
                "[{}]: {REPO}/compare/{previous}...{}\n",
                release.version, release.tag
            )),
            None => out.push_str(&format!(
                "[{}]: {REPO}/releases/tag/{}\n",
                release.version, release.tag
            )),
        }
    }
}

/// The file with its `Unreleased` section removed, which is what `--check`
/// compares. See the module docs for why that section is exempt.
fn released_only(text: &str) -> String {
    let mut kept = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        if line.starts_with(UNRELEASED) {
            skipping = true;
            continue;
        }
        // Any other `## ` heading ends the section, as do the link
        // references that close the file.
        if skipping && (line.starts_with("## ") || line.starts_with("[Unreleased]:")) {
            skipping = false;
        }
        if !skipping {
            kept.push(line);
        }
    }
    kept.join("\n")
}
