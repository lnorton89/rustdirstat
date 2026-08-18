use std::path::PathBuf;

/// A single file or directory in the scanned tree.
///
/// For directories, `size` and `file_count` are aggregates of every
/// descendant, kept up to date as entries are removed via `App::confirm_delete`.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub file_count: u64,
    pub children: Vec<Node>,
    /// Set when the directory could not be read (e.g. permission denied).
    pub error: bool,
}

impl Node {
    pub fn extension(&self) -> &str {
        if self.is_dir {
            return "";
        }
        self.path.extension().and_then(|e| e.to_str()).unwrap_or("")
    }
}
