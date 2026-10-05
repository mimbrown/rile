# rile

rile is a Rust typesetting engine that began as a port of SILE (whose source lives in `../sile`). Layout knows nothing about input syntax, pages or output formats; those are separate crates on its public API.

The public API is described in its own terms. Where code ports SILE, a `// SILE: ...` comment after the doc comment names what it ports; SILE names otherwise appear only in internal comments and the parity crate.

## Current crates

* `rile`: contains the core types and logic; lays documents out into a format-neutral `Layout` (pages, fonts, outline, metadata, structure)
* `rile-pages`: pages on top of rile, through its public API only: `DocumentBuilder` (frames, templates, classes, insertions and footnotes, page breaking, parallel flows), frame declarations (`framespec`), folios, cropmarks, TOC, index and `lay_out_until_settled`
* `rile-pdf`: writes a `Layout` as PDF (`rile_pdf::render`), with `PdfOptions` for how it is written
* `rile-svg`: writes a `Layout` as SVG, one document per page (`rile_svg::render`); the CLI uses it for `.svg` outputs
* `rile-markdown`: sets Markdown (CommonMark plus GitHub's tables, footnotes, strikethrough and task lists, with `$`/`$$` math) into any arranger. `MarkdownTarget` lets an arranger set headings and footnotes itself: `DocumentBuilder` (feature `pages`) uses page footnotes and book sectioning; elsewhere headings are plain and notes are endnotes
* `rile-cli`: the `rile` command, which typesets Markdown to tagged, accessible PDF or SVG, on pages or (`--width`) in a galley
* `starter`: a small book typeset from Rust, the example for new projects; CI builds it
* `sile-parity`: test-only tool that runs SILE's regression tests through `rile` and compares layouts with SILE's expected output

## Typesetter, arrangers and galleys

`rile::builder::Typesetter` turns text and settings into lines and a vertical list without knowing about pages: lines are set to its `FrameContext` (measure, direction, tate). Vertical-mode commands (`new_paragraph`, `add_vskip`, `add_rule`, ...) are provided methods of the `Arranger` trait, whose implementors say where the vertical list goes:

* `rile_pages::DocumentBuilder` (`rile-pages/src/paginator.rs`) pages it. It derefs to its typesetter and owns everything that knows about pages: frames, templates, classes, insertions, page breaking, parallel flows and references. It calls `sync_frame` whenever the frame being filled changes.
* `Galley` (`builder/galley.rs`) keeps it: no measure sets paragraphs at natural width, a measure breaks them, and `take_frame` cuts off what fits a height, leaving the overflow. `lay_out` gives one surface as tall as the material.

Settings are scoped (setters plus `settings`/`restore_settings`). The public styling layer is `builder/style.rs`: `TextStyle` and `ParagraphStyle` (unset fields inherit), applied by `span`/`paragraph` (free functions over `Context`, and `Arranger` methods), which save the settings, apply the style and restore them (Michael chose this hybrid over replacing the setters).

Content that needs vertical mode (lists, tables, display math, specimens) is written once as `Arranger` methods, and content that takes callbacks is generic over `Context`, which an arranger or a frontend's state around one implements. Anything rile-pages needs from the typesetter goes through rile's public API; footnotes, classes, the TOC and the index stay `DocumentBuilder`-only.

## Features

* Default builds are pure Rust (rustybuzz). `--features harfbuzz` links system HarfBuzz, needed for Graphite fonts.
* Fonts come from `FontSource`s added with `Typesetter::add_font_source`, then the built-in `FontDatabase` (fonts given as data, files or directories). Finding installed fonts through fontconfig is rile's default `system-fonts` feature.
* Heavy packages are default features, so a minimal user can turn them off: rile's `math` and `bibliography` (hayagriva), rile-pages' `index` (icu_collator) and rile-pdf's `images` (PNG decoding; JPEGs are embedded as they are without it). Library crates depend on rile with `default-features = false`. CI tests and lints each library crate without its optional features.

## Shaping cache

`rile/src/word_shaping.rs` shapes runs a word at a time from a per-font cache, for fonts whose GSUB/GPOS/kern tables it proves can't reach across a space (characters a space-sensitive rule pairs with a space are kept attached to it). Graphite and AAT fonts are always shaped whole. Debug builds also shape every run whole and assert the results are identical, so the test suite and a debug parity run check the cache.

## Language data

Patterns are SILE's own, converted to `rile/languages/*.pat`; localized messages are SILE's Fluent files (`*.ftl`). Both are embedded via `rile/src/language_data.rs`; regenerate with `scripts/import-sile-languages.py <sile checkout>`.

## Math

`rile/src/math` ports SILE's math package. Formulas are a typed `MathNode` tree; `TexMath` parses SILE's TeX-like syntax into it and `mathml::Element` converts MathML (the parity driver uses this for `\mathml`). `Arranger::add_math` sets a formula inline or displayed. The operator dictionary `rile/math/operators.txt` is generated from SILE's by `scripts/import-sile-math.py <sile checkout>`.

## Tagged PDF

`Typesetter::set_tagged` records the document's structure (`rile/src/structure.rs`, SILE's `pdfstructure`): every NNode and inked HBox carries the tag of the structure element it belongs to, and `rile-pdf` wraps content in marked content, untagged content being artifacts. Text opens paragraphs on its own; classes, lists, links, the TOC, images and math tag themselves. Page furniture (folios, running heads) is set inside `untagged`. Check output with veraPDF's PDF/UA-1 profile (`greenfield-apps` from Maven Central; software.verapdf.org is blocked).

## SILE parity

```sh
scripts/fetch-parity-corpus.sh          # pinned SILE tests + fonts into .parity/
cargo run --release -p sile-parity      # summary table, HTML report in target/parity/
cargo run --release -p sile-parity -- --trace italic   # our debug trace for one test
cargo run --release -p sile-parity --features rile/harfbuzz   # shape like SILE
```

SILE shapes Gentium Plus with Graphite, which doesn't kern, so the pure Rust shaper's kerning shows up as glyph width differences. Run with `--features rile/harfbuzz` for comparable widths.

Comparison is deliberately fuzzy: glyphs, line/page breaks, and positions within 0.5pt are scored separately. `sile-parity/src/driver.rs` maps the SIL subset onto `DocumentBuilder`; unsupported commands are reported, never approximated. Settled outcomes live in `sile-parity/src/known.rs`: differences that come from SILE's expected output (with the reason), tests SILE marks KNOWNBAD, and skipped features. They are reported as explained/skipped, so close/differs/unsupported lists only open work. Tests whose Lua is ported to Rust live in `sile-parity/src/ports.rs`: each port does what the Lua does through rile's public API, in place of the `\lua`/`\script` chunks and the commands they define.

## Leaks

`scripts/leak-check.sh [--harfbuzz] [file.md]` typesets Markdown through the CLI under Valgrind and fails on definite or indirect leaks. It is deliberately not part of CI. The operator dictionary shows up as "possibly lost"; it is a static kept for the process's life.

## CI

`.github/workflows/ci.yml` gates PRs on tests and clippy with both shapers. Parity runs too but never fails the build; its summary is on the run page and the HTML report is an artifact.

## Codebase rules

* You are a senior developer. Don't overuse comments.
