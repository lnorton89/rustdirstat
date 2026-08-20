//! The theme catalog: loading `assets/themes.toml`, and the rules for
//! deriving a full [`Palette`] from the handful of colors a theme states.
//!
//! The colors themselves are data, not code — see `assets/themes.toml`,
//! which is compiled in with `include_str!` and can be supplemented at
//! runtime by `*.toml` files under `<config dir>/rustdirstat/themes/`.
//! TOML rather than YAML or JSON because `toml` and `serde` are already
//! dependencies of this crate for the config file, so the format costs
//! nothing to support and stays consistent with what a user of this app
//! already edits by hand.
//!
//! A [`ThemeSpec`] is what a theme actually chooses: four surface
//! layers, a border, an accent, two text weights, and three status
//! colors. Everything else the window needs — muted accent fills,
//! selection, the danger callout background — is *derived* in
//! [`Palette::from_spec`] rather than listed per theme, so a theme
//! cannot ship a selection color that clashes with its own accent, and
//! adding a derived color later means editing one function instead of
//! every entry in the file.
//!
//! The surface layers are named for depth, not for use: `app` sits
//! behind everything, `panel` sits on it, `raised` on that, and `hover`
//! is the interactive lift above `raised`. In a dark theme that ramp
//! gets lighter as it comes forward; in a light theme `hover` goes the
//! other way, because a pale surface reads as "lifted" by getting
//! darker. `theme_layers_are_distinct_and_copy_is_readable` in
//! `tests.rs` enforces both polarities across every theme in the file.

use eframe::egui::Color32;
use serde::Deserialize;
use std::sync::OnceLock;

use super::theme::{blend, contrast_ratio, readable_text_color};

/// The bundled catalog. Parsed once, lazily, into [`THEMES`].
const BUNDLED: &str = include_str!("../../../assets/themes.toml");

/// Whether a theme's surfaces are dark or light. Several derived colors
/// and a handful of layout choices read this rather than re-deriving it
/// from luminance at each call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub(crate) fn is_dark(self) -> bool {
        self == ThemeMode::Dark
    }
}

/// One theme exactly as it appears in the file.
#[derive(Deserialize)]
struct RawTheme {
    id: String,
    name: String,
    mode: ThemeMode,
    app: String,
    panel: String,
    raised: String,
    hover: String,
    border: String,
    accent: String,
    primary_text: String,
    secondary_text: String,
    danger: String,
    warning: String,
    success: String,
}

#[derive(Deserialize)]
struct RawCatalog {
    #[serde(default)]
    theme: Vec<RawTheme>,
}

/// The colors one theme defines for itself. See the module docs for why
/// this is deliberately smaller than [`Palette`].
pub(crate) struct ThemeSpec {
    /// Stable identifier, persisted in the config file.
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) mode: ThemeMode,
    app: Color32,
    panel: Color32,
    raised: Color32,
    hover: Color32,
    border: Color32,
    accent: Color32,
    primary_text: Color32,
    secondary_text: Color32,
    danger: Color32,
    warning: Color32,
    success: Color32,
}

/// `#rgb`, `#rrggbb`, or either without the leading `#`.
pub(crate) fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    let byte = |at: usize| -> Option<u8> { u8::from_str_radix(hex.get(at..at + 2)?, 16).ok() };
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        3 => {
            // `#abc` means `#aabbcc`; each nibble doubles.
            let nibble = |at: usize| -> Option<u8> {
                let value = u8::from_str_radix(hex.get(at..at + 1)?, 16).ok()?;
                Some(value * 17)
            };
            Some(Color32::from_rgb(nibble(0)?, nibble(1)?, nibble(2)?))
        }
        _ => None,
    }
}

impl RawTheme {
    /// `None` if any color fails to parse. A theme with one unreadable
    /// color would be worse than a missing theme: it would render, in
    /// whatever the fallback for that one slot happened to be.
    fn into_spec(self) -> Option<ThemeSpec> {
        Some(ThemeSpec {
            id: self.id,
            name: self.name,
            mode: self.mode,
            app: parse_hex(&self.app)?,
            panel: parse_hex(&self.panel)?,
            raised: parse_hex(&self.raised)?,
            hover: parse_hex(&self.hover)?,
            border: parse_hex(&self.border)?,
            accent: parse_hex(&self.accent)?,
            primary_text: parse_hex(&self.primary_text)?,
            secondary_text: parse_hex(&self.secondary_text)?,
            danger: parse_hex(&self.danger)?,
            warning: parse_hex(&self.warning)?,
            success: parse_hex(&self.success)?,
        })
    }
}

