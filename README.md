# rustdirstat

A cross-platform, terminal-based clone of [WinDirStat](https://windirstat.net/), written in Rust.

It scans a directory tree and gives you the same "why is my disk full" workflow
WinDirStat is known for: a sortable size-ranked file list, a squarified,
recursively-nested treemap for spotting the few things eating all your space
at a glance, and a breakdown by file type — all in a fast terminal UI, styled
like an application rather than a bare terminal listing, and fully driven by
either the keyboard or the mouse. It runs anywhere Rust does: Linux, macOS,
and Windows.

## Install / build

Requires a recent stable Rust toolchain (install via [rustup](https://rustup.rs)).

```sh
cargo build --release
./target/release/rustdirstat /path/to/scan
```

## Usage

```sh
rustdirstat [PATH]                # launch the interactive TUI (defaults to '.')
rustdirstat --no-tui [PATH]       # print a plain-text report instead
rustdirstat --no-tui -t 30 -d 3   # report: top 30 entries per dir, 3 levels deep
```

| Flag | Description |
|---|---|
| `-n`, `--no-tui` | Print a text report instead of opening the TUI |
| `-t`, `--top <N>` | Entries shown per directory in report mode (default 20) |
| `-d`, `--depth <N>` | Depth of the report tree (default 2) |

## TUI keybindings

| Key | Action |
|---|---|
| `↑`/`k`, `↓`/`j` | Move selection |
| `→`/`l`/`Enter` | Open the selected directory |
| `←`/`h`/`Backspace` | Go up a directory |
| `s` | Cycle sort order (size, name, modified — each ascending/descending) |
| `m` | Show/hide file counts and modified dates in the list |
| `t` | Toggle the treemap panel |
| `f` | Toggle the "biggest files in this subtree" flat view |
| `/` | Search/filter the current view by name |
| `1`-`9` | Highlight a file-type category in the treemap |
| `0` | Clear the highlight |
| `o` | Open the selected item in the OS file manager |
| `r` | Rescan from the root (keeps your current location) |
| `e` | Export a text report of the current view to a file |
| `d` | Delete the selected item — moves it to the Recycle Bin/Trash |
| `D` | Delete **permanently**, bypassing the Recycle Bin/Trash |
| `?` | Toggle the in-app help screen |
| `q`, `Esc` | Quit |

## Mouse

Every action above also works with the mouse — nothing is keyboard-only
(except `D`, permanent delete, which is deliberately kept off any clickable
surface):

- **Click a treemap tile** to jump straight to it: the file list navigates
  to (and selects) whatever you clicked, exactly like WinDirStat's linked
  list/treemap selection, however deep the tile is nested.
- **Click a list row** to select it (or a "biggest files" row to jump to
  it); click it again quickly to open it (double-click), or scroll the
  wheel to move the selection.
- **Click the extension legend** to highlight that category in the treemap.
- **Click the "Files" title bar** to cycle sort order, the **treemap title
  bar** to toggle the panel, and the **header** to go up a directory.
- **Click the footer buttons** (Open, Up, Delete, Quit, and "more
  shortcuts" for the rest), or the **Yes/No buttons** in the delete
  confirmation popup.

## Design

The default view is deliberately spare — one accent color for navigation,
plain text for file names, a handful of footer buttons — because a screen
that colors and labels everything ends up highlighting nothing. Anything
extra is opt-in rather than always-on:

- File/dir counts and modified dates are hidden until you press `m`.
- The treemap favors a few large, legible tiles over exhaustively nesting
  every subdirectory into illegible fragments.
- The footer shows only the handful of actions used constantly; the rest
  are one keystroke away and listed in `?`.
- Color is reserved for where it's informative — the treemap, the
  extension legend, and highlighting — not sprayed across every row.

## Performance

Built to stay responsive on very large trees (tested against directory
trees in the hundreds-of-thousands-of-files range; designed for
multi-million-file, multi-hundred-GB drives):

- **No per-node paths.** Nodes store only their own name, not a full
  absolute path — a `PathBuf` on every node would duplicate its entire
  ancestor chain, dominating both scan time and memory on a huge tree.
  Paths are reconstructed on demand (open, delete, display) by walking down
  from the root, which is cheap and only needed for a handful of operations,
  not for every node.
- **O(1) extension stats.** Each directory's file-type breakdown is rolled
  up bottom-up once during scanning, so opening even a directory with
  millions of descendants doesn't re-walk its subtree just to show what's
  in it — it's a direct array read.
- **Precomputed categories.** A file's type bucket is computed once at scan
  time (as a small `Copy` enum, not a `String`), not re-parsed from its
  extension on every frame it's rendered.
- **Sequential scanning for small directories.** Most real directories are
  small; spinning up parallel tasks for a handful of entries costs more in
  scheduling overhead than it saves, so only directories above a size
  threshold get parallelized with `rayon`.
- **Batched progress updates.** Scan progress counters are updated once per
  directory, not once per file, to avoid dozens of threads contending the
  same atomic on every single entry.
- **Event-driven redraws.** The UI only redraws in response to actual input
  (or mouse scroll), not on a fixed timer — nothing in the interface
  animates, so polling and recomputing the treemap layout on a clock would
  just waste CPU.
- **Bounded "biggest files" search.** Finding the largest files across a
  huge subtree uses a streaming bounded min-heap, not collect-then-sort, so
  it costs O(k) memory instead of O(n).

## How it works

- **Scanning** walks the directory tree with a `rayon`-parallelized recursive
  descent, aggregating size, file/dir counts, and extension totals bottom-up.
  It never follows symlinks (avoiding cycles), and directories it can't read
  are flagged rather than aborting the whole scan.
- **Treemap** recursively lays out each directory's *entire* subtree (not
  just its immediate children) using the squarified treemap algorithm
  (Bruls, Huizing, van Wijk), nesting a directory's own layout inside the
  rectangle it was allotted by its parent — so a folder that dominates its
  parent still shows real internal structure instead of one flat block.
  Nesting depth and per-level item counts are capped so it stays fast even
  on huge trees.
- **Deletes** move to the OS Recycle Bin/Trash by default (`trash` crate,
  cross-platform); permanent deletion is a separate, deliberately
  keyboard-only action. Either way, the in-memory tree's aggregate sizes,
  counts, and extension totals are updated directly, so there's no need to
  rescan after cleaning something up.
- **Refresh (`r`)** rescans from the root and restores your browsing
  location by path, rather than discarding your place in the tree.
- **Mouse and keyboard input share one dispatch path**: every action is a
  variant of an `Action` enum, and both a key press and a click on a
  registered screen region just produce an `Action` and hand it to the same
  handler — so there's no divergent mouse-only or keyboard-only behavior.

## License

MIT
