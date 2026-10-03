# sile-rust

This is a port of the `sile` typesetting system to rust. The source code for `sile` lives in `../sile`. Refer to `RUST_PORT_PLAN.md` for a high level implementation plan.

## Current crates

* `sile-core`: contains the core types and logic; lays documents out into a format-neutral `Layout` (pages, fonts, outline, metadata, structure)
* `sile-pages`: pages on top of sile-core, through its public API only: `DocumentBuilder` (frames, templates, classes, insertions and footnotes, page breaking, parallel flows), frame declarations (`framespec`), folios, cropmarks, TOC, index and `lay_out_until_settled`
* `sile-pdf`: writes a `Layout` as PDF (`sile_pdf::render`), with `PdfOptions` for how it is written
* `sile-svg`: writes a `Layout` as SVG, one document per page (`sile_svg::render`); the CLI uses it for `.svg` outputs
* `sile-cli`: the `sile` command, which typesets Markdown (CommonMark plus GitHub's tables, footnotes, strikethrough and task lists, with `$`/`$$` math) to tagged, accessible PDF through the builder API
* `sile-parity`: test-only tool that runs SILE's regression tests through `sile-core` and compares layouts with SILE's expected output

## Typesetter, arrangers and galleys

`sile_core::builder::Typesetter` turns text and settings into lines and a vertical list without knowing about pages: lines are set to its `FrameContext` (measure, direction, tate). Vertical-mode commands (`new_paragraph`, `add_vskip`, `add_rule`, ...) are provided methods of the `Arranger` trait, whose implementors say where the vertical list goes:

* `sile_pages::DocumentBuilder` (`sile-pages/src/paginator.rs`) pages it. It derefs to its typesetter and owns everything that knows about pages: frames, templates, classes, insertions, page breaking, parallel flows and references. It calls `sync_frame` whenever the frame being filled changes.
* `Galley` (`builder/galley.rs`) keeps it: no measure sets paragraphs at natural width, a measure breaks them, and `take_frame` cuts off what fits a height, leaving the overflow. `lay_out` gives one surface as tall as the material.

Content that needs vertical mode (lists, tables, display math, specimens) is written once as `Arranger` methods, and content that takes callbacks is generic over `Context`, which an arranger or a frontend's state around one implements. Anything sile-pages needs from the typesetter goes through sile-core's public API; footnotes, classes, the TOC and the index stay `DocumentBuilder`-only.

## Features

* Default builds are pure Rust (rustybuzz). `--features harfbuzz` links system HarfBuzz, needed for Graphite fonts.
* Fonts come from `FontSource`s added with `Typesetter::add_font_source`, then the built-in `FontDatabase` (fonts given as data, files or directories). Finding installed fonts through fontconfig is sile-core's default `system-fonts` feature.
* Heavy packages are default features, so a minimal user can turn them off: sile-core's `math` and `bibliography` (hayagriva), sile-pages' `index` (icu_collator) and sile-pdf's `images` (PNG decoding; JPEGs are embedded as they are without it). Library crates depend on sile-core with `default-features = false`. CI tests and lints each library crate without its optional features.

## Shaping cache

`sile-core/src/word_shaping.rs` shapes runs a word at a time from a per-font cache, for fonts whose GSUB/GPOS/kern tables it proves can't reach across a space (characters a space-sensitive rule pairs with a space are kept attached to it). Graphite and AAT fonts are always shaped whole. Debug builds also shape every run whole and assert the results are identical, so the test suite and a debug parity run check the cache.

## Language data

Patterns are SILE's own, converted to `sile-core/languages/*.pat`; localized messages are SILE's Fluent files (`*.ftl`). Both are embedded via `sile-core/src/language_data.rs`; regenerate with `scripts/import-sile-languages.py <sile checkout>`.

## Math

`sile-core/src/math` ports SILE's math package. Formulas are a typed `MathNode` tree; `TexMath` parses SILE's TeX-like syntax into it and `mathml::Element` converts MathML (the parity driver uses this for `\mathml`). `Arranger::add_math` sets a formula inline or displayed. The operator dictionary `sile-core/math/operators.txt` is generated from SILE's by `scripts/import-sile-math.py <sile checkout>`.

## Tagged PDF

`Typesetter::set_tagged` records the document's structure (`sile-core/src/structure.rs`, SILE's `pdfstructure`): every NNode and inked HBox carries the tag of the structure element it belongs to, and `sile-pdf` wraps content in marked content, untagged content being artifacts. Text opens paragraphs on its own; classes, lists, links, the TOC, images and math tag themselves. Page furniture (folios, running heads) is set inside `untagged`. Check output with veraPDF's PDF/UA-1 profile (`greenfield-apps` from Maven Central; software.verapdf.org is blocked).

## SILE parity

```sh
scripts/fetch-parity-corpus.sh          # pinned SILE tests + fonts into .parity/
cargo run --release -p sile-parity      # summary table, HTML report in target/parity/
cargo run --release -p sile-parity -- --trace italic   # our debug trace for one test
cargo run --release -p sile-parity --features sile-core/harfbuzz   # shape like SILE
```

SILE shapes Gentium Plus with Graphite, which doesn't kern, so the pure Rust shaper's kerning shows up as glyph width differences. Run with `--features sile-core/harfbuzz` for comparable widths.

Comparison is deliberately fuzzy: glyphs, line/page breaks, and positions within 0.5pt are scored separately. `sile-parity/src/driver.rs` maps the SIL subset onto `DocumentBuilder`; unsupported commands are reported, never approximated. Settled outcomes live in `sile-parity/src/known.rs`: differences that come from SILE's expected output (with the reason), tests SILE marks KNOWNBAD, and skipped features. They are reported as explained/skipped, so close/differs/unsupported lists only open work. Tests whose Lua is ported to Rust live in `sile-parity/src/ports.rs`: each port does what the Lua does through sile-core's public API, in place of the `\lua`/`\script` chunks and the commands they define.

## Leaks

`scripts/leak-check.sh [--harfbuzz] [file.md]` typesets Markdown through the CLI under Valgrind and fails on definite or indirect leaks. It is deliberately not part of CI. The operator dictionary shows up as "possibly lost"; it is a static kept for the process's life.

## CI

`.github/workflows/ci.yml` gates PRs on tests and clippy with both shapers. Parity runs too but never fails the build; its summary is on the run page and the HTML report is an artifact.

## Codebase rules

* You are a senior developer. Don't overuse comments.