/// The full set of colors the drawing code paints with.
///
/// `Copy` on purpose: it is read hundreds of times per frame through
/// [`super::theme::palette`], and a clone-per-read of twenty `Color32`
/// values would be silly. Twenty `u32`s is smaller than plenty of the
/// structs egui already passes by value.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) mode: ThemeMode,
    pub(crate) app: Color32,
    pub(crate) panel: Color32,
    pub(crate) raised: Color32,
    pub(crate) hover: Color32,
    /// Between `panel` and `raised`; egui's `faint_bg_color`, used for
    /// zebra striping.
    pub(crate) faint: Color32,
    pub(crate) border: Color32,
    /// A stronger border, for controls that need to read as interactive
    /// before the pointer arrives.
    pub(crate) border_strong: Color32,
    pub(crate) accent: Color32,
    /// The interior of a filled control that has to be visible on *any*
    /// surface: a scrollbar handle, a checkbox box, a slider rail.
    ///
    /// Distinct from `raised` on purpose. A button is a surface and can
    /// share the card's color; a scrollbar handle sitting on a card in
    /// the card's own color is simply invisible, which is exactly what
    /// happened when both came from `raised`.
    pub(crate) control: Color32,
    pub(crate) control_hover: Color32,
    /// The accent knocked back into a fill: what a selected tab, an
    /// active toggle, or a primary button is painted with.
    pub(crate) accent_muted: Color32,
    /// Text and icons drawn on top of `accent_muted`.
    pub(crate) on_accent: Color32,
    pub(crate) primary_text: Color32,
    pub(crate) secondary_text: Color32,
    pub(crate) selection_bg: Color32,
    pub(crate) selection_stroke: Color32,
    pub(crate) danger: Color32,
    /// Fill behind a destructive warning callout.
    pub(crate) danger_bg: Color32,
    /// Text on `danger_bg`. Not `danger` itself, which is tuned as a
    /// marker against a neutral surface, not as readable copy on its own
    /// tint.
    pub(crate) danger_text: Color32,
    pub(crate) warning: Color32,
    pub(crate) warning_bg: Color32,
    pub(crate) warning_text: Color32,
    pub(crate) success: Color32,
    /// Hairline between treemap tiles.
    pub(crate) treemap_grid: Color32,
    /// What a treemap tile or category swatch is blended toward when it
    /// falls outside the active highlight.
    pub(crate) dim: Color32,
    /// Outline around the selected treemap tile. It has to beat every
    /// tile color that could be under it, so it is the extreme end of
    /// the theme's polarity rather than any palette color.
    pub(crate) treemap_selection: Color32,
    /// Outline around the treemap tile under the pointer.
    ///
    /// The same polarity as `treemap_selection` and deliberately weaker,
    /// so hovering the selected tile cannot be mistaken for selecting a
    /// different one. It is an alpha rather than a blend because the tile
    /// underneath is a different color for every file type there is, and
    /// a fixed opaque grey reads as a marker over half of them and as
    /// nothing at all over the other half.
    pub(crate) treemap_hover: Color32,
}

/// Lightens or darkens `color` against `background` until it clears
/// `target` contrast, or until there is nowhere further to go.
///
/// Callout text is derived, not authored, so no theme author ever looks
/// at it — which makes a fixed blend factor the wrong tool. Monokai's
/// magenta is the case that proved it: the same 30% lift that clears AA
/// comfortably for a red left the pink at 4.34:1. Searching for the
/// factor instead means a new theme cannot fail this way at all.
fn ensure_contrast(color: Color32, background: Color32, target: f32, toward: Color32) -> Color32 {
    let mut best = color;
    // Twenty steps of 5% each. Fine enough that the result is never
    // visibly further from the theme's own color than it has to be.
    for step in 0..=20 {
        best = blend(color, toward, step as f32 * 0.05);
        if contrast_ratio(best, background) >= target {
            break;
        }
    }
    best
}

