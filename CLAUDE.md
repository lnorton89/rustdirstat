# rustdirstat — working notes for agents

A WinDirStat clone in Rust with two front ends over one scanning core: a
ratatui TUI (`rustdirstat`) and an egui/eframe desktop GUI
(`rustdirstat-gui`).

Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the module map and
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) before touching anything on a
per-frame path. Those two are where the non-obvious constraints live; this
file is the short version.

For the human-facing contribution guide — how to build and test, the rules
CI enforces, and what a pull request needs — see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

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

```bash
cargo run --example changelog          # regenerate CHANGELOG.md
cargo run --example changelog -- --check   # what CI runs
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

## Commit only what you changed

**This repository is often worked on by more than one agent at a time.**
Assume the working tree holds someone else's half-finished work, and
check `git status` before staging anything: files you never opened will
be modified, and files you *did* open will carry their edits alongside
yours.

Note that `git commit` commits the whole index, not just the paths you
passed to `git add`. Use `git commit -- path/to/file` so a staged change
you did not make cannot ride along — that is how a half-finished module
move once reached `main` without the `mod` declarations that went with
it, leaving `HEAD` unbuildable.

**Line endings are `.gitattributes`' problem, not yours.** Every text
file is stored and checked out as LF. Do not convert endings by hand, do
not "restore CRLF" after editing, and do not treat a whole-file rewrite
as needing special care — git normalizes on the way in, so nothing you
do to a file's endings can reach a commit.

So never `git add -A`, never `git add .`, and never `git commit -a`.
Stage the files you actually changed, by name. Where a file you touched
also carries someone else's in-flight edits, stage only your own hunks —
build the blob from `HEAD` plus your change and `git update-index` it
rather than adding the whole file:

```bash
git show HEAD:path/to/file.rs > /tmp/base && patch it, then: git hash-object -w /tmp/base
```

Two things that follow from this and are easy to get wrong:

- **A partial stage leaves the worktree ahead of the commit.** That is
  fine for code, but for a shared prose file — `CLAUDE.md`,
  `docs/ARCHITECTURE.md` — write your paragraph into the worktree copy
  *as well*, or the next agent to commit that file will silently revert
  it.
- **`cargo fmt --all` reformats the whole tree**, including the other
  agent's in-progress files. That is harmless, but it is not yours to
  commit; it is another reason to stage by name.

Verify before you push: the committed combination is `HEAD` plus your
hunks, which is *not* what you just tested if the worktree had other
work in it. Reason it through, or check out the index somewhere clean.

## Rules this codebase actually enforces

**Every source file opens with the header banner.** A ruled block naming
`Module:`, `Description:`, and `Dependencies:`, then a blank line, then
the module's `//!` docs. `every_source_file_carries_a_module_header` in
`src/header_check.rs` walks `src/` and `tests/` and fails the build for a
file that is missing one — so a new file needs its header in the same
commit that adds it.

`Module:` is checked against the path the file actually sits at
(`src/gui/ui/theme.rs` → `gui::ui::theme`, `src/gui/mod.rs` → `gui`,
`src/lib.rs` → `rustdirstat (library crate root)`), which is what catches
a header copied off a neighbour and never re-read. The rest is checked
structurally: the fields have to be present and filled in, and an
unedited `[Brief description...]` placeholder is rejected. Nothing judges
whether the prose is any good — that is what review is for.

The banner is plain `//`, not `//!`, and is deliberately *not* rustdoc.
It is the orientation a person gets on opening the file cold; the `//!`
block below it is the documentation, and that is where the reasoning
belongs. Do not restate one in the other, and keep `Dependencies:` to
what a reader actually needs to know is in play — the crates and modules
the file leans on, not a transcription of every `use`.

**`CHANGELOG.md` is generated — never hand-edit it.** `examples/changelog.rs`
rebuilds it from `git tag` plus conventional-commit subjects, and CI runs
`--check` against the *released* sections, so an edit to one is a build
failure. `Unreleased` is exempt from the check by design: it changes with
every merge, and a squash rewrites the hashes it pins. Prose for a release
belongs in the GitHub release body, not here. `CONTRIBUTING.md` has the
release ordering — the changelog section is written *before* the tag
(`cargo run --example changelog -- --release vX.Y.Z`), so the tag names a
tree whose changelog is already finished; v0.2.1's tag permanently
carries a stale changelog because it was cut the other way around.
Commits touching only `CHANGELOG.md` are excluded from every section (the
changelog commit cannot list itself), and release dates are UTC with
`--check` date-insensitive, so a tag landing across midnight cannot fail
its own check.

