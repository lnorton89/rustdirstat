# Examples

Two binaries that write into the source tree. That is the whole reason
they are `examples/` rather than `#[test]`s: `cargo test` should never
mutate the repository, so the parts of the project that *are* generated —
the brand art and the changelog — live behind `cargo run --example`
instead. Both are still compiled by `cargo clippy --all-targets`, so they
cannot rot silently the way a script nobody runs would.

## `brand_assets`

Regenerates the PNGs and ICO in `assets/brand/` from the single mark
definition in [`src/brand.rs`](../src/brand.rs):

```sh
cargo run --example brand_assets
```

Every icon size is the same call the window and taskbar icons are built
from (`rustdirstat::brand::rgba`), so the README art, the About card, and
the taskbar icon are one drawing by construction rather than by anyone
remembering to re-export one when the other changes. The wordmark is set
in egui's default proportional family, so the name on the project page is
the name in the app's own title bar. Regenerate the PNGs rather than
editing them.

## `changelog`

Regenerates `CHANGELOG.md` from the git history, grouping
conventional-commit subjects under the release tag that shipped them:

```sh
cargo run --example changelog
```

Check the committed file against the history without rewriting it:

```sh
cargo run --example changelog -- --check
```

Or write the section for a release that is *about to be tagged*, so the
tag can be placed on a commit whose changelog is already final:

```sh
cargo run --example changelog -- --release v0.2.2
```

`CHANGELOG.md` is generated, never hand-edited — released sections are
rewritten on every run. `--release` exists because of `v0.2.1`, whose tag
was placed before the changelog was regenerated and so forever carries the
release listed under `Unreleased`. See [`CONTRIBUTING.md`](../CONTRIBUTING.md)
for the release ordering, and the module docs in `changelog.rs` for the
things worth knowing before editing the file by hand.
