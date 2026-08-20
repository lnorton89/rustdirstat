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

**No `unwrap`, `expect`, or `panic!` anywhere — including tests.** Denied
by lint, so a violation is a build failure, not a warning. In library
code use `let ... else`, `?`, or an explicit fallback. In tests, return
`anyhow::Result<()>` and use `?` for anything fallible, and `assert!` /
`assert_eq!` with a message for the actual assertions. There is no
crate-wide exemption and there should not be one: a blanket
`cfg_attr(test, allow(..))` in `lib.rs` covers *all* `#[cfg(test)]`
items, including library code that merely happens to be test-gated.

**Every `unsafe` block gets a safe leaf wrapper.** The pattern, used in
`platform.rs`, `gui/shell_icons.rs`, and `tests/quit_stress.rs`:

- One named function per FFI call, taking safe Rust arguments and
  returning `Option`/`Result` of a safe type.
- The `unsafe` block contains *the call and nothing else*. Arithmetic,
  error handling, and string marshalling go outside it — if it can be
  safe code, it is.
- A `// SAFETY:` comment on every block, stating the argument-validity
  reasoning it actually depends on.
- Anything with a matching destroy/free/close call gets an owning
  wrapper with a `Drop` impl, so early returns cannot leak it. See
  `OwnedIcon` and `OwnedBitmap`.
- Prefer `MaybeUninit` over `mem::zeroed` for out-parameters, and only
  `assume_init` on the path where the callee reported success.

The result should be that no caller of one of these needs `unsafe`
itself, and no test body contains any.

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

**No literal `Color32` in drawing code.** Every color comes from
`palette()` (`src/gui/ui/theme.rs`), which returns the active theme's
`Palette`. The catalog is data — `assets/themes.toml` — and a theme
states twelve colors while `Palette::from_spec` derives the rest, so
adding a *derived* color means editing one function rather than every
entry in the file. `theme_layers_are_distinct_and_copy_is_readable`
checks all of them for layer separation and WCAG contrast; a theme that
fails is a failing build. The only literals left are lighting effects
that are not theme colors at all — the treemap cushion highlight, and
alpha-only scrims.

**No hand-picked pixel gaps.** Every margin, inset, and `add_space` in
the GUI is one of `SPACE_XS` / `SPACE_SM` / `SPACE_MD` / `SPACE_LG` in
`src/gui/ui/theme.rs`, and `PAD` — the inset from a panel edge to its
content — is one of them too. Four values, so the left edges of the
toolbar, the status bar, and every pane's heading form one column. Two
traps to know about: `ui.add_space` advances the cursor *without*
becoming an item, so the widget after one is not given the row's item
spacing (`a_file_row_lines_its_icon_up_with_the_folders_beside_it`); and
`ui.separator()` allocates padding of its own on top of the row spacing,
which is why panes rule off their headings with `section_rule` rather
than by hand (`every_pane_rules_off_its_heading_at_the_same_inset`).

**Every hover fades; nothing switches.** Hoverable surfaces route their
highlight through `hover_t` / `hover_fill` in `src/gui/ui/widgets.rs`,
which is one `cubic_out` ramp over `HOVER_SECONDS` for the whole window —
one control that snaps beside one that fades is most of what reads as
unfinished. A control whose background is painted before its own response
exists (an `egui::Frame`, so the category chips) uses
`remembered_hover` / `remember_hover` instead. `a_hover_highlight_ramps_rather_than_switching`
pins the curve. Note that the *first* observation of an egui animation
returns its target, which is what stops a control that appears already
selected from sliding into place — and means a test has to show egui the
resting state for a frame before it can measure a ramp.

**`bg_fill` and `weak_bg_fill` are not interchangeable.** egui paints
buttons from `weak_bg_fill` and filled controls — scrollbar handle,
checkbox interior, slider rail — from `bg_fill`. A button is a surface
and may share the card's color; a scrollbar handle in the card's own
color is invisible. `apply_style` therefore points `weak_bg_fill` at the
surfaces and `bg_fill` at `Palette::control`. Setting both to `raised` is
what made every scrollbar in the app disappear.
`a_scrollbar_handle_is_never_the_color_of_what_it_scrolls` checks the
separation for every theme, against every surface a bar can land on.

**All five `ScrollArea`s take their look from `scroll_style()`** in
`src/gui/ui/theme.rs`, by way of `Style::spacing` — never configure one at
its call site. Bars are solid rather than floating on purpose: a floating
bar is invisible until the pointer is already on it, so a table with
columns past its edge looks like it has simply lost them.

**There is one modal, not several.** `app.modal: Option<ModalPage>`
selects a page of a single card (`src/gui/ui/modal.rs`, contents in
`pages.rs`); confirmations layer above it off `pending_delete` /
`pending_windows_tool`. Do not add an `egui::Window` — that is what the
six unaligned, unscrollable, non-modal dialogs this replaced all were.
`handle_shortcuts` returns early while a modal is open, so a new
shortcut is automatically blocked during a confirmation.

**A right-to-left region inside a top-aligned horizontal claims the
parent's whole remaining height.** In a scroll area that is the rest of
the page, so one list row grows to fill the pane and strands its own
button at the bottom. Use `allocate_ui_with_layout` with an explicit
width and a zero desired height for each column instead.
`maintenance_rows_stay_row_sized_and_do_not_overlap` guards it.

**Menu and toolbar layout is column-based, not space-padded.** Do not
build a menu label like `"     Open     Ctrl+O"`. The UI font is
proportional, so padding with spaces aligns nothing. Use `menu_action`,
`menu_toggle`, `menu_choice` in `src/gui/ui/widgets.rs`, which lay out
icon / label / shortcut as real columns. `menu_rows_align_and_keep_shortcuts_off_their_labels`
in `src/gui/ui/tests.rs` guards this.

**The menu bar's own styling has to be set from inside `menu::bar`.**
egui's `set_menu_style` runs on the child `Ui` as that call's first act,
so button padding, item spacing, and widget rounding configured on the
way in are all silently discarded. `draw_menu_bar` sets them on the child
instead, and squares the rounding off — the app rounds widgets by 6,
which under a top-level menu name paints a pill floating in the strip
rather than a bar responding. The panel frame carries no vertical margin
for the same reason: the highlight *is* the button's background, so any
margin shows as a gap it cannot reach.
`menu_bar_highlights_are_square_and_fill_the_bar` measures both.

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
