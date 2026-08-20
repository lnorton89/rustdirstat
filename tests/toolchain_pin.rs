// ============================================================================
// Module:       toolchain_pin (integration test)
// Description:  Checks that rust-toolchain.toml and both CI workflows name the
//               same Rust version, so local and CI lint identically.
//
// Dependencies: anyhow, std::fs (reads repo config at test time)
// ============================================================================

//! Keeps the pinned toolchain in one agreed state.
//!
//! `rust-toolchain.toml` is what rustup obeys; the workflows also name a
//! version so the setup action installs it up front instead of letting
//! the first `cargo` call download it. Three places, one fact — which is
//! the shape that drifts.
//!
//! It drifting is not hypothetical. CI used `dtolnay/rust-toolchain@stable`,
//! so it silently moved to each new Rust release while a local checkout
//! stayed wherever it was. `chunks_exact_to_as_chunks` was added in 1.98
//! and failed CI against a local toolchain on 1.97 that could not have
//! fired it: a lint that does not exist locally cannot be caught locally,
//! however carefully anyone runs clippy first.

use anyhow::{anyhow, Result};
use std::fs;

/// The `channel = "..."` value from `rust-toolchain.toml`.
fn pinned_channel() -> Result<String> {
    let text = fs::read_to_string("rust-toolchain.toml")?;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("channel") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        return Ok(rest.trim().trim_matches('"').to_owned());
    }
    Err(anyhow!("rust-toolchain.toml has no `channel` line"))
}

/// The `TOOLCHAIN: "..."` value from a workflow.
fn workflow_toolchain(path: &str) -> Result<String> {
    let text = fs::read_to_string(path)?;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("TOOLCHAIN:") else {
            continue;
        };
        return Ok(rest.trim().trim_matches('"').to_owned());
    }
    Err(anyhow!("{path} does not set TOOLCHAIN"))
}

#[test]
fn every_workflow_pins_the_same_toolchain_as_the_repo() -> Result<()> {
    let pinned = pinned_channel()?;
    assert!(
        !pinned.is_empty(),
        "rust-toolchain.toml should name a version, not an empty channel"
    );

    for workflow in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let declared = workflow_toolchain(workflow)?;
        assert_eq!(
            declared, pinned,
            "{workflow} installs Rust {declared} but rust-toolchain.toml pins {pinned}, \
             so CI would lint with a different compiler than a local checkout does"
        );
    }
    Ok(())
}

/// No workflow may go back to floating `@stable`.
///
/// That is what let CI move to a new release on its own, and the failure
/// mode is a red build on a morning nobody changed anything.
#[test]
fn no_workflow_installs_a_floating_stable_toolchain() -> Result<()> {
    for workflow in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let text = fs::read_to_string(workflow)?;
        assert!(
            !text.contains("rust-toolchain@stable"),
            "{workflow} uses dtolnay/rust-toolchain@stable, which follows whatever Rust \
             releases next — pin it to the version in rust-toolchain.toml instead"
        );
    }
    Ok(())
}
