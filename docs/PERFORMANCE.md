# Working with large scans

The design target is a whole system drive. The reference case throughout
this document is a real one: **8,955,037 files in 1,337,406 folders,
839.8 GB**, which lands as roughly 10.3M `Node`s at 128 bytes each plus a
`String` per node and a category-totals `Vec` per directory — comfortably
over 2 GB resident.

Everything below exists because that scale breaks assumptions that hold
fine on a project folder. If you are changing the GUI, this is the file to
read first. For the module map, see
[`ARCHITECTURE.md`](ARCHITECTURE.md); the [README](../README.md) has the
user-facing overview.

## 0. The target: 120 FPS, including while a scan runs

Everything in this document is in service of one number. The window is
built to hold **120 frames per second — an 8.33 ms budget per frame —
with a scan of a whole volume in flight**, not merely to stay responsive.
It is stated as `theme::FRAME_BUDGET` rather than left implied, because a
budget nothing names is a budget nothing checks.

Four things deliver it, and each has a check that fails the build:

| What it guarantees | Check |
|---|---|
| A still window does no tree-sized work at all — both caches hit | `a_still_window_rebuilds_neither_rows_nor_tiles` |
| A frame costs what the window shows, not what the scan found | `a_frame_over_a_huge_tree_draws_only_what_fits` |
| The median frame over a quarter-million-node tree fits the budget | `a_frame_over_a_huge_tree_fits_the_budget` |
| ...and still fits with a real scan running underneath it | `frames_stay_inside_the_budget_while_a_scan_runs` |
| A busy app asks for the *next* frame rather than one on a timer | `a_busy_app_asks_for_the_next_frame_immediately` |

Three things about those checks are deliberate:

- **Median, not mean or max.** One scheduler preemption on a shared CI
  runner must not fail a build, and a real regression moves the middle of
  the distribution rather than one sample.
- **A debug build gets a documented multiple** of the budget
  (`DEBUG_FRAME_BUDGET_FACTOR`), because tests run unoptimized and the
  target describes the release binary. A regression that overshoots by
  more than that factor still fails.
- **The under-load check loops a real scan** rather than scanning once.
  A fixture small enough to build inside a test is walked in a couple of
  frames, which would measure almost nothing; the worker re-walks it
  until the render loop has its samples, so every sample is taken with
  the rayon pool, the atomics, and the allocation churn of a real walk
  underneath it.

The last row is easy to lose by accident. Until 0.3.0 the app answered a
scan in progress with `request_repaint_after(33ms)`, which caps the window
at 30 FPS for the whole of a scan no matter how much headroom the machine
has — no individual frame is slow, and the target is still missed by a
factor of four. A busy app now asks for the next frame immediately and
lets vsync do the pacing.

## 0b. What a Windows directory listing costs, and buys

Since 0.3.0 the walk lists each Windows directory through its own handle
(`platform::directory_listing`) rather than through `read_dir`. The
listing reports, for every entry and in the same call: the name, the
attributes, the logical size, the **allocation size**, the timestamps and
the **file id**. That is what makes two things possible that were not
before — physical sizes that mean what Explorer means by "size on disk",
and hard-link identity captured at scan time rather than recovered later
from the duplicate hasher's open handle.

It is not free, and the number matters more than the argument. Measured
on this repository's `target/` directory — 1,913 directories holding
13,452 files, so roughly seven files per directory, which is close to the
worst case for a per-directory cost — with a warm cache and a release
build:

| Walk | Wall clock |
|---|---|
| `read_dir` + per-entry metadata | ~55 ms |
| One directory handle per directory | ~80 ms |

About **13 µs per directory**, or ~45% on a tree shaped like that one.
The overhead is per *directory*, not per file, so it shrinks against any
directory with more than a handful of entries in it; on a drive-sized
scan of ~1.3M directories it is on the order of fifteen seconds against a
scan measured in minutes.