**Never amend or rebase a commit after tagging it.** The tag survives
pointing at the discarded copy, on no branch at all, and every release
range (`<previous>..<tag>`) computed from it is then wrong. `v0.2.0` was in
that state until it was re-pointed at `a2fa793`. The changelog `--check`
now fails on any version tag no branch can reach. Repairing one means a
force-pushed tag, which re-triggers `release.yml` and its
`gh release upload --clobber` — cancel that run unless you actually want a
published release's assets rebuilt.

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

**The window is built to hold 120 FPS — an 8.33 ms frame — *while a scan
is running*.** That is the number the rest of these rules serve, and it is
`theme::FRAME_BUDGET` rather than folklore: five checks in
`gui/ui/tests.rs` measure against it (a still window rebuilding nothing, a
frame over a quarter-million nodes drawing only what fits, the median
frame idle and under a live scan, and a busy app asking for the *next*
frame rather than one on a 33 ms timer). `docs/PERFORMANCE.md` §0 says
what each one actually guarantees.

**A scan fills the tree in as it goes, and that is why the caches have a
generation.** `scanner::scan_streaming` publishes each finished top-level
child; `GuiApp::attach_child` pushes it into the tree *in place* (O(1),
never a copy — a partial tree of nine million nodes cannot be cloned per
update). In-place growth keeps the `Arc`'s address, which is what both
caches key off, so `RowKey` and `TreemapKey` carry `tree_generation` and
it is bumped on every mutation. `Arc::get_mut` is what makes the mutation
safe: a frame where a worker still holds a clone defers its children to
the next one rather than blocking. See `docs/PERFORMANCE.md` §0c.

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

**The brand mark is the exception to the palette rule, and the only
one.** `src/brand.rs` holds the mark's geometry and its five colours as
literals, because a logo that restyles itself under a dark theme is not
a logo. `gui::icons::paint_brand` paints it and `app_icon` rasterises
it, both from that one table, and `assets/brand/` is the same call at
larger sizes — regenerate those with `cargo run --example brand_assets`
rather than editing the PNGs. Nothing else in drawing code gets to hold
a literal on this argument.

**Constants live with what they govern, and there is no `constants.rs`.**
Three tiers, in order of preference — reach for the narrowest that works:

- **Function-local `const`**, when one function uses it. Most of the GUI's
  geometry is this: `nav_row`'s `ICON` / `GAP` / `HEIGHT` are declared
  inside `nav_row`, where the code that has to agree with them is.
- **Module-private `const`** at the top of the file, when several
  functions in one module share it — `treemap_layout`'s `MIN_TILE_AREA_PX`,
  `scanner`'s `PAR_THRESHOLD`, `duplicates`' `MAX_CANDIDATES`.
- **`pub` / `pub(super)` in the module that owns the concept**, when two
  modules genuinely need it. `theme` owns the spacing scale, `color` owns
  `Category::ALL`, `wintools` owns `TOOLS`, `model::fixtures` owns
  `DEEP_CHAIN_DEPTH`.

A shared `constants.rs` is the thing to avoid, and the reason is not
style. It is a file every change touches, it files unrelated numbers next
to each other by the accident of both being numbers, and it separates a
value from the reasoning that justifies it — which here is usually a
paragraph, not a line. The same rule that governs code governs constants:
neither front end may reach into the other, so anything both need lives
at the crate root, not in `gui/` or `tui/`.

Duplication across modules is the signal to promote one. `DEEP_CHAIN_DEPTH`
was `const DEEP: usize = 60_000` in both `search.rs` and `top_files.rs`,
with the same three-line comment above each — two places to edit when the
answer to "how deep is deep enough" changes.

**Never shadow a name the theme exports.** Every module in `gui::ui`
glob-imports `theme`, and a glob is the weakest binding there is: a local
`const PAD` beats it silently, with no warning, so the identifier means
10.0 inside one function and 12.0 three lines away. `nav_row` had exactly
that. `no_local_const_shadows_the_theme_scale` in `gui/ui/tests.rs` reads
the names back out of `theme.rs` and fails the build for a collision, so
the check extends itself as the scale grows.

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

