//! Citations and bibliographies from BibTeX files, formatted by CSL styles
//! (SILE's `bibtex` package). Formatting is hayagriva's CSL processor.

use hayagriva::archive::{locales, ArchivedStyle};
use hayagriva::citationberg::taxonomy::Locator;
use hayagriva::citationberg::{
    Display, FontStyle, FontVariant, FontWeight, IndependentStyle, Locale, LocaleCode, Style, VerticalAlign,
};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, CitationItem, CitationRequest, ElemChild, ElemChildren, Entry, Formatting,
    Library, LocatorPayload, Rendered, SpecificLocator,
};

use crate::builder::{Arranger, BuilderError, LineSkips};
use crate::font::{FontStyle as Style_, FontWeight as Weight};
use crate::length::Length;
use crate::node::LinkDest;

/// One work cited, with where in it, as in `page` and `42–45`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cite {
    pub key: String,
    pub locator: Option<(String, String)>,
}

impl Cite {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into(), locator: None }
    }
}

/// The works a document can cite, how citations look, and what it has
/// cited so far, in order.
pub struct Bibliography {
    library: Library,
    style: IndependentStyle,
    locales: Vec<Locale>,
    locale: Option<LocaleCode>,
    citations: Vec<Vec<Cite>>,
    /// Indentation of the lines after the first when the style hangs them
    /// (SILE's `bibliography.indent`); 3em when `None`.
    pub indent: Option<f64>,
}

impl Default for Bibliography {
    fn default() -> Self {
        let Style::Independent(style) = ArchivedStyle::ChicagoAuthorDate.get() else { unreachable!() };
        Self { library: Library::new(), style, locales: locales(), locale: None, citations: Vec::new(), indent: None }
    }
}

/// The CSL locator a `\cite` option names, allowing SILE's abbreviations.
pub fn locator_label(option: &str) -> &str {
    match option {
        "app" => "appendix",
        "article" | "art" => "article-locator",
        "ch" | "chap" => "chapter",
        "col" => "column",
        "fig" => "figure",
        "fol" => "folio",
        "svv" => "sub-verbo",
        "title" => "title-locator",
        "vol" => "volume",
        other => other,
    }
}

fn error(what: impl std::fmt::Display) -> BuilderError {
    BuilderError::Layout(format!("bibliography: {what}"))
}

impl Bibliography {
    /// Add the entries in BibTeX or BibLaTeX source `bib` (SILE's
    /// `\loadbibliography`).
    pub fn load_bibtex(&mut self, bib: &str) -> Result<&mut Self, BuilderError> {
        let library = hayagriva::io::from_biblatex_str(bib).map_err(|e| error(format!("{e:?}")))?;
        for entry in library.iter() {
            self.library.push(entry);
        }
        Ok(self)
    }

    /// Format with a style hayagriva ships, by name or CSL id, such as
    /// `chicago-author-date` or `apa`, in `lang` when given (SILE's
    /// `\bibliographystyle`).
    pub fn set_style(&mut self, name: &str, lang: Option<&str>) -> Result<&mut Self, BuilderError> {
        let style = ArchivedStyle::by_name(name).ok_or_else(|| error(format!("no style {name}")))?;
        let Style::Independent(style) = style.get() else { return Err(error(format!("{name} is not an independent style"))) };
        self.style = style;
        self.locale = lang.map(|l| LocaleCode(l.to_string()));
        Ok(self)
    }

    /// Format with the CSL style in `xml`, in `lang` when given.
    pub fn set_csl_style(&mut self, xml: &str, lang: Option<&str>) -> Result<&mut Self, BuilderError> {
        match Style::from_xml(xml).map_err(error)? {
            Style::Independent(style) => self.style = style,
            Style::Dependent(_) => return Err(error("dependent CSL styles need their parent")),
        }
        self.locale = lang.map(|l| LocaleCode(l.to_string()));
        Ok(self)
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.library.get(key)
    }

    fn items<'a>(&'a self, cites: &'a [Cite]) -> Result<Vec<CitationItem<'a, Entry>>, BuilderError> {
        cites
            .iter()
            .map(|cite| {
                let entry = self.entry(&cite.key).ok_or_else(|| error(format!("no entry {}", cite.key)))?;
                let locator = match &cite.locator {
                    Some((kind, value)) => {
                        let kind: Locator = locator_label(kind).parse().map_err(|_| error(format!("no locator {kind}")))?;
                        Some(SpecificLocator(kind, LocatorPayload::Str(value)))
                    }
                    None => None,
                };
                Ok(CitationItem::with_locator(entry, locator))
            })
            .collect()
    }

