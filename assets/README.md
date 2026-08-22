# Assets

Three kinds of thing live here, and only one of them is meant to be
edited by hand.

## `brand/`

Generated art: the mark at five PNG sizes, the Windows `icon.ico`, the
README banner, and the wordmark. Do not edit these — they are regenerated
from the single mark definition in [`src/brand.rs`](../src/brand.rs):

```sh
cargo run --example brand_assets
```

The icons are the same `rustdirstat::brand::rgba` call the window and
taskbar icons are built from, so the README art and the taskbar icon are
one drawing by construction. See [`examples/README.md`](../examples/README.md).

## `screenshots/`

`gui.png` and `tui.png`, the two screenshots embedded at the top of the
root [`README.md`](../README.md).

## `themes.toml`

The theme catalog. Unlike `brand/`, this one *is* authored — but with a
strict shape. Each theme states twelve colors and everything else is
derived from them in `Palette::from_spec`, so a theme cannot ship a
selection color that clashes with its own accent. The file is compiled
into the binary with `include_str!`, and any `*.toml` dropped into
`<config dir>/rustdirstat/themes/` is loaded on top of it at startup, so
a theme can be added without rebuilding.

Adding a theme is editing data, and it is checked like code:
`theme_layers_are_distinct_and_copy_is_readable` requires adjacent surface
layers to be distinguishable and the two text weights plus the accent to
clear WCAG AA contrast against the panel they sit on. A theme that fails
is a failing build. Two things to know before touching an entry:

- `id` is persisted in the config file — never rename one, or you silently
  drop everyone using it back to the default.
- The first entry is the default theme.
