# Architecture

Two front ends over one scanning core. Nothing UI-shaped lives below
`src/tui/` or `src/gui/`, and neither front end knows the other exists.
The [README](../README.md) has the user-facing overview; see
[`PERFORMANCE.md`](PERFORMANCE.md) for the scale constraints that shape
this design.

```
                    ┌──────────────┐
   filesystem ─────▶│  scanner.rs  │  parallel walk (rayon)
                    └──────┬───────┘
                           │ Tree { root: Node, volume_free, .. }
                    ┌──────▼───────┐
                    │   model.rs   │  the one in-memory representation
                    └──────┬───────┘
              ┌────────────┴────────────┐
              │                         │
      ┌───────▼────────┐        ┌───────▼────────┐
      │   src/tui/     │        │   src/gui/     │
      │  ratatui TUI   │        │ egui/eframe UI │
      └───────┬────────┘        └───────┬────────┘
              │                         │
              └──────────┬──────────────┘
                         │
              shared: treemap.rs (squarify), color.rs,
              duplicates.rs, csv_export.rs, config.rs, util.rs,
              search.rs, top_files.rs
```

## Core (front-end agnostic)

| File | What it owns |
| --- | --- |
| `src/scanner.rs` | The parallel filesystem walk. Falls back to single-threaded below `PAR_THRESHOLD` entries per directory. Reports live counts through a lock-free `Progress`. Materializes owned `EntryInfo` records (metadata fetched in parallel for wide directories) and drops each `DirEntry` before scanning its children, so a deep tree cannot exhaust Unix `RLIMIT_NOFILE`. Stays on the root's filesystem by default — an entry on another device is kept as a childless zero-byte marker (`Node.other_filesystem`) rather than descended into or silently dropped; crossing for real needs `--cross-filesystems` (Unix only — Windows reports no device identity here). |
| `src/model.rs` | `Node`, `Tree`, and `SortMode`/`sort_nodes` — the order siblings are listed in, which lives here because both front ends and the persisted config need it. Aggregates (`size`, `file_count`, `dir_count`, `ext_totals`) are computed bottom-up at scan time so browsing never re-walks a subtree. `Node`'s `Drop` is iterative: the derived one recurses per level, so freeing a deep tree overflowed the stack. `path_for`/`node_for` are exact (`None` on a stale path) with forgiving `deepest_valid_node`/`deepest_valid_path`/`valid_prefix` for display. `Node.file_id` is the filesystem object identity captured at scan time — `(st_dev, st_ino)` on Unix, `None` on Windows, where it would cost a handle-based syscall per file — and is what makes hard-link aliases distinguishable from real duplicate content where it exists. |
| `src/treemap.rs` | The squarified treemap algorithm (Bruls/Huizing/van Wijk), on an abstract integer grid. Rounds rectangle *edges*, not width/height, so siblings cannot round into a gap or an overlap. |
| `src/color.rs` | Extension → `Category` mapping, the category palette, and `extension_hue` — the one place an extension's colour is decided, so a file is the same colour in the terminal and in the window. It normalises its input, since the GUI holds `.mkv` and the TUI holds `mkv`. |
| `src/duplicates.rs` | Size-bucketed, blake3-hashed duplicate detection, hard-link aware: two names for one inode are never reported as reclaimable duplicates. |
| `src/search.rs`, `src/top_files.rs` | Name search across a subtree (glob, or regex behind `re:`) and the k largest files in one. Both walk iteratively and answer in index paths, so neither front end has an opinion about them. |
| `src/platform.rs`, `src/wintools.rs` | Volume free/total space; Windows maintenance tool shell-outs. |
| `src/gui/shell_icons.rs` | The icon the OS shows for a file type, cached per extension. Windows-only; elsewhere it reports nothing and callers fall back to the drawn set. |
| `src/config.rs` | Persisted preferences. Every field is `Option`; a missing or corrupt file means "use defaults", never an error. |
| `src/report.rs`, `src/csv_export.rs`, `src/stats.rs` | Non-interactive output modes. The CSV export streams and walks iteratively — it has no depth or count limit by design, so a drive-sized scan must not be buffered whole or put the tree's depth on the stack. |

### Talking to the platform

Every `unsafe` call in the crate sits behind a named safe function that
takes safe arguments and returns an `Option` or `Result`, with the
`unsafe` block reduced to the FFI call itself and a `// SAFETY:` note
saying what it relies on. Handles that need releasing get an owning
`Drop` wrapper so an early return cannot leak them. `platform.rs` and
`gui/shell_icons.rs` are the worked examples; CLAUDE.md states the rule.

### Index paths

A `Node` does **not** store its own path — one `PathBuf` per node would
dominate memory on a multi-million-node scan, since each duplicates its
whole ancestor chain. Instead, a node is addressed by a `Vec<usize>` of
child indices from the root, and `Tree::path_for` reconstructs the real
path on demand for the few operations that touch the filesystem.

This is why selection, expansion, and treemap tiles all carry
`Vec<usize>` rather than paths. A rescan can leave an old index path
pointing somewhere else entirely — sibling order is not stable between
scans — so the GUI restores zoom/selection/expansion by *name*: it
captures the lossless `OsString` components before replacing the tree and
resolves them against the new one, landing on the same directory rather
than the same index. Lookups are exact by default — `path_for` returns
`Option<PathBuf>` and `node_for` returns `Option<&Node>`, `None` when the
path runs off the end of the tree — because a destructive operation that
resolved forgivingly would act on a *different* directory than the one
the user pointed at. The forgiving forms (`deepest_valid_node`,
`deepest_valid_path`, `valid_prefix`) exist for display and navigation,
and are named so no one reaches for them in mutation code by accident.

