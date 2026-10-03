//! Pages on top of rile's typesetter: frames declared per page and
//! chained, page templates and classes, insertions such as footnotes,
//! folios and running heads, and the passes that settle page references.
//!
//! A [`DocumentBuilder`] is an `Arranger` that fills the frames of each
//! page in turn and starts a new page when they are full. Its class
//! ([`class::Plain`], [`class::Book`] or your own
//! [`DocumentClass`](class::DocumentClass)) says which frames a page has
//! and what happens as pages start and end.
//!
//! A table of contents or a cross-reference needs to know where things
//! fell, so [`lay_out_until_settled`] builds the document again with what
//! the previous pass found until nothing moves:
//!
//! ```
//! use rile::builder::{Arranger, BuilderError};
//! use rile::font::FontSpec;
//! use rile::frame::PaperSize;
//! use rile_pages::class::{Book, Heading};
//! use rile_pages::toc::{DefaultTocStyle, TableOfContents};
//! use rile_pages::{DocumentBuilder, lay_out_until_settled};
//!
//! # fn main() -> Result<(), BuilderError> {
//! let layout = lay_out_until_settled(5, |references| -> Result<DocumentBuilder, BuilderError> {
//!     let mut doc = DocumentBuilder::new(PaperSize::A5);
//! #   doc.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts"));
//!     doc.load_fonts_dir("fonts");
//!     doc.set_class(Book::new()).set_references(references);
//!     doc.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() })?;
//!     TableOfContents::default().typeset(&mut doc, &DefaultTocStyle)?;
//!     Book::chapter(&mut doc, Heading::default(), |doc: &mut DocumentBuilder| -> Result<(), BuilderError> {
//!         doc.add_text("Beginnings");
//!         Ok(())
//!     })?;
//!     doc.add_text("The chapter's first paragraph.");
//!     doc.new_paragraph()?;
//!     Ok(doc)
//! })?;
//! assert!(layout.pages.len() >= 2);
//! # Ok(())
//! # }
//! ```

pub mod bible;
pub mod class;
pub mod cropmarks;
pub mod footnotes;
pub mod framespec;
#[cfg(feature = "index")]
pub mod index;
pub mod insertion;
mod paginator;
pub mod toc;

use rile::builder::{BuilderError, Layout};
use rile::references::CrossReferences;

pub use paginator::DocumentBuilder;

/// Run on a page as it starts or ends.
pub type PageHook = Box<dyn FnMut(&mut DocumentBuilder) -> Result<(), BuilderError>>;

/// Lay a document out again until a pass finds what the one before it did,
/// or `max_passes` have run. `build` makes each pass's document, handing
/// what the previous pass found (`None` on the first) to
/// `DocumentBuilder::set_references`. A document that never asks for its
/// references is laid out once.
pub fn lay_out_until_settled<E: From<BuilderError>>(
    max_passes: usize,
    mut build: impl FnMut(Option<CrossReferences>) -> Result<DocumentBuilder, E>,
) -> Result<Layout, E> {
    let mut previous = None;
    for _ in 1..max_passes {
        let layout = build(previous.clone())?.lay_out()?;
        if !layout.consulted_references || previous.as_ref() == Some(&layout.references) {
            return Ok(layout);
        }
        previous = Some(layout.references.clone());
    }
    Ok(build(previous)?.lay_out()?)
}
