//! CommonMark set through the builder API into any arranger: paragraphs,
//! fonts, lists, links, images, verbatim blocks, footnotes, tables, task
//! lists and `$`/`$$` math in the TeX-like syntax. Arrangers say how
//! headings and footnotes are set through [`MarkdownTarget`]; by default
//! headings are bold and unnumbered and notes are endnotes.
//!
//! ```
//! use std::path::Path;
//! use rile::builder::{BuilderError, Galley};
//! use rile::font::FontSpec;
//! use rile_markdown::Markdown;
//!
//! # fn main() -> Result<(), BuilderError> {
//! let mut galley = Galley::new(Some(300.0));
//! # galley.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts"));
//! galley.load_fonts_dir("fonts");
//! galley.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() })?;
//! let mut md = Markdown::new(galley, Path::new("."), "Gentium Plus");
//! md.typeset("# Notes\n\nSome *emphasis* and a footnote.[^1]\n\n[^1]: Set at the end.\n")?;
//! assert!(md.warnings.is_empty());
//! let layout = md.finish().lay_out()?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use pulldown_cmark::{Event, HeadingLevel};
use pulldown_cmark::{Alignment, CodeBlockKind, Options, Parser, Tag, TagEnd};
use rile::builder::{Arranger, BuilderError, Context, Galley, LineSkips};
use rile::builder::{bigskip, medskip, smallskip, with_font};
use rile::font::{FontSpec, FontStyle, FontWeight};
use rile::image::Image;
use rile::length::Length;
use rile::lists::{ListKind, ListOptions};
#[cfg(feature = "math")]
use rile::math::{MathMode, TexMath};
use rile::node::{LinkDest, Stroke};
use rile::structure::Role;
use rile::table::CellAlign;
#[cfg(feature = "pages")]
use rile_pages::{
    DocumentBuilder,
    class::{Book, Heading},
};

/// How an arranger sets what Markdown leaves to it. Each returns whether
/// it set the material; when it didn't, [`Markdown`] does it its own way.
pub trait MarkdownTarget: Arranger + Sized {
    fn footnote(_md: &mut Markdown<Self>, _note: &[Event]) -> Result<bool, BuilderError> {
        Ok(false)
    }

    fn heading(_md: &mut Markdown<Self>, _level: HeadingLevel, _title: &[Event]) -> Result<bool, BuilderError> {
        Ok(false)
    }
}

impl MarkdownTarget for Galley {}

/// Footnotes at the foot of the page, and chapters, sections and
/// subsections for the first three heading levels under the book class.
#[cfg(feature = "pages")]
impl MarkdownTarget for DocumentBuilder {
    fn footnote(md: &mut Markdown<Self>, note: &[Event]) -> Result<bool, BuilderError> {
        DocumentBuilder::footnote(md, |md: &mut Markdown<Self>| md.events(note))?;
        Ok(true)
    }

