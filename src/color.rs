use ratatui::style::Color;

/// Bucket a file extension into a WinDirStat-style category used for both
/// the extension breakdown table and the treemap colors.
pub fn category_for_ext(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "tgz" => "Archives",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "tiff" | "ico" | "heic" => {
            "Images"
        }
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" => "Video",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => "Audio",
        "doc" | "docx" | "pdf" | "txt" | "md" | "odt" | "rtf" | "xls" | "xlsx" | "ppt" | "pptx"
        | "csv" => "Documents",
        "exe" | "dll" | "so" | "dylib" | "bin" | "app" | "msi" => "Programs",
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "h" | "hpp" | "java" | "go"
        | "rb" | "php" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "sh" => "Source",
        "" => "No Extension",
        _ => "Other",
    }
}

pub fn category_color(category: &str) -> Color {
    match category {
        "Archives" => Color::Rgb(255, 140, 0),
        "Images" => Color::Rgb(46, 204, 113),
        "Video" => Color::Rgb(155, 89, 182),
        "Audio" => Color::Rgb(241, 196, 15),
        "Documents" => Color::Rgb(52, 152, 219),
        "Programs" => Color::Rgb(231, 76, 60),
        "Source" => Color::Rgb(26, 188, 156),
        "No Extension" => Color::Rgb(149, 165, 166),
        "Directory" => Color::Rgb(93, 133, 193),
        _ => Color::Rgb(127, 140, 141),
    }
}
