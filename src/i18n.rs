// ============================================================================
// Module:       i18n
// Description:  The message catalogue both front ends look strings up in, its
//               on-disk format, and the language selection around it.
//
// Dependencies: toml (catalogue format), dirs (user catalogues); assets/lang
// ============================================================================

//! Message lookup for both front ends.
//!
//! Every user-facing string in a localized module is a *key* here rather
//! than a literal at the call site, and the catalogue for the active
//! language turns it back into text. English is compiled in and is the
//! fallback for every other language, so a translation that is missing a
//! key — or a key added after a translation was written — shows English
//! rather than a blank or a raw key.
//!
//! Two decisions worth knowing before adding to it.
//!
//! **Keys are dotted and describe the place, not the words.**
//! `menu.file.rescan`, not `rescan_button`. A key named after its English
//! text has to change when the English does, and every catalogue then
//! silently loses the entry.
//!
//! **Formatting stays at the call site.** The catalogue holds `{n} files`
//! and the caller substitutes; the alternative is a formatting language
//! inside the catalogue, which is a parser and an error path in exchange
//! for nothing this app needs. Substitution is by name, so a translation
//! may reorder placeholders — which is the part that actually matters
//! across languages.

use std::collections::HashMap;
use std::sync::RwLock;

/// The English catalogue, compiled in.
///
/// Not a file lookup: English is the fallback for every other language,
/// so an installation with no catalogues at all still has to produce a
/// complete UI.
const ENGLISH: &str = include_str!("../assets/lang/en.toml");

/// A language's messages, keyed the way the code asks for them.
#[derive(Default)]
struct Catalog {
    /// The active language's entries. Empty for English.
    active: HashMap<String, String>,
    /// What `language()` reports, for the settings UI.
    tag: String,
}

fn catalog() -> &'static RwLock<Catalog> {
    static CATALOG: std::sync::OnceLock<RwLock<Catalog>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(Catalog::default()))
}

fn english() -> &'static HashMap<String, String> {
    static ENGLISH_MAP: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
    ENGLISH_MAP.get_or_init(|| parse(ENGLISH).unwrap_or_default())
}

/// Parses a catalogue: a flat TOML table of key = "text".
///
/// Flat rather than nested, so a key is one string in the file exactly as
/// it is one string in the code — nesting would mean the file and the
/// call site could disagree about where a key lives.
fn parse(text: &str) -> Option<HashMap<String, String>> {
    let table: toml::Table = text.parse().ok()?;
    Some(
        table
            .into_iter()
            .filter_map(|(key, value)| Some((key, value.as_str()?.to_string())))
            .collect(),
    )
}

/// The languages this build can offer, newest lookup first.
///
/// English always, plus any `*.toml` beside the user's config. Shipping
/// translations as files rather than compiling them in is what lets
/// someone add one without rebuilding — a translation is content, and
/// content that needs a compiler is a translation nobody writes.
pub fn available() -> Vec<String> {
    let mut tags = vec!["en".to_string()];
    let Some(dir) = catalog_dir() else {
        return tags;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return tags;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if tag != "en" {
            tags.push(tag.to_string());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/// Where user-supplied catalogues live: `lang/` beside the config file.
pub fn catalog_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("rustdirstat").join("lang"))
}

/// Switches language, or back to English.
///
/// A tag with no catalogue is not an error: it selects English, which is
/// what a missing translation should look like from the user's side.
pub fn set_language(tag: &str) {
    let entries = if tag == "en" {
        HashMap::new()
    } else {
        catalog_dir()
            .map(|dir| dir.join(format!("{tag}.toml")))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| parse(&text))
            .unwrap_or_default()
    };
    if let Ok(mut catalog) = catalog().write() {
        catalog.active = entries;
        catalog.tag = tag.to_string();
    }
}

/// The active language tag, `"en"` when none has been selected.
pub fn language() -> String {
    let tag = catalog()
        .read()
        .map(|catalog| catalog.tag.clone())
        .unwrap_or_default();
    if tag.is_empty() {
        "en".to_string()
    } else {
        tag
    }
}

