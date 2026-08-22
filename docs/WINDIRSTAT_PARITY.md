# WinDirStat 1.1.2 view parity

The reference installed on the development machine is **WinDirStat
1.1.2.80 (Unicode)**. This document deliberately compares rustdirstat with
that installed version. WinDirStat 2.x is a different, actively evolving
product surface with additional analytics visualizations. It backs the
view-by-view comparison summarized in the [README](../README.md).

## The three WinDirStat views

WinDirStat 1.1.2 defines three coupled views. Their coupling is part of each
view's contract, not an optional convenience.

| View | WinDirStat definition | rustdirstat GUI |
|---|---|---|
| Directory list | An Explorer-like expandable hierarchy, sorted independently at each level. Columns expose size, subtree distribution, percentage, files, subdirectories, last change, and attributes. | `All Files` is an expandable hierarchy rooted at the scan target. It has sortable name/size/modified columns, drag-reorderable headers, all reference statistics, distribution bars, unreadable/link indicators, and selection coupling. |
| Extension list | A sortable type breakdown with Extension, Color, Description, Bytes, % Bytes, and Files columns. Selecting an extension highlights matching files in the treemap. Selecting a file selects its extension. | The `Extensions` pane groups by exact lowercase extension (including `[no extension]`), presents those six sortable, drag-reorderable columns separately, and performs both directions of selection coupling. |
| Treemap | Every file is a rectangle whose area is proportional to size. Directory subtrees form nested regions, extension determines color, and cushion shading exposes hierarchy. Clicking a tile selects/expands its directory-list path; list selection frames the tile. Zoom changes the displayed subtree. | The recursively nested squarified treemap uses proportional area, extension color, cushion shading, optional grid/labels/free space, click-to-tree coupling, a white selection frame, and zoom in/out/reset. It can be hidden and resized to zero. |

## Workspace and layout parity

- The classic layout places Directory and Extension views above the Treemap.
- The toolbar's orientation button and **Treemap** menu can switch to a
  vertical layout with the Treemap on the right.
- Every list/treemap boundary is a real resizable splitter with a zero minimum;
  no pane has the previous hard-coded 80-pixel floor.
- Menu bar, padded toolbar, padded pane frames, status bar, and settings are
  separate surfaces and can be shown or hidden where applicable.
- Logical/physical size, free-space tile, grid lines, labels, zoom, rescan,
  folder selection, CSV export, open/reveal/copy, properties, Recycle Bin
  deletion, permanent deletion, and empty-folder actions have GUI commands.
- Rescans, folder changes, duplicate hashing, and Windows maintenance commands
  run in background workers; progress is visible while the previous scan stays
  interactive.
- Toolbar actions wrap at narrow widths and the directory tree switches to a
  compact Name/Size/% layout rather than clipping important content.
- Tree rows expose the main cleanup/navigation commands through a context menu
  and support Up/Down/Left/Right/Enter keyboard navigation.

## Choosing what to scan

WinDirStat 1.1.2 opens on a selection dialog offering a folder, one
drive, several drives, or all local drives. rustdirstat matches that
since 0.3.0:

- The **Locations** page of the settings card lists the fixed and
  removable drives with how much of each is used, ticks any number of
  them, and scans them into one tree; the folder picker is beside it for
  anywhere the list does not know about.
- Both binaries take several paths on the command line
  (`rustdirstat C:\ D:\`), which is the same choice without the window.
- Several roots hang off a synthetic top-level node, and every path
  resolves against the root it belongs to rather than through that node.
- Free space is reported **per root** and never summed: it is a property
  of a volume, so two roots on one volume share one figure and two on
  different volumes have two that cannot be added. The free-space tile
  therefore appears when the view is a whole volume — the tree itself for
  a single-drive scan, or one root once zoomed into it.

## Additional rustdirstat views

These are additive and do not replace the installed-version core:

- **Largest Files**: the 200 largest files in the scan, using the bounded
  streaming implementation shared with the TUI.
- **Duplicate Files**: byte-identical groups found by size prefilter plus
  BLAKE3 content hashing, with reclaimable space shown per group.
- **Search Results**: whole-tree glob search (`*`, `?`, `[a-z]`, `{jpg,png}`), or regular expressions prefixed
  with `re:`, capped at 2,000 visible results.

## Reference material

- Installed executable metadata: `C:\Program Files (x86)\WinDirStat\windirstat.exe`
  reports 1.1.2.80.
- WinDirStat documentation: <https://documentation.help/WinDirStat/windirstat.htm>
- View coupling: <https://documentation.help/WinDirStat/coupling.htm>
- Directory list: <https://documentation.help/WinDirStat/directorytree.htm>
- Treemap: <https://documentation.help/WinDirStat/treemap.htm>
- Cleanups: <https://documentation.help/WinDirStat/actions.htm>
