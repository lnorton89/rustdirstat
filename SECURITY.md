# Security Policy

## Reporting a vulnerability

Please do **not** open a public issue for a security vulnerability. Instead,
report it privately:

- **Preferred:** use GitHub's private vulnerability reporting on the
  [Security tab](https://github.com/lnorton89/rustdirstat/security/advisories/new).
- **By email:** [lnorton89@gmail.com](mailto:lnorton89@gmail.com). If you
  encrypt, ask for a key first; otherwise plain text is fine.

Include what you found, how to reproduce it (a small fixture directory is
ideal — the app scans user-supplied paths), and which version or commit you
tested against. Reports are acknowledged within a few days, and fixes land
in the next release.

## Supported versions

Only the latest release is supported. Fixes ship in a new release; there is
no backport window for older versions.

## Scope

RustDirStat scans paths you give it and can open, move, or delete files it
finds there — that is the product, not a vulnerability. What *is* in scope:
memory-safety issues in path handling, crashes on malicious directory
structures, unsafe-block misuse, or anything that would let an untrusted
tree do something to your system beyond what the UI asks for.

Everything else — bug reports, feature requests, and questions — belongs on
the [issue tracker](https://github.com/lnorton89/rustdirstat/issues); see
[`CONTRIBUTING.md`](CONTRIBUTING.md).
