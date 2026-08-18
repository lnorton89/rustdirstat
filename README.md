# rustdirstat

A cross-platform, terminal-based clone of [WinDirStat](https://windirstat.net/), written in Rust.

It scans a directory tree and gives you the same "why is my disk full" workflow
WinDirStat is known for: a sortable size-ranked file list, a squarified,
recursively-nested treemap for spotting the few things eating all your space
at a glance, and a breakdown by file type — all in a fast terminal UI that's
fully driven by either the keyboard or the mouse. It runs anywhere Rust does:
Linux, macOS, and Windows.

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
| `←`/`h`/`Backspace` | Go up to the parent directory |
| `s` | Cycle sort order (size ↓, size ↑, name ↓, name ↑) |
| `t` | Toggle the treemap panel |
| `o` | Open the selected item in the OS file manager |
| `d` | Delete the selected file/directory (asks for confirmation) |
| `1`-`9` | Highlight a file-type category in the treemap (dims everything else) |
| `0` | Clear the highlight |
| `q`, `Esc` | Quit |

## Mouse

Every action above also works with the mouse — nothing is keyboard-only:

- **Click a treemap tile** to jump straight to it: the file list navigates
  to (and selects) whatever you clicked, exactly like WinDirStat's linked
  list/treemap selection, however deep the tile is nested.
- **Click a list row** to select it; click it again quickly to open it
  (double-click), or scroll the wheel to move the selection.
- **Click the extension legend** to highlight that category in the treemap.
- **Click the "files" title bar** to cycle sort order, the **treemap title
  bar** to toggle the panel, and the **header** to go up a directory.
- **Click the footer buttons**, or the **Yes/No buttons** in the delete
  confirmation popup.

## How it works

- **Scanning** walks the directory tree with a `rayon`-parallelized recursive
  descent, aggregating size and file counts bottom-up. It never follows
  symlinks (avoiding cycles), and directories it can't read are flagged
  rather than aborting the whole scan.
- **Treemap** recursively lays out each directory's *entire* subtree (not
  just its immediate children) using the squarified treemap algorithm
  (Bruls, Huizing, van Wijk), nesting a directory's own layout inside the
  rectangle it was allotted by its parent — so a folder that dominates its
  parent still shows real internal structure instead of one flat block.
  Nesting depth and per-level item counts are capped so it stays fast even
  on huge trees.
- **Deletes** update the in-memory tree's aggregate sizes directly, so there's
  no need to rescan after cleaning something up.
- **Mouse and keyboard input share one dispatch path**: every action is a
  variant of an `Action` enum, and both a key press and a click on a
  registered screen region just produce an `Action` and hand it to the same
  handler — so there's no divergent mouse-only or keyboard-only behavior.

## License

MIT
