// The crate denies `unwrap`/`expect`/`panic` (see `[lints.clippy]` in
// Cargo.toml) because library code that ships must not have them. Test
// code is a different proposition: a test asserting on a value it knows is
// there reads better as `unwrap` than as a `let ... else` that cannot
// happen, and a panic there is a failing test rather than a crashed app.
// Exempting the test configuration here — rather than sprinkling `allow`
// attributes around — is what lets CI lint every target strictly.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod color;
pub mod config;
pub mod csv_export;
pub mod duplicates;
pub mod gui;
pub mod model;
pub mod platform;
pub mod report;
pub mod scanner;
pub mod stats;
pub mod treemap;
pub mod tui;
pub mod util;
pub mod wintools;
