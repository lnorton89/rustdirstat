# rustdirstat

A cross-platform, terminal-based clone of [WinDirStat](https://windirstat.net/), written in Rust.

It scans a directory tree and gives you the same "why is my disk full" workflow
WinDirStat is known for: a sortable size-ranked file list, a squarified
treemap for spotting the few things eating all your space at a glance, and a
breakdown by file type — all in a fast, keyboard-driven terminal UI. It runs
anywhere Rust does: Linux, macOS, and Windows.

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
| `q`, `Esc` | Quit |

## How it works

- **Scanning** walks the directory tree with a `rayon`-parallelized recursive
  descent, aggregating size and file counts bottom-up. It never follows
  symlinks (avoiding cycles), and directories it can't read are flagged
  rather than aborting the whole scan.
- **Treemap** uses the squarified treemap algorithm (Bruls, Huizing, van
  Wijk) to lay out each directory's children as area-proportional rectangles,
  colored by file-type category.
- **Deletes** update the in-memory tree's aggregate sizes directly, so there's
  no need to rescan after cleaning something up.

## License

MIT