This is now literally true, not aspirational. It used to be neither: the
modal carried about forty hand-picked values — `add_space(6.0)`,
`add_space(28.0)`, `Margin::symmetric(7.0, 2.0)`, `BODY_PAD = 20.0`,
even a `SPACE_XS + 1.0` — from before the scale existed. Seventeen already
matched a step exactly and were swapped for the name; the remaining
twenty-nine were snapped to the nearest step, ties rounding up. `BODY_PAD`
is now `SPACE_LG` rather than a number of its own.

So there is no literal spacing left in `src/gui`: the only bare number in
a `Margin` is a `0`, which is an absent margin rather than a step. If
you find yourself wanting a value between two steps, the answer is one of
the two steps — that is what makes the toolbar, the status bar, every
pane heading, and now every modal page share one column.

Two mechanics from egui 0.31 sit under that. A `Margin` is measured in
whole pixels (`i8`), so the scale crosses into it through `theme::px`
rather than through an `as i8` at each call site. And **a `Frame`'s
stroke counts as part of its padding**: a 1px border on a pane frame
insets its content to 13px and silently breaks the shared column, which
is why `panel_frame` and the three chrome frames carry no stroke and the
line between panes is the `Panel`'s own separator instead.

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

**There is one modal, and one window that is deliberately not modal.**
`app.modal: Option<ModalPage>` selects a page of a single card
(`src/gui/ui/modal.rs`, contents in `pages.rs`); confirmations layer above
it off `pending_delete` / `pending_windows_tool`. `handle_shortcuts`
returns early while a modal is open, so a new shortcut is automatically
blocked during a confirmation.

The exception is **Properties** (`src/gui/ui/properties.rs`), which is an
`egui::Window` on purpose: it describes the current selection, and the
way people use it is to leave it open while clicking through the tree,
which a modal forbids. It lives in `app.properties`, separate from
`app.modal`, so `modal_is_open` stays false and every shortcut keeps
working while it is up — `shortcuts_still_work_while_the_inspector_is_open`
and `the_properties_window_leaves_the_app_usable` pin both halves.

That is the whole list. Anything else that wants to be a window is what
the six unaligned, unscrollable dialogs the modal replaced all were, and
the answer is a modal page. The bar for a second exception is that the
surface describes something the user is still in the middle of changing —
Properties is the only one here that does.

**A right-to-left region inside a top-aligned horizontal claims the
parent's whole remaining height.** In a scroll area that is the rest of
the page, so one list row grows to fill the pane and strands its own
button at the bottom. Use `allocate_ui_with_layout` with an explicit
width and a zero desired height for each column instead.
`maintenance_rows_stay_row_sized_and_do_not_overlap` guards it.

**A `Frame` does not wrap.** It measures itself against the space left
on the current line and *then* allocates what it measured, so inside a
`horizontal_wrapped` a frame that does not fit overflows the row rather
than moving to the next line. Nine category chips did exactly that and
pinned the extensions pane at nearly the window's width. Measure first
and place with `allocate_exact_size` — an exact allocation is what a
wrapped layout wraps on — then paint into the rect it returns, the way
`sortable_header`, `view_tab` and `category_chip` all do.

**A side panel stores its content's width as its own.** So content that
overflows ratchets the panel wider and the divider will not come back:
the panel reopens at the stored width on the next frame, and a drag
springs back. Anything in a resizable pane must fit the width it is
given. `the_extensions_pane_takes_the_width_it_is_given` drags a real
divider against a realistic pane and guards it — the version of that
test using three extensions and two categories passed throughout,
because with that little in the pane nothing overflows.

**Table columns start at stated widths, never `Column::auto()`.** An
`auto` column makes the table's first frame a *sizing pass*, which egui
lays out with an unbounded width; the content comes back as wide as the
window and, in a side panel, ratchets it open on frame one.

**Exactly one column absorbs the pane's slack, and it is a spacer.**
Only a non-resizable `remainder()` re-fits to the pane each frame, and a
`remainder()` cannot also be resizable — dragging one gives it a stored
width, after which it absorbs nothing and the table stops filling the
pane. So the absorbing column is the one column that cannot be dragged,
which is why it is a trailing spacer with nothing in it rather than a
real column. Making it a real column stretched that column to hundreds
of pixels of empty cell *and* meant the table could never exceed its
pane, so the horizontal scrollbar had nothing to do at any width anyone
uses.

