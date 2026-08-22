// ============================================================================
// Module:       cleanups
// Description:  User-defined commands run against the selected item, and the
//               substitution rules that keep a filename from becoming syntax.
//
// Dependencies: serde (config format), std::process; crate::config
// ============================================================================

//! User-defined cleanup commands — WinDirStat's "Cleanups".
//!
//! A cleanup is a name, a program, and a list of arguments with
//! placeholders standing in for the selected item. The app substitutes,
//! launches, and reports; it never interprets.
//!
//! The rules below are not style choices. They come from
//! [`docs/CLEANUPS_THREAT_MODEL.md`](../docs/CLEANUPS_THREAT_MODEL.md),
//! whose short version is that **file names are attacker-controlled
//! text**: anyone who can write into a scanned directory chooses what
//! this module substitutes, and they do not need access to the machine to
//! do it — an unzipped archive is enough. So:
//!
//! - There is no shell. Ever. The program is launched with an argv array,
//!   nothing is handed to `sh -c` or `cmd /c`, and no string is ever
//!   split into arguments. A file called `; rm -rf ~` is one argument
//!   that happens to contain a semicolon.
//! - Substitution happens *inside* one argument and cannot produce a
//!   second one, so a name can never become a new flag.
//! - An unknown placeholder is an error rather than an empty string: `%q`
//!   silently becoming nothing is how a command quietly runs against the
//!   wrong thing.
//! - Nothing here is configured by default. An app that shipped with
//!   cleanups would be shipping commands nobody read.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// One configured command.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cleanup {
    /// What the menu calls it.
    pub name: String,
    /// The program to run. Taken literally: no `PATH`-guessing beyond
    /// what the OS itself does when it launches, and no shell.
    pub program: String,
    /// Arguments, each one its own argv element. Placeholders are
    /// substituted within an element and never across them.
    #[serde(default)]
    pub args: Vec<String>,
    /// Whether to ask first. Defaults to *yes*, and a cleanup that wants
    /// otherwise has to say so in its own entry.
    #[serde(default = "yes")]
    pub confirm: bool,
    /// Whether the command's output is worth waiting for and showing.
    /// A cleanup that opens a window (a shell, a file manager) sets this
    /// false and is launched detached.
    #[serde(default = "yes")]
    pub capture_output: bool,
}

fn yes() -> bool {
    true
}

/// What went wrong before anything was launched.
#[derive(Debug, PartialEq, Eq)]
pub enum CleanupError {
    /// A placeholder this module does not define. Refused rather than
    /// guessed at.
    UnknownPlaceholder(char),
    /// A `%` at the very end of an argument, which names no placeholder.
    TrailingPercent,
    /// The selection did not resolve. Exactly like the delete path: a
    /// stale index path must not act on the nearest surviving ancestor.
    NoTarget,
}

impl std::error::Error for CleanupError {}

impl std::fmt::Display for CleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlaceholder(c) => {
                write!(f, "unknown placeholder %{c} — use %p, %n, %d or %%")
            }
            Self::TrailingPercent => write!(f, "an argument ends with % and names no placeholder"),
            Self::NoTarget => write!(f, "select an item for the cleanup to act on"),
        }
    }
}

/// The command a cleanup resolves to for one target, ready to run and to
/// show.
#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
}

impl Command {
    /// The command as one readable line, for the confirmation.
    ///
    /// Presentation only: quoting here is for a human reading it, and
    /// nothing ever parses this back into arguments. That is the whole
    /// reason it is safe to render at all.
    pub fn preview(&self) -> String {
        let mut out = quote_for_display(&self.program);
        for arg in &self.args {
            out.push(' ');
            out.push_str(&quote_for_display(arg));
        }
        out
    }
}

fn quote_for_display(text: &str) -> String {
    if text.is_empty() || text.contains(char::is_whitespace) || text.contains('"') {
        format!("\"{}\"", text.replace('"', "\\\""))
    } else {
        text.to_string()
    }
}

/// Substitutes a cleanup's placeholders for one target path.
///
/// The target is a real, exactly-resolved path — see the threat model on
/// why the forgiving lookup is not allowed anywhere near this.
pub fn resolve(cleanup: &Cleanup, target: &Path) -> Result<Command, CleanupError> {
    let full = target.to_string_lossy().to_string();
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| full.clone());
    let parent = target
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| full.clone());

    let mut args = Vec::with_capacity(cleanup.args.len());
    for arg in &cleanup.args {
        args.push(substitute(arg, &full, &name, &parent)?);
    }
    Ok(Command {
        program: cleanup.program.clone(),
        args,
    })
}

/// One argument, with its placeholders replaced.
///
/// Returns a single `String` by construction: whatever a filename
/// contains, it lands inside the argument it was substituted into and
/// cannot become another one.
fn substitute(arg: &str, full: &str, name: &str, parent: &str) -> Result<String, CleanupError> {
    let mut out = String::with_capacity(arg.len());
    let mut chars = arg.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p') => out.push_str(full),
            Some('n') => out.push_str(name),
            Some('d') => out.push_str(parent),
            Some('%') => out.push('%'),
            Some(other) => return Err(CleanupError::UnknownPlaceholder(other)),
            None => return Err(CleanupError::TrailingPercent),
        }
    }
    Ok(out)
}

