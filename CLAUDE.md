# sile-rust

This is a port of the `sile` typesetting system to rust. The source code for `sile` lives in `../sile`. Refer to `RUST_PORT_PLAN.md` for a high level implementation plan.

## Current crates

* `sile-core`: contains the core types and logic
* `sile-cli`: the `sile` command, which typesets Markdown (CommonMark, with `$`/`$$` math) to PDF through the builder API
* `sile-parity`: test-only tool that runs SILE's regression tests through `sile-core` and compares layouts with SILE's expected output

## Features

* Default builds are pure Rust (rustybuzz). `--features harfbuzz` links system HarfBuzz, needed for Graphite fonts.

## Language data

Patterns are SILE's own, converted to `sile-core/languages/*.pat`; localized messages are SILE's Fluent files (`*.ftl`). Both are embedded via `sile-core/src/language_data.rs`; regenerate with `scripts/import-sile-languages.py <sile checkout>`.

## Math

`sile-core/src/math` ports SILE's math package. Formulas are a typed `MathNode` tree; `TexMath` parses SILE's TeX-like syntax into it and `mathml::Element` converts MathML (the parity driver uses this for `\mathml`). `DocumentBuilder::add_math` sets a formula inline or displayed. The operator dictionary `sile-core/math/operators.txt` is generated from SILE's by `scripts/import-sile-math.py <sile checkout>`.

## SILE parity

```sh
scripts/fetch-parity-corpus.sh          # pinned SILE tests + fonts into .parity/
cargo run --release -p sile-parity      # summary table, HTML report in target/parity/
cargo run --release -p sile-parity -- --trace italic   # our debug trace for one test
cargo run --release -p sile-parity --features sile-core/harfbuzz   # shape like SILE
```

SILE shapes Gentium Plus with Graphite, which doesn't kern, so the pure Rust shaper's kerning shows up as glyph width differences. Run with `--features sile-core/harfbuzz` for comparable widths.

Comparison is deliberately fuzzy: glyphs, line/page breaks, and positions within 0.5pt are scored separately. `sile-parity/src/driver.rs` maps the SIL subset onto `DocumentBuilder`; unsupported commands are reported, never approximated. Settled outcomes live in `sile-parity/src/known.rs`: differences that come from SILE's expected output (with the reason), tests SILE marks KNOWNBAD, and skipped features (math). They are reported as explained/skipped, so close/differs/unsupported lists only open work. Tests whose Lua is ported to Rust live in `sile-parity/src/ports.rs`: each port does what the Lua does through sile-core's public API, in place of the `\lua`/`\script` chunks and the commands they define.

## CI

`.github/workflows/ci.yml` gates PRs on tests and clippy with both shapers. Parity runs too but never fails the build; its summary is on the run page and the HTML report is an artifact.

## Codebase rules

* You are a senior developer. Don't overuse comments.