**A header that allocates its whole cell pins that column's minimum.**
`egui_extras` records the widest thing a column ever allocated and will
not shrink a `remainder()` below it, so a full-width heading is a floor
the column can never come back under — widen the window, narrow it, and
the table keeps the old width with a scrollbar over space it gave back.
`sortable_header` takes `claims_width` for this. Paint and sense across
the whole cell; allocate only what the text needs.

**No column ever disappears.** The file list used to drop all but Name,
Size and % of total below 760px. A column that vanishes cannot be
scrolled to, resized, or even known to exist — it reads as the app
having lost them. Narrow panes scroll instead;
`no_column_disappears_when_the_pane_narrows` walks the pane down to
120px.

**Menu and toolbar layout is column-based, not space-padded.** Do not
build a menu label like `"     Open     Ctrl+O"`. The UI font is
proportional, so padding with spaces aligns nothing. Use `menu_action`,
`menu_toggle`, `menu_choice` in `src/gui/ui/widgets.rs`, which lay out
icon / label / shortcut as real columns. `menu_rows_align_and_keep_shortcuts_off_their_labels`
in `src/gui/ui/tests.rs` guards this.

**The menu bar's styling goes through `MenuBar::style`, and its frame
carries no vertical margin and no stroke.** egui applies a menu bar's
style modifier *after* entering the bar's own scope, so anything set on
the surrounding `Ui` is discarded — `menu_bar_style` in
`gui/ui/chrome.rs` is handed to `MenuBar::style` instead, and calls
egui's own `menu_style` first because a modifier replaces the default
rather than adding to it. It squares the widget rounding off: the app
rounds widgets by 6, which under a top-level menu name paints a pill
floating in the strip rather than a bar responding. The frame claims no
vertical margin because the highlight *is* the button's background, so
any margin shows as a gap it cannot reach — and no stroke, because since
egui 0.31 a frame's stroke width is padding too and put that gap back.
The rule under the bar is the `Panel`'s own separator line, which is
painted outside the content. `menu_bar_highlights_are_square_and_fill_the_bar`
measures all of it.

**A menu closes on `ui.close()`, and only there.** egui 0.32 made
`CloseOnClick` the default, which would dismiss the menu under a size
choice or a view toggle — rows that are deliberately flippable several at
a time. `menu_config` in `chrome.rs` asks for `CloseOnClickOutside`
instead, so the rows that *should* dismiss the menu say so themselves.

**A shortcut shown in a menu must exist in `handle_shortcuts`**
(`src/gui/ui/actions.rs`). The menus advertise `Ctrl+O`, `F5`, `Ctrl+C`,
`Ctrl+F`, `Del`, `Shift+Del`, `+`, `-`, and `Home`; all are implemented.

**Nothing tree-sized may recurse, and that includes `Drop`.** A path
that walks the tree once per level puts the tree's depth on the call
stack, and depth is user-supplied. `Node`'s drop, the CSV export,
`search.rs`, `top_files.rs`, and `Tree::path_for`/`node_for` are all
iterative or bounded for this reason; `report.rs` takes an explicit
`max_depth`. The two searches were the stragglers — both recursed once
per directory level until `a_tree_far_deeper_than_the_stack_is_still_searched`
in each of them started failing the build. If you add a walk,
make it iterative — and if it is a whole-subtree pre-order walk that
reports index paths, build it on `model::walk_preorder` rather than
copying the frame discipline a fourth time; `duplicates.rs` and
`csv_export.rs` keep their own walks only because they need a live
`PathBuf` for hashing/io. And note that `Node`'s `Drop` is written so
that the outermost drop drains the whole tree, leaving every node below
it childless and costing one allocation for the tree rather than one per
node. `scanner.rs` is the deliberate exception: it recurses through
rayon, bounded by what a real path can express.

**One extension, one colour.** `color::extension_hue` decides it, for
both front ends, and normalises its input first — the GUI holds `.mkv`
and the TUI holds `mkv`, which are unrelated strings to a hash. Only
saturation and value are per-front-end. The reserved hue band around the
directory tan is load-bearing: without it an ordinary `.wav` tile is
hard to tell from a folder tile beside it.

