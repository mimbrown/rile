//! What a document says about itself, for outputs to record.

use crate::frame::PaperSize;

/// Document information: title, author, subject and other entries, such
/// as `Keywords` or `CreationDate`.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub info: Vec<(String, String)>,
    /// The sheet pages are printed on, when bigger than the page: each
    /// page is centred on it.
    pub sheet: Option<PaperSize>,
}

/// An entry in the document outline, opening at a named destination.
/// Entries nest under the closest earlier one with a lower level.
#[derive(Debug, Clone)]
pub struct Bookmark {
    pub title: String,
    pub level: u32,
    pub dest: String,
}
