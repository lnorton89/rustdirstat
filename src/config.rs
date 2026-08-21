// ============================================================================
// Module:       config
// Description:  Persisted user preferences, loaded once at startup and saved
//               on a normal quit.
//
// Dependencies: serde + toml (on-disk format), dirs (config location);
//               crate::model::SortMode
// ============================================================================

//! Persisted user preferences (sort order, treemap visibility/split,
//! detail level, physical-size toggle) — loaded once at startup and saved
//! on a normal quit, so the app reopens the way it was left, the way a GUI
//! app's window state would. Every field is optional: a missing, partial,
//! or unreadable config file just means "use the built-in defaults" for
//! whatever wasn't there, not a hard failure — this is convenience state,
//! not anything that could hide data loss the way silently-dropped scan
//! errors could.

use crate::model::SortMode;
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
    #[serde(default)]
    pub gui_orientation: Option<String>,
    #[serde(default)]
    pub gui_show_extensions: Option<bool>,
    #[serde(default)]
    pub gui_show_toolbar: Option<bool>,
    #[serde(default)]
    pub gui_show_status_bar: Option<bool>,
    #[serde(default)]
    pub gui_show_free_space: Option<bool>,
    #[serde(default)]
    pub gui_show_grid: Option<bool>,
    #[serde(default)]
    pub gui_show_labels: Option<bool>,
    /// A theme `id` from `assets/themes.toml`, or from a user theme file.
    /// An id that no longer exists falls back to the default rather than
    /// failing to load — themes come and go, preferences should not
    /// become unreadable when one does.
    #[serde(default)]
    pub gui_theme: Option<String>,
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
    let Ok(s) = toml::to_string_pretty(cfg) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    // Written to a temp file and renamed over the real one, so a crash
    // or a kill mid-write cannot leave the config half-written — the
    // rename is atomic on the filesystems this targets, and either the
    // old config or the new one survives, never a torn mixture. (The
    // old spelling wrote the final file directly, so an interrupted
    // save could silently produce a config that parses as empty on the
    // next launch.)
    // A pid suffix keeps a lingering tmp from a crashed previous run
    // from being overwritten under a running app — harmless either way,
    // but uniqueness is one less interleaving to reason about.
    let tmp = parent.join(format!(".config.toml.{}.tmp", std::process::id()));
    if std::fs::write(&tmp, &s).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, &path);
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn gui_preferences_round_trip_through_toml() -> anyhow::Result<()> {
        let config = Config {
            gui_orientation: Some("vertical".to_string()),
            gui_show_extensions: Some(false),
            gui_show_toolbar: Some(true),
            gui_show_status_bar: Some(false),
            gui_show_free_space: Some(true),
            gui_show_grid: Some(false),
            gui_show_labels: Some(true),
            gui_theme: Some("catppuccin-mocha".to_string()),
            ..Config::default()
        };
        let encoded = toml::to_string(&config)?;
        let decoded: Config = toml::from_str(&encoded)?;
        assert_eq!(decoded.gui_orientation.as_deref(), Some("vertical"));
        assert_eq!(decoded.gui_show_extensions, Some(false));
        assert_eq!(decoded.gui_show_grid, Some(false));
        assert_eq!(decoded.gui_show_labels, Some(true));
        assert_eq!(decoded.gui_theme.as_deref(), Some("catppuccin-mocha"));
        Ok(())
    }
}
