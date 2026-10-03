//! Indexes of terms with the pages they are on, set from what the previous
//! pass found (SILE's `indexer` package).

use icu_collator::options::CollatorOptions;
use icu_collator::Collator;
use icu_locale::Locale;

use crate::builder::{Arranger, BuilderError, DocumentBuilder, LineSkips};
use crate::class::{bigskip, smallskip};
use crate::length::Length;
use crate::node::LinkDest;
use crate::references::IndexPage;

/// How page ranges are written (SILE's `page-range-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRanges {
    /// Every page listed: 42, 43, 44.
    None,
    /// 42–45, 321–328.
    Expanded,
    /// Digits the second number repeats left out: 42–5, 321–8.
    Minimal,
    /// As `Minimal`, but keeping two digits: 42–45, 321–28.
    MinimalTwo,
}

/// What separates a term from its pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filler {
    Dotfill,
    Fill,
    Comma,
}

/// How an index looks. Each method has SILE's default.
pub trait IndexStyle {
    fn entry(&self, doc: &mut DocumentBuilder, _index: &str, label: &str) -> Result<(), BuilderError> {
        doc.add_text(label);
        Ok(())
    }

    fn page(&self, doc: &mut DocumentBuilder, _index: &str, page: &str) -> Result<(), BuilderError> {
        doc.add_text(page);
        Ok(())
    }
}

pub struct DefaultIndexStyle;

impl IndexStyle for DefaultIndexStyle {}

#[derive(Debug, Clone)]
pub struct Indexer {
    pub page_ranges: PageRanges,
    pub range_delimiter: String,
    pub page_delimiter: String,
    pub filler: Filler,
}

impl Default for Indexer {
    fn default() -> Self {
        Self { page_ranges: PageRanges::Expanded, range_delimiter: "–".into(), page_delimiter: ", ".into(), filler: Filler::Dotfill }
    }
}

impl Indexer {
    /// Set the index called `index` as the previous pass found it, its
    /// terms sorted for the document's language (SILE's `\printindex`).
    /// The first pass sets nothing.
    pub fn typeset(&self, doc: &mut DocumentBuilder, index: &str, style: &dyn IndexStyle) -> Result<(), BuilderError> {
        let Some(terms) = doc.references().and_then(|r| r.index.get(index)).cloned() else {
            return Ok(());
        };
        let mut labels: Vec<&String> = terms.keys().collect();
        sort_collated(&mut labels, doc.language());
        doc.add_explicit_vskip(bigskip())?;
        for label in labels {
            let saved = doc.settings().clone();
            let result = self.entry(doc, index, label, &terms[label], style);
            doc.restore_settings(saved);
            result?;
        }
        Ok(())
    }

    fn entry(&self, doc: &mut DocumentBuilder, index: &str, label: &str, pages: &[IndexPage], style: &dyn IndexStyle) -> Result<(), BuilderError> {
        if self.filler != Filler::Comma {
            let skips = doc.line_skips();
            doc.set_line_skips(LineSkips { par_fill: Length::zero(), ..skips });
        }
        doc.set_current_indent(Some(0.0));
        style.entry(doc, index, label)?;
        match self.filler {
            Filler::Dotfill => doc.add_dotfill(),
            Filler::Fill => doc.add_hss(),
            Filler::Comma => doc.add_text(", "),
        };
        for (i, item) in self.pages(pages).into_iter().enumerate() {
            if i > 0 {
                doc.add_text(self.page_delimiter.clone());
            }
            match item {
                Item::Page(text, link) => page(doc, index, &text, link, style)?,
                Item::Range((from, from_link), (to, to_link)) => {
                    page(doc, index, &from, from_link, style)?;
                    doc.add_text(self.range_delimiter.clone());
                    page(doc, index, &to, to_link, style)?;
                }
            }
        }
        doc.add_explicit_vskip(smallskip())?;
        Ok(())
    }

    fn pages<'a>(&self, pages: &'a [IndexPage]) -> Vec<Item<'a>> {
        let one = |p: &'a IndexPage| (p.page.to_string(), p.link.as_deref());
        if self.page_ranges == PageRanges::None {
            return pages.iter().map(|p| Item::Page(one(p).0, one(p).1)).collect();
        }
        let mut groups: Vec<Vec<&IndexPage>> = Vec::new();
        for p in pages {
            match groups.last_mut() {
                Some(g) if g.last().is_some_and(|l| l.page.display == p.page.display && l.page.value + 1 == p.page.value) => g.push(p),
                _ => groups.push(vec![p]),
            }
        }
        groups
            .into_iter()
            .map(|g| match g.as_slice() {
                [p] => Item::Page(one(p).0, one(p).1),
                [first, .., last] => {
                    let (from, mut to) = (one(first), one(last));
                    if self.page_ranges != PageRanges::Expanded && first.page.display == "arabic" {
                        to.0 = simplify(&from.0, &to.0, self.page_ranges);
                    }
                    Item::Range(from, to)
                }
                [] => unreachable!(),
            })
            .collect()
    }
}

