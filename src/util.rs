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
    launch(path)
}

/// Opens `path` itself with its default handler — a file launches its
/// associated app (the same as double-clicking it), a directory opens in
/// the file manager. All three launchers below already behave this way
/// when given the item's own path directly (as opposed to
/// `open_in_file_manager`, which deliberately passes a file's *parent*
/// instead, to reveal/select it rather than opening it).
pub fn open_path(path: &std::path::Path) -> std::io::Result<()> {
    launch(path)
}

/// Hands `path` to the platform's shell launcher and returns immediately.
///
/// The three launchers all mean "do the default thing with this", so
/// what separates revealing an item from opening it is entirely which
/// path the caller passes — the parent, or the item itself. That is why
/// [`open_in_file_manager`] and [`open_path`] are two names over one
/// body rather than two bodies: the platform table is the part that
/// must not drift, and it now exists once.
fn launch(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");

    command.arg(path).spawn()?;
    Ok(())
}

/// Copies `text` to the OS clipboard.
///
/// This used to spawn `clip`, `pbcopy`, `wl-copy` or `xclip` and pipe to
/// it, on the reasoning that one call site did not justify a dependency.
/// What that actually bought was a copy that fails on any machine
/// without the right helper installed, reports it as nothing more than
/// "Copy failed" at the moment of use, and encodes wrong: `clip.exe`
/// reads unmarked input using the console code page, so every path with
/// an accent or a CJK character in it arrived mangled — and copying
/// paths is the entire purpose of this.
///
/// `arboard` talks to each platform's clipboard directly, with no
/// external binary to be missing and no encoding to get wrong. It also
/// handles the part that made the old code subtle: an X11 or Wayland
/// selection is *owned* by a live process rather than stored centrally,
/// so something has to keep serving it after this call returns.
pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| std::io::Error::other(e.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Moves `source` to `dest`. Tries a plain rename first (atomic, cheap —
/// works whenever both paths are on the same filesystem); falls back to a
/// recursive copy-then-delete only when the rename failed for the one
/// reason copy can fix: source and dest on different volumes
/// (`rename(2)`/`MoveFile` cannot bridge a device boundary).
///
/// Two things are deliberately *not* done here. Moving a directory into
/// its own descendant is rejected up front: the copy fallback would
/// enumerate the destination it just created and recurse forever
/// (`/data/archive/moved/moved/...` until the filesystem gives up). And
/// every other rename failure is returned as-is rather than treated as a
/// cross-device signal — a permission error or an invalid name is not
/// evidence the source lives on another volume, and copying for it would
/// mask the real error while doing exactly the wrong thing. Refuses to
/// silently overwrite an existing `dest`.
pub fn move_path(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if dest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", dest.display()),
        ));
    }
    // A destination inside the source is the one case where the copy
    // fallback is not merely unhelpful but self-amplifying: once `dest`
    // exists the walk of `source` can discover it and start nesting
    // copies inside itself. Canonicalizing both sides first (so `a/../a`
    // cannot sneak past on spelling) and comparing components catches it
    // before anything touches the disk. If canonicalization fails the
    // rename below will simply error and be reported, which is safe.
    if let (Ok(canon_source), Some(parent)) = (source.canonicalize(), dest.parent()) {
        if let Ok(canon_parent) = parent.canonicalize() {
            if canon_parent.starts_with(&canon_source) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "cannot move {} into itself ({})",
                        source.display(),
                        dest.display()
                    ),
                ));
            }
        }
    }
    match std::fs::rename(source, dest) {
        Ok(()) => return Ok(()),
        Err(error) if is_cross_device(&error) => {}
        Err(error) => return Err(error),
    }
    // Cross-device, and only cross-device, reaches the copy fallback:
    // `symlink_metadata` (never follows the link itself) so a symlink is
    // recreated as a symlink at `dest` rather than silently resolved and
    // copied as if it were its target (potentially huge, or entirely
    // outside the scanned tree) — `copy_dir_recursive` below already
    // makes this same distinction for a symlink nested *inside* a moved
    // directory; this is the top-level single-item case that was missed.
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

/// Whether a rename failed because the two paths are on different
/// filesystems — the only rename failure the copy fallback can fix.
///
/// Every other error is returned to the caller rather than triggering a
/// copy: a permission denial or invalid name is not evidence of a device
/// boundary, and a copy would paper over it.
#[cfg(unix)]
fn is_cross_device(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::EXDEV)
}

#[cfg(windows)]
fn is_cross_device(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(17) // ERROR_NOT_SAME_DEVICE
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

/// A scratch directory unique to this test run, for tests that need a
/// real filesystem.
///
/// Named with the pid and a counter rather than a timestamp: two tests
/// sharing a temp directory delete it out from under each other, which
/// showed up as an intermittent `PermissionDenied` from `remove_dir_all`
/// on Windows CI. Every test module used to carry its own copy of this
/// with a different prefix; the prefix is now a parameter.
#[cfg(test)]
pub(crate) fn scratch_dir(prefix: &str, name: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "rustdirstat_{prefix}_{}_{name}_{unique}",
        std::process::id()
    ))
}

