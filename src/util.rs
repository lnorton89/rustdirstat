// ============================================================================
// Module:       util
// Description:  Cross-cutting helpers: byte and timestamp formatting, display-
//               safe paths, the clipboard, and the platform open/move shims.
//
// Dependencies: std::process::Command (platform launchers and clipboard),
//               trash
// ============================================================================

//! Cross-cutting helpers with no better home: byte and timestamp
//! formatting, display-safe paths, the clipboard, and the platform shims
//! for opening and moving a file.
//!
//! "Display-safe" is the non-obvious one. `Path::canonicalize` on Windows
//! returns an extended-length path — `\\?\C:\Users\...` — which is the
//! correct thing to hand the filesystem and the wrong thing to put in a
//! title bar. [`display_path`] strips that prefix for display only; the
//! canonical path is still what reaches the OS.

/// Format a byte count the way WinDirStat does: base-1024 units with one
/// decimal place beyond bytes.
/// A path as a person would write it, for showing on screen.
///
/// `Path::canonicalize` on Windows returns an extended-length path —
/// `\\?\C:\Users\...` — which is the correct thing to hand to the
/// filesystem and the wrong thing to put in a title bar. The prefix is
/// an instruction to the Win32 path parser, not part of the name, and it
/// appeared at the front of every heading, status line and tooltip in
/// the app.
///
/// Only ever used for display. The canonical path is still what gets
/// passed to the OS.
pub fn display_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    // The UNC form `\\?\UNC\server\share` has to become `\\server\share`
    // rather than losing its leading slashes entirely.
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

/// Just the name of the thing a path points at, for places that only
/// need to say *which* folder rather than where it lives.
///
/// Falls back to the whole displayable path for roots like `C:\`, which
/// have no file name of their own.
pub fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| display_path(path))
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{} {}", bytes, UNITS[0]);
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

/// Format an integer with thousands separators (e.g. `8_924_548` -> `"8,924,548"`).
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// Format a modification time as `YYYY-MM-DD HH:MM` (UTC). Implemented by
/// hand (Howard Hinnant's `civil_from_days` algorithm) instead of pulling in
/// a date/time crate for what's ultimately a display-only field.
pub fn format_modified(t: Option<std::time::SystemTime>) -> String {
    let Some(t) = t else { return "-".to_string() };
    let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) else {
        return "-".to_string();
    };
    let secs = d.as_secs();
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, m, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        y,
        m,
        day,
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Public-domain "days since the Unix epoch" -> (year, month, day)
/// conversion (proleptic Gregorian calendar), by Howard Hinnant.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Open `path`'s containing folder (or the path itself, if a directory) in
/// the platform's native file manager — a "reveal"/"show in folder", not
/// opening the item itself. See `open_path` for that.
pub fn open_in_file_manager(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Opens `path` itself with its default handler — a file launches its
/// associated app (the same as double-clicking it), a directory opens in
/// the file manager. All three launchers below already behave this way
/// when given the item's own path directly (as opposed to
/// `open_in_file_manager`, which deliberately passes a file's *parent*
/// instead, to reveal/select it rather than opening it).
pub fn open_path(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Copies `text` to the OS clipboard by shelling out to a platform clipboard
/// utility, rather than adding a clipboard-access crate dependency for what's
/// used in exactly one place. `wl-copy`/`xclip` fork into the background to
/// keep serving the selection after this process exits (X11/Wayland
/// selections are "owned" by a live process, not stored centrally the way
/// Windows/macOS clipboards are) — not waiting on the spawned child and not
/// keeping its handle around is deliberate, not an oversight.
///
/// On Windows the text goes over as UTF-16LE behind a byte-order mark,
/// which is the one encoding `clip.exe` reads unambiguously. Raw UTF-8
/// used to be piped straight in, and `clip.exe` decodes unmarked input
/// using the console code page — so every path with an accent or a CJK
/// character in it arrived mangled. Copying paths is the entire purpose
/// of this function.
pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "windows")]
    let mut child = Command::new("clip").stdin(Stdio::piped()).spawn()?;

    #[cfg(target_os = "macos")]
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut child = match Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(_) => Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()?,
    };

    #[cfg(target_os = "windows")]
    let payload = utf16le_with_bom(text);
    #[cfg(not(target_os = "windows"))]
    let payload = text.as_bytes().to_vec();

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&payload)?;
        // Dropped before waiting, so the child sees end-of-input rather
        // than blocking on a pipe that is still open.
        drop(stdin);
    }

    // Waited for only where the clipboard is a central store the child
    // fills and exits. Under X11/Wayland the child *is* the clipboard for
    // as long as the selection lives, so waiting here would block until
    // the user next copied something. That asymmetry is why the failure
    // of `clip`/`pbcopy` went unreported before: nothing ever looked.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let status = child.wait()?;
        if !status.success() {
            return Err(std::io::Error::other(format!(
                "the clipboard helper exited with {status}"
            )));
        }
    }
    Ok(())
}

