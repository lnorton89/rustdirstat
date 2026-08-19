//! Persisted user preferences (sort order, treemap visibility/split,
//! detail level, physical-size toggle) — loaded once at startup and saved
//! on a normal quit, so the app reopens the way it was left, the way a GUI
//! app's window state would. Every field is optional: a missing, partial,
//! or unreadable config file just means "use the built-in defaults" for
//! whatever wasn't there, not a hard failure — this is convenience state,
//! not anything that could hide data loss the way silently-dropped scan
//! errors could.

use crate::tui::SortMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub sort: Option<SortMode>,
    #[serde(default)]
    pub show_treemap: Option<bool>,
    #[serde(default)]
    pub treemap_split: Option<u16>,
    #[serde(default)]
    pub detailed: Option<bool>,
    #[serde(default)]
    pub use_physical: Option<bool>,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rustdirstat").join("config.toml"))
}

pub fn load() -> Config {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(s) = toml::to_string_pretty(cfg) {
        let _ = std::fs::write(path, s);
    }
}