    /// Format every citation so far plus `extra` and the bibliography of
    /// the works they cite, in order.
    fn render(&self, extra: &[Vec<CitationItem<'_, Entry>>]) -> Result<Rendered, BuilderError> {
        let mut driver = BibliographyDriver::new();
        let mut all = Vec::new();
        for cites in &self.citations {
            all.push(self.items(cites)?);
        }
        all.extend(extra.iter().cloned());
        for items in all {
            driver.citation(CitationRequest::new(items, &self.style, self.locale.clone(), &self.locales, None));
        }
        Ok(driver.finish(BibliographyRequest::new(&self.style, self.locale.clone(), &self.locales)))
    }

    /// Cite `cites` together, setting the citation the style makes of them
    /// (SILE's `\cite` and `\cites`).
    pub fn cite<A: Arranger>(&mut self, doc: &mut A, cites: Vec<Cite>) -> Result<(), BuilderError> {
        self.items(&cites)?;
        self.citations.push(cites);
        let rendered = self.render(&[])?;
        let citation = rendered.citations.last().map(|c| c.citation.clone()).unwrap_or_else(|| ElemChildren(Vec::new()));
        self.set(doc, &citation)
    }

    /// Count `key` as cited without citing it here (SILE's `\nocite`).
    pub fn nocite(&mut self, key: &str) -> Result<(), BuilderError> {
        self.entry(key).ok_or_else(|| error(format!("no entry {key}")))?;
        self.citations.push(vec![Cite::new(key)]);
        Ok(())
    }

    /// Set the bibliography entry for `key` here (SILE's `\reference`).
    pub fn reference<A: Arranger>(&self, doc: &mut A, key: &str) -> Result<(), BuilderError> {
        let entry = self.entry(key).ok_or_else(|| error(format!("no entry {key}")))?;
        let mut driver = BibliographyDriver::new();
        driver.citation(CitationRequest::new(vec![CitationItem::with_entry(entry)], &self.style, self.locale.clone(), &self.locales, None));
        let rendered = driver.finish(BibliographyRequest::new(&self.style, self.locale.clone(), &self.locales));
        match rendered.bibliography.and_then(|b| b.items.into_iter().next()) {
            Some(item) => self.set(doc, &item.content),
            None => Ok(()),
        }
    }

    /// Set the bibliography of the works cited so far, or of every work
    /// loaded unless `cited_only` (SILE's `\printbibliography`).
    pub fn typeset<A: Arranger>(&self, doc: &mut A, cited_only: bool) -> Result<(), BuilderError> {
        let extra = if cited_only {
            Vec::new()
        } else {
            self.library.iter().map(|e| vec![CitationItem::new(e, None, None, true, None)]).collect()
        };
        let rendered = self.render(&extra)?;
        let Some(bibliography) = rendered.bibliography else { return Ok(()) };
        doc.leave_hmode(false)?;
        let saved = doc.settings().clone();
        let skips = doc.line_skips();
        let left = Length::pt(skips.left.length.to_pt().unwrap_or(0.0));
        let hang = bibliography.hanging_indent || bibliography.second_field_align.is_some();
        let indent = self.indent.unwrap_or_else(|| 3.0 * doc.font_spec().map_or(10.0, |f| f.size));
        let result = (|| {
            if hang {
                doc.set_line_skips(LineSkips { left: left + Length::pt(indent), ..skips });
                doc.set_paragraph_indent(-indent);
            } else {
                doc.set_line_skips(LineSkips { left, ..skips });
                doc.set_paragraph_indent(0.0);
            }
            for item in &bibliography.items {
                if let Some(first) = &item.first_field {
                    doc.start_hbox();
                    self.set_child(doc, first, Formatting::default())?;
                    let mut hbox = doc.make_hbox()?;
                    if hbox.width.length.to_pt().unwrap_or(0.0) > indent {
                        doc.add_box(hbox).add_text(" ");
                    } else {
                        hbox.width = Length::pt(indent);
                        doc.add_box(hbox);
                    }
                }
                self.set(doc, &item.content)?;
                doc.leave_hmode(false)?;
            }
            Ok(())
        })();
        doc.restore_settings(saved);
        result
    }

    fn set<A: Arranger>(&self, doc: &mut A, children: &ElemChildren) -> Result<(), BuilderError> {
        for child in &children.0 {
            self.set_child(doc, child, Formatting::default())?;
        }
        Ok(())
    }

    fn set_child<A: Arranger>(&self, doc: &mut A, child: &ElemChild, _outer: Formatting) -> Result<(), BuilderError> {
        match child {
            ElemChild::Text(t) => formatted(doc, &t.text, t.formatting)?,
            ElemChild::Markup(text) => {
                doc.add_text(text.clone());
            }
            ElemChild::Link { text, url } => {
                doc.start_link(LinkDest::Uri(url.clone()));
                formatted(doc, &text.text, text.formatting)?;
                doc.end_hbox();
            }
            ElemChild::Transparent { .. } => {}
            ElemChild::Elem(elem) => {
                for child in &elem.children.0 {
                    self.set_child(doc, child, Formatting::default())?;
                }
                if matches!(elem.display, Some(Display::Block | Display::Indent)) {
                    doc.leave_hmode(false)?;
                }
            }
        }
        Ok(())
    }
}

/// `text` in the font `formatting` asks for. Super- and subscripts are
/// raised or lowered and set smaller, as SILE fakes them.
fn formatted<A: Arranger>(doc: &mut A, text: &str, formatting: Formatting) -> Result<(), BuilderError> {
    if formatting == Formatting::default() {
        doc.add_text(text);
        return Ok(());
    }
    let saved = doc.settings().clone();
    let x_height = doc.x_height();
    let result = (|| {
        doc.update_font(|f| {
            if formatting.font_style == FontStyle::Italic {
                f.style = Style_::Italic;
            }
            match formatting.font_weight {
                FontWeight::Bold => f.weight = Weight(700),
                FontWeight::Light => f.weight = Weight(300),
                FontWeight::Normal => {}
            }
            if formatting.font_variant == FontVariant::SmallCaps {
                f.features = if f.features.is_empty() { "+smcp".into() } else { format!("{};+smcp", f.features) };
            }
            if matches!(formatting.vertical_align, VerticalAlign::Sup | VerticalAlign::Sub) {
                f.size = 1.5 * x_height;
            }
        })?;
        let shift = match formatting.vertical_align {
            VerticalAlign::Sup => 0.7 * x_height,
            VerticalAlign::Sub => -0.3 * x_height,
            _ => 0.0,
        };
        if shift != 0.0 {
            doc.add_baseline_shift(shift);
        }
        doc.add_text(text);
        if shift != 0.0 {
            doc.add_baseline_shift(-shift);
        }
        Ok(())
    })();
    doc.restore_settings(saved);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::builder::Galley;

    const BIB: &str = r#"
@book{knuth1984,
  author = {Knuth, Donald E.},
  title = {The {TeX}book},
  publisher = {Addison-Wesley},
  year = {1984},
}
@article{lamport1986,
  author = {Lamport, Leslie},
  title = {LaTeX: A Document Preparation System},
  journal = {Computers and Typesetting},
  year = {1986},
}
"#;

    fn text(d: Galley) -> String {
        let pages = d.lay_out().unwrap().pages;
        text_in(&pages[0], "content")
    }

    #[test]
    fn citations_and_bibliography_follow_the_style() {
        let mut bib = Bibliography::default();
        bib.load_bibtex(BIB).unwrap();
        let mut d = galley();
        d.add_text("See ");
        bib.cite(&mut d, vec![Cite { key: "knuth1984".into(), locator: Some(("page".into(), "42".into())) }]).unwrap();
        d.new_paragraph().unwrap();
        bib.typeset(&mut d, true).unwrap();
        let text = text(d);
        assert!(text.contains("(Knuth1984,42)"), "{text}");
        assert!(text.contains("TheTeXbook"), "{text}");
        assert!(!text.contains("Lamport"), "{text}");
    }

    #[test]
    fn numeric_styles_number_in_citation_order_and_can_list_everything() {
        let mut bib = Bibliography::default();
        bib.load_bibtex(BIB).unwrap();
        bib.set_style("ieee", None).unwrap();
        let mut d = galley();
        bib.cite(&mut d, vec![Cite::new("lamport1986")]).unwrap();
        bib.cite(&mut d, vec![Cite::new("knuth1984")]).unwrap();
        d.new_paragraph().unwrap();
        bib.typeset(&mut d, false).unwrap();
        let text = text(d);
        assert!(text.starts_with("[1][2]"), "{text}");
        assert!(text.contains("Lamport") && text.contains("Knuth"), "{text}");
        assert!(bib.cite(&mut galley(), vec![Cite::new("nobody")]).is_err());
    }
}