/// Runs a resolved command.
///
/// `std::process::Command` with an argv array, which is the whole
/// security property: there is no shell to reinterpret anything, and
/// nothing here builds a command line as text.
pub fn run(command: &Command, capture_output: bool) -> Result<crate::wintools::ToolOutput, String> {
    let mut process = std::process::Command::new(&command.program);
    process.args(&command.args);

    if !capture_output {
        return process
            .spawn()
            .map(|_| crate::wintools::ToolOutput {
                summary: format!("Launched {}", command.program),
                detail: String::new(),
            })
            .map_err(|error| format!("Failed to launch {}: {error}", command.program));
    }

    let output = process
        .output()
        .map_err(|error| format!("Failed to run {}: {error}", command.program))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(crate::wintools::ToolOutput {
            summary: format!("{} completed", command.program),
            detail: stdout,
        })
    } else {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        Err(format!(
            "{} exited with {}: {detail}",
            command.program, output.status
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cleanup(args: &[&str]) -> Cleanup {
        Cleanup {
            name: "test".to_string(),
            program: "echo".to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            confirm: true,
            capture_output: true,
        }
    }

    /// A filename full of shell syntax is one argument, not a script.
    ///
    /// The reason the whole feature is argv-only. This name is what an
    /// unzipped archive can put on disk, and every character in it has to
    /// survive as data.
    #[test]
    fn a_filename_that_looks_like_shell_syntax_stays_one_argument() -> anyhow::Result<()> {
        let target = PathBuf::from("/tmp").join("; rm -rf ~ && echo `whoami`.txt");
        let resolved = resolve(&cleanup(&["%p"]), &target)?;

        assert_eq!(resolved.args.len(), 1, "one placeholder, one argument");
        let Some(only) = resolved.args.first() else {
            anyhow::bail!("the argument should be there");
        };
        assert!(
            only.contains("; rm -rf ~ && echo `whoami`.txt"),
            "the name should arrive intact as data: {only}"
        );
        Ok(())
    }

    /// A name that looks like a flag cannot become one.
    #[test]
    fn a_name_that_looks_like_a_flag_stays_inside_its_argument() -> anyhow::Result<()> {
        let target = PathBuf::from("/tmp").join("--delete-everything");
        let resolved = resolve(&cleanup(&["--path=%p", "--"]), &target)?;

        assert_eq!(
            resolved.args.len(),
            2,
            "the argument list is what was configured"
        );
        let Some(first) = resolved.args.first() else {
            anyhow::bail!("missing argument");
        };
        assert!(
            first.starts_with("--path=/tmp"),
            "substitution happens inside the argument: {first}"
        );
        assert!(
            !resolved.args.iter().any(|arg| arg == "--delete-everything"),
            "and never as an argument of its own: {:?}",
            resolved.args
        );
        Ok(())
    }

    /// Every placeholder, including the escape.
    #[test]
    fn the_placeholders_are_what_the_threat_model_says() -> anyhow::Result<()> {
        let target = PathBuf::from("/home/lawrence/photos/holiday.jpg");
        let resolved = resolve(&cleanup(&["%p", "%n", "%d", "100%%"]), &target)?;

        assert_eq!(
            resolved.args,
            vec![
                "/home/lawrence/photos/holiday.jpg".to_string(),
                "holiday.jpg".to_string(),
                "/home/lawrence/photos".to_string(),
                "100%".to_string(),
            ]
        );
        Ok(())
    }

    /// An unknown placeholder refuses rather than vanishing.
    ///
    /// `%q` becoming an empty string is how a command quietly runs
    /// against the working directory instead of the selection.
    #[test]
    fn an_unknown_placeholder_is_refused() {
        let target = PathBuf::from("/tmp/file.txt");
        assert_eq!(
            resolve(&cleanup(&["%q"]), &target),
            Err(CleanupError::UnknownPlaceholder('q'))
        );
        assert_eq!(
            resolve(&cleanup(&["trailing%"]), &target),
            Err(CleanupError::TrailingPercent)
        );
    }

    /// The confirmation shows the real command, quoted for reading only.
    #[test]
    fn the_preview_shows_every_argument() -> anyhow::Result<()> {
        let target = PathBuf::from("/tmp/two words.txt");
        let resolved = resolve(&cleanup(&["-v", "%p"]), &target)?;
        let preview = resolved.preview();

        assert!(preview.starts_with("echo -v "), "got {preview}");
        assert!(
            preview.contains("\"/tmp/two words.txt\""),
            "a path with a space is readable as one thing: {preview}"
        );
        Ok(())
    }

    /// Defaults are the safe ones: confirm, and no cleanups at all.
    #[test]
    fn a_config_without_cleanups_has_none() -> anyhow::Result<()> {
        let parsed: Cleanup = toml::from_str(
            r#"
            name = "Open a shell here"
            program = "bash"
            "#,
        )?;
        assert!(parsed.confirm, "confirmation is the default");
        assert!(parsed.args.is_empty());
        assert!(
            crate::config::Config::default().cleanups.is_empty(),
            "nothing is configured out of the box"
        );
        Ok(())
    }
}