    fn heading(md: &mut Markdown<Self>, level: HeadingLevel, title: &[Event]) -> Result<bool, BuilderError> {
        if md.doc.class_mut::<Book>().is_none() {
            return Ok(false);
        }
        let title = |md: &mut Markdown<Self>| md.events(title);
        match level {
            HeadingLevel::H1 => Book::chapter(md, Heading::default(), title)?,
            HeadingLevel::H2 => Book::section(md, Heading::default(), title)?,
            HeadingLevel::H3 => Book::subsection(md, Heading::default(), title)?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

pub struct Markdown<A: MarkdownTarget> {
    doc: A,
    /// Where relative image paths start from.
    base: PathBuf,
    mono: String,
    fonts: Vec<Option<FontSpec>>,
    #[cfg(feature = "math")]
    tex: TexMath,
    footnotes: HashMap<String, Vec<Event<'static>>>,
    endnotes: Vec<Vec<Event<'static>>>,
    pub warnings: Vec<String>,
}

impl<A: MarkdownTarget> Context for Markdown<A> {
    type Arranger = A;

    fn arranger(&mut self) -> &mut A {
        &mut self.doc
    }
}

impl<A: MarkdownTarget> Markdown<A> {
    /// Set Markdown into `doc`, finding images relative to `base` and
    /// setting code in the `mono` font family.
    pub fn new(doc: A, base: &Path, mono: impl Into<String>) -> Self {
        Self {
            doc,
            base: base.to_path_buf(),
            mono: mono.into(),
            fonts: Vec::new(),
            #[cfg(feature = "math")]
            tex: TexMath::new(),
            footnotes: HashMap::new(),
            endnotes: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn finish(self) -> A {
        self.doc
    }

    pub fn typeset(&mut self, src: &str) -> Result<(), BuilderError> {
        let options = Options::ENABLE_SMART_PUNCTUATION
            | Options::ENABLE_MATH
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS;
        let mut events = Vec::new();
        let mut note: Option<(String, Vec<Event<'static>>)> = None;
        for event in Parser::new_ext(src, options) {
            match event {
                Event::Start(Tag::FootnoteDefinition(label)) => note = Some((label.to_string(), Vec::new())),
                Event::End(TagEnd::FootnoteDefinition) => {
                    if let Some((label, mut body)) = note.take() {
                        if matches!(body.last(), Some(Event::End(TagEnd::Paragraph))) {
                            body.pop();
                        }
                        self.footnotes.insert(label, body);
                    }
                }
                event => match &mut note {
                    Some((_, body)) => body.push(event.into_static()),
                    None => events.push(event),
                },
            }
        }
        self.events(&events)?;
        self.doc.new_paragraph()?;
        self.endnotes()
    }

    /// Set parsed Markdown; for the hooks of a [`MarkdownTarget`].
    pub fn events(&mut self, events: &[Event]) -> Result<(), BuilderError> {
        let mut i = 0;
        while i < events.len() {
            match &events[i] {
                Event::Start(Tag::Heading { level, .. }) => {
                    let end = events[i..].iter().position(|e| matches!(e, Event::End(TagEnd::Heading(_)))).map_or(events.len(), |p| i + p);
                    let title = &events[i + 1..end];
                    if !A::heading(self, *level, title)? {
                        self.heading(*level, title)?;
                    }
                    i = end;
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let end = events[i..].iter().position(|e| matches!(e, Event::End(TagEnd::CodeBlock))).map_or(events.len(), |p| i + p);
                    let code: String = events[i + 1..end].iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_ref()) } else { None }).collect();
                    let indented = matches!(kind, CodeBlockKind::Indented);
                    self.code_block(code.strip_suffix('\n').unwrap_or(&code), indented)?;
                    i = end;
                }
                Event::Start(Tag::Item) => {
                    let task = events[i + 1..].iter().take(2).find_map(|e| if let Event::TaskListMarker(done) = e { Some(*done) } else { None });
                    match task {
                        Some(done) => self.doc.begin_task_item(done)?,
                        None => self.doc.begin_item(None)?,
                    };
                }
                event => self.event(event)?,
            }
            i += 1;
        }
        Ok(())
    }

    fn event(&mut self, event: &Event) -> Result<(), BuilderError> {
        let doc = &mut self.doc;
        match event {
            Event::Text(text) => {
                doc.add_text(text.to_string());
            }
            Event::Code(code) => {
                let mono = self.mono.clone();
                self.push_font(|f| f.family = Some(mono))?;
                self.doc.begin_structure(Role::Code).add_text(code.to_string()).end_structure();
                self.pop_font()?;
            }
            Event::InlineMath(src) => self.math(src, false)?,
            Event::DisplayMath(src) => self.math(src, true)?,
            Event::SoftBreak => {
                doc.add_text(" ");
            }
            Event::HardBreak => {
                doc.add_hfill().add_penalty(-10_000);
            }
            Event::Rule => {
                doc.new_paragraph()?;
                doc.add_explicit_vskip(medskip())?;
                doc.set_current_indent(Some(0.0));
                doc.add_hrulefill(Stroke { raise: 0.0, thickness: 0.5 });
                doc.new_paragraph()?;
                doc.add_explicit_vskip(medskip())?;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                doc.new_paragraph()?;
            }
            Event::Start(Tag::Emphasis) => self.push_font(|f| f.style = if f.style == FontStyle::Italic { FontStyle::Normal } else { FontStyle::Italic })?,
            Event::Start(Tag::Strong) => self.push_font(|f| f.weight = FontWeight::BOLD)?,
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => self.pop_font()?,
            Event::Start(Tag::Link { dest_url, .. }) => {
                doc.start_link(LinkDest::Uri(dest_url.to_string()));
            }
            Event::End(TagEnd::Link) => {
                doc.end_hbox();
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let path = self.base.join(dest_url.as_ref());
                let image = Image::load(&path, dest_url.to_string())?;
                let skips = self.doc.line_skips();
                self.doc.before_lines()?;
                let width = self.doc.frame_context().line_length - skips.left.to_pt_abs() - skips.right.to_pt_abs();
                let width = (image.natural_size().0 > width).then_some(width);
                self.doc.begin_structure(Role::Figure).add_image(Arc::new(image), width, None);
                self.doc.begin_capture();
            }
            Event::End(TagEnd::Image) => {
                let alt = self.doc.end_capture().text();
                self.doc.set_alt_text(alt).end_structure();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                doc.new_paragraph()?;
                doc.begin_structure(Role::BlockQuote);
                let skips = doc.line_skips();
                let indent = 2.0 * doc.font_spec().map_or(10.0, |f| f.size);
                doc.set_line_skips(LineSkips { left: skips.left + Length::pt(indent), right: skips.right + Length::pt(indent), ..skips });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                doc.new_paragraph()?;
                doc.end_structure();
                let skips = doc.line_skips();
                let indent = 2.0 * doc.font_spec().map_or(10.0, |f| f.size);
                doc.set_line_skips(LineSkips { left: skips.left - Length::pt(indent), right: skips.right - Length::pt(indent), ..skips });
            }
            Event::Start(Tag::List(start)) => {
                doc.new_paragraph()?;
                let kind = if start.is_some() { ListKind::Enumerate } else { ListKind::Itemize };
                let options = ListOptions { start: start.map(|s| s as i64), ..Default::default() };
                doc.begin_list(kind, &options)?;
            }
            Event::End(TagEnd::List(_)) => {
                doc.end_list()?;
            }
            Event::TaskListMarker(_) => {}
            Event::End(TagEnd::Item) => {
                doc.new_paragraph()?;
                doc.end_item()?;
            }
            Event::Start(Tag::Strikethrough) => {
                doc.start_strikethrough();
            }
            Event::End(TagEnd::Strikethrough) => {
                doc.end_hbox();
            }
            Event::FootnoteReference(label) => match self.footnotes.get(label.as_ref()).cloned() {
                Some(note) => {
                    if !A::footnote(self, &note)? {
                        self.endnote(note)?;
                    }
                }
                None => {
                    self.warnings.push(format!("no footnote [^{label}]"));
                    doc.add_text(format!("[^{label}]"));
                }
            },
            Event::Start(Tag::Table(alignments)) => {
                let columns: Vec<CellAlign> = alignments
                    .iter()
                    .map(|a| match a {
                        Alignment::Center => CellAlign::Center,
                        Alignment::Right => CellAlign::Right,
                        Alignment::Left | Alignment::None => CellAlign::Left,
                    })
                    .collect();
                doc.begin_table(&columns)?;
            }
            Event::End(TagEnd::Table) => {
                doc.end_table()?;
                doc.set_current_indent(Some(0.0));
            }
            Event::Start(Tag::TableHead) => {
                doc.begin_table_row(true);
                self.push_font(|f| f.weight = FontWeight::BOLD)?;
            }
            Event::End(TagEnd::TableHead) => {
                self.pop_font()?;
                self.doc.end_table_row();
            }
            Event::Start(Tag::TableRow) => {
                doc.begin_table_row(false);
            }
            Event::End(TagEnd::TableRow) => {
                doc.end_table_row();
            }
            Event::Start(Tag::TableCell) => {
                doc.begin_table_cell();
            }
            Event::End(TagEnd::TableCell) => {
                doc.end_table_cell();
            }
            Event::Html(html) | Event::InlineHtml(html) => self.warnings.push(format!("HTML is not supported: {}", html.trim())),
            Event::Start(Tag::HtmlBlock) | Event::End(TagEnd::HtmlBlock) => {}
            other => self.warnings.push(format!("not supported: {other:?}")),
        }
        Ok(())
    }

    /// A formula, or its source as code with a warning when it can't be
    /// set.
    fn math(&mut self, src: &str, display: bool) -> Result<(), BuilderError> {
        #[cfg(feature = "math")]
        let set = {
            let mode = if display { MathMode::Display { number: None } } else { MathMode::Text };
            match self.tex.parse(src) {
                Ok(formula) => self.doc.add_math(&formula, mode).map(|_| ()).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        #[cfg(not(feature = "math"))]
        let set: Result<(), String> = {
            let _ = display;
            Err("math is not supported".into())
        };
        if let Err(e) = set {
            self.warnings.push(format!("math {src:?}: {e}"));
            self.event(&Event::Code(src.to_string().into()))?;
        }
        Ok(())
    }

    /// A raised number here for a note set at the end.
    fn endnote(&mut self, note: Vec<Event<'static>>) -> Result<(), BuilderError> {
        self.endnotes.push(note);
        let number = self.endnotes.len().to_string();
        let doc = &mut self.doc;
        let saved = doc.settings().clone();
        let (raise, size) = (0.7 * doc.x_height(), 1.5 * doc.x_height());
        doc.begin_structure(Role::Reference);
        doc.update_font(|f| f.size = size)?;
        doc.add_baseline_shift(raise).add_text(number).add_baseline_shift(-raise);
        doc.end_structure().restore_settings(saved);
        Ok(())
    }

    /// The notes so far, numbered and at 90% size, after a rule.
    fn endnotes(&mut self) -> Result<(), BuilderError> {
        let notes = std::mem::take(&mut self.endnotes);
        if notes.is_empty() {
            return Ok(());
        }
        self.event(&Event::Rule)?;
        let saved = self.doc.settings().clone();
        self.doc.update_font(|f| f.size *= 0.9)?;
        let em = self.doc.font_spec().map_or(10.0, |f| f.size);
        let mut result = Ok(());
        for (i, note) in notes.iter().enumerate() {
            let doc = &mut self.doc;
            doc.begin_structure(Role::Note).set_current_indent(Some(0.0));
            doc.begin_structure(Role::Lbl).add_text(format!("{}.", i + 1)).end_structure();
            doc.add_glue(Length::pt(2.0 * em));
            result = self.events(note).and_then(|_| self.doc.new_paragraph().map(|_| ()));
            self.doc.end_structure();
            if result.is_err() {
                break;
            }
        }
        self.doc.restore_settings(saved);
        result
    }

    fn push_font(&mut self, change: impl FnOnce(&mut FontSpec)) -> Result<(), BuilderError> {
        self.fonts.push(self.doc.font_spec().cloned());
        self.doc.update_font(change)?;
        Ok(())
    }

    fn pop_font(&mut self) -> Result<(), BuilderError> {
        if let Some(Some(spec)) = self.fonts.pop() {
            self.doc.set_font_spec(spec)?;
        }
        Ok(())
    }

    /// A bold unnumbered heading.
    fn heading(&mut self, level: HeadingLevel, title: &[Event]) -> Result<(), BuilderError> {
        let title = |md: &mut Self| md.events(title);
        let base = self.doc.font_spec().map_or(10.0, |f| f.size);
        let (size, skip) = match level {
            HeadingLevel::H1 => (base * 1.7, bigskip()),
            HeadingLevel::H2 => (base * 1.4, bigskip()),
            HeadingLevel::H3 => (base * 1.2, medskip()),
            _ => (base, medskip()),
        };
        let doc = &mut self.doc;
        doc.new_paragraph()?;
        doc.set_current_indent(Some(0.0));
        doc.add_explicit_vskip(skip)?;
        doc.add_penalty(-500);
        doc.begin_structure(Role::heading(level as usize));
        let titled = with_font(self, |f| {
            f.weight = FontWeight::BOLD;
            f.size = size;
        }, title);
        let doc = &mut self.doc;
        doc.new_paragraph()?;
        doc.end_structure();
        titled?;
        doc.add_vertical_penalty(10_000)?;
        doc.add_explicit_vskip(smallskip())?;
        doc.add_vertical_penalty(10_000)?;
        doc.set_current_indent(Some(0.0));
        Ok(())
    }

    /// Lines set as they are in the `mono` font, without hyphenation.
    fn code_block(&mut self, code: &str, indented: bool) -> Result<(), BuilderError> {
        let doc = &mut self.doc;
        doc.new_paragraph()?;
        doc.add_explicit_vskip(smallskip())?;
        let saved = doc.settings().clone();
        let mono = self.mono.clone();
        let result = (|| -> Result<(), BuilderError> {
            let doc = &mut self.doc;
            doc.update_font(|f| f.family = Some(mono))?;
            doc.set_language("und").set_obey_spaces(true).set_paragraph_indent(0.0).set_paragraph_skip(0.0);
            doc.begin_structure(Role::Code);
            let skips = doc.line_skips();
            let indent = if indented { 2.0 * doc.font_spec().map_or(10.0, |f| f.size) } else { 0.0 };
            doc.set_line_skips(LineSkips { left: Length::pt(skips.left.to_pt_abs() + indent), right: Length::pt(skips.right.to_pt_abs()), ..skips });
            for line in code.split('\n') {
                doc.set_current_indent(Some(0.0));
                if line.is_empty() {
                    doc.add_box(rile::node::HBox::new(Length::zero(), Length::zero(), Length::zero()));
                } else {
                    doc.add_text(line);
                }
                doc.new_paragraph()?;
            }
            Ok(())
        })();
        self.doc.restore_settings(saved);
        self.doc.end_structure();
        result?;
        self.doc.add_explicit_vskip(smallskip())?;
        self.doc.set_current_indent(Some(0.0));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "pages")]
    use rile::frame::PaperSize;
    #[cfg(feature = "pages")]
    use rile_pages::class::{DocumentClass, Plain};

    fn set<A: MarkdownTarget>(mut doc: A, src: &str) -> (A, Vec<String>) {
        doc.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts"));
        doc.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() }).unwrap();
        let mut md = Markdown::new(doc, Path::new("."), "Gentium Plus");
        md.typeset(src).unwrap();
        let warnings = std::mem::take(&mut md.warnings);
        (md.finish(), warnings)
    }

    #[cfg(feature = "pages")]
    fn trace(class: impl DocumentClass, src: &str) -> (String, Vec<String>) {
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        doc.set_class(class);
        let (doc, warnings) = set(doc, src);
        (doc.render_debug().unwrap(), warnings)
    }

    fn galley_trace(src: &str) -> (String, Vec<String>) {
        let (galley, warnings) = set(Galley::new(Some(300.0)), src);
        (galley.lay_out().unwrap().render_debug(), warnings)
    }

    fn words(trace: &str) -> Vec<&str> {
        trace.lines().filter_map(|l| l.rsplit_once('(').and_then(|(_, w)| w.strip_suffix(')'))).collect()
    }

    #[cfg(feature = "pages")]
    #[test]
    fn book_headings_are_numbered_and_lists_labelled() {
        let (trace, warnings) = trace(Book::new(), "# Start\n\nSome *text*.\n\n## Part\n\n1. one\n2. two\n\n- dot\n");
        assert!(warnings.is_empty());
        let words = words(&trace);
        for expected in ["Chapter", "Start", "1.1", "Part", "one", "2", "two", "•", "dot"] {
            assert!(words.contains(&expected), "{expected} missing from {words:?}");
        }
    }

    #[test]
    fn code_blocks_keep_their_lines_and_html_is_reported() {
        let (trace, warnings) = galley_trace("Before\n\n```\nfn main() {\n\n    x\n}\n```\n\n<b>no</b>\n");
        assert_eq!(warnings, ["HTML is not supported: <b>", "HTML is not supported: </b>"]);
        let lines: Vec<&str> = trace.lines().collect();
        let x_of = |word: &str| -> f64 {
            let at = lines.iter().position(|l| l.ends_with(&format!("({word})"))).unwrap();
            lines[..at].iter().rev().find_map(|l| l.strip_prefix("Mx \t")).unwrap().parse().unwrap()
        };
        assert!(x_of("x") > x_of("fn") + 5.0);
        assert!((x_of("}") - x_of("fn")).abs() < 1e-3);
    }

    #[cfg(feature = "math")]
    #[test]
    fn math_without_a_math_font_is_set_as_code() {
        let (trace, warnings) = galley_trace("Euler: $e^{i\\pi} = -1$.\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("math \"e^{i\\\\pi} = -1\""), "{warnings:?}");
        assert!(words(&trace).contains(&"Euler"), "{trace}");
    }

    #[cfg(feature = "pages")]
    #[test]
    fn footnotes_tables_task_lists_and_strikethrough_are_set() {
        let src = "Text.[^n] ~~Gone~~\n\n| A | B |\n|---|--:|\n| x | 1 |\n\n- [x] done\n- [ ] todo\n\n[^n]: The note.\n";
        let (trace, warnings) = trace(Plain::new(), src);
        assert!(warnings.is_empty(), "{warnings:?}");
        let words = words(&trace);
        for expected in ["Text", "1", "Gone", "A", "B", "x", "done", "todo", "The", "note"] {
            assert!(words.contains(&expected), "{expected} missing from {words:?}");
        }
        assert!(!words.contains(&"[x]") && !words.contains(&"[^n]"), "{words:?}");
    }

    #[test]
    fn galleys_set_plain_headings_and_endnotes() {
        let (trace, warnings) = galley_trace("# Start\n\nText.[^n] More.\n\n[^n]: The note.\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        let words = words(&trace);
        let at = |w: &str| words.iter().position(|x| *x == w).unwrap_or_else(|| panic!("{w} missing from {words:?}"));
        assert!(!words.contains(&"Chapter"), "{words:?}");
        let label = words.iter().rposition(|w| *w == "1").unwrap();
        assert!(at("Start") < at("Text") && at("Text") < at("1") && at("1") < at("More"), "{words:?}");
        assert!(at("More") < label && label < at("The"), "{words:?}");
    }
}
