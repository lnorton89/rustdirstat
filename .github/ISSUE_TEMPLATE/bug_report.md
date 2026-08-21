name: Bug report
description: Something is broken — the app crashed, hung, or did the wrong thing
title: "[Bug]: "
labels: ["bug"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for reporting. Please search the [existing issues](https://github.com/lnorton89/rustdirstat/issues) first — it may already be known.

  - type: textarea
    id: environment
    attributes:
      label: Environment
      description: Your OS and how you installed the binary
      placeholder: |
        - OS: (e.g. Windows 11, macOS 15, Ubuntu 24.04)
        - Install: release archive, Nix, or `cargo build`
        - Version: rustdirstat 0.2.0, or the commit hash if you built from source
        - Front end: GUI (`rustdirstat-gui`) or terminal UI (`rustdirstat`)
    validations:
      required: true

  - type: textarea
    id: reproduction
    attributes:
      label: Steps to reproduce
      description: The exact command you ran, and anything you did after. A small fixture directory beats "scan my whole drive".
      placeholder: |
        1. `cargo run --bin rustdirstat-gui -- /path/to/fixture`
        2. Click ... / press ...
        3. ...
    validations:
      required: true

  - type: textarea
    id: expected
    attributes:
      label: Expected behavior
      description: What did you expect to happen?
    validations:
      required: true

  - type: textarea
    id: actual
    attributes:
      label: Actual behavior
      description: What actually happened instead? Include any error text or panic output.
    validations:
      required: true

  - type: textarea
    id: logs
    attributes:
      label: Relevant output
      description: Terminal output, error messages, or a screenshot of the window.
      render: text

  - type: textarea
    id: context
    attributes:
      label: Additional context
      description: Anything else that might help — what you were scanning, how large the tree is, what else was running.
