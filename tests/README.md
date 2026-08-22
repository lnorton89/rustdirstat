# Integration tests

The binaries in this directory are what `cargo test` runs *after* the
unit tests inside `src/`. They compile as separate crates against the
library and the built binaries, so they cover the seams a unit test
cannot reach: argument parsing, the real process boundary, and the actual
terminal. All three follow the house rule — no `unwrap`, `expect`, or
`panic!`, `anyhow::Result<()>` from each test, and assertions carry a
message.

## `cli.rs`

End-to-end cover for the two non-interactive modes. It runs the actual
compiled `rustdirstat` binary over a fixture tree and checks the report
and CSV output, the missing-path error, and that `--no-tui` and `--csv`
are refused together. It deliberately does not launch the TUI or GUI —
that is what `quit_stress.rs` is for.

## `quit_stress.rs`

The TUI regression test. It drives the real compiled binary inside an
actual pty (so crossterm's raw-mode and event parsing run for real),
floods it with a backlog of synthetic drag-mouse events the way a terminal
replaying buffered input would, then sends a quit key and asserts the
process exits cleanly within a bounded time.

This is the test that found the `use-dev-tty` bug: crossterm's default
Unix event backend registered the tty fd edge-triggered but did not drain
it to `EAGAIN`, so past ~1 KB of buffered input everything after was
silently dropped and the app hung in `epoll_wait`. Unit-level reasoning
about our own event loop could never have caught it. The pty setup is
POSIX-specific, so the whole file is `#![cfg(unix)]` — it compiles to
nothing on Windows.

## `toolchain_pin.rs`

Checks that `rust-toolchain.toml` and both CI workflows name the same Rust
version, and that no workflow falls back to floating `@stable`. Three
places, one fact — the shape that drifts. It reads the repo config at test
time rather than running any binary.
