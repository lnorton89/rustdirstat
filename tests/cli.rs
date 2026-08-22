// ============================================================================
// Module:       cli (integration test)
// Description:  Runs the built binary end to end over a fixture tree, covering
//               the non-interactive report and CSV export paths.
//
// Dependencies: anyhow, std::process (invokes CARGO_BIN_EXE_rustdirstat)
// ============================================================================

//! End-to-end cover for the two non-interactive modes.
//!
//! Everything else in the suite tests a function; this runs the actual
//! binary, so it covers the wiring nothing else touches — argument
//! parsing, the scan being started, the tree reaching the writer, and the
//! process exiting with a sensible status. A unit test cannot catch
//! `main` handing the report the wrong node or a flag never being read.
//!
//! The TUI and GUI are deliberately not launched here: both take over a
//! terminal or open a window, which is what `tests/quit_stress.rs` exists
//! for on the TUI side.

use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A scratch directory named after the test and this process, so two
/// tests never share one and delete it out from under each other.
fn scratch(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "rustdirstat_cli_{}_{name}_{unique}",
        std::process::id()
    ))
}

/// A small tree with known sizes, so the numbers in the output can be
/// checked rather than merely present.
fn build_fixture(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("docs"))?;
    fs::create_dir_all(root.join("empty"))?;
    fs::write(root.join("top.txt"), vec![b'a'; 1000])?;
    fs::write(root.join("docs").join("note.md"), vec![b'b'; 2000])?;
    fs::write(root.join("docs").join("data.csv"), vec![b'c'; 3000])?;
    Ok(())
}

fn run(args: &[&str]) -> Result<(bool, String)> {
    let output = Command::new(env!("CARGO_BIN_EXE_rustdirstat"))
        .args(args)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), text))
}

#[test]
fn the_report_mode_scans_and_prints_totals() -> Result<()> {
    let root = scratch("report");
    let _ = fs::remove_dir_all(&root);
    build_fixture(&root)?;

    let path = root
        .to_str()
        .ok_or_else(|| anyhow!("the scratch path is not valid UTF-8"))?;
    let (ok, text) = run(&["-n", "-d", "2", "-t", "10", path])?;
    assert!(ok, "the report should exit successfully:\n{text}");

    // 3 files, 6000 bytes, 2 directories below the root.
    assert!(
        text.contains("3 files"),
        "the report should count every file:\n{text}"
    );
    assert!(
        text.contains("2 dirs") || text.contains("2 directories"),
        "the report should count the directories:\n{text}"
    );
    assert!(
        text.contains("docs"),
        "the largest subdirectory should be listed:\n{text}"
    );
    // 6000 bytes renders as "5.9 KB" at base 1024.
    assert!(
        text.contains("KB"),
        "sizes should be human readable:\n{text}"
    );

    fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn the_csv_mode_writes_a_row_per_node() -> Result<()> {
    let root = scratch("csv");
    let _ = fs::remove_dir_all(&root);
    build_fixture(&root)?;
    let out = root.join("export.csv");

    let path = root
        .to_str()
        .ok_or_else(|| anyhow!("the scratch path is not valid UTF-8"))?;
    let out_path = out
        .to_str()
        .ok_or_else(|| anyhow!("the output path is not valid UTF-8"))?;
    let (ok, text) = run(&["--csv", out_path, path])?;
    assert!(ok, "the export should exit successfully:\n{text}");

    let csv = fs::read_to_string(&out)?;
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines.first().map(|l| l.split(',').next()),
        Some(Some("path")),
        "the first line should be the header"
    );

    // The root, two directories, three files, and the export itself if
    // the scan happened to see it — so at least seven lines including the
    // header, and every one of the fixture's entries by name.
    for name in ["top.txt", "note.md", "data.csv", "docs", "empty"] {
        assert!(
            csv.contains(name),
            "{name} should appear in the export:\n{csv}"
        );
    }
    assert!(
        lines.len() >= 7,
        "expected a row per node, got {} lines:\n{csv}",
        lines.len()
    );

    // Sizes are real: the 3000-byte file must say so, in the `size`
    // column specifically.
    //
    // Counted from the right, because a path may itself contain commas.
    // The first spelling of this looked for ",3000," anywhere in the row,
    // which the *physical_size* column satisfies just as well — zeroing
    // `size` left that version of the test green.
    let size_of = |name: &str| -> Option<u64> {
        lines
            .iter()
            .find(|line| line.contains(name))
            .and_then(|line| line.rsplit(',').nth(5))
            .and_then(|field| field.parse().ok())
    };
    assert_eq!(
        size_of("data.csv"),
        Some(3000),
        "data.csv should be recorded at 3000 bytes in the size column:\n{csv}"
    );
    assert_eq!(
        size_of("note.md"),
        Some(2000),
        "note.md should be recorded at 2000 bytes:\n{csv}"
    );

    fs::remove_dir_all(&root)?;
    Ok(())
}

