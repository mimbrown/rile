//! A typesetting engine: fonts and shaping, hyphenation, Knuth–Plass line
//! breaking, bidi, lists, tables, math and tagged structure, laid out into a
//! [`Layout`](builder::Layout) that knows nothing about input syntax or
//! output formats.
//!
//! # How the pieces fit
//!
//! A [`Typesetter`](builder::Typesetter) holds the settings in scope (font,
//! language, spacing, alignment) and turns text into lines. Where those
//! lines go is up to an [`Arranger`](builder::Arranger), which provides the
//! vertical-mode commands such as `new_paragraph`, `add_vskip` and
//! `begin_list`:
//!
//! - [`Galley`](builder::Galley) keeps everything on one surface, as tall
//!   as its material, with paragraphs broken to a measure or set at their
//!   natural width. `take_frame` cuts off what fits a given height and
//!   keeps the rest.
//! - `rile_pages::DocumentBuilder` pages it, with frames, page templates,
//!   classes, footnotes and running heads.
//!
//! Content written against `Arranger`, or against [`Context`](builder::Context)
//! when it takes callbacks, works with either. The finished `Layout` is
//! drawn by `rile_pdf` or `rile_svg`, or by your own
//! [`Canvas`](render::Canvas).
//!
//! # Example
//!
//! ```
//! use rile::builder::{Arranger, Galley};
//! use rile::font::FontSpec;
//!
//! # fn main() -> Result<(), rile::builder::BuilderError> {
//! let mut galley = Galley::new(Some(300.0));
//! # galley.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts"));
//! galley.load_fonts_dir("fonts");
//! galley.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 11.0, ..Default::default() })?;
//! galley.add_text("Paragraphs are broken to the 300pt measure and stacked as far down as they need to go.");
//! galley.new_paragraph()?;
//! let layout = galley.lay_out()?;
//! assert_eq!(layout.pages.len(), 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Fonts
//!
//! Fonts are found by family or filename through the
//! [`FontSource`](font::FontSource)s added with `add_font_source`, then the
//! built-in database of fonts given as data (`add_font`), files or
//! directories. The default `system-fonts` feature adds the fonts installed
//! on the system (`load_system_fonts`).
//!
//! # Features
//!
//! - `system-fonts` (default): find installed fonts through fontconfig.
//! - `math` (default): formulas from a typed tree, TeX-like syntax or MathML.
//! - `bibliography` (default): citations from BibTeX through CSL styles.
//! - `harfbuzz`: shape with the system HarfBuzz instead of rustybuzz, for
//!   Graphite fonts.

#[cfg(feature = "bibliography")]
pub mod bibliography;
pub mod builder;
pub mod chords;
pub mod color;
pub mod counter;
pub mod date;
pub mod dropcap;
pub mod features;
pub mod font;
pub mod frame;
pub mod hyphenation;
pub mod image;
mod language_data;
pub mod messages;
pub mod length;
pub mod linebreak;
pub mod lists;
#[cfg(feature = "math")]
pub mod math;
pub mod ruby;
pub mod measurement;
pub mod node;
pub mod nodemaker;
pub mod pagebuilder;
pub mod metadata;
pub mod pullquote;
pub mod references;
pub mod render;
pub mod shaper;
pub mod structure;
pub mod svg;
pub mod svg_image;
pub mod table;
pub mod textcase;
pub mod url;
mod word_shaping;
pub mod trace;
pub mod transform;
#[cfg(test)]
mod test_support;

#[cfg(feature = "harfbuzz")]
mod harfbuzz_ffi;
#[cfg(feature = "harfbuzz")]
pub mod shaper_harfbuzz;
