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
        .map(|s| parse(&s))
        .unwrap_or_default()
}

/// Parses a config, forgiving per field: one malformed value costs that
/// one preference, not all of them.
///
/// Deserializing the whole struct in one shot meant a single bad field
/// — a mistyped sort name, a string where a bool belongs — silently
/// threw away every other saved preference along with it. A file that
/// is not TOML at all still yields the defaults; there is nothing in it
/// to salvage.
fn parse(text: &str) -> Config {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Config::default();
    };
    fn field<T: serde::de::DeserializeOwned>(table: &toml::Table, key: &str) -> Option<T> {
        table
            .get(key)
            .cloned()
            .and_then(|value| value.try_into().ok())
    }
    Config {
        sort: field(&table, "sort"),
        show_treemap: field(&table, "show_treemap"),
        treemap_split: field(&table, "treemap_split"),
        detailed: field(&table, "detailed"),
        use_physical: field(&table, "use_physical"),
        gui_orientation: field(&table, "gui_orientation"),
        gui_show_extensions: field(&table, "gui_show_extensions"),
        gui_show_toolbar: field(&table, "gui_show_toolbar"),
        gui_show_status_bar: field(&table, "gui_show_status_bar"),
        gui_show_free_space: field(&table, "gui_show_free_space"),
        gui_show_grid: field(&table, "gui_show_grid"),
        gui_show_labels: field(&table, "gui_show_labels"),
        gui_theme: field(&table, "gui_theme"),
    }
}

/// Writes `cfg` to the platform config location, atomically.
///
/// Failures are returned rather than swallowed: both front ends save on
/// exit, and "your preferences were not saved" is the caller's to
/// surface — the old `()` return made saying so impossible, so every
/// failed save looked identical to a successful one.
pub fn save(cfg: &Config) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Err(std::io::Error::other(
            "no configuration directory could be determined",
        ));
    };
    let s = toml::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::other(format!(
            "{} has no parent directory",
            path.display()
        )));
    };
    std::fs::create_dir_all(parent)?;
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
    std::fs::write(&tmp, &s)?;
    if let Err(error) = std::fs::rename(&tmp, &path) {
        // The temp file is orphaned at this point — best effort not to
        // litter the config directory with one per failed save.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse, Config};

    /// One malformed field costs that field, not the whole file: the
    /// old whole-struct deserialize threw away every preference when a
    /// single value failed to parse.
    #[test]
    fn one_malformed_field_does_not_discard_the_rest() {
        let config = parse(concat!(
            "sort = \"NotARealSortMode\"\n",
            "use_physical = true\n",
            "gui_theme = \"catppuccin-mocha\"\n",
            "treemap_split = \"not a number\"\n",
        ));
        assert!(config.sort.is_none(), "the bad sort value is dropped alone");
        assert_eq!(config.treemap_split, None, "the bad split is dropped alone");
        assert_eq!(
            config.use_physical,
            Some(true),
            "a good field beside a bad one survives"
        );
        assert_eq!(config.gui_theme.as_deref(), Some("catppuccin-mocha"));
    }

    /// A file that is not TOML at all still yields defaults rather than
    /// an error — preferences are convenience state, never a failure.
    #[test]
    fn garbage_yields_defaults() {
        let config = parse("this is { not toml");
        assert!(config.sort.is_none(), "no sort can come out of garbage");
        assert_eq!(config.gui_theme, None);
    }

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