/// UTF-16LE with a leading byte-order mark, the form `clip.exe` detects.
#[cfg(target_os = "windows")]
fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2 + 2);
    out.extend_from_slice(&[0xFF, 0xFE]);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// Moves `source` to `dest`. Tries a plain rename first (atomic, cheap —
/// works whenever both paths are on the same filesystem); falls back to a
/// recursive copy-then-delete when that fails, most commonly because
/// source and dest are on different volumes, which `rename(2)`/`MoveFile`
/// can't do atomically. Refuses to silently overwrite an existing `dest`.
pub fn move_path(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if dest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", dest.display()),
        ));
    }
    if std::fs::rename(source, dest).is_ok() {
        return Ok(());
    }
    // rename failed — most commonly because source and dest are on
    // different volumes, which rename can never bridge — so fall back to
    // copy-then-remove. `symlink_metadata` (never follows the link
    // itself) so a symlink is recreated as a symlink at `dest` rather
    // than silently resolved and copied as if it were its target
    // (potentially huge, or entirely outside the scanned tree) —
    // `copy_dir_recursive` below already makes this same distinction for
    // a symlink nested *inside* a moved directory; this is the top-level
    // single-item case that was missed.
    let meta = std::fs::symlink_metadata(source)?;
    let is_symlink = meta.file_type().is_symlink();
    let is_dir = meta.is_dir();

    let copy_result = if is_symlink {
        recreate_symlink(source, dest)
    } else if is_dir {
        copy_dir_recursive(source, dest)
    } else {
        std::fs::copy(source, dest).map(|_| ())
    };
    if let Err(e) = copy_result {
        // Whatever landed at `dest` (nothing, or a partial copy) is
        // cleaned up so a retry isn't immediately blocked by the
        // AlreadyExists check above, and a failed move doesn't silently
        // double disk usage by leaving a copy sitting next to the
        // still-intact original.
        let _ = remove_path(dest, is_dir, is_symlink);
        return Err(e);
    }

    remove_path(source, is_dir, is_symlink)
}

/// Recreates `source` (a symlink) at `dest` rather than following it — see
/// the comment in `move_path` for why.
fn recreate_symlink(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    let link_target = std::fs::read_link(source)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_target, dest)
    }
    #[cfg(windows)]
    {
        if link_target.is_dir() {
            std::os::windows::fs::symlink_dir(&link_target, dest)
        } else {
            std::os::windows::fs::symlink_file(&link_target, dest)
        }
    }
}

/// Removes whichever of a real directory, a symlink, or a plain file
/// `path` is. Symlinks need special care on Windows only: a directory
/// symlink/junction carries `FILE_ATTRIBUTE_DIRECTORY`, and Windows'
/// `DeleteFileW` (what `remove_file` calls) refuses to touch anything with
/// that attribute set — it has to go through `RemoveDirectoryW`
/// (`remove_dir`) instead, same as an empty real directory, even though
/// it's not one. `symlink_metadata` never resolves the link, so whether
/// it points at a directory has to be checked here, by following it once,
/// purely to pick the correct removal call. Unix has no such distinction
/// (`remove_file`/`unlink` removes any symlink regardless of what it
/// points to), so `cfg!(windows)` keeps this a no-op there.
fn remove_path(path: &std::path::Path, is_dir: bool, is_symlink: bool) -> std::io::Result<()> {
    if is_dir {
        return std::fs::remove_dir_all(path);
    }
    if is_symlink && cfg!(windows) && std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        return std::fs::remove_dir(path);
    }
    std::fs::remove_file(path)
}