/// A path that is not there fails, rather than reporting an empty tree
/// as though it had scanned something.
#[test]
fn a_missing_path_is_an_error() -> Result<()> {
    let missing = scratch("missing");
    let path = missing
        .to_str()
        .ok_or_else(|| anyhow!("the scratch path is not valid UTF-8"))?;
    let (ok, text) = run(&["-n", path])?;
    assert!(
        !ok,
        "scanning a path that does not exist should fail, but it succeeded:\n{text}"
    );
    Ok(())
}

/// The two non-interactive modes are mutually exclusive: `--no-tui` with
/// `--csv` used to be accepted and silently mean CSV, which is a report
/// that never printed and an export the user did not ask for.
#[test]
fn the_report_and_csv_modes_conflict() -> Result<()> {
    let root = scratch("conflict");
    let _ = fs::remove_dir_all(&root);
    build_fixture(&root)?;
    let out = root.join("export.csv");

    let path = root
        .to_str()
        .ok_or_else(|| anyhow!("the scratch path is not valid UTF-8"))?;
    let out_path = out
        .to_str()
        .ok_or_else(|| anyhow!("the output path is not valid UTF-8"))?;
    let (ok, text) = run(&["-n", "--csv", out_path, path])?;
    assert!(
        !ok,
        "--no-tui and --csv must be refused together, but it succeeded:\n{text}"
    );
    assert!(
        text.to_lowercase().contains("cannot be used with")
            || text.to_lowercase().contains("conflict"),
        "clap should explain the conflict:\n{text}"
    );
    assert!(
        !out.exists(),
        "no CSV may be written for a refused invocation"
    );

    fs::remove_dir_all(&root)?;
    Ok(())
}

/// Two paths on the command line scan into one tree.
///
/// The terminal equivalent of WinDirStat opening on several drives: each
/// root becomes a top-level entry under a label, and the totals are the
/// sum of the roots rather than of either one.
#[test]
fn several_paths_scan_into_one_report() -> Result<()> {
    let first = scratch("multi_first");
    let second = scratch("multi_second");
    let _ = fs::remove_dir_all(&first);
    let _ = fs::remove_dir_all(&second);
    fs::create_dir_all(&first)?;
    fs::create_dir_all(&second)?;
    fs::write(first.join("one.bin"), vec![b'a'; 1000])?;
    fs::write(second.join("two.bin"), vec![b'b'; 2000])?;

    let first_arg = first
        .to_str()
        .ok_or_else(|| anyhow!("scratch path is not UTF-8"))?;
    let second_arg = second
        .to_str()
        .ok_or_else(|| anyhow!("scratch path is not UTF-8"))?;
    let (ok, text) = run(&["--no-tui", "-d", "2", first_arg, second_arg])?;

    assert!(ok, "the report should succeed: {text}");
    assert!(
        text.contains("one.bin") && text.contains("two.bin"),
        "both roots should appear in one report: {text}"
    );

    let _ = fs::remove_dir_all(&first);
    let _ = fs::remove_dir_all(&second);
    Ok(())
}

/// A typo in the second path fails before anything is scanned.
#[test]
fn a_missing_second_path_is_an_error() -> Result<()> {
    let real = scratch("multi_real");
    let _ = fs::remove_dir_all(&real);
    fs::create_dir_all(&real)?;
    let missing = scratch("multi_missing");
    let _ = fs::remove_dir_all(&missing);

    let real_arg = real
        .to_str()
        .ok_or_else(|| anyhow!("scratch path is not UTF-8"))?;
    let missing_arg = missing
        .to_str()
        .ok_or_else(|| anyhow!("scratch path is not UTF-8"))?;
    let (ok, text) = run(&["--no-tui", real_arg, missing_arg])?;

    assert!(!ok, "a missing path should fail the run: {text}");
    assert!(
        text.contains("path does not exist"),
        "and say which: {text}"
    );

    let _ = fs::remove_dir_all(&real);
    Ok(())
}