**A pathname and a filesystem object are different things.** `Node`
holds `file_id` — `(st_dev, st_ino)` on Unix, volume serial plus
file index on Windows — captured at scan time. Duplicates use it to tell two hard links to one
file from two real copies: a same-content group whose names all share
one inode is not reclaimable space, and `DupGroup::reclaimable` counts
`distinct_inodes - 1`, not `files.len() - 1`. The scanner stays on the
root's filesystem by default (`--cross-filesystems` opts out; Unix only —
Windows reports no device identity to the scanner); bytes on other
devices compared against the root volume's free space would be a
category error, so a mount point is kept as a childless zero-byte marker
(`Node.other_filesystem`) rather than descended into — and rather than
dropped, which made it look like the scanner had lost it. And scan-time
identity is why a rescan restores the
GUI's zoom/selection/expansion by *name* components rather than by the
`Vec<usize>` indices, which only mean anything against the tree they
were taken from. Tree lookups are exact by default — `node_for` /
`path_for` return `Option`, `None` on a stale path — and the forgiving
`deepest_valid_node` / `deepest_valid_path` / `valid_prefix` exist for
display and navigation only. A destructive operation that resolved
forgivingly would act on a *different* directory than the one the user
pointed at.

**Neither front end may reach into the other.** Anything both need
lives at the crate root: `search`, `top_files`, `color`, `stats`,
`util`, and `SortMode`/`sort_nodes` on `model`. The GUI used to import
`crate::tui::{search, top_files, SortMode}` and `config.rs` — core —
imported `SortMode` from the TUI, which is how the terminal's copy of
the sort quietly drifted into ignoring `use_physical`.

**Every TUI pane and popup frame comes from `tui/ui/widgets.rs`**, the
counterpart of `gui/ui/widgets.rs`: `panel_block` / `popup_block` /
`danger_block` for frames, `open_popup` for the shadow-clear-frame
sequence every popup opens with, `pane_list` for a titled scrolling list
with its click zones, `text_prompt`, `progress_splash`, `size_bar`. Do
not hand-build a `Block` — four list renderers each carried their own
copy of the same forty-line tail, so the clamp that keeps a stale
`selected` from desyncing the scroll offset had to be right in four
places.

**State lives in the struct that owns the view.** `App` has
`SearchState` / `DuplicatesState` / `MoveState` / `WinToolsState`;
`GuiApp` has `SearchState` / `ToolsState` / `ViewOptions`. A new field
goes in its group, not at the top level — that is what forty-odd flat
fields prefixed `duplicate_` and `search_` turned into.

**A destructive confirmation only answers to the keys it offers.** The
delete and Windows-tool prompts advertise `[Y]es`, `[E]mpty`, `[N]o`;
anything else — an arrow key, F5, a stray modifier — leaves the dialog
standing. Treating every unrecognised key as a cancel meant the next
keystroke, aimed at the dialog, went to the file list instead.

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
- **The egui stack is on 0.36, and it got there in 0.3.0.** It was held
  at 0.29 for two releases because moving it is a project rather than a
  bump; the migration is done and the notes below are what it changed, so
  a reader of old code (or an old branch) can tell which world they are
  in. `eframe::App` split `update` into `logic(ctx, frame)` and
  `ui(ui, frame)`, so the draw path threads a `Ui`; `SidePanel` and
  `TopBottomPanel` became one `Panel::left/right/top/bottom(id)` taking
  that `Ui`; the menu system was rebuilt around
  `MenuBar`/`MenuButton`/`MenuConfig`, with `close_menu` replaced by
  `ui.close()`. Mechanically: `Rounding` → `CornerRadius` (`u8`),
  `Margin` and `Shadow` in whole pixels (`i8`/`u8`), `Frame::none()` →
  `Frame::NONE`, an explicit `StrokeKind` on every painted rect stroke,
  `Context::style` → `global_style`, `screen_rect` → `viewport_rect`,
  `wants_keyboard_input` → `egui_wants_keyboard_input`, and `Context::run`
  → `run_ui` in tests. Two behaviours changed under the API and are worth
  knowing: a `Frame`'s stroke width now counts as padding, and menus
  close on click by default.
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