/// Copies a directory tree, iteratively.
///
/// The name is now a small lie, kept because it says what it does. It
/// used to call itself once per directory level, which put the depth of
/// whatever the user was moving on the call stack — and depth is theirs
/// to choose, not ours. The worklist holds one entry per directory still
/// to visit, which is heap and can simply be large.
fn copy_dir_recursive(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    let mut pending = vec![(source.to_path_buf(), dest.to_path_buf())];
    while let Some((source, dest)) = pending.pop() {
        std::fs::create_dir_all(&dest)?;
        for entry in std::fs::read_dir(&source)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let dest_path = dest.join(entry.file_name());
            if ty.is_symlink() {
                let link_target = std::fs::read_link(entry.path())?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(&link_target, &dest_path)?;
                #[cfg(windows)]
                {
                    if link_target.is_dir() {
                        std::os::windows::fs::symlink_dir(&link_target, &dest_path)?;
                    } else {
                        std::os::windows::fs::symlink_file(&link_target, &dest_path)?;
                    }
                }
            } else if ty.is_dir() {
                pending.push((entry.path(), dest_path));
            } else {
                std::fs::copy(entry.path(), &dest_path)?;
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    // Unix-only: creating a symlink on Windows needs elevated privileges
    // or developer mode, which a test environment can't assume. Exercises
    // `recreate_symlink` directly rather than the full `move_path`, since
    // reliably forcing a genuine cross-device rename failure (the only
    // way `move_path` itself reaches this code) isn't something a test
    // environment can depend on having two filesystems for.
    #[test]
    fn move_fallback_preserves_symlinks_instead_of_copying_target() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "rustdirstat_test_{}_{}",
            std::process::id(),
            "move_fallback_preserves_symlinks"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;

        let target = dir.join("target.txt");
        std::fs::write(&target, b"hello")?;
        let link = dir.join("link");
        std::os::unix::fs::symlink(&target, &link)?;

        let dest = dir.join("link_moved");
        recreate_symlink(&link, &dest)?;

        let meta = std::fs::symlink_metadata(&dest)?;
        assert!(
            meta.file_type().is_symlink(),
            "moving a symlink across the fallback path should recreate a \
             symlink at the destination, not copy the bytes it points to"
        );
        assert_eq!(std::fs::read_link(&dest)?, target);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}

/// Windows-only, because the encoding it checks is: the crate denies
/// dead code, so the helper itself is gated too rather than sitting
/// unused in every Linux and macOS build.
#[cfg(all(test, target_os = "windows"))]
mod clipboard_tests {
    use super::*;

    /// The Windows clipboard payload is UTF-16LE behind a BOM.
    ///
    /// Raw UTF-8 used to be piped to `clip.exe`, which decodes unmarked
    /// input using the console code page — so a path with an accent or a
    /// CJK character in it landed on the clipboard mangled. Copying
    /// paths is the whole point of the function.
    #[test]
    fn clipboard_text_is_encoded_for_clip_exe() {
        let bytes = utf16le_with_bom("a/Ω/文.txt");
        assert_eq!(
            &bytes[..2],
            &[0xFF, 0xFE],
            "clip.exe only reads UTF-16 unambiguously when it is marked as such"
        );

        // Decoded back, it must be the same string.
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .filter_map(|pair| <[u8; 2]>::try_from(pair).ok())
            .map(u16::from_le_bytes)
            .collect();
        let decoded = String::from_utf16(&units);
        assert!(decoded.is_ok(), "the payload should decode as UTF-16");
        assert_eq!(decoded.unwrap_or_default(), "a/Ω/文.txt");

        // ASCII is not passed through as bytes either: two 16-bit units
        // behind the mark, which is what catches a plain byte copy.
        assert_eq!(utf16le_with_bom("ab").len(), 2 + 4);
    }
}

/// Cross-platform, unlike the symlink tests above: nothing here needs
/// privileges to create.
#[cfg(test)]
mod copy_tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "rustdirstat_copy_{}_{name}_{unique}",
            std::process::id()
        ))
    }

    /// A copied tree keeps its shape and its contents.
    ///
    /// The walk was rewritten from one call per directory level to a
    /// worklist, because the depth being walked is whatever the user
    /// asked to move rather than anything this code chooses. Nesting is
    /// the part a worklist gets wrong when it gets it wrong: a flat
    /// result, or children copied to the wrong parent.
    #[test]
    fn a_copied_tree_keeps_its_nesting_and_contents() -> anyhow::Result<()> {
        let root = scratch("nesting");
        let _ = fs::remove_dir_all(&root);
        let source = root.join("source");
        fs::create_dir_all(source.join("a").join("deep"))?;
        fs::create_dir_all(source.join("b"))?;
        fs::write(source.join("top.txt"), b"top")?;
        fs::write(source.join("a").join("mid.txt"), b"mid")?;
        fs::write(source.join("a").join("deep").join("leaf.txt"), b"leaf")?;
        fs::write(source.join("b").join("other.txt"), b"other")?;

        // Nested well past one level, so a walk that loses track of which
        // destination a directory belongs under cannot pass.
        let mut chain = source.join("chain");
        for i in 0..40 {
            chain = chain.join(format!("d{i}"));
        }
        fs::create_dir_all(&chain)?;
        fs::write(chain.join("bottom.txt"), b"bottom")?;

        let dest = root.join("dest");
        copy_dir_recursive(&source, &dest)?;

        for (relative, expected) in [
            ("top.txt", "top"),
            ("a/mid.txt", "mid"),
            ("a/deep/leaf.txt", "leaf"),
            ("b/other.txt", "other"),
        ] {
            let mut path = dest.clone();
            for part in relative.split('/') {
                path = path.join(part);
            }
            let found = fs::read_to_string(&path);
            assert!(found.is_ok(), "{relative} should exist under the copy");
            assert_eq!(found.unwrap_or_default(), expected, "{relative}");
        }

        let mut copied_chain = dest.join("chain");
        for i in 0..40 {
            copied_chain = copied_chain.join(format!("d{i}"));
        }
        let bottom = fs::read_to_string(copied_chain.join("bottom.txt"));
        assert!(
            bottom.is_ok(),
            "the deeply nested file should be copied to the same depth"
        );
        assert_eq!(bottom.unwrap_or_default(), "bottom");

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