## GUI (`src/gui/`)

```
gui/
  mod.rs             run(), eframe NativeOptions (wgpu renderer)
  app.rs             GuiApp: ALL state, background work, derived-data caches
  icons.rs           vector icon set, painted not fonted
  treemap_layout.rs  Node subtree -> positioned pixel tiles
  ui/
    mod.rs           draw() + panel composition
    themes.rs        theme catalog loading + Palette derivation
    theme.rs         active palette, egui style, spacing scale, motion timings
    widgets.rs       menu rows, toolbar buttons, table headers, view tabs,
                     and the shared hover/motion helpers they all route through
    categories.rs    the file-category bar and chips above the extension table
    chrome.rs        menu bar, toolbar, status bar
    directory.rs     the directory tree view
    extensions.rs    the extension list
    lists.rs         largest files, search results, duplicates
    treemap.rs       tile painting + cushion shading
    modal.rs         the one modal shell: scrim, blur, card, nav rail
    pages.rs         the contents of each modal page + confirmations
    actions.rs       commands + keyboard shortcuts
    probes.rs        (test) recorded geometry
    tests.rs         (test) interaction suite
assets/
  themes.toml        the theme catalog, compiled in with include_str!
```

### The modal layer

There is exactly one modal surface. Everything that used to be a separate
`egui::Window` — settings, properties, maintenance tools, the view guide
— is a page of one card, reached from `app.modal: Option<ModalPage>`, and
pages can link to one another. Delete and destructive-tool confirmations
are a second layer above that, keyed off `pending_delete` and
`pending_windows_tool`, so an "are you sure" can sit over the page that
raised it.

The backdrop behind the card is a real blur: the modal asks for a
`ViewportCommand::Screenshot`, waits for the `Event::Screenshot` reply
(one or two frames), downscales it to ~220px wide, runs three box passes,
and uploads that as a texture. It costs one GPU readback per open and
nothing per frame. A backend that never answers — including
`egui::Context::default`, so every test — falls back to a plain scrim
after two frames.

### Themes

`assets/themes.toml` is the catalog; `*.toml` under
`<config dir>/rustdirstat/themes/` is loaded on top of it. A theme states
twelve colors and `Palette::from_spec` derives the other dozen, so a
theme cannot ship a selection or callout color that clashes with its own
accent. `theme_layers_are_distinct_and_copy_is_readable` checks every
theme in the catalog for layer separation (in CIE L*, not luminance —
luminance collapses near black, where several of these themes live) and
for WCAG AA/AAA contrast on both authored and derived colors.

`GuiApp` owns everything; `ui` is stateless. `draw` runs top to bottom
once per frame and rebuilds the window from scratch. That is the single
most important thing to know before editing it — see
[`PERFORMANCE.md`](PERFORMANCE.md).

Modules under `ui/` share helpers via `pub(super)`, which makes an item
visible throughout the `ui` subtree but no further.

### Background work

Scanning, duplicate hashing, and Windows maintenance tools each run on a
detached thread and report back over an `mpsc` channel that
`GuiApp::poll_background` drains once per frame. `is_busy()` gates the
controls that must not start a second long job. The window opens
immediately on a placeholder tree (`Tree::placeholder`) so a large drive
never looks like a failed launch.

## TUI (`src/tui/`)

`app.rs` holds state and the event loop; `ui/` renders, split the same
way `gui/ui/` is — `mod.rs` lays the panes out, then `chrome.rs`
(header, footer, extension legend), `lists.rs` (directory listing,
largest files, search, duplicates), `treemap.rs`, `popups.rs` (every
prompt, confirmation and help screen) and `text.rs` (width-aware
trimming). `nested.rs`/`theme.rs` carry the pieces the renderer needs, and
`widgets.rs` holds the framed surfaces the panes and popups are built
from — the counterpart of `gui/ui/widgets.rs`.

`nested.rs` is the terminal-cell counterpart of `gui::treemap_layout` —
same shared squarify call underneath, different recursion floors,
because a terminal cell is roughly four orders of magnitude larger than
a pixel.

`App`'s state is grouped by the view that owns it: `SearchState`,
`DuplicatesState`, `MoveState`, `WinToolsState`. `GuiApp` does the same
with `SearchState`, `ToolsState` and `ViewOptions`. Add a field to the
group it belongs to rather than to the top level.

## Where a change usually goes

| Change | File |
| --- | --- |
| A new column in the file table | `gui/ui/directory.rs` + `DirectoryColumn` in `gui/app.rs` |
| A new menu item | `gui/ui/chrome.rs`; the command itself in `gui/ui/actions.rs` |
| A new keyboard shortcut | `handle_shortcuts` in `gui/ui/actions.rs`, and show it on the matching menu row |
| A new theme | `assets/themes.toml` — data only, no code |
| A new *derived* color | `Palette` in `gui/ui/themes.rs`, then the contrast test |
| Spacing, fonts, egui style | `gui/ui/theme.rs` — pick one of the four `SPACE_*` steps |
| A hover or motion effect | `hover_t` / `hover_fill` in `gui/ui/widgets.rs`, never a bespoke ramp |
| A new settings surface | a `ModalPage` in `gui/ui/modal.rs` + its page in `gui/ui/pages.rs` |
| How tiles are chosen or sized | `gui/treemap_layout.rs` |
| The tiling math itself | `treemap.rs` (shared with the TUI — check both) |
| Anything scanned or aggregated | `scanner.rs` + `model.rs` |
| A shared TUI pane, popup frame, or list | `tui/ui/widgets.rs` |
| A new persisted preference | `config.rs`, then both halves of `ViewOptions::from_config`/`to_config` (or `GuiApp::new`/`save_preferences` for anything outside the view toggles) |
