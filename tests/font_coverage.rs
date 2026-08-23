// ============================================================================
// Module:       font_coverage (integration test)
// Description:  Every character a shipped translation uses must exist in the
//               font the window bundles, or that language renders as boxes.
//
// Dependencies: std::fs (reads assets/lang and the epaint font)
// ============================================================================

//! A language in the picker that draws as a row of empty rectangles is
//! worse than no language at all, and nothing else in this project would
//! notice: the catalogue parses, the tests pass, the strings are correct,
//! and the window shows tofu.
//!
//! egui bundles Ubuntu-Light, which covers Latin, Latin Extended and
//! Cyrillic and has no CJK glyphs at all. That is why the shipped set is
//! ten Latin-script languages and Russian, and why Japanese, Chinese and
//! Korean are not among them — they need a bundled CJK face first, which
//! is a size and licensing decision rather than a translation.
//!
//! This reads the font's own character map and checks every catalogue
//! against it, so the constraint is enforced rather than remembered.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[test]
fn every_shipped_catalogue_draws_in_the_bundled_font() -> anyhow::Result<()> {
    let Some(font) = bundled_font() else {
        // The font lives in the cargo registry, which a vendored or
        // offline build may lay out differently. Not finding it is not
        // evidence of a problem.
        eprintln!("bundled font not located; skipping");
        return Ok(());
    };
    let covered = coverage(&std::fs::read(&font)?)?;
    assert!(
        covered.len() > 200,
        "the parsed character map looks wrong: {} characters",
        covered.len()
    );

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lang");
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text = std::fs::read_to_string(&path)?;
        // Only the values: a comment is for whoever reads the file, and
        // never reaches a window.
        let mut missing: Vec<char> = text
            .lines()
            .filter(|line| line.starts_with('"'))
            .flat_map(|line| line.chars())
            .filter(|ch| !ch.is_control() && !covered.contains(ch))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        if !missing.is_empty() {
            let sample: String = missing.iter().take(20).collect();
            problems.push(format!(
                "{name}: {} characters, e.g. {sample}",
                missing.len()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "these catalogues use characters the bundled UI font cannot draw, so they would \
         render as empty boxes. Bundle a font that covers them before shipping the \
         language: {problems:#?}"
    );
    Ok(())
}

/// Where cargo unpacked epaint's default fonts.
fn bundled_font() -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|dir| dir.join(".cargo")))?;
    let registry = home.join("registry").join("src");
    for index in std::fs::read_dir(registry).ok()?.flatten() {
        let Ok(crates) = std::fs::read_dir(index.path()) else {
            continue;
        };
        for entry in crates.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("epaint_default_fonts-") {
                continue;
            }
            let font = entry.path().join("fonts").join("Ubuntu-Light.ttf");
            if font.exists() {
                return Some(font);
            }
        }
    }
    None
}

/// Every character the font's `cmap` maps, read out by hand.
///
/// Format 4 only, which is the sub-table every Windows-compatible font
/// carries and the one Ubuntu-Light uses. A font that offered nothing but
/// format 12 would come back empty, which the caller checks for rather
/// than passing silently.
fn coverage(data: &[u8]) -> anyhow::Result<HashSet<char>> {
    let read_u16 = |at: usize| -> anyhow::Result<u16> {
        let bytes = data
            .get(at..at + 2)
            .ok_or_else(|| anyhow::anyhow!("font ends at {at}"))?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    };
    let read_u32 = |at: usize| -> anyhow::Result<u32> {
        let bytes = data
            .get(at..at + 4)
            .ok_or_else(|| anyhow::anyhow!("font ends at {at}"))?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };

    let tables = read_u16(4)? as usize;
    let mut cmap = None;
    for index in 0..tables {
        let record = 12 + 16 * index;
        if data.get(record..record + 4) == Some(b"cmap") {
            cmap = Some(read_u32(record + 8)? as usize);
        }
    }
    let cmap = cmap.ok_or_else(|| anyhow::anyhow!("the font has no cmap table"))?;

    // The last format-4 sub-table wins, matching what the probe script
    // this test grew out of did; every one of them covers the BMP.
    let subtables = read_u16(cmap + 2)? as usize;
    let mut format4 = None;
    for index in 0..subtables {
        let record = cmap + 4 + 8 * index;
        let at = cmap + read_u32(record + 4)? as usize;
        if read_u16(at)? == 4 {
            format4 = Some(at);
        }
    }
    let at = format4.ok_or_else(|| anyhow::anyhow!("no format 4 cmap sub-table"))?;

    let segments = read_u16(at + 6)? as usize / 2;
    let mut covered = HashSet::new();
    for segment in 0..segments {
        let end = read_u16(at + 14 + segment * 2)?;
        let start = read_u16(at + 16 + segments * 2 + segment * 2)?;
        if start == 0xFFFF {
            continue;
        }
        for code in start..=end.min(0xFFFE) {
            if let Some(ch) = char::from_u32(code as u32) {
                covered.insert(ch);
            }
        }
    }
    Ok(covered)
}
