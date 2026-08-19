/// Format a byte count the way WinDirStat does: base-1024 units with one
/// decimal place beyond bytes.
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

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    Ok(())
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
    let meta = std::fs::symlink_metadata(source)?;
    if meta.is_dir() {
        copy_dir_recursive(source, dest)?;
        std::fs::remove_dir_all(source)?;
    } else {
        std::fs::copy(source, dest)?;
        std::fs::remove_file(source)?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
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
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
