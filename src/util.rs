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
/// the platform's native file manager.
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
