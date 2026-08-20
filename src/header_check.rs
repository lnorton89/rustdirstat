// ============================================================================
// Module:       header_check
// Description:  Enforces the file-header convention: every source file opens
//               with a banner naming its module, purpose, and dependencies.
//
// Dependencies: anyhow, std::fs (walks src/ and tests/ at test time)
// ============================================================================

//! Enforces the file-header convention across every source file.
//!
//! The banner at the top of a file is the one piece of documentation
//! nothing else can imply: a reader opening a module cold gets its path in
//! the crate, what it is for, and what it leans on, before reading a line
//! of code. A convention only holds if something checks it, so this walks
//! `src/` and `tests/` and fails the build for any file whose header is
//! missing, malformed, or naming the wrong module.
//!
//! The check is structural, not editorial. It verifies that the fields are
//! present and filled in, and that `Module:` matches the path the file
//! actually sits at — which is what catches a header copied from its
//! neighbour and never re-read. Whether a description is any *good* is a
//! question for review, and this does not pretend to answer it.

#[cfg(test)]
mod tests {
    use anyhow::{anyhow, Result};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// The rule above and below every header. Both edges are checked, so a
    /// header cannot silently run on into the code below it.
    const RULE: &str =
        "// ============================================================================";

    /// Width of the label column, so `Description:` and its continuation
    /// lines share one left edge. Kept in sync with the headers by the
    /// continuation parsing in [`field`], which only accepts this indent.
    const LABEL_WIDTH: usize = 14;

    /// Fields every header must carry, in the order they must appear.
    const REQUIRED: [&str; 3] = ["Module", "Description", "Dependencies"];

    /// rustfmt's default `max_width`. A header that runs past it wraps in
    /// an editor and stops lining up, which is the whole point of it.
    const MAX_WIDTH: usize = 100;

    /// Every `.rs` file under `root`, found iteratively.
    ///
    /// Iterative rather than recursive to match the rest of the codebase —
    /// `src/` is shallow enough that it hardly matters here, but a walk
    /// that recurses is the pattern this project deliberately does not
    /// keep, and a test is not the place to make an exception to it.
    fn rust_sources(root: &Path) -> Result<Vec<PathBuf>> {
        let mut found = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    found.push(path);
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// The `Module:` value a file at `relative` is required to declare.
    ///
    /// Derived from the path rather than accepted as written, so a header
    /// copied from a neighbouring file fails instead of quietly claiming
    /// to document something else.
    fn expected_module(relative: &str) -> Result<String> {
        let (area, rest) = relative
            .split_once('/')
            .ok_or_else(|| anyhow!("{relative} is not under src/ or tests/"))?;
        let stem = rest
            .strip_suffix(".rs")
            .ok_or_else(|| anyhow!("{relative} is not a Rust source file"))?;

        if area == "tests" {
            return Ok(format!("{stem} (integration test)"));
        }
        if let Some(binary) = stem.strip_prefix("bin/") {
            return Ok(format!("rustdirstat-{binary} (binary crate root)"));
        }
        match stem {
            "lib" => return Ok("rustdirstat (library crate root)".to_string()),
            "main" => return Ok("rustdirstat (binary crate root)".to_string()),
            _ => {}
        }
        // `foo/mod.rs` documents `foo`, not `foo::mod`.
        let module = stem.strip_suffix("/mod").unwrap_or(stem);
        Ok(module.replace('/', "::"))
    }

    /// One field's value, with any aligned continuation lines folded in.
    ///
    /// A continuation is recognised only by the label-column indent, so a
    /// following field (whose label starts at column 3) ends the value
    /// rather than being swallowed into it.
    fn field(body: &[&str], label: &str) -> Option<String> {
        let prefix = format!("// {label}:");
        let start = body.iter().position(|line| line.starts_with(&prefix))?;
        let indent = format!("// {}", " ".repeat(LABEL_WIDTH));

        let mut value = body.get(start)?.get(prefix.len()..)?.trim().to_string();
        for line in body.get(start + 1..)? {
            let Some(rest) = line.strip_prefix(&indent) else {
                break;
            };
            value.push(' ');
            value.push_str(rest.trim());
        }
        Some(value)
    }

    /// Checks one file, returning a description of each problem found.
    ///
    /// Returns every problem rather than the first, so a single run tells
    /// you everything to fix instead of one thing per `cargo test`.
    fn problems(relative: &str, text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let lines: Vec<&str> = text.lines().collect();

        if lines.first() != Some(&RULE) {
            found.push(format!("{relative}: does not open with the header rule"));
            return found;
        }
        let Some(close) = lines.iter().skip(1).position(|line| *line == RULE) else {
            found.push(format!("{relative}: header rule is never closed"));
            return found;
        };
        let body = &lines[1..=close];

        for line in body {
            if !line.starts_with("// ") && *line != "//" {
                found.push(format!(
                    "{relative}: non-comment line inside header: {line}"
                ));
            }
            if line.chars().count() > MAX_WIDTH {
                found.push(format!(
                    "{relative}: header line exceeds {MAX_WIDTH} columns"
                ));
            }
        }

        for label in REQUIRED {
            let Some(value) = field(body, label) else {
                found.push(format!("{relative}: header is missing `{label}:`"));
                continue;
            };
            if value.is_empty() {
                found.push(format!("{relative}: `{label}:` is empty"));
            } else if value.starts_with('[') {
                found.push(format!(
                    "{relative}: `{label}:` still holds the template placeholder"
                ));
            }
        }

        match (field(body, "Module"), expected_module(relative)) {
            (Some(declared), Ok(expected)) if declared != expected => found.push(format!(
                "{relative}: declares `Module: {declared}` but sits at `{expected}`"
            )),
            (_, Err(error)) => found.push(format!("{relative}: {error}")),
            _ => {}
        }

        found
    }

    #[test]
    fn every_source_file_carries_a_module_header() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut failures = Vec::new();
        let mut checked = 0usize;

        for area in ["src", "tests"] {
            let dir = root.join(area);
            if !dir.is_dir() {
                continue;
            }
            for path in rust_sources(&dir)? {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = fs::read_to_string(&path)?;
                failures.extend(problems(&relative, &text));
                checked += 1;
            }
        }

        assert!(
            checked > 0,
            "found no source files to check under {}",
            root.display()
        );
        assert!(
            failures.is_empty(),
            "{} file header problem(s) across {checked} file(s):\n{}",
            failures.len(),
            failures.join("\n")
        );
        Ok(())
    }

