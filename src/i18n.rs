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

/// The translations that ship with the program, compiled in beside
/// English.
///
/// They used to live only in the source tree. `assets/lang/` is in no
/// `.deb`, no `.rpm` and no release archive, and `set_language` read only
/// the user's config directory — so the German translation this project
/// maintains was unreachable from every installed build, and the language
/// picker offered exactly one language forever. Compiling them in is what
/// makes a shipped translation a feature rather than a file in a
/// repository: it needs no packaging, cannot be half-installed, and costs
/// tens of kilobytes.
///
/// A user's own catalogue in [`catalog_dir`] still wins over the one
/// here, so a translation can be corrected without waiting for a release.
/// `every_shipped_catalogue_is_listed_here` fails the build if a file is
/// added to `assets/lang/` and not to this table.
/// Every language here draws in the font the window bundles, and that is
/// the constraint on which languages can be here at all: egui ships
/// Ubuntu-Light, which covers Latin, Latin Extended and Cyrillic and has
/// no CJK glyphs whatever. Adding `ja`/`zh`/`ko` without also bundling a
/// CJK face would put entries in the language picker that render as rows
/// of empty boxes. `every_shipped_catalogue_draws_in_the_bundled_font`
/// enforces it.
const SHIPPED: &[(&str, &str)] = &[
    ("cs", include_str!("../assets/lang/cs.toml")),
    ("de", include_str!("../assets/lang/de.toml")),
    ("es", include_str!("../assets/lang/es.toml")),
    ("fr", include_str!("../assets/lang/fr.toml")),
    ("it", include_str!("../assets/lang/it.toml")),
    ("nl", include_str!("../assets/lang/nl.toml")),
    ("pl", include_str!("../assets/lang/pl.toml")),
    ("pt-BR", include_str!("../assets/lang/pt-BR.toml")),
    ("ru", include_str!("../assets/lang/ru.toml")),
    ("sv", include_str!("../assets/lang/sv.toml")),
    ("tr", include_str!("../assets/lang/tr.toml")),
];

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