Two things were tried and did not help: opening with the narrow
`FILE_LIST_DIRECTORY` right rather than `GENERIC_READ`, and caching the
volume serial per volume instead of asking per directory (kept anyway —
it is a syscall per directory not made). Building each name straight from
UTF-16 with `OsStringExt::from_wide` rather than via
`String::from_utf16_lossy` *did*: the double conversion was most of the
cost of the parse.

One thing was removed to pay for it: the walk no longer calls
`symlink_metadata` for each directory to learn its own timestamp, because
the parent's listing already carried it. Only the scan root pays for that
now, on either path.

`RUSTDIRSTAT_STD_LISTING=1` forces the `read_dir` walk — for a filesystem
where the listing path misbehaves, and for reproducing the table above.

## 1. The GUI is immediate mode: every frame rebuilds the window

`gui::ui::draw` runs top to bottom on every frame. There is no retained
widget tree and no diffing. Anything it computes, it computes ~60 times a
second.

So: **nothing on a draw path may be O(tree).** The two things that used to
be are now cached on `GuiApp`:

| Cache | Built by | Key |
| --- | --- | --- |
| `visible_rows` | `refresh_visible_rows` | `RowKey` — tree identity, sort mode, size mode, expanded-set fingerprint |
| `treemap_tiles` | `refresh_treemap` | `TreemapKey` — tree identity, zoom path, panel rect (whole pixels), size mode, free-space toggle |

Both keys are **derived from observed state and compared each frame**,
rather than invalidated by hand at each mutation site. That is deliberate.
The inputs are scattered across the UI — every expand/collapse, every sort
click, the logical/physical toggle, every rescan, every splitter drag —
and hand-written invalidation is one missed call away from painting a
stale tree, which presents as the app ignoring input. A key that is
recomputed from state cannot go stale.

**If you add anything that changes which rows or tiles are produced, add
it to the key.** `cached_rows_refresh_whenever_an_input_changes` and
`cached_treemap_follows_the_panel_rect_and_the_zoom` in
`src/gui/app/mod.rs` cover the existing inputs; extend them.

The borrow pattern this forces, in `directory.rs` and `ui/treemap.rs`,
looks like:

```rust
app.refresh_visible_rows();   // &mut, ends here
{
    let app = &*app;              // shared for the duration of painting
    let rows = &app.visible_rows; // borrowed, not cloned
    // ... paint, recording user intent into locals ...
}
// ... apply the locals to &mut app ...
```

Cloning the row list instead would put the per-frame allocation right
back; on a wide expanded directory that is hundreds of thousands of
`String` and `Vec` allocations per frame.

## 2. Freeing a scanned tree is expensive — never do it on the UI thread

Dropping a `Tree` walks every node and returns millions of individual
allocations. Measured on this machine, release build, everything resident:

| Nodes | Drop time |
| --- | --- |
| 0.7M | 80 ms |
| 11.2M | **1.19 s** |

That is the floor. Once the working set has been paged out — likely after
a multi-gigabyte scan — the allocator has to fault all of it back in just
to release it, and the wall-clock cost goes up sharply. On the UI thread
that reads as the window hanging.

Two places hand a tree off instead, both via `drop_in_background` in
`src/gui/app/scan.rs`:

- **`replace_tree`**, when a rescan produces a new tree. The old one is
  reclaimed on a detached thread so the UI never stalls mid-rescan.
- **`release_tree`**, from `on_exit`. Preferences are written first;
  everything else is a cache of the filesystem, so the tree and everything
  derived from it are handed off and the window closes at once. At process
  exit the reclaim thread is killed wherever it is, which is the point —
  the OS releases the whole address space regardless, so the teardown was
  work nobody was waiting for.

`release_tree` swaps in `Tree::placeholder` rather than leaving a dangling
state, so anything that still queries the app while the window tears down
gets a valid empty tree.

## 3. Treemap tile budget: level-order, never depth-first