enum Item<'a> {
    Page(String, Option<&'a str>),
    Range((String, Option<&'a str>), (String, Option<&'a str>)),
}

fn page(doc: &mut DocumentBuilder, index: &str, text: &str, link: Option<&str>, style: &dyn IndexStyle) -> Result<(), BuilderError> {
    if let Some(link) = link {
        doc.start_link(LinkDest::Internal(link.to_string()));
    }
    style.page(doc, index, text)?;
    if link.is_some() {
        doc.end_hbox();
    }
    Ok(())
}

/// The end of a range with the digits it shares with its start left out.
fn simplify(from: &str, to: &str, format: PageRanges) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (from.chars().collect(), to.chars().collect());
    if a.len() > 1 && a.len() == b.len() {
        let keep = if format == PageRanges::Minimal { 1 } else { 2 };
        for i in 0..a.len() - keep {
            if a[i] != b[i] {
                return b[i..].iter().collect();
            }
        }
        return b[a.len() - keep..].iter().collect();
    }
    to.to_string()
}

/// Sort `items` in the order `language` sorts words in (SILE's
/// `SU.collatedSort`).
pub fn sort_collated<S: AsRef<str>>(items: &mut [S], language: &str) {
    let locale = Locale::try_from_str(language).unwrap_or(Locale::UNKNOWN);
    match Collator::try_new((&locale).into(), CollatorOptions::default()) {
        Ok(collator) => items.sort_by(|a, b| collator.compare(a.as_ref(), b.as_ref())),
        Err(_) => items.sort_by(|a, b| a.as_ref().cmp(b.as_ref())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::*;
    use crate::class::Plain;
    use crate::references::{lay_out_until_settled, CrossReferences};

    fn document(references: Option<CrossReferences>, indexer: &Indexer) -> Result<DocumentBuilder, BuilderError> {
        let mut d = doc(Plain::new());
        d.set_references(references);
        for page in 1..=5 {
            if page != 4 {
                d.add_index_entry("main", "term");
            }
            if page == 2 {
                d.add_index_entry("main", "Äpfel");
            }
            d.add_text(format!("Page {page}."));
            d.supereject()?;
        }
        d.add_index_entry("main", "apple");
        indexer.typeset(&mut d, "main", &DefaultIndexStyle)?;
        Ok(d)
    }

    fn index_lines(indexer: Indexer) -> Vec<String> {
        let layout = lay_out_until_settled(5, |r| document(r, &indexer)).unwrap();
        let last = layout.pages.last().unwrap();
        let mut lines = Vec::new();
        for (_, nodes) in &last.content {
            for node in nodes {
                if let crate::node::Node::VBox(v) = node {
                    let mut text = String::new();
                    collect(&v.nodes, &mut text);
                    lines.push(text);
                }
            }
        }
        lines
    }

    fn collect(nodes: &[crate::node::Node], out: &mut String) {
        for n in nodes {
            match n {
                crate::node::Node::NNode(n) => out.push_str(&n.text),
                crate::node::Node::HBox(b) => collect(&b.nodes, out),
                _ => {}
            }
        }
    }

    #[test]
    fn terms_are_sorted_with_their_page_ranges() {
        let lines = index_lines(Indexer::default());
        let entries: Vec<String> = lines.iter().map(|l| l.replace('.', "")).filter(|l| l.len() > 1).collect();
        assert_eq!(entries, ["Äpfel2", "apple6", "term1–3,5"]);
        let none = index_lines(Indexer { page_ranges: PageRanges::None, ..Default::default() });
        assert!(none.iter().any(|l| l.replace('.', "") == "term1,2,3,5"));
    }

    #[test]
    fn ranges_can_leave_out_repeated_digits() {
        assert_eq!(simplify("321", "328", PageRanges::Minimal), "8");
        assert_eq!(simplify("321", "328", PageRanges::MinimalTwo), "28");
        assert_eq!(simplify("2787", "2816", PageRanges::Minimal), "816");
        assert_eq!(simplify("42", "45", PageRanges::MinimalTwo), "45");
        assert_eq!(simplify("98", "102", PageRanges::Minimal), "102");
    }

    #[test]
    fn sorting_follows_the_language() {
        let mut words = vec!["zebra", "apple", "Zoo", "Äpfel"];
        sort_collated(&mut words, "de");
        assert_eq!(words, ["Äpfel", "apple", "zebra", "Zoo"]);
        let mut words = vec!["ö", "z"];
        sort_collated(&mut words, "sv");
        assert_eq!(words, ["z", "ö"]);
    }
}
