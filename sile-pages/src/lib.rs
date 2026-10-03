//! Pages on top of sile-core's typesetter: frames declared per page and
//! chained, page templates and classes, insertions such as footnotes,
//! folios and running heads, and the passes that settle page references.

pub mod bible;
pub mod class;
pub mod cropmarks;
pub mod footnotes;
pub mod framespec;
pub mod index;
pub mod insertion;
mod paginator;
pub mod toc;

use sile_core::builder::{BuilderError, Layout};
use sile_core::references::CrossReferences;

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
