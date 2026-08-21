name: Feature request
description: Suggest an improvement — a new feature, or something that should work better
title: "[Feature]: "
labels: ["enhancement"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for the suggestion. Please search the [existing issues](https://github.com/lnorton89/rustdirstat/issues) first — it may already be proposed.

  - type: dropdown
    id: frontend
    attributes:
      label: Where does this belong?
      description: The two front ends share a scanning core but not a UI. Picking one helps scope the work.
      multiple: true
      options:
        - Terminal UI (rustdirstat)
        - Desktop GUI (rustdirstat-gui)
        - Shared core (scanning, search, reports)
        - Both front ends
    validations:
      required: true

  - type: textarea
    id: motivation
    attributes:
      label: Problem
      description: What is this for? What does it let someone do that they cannot today?
    validations:
      required: true

  - type: textarea
    id: solution
    attributes:
      label: Proposed solution
      description: How you imagine it working — interaction, placement, defaults. Sketches or ASCII mockups welcome.
    validations:
      required: true

  - type: textarea
    id: alternatives
    attributes:
      label: Alternatives considered
      description: Anything else you tried or thought of, and why it does not cover the need.
