//! Versified text (SILE's `bible` class and `chapterverse` package).

use std::sync::Arc;

use crate::builder::{BuilderError, DocumentBuilder, LineSkips};
use crate::class::{Book, DocumentClass, PageTemplate};
use crate::length::Length;

/// Where a verse starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub book: Option<String>,
    pub chapter: Option<String>,
    pub verse: String,
}

pub type ReferenceFormatter = Arc<dyn Fn(&mut DocumentBuilder, &Reference) -> Result<(), BuilderError> + Send + Sync>;

const REFERENCES: &str = "references";

/// A two-sided book whose running heads show the first verse on left pages
/// and the last on right pages.
pub struct Bible {
    pub book: Book,
    /// Sets a reference in running heads; `Book c:v` by default.
    pub format_reference: ReferenceFormatter,
    title: Option<String>,
    chapter: Option<String>,
    has_heads: bool,
}

impl Default for Bible {
    fn default() -> Self {
        Self::new()
    }
}

impl Bible {
    pub fn new() -> Self {
        Self {
            book: Book::new(),
            format_reference: Arc::new(|doc, r| {
                let reference = format!("{}:{}", r.chapter.as_deref().unwrap_or(""), r.verse);
                match &r.book {
                    Some(book) => doc.add_text(format!("{book} {reference}")),
                    None => doc.add_text(reference),
                };
                Ok(())
            }),
            title: None,
            chapter: None,
            has_heads: false,
        }
    }

    pub fn save_book_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    pub fn save_chapter_number(&mut self, chapter: impl Into<String>) {
        self.chapter = Some(chapter.into());
    }

    /// Start verse `number`: the reference is noted on the page for the
    /// running heads. The number itself is the caller's to set.
    pub fn verse_number(doc: &mut DocumentBuilder, number: &str) -> Result<(), BuilderError> {
        let indent = doc.paragraph_indent();
        doc.set_current_indent(Some(indent));
        let bible = doc.class_mut::<Bible>().ok_or_else(|| BuilderError::Layout("not a bible".into()))?;
        bible.has_heads = true;
        let reference = Reference { book: bible.title.clone(), chapter: bible.chapter.clone(), verse: number.to_string() };
        doc.add_info(REFERENCES, reference);
        Ok(())
    }

    pub fn first_reference(&self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        match doc.page_info::<Reference>(REFERENCES).first() {
            Some(r) => (self.format_reference)(doc, r),
            None => Ok(()),
        }
    }

    pub fn last_reference(&self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        match doc.page_info::<Reference>(REFERENCES).last() {
            Some(r) => (self.format_reference)(doc, r),
            None => Ok(()),
        }
    }

    fn running_head(&self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        let odd = self.book.odd_page();
        doc.use_toplevel();
        let skips = LineSkips { left: Length::zero(), right: Length::zero(), ..doc.line_skips() };
        doc.set_line_skips(skips).set_current_indent(Some(0.0));
        if odd {
            doc.set_line_skips(LineSkips { par_fill: Length::zero(), ..skips });
        }
        // SILE also asks for "Gentium", its old default family, which no
        // longer exists; heads keep the document's family instead.
        doc.update_font(|f| f.size = 10.0)?;
        if odd {
            doc.add_hfill();
            self.last_reference(doc)?;
        } else {
            self.first_reference(doc)?;
            doc.add_hfill();
        }
        doc.new_paragraph()?;
        Ok(())
    }
}

impl DocumentClass for Bible {
    fn page_template(&self) -> PageTemplate {
        self.book.page_template()
    }

    fn new_page(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        self.book.new_page(doc)
    }

    fn end_page(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        if self.has_heads {
            doc.typeset_into("runningHead", |d| self.running_head(d))?;
        }
        self.book.folio.output(doc)
    }

    fn folio(&self) -> Option<String> {
        self.book.folio()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::*;

    #[test]
    fn heads_show_the_first_verse_on_left_pages_and_the_last_on_right() {
        let mut d = doc(Bible::new());
        let bible = d.class_mut::<Bible>().unwrap();
        bible.save_book_title("Gn");
        bible.save_chapter_number("1");
        for (verse, text) in [("1", "In the beginning"), ("2", "And the earth")] {
            Bible::verse_number(&mut d, verse).unwrap();
            d.add_text(text);
        }
        d.supereject().unwrap();
        for (verse, text) in [("3", "And God said"), ("4", "And God saw")] {
            Bible::verse_number(&mut d, verse).unwrap();
            d.add_text(text);
        }
        let pages = d.into_pages().unwrap();
        assert_eq!(text_in(&pages[0], "runningHead"), "Gn1:2");
        assert_eq!(text_in(&pages[1], "runningHead"), "Gn1:3");
    }
}
