# sile-rust

This is a port of the `sile` typesetting system to rust. The source code for `sile` lives in `../sile`. Refer to `RUST_PORT_PLAN.md` for a high level implementation plan.

## Current crates

* `sile-core`: contains the core types and logic
* `sile-cli`: the entry point for the sile cli tool
* `sile-parity`: test-only tool that runs SILE's regression tests through `sile-core` and compares layouts with SILE's expected output

## Features

* Default builds are pure Rust (rustybuzz). `--features harfbuzz` links system HarfBuzz, needed for Graphite fonts.

## SILE parity

```sh
scripts/fetch-parity-corpus.sh          # pinned SILE tests + fonts into .parity/
cargo run --release -p sile-parity      # summary table, HTML report in target/parity/
cargo run --release -p sile-parity -- --trace italic   # our debug trace for one test
```

Comparison is deliberately fuzzy: glyphs, line/page breaks, and positions within 0.5pt are scored separately. `sile-parity/src/driver.rs` maps the SIL subset onto `DocumentBuilder`; unsupported commands are reported, never approximated.

## Codebase rules

* You are a senior developer. Don't overuse comments.