A full drive produces far more tiles than are worth drawing, so
`gui::treemap_layout` works to a budget (`MAX_TILES`). **How that budget
is spent is a correctness issue, not a tuning knob.**

Spending it depth-first — what a plain recursion does — means the first
top-level directory descends to its leaves and consumes the whole budget
before its siblings are reached. Measured on a synthetic drive-shaped
tree, the old code emitted **4 of 7 top-level tiles, covering 57% of the
panel and stopping at x=1206 of 1900**. On a real C: drive that showed up
as the right-hand third of the treemap rendering as blank panel.

The traversal is now level-order:

- A level either completes or is not started. `expand` takes no budget
  argument for exactly this reason — stopping part-way through a level is
  what opens a hole.
- The budget check happens between levels, in `prioritize`, which sorts
  pending expansions by visible area and keeps what fits. Dropping a
  pending expansion only costs detail: that directory still renders as its
  own solid tile.
- Complete coverage is therefore an invariant, guarded by
  `a_tree_too_big_for_the_budget_still_covers_the_whole_panel`.

Level order also happens to give the correct paint order for free —
children are emitted after the parents they paint over.

### Which children get a tile

By projected area, not by rank. A child gets its own tile when its share
of the parent works out to at least `MIN_TILE_AREA_PX`; the rest fold
into one aggregate "N more items" tile.

This started life as a fixed "first 80 children" cap, which asked the
wrong question. Whether a child is worth drawing depends on how much room
it would get, not on where it sorted by size — a folder of 190 chunky
subdirectories got 80 tiles and one grey slab covering most of the panel,
while a folder of 30 specks was drawn in detail nobody could see.
`MAX_CHILDREN_PER_LEVEL` survives at 2048 purely so a pathological
directory cannot make one level allocate without bound.

Folding rather than dropping matters: the squarify pass normalizes
against the sizes it is handed, so discarding the tail makes the
survivors expand to fill area that is not theirs. A directory whose top
80 files are 40% of its bytes would draw them at 100%.

A directory whose children are *all* below the floor is left as its own
tile instead. The parent already represents exactly those bytes, and a
grey slab covering the identical rect says strictly less.

### Geometry has to stay inside its parent

Pixel dimensions round **down**, and the label strip is a whole number of
pixels. Rounding to nearest let a child come out fractionally larger than
the rect it was given, so it extended past its parent, painted over a
sibling, and credited that sibling's pixels to the wrong directory — an
error that grew with depth. Rounding down leaves an invisible hairline
instead. `no_tile_ever_escapes_the_panel_or_its_parent` sweeps a range of
panel sizes and checks this.

The label strip is measured from the font the renderer will actually draw
with and passed into the layout, rather than hard-coded. A strip shorter
than the text means the children painted into the rest of the tile cover
the bottom of their own parent's name, which showed up as descenders
being sliced off every `g` and `p`. That is also why the strip is part of
the treemap cache key.

### Render cost

Each tile is a 5×5 cushion-shaded mesh — 25 vertices, 32 triangles —
which at `MAX_TILES` would be a million vertices per frame. Two things
keep that in hand: tiles under `MIN_CUSHION_PX` on a side get a flat rect
(4 vertices, 2 triangles), since the gradient spans too few pixels to see
anyway, and tiles under `MIN_GRID_PX` get no grid outline. The outlines
were the worse of the two — a 1px border on each side of a 3px tile
leaves one pixel of colour, so the dense regions rendered as black mush
*and* paid a stroke per tile for it.

## 4. Things that are still O(tree), and when they run

These are correct but not cheap, and they are on user-triggered paths, not
per-frame ones. Keep it that way:

- `refresh_extensions` — full subtree walk. Runs on zoom change, size-mode
  toggle, and scan completion.
- `refresh_largest_files` — full subtree walk. Runs on scan completion.
- `duplicates::find_duplicates` — already backgrounded.

If any of these ever needs to run more often, background it rather than
making the frame wait.
