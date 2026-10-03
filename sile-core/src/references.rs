//! What one pass over a document learns for the next: the table of
//! contents and where labels fell (SILE's `.toc` file).

use std::collections::BTreeMap;

use crate::counter::PageNumber;

pub const TOC: &str = "toc";
pub const LABELS: &str = "labels";
pub const INDEX: &str = "index";

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
pub struct IndexMark {
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