impl Palette {
    pub(crate) fn from_spec(spec: &ThemeSpec) -> Self {
        let dark = spec.mode.is_dark();
        let (near, far) = if dark {
            (Color32::WHITE, Color32::BLACK)
        } else {
            (Color32::BLACK, Color32::WHITE)
        };
        // A muted accent has to stay a *surface* — something text sits on
        // — so it is mixed into the panel rather than dimmed down from
        // the accent. Light themes take less of it: the same 30% that
        // reads as restrained on a dark panel reads as a saturated block
        // on a white one.
        let accent_muted = blend(spec.panel, spec.accent, if dark { 0.30 } else { 0.18 });
        let danger_bg = blend(spec.panel, spec.danger, if dark { 0.22 } else { 0.13 });
        let warning_bg = blend(spec.panel, spec.warning, if dark { 0.20 } else { 0.13 });
        Self {
            mode: spec.mode,
            app: spec.app,
            panel: spec.panel,
            raised: spec.raised,
            hover: spec.hover,
            faint: blend(spec.panel, spec.raised, 0.5),
            border: spec.border,
            border_strong: blend(spec.border, near, 0.28),
            accent: spec.accent,
            // Anchored to the border rather than to a surface: the border
            // is already the theme's answer to "what is visible against
            // my own backgrounds", so a step past it is visible against
            // all of them.
            control: blend(spec.border, near, 0.12),
            control_hover: blend(spec.border, near, 0.34),
            accent_muted,
            on_accent: readable_text_color(accent_muted),
            primary_text: spec.primary_text,
            secondary_text: spec.secondary_text,
            selection_bg: blend(spec.panel, spec.accent, if dark { 0.55 } else { 0.34 }),
            selection_stroke: blend(spec.accent, near, 0.35),
            danger: spec.danger,
            danger_bg,
            danger_text: ensure_contrast(spec.danger, danger_bg, 4.6, near),
            warning: spec.warning,
            warning_bg,
            warning_text: ensure_contrast(spec.warning, warning_bg, 4.6, near),
            success: spec.success,
            treemap_grid: blend(spec.app, far, 0.45),
            dim: blend(spec.panel, spec.raised, 0.7),
            treemap_selection: if dark { Color32::WHITE } else { Color32::BLACK },
            treemap_hover: if dark {
                Color32::from_white_alpha(170)
            } else {
                Color32::from_black_alpha(150)
            },
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::from_spec(default_spec())
    }
}

/// Last-resort theme, used only if `assets/themes.toml` fails to parse.
///
/// It exists so that a broken catalog degrades to a plain dark window
/// rather than to a panic — the crate denies `unwrap`, and "the app will
/// not start" is a poor answer to "someone mistyped a hex color".
/// `bundled_catalog_parses_and_every_theme_survives` makes reaching this
/// a failing build rather than something a user discovers.
fn fallback_spec() -> &'static ThemeSpec {
    static FALLBACK: OnceLock<ThemeSpec> = OnceLock::new();
    FALLBACK.get_or_init(|| ThemeSpec {
        id: "dark-modern".to_string(),
        name: "Dark Modern".to_string(),
        mode: ThemeMode::Dark,
        app: Color32::from_rgb(0x18, 0x18, 0x18),
        panel: Color32::from_rgb(0x1f, 0x1f, 0x1f),
        raised: Color32::from_rgb(0x2a, 0x2d, 0x2e),
        hover: Color32::from_rgb(0x37, 0x39, 0x3b),
        border: Color32::from_rgb(0x3c, 0x3c, 0x3c),
        accent: Color32::from_rgb(0x3c, 0x99, 0xe6),
        primary_text: Color32::from_rgb(0xe0, 0xe0, 0xe0),
        secondary_text: Color32::from_rgb(0x9d, 0x9d, 0x9d),
        danger: Color32::from_rgb(0xf1, 0x4c, 0x4c),
        warning: Color32::from_rgb(0xcc, 0xa7, 0x00),
        success: Color32::from_rgb(0x89, 0xd1, 0x85),
    })
}

/// Every theme available, bundled ones first, in file order.
pub(crate) fn themes() -> &'static [ThemeSpec] {
    static THEMES: OnceLock<Vec<ThemeSpec>> = OnceLock::new();
    let loaded = THEMES.get_or_init(|| {
        let mut specs = parse_catalog(BUNDLED);
        specs.extend(user_themes());
        specs
    });
    if loaded.is_empty() {
        std::slice::from_ref(fallback_spec())
    } else {
        loaded
    }
}

pub(crate) fn parse_catalog(text: &str) -> Vec<ThemeSpec> {
    let Ok(catalog) = toml::from_str::<RawCatalog>(text) else {
        return Vec::new();
    };
    catalog
        .theme
        .into_iter()
        .filter_map(RawTheme::into_spec)
        .collect()
}

/// Themes dropped into `<config dir>/rustdirstat/themes/`.
///
/// Every failure here — no config directory, no such folder, an
/// unreadable or malformed file — means "no extra themes", never an
/// error. This is decoration a user opted into, and the app has a
/// perfectly good catalog without it.
fn user_themes() -> Vec<ThemeSpec> {
    let Some(dir) = dirs::config_dir().map(|d| d.join("rustdirstat").join("themes")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    // Directory order is not defined, and the theme list is a menu — it
    // should not reshuffle itself between runs.
    paths.sort();
    paths
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .flat_map(|text| parse_catalog(&text))
        .collect()
}

/// The theme used when the config names nothing, or names something no
/// longer in the catalog.
pub(crate) fn default_spec() -> &'static ThemeSpec {
    themes().first().unwrap_or_else(|| fallback_spec())
}

pub(crate) fn default_theme_id() -> &'static str {
    &default_spec().id
}

pub(crate) fn spec_by_id(id: &str) -> Option<&'static ThemeSpec> {
    themes().iter().find(|spec| spec.id == id)
}

pub(crate) fn palette_for(id: &str) -> Palette {
    Palette::from_spec(spec_by_id(id).unwrap_or_else(default_spec))
}
