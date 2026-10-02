//! What one pass over a document learns for the next: the table of
//! contents and where labels fell (SILE's `.toc` file).

use std::collections::BTreeMap;

use crate::builder::{BuilderError, DocumentBuilder, Layout};
use crate::counter::PageNumber;

pub(crate) const TOC: &str = "toc";
pub(crate) const LABELS: &str = "labels";
pub(crate) const INDEX: &str = "index";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossReferences {
    pub toc: Vec<TocEntry>,
    pub labels: BTreeMap<String, Label>,
    /// Each index by name: the pages each term is on, in page order.
    pub index: BTreeMap<String, BTreeMap<String, Vec<IndexPage>>>,
}

impl CrossReferences {
    pub fn label(&self, name: &str) -> Option<&Label> {
        self.labels.get(name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    pub label: String,
    pub level: usize,
    pub number: Option<String>,
    /// The page number as the class shows it.
    pub page: String,
    /// Where the heading is, for links to it.
    pub dest: Option<String>,
}

/// A page an index term is on, and where on it for links.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexPage {
    pub page: PageNumber,
    pub link: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct IndexMark {
    pub index: String,
    pub label: String,
    pub link: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub page: String,
    pub value: Option<String>,
    pub dest: String,
}

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
