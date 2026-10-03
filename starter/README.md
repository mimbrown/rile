# rile starter

A short book typeset from Rust: a table of contents, a chapter with a footnote,
and a section with a list. The list is written once, against `Arranger`, and is
also set on its own in a galley and written as SVG.

```sh
cargo run -p rile-starter -- path/to/fonts
```

writes `starter.pdf` and `points.svg`. The text is set in Gentium Plus, found
among the installed fonts or in the directory given (`../sile-parity/fonts` has
it).

To start a project of your own, copy this directory and point the dependencies
at the repository instead of the sibling directories:

```toml
rile = { git = "https://github.com/mimbrown/sile-rust" }
rile-pages = { git = "https://github.com/mimbrown/sile-rust" }
rile-pdf = { git = "https://github.com/mimbrown/sile-rust" }
rile-svg = { git = "https://github.com/mimbrown/sile-rust" }
```
