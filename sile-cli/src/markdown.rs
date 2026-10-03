//! CommonMark set through the builder API: headings become the class's
//! sectioning, and everything else maps onto paragraphs, fonts, lists,
//! links, images, verbatim blocks, footnotes, tables, task lists and
//! `$`/`$$` math in the TeX-like syntax.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sile_core::builder::{Arranger, BuilderError, DocumentBuilder, LineSkips};
use sile_core::class::{bigskip, medskip, smallskip, with_font, Book, Heading};
use sile_core::font::{FontSpec, FontStyle, FontWeight};
use sile_core::image::Image;
use sile_core::length::Length;
use sile_core::lists::{ListKind, ListOptions};
use sile_core::math::{MathMode, TexMath};
use sile_core::node::{LinkDest, Stroke};
use sile_core::structure::Role;
use sile_core::table::CellAlign;

pub struct Markdown<'a> {
    doc: DocumentBuilder,
    /// Where relative image paths start from.
    base: PathBuf,
    mono: &'a str,
    fonts: Vec<Option<FontSpec>>,
    tex: TexMath,
    footnotes: HashMap<String, Vec<Event<'static>>>,
    pub warnings: Vec<String>,
}

impl AsMut<DocumentBuilder> for Markdown<'_> {
    fn as_mut(&mut self) -> &mut DocumentBuilder {
        &mut self.doc
    }
}

impl<'a> Markdown<'a> {
    /// Set Markdown into `doc`, finding images relative to `base` and
    /// setting code in the `mono` font family.
    pub fn new(doc: DocumentBuilder, base: &Path, mono: &'a str) -> Self {
        Self { doc, base: base.to_path_buf(), mono, fonts: Vec::new(), tex: TexMath::new(), footnotes: HashMap::new(), warnings: Vec::new() }
    }

    pub fn finish(self) -> DocumentBuilder {
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
        Ok(())
    }

    fn events(&mut self, events: &[Event]) -> Result<(), BuilderError> {
        let mut i = 0;
        while i < events.len() {
            match &events[i] {
                Event::Start(Tag::Heading { level, .. }) => {
                    let end = events[i..].iter().position(|e| matches!(e, Event::End(TagEnd::Heading(_)))).map_or(events.len(), |p| i + p);
                    self.heading(*level, &events[i + 1..end])?;
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
                let mono = self.mono.to_string();
                self.push_font(|f| f.family = Some(mono))?;
                self.doc.begin_structure(Role::Code).add_text(code.to_string()).end_structure();
                self.pop_font()?;
            }
            Event::InlineMath(src) => self.math(src, MathMode::Text)?,
            Event::DisplayMath(src) => self.math(src, MathMode::Display { number: None })?,
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
                let width = self.doc.frame_size()?.0 - skips.left.to_pt_abs() - skips.right.to_pt_abs();
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
                Some(note) => DocumentBuilder::footnote(self, |md: &mut Self| md.events(&note))?,
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
    fn math(&mut self, src: &str, mode: MathMode) -> Result<(), BuilderError> {
        let set = match self.tex.parse(src) {
            Ok(formula) => self.doc.add_math(&formula, mode).map(|_| ()).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        if let Err(e) = set {
            self.warnings.push(format!("math {src:?}: {e}"));
            self.event(&Event::Code(src.to_string().into()))?;
        }
        Ok(())
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

    /// Book chapters, sections and subsections for the first three levels
    /// under the book class; bold unnumbered headings otherwise.
    fn heading(&mut self, level: HeadingLevel, title: &[Event]) -> Result<(), BuilderError> {
        let book = self.doc.class_mut::<Book>().is_some();
        let title = |md: &mut Self| md.events(title);
        match (book, level) {
            (true, HeadingLevel::H1) => Book::chapter(self, Heading::default(), title),
            (true, HeadingLevel::H2) => Book::section(self, Heading::default(), title),
            (true, HeadingLevel::H3) => Book::subsection(self, Heading::default(), title),
            _ => {
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
        }
    }

    /// Lines set as they are in the `mono` font, without hyphenation.
    fn code_block(&mut self, code: &str, indented: bool) -> Result<(), BuilderError> {
        let doc = &mut self.doc;
        doc.new_paragraph()?;
        doc.add_explicit_vskip(smallskip())?;
        let saved = doc.settings().clone();
        let mono = self.mono.to_string();
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
                    doc.add_box(sile_core::node::HBox::new(Length::zero(), Length::zero(), Length::zero()));
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
    use sile_core::class::Plain;
    use sile_core::frame::PaperSize;

    fn trace(class: impl sile_core::class::DocumentClass, src: &str) -> (String, Vec<String>) {
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        doc.set_class(class);
        doc.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts"));
        doc.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() }).unwrap();
        let mut md = Markdown::new(doc, Path::new("."), "Gentium Plus");
        md.typeset(src).unwrap();
        let warnings = std::mem::take(&mut md.warnings);
        (md.finish().render_debug().unwrap(), warnings)
    }

    fn words(trace: &str) -> Vec<&str> {
        trace.lines().filter_map(|l| l.rsplit_once('(').and_then(|(_, w)| w.strip_suffix(')'))).collect()
    }

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
        let (trace, warnings) = trace(Plain::new(), "Before\n\n```\nfn main() {\n\n    x\n}\n```\n\n<b>no</b>\n");
        assert_eq!(warnings, ["HTML is not supported: <b>", "HTML is not supported: </b>"]);
        let lines: Vec<&str> = trace.lines().collect();
        let x_of = |word: &str| -> f64 {
            let at = lines.iter().position(|l| l.ends_with(&format!("({word})"))).unwrap();
            lines[..at].iter().rev().find_map(|l| l.strip_prefix("Mx \t")).unwrap().parse().unwrap()
        };
        assert!(x_of("x") > x_of("fn") + 5.0);
        assert!((x_of("}") - x_of("fn")).abs() < 1e-3);
    }

    #[test]
    fn math_without_a_math_font_is_set_as_code() {
        let (trace, warnings) = trace(Plain::new(), "Euler: $e^{i\\pi} = -1$.\n");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].starts_with("math \"e^{i\\\\pi} = -1\""), "{warnings:?}");
        assert!(words(&trace).contains(&"Euler"), "{trace}");
    }

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
}
