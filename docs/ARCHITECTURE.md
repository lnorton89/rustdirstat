# Architecture

Two front ends over one scanning core. Nothing UI-shaped lives below
`src/tui/` or `src/gui/`, and neither front end knows the other exists.

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
              duplicates.rs, csv_export.rs, config.rs, util.rs
```

## Core (front-end agnostic)

| File | What it owns |
| --- | --- |
| `src/scanner.rs` | The parallel filesystem walk. Falls back to single-threaded below `PAR_THRESHOLD` entries per directory. Reports live counts through a lock-free `Progress`. |
| `src/model.rs` | `Node` and `Tree`. Aggregates (`size`, `file_count`, `dir_count`, `ext_totals`) are computed bottom-up at scan time so browsing never re-walks a subtree. |
| `src/treemap.rs` | The squarified treemap algorithm (Bruls/Huizing/van Wijk), on an abstract integer grid. Rounds rectangle *edges*, not width/height, so siblings cannot round into a gap or an overlap. |
| `src/color.rs` | Extension → `Category` mapping and the category palette, shared so both front ends color the same file the same way. |
| `src/duplicates.rs` | Size-bucketed, blake3-hashed duplicate detection. |
| `src/platform.rs`, `src/wintools.rs` | Volume free/total space; Windows maintenance tool shell-outs. |
| `src/gui/shell_icons.rs` | The icon the OS shows for a file type, cached per extension. Windows-only; elsewhere it reports nothing and callers fall back to the drawn set. |
| `src/config.rs` | Persisted preferences. Every field is `Option`; a missing or corrupt file means "use defaults", never an error. |
| `src/report.rs`, `src/csv_export.rs`, `src/stats.rs` | Non-interactive output modes. |

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
`Vec<usize>` rather than paths, and why `valid_prefix` exists: after a
rescan the old indices may no longer resolve, so they are truncated to the
longest prefix that still does.

## GUI (`src/gui/`)

```
gui/
  mod.rs             run(), eframe NativeOptions (wgpu renderer)
  app.rs             GuiApp: ALL state, background work, derived-data caches
  icons.rs           vector icon set, painted not fonted
  treemap_layout.rs  Node subtree -> positioned pixel tiles
  ui/
    mod.rs           draw() + panel composition
    theme.rs         palette, egui style, color math
    widgets.rs       menu rows, toolbar buttons, table headers, view tabs
    chrome.rs        menu bar, toolbar, status bar
    directory.rs     the directory tree view
    extensions.rs    the extension list
    lists.rs         largest files, search results, duplicates
    treemap.rs       tile painting + cushion shading
    dialogs.rs       modals
    actions.rs       commands + keyboard shortcuts
    probes.rs        (test) recorded geometry
    tests.rs         (test) interaction suite
```

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

`app.rs` holds state and the event loop, `ui.rs` renders, and
`nested.rs`/`top_files.rs`/`search.rs`/`theme.rs` split out the pieces the
renderer needs. `nested.rs` is the terminal-cell counterpart of
`gui::treemap_layout` — same shared squarify call underneath, different
recursion floors, because a terminal cell is roughly four orders of
magnitude larger than a pixel.

## Where a change usually goes

| Change | File |
| --- | --- |
| A new column in the file table | `gui/ui/directory.rs` + `DirectoryColumn` in `gui/app.rs` |
| A new menu item | `gui/ui/chrome.rs`; the command itself in `gui/ui/actions.rs` |
| A new keyboard shortcut | `handle_shortcuts` in `gui/ui/actions.rs`, and show it on the matching menu row |
| Colors, spacing, fonts | `gui/ui/theme.rs` |
| How tiles are chosen or sized | `gui/treemap_layout.rs` |
| The tiling math itself | `treemap.rs` (shared with the TUI — check both) |
| Anything scanned or aggregated | `scanner.rs` + `model.rs` |
| A new persisted preference | `config.rs`, then `GuiApp::new` and `save_preferences` |