    #[test]
    fn a_module_path_is_derived_from_where_the_file_sits() -> Result<()> {
        let cases = [
            ("src/lib.rs", "rustdirstat (library crate root)"),
            ("src/main.rs", "rustdirstat (binary crate root)"),
            ("src/bin/gui.rs", "rustdirstat-gui (binary crate root)"),
            ("src/color.rs", "color"),
            ("src/gui/mod.rs", "gui"),
            ("src/gui/ui/mod.rs", "gui::ui"),
            ("src/gui/ui/theme.rs", "gui::ui::theme"),
            ("tests/quit_stress.rs", "quit_stress (integration test)"),
        ];
        for (path, expected) in cases {
            assert_eq!(
                expected_module(path)?,
                expected,
                "wrong module path derived for {path}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_header_naming_the_wrong_module_is_rejected() {
        let text = format!(
            "{RULE}\n\
             // Module:       gui::ui::theme\n\
             // Description:  Something.\n\
             //\n\
             // Dependencies: None.\n\
             {RULE}\n"
        );
        let found = problems("src/color.rs", &text);
        assert!(
            found.iter().any(|problem| problem.contains("but sits at")),
            "a mismatched Module: should be reported, got {found:?}"
        );
    }

    #[test]
    fn a_missing_or_placeholder_field_is_rejected() {
        let missing = format!(
            "{RULE}\n\
             // Module:       color\n\
             // Dependencies: None.\n\
             {RULE}\n"
        );
        let found = problems("src/color.rs", &missing);
        assert!(
            found.iter().any(|problem| problem.contains("Description:")),
            "a missing Description: should be reported, got {found:?}"
        );

        let placeholder = format!(
            "{RULE}\n\
             // Module:       color\n\
             // Description:  [Brief description of the file's purpose]\n\
             //\n\
             // Dependencies: None.\n\
             {RULE}\n"
        );
        let found = problems("src/color.rs", &placeholder);
        assert!(
            found.iter().any(|problem| problem.contains("placeholder")),
            "an unfilled template should be reported, got {found:?}"
        );
    }

    #[test]
    fn a_field_wrapped_onto_the_label_column_reads_as_one_value() {
        let body = [
            "// Module:       color",
            "// Description:  First half of the sentence",
            "//               and its continuation.",
            "//",
            "// Dependencies: ratatui",
        ];
        assert_eq!(
            field(&body, "Description"),
            Some("First half of the sentence and its continuation.".to_string()),
            "continuation lines should fold into the field they belong to"
        );
        assert_eq!(
            field(&body, "Module"),
            Some("color".to_string()),
            "a field must not swallow the field below it"
        );
    }
}
