# Contributing to RustDirStat

Thanks for considering a contribution. RustDirStat is a WinDirStat clone in
Rust with two front ends — an egui/eframe desktop GUI and a ratatui terminal
UI — over one scanning core. Before you start, read
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the module map and
[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) before touching anything on a
per-frame path. Both are short, and the non-obvious constraints live there.
The [README](README.md) covers installation and usage; this file is about
contributing.

## Reporting bugs and requesting features

Open an issue on the [issue tracker](https://github.com/lnorton89/rustdirstat/issues).
Security vulnerabilities are *not* reported here — see
[`SECURITY.md`](SECURITY.md) for the private channel.

For a bug report, include:

- Your OS and how you installed the binary (release archive, Nix, `cargo build`)
- The exact command you ran
- What you expected to happen and what actually happened
- A minimal reproduction if you can make one (a small fixture directory beats
  "scan my whole drive")

Feature requests are welcome as issues too. It helps to say which of the two
front ends the feature is for — they share a core but not a UI.

## Building and running

Requires a recent stable Rust toolchain (via [rustup](https://rustup.rs));
the exact version CI uses is pinned in `rust-toolchain.toml`.

```bash
cargo build
cargo run --bin rustdirstat /path/to/scan      # terminal UI
cargo run --bin rustdirstat-gui /path/to/scan  # desktop GUI
```

A full debug build of this crate is large (~5.6 GB with default settings); if
disk is tight, `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` cuts
that to ~1.2 GB without changing what gets checked. Prefer Nix?
[`NIX.md`](NIX.md) covers the flake and the dev shell.

## Before you submit

Three commands run in CI on Linux, macOS, and Windows, and all three must be
clean locally first — a warning is a build failure there:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Rules the codebase actually enforces

These are enforced by lints and build-time checks, so a violation fails CI
rather than getting caught in review:

- **Every source file opens with a module header banner** — a ruled block
  naming `Module:`, `Description:`, and `Dependencies:`, then the module's
  `//!` docs. `src/header_check.rs` walks `src/` and `tests/` and fails the
  build for a file missing one, so a new file needs its header in the same
  commit that adds it.
- **No `unwrap`, `expect`, or `panic!` anywhere — including tests.** In
  library code use `let ... else`, `?`, or an explicit fallback. Tests return
  `anyhow::Result<()>` and use `?` for fallible calls.
- **Every `unsafe` block is a safe leaf wrapper.** One named function per
  FFI call taking safe Rust arguments, a `// SAFETY:` comment on the block,
  and an owning wrapper with a `Drop` impl where there's a matching
  destroy/free/close call. Callers and test bodies should never need
  `unsafe` themselves.
- **Nothing tree-sized is recomputed in a draw call.** The GUI is immediate
  mode — `gui::ui::draw` runs in full every frame. Derived data (visible
  rows, treemap tiles) is cached on `GuiApp` and keyed off observed state,
  not invalidated by hand. Never free a scanned tree on the UI thread either;
  use `drop_in_background`.
- **Treemap traversal is level-order, and must stay that way.** Depth-first
  tile budgeting leaves the right-hand side of a large volume's treemap
  blank. See `src/gui/treemap_layout.rs`.
- **No literal `Color32` in drawing code.** Every color comes from
  `palette()` in `src/gui/ui/theme.rs`; the one exception is the brand mark
  in `src/brand.rs`.
- **Spacing comes from the shared scale.** Margins and insets are one of
  `SPACE_XS` / `SPACE_SM` / `SPACE_MD` / `SPACE_LG`. All five `ScrollArea`s
  take their look from `scroll_style()`. There is one modal (an
  `Option<ModalPage>`), not ad-hoc `egui::Window`s.

## Platform notes

A lot of code is platform-gated — `platform.rs` has a whole `cfg(unix)`
module, the Windows system tools are `cfg(windows)`, and
`tests/quit_stress.rs` compiles to nothing on Windows. A clean `cargo test`
on one OS proves less than it looks, so verify on whatever platforms you can.

On Windows, WSL is a fast way to exercise the Linux side:

```bash
wsl -e bash -lc "cd /mnt/c/path/to/repo && CARGO_TARGET_DIR=\$HOME/rds-target cargo clippy --all-targets"
```

Use a separate `CARGO_TARGET_DIR` so the two toolchains don't fight over
`target/`.

## Commit hygiene

- Keep each commit focused on one change, with a message that says *why*
  (`fix:`, `feat:`, `refactor:`, `docs:`, `test:`, `build:` prefixes match
  the existing history).
- Stage only the files you changed, by name — not `git add -A`.
- Line endings are handled by `.gitattributes`; every text file is stored and
  checked out as LF. Don't convert endings by hand, and don't worry about a
  file's line endings when you edit it — git normalizes on the way in.
- If you touched a file that also carries someone else's in-flight edits,
  stage only your own hunks rather than the whole file.

## Opening a pull request

Open the PR against `main` with a title that summarizes the change and a
body that says what and why. CI runs the three commands above on all three
platforms; a red check means the change is not ready to merge. Small,
focused PRs are easier to review and land faster than large ones.
