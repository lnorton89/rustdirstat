# rustdirstat — working notes for agents

A WinDirStat clone in Rust with two front ends over one scanning core: a
ratatui TUI (`rustdirstat`) and an egui/eframe desktop GUI
(`rustdirstat-gui`).

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the module map and
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) before touching anything on a
per-frame path. Those two are where the non-obvious constraints live; this
file is the short version.

## Commands

```bash
cargo test
```

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

```bash
cargo fmt --all -- --check
```

```bash
cargo run --bin rustdirstat-gui -- C:/some/path
```

All three are clean on `main` and are enforced by CI
(`.github/workflows/ci.yml`) on Linux, macOS, and Windows. Keep them that
way — a warning is a build failure there.

The integration test in `tests/quit_stress.rs` is `#![cfg(unix)]` and
compiles to nothing on Windows, so `cargo test` there runs the unit
tests only. **A lot of code is platform-gated** — `platform.rs` has a
whole `cfg(unix)` module, `util.rs` has a unix-only test module — so a
clean run on one OS proves less than it looks. On Windows, WSL is the
fast way to check the Linux side before pushing:

```bash
wsl -e bash -lc "cd /mnt/c/path/to/repo && CARGO_TARGET_DIR=\$HOME/rds-target cargo clippy --all-targets"
```

Use a separate `CARGO_TARGET_DIR` so the two toolchains do not fight
over `target/`. A full debug build of this crate is ~5.6 GB; if disk is
tight, `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` cuts that
to ~1.2 GB without changing what is checked.

There is also a Nix flake — `nix flake check` and `nix develop`, with
[`NIX.md`](NIX.md) covering installation. If WSL has Nix, `nix flake
check` there is another way to exercise the Linux build.

## Rules this codebase actually enforces

**No `unwrap`, `expect`, or `panic!` in library code.** Denied by lint, so
a violation is a build failure, not a warning. Use `let ... else`, `?`, or
an explicit fallback. Test code is exempt via a `cfg_attr` in
`src/lib.rs` — do not widen that exemption to reach shipping code.

**Never recompute something tree-sized inside a draw call.** The GUI is
immediate mode: `gui::ui::draw` runs in full every frame. A scan of a
whole drive is ~9M nodes, so anything O(tree) in a draw path freezes the
window. Derived data is cached on `GuiApp` (`refresh_visible_rows`,
`refresh_treemap`) and the caches are keyed off observed state rather than
invalidated by hand. If you add a field that affects rows or tiles, add it
to `RowKey` / `TreemapKey` in `src/gui/app.rs`.

**Never free a scanned tree on the UI thread.** Use
`drop_in_background` in `src/gui/app.rs`. Details in
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).

**Treemap traversal is level-order, and must stay that way.** Depth-first
tile budgeting leaves the right-hand side of a large volume's treemap
blank. See the module doc in `src/gui/treemap_layout.rs` and the
regression test `a_tree_too_big_for_the_budget_still_covers_the_whole_panel`.

**Menu and toolbar layout is column-based, not space-padded.** Do not
build a menu label like `"     Open     Ctrl+O"`. The UI font is
proportional, so padding with spaces aligns nothing. Use `menu_action`,
`menu_toggle`, `menu_choice` in `src/gui/ui/widgets.rs`, which lay out
icon / label / shortcut as real columns. `menu_rows_align_and_keep_shortcuts_off_their_labels`
in `src/gui/ui/tests.rs` guards this.

**A shortcut shown in a menu must exist in `handle_shortcuts`**
(`src/gui/ui/actions.rs`). The menus advertise `Ctrl+O`, `F5`, `Ctrl+C`,
`Ctrl+F`, `Del`, `Shift+Del`, `+`, `-`, and `Home`; all are implemented.

## Testing an immediate-mode UI

There is no retained widget tree to query, so the drawing code records the
rects it actually painted into statics in `src/gui/ui/probes.rs`, and
`src/gui/ui/tests.rs` drives a real `egui::Context` and clicks those
recorded coordinates. When you add a control that a test should be able to
reach, record its rect the same way rather than hard-coding a position —
otherwise a layout change can move a control out from under its own click
target and no test will notice.

Tests that render share `TEST_UI_LOCK` because the probe statics are
global. Keep taking that lock in new rendering tests.

## Things that are deliberate, not oversights

- **Nodes do not store their own path.** `Tree::path_for` rebuilds it from
  child indices. A `PathBuf` per node dominates memory on a large scan.
  Selections are therefore `Vec<usize>` index paths, not paths.
- **`crossterm` uses the `use-dev-tty` feature.** Not about `/dev/tty` —
  it avoids a real event-loss bug in the default mio backend. The long
  comment in `Cargo.toml` explains it; `tests/quit_stress.rs` is the
  regression test.
- **eframe runs on wgpu, not glow.** The glow/glutin path rejects some
  valid Linux EGL/GLX setups.
- **The TUI and GUI have separate treemap recursion policies**
  (`tui::nested` vs `gui::treemap_layout`) over one shared squarify
  implementation (`treemap::layout`). A terminal cell and a pixel differ
  by orders of magnitude in area, so they need different floors. Do not
  "deduplicate" these into one.
