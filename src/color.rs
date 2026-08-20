use ratatui::style::Color;

/// A file-type bucket, used for both the extension breakdown table and
/// treemap/list coloring. A `Copy` enum (not a `String`) so categorizing a
/// file costs nothing at render time — it's computed once per node during
/// scanning and just read back afterward.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Archives,
    Images,
    Video,
    Audio,
    Documents,
    Programs,
    Source,
    NoExtension,
    Other,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Archives,
        Category::Images,
        Category::Video,
        Category::Audio,
        Category::Documents,
        Category::Programs,
        Category::Source,
        Category::NoExtension,
        Category::Other,
    ];

    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Archives => "Archives",
            Category::Images => "Images",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Documents => "Documents",
            Category::Programs => "Programs",
            Category::Source => "Source",
            Category::NoExtension => "No Extension",
            Category::Other => "Other",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Category::Archives => Color::Rgb(255, 140, 0),
            Category::Images => Color::Rgb(46, 204, 113),
            Category::Video => Color::Rgb(155, 89, 182),
            Category::Audio => Color::Rgb(241, 196, 15),
            Category::Documents => Color::Rgb(214, 93, 177),
            Category::Programs => Color::Rgb(231, 76, 60),
            Category::Source => Color::Rgb(26, 188, 156),
            Category::NoExtension => Color::Rgb(149, 165, 166),
            Category::Other => Color::Rgb(127, 140, 141),
        }
    }
}

pub fn category_for_ext(ext: &str) -> Category {
    match ext.to_lowercase().as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" => Category::Archives,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "tiff" | "ico" | "heic" => {
            Category::Images
        }
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => Category::Video,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => Category::Audio,
        "doc" | "docx" | "pdf" | "txt" | "md" | "odt" | "rtf" | "xls" | "xlsx" | "ppt" | "pptx"
        | "csv" => Category::Documents,
        // Compiled/binary build artifacts (static libs, object files, debug
        // symbols, ...) — conceptually the same bucket as a finished
        // executable even though you wouldn't run them directly. Without
        // this, a build output directory (cargo's target/, node_modules,
        // a C/C++ build tree) reads as almost entirely the catch-all
        // "Other" color, since none of these extensions are documents,
        // media, or archives either — they just have nowhere else to go.
        "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "msi" | "rlib" | "rmeta" | "a" | "lib"
        | "o" | "obj" | "pdb" | "ilk" | "exp" | "class" | "jar" | "wasm" => Category::Programs,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "h" | "hpp" | "java" | "go"
        | "rb" | "php" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "sh" | "d" => {
            Category::Source
        }
        "" => Category::NoExtension,
        _ => Category::Other,
    }
}

/// Directories aren't a `Category` (they carry no extension statistics of
/// their own), but they still need a consistent color in the list/treemap.
/// Deliberately a warm, desaturated tan (folder-like) rather than a blue —
/// every file `Category` above sits in the blue/green/cool range except
/// Documents and Programs, so a blue directory color made the treemap read
/// as "everything is blue" whenever directories dominated the view (which
/// they usually do, since most of a tree's area is directories until the
/// treemap recurses down into actual files).
pub fn free_space_color() -> Color {
    Color::Rgb(70, 78, 86)
}

pub fn directory_color() -> Color {
    Color::Rgb(196, 164, 96)
}

/// Per-extension treemap/list color — deterministically derived from a
/// hash of the extension itself rather than the small, fixed set of
/// broad `Category` buckets. Nine categories inevitably collapse a
/// directory dominated by one kind of thing (a cargo `target/` full of
/// `.rlib`/`.rmeta`/`.d`/`.pdb`, all legitimately "Programs" or "Source")
/// down to one or two colors, no matter how the categories are drawn —
/// that's what the buckets mean, not a rendering bug, but it still reads
/// as "broken" next to WinDirStat's own treemap, which colors by
/// individual extension and so stays visually varied even when a
/// directory is semantically homogeneous. Hashing the extension into a
/// hue spread across the full color wheel gets the same effect without
/// maintaining a growing, ever-incomplete list of extension-to-category
/// mappings. `Category` itself is unchanged and still drives the
/// extension-breakdown legend and highlighting — this only changes what
/// color a file's tile actually renders as.
pub fn ext_color(name: &str) -> Color {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext.is_empty() {
        return Category::NoExtension.color();
    }
    hsv_to_rgb(extension_hue(&ext), 0.55, 0.85)
}

