## Summary

What this change does and why. One or two sentences — the *why* matters more
than the *what*. If this fixes an issue, reference it here (e.g. `Closes #12`).

## Test plan

What you ran and on which platform(s). `cargo test`, `cargo clippy --all-targets
--all-features -- -D warnings`, and `cargo fmt --all -- --check` must be clean
before this is mergeable — a warning is a build failure in CI. Note that a lot
of code is platform-gated (`cfg(unix)` / `cfg(windows)`), so a clean run on one
OS proves less than it looks; say which ones you checked.

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --all-features` passes
- [ ] No new `unwrap` / `expect` / `panic!` — not even in tests
- [ ] New files carry the module header banner (`Module:` / `Description:` /
      `Dependencies:`) and the matching `mod` declaration
- [ ] UI changes include a screenshot or description of the visual effect
- [ ] Commit message uses the `fix:` / `feat:` / `refactor:` / `docs:` /
      `test:` / `build:` prefixes

Full guidelines, including how to build and what CI enforces, are in
[`CONTRIBUTING.md`](CONTRIBUTING.md).
