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
            Category::Documents => Color::Rgb(52, 152, 219),
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
        "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "msi" => Category::Programs,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "h" | "hpp" | "java" | "go"
        | "rb" | "php" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "sh" => {
            Category::Source
        }
        "" => Category::NoExtension,
        _ => Category::Other,
    }
}

/// Directories aren't a `Category` (they carry no extension statistics of
/// their own), but they still need a consistent color in the list/treemap.
pub fn directory_color() -> Color {
    Color::Rgb(97, 148, 224)
}