/// The languages this build can offer.
///
/// English always; the translations in [`SHIPPED`], which are part of the
/// binary and therefore present however the program was installed; and
/// any `*.toml` beside the user's config, which is what lets someone add
/// a language without rebuilding — a translation is content, and content
/// that needs a compiler is a translation nobody writes.
pub fn available() -> Vec<String> {
    let mut tags = vec!["en".to_string()];
    tags.extend(SHIPPED.iter().map(|(tag, _)| (*tag).to_string()));
    if let Some(entries) = catalog_dir().and_then(|dir| std::fs::read_dir(dir).ok()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // `en.toml` in that folder is the starter template
            // [`write_starter_catalogue`] leaves there, not a language to
            // offer — English is already the first entry.
            if tag != "en" {
                tags.push(tag.to_string());
            }
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

/// Writes the English catalogue into [`catalog_dir`], and says where.
///
/// Without this the feature is unreachable in an installed build. A
/// translation is a copy of the English catalogue with the right-hand
/// side rewritten — but the catalogue is `include_str!`'d into the
/// binary, and no package or release archive carries a copy of it, so the
/// only way to obtain one was to clone the source. The settings page
/// said "a translation is a copy of the catalogue below" while offering a
/// folder path and no catalogue.
///
/// Deliberately written as `en.toml`, which is the one name
/// [`available`] skips and [`set_language`] never reads: it is a template
/// to be renamed, not a translation that takes effect where it lands. An
/// existing `en.toml` is overwritten for the same reason — it is a copy
/// of what is compiled in, so the only version of it worth having is this
/// build's.
pub fn write_starter_catalogue() -> std::io::Result<std::path::PathBuf> {
    let Some(dir) = catalog_dir() else {
        return Err(std::io::Error::other(
            "no configuration directory could be determined",
        ));
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("en.toml");
    std::fs::write(&path, ENGLISH)?;
    Ok(path)
}

/// Switches language, or back to English.
///
/// A tag with no catalogue is not an error: it selects English, which is
/// what a missing translation should look like from the user's side.
pub fn set_language(tag: &str) {
    let entries = if tag == "en" {
        HashMap::new()
    } else {
        // The user's own file first, then the one compiled in. That
        // order is the point: a shipped translation can be corrected or
        // extended locally without waiting for a release, by dropping a
        // file of the same name into the config directory.
        let theirs = catalog_dir()
            .map(|dir| dir.join(format!("{tag}.toml")))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| parse(&text));
        let ours = || {
            SHIPPED
                .iter()
                .find(|(shipped, _)| *shipped == tag)
                .and_then(|(_, text)| parse(text))
        };
        theirs.or_else(ours).unwrap_or_default()
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

    /// And the other direction: every key the catalogue defines is one
    /// the code actually asks for.
    ///
    /// The test above catches a key the code invents and the catalogue
    /// lacks. This catches the opposite, which is quieter and was
    /// therefore worse: seven keys — `status.scan_complete`,
    /// `status.scanning`, `status.busy`, and the rest of the status bar —
    /// sat in the catalogue while the code beside them held the English
    /// literal. Nothing was broken enough to notice. A German user saw an
    /// English status bar under a translated menu, and a translator who
    /// translated those lines saw no effect anywhere.
    ///
    /// A key is "asked for" if its quoted text appears anywhere in `src`,
    /// not only inside a `tr(` call: the inspector passes keys as plain
    /// arguments to its own `row`/`section` helpers, and those are real
    /// uses.
    #[test]
    fn every_key_the_catalogue_defines_is_asked_for() -> anyhow::Result<()> {
        let mut source = String::new();
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
                // This module quotes example keys in its own docs and
                // tests, which would vouch for keys nothing displays.
                if path.file_name().and_then(|name| name.to_str()) == Some("i18n.rs") {
                    continue;
                }
                source.push_str(&std::fs::read_to_string(&path)?);
            }
        }
        let unused: Vec<&String> = english()
            .keys()
            .filter(|key| !source.contains(&format!("\"{key}\"")))
            .collect();
        assert!(
            unused.is_empty(),
            "these keys are defined in assets/lang/en.toml but nothing asks for them, \
             which means the text they describe is hardcoded somewhere: {unused:#?}"
        );
        Ok(())
    }

    /// Every catalogue in `assets/lang/` is in the compiled-in table.
    ///
    /// `SHIPPED` is written by hand, so adding `fr.toml` to the directory
    /// without adding a line to it produces a translation that exists in
    /// the source tree and in no build — which is exactly the state
    /// German was in for a whole release.
    #[test]
    fn every_shipped_catalogue_is_listed_here() -> anyhow::Result<()> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lang");
        let listed: Vec<&str> = SHIPPED.iter().map(|(tag, _)| *tag).collect();
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(tag) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if tag == "en" {
                continue;
            }
            assert!(
                listed.contains(&tag),
                "assets/lang/{tag}.toml is in the tree but not in `SHIPPED`, so no build \
                 can offer it"
            );
        }
        Ok(())
    }

    /// A language that ships with the program is selectable without any
    /// file on disk.
    ///
    /// This is the whole point of compiling them in: `assets/lang/` is in
    /// no package and no archive, so before this a fresh install offered
    /// English and nothing else however many translations the project
    /// maintained.
    #[test]
    fn a_shipped_language_can_be_selected_with_no_files_installed() {
        for (tag, _) in SHIPPED {
            assert!(
                available().iter().any(|offered| offered == tag),
                "{tag} ships with the program, so the picker must offer it"
            );
            set_language(tag);
            assert_eq!(language(), *tag);
            assert_ne!(
                tr("menu.file"),
                "menu.file",
                "{tag} should resolve a key rather than echo it"
            );
            // Measured across the catalogue rather than on one chosen
            // key: a real translation legitimately keeps some words
            // ("File" is Italian for File, "Video" is most languages'
            // word for video), so a single-key check tests the key, not
            // the translation.
            let english = english();
            let translated = SHIPPED
                .iter()
                .find(|(shipped, _)| shipped == tag)
                .and_then(|(_, text)| parse(text))
                .unwrap_or_default();
            let differing = translated
                .iter()
                .filter(|(key, text)| english.get(*key).is_some_and(|ours| ours != *text))
                .count();
            assert!(
                differing * 2 > english.len(),
                "{tag} differs from English in only {differing} of {} entries, \
                 which is not a translation",
                english.len()
            );
        }
        set_language("en");
    }

    /// A translation *shipped in this repository* is complete.
    ///
    /// The forgiving rule — a missing key falls back to English — is for
    /// the translations people write for themselves, where a half-done
    /// file is better than none. It is the wrong rule for the one
    /// translation this project ships, which is both a feature and the
    /// worked example somebody copies: German sat at 51% of the
    /// catalogue for a whole release, so a German window showed a
    /// translated menu bar over an English everything-else, and nothing
    /// said so.
    ///
    /// Note this checks `assets/lang/`, not the user's config directory.
    /// A file someone wrote themselves is theirs to leave half done.
    #[test]
    fn a_shipped_translation_is_complete() -> anyhow::Result<()> {
        let english = english();
        for (name, entries) in shipped_translations()? {
            let missing: Vec<&String> = english
                .keys()
                .filter(|key| !entries.contains_key(*key))
                .collect();
            assert!(
                missing.is_empty(),
                "{name} is shipped with this project, so it has to cover every key. \
                 {} of {} missing: {missing:#?}",
                missing.len(),
                english.len()
            );
        }
        Ok(())
    }

    /// A translation uses the same placeholders as the English it
    /// replaces.
    ///
    /// Substitution is by name, so a translation may reorder `{files}`
    /// and `{folders}` freely — but one that invents `{dateien}`, or
    /// drops `{error}`, produces a message with a literal brace in it or
    /// with the interesting part missing. Neither shows up until someone
    /// switches language and hits that exact line.
    #[test]
    fn a_translation_keeps_the_placeholders_it_was_given() -> anyhow::Result<()> {
        let english = english();
        let mut wrong = Vec::new();
        for (name, entries) in shipped_translations()? {
            for (key, text) in &entries {
                let Some(original) = english.get(key) else {
                    continue;
                };
                let (mut theirs, mut ours) = (placeholders(text), placeholders(original));
                theirs.sort();
                ours.sort();
                if theirs != ours {
                    wrong.push(format!(
                        "{name}: {key} has {theirs:?}, English has {ours:?}"
                    ));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "a translation may reorder placeholders but not invent or drop them: {wrong:#?}"
        );
        Ok(())
    }

    /// The `{name}`s in one message.
    fn placeholders(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                break;
            };
            found.push(after[..close].to_string());
            rest = &after[close + 1..];
        }
        found
    }

    /// Every catalogue under `assets/lang/`, English excepted.
    fn shipped_translations() -> anyhow::Result<Vec<(String, HashMap<String, String>)>> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lang");
        let mut out = Vec::new();
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
            let Some(entries) = parse(&std::fs::read_to_string(&path)?) else {
                anyhow::bail!("{name} is not a readable catalogue");
            };
            out.push((name, entries));
        }
        Ok(out)
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
