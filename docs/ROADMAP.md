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
| 16 | Localization finished: every string routed, twelve languages shipped | done |
| 17 | An audit for the rest of the half-finished work | done |
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
| Cut `v0.3.0` | not started | Per [`CONTRIBUTING.md`](../CONTRIBUTING.md), in order: version bump commit, `cargo run --example changelog -- --release v0.3.0` committed *before* any tag, PR with checks green, merge commit (never a squash), annotated tag on the merge commit, push. The release workflow now refuses a tag that disagrees with `Cargo.toml` before it creates the draft, so forgetting the bump is a failed run rather than a `v0.3.0` release carrying `Version: 0.2.2-1` that apt declines to install over 0.2.2. As of this writing `Cargo.toml` still reads `0.2.2`, so that guard would fire today. |
| Watch the window on macOS and Linux | **Linux runs; nobody has looked at macOS** | The owner has watched the window on Linux, but on an older tree — and 0.3.0 *is* the egui 0.29 → 0.36 migration, which rewrote the panel, menu and margin APIs, so that is the change most likely to move rendering. Against this branch the Linux binary was built and *run* under WSLg: wgpu picked an adapter, winit created a Wayland surface, and the event loop ran for 25 seconds with nothing on stderr. That exercises adapter selection, window creation and egui 0.36's per-frame panel and menu construction — strictly more than "compiles and passes tests", strictly less than looking at it, because WSLg would not export the window to the Windows desktop for a screenshot. macOS has had neither: only CI's tests, which drive a real `egui::Context` but draw to no screen. |
| First real run of the new packaging | **rehearsed end to end; only the live upload is untried** | Both Linux packages were built in WSL with the release's own `--target` invocation and their contents read: `--target` re-maps the `target/release/...` asset paths for `cargo-deb` and `cargo-generate-rpm` alike, and both carry both binaries. That inspection turned up three defects in a `.deb` nobody had ever opened — no `Maintainer`, a 527-line description scraped out of README.md's raw HTML, and a copyright file naming MIT without quoting it — all fixed and now asserted by CI. The manifest step was then extracted straight out of `release.yml` and executed against stub assets: all three files render, the winget `RelativeFilePath` backslashes and the PKGBUILD's `$pkgdir`/`$pkgname` survive the heredoc intact, the tarball packs, and `rustdirstat --version` — which the Homebrew `test do` block asserts — works. What remains untried is only what needs a real tag: the upload, the attestations, and notarization. |
| GUI runtime dependencies are dlopen'd, so no packager can see them | **declared by hand** | `ldd rustdirstat-gui` lists libc, libm and libgcc_s and nothing else — winit and wgpu open Wayland, EGL, X11, Xcursor, Xi, xkbcommon, dbus and Vulkan by soname at runtime. So cargo-deb's auto-derived `Depends` came out as bare `libc6` and the PKGBUILD declared nothing, meaning a GUI installed from either would have failed at startup with a dlopen error. Now listed explicitly in all three: Debian as `Recommends` (apt takes them by default, `--no-install-recommends` still gets a TUI-only install on a headless box), RPM as weak `Recommends` too but written as *sonames* rather than package names, because Fedora calls it `libX11` and openSUSE calls it `libX11-6` while both provide `libX11.so.6()(64bit)`, and Arch as hard `depends`, which has no equivalent tier. Hand-written lists go stale silently — the package installs and the window does not open — so CI now reads the sonames back out of the binary it just built and fails if one is undeclared. That catches a dependency bump that starts opening something new; it does not catch a package *name* going wrong in the Debian or Arch list, which no longer matches the soname. |
| Look at the language picker's flags | **drawn and tested, never seen** | The picker now leads each entry with a painted flag (`src/gui/flags.rs`) rather than a bare tag. Geometry is asserted — every tile fits the card, none overlaps its neighbour, every flag paints more than a field and a border, every endonym draws in the bundled font — but a test cannot say whether a 22×15 Union Flag or an arms-less Serbian tricolour is *recognisable*, and nothing in this project can screenshot the window. Four flags are deliberately absent for that reason (Greek, Croatian, Slovak, Slovenian: too fine to draw, or identical to another flag without the charge that distinguishes them) and fall through to a globe; whether any of the twenty drawn ones belong in that group as well is a question for a pair of eyes. |
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
| **Languages the bundled font cannot draw** | Twelve languages ship — English plus Czech, Dutch, French, German, Italian, Polish, Portuguese (BR), Russian, Spanish, Swedish and Turkish — and every one of them is complete, compiled into the binary and selectable with no files installed. Japanese, Chinese and Korean are *not* among them and not for want of translating: egui bundles Ubuntu-Light, which has no CJK glyphs, so those languages would render as rows of empty boxes. Shipping them means bundling a CJK face first, which is a size and licensing decision (Noto Sans CJK is tens of megabytes even subsetted) rather than a translation task. `every_shipped_catalogue_draws_in_the_bundled_font` reads the font's own character map and fails the build for any catalogue that would tofu, so the constraint is enforced rather than remembered. | when a CJK font is bundled |
| **Signed installers** (Authenticode, macOS notarization) | **The workflow is written and inert.** `release.yml` signs the Windows binaries with `signtool`, signs the macOS binaries with a hardened runtime and a timestamp, and submits a zip of the macOS binaries to `notarytool` (a zip because that is one of the three formats the notary service accepts, and the shipped archive is a tarball) — each step gated on its secret being set, so a release with no certificates behaves exactly as it does today. What is missing is the certificates themselves: an Authenticode certificate and an Apple Developer membership are bought, are tied to an identity, and cannot live in a public repository. Add the secrets named in the workflow comments and the next release is signed. | blocked on certificates only |
| **A text report and a move command in the window** | Both exist, are shared code, and are wired to the terminal only. `report.rs` is 328 localized lines the window never calls; `util::move_path` carries a cross-device fallback, symlink recreation, metadata preservation and a self-nesting guard, and only `M` in the terminal reaches it. Neither is a bug — nothing claims the window has them — but they are the same shape as the localization gap: machinery built once and connected once. Adding *Move to…* to the window means a destination picker and a confirmation, which is a feature rather than a repair, so it is a decision rather than an omission. | when the window wants them |
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
- **Three things built and left unconnected**, found by auditing for the
  pattern rather than waiting to be told: the terminal never applied the
  saved language, so every string it had been taught to translate came
  out English; the window hardcoded `ScanOptions::default()`, so
  hard-link measurement — a whole feature, translated into twelve
  languages — could never be turned on under Windows; and `report.rs`
  and `util::move_path` are shared modules wired to the terminal only.
  The first two are fixed and guarded. The third is listed below,
  because connecting it is a feature decision rather than a repair.
- **Six defects in the release's own 10,800 lines** — a `u16` slice built
  at an alignment the same function refused to assume, a notarisation
  step submitting a format Apple does not accept, ten string literals
  that lost their line continuations, seven catalogue keys nothing asked
  for, a translation feature with no way to obtain a catalogue, and a
  hard-link figure in the wrong unit — in the second half of
  [`0.3.0_BUG_PASS.md`](0.3.0_BUG_PASS.md). Two of the six now have a
  check that catches the whole class: `tests/source_hygiene.rs` and
  `every_key_the_catalogue_defines_is_asked_for`.
