# WinDirStat 1.1.2 view parity

The reference installed on the development machine is **WinDirStat
1.1.2.80 (Unicode)**. This document deliberately compares rustdirstat with
that installed version. WinDirStat 2.x is a different, actively evolving
product surface with additional analytics visualizations.

## The three WinDirStat views

WinDirStat 1.1.2 defines three coupled views. Their coupling is part of each
view's contract, not an optional convenience.

| View | WinDirStat definition | rustdirstat GUI |
|---|---|---|
| Directory list | An Explorer-like expandable hierarchy, sorted independently at each level. Columns expose size, subtree distribution, percentage, files, subdirectories, last change, and attributes. | `All Files` is an expandable hierarchy rooted at the scan target. It has sortable name/size/modified columns, all reference statistics, distribution bars, unreadable/link indicators, and selection coupling. |
| Extension list | A sortable type breakdown with color, extension/description, bytes, percentage, and file count. Selecting an extension highlights matching files in the treemap. Selecting a file selects its extension. | The `Extensions` pane groups by exact lowercase extension (including `[no extension]`), shows color/category, bytes, percentage, and count, and performs both directions of selection coupling. |
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

## Additional rustdirstat views

These are additive and do not replace the installed-version core:

- **Largest Files**: the 200 largest files in the scan, using the bounded
  streaming implementation shared with the TUI.
- **Duplicate Files**: byte-identical groups found by size prefilter plus
  BLAKE3 content hashing, with reclaimable space shown per group.
- **Search Results**: whole-tree glob search, or regular expressions prefixed
  with `re:`, capped at 2,000 visible results.

## Reference material

- Installed executable metadata: `C:\Program Files (x86)\WinDirStat\windirstat.exe`
  reports 1.1.2.80.
- WinDirStat documentation: <https://documentation.help/WinDirStat/windirstat.htm>
- View coupling: <https://documentation.help/WinDirStat/coupling.htm>
- Directory list: <https://documentation.help/WinDirStat/directorytree.htm>
- Treemap: <https://documentation.help/WinDirStat/treemap.htm>
- Cleanups: <https://documentation.help/WinDirStat/actions.htm>
