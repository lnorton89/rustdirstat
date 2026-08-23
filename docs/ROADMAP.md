# Roadmap and deferred work

Three lists: what a release is aiming at, what is still open on the
release currently in flight, and everything that has been *deliberately*
left out — each with its reason and the earliest release it could
sensibly land in.

The last two are the point of this file. A sprint plan's non-goals
disappear when the release ships, and the same idea then gets re-proposed
from scratch six months later with none of the reasoning attached; work
that is merely *unfinished* disappears faster still, because it only ever
existed in whichever conversation stopped short of it. These are
decisions and open threads, not omissions — so they live somewhere that
outlives the plan, and the session, that made them.

**Anything deferred, cut, or left unverified goes in this file before the
conversation that decided it ends.**

## In flight

**0.3.0 — the platform release.** Full detail in
[`0.3.0_PLAN.md`](0.3.0_PLAN.md). Sprints past 7 were not in that plan:
they are its deferred list, being worked through.

| Sprint | Item | State |
|---|---|---|
| 8 | A tree that fills in while the scan runs | done |
| 9 | Hard-link overlap measured and reported | done |
| 10 | User-defined cleanups, with a written threat model | done |
| 11 | Localization: catalogue, fallback, language picker, chrome migrated | done |
| 12 | Signing path written, waiting only on certificates | done |
| 13 | TUI parity: hard links, a scan that shows its work, cleanups, localization | done |
| 14 | A bug pass over the code the release did not touch | done |
| 15 | A review of the release's own changes | done |
| 1 | Leave egui 0.29 behind (0.36) | done |
| 2 | A scan you can stop (cancellation) | done |
| 3 | Windows: one handle per directory (allocation size, scan-time file id) | done |
| 4 | More than one scan root | done |
| 5 | Distribution breadth (rpm, winget, Homebrew, AUR) | done |
| 6 | 120 FPS held during a scan, with tests that check it | done |
| 7 | Properties as a modeless, movable window | done |

## Open on 0.3.0

Everything in the sprint table above is implemented and reviewed; these
are what stand between that and a shipped release.

| Item | State | Notes |
|---|---|---|
| Merge [PR #7](https://github.com/lnorton89/rustdirstat/pull/7) | waiting | Required checks were green on the branch before and after the review-fix commit. |
| Cut `v0.3.0` | not started | Per [`CONTRIBUTING.md`](../CONTRIBUTING.md), in order: version bump commit, `cargo run --example changelog -- --release v0.3.0` committed *before* any tag, PR with checks green, merge commit (never a squash), annotated tag on the merge commit, push. |
| Watch the window on macOS and Linux | **not done** | The egui 0.36 migration's own "done when" asked for a person watching the app on all three platforms. It was run, screenshotted and driven on Windows only; the other two have passing tests and a compiling build, which is not the same claim. |
| First real run of the new packaging | pending | The `.rpm` was built for real in WSL and CI now builds both Linux packages on every PR, but the winget/Homebrew/AUR manifest rendering has only ever run as workflow text. The `v0.3.0` release is its first execution — check the `package-manifests.tar.gz` asset before announcing it anywhere. |
| Decide whether the Windows listing keeps its cost | open | The directory-listing walk buys cluster-accurate sizes and scan-time identity for about 13 µs per directory ([`PERFORMANCE.md`](PERFORMANCE.md) §0b). That is a real regression on a shallow tree of tiny directories, taken deliberately. `RUSTDIRSTAT_STD_LISTING=1` is the way back, and whether that escape hatch stays past 0.3.0 is undecided. |

## Deferred, with reasons

What is left, after a pass through this list. Four of the five non-goals
the 0.3.0 plan named have since been built — the live tree, hard-link
measurement, user-defined cleanups, and localization — and what remains
below is either genuinely blocked (a certificate nobody here can buy, a
desktop nobody here can test on), an incremental follow-through with the
mechanism already shipped, or a decision taken rather than a task
skipped. Each says which.

| Item | Why not now | Earliest |
|---|---|---|
| **Localization: the rest of the strings** | The mechanism ships and both front ends are on it. Migrated in the window: menus, view names, status bar, settings pages, inspector. Migrated in the terminal: the header, the footer, every pane title, the list markers, the popup titles, and the whole help screen. Still English literals: the window's guide, about, maintenance and duplicates pages; the terminal's properties popup body, its prompt hints, and the status messages `tui::app::operations` writes; and the Windows tool descriptions, which are one table shared by both. Also still English: the nine `Category::label()` names, which both front ends show. Each is a mechanical pass now that the catalogue is guarded in both directions — `every_key_the_code_uses_is_in_the_catalogue` catches a key the code invents, and `every_key_the_catalogue_defines_is_asked_for` catches the quieter opposite, a key translated by someone and displayed by nothing. Note that `assets/lang/de.toml` is a worked example in this repository, not a shipped translation: nothing packages it, and a user starts one with the Appearance page's *Write a starter catalogue* button instead. | incremental |
| **Signed installers** (Authenticode, macOS notarization) | **The workflow is written and inert.** `release.yml` signs the Windows binaries with `signtool`, signs the macOS binaries with a hardened runtime and a timestamp, and submits a zip of the macOS binaries to `notarytool` (a zip because that is one of the three formats the notary service accepts, and the shipped archive is a tarball) — each step gated on its secret being set, so a release with no certificates behaves exactly as it does today. What is missing is the certificates themselves: an Authenticode certificate and an Apple Developer membership are bought, are tied to an identity, and cannot live in a public repository. Add the secrets named in the workflow comments and the next release is signed. | blocked on certificates only |
| **AppImage** | The one packaging format in the 0.3.0 plan that did not land. Building one is easy; *verifying* one is not — it bundles a GUI's runtime and only a real desktop can say whether the bundle works, which is exactly the check nothing in this project can run. The `.deb`, the `.rpm` and the Nix flake already cover the distributions an AppImage would target. | when someone can test one |
| **Publishing to crates.io** | Decided against for 0.3.0: `cargo install rustdirstat` would work, but it would build the GUI from source on every machine that ran it — pulling the whole egui/wgpu stack and the Linux windowing headers — to produce what the release archives already ship prebuilt. The package-manager manifests are the better answer to the same want. | unscheduled |

## Done, and where the reasoning lives

- **Filesystem identity, destructive-operation safety** — 0.2.1, from
  [`0.2.0_REVIEW.md`](0.2.0_REVIEW.md) via [`0.2.1_PLAN.md`](0.2.1_PLAN.md).
- **Release process: changelog before tag, pinned packaging tools, bound
  SBOM** — 0.2.2, from [`0.2.1-release-review.md`](0.2.1-release-review.md).
- **Eight defects in code 0.3.0 did not touch**, six of them the same two
  mistakes repeated — a multi-root label used as a path, and a canonical
  `\\?\` path shown to something that is not the filesystem —
  [`0.3.0_BUG_PASS.md`](0.3.0_BUG_PASS.md). That file also records what
  was read and found sound, and what was not read at all.
- **Six defects in the release's own 10,800 lines** — a `u16` slice built
  at an alignment the same function refused to assume, a notarisation
  step submitting a format Apple does not accept, ten string literals
  that lost their line continuations, seven catalogue keys nothing asked
  for, a translation feature with no way to obtain a catalogue, and a
  hard-link figure in the wrong unit — in the second half of
  [`0.3.0_BUG_PASS.md`](0.3.0_BUG_PASS.md). Two of the six now have a
  check that catches the whole class: `tests/source_hygiene.rs` and
  `every_key_the_catalogue_defines_is_asked_for`.