/// The text for `key`, in the active language, falling back to English.
///
/// A key with no entry anywhere returns the key itself. That is
/// deliberate and it is what the catalogue test looks for: a visible
/// `menu.file.rescan` on screen is a bug report, where an empty label is
/// a mystery.
pub fn tr(key: &str) -> String {
    if let Ok(catalog) = catalog().read() {
        if let Some(text) = catalog.active.get(key) {
            return text.clone();
        }
    }
    english()
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// [`tr`] with named substitutions: `tr_with("status.files", &[("n", "12")])`.
///
/// Named rather than positional so a translation can reorder them, which
/// is the half of formatting that actually differs between languages.
pub fn tr_with(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the English catalogue defines resolves to real text.
    #[test]
    fn the_english_catalogue_parses_and_is_not_empty() {
        let english = english();
        assert!(
            english.len() > 20,
            "the compiled-in catalogue should hold the UI's strings, got {}",
            english.len()
        );
        for (key, text) in english {
            assert!(!text.is_empty(), "{key} has no text");
            assert!(
                key.contains('.'),
                "{key} should be a dotted path naming where it appears"
            );
        }
    }

    /// A key nobody has translated falls back rather than disappearing.
    #[test]
    fn a_missing_translation_falls_back_to_english() {
        set_language("en");
        let known = "menu.file";
        assert_ne!(
            tr(known),
            known,
            "a key in the English catalogue resolves to its text"
        );
        // A language with no catalogue on this machine: every lookup
        // still answers, in English.
        set_language("zz-nonexistent");
        assert_ne!(tr(known), known, "and still resolves with no catalogue");
        set_language("en");
    }

    /// An unknown key shows itself, which is a bug report rather than a
    /// blank space.
    #[test]
    fn an_unknown_key_shows_itself() {
        set_language("en");
        assert_eq!(tr("nothing.defines.this"), "nothing.defines.this");
    }

    /// Substitution is by name, so a translation may reorder.
    #[test]
    fn substitution_is_by_name() {
        set_language("en");
        let text = tr_with(
            "status.scan_counts",
            &[("files", "12"), ("folders", "3"), ("size", "4 KB")],
        );
        assert!(text.contains("12") && text.contains('3'), "got {text}");
        assert!(
            !text.contains('{'),
            "every placeholder should have been filled: {text}"
        );
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::*;

    /// Every key the code asks for exists in the English catalogue.
    ///
    /// The failure mode this stops is silent and permanent: `tr` on an
    /// unknown key returns the key, so a typo ships as `menu.file.rescn`
    /// painted into the menu bar. Scanning the source for `tr("…")` is
    /// crude and exactly good enough — the keys are literals by
    /// convention, and a key built at runtime is the thing this test
    /// should refuse to bless anyway.
    #[test]
    fn every_key_the_code_uses_is_in_the_catalogue() -> anyhow::Result<()> {
        let english = english();
        let mut missing = Vec::new();
        let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path)?;
                let file = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();
                // The module that defines the lookup quotes example keys
                // in its own docs and tests.
                if file == "i18n.rs" {
                    continue;
                }
                for (index, line) in text.lines().enumerate() {
                    for key in keys_in(line) {
                        if !english.contains_key(&key) {
                            missing.push(format!("{file}:{} asks for {key}", index + 1));
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these keys are used but not defined in assets/lang/en.toml: {missing:#?}"
        );
        Ok(())
    }

    /// Pulls `tr("…")` / `tr_with("…"` keys out of one line of source.
    ///
    /// The call has to *start* at the match: `tr("` is a substring of
    /// `str("`, and the first version of this reported `.*` from a regex
    /// in the search module as a missing message key.
    fn keys_in(line: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for call in ["tr(\"", "tr_with(\""] {
            let mut offset = 0usize;
            while let Some(at) = line[offset..].find(call) {
                let at = offset + at;
                let preceded_by_name = line[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let after = &line[at + call.len()..];
                offset = at + call.len();
                if preceded_by_name {
                    continue;
                }
                let Some(end) = after.find('"') else {
                    break;
                };
                let key = &after[..end];
                if key.contains('.') && !key.contains(' ') {
                    keys.push(key.to_string());
                }
            }
        }
        keys
    }

    /// A translation file may be partial, but it may not be *wrong*: every
    /// key it defines has to be one the app actually asks for, or it is a
    /// typo that will never be seen.
    #[test]
    fn a_shipped_translation_defines_only_real_keys() -> anyhow::Result<()> {
        let english = english();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lang");
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
            if name == "en.toml" {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let Some(entries) = parse(&text) else {
                anyhow::bail!("{name} is not a readable catalogue");
            };
            let unknown: Vec<&String> = entries
                .keys()
                .filter(|key| !english.contains_key(*key))
                .collect();
            assert!(
                unknown.is_empty(),
                "{name} defines keys the app never asks for: {unknown:?}"
            );
        }
        Ok(())
    }
}