/// The hue, in degrees, that an extension is drawn at.
///
/// Both front ends call this, because an extension that is one color in
/// the terminal and another in the window is just wrong. They used to
/// hash independently — 64-bit FNV modulo 320 with the gap below here,
/// 32-bit FNV modulo 360 without it in `gui::ui::theme` — so the same
/// `.mp4` came out a different color in each, and the GUI had the exact
/// clash the gap exists to prevent even though it paints folders with
/// the same [`directory_color`].
///
/// The hash is taken modulo 320 rather than the full 360° and then
/// shifted past a 40°-wide gap centered on the directory tan's own hue
/// (~41°) — without this, an ordinary extension (.wav, .csv, .mp4 all
/// land within ~10° of it) can hash to a color close enough to the
/// directory tan that a file tile and a folder tile next to it at the
/// same depth become hard to tell apart by color alone, defeating the
/// whole reason directories get a dedicated, distinct hue.
pub fn extension_hue(ext: &str) -> f32 {
    // Normalised first, because the two front ends hold an extension in
    // different forms: the GUI keeps `.mkv`, the TUI keeps `mkv`. Those
    // are two unrelated strings to a hash, so sharing this function is
    // not by itself enough to make the colors agree.
    let ext = ext.strip_prefix('.').unwrap_or(ext).to_ascii_lowercase();
    const GAP_START: f32 = 21.0;
    const GAP_WIDTH: f32 = 40.0;
    let raw = (fnv1a(ext.as_bytes()) % 320) as f32;
    if raw < GAP_START {
        raw
    } else {
        raw + GAP_WIDTH
    }
}

/// FNV-1a. There was one of these here, one in `gui::app` and one in
/// `gui::ui::theme`, and two of the three disagreed with this one.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// HSV to RGB, shared with the GUI, which wraps it in its own color type.
pub fn hsv_to_rgb_bytes(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let c = value * saturation;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = value - c;
    let (r, g, b) = match hue {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let (r, g, b) = hsv_to_rgb_bytes(h, s, v);
    Color::Rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hue must not depend on which front end is asking.
    ///
    /// The GUI holds an extension as `.mkv` and the TUI as `mkv`, and
    /// they used to hash whichever form they had with two different FNV
    /// variants — so the same file was one color in the terminal and
    /// another in the window.
    #[test]
    fn an_extension_has_one_hue_whichever_form_it_is_written_in() {
        for ext in ["mkv", "rs", "tar", "csv", "wav", "mp4"] {
            let bare = extension_hue(ext);
            assert_eq!(
                bare,
                extension_hue(&format!(".{ext}")),
                "{ext} and .{ext} are the same extension and must share a hue"
            );
            assert_eq!(
                bare,
                extension_hue(&ext.to_ascii_uppercase()),
                "{ext} is case-insensitive"
            );
        }
    }

    /// Nothing hashes into the band reserved around the directory tan.
    ///
    /// The GUI painted folders with `directory_color` while hashing
    /// extensions across the full wheel, so a `.wav` tile beside a folder
    /// tile could be the same tan.
    #[test]
    fn no_extension_hashes_into_the_directory_colors_hue_band() {
        // `if let`, not `let ... else` with a `panic!`: the crate denies
        // `panic!` in tests too.
        let mut rgb = None;
        if let Color::Rgb(r, g, b) = directory_color() {
            rgb = Some((r, g, b));
        }
        assert!(
            rgb.is_some(),
            "the directory color should be a literal RGB triple"
        );
        let (r, g, b) = rgb.unwrap_or_default();
        let directory_hue = hue_of(r, g, b);
        assert!(
            (21.0..61.0).contains(&directory_hue),
            "the reserved band is meant to straddle the directory hue, which is {directory_hue}"
        );

        // Every two-and three-letter extension, which covers the common
        // ones and is plenty to catch a gap that is not applied at all.
        let letters = b'a'..=b'z';
        for first in letters.clone() {
            for second in letters.clone() {
                for third in letters.clone() {
                    let ext = String::from_utf8_lossy(&[first, second, third]).to_string();
                    let hue = extension_hue(&ext);
                    assert!(
                        !(21.0..61.0).contains(&hue),
                        ".{ext} hashes to hue {hue}, inside the band reserved                          around the directory color"
                    );
                }
            }
        }
    }

    fn hue_of(r: u8, g: u8, b: u8) -> f32 {
        let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if delta == 0.0 {
            return 0.0;
        }
        let hue = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        if hue < 0.0 {
            hue + 360.0
        } else {
            hue
        }
    }
}