/// The move-hardening regressions: a move must never be allowed to recurse
/// into its own output, and only a genuine cross-device rename may trigger
/// the copy fallback. Unix-only: forcing a real `EXDEV` needs two
/// filesystems, which a test environment can't assume, but the *other*
/// half of the fix — refusing everything that is not a device boundary —
/// is exactly what these exercise.
#[cfg(all(test, unix))]
mod move_path_tests {
    use super::*;

    /// `a -> a/b`: the copy fallback would enumerate the destination it
    /// just created and nest copies inside itself (`a/b/b/b/...`) until
    /// the filesystem gave up. The move must be refused before anything
    /// touches the disk.
    #[test]
    fn moving_a_directory_into_its_own_descendant_is_rejected() -> anyhow::Result<()> {
        let root = scratch_dir("move", "into_descendant");
        let _ = std::fs::remove_dir_all(&root);
        let a = root.join("a");
        std::fs::create_dir_all(a.join("existing"))?;
        std::fs::write(a.join("existing").join("file.txt"), b"x")?;

        for dest in [a.join("b"), a.join("b").join("c")] {
            let result = move_path(&a, &dest);
            assert!(
                result.is_err(),
                "moving {a:?} to {:?} must be refused",
                dest
            );
            let error = result.expect_err("checked above");
            assert!(
                !error.to_string().contains("already exists"),
                "the refusal is about nesting, not an existing destination"
            );
        }

        // Nothing was created or destroyed.
        assert!(
            a.join("existing").join("file.txt").exists(),
            "the source must be untouched"
        );
        assert!(!a.join("b").exists(), "no destination may be created");

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// `a -> a` is caught by the existing-destination check: the source
    /// exists, so the destination already exists.
    #[test]
    fn moving_a_directory_onto_itself_is_rejected() -> anyhow::Result<()> {
        let root = scratch_dir("move", "onto_itself");
        let _ = std::fs::remove_dir_all(&root);
        let a = root.join("a");
        std::fs::create_dir_all(&a)?;
        std::fs::write(a.join("file.txt"), b"x")?;

        assert!(move_path(&a, &a).is_err(), "a -> a must be refused");
        assert!(a.join("file.txt").exists(), "the source must be untouched");

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// A rename failure that has nothing to do with a device boundary
    /// must be returned, not turned into a copy. (The destination's
    /// parent does not exist, so `rename` fails with ENOENT.) The copy
    /// fallback used to run on *any* rename error, which would mask this
    /// as a copy failure while copying a directory that should never
    /// have been copied.
    #[test]
    fn a_non_cross_device_rename_failure_is_returned_not_copied() -> anyhow::Result<()> {
        let root = scratch_dir("move", "rename_error");
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source");
        std::fs::create_dir_all(&source)?;
        std::fs::write(source.join("file.txt"), b"x")?;

        let dest = root.join("no/such/parent").join("target");
        let error = move_path(&source, &dest).expect_err("rename must fail");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "the original rename error must be returned as-is, got {error}"
        );
        assert!(!dest.exists(), "no copy may exist");
        assert!(
            source.join("file.txt").exists(),
            "the source must be intact"
        );

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// An existing destination is refused before anything else, whether
    /// or not the source would nest inside it.
    #[test]
    fn moving_onto_an_existing_destination_is_refused() -> anyhow::Result<()> {
        let root = scratch_dir("move", "existing_dest");
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source");
        std::fs::create_dir_all(&source)?;
        std::fs::write(source.join("file.txt"), b"x")?;
        std::fs::create_dir_all(root.join("taken"))?;

        let dest = root.join("taken");
        let error = move_path(&source, &dest).expect_err("the destination exists");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(
            source.join("file.txt").exists(),
            "the source must be intact"
        );

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
}

/// Cross-platform, unlike the symlink tests above: nothing here needs
/// privileges to create.
#[cfg(test)]
mod copy_tests {
    use super::*;
    use std::fs;

    /// A copied tree keeps its shape and its contents.
    ///
    /// The walk was rewritten from one call per directory level to a
    /// worklist, because the depth being walked is whatever the user
    /// asked to move rather than anything this code chooses. Nesting is
    /// the part a worklist gets wrong when it gets it wrong: a flat
    /// result, or children copied to the wrong parent.
    #[test]
    fn a_copied_tree_keeps_its_nesting_and_contents() -> anyhow::Result<()> {
        let root = scratch_dir("copy", "nesting");
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
