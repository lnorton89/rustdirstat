# Roadmap and deferred work

Two lists. The first is what a release is currently aiming at; the second
is everything that has been *deliberately* left out, with the reason and
the earliest release it could sensibly land in.

The second list is the point of this file. A sprint plan's non-goals
disappear when the release ships, and the same idea then gets re-proposed
from scratch six months later with none of the reasoning attached. These
are decisions, not omissions — so they live somewhere that outlives the
plan that made them.

## In flight

**0.3.0 — the platform release.** Full detail in
[`0.3.0_PLAN.md`](0.3.0_PLAN.md).

| Sprint | Item | State |
|---|---|---|
| 1 | Leave egui 0.29 behind (0.36) | in progress |
| 2 | A scan you can stop (cancellation) | not started |
| 3 | Windows: one handle per directory (allocation size, scan-time file id) | not started |
| 4 | More than one scan root | not started |
| 5 | Distribution breadth (rpm, AppImage, winget, Homebrew, AUR) | not started |
| 6 | 120 FPS held during a scan, with tests that check it | not started |
| 7 | Properties as a modeless, movable window | not started |

## Deferred, with reasons

| Item | Why not now | Earliest |
|---|---|---|
| **A tree that fills in live during the scan** | WinDirStat does this; rustdirstat shows counters and keeps the previous tree interactive. Publishing partial trees needs a snapshot protocol that does not violate "nothing tree-sized in a draw call" — the constraint in [`PERFORMANCE.md`](PERFORMANCE.md) that the whole GUI is built around. | 0.4.0 |
| **Inode-deduped tree-wide physical totals** | Totals count each hard link once per pathname, which is WinDirStat parity. True inode-deduped accounting needs a tree-wide inode set, which costs real memory on a drive-sized scan. Duplicate *reclaimable* space is already hard-link aware, which is where the number actually misleads. | unscheduled |
| **User-defined cleanup commands** (WinDirStat's "Cleanups") | A shell-execution surface in an app whose other buttons delete files. Wants a written threat model — argument quoting, what a `%p` expansion may contain, whether a command may run on a multi-selection — before any code. | unscheduled |
| **Localization** | Both front ends are English-only and nothing is wired for translation; the TUI additionally assumes width-1 glyphs in places `unicode-width` does not cover. | unscheduled |
| **Signed installers** (Authenticode, macOS notarization) | Needs paid certificates that cannot live in the repository. Provenance and SBOM attestations are the substitute a certificate-less project can offer, and they ship today. | blocked, not deferred |
| **Publishing to crates.io** | Undecided rather than rejected: `cargo install rustdirstat` would work, but a GUI binary crate on crates.io implies a support surface. Revisit alongside the 0.3.0 packaging sprint. | 0.3.0, if decided |

## Done, and where the reasoning lives

- **Filesystem identity, destructive-operation safety** — 0.2.1, from
  [`0.2.0_REVIEW.md`](0.2.0_REVIEW.md) via [`0.2.1_PLAN.md`](0.2.1_PLAN.md).
- **Release process: changelog before tag, pinned packaging tools, bound
  SBOM** — 0.2.2, from [`0.2.1-release-review.md`](0.2.1-release-review.md).
