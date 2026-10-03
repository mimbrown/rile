//! Tables of contents, set from the entries the previous pass found (SILE's
//! `tableofcontents` package).

use sile_core::builder::{Arranger, BuilderError, LineSkips};
use crate::DocumentBuilder;
use sile_core::builder::{bigskip, medskip, smallskip, with_font};
use sile_core::font::{FontSpec, FontWeight};
use sile_core::length::Length;
use sile_core::node::LinkDest;
use sile_core::references::TocEntry;
use sile_core::structure::Role;

type Content<'a> = &'a mut dyn FnMut(&mut DocumentBuilder) -> Result<(), BuilderError>;

/// How a table of contents looks. Each method has SILE's default.
pub trait TocStyle {
    fn header_font(&self, font: &mut FontSpec) {
        font.size = 24.0;
        font.weight = FontWeight(800);
    }

    /// Set in place of the table on the first pass.
    fn not_generated(&self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        let text = message(doc, "tableofcontents-not-generated");
        with_font(doc, |f| self.header_font(f), |doc| {
            doc.add_text(text);
            Ok(())
        })
    }

    fn header(&self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        doc.new_paragraph()?;
        doc.set_current_indent(Some(0.0));
        let text = message(doc, "tableofcontents-title");
        with_font(doc, |f| self.header_font(f), |doc| {
            doc.add_text(text);
            Ok::<_, BuilderError>(())
        })?;
        doc.add_explicit_vskip(medskip())?;
        Ok(())
    }

    fn footer(&self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        Ok(())
    }

    /// Set an entry at `level` around `content`, which sets its number,
    /// label, leaders and page.
    fn item(&self, doc: &mut DocumentBuilder, level: usize, content: Content) -> Result<(), BuilderError> {
        let (before, size, indent, after) = match level {
            1 => (Some(bigskip()), 14.0, Some(0.0), medskip()),
            2 => (None, 12.0, Some(0.0), medskip()),
            3 => (None, 10.0, None, smallskip()),
            _ => return content(doc),
        };
        if let Some(skip) = before {
            doc.add_explicit_vskip(skip)?;
        }
        doc.set_current_indent(indent);
        let weight = if level == 1 { Some(FontWeight(800)) } else { None };
        with_font(
            doc,
            |f| {
                f.size = size;
                if let Some(weight) = weight {
                    f.weight = weight;
                }
            },
            |doc| content(doc),
        )?;
        doc.add_explicit_vskip(after)?;
        Ok(())
    }

    /// Set a numbered entry's number; SILE leaves it out.
    fn number(&self, _doc: &mut DocumentBuilder, _level: usize, _number: &str) -> Result<(), BuilderError> {
        Ok(())
    }
}

/// SILE's look.
pub struct DefaultTocStyle;

impl TocStyle for DefaultTocStyle {}

/// Entries down to `depth`, linked to their headings when `linking`.
#[derive(Debug, Clone, Copy)]
pub struct TableOfContents {
    pub depth: usize,
    pub linking: bool,
}

impl Default for TableOfContents {
    fn default() -> Self {
        Self { depth: 3, linking: true }
    }
}

impl TableOfContents {
    /// Set the table found by the previous pass, or a note that there is
    /// none yet (SILE's `\tableofcontents`).
    pub fn typeset(&self, doc: &mut DocumentBuilder, style: &dyn TocStyle) -> Result<(), BuilderError> {
        let Some(references) = doc.references() else {
            return style.not_generated(doc);
        };
        let entries = references.toc.clone();
        style.header(doc)?;
        doc.with_structure(Role::TOC, |doc| {
            for entry in entries.iter().filter(|e| e.level <= self.depth) {
                let saved = doc.settings().clone();
                let skips = doc.line_skips();
                doc.set_line_skips(LineSkips { par_fill: Length::zero(), ..skips });
                let result = doc.with_structure(Role::TOCI, |doc| style.item(doc, entry.level, &mut |doc| self.entry(doc, style, entry)));
                doc.restore_settings(saved);
                result?;
            }
            Ok(())
        })?;
        style.footer(doc)
    }

    fn entry(&self, doc: &mut DocumentBuilder, style: &dyn TocStyle, entry: &TocEntry) -> Result<(), BuilderError> {
        let dest = entry.dest.as_ref().filter(|_| self.linking);
        if let Some(dest) = dest {
            doc.start_link(LinkDest::Internal(dest.clone()));
        }
        if let Some(number) = &entry.number {
            style.number(doc, entry.level, number)?;
        }
        doc.add_text(entry.label.clone());
        doc.add_dotfill();
        doc.add_text(entry.page.clone());
        if dest.is_some() {
            doc.end_hbox();
        }
        Ok(())
    }
}

fn message(doc: &DocumentBuilder, id: &str) -> String {
    doc.message(id, &[]).unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::*;
    use crate::class::{Book, Heading};
    use crate::lay_out_until_settled;
    use sile_core::references::CrossReferences;

    fn book(references: Option<CrossReferences>) -> Result<DocumentBuilder, BuilderError> {
        let mut d = doc(Book::new());
        d.set_references(references);
        TableOfContents::default().typeset(&mut d, &DefaultTocStyle)?;
        for chapter in ["One", "Two"] {
            Book::chapter(&mut d, Heading::default(), |d: &mut DocumentBuilder| {
                d.add_text(chapter);
                Ok::<_, BuilderError>(())
            })?;
            Book::section(&mut d, Heading::default(), |d: &mut DocumentBuilder| {
                d.add_text("Start");
                Ok::<_, BuilderError>(())
            })?;
            d.add_label(chapter, None).add_text("Text.");
        }
        Ok(d)
    }

    #[test]
    fn the_first_pass_asks_for_another() {
        let pages = book(None).unwrap().into_pages().unwrap();
        assert!(text_in(&pages[0], "content").starts_with("RerunSILE"));
    }

    #[test]
    fn later_passes_list_headings_with_their_pages() {
        let layout = lay_out_until_settled(3, book).unwrap();
        let toc: Vec<_> = layout
            .references
            .toc
            .iter()
            .map(|e| (e.label.as_str(), e.level, e.number.as_deref(), e.page.as_str()))
            .collect();
        assert_eq!(toc, [("One", 1, Some("1"), "3"), ("Start", 2, Some("1.1"), "3"), ("Two", 1, Some("2"), "5"), ("Start", 2, Some("2.1"), "5")]);
        assert_eq!(text_in(&layout.pages[0], "content"), "TableofContentsOne3Start3Two5Start5");
        assert_eq!(layout.references.label("Two").map(|l| l.page.as_str()), Some("5"));
    }
}
