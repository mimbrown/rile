# rile

rile is a typesetting engine written in Rust. It began as a port of
[SILE](https://sile-typesetter.org) and keeps its typography: HarfBuzz-compatible
shaping, Liang hyphenation for dozens of languages, Knuth–Plass line breaking,
bidirectional and vertical text, OpenType math, and tagged, accessible PDF.
Documents are written in Rust, or in Markdown through the CLI.

## Crates

| Crate | What it does |
|---|---|
| `rile` | The engine: fonts, settings, paragraphs, lists, tables, math, structure. Lays content out into a `Layout` that knows nothing about input or output formats. |
| `rile-pages` | Pages: frames, page templates, classes (plain, book and others), footnotes, running heads, tables of contents and indexes. |
| `rile-pdf` | Writes a `Layout` as PDF, tagged for PDF/UA when the document is. |
| `rile-svg` | Writes a `Layout` as SVG, one document per page. |
| `rile-markdown` | Sets CommonMark, with GitHub's tables, footnotes and task lists and `$` math, into pages or a galley. |
| `rile-cli` | The `rile` command: Markdown to PDF or SVG. |
| `sile-parity` | Test tool that runs SILE's regression tests through rile and scores the layouts against SILE's. |

## The command

```sh
cargo run --release -p rile-cli -- notes.md                    # notes.pdf on A4
cargo run --release -p rile-cli -- --class book --paper a5 --toc notes.md
cargo run --release -p rile-cli -- --width 400 notes.md -o notes.svg   # one surface, no pages
```

`rile --help` lists the font, language and metadata options.

## The library

A `Typesetter` holds settings and turns text into lines. An `Arranger` decides
where the lines go: `Galley` keeps them on one surface as tall as the text, and
`rile_pages::DocumentBuilder` pages them. Content written against `Arranger`
works with both.

```rust
use rile::builder::{Arranger, Galley};
use rile::font::FontSpec;

let mut galley = Galley::new(Some(300.0));
galley.load_fonts_dir("fonts");
galley.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 11.0, ..Default::default() })?;
galley.add_text("Broken to a 300pt measure, with no pages in sight.");
galley.new_paragraph()?;
let svg = rile_svg::render(&galley.lay_out()?);
```

[`starter/`](starter) is a small book built this way, with chapters, a
footnote, a list and a table of contents, written to PDF.

## Development

`cargo test --workspace` runs the tests; add `--features rile/harfbuzz` to
shape with the system HarfBuzz. `CLAUDE.md` describes the architecture and the
SILE parity tooling.
