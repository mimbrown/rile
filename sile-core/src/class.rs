//! Document classes: the page template content flows into, and what
//! happens as each page starts and ends (folios, running heads).

use std::any::Any;

use crate::builder::{BuilderError, DocumentBuilder, LineSkips, Material, TextAlign};
use crate::font::{FontSpec, FontStyle, FontWeight};
use crate::framespec::{FrameDirection, FrameSpec};
use crate::length::Length;
use crate::measurement::Measurement;
use crate::messages;

/// The frames of a page and the one content starts in.
#[derive(Debug, Clone, PartialEq)]
pub struct PageTemplate {
    pub frames: Vec<FrameSpec>,
    pub first_content_frame: String,
}

pub trait AsAny: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A document class. The builder asks it for each page's template, and
/// calls its hooks when a page ends (with the page still current, so
/// material can be set into its frames) and when the next one starts.
pub trait DocumentClass: AsAny {
    fn page_template(&self) -> PageTemplate;

    fn new_page(&mut self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        Ok(())
    }

    fn end_page(&mut self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolioState {
    On,
    Off,
    /// Off for the current page only (`\nofoliothispage`).
    OffThisPage,
}

/// Page numbers, set centred in the `folio` frame with the document's own
/// settings (SILE's `folio` package).
#[derive(Debug, Clone)]
pub struct Folio {
    pub value: usize,
    pub state: FolioState,
    pub frame: String,
}

impl Default for Folio {
    fn default() -> Self {
        Self { value: 1, state: FolioState::On, frame: "folio".to_string() }
    }
}

impl Folio {
    pub fn output(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        match self.state {
            FolioState::OffThisPage => self.state = FolioState::On,
            FolioState::Off => {}
            FolioState::On => {
                let text = self.value.to_string();
                doc.typeset_into(&self.frame, |d| {
                    d.use_toplevel();
                    centered(d, |d| {
                        d.add_text(text);
                        Ok(())
                    })
                })?;
            }
        }
        Ok(())
    }
}

/// SILE's `\center`: zero indent and centring skips for `f`, ended as a
/// paragraph.
pub fn centered(
    doc: &mut DocumentBuilder,
    f: impl FnOnce(&mut DocumentBuilder) -> Result<(), BuilderError>,
) -> Result<(), BuilderError> {
    let saved = doc.settings().clone();
    let skips = doc.line_skips().aligned(TextAlign::Center);
    doc.set_line_skips(skips).set_paragraph_indent(0.0).set_current_indent(Some(0.0));
    let result = f(doc).and_then(|_| doc.new_paragraph().map(|_| ()));
    doc.restore_settings(saved);
    result
}

// ---------------------------------------------------------------------------
// plain
// ---------------------------------------------------------------------------

/// SILE's `plain` class: one content frame and a folio below it.
#[derive(Debug, Clone)]
pub struct Plain {
    pub folio: Folio,
    pub frames: Vec<FrameSpec>,
}

impl Plain {
    pub fn new() -> Self {
        Self { folio: Folio::default(), frames: Self::frameset() }
    }

    pub fn frameset() -> Vec<FrameSpec> {
        vec![
            FrameSpec::new("content").left("5%pw").right("95%pw").top("5%ph").bottom("top(footnotes)"),
            FrameSpec::new("folio")
                .left("left(content)")
                .right("right(content)")
                .top("bottom(footnotes)+2%ph")
                .bottom("97%ph"),
            FrameSpec::new("footnotes")
                .left("left(content)")
                .right("right(content)")
                .height("0")
                .bottom("90%ph"),
        ]
    }
}

impl Plain {
    /// SILE's `jplain`: a content frame on a 50 by 30 character grid, set
    /// vertically if `tate`.
    pub fn japanese(tate: bool) -> Self {
        let mut frames = Self::frameset();
        frames[0] = Hanmen::PLAIN.frame("content", "8.3%pw", "11.6%ph", tate);
        Self { folio: Folio::default(), frames }
    }
}

impl Default for Plain {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentClass for Plain {
    fn page_template(&self) -> PageTemplate {
        PageTemplate { frames: self.frames.clone(), first_content_frame: "content".to_string() }
    }

    fn new_page(&mut self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        self.folio.value += 1;
        Ok(())
    }

    fn end_page(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        self.folio.output(doc)
    }
}

// ---------------------------------------------------------------------------
// hanmen
// ---------------------------------------------------------------------------

/// The character grid Japanese pages are laid out on: lines of square cells
/// with a gap between lines (SILE's `hanmenkyoshi`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hanmen {
    pub gridsize: f64,
    pub linegap: f64,
    pub linelength: usize,
    pub linecount: usize,
}

impl Hanmen {
    pub const PLAIN: Hanmen = Hanmen { gridsize: 10.0, linegap: 7.0, linelength: 50, linecount: 30 };
    pub const BOOK: Hanmen = Hanmen { gridsize: 10.0, linegap: 7.0, linelength: 40, linecount: 35 };

    /// A frame exactly one grid in size, of vertical lines from right to
    /// left if `tate`.
    pub fn frame(&self, id: &str, left: &str, top: &str, tate: bool) -> FrameSpec {
        let length = self.gridsize * self.linelength as f64;
        let breadth = self.gridsize * self.linecount as f64 + self.linegap * (self.linecount as f64 - 1.0);
        let (width, height) = if tate { (breadth, length) } else { (length, breadth) };
        let frame = FrameSpec::new(id).left(left).top(top).width(format!("{width}pt")).height(format!("{height}pt"));
        match tate {
            true => FrameSpec { direction: Some(FrameDirection::TATE), tate: true, ..frame },
            false => frame,
        }
    }

    /// From one line's baseline to the next.
    pub fn baseline_skip(&self) -> f64 {
        self.gridsize + self.linegap
    }
}

// ---------------------------------------------------------------------------
// book
// ---------------------------------------------------------------------------

/// SILE's `book` class: mirrored right and left pages, running heads above
/// the content and a folio below it.
#[derive(Clone)]
pub struct Book {
    pub folio: Folio,
    right: Vec<FrameSpec>,
    left: Vec<FrameSpec>,
    odd: bool,
    spread_counter: usize,
    pub left_head: Option<Material>,
    pub right_head: Option<Material>,
    /// No running head on the current page (blank pages of a spread).
    pub skip_head_this_page: bool,
    /// Chapters start on a new odd page.
    pub chapters_open_spread: bool,
}

impl Book {
    pub fn new() -> Self {
        Self::with_frames(Self::frameset())
    }

    /// A book whose odd (right) pages use `frames`; even pages mirror them.
    pub fn with_frames(frames: Vec<FrameSpec>) -> Self {
        Self {
            folio: Folio::default(),
            left: frames.iter().map(FrameSpec::mirrored).collect(),
            right: frames,
            odd: true,
            spread_counter: 0,
            left_head: None,
            right_head: None,
            skip_head_this_page: false,
            chapters_open_spread: true,
        }
    }

    /// SILE's `jbook`: a content frame on a 40 by 35 character grid, set
    /// vertically if `tate`.
    pub fn japanese(tate: bool) -> Self {
        Self::with_frames(vec![
            FrameSpec::new("runningHead")
                .left("left(content) + 9pt")
                .right("right(content) - 9pt")
                .height("20pt")
                .bottom("top(content)-9pt"),
            Hanmen::BOOK.frame("content", "8.3%pw", "12%ph", tate),
            FrameSpec::new("folio")
                .left("left(content)")
                .right("right(content)")
                .top("bottom(footnotes)+3%ph")
                .bottom("bottom(footnotes)+5%ph"),
            FrameSpec::new("footnotes").left("left(content)").right("right(content)").height("0").bottom("83.3%ph"),
        ])
    }

    pub fn frameset() -> Vec<FrameSpec> {
        vec![
            FrameSpec::new("content")
                .left("8.3%pw")
                .right("86%pw")
                .top("11.6%ph")
                .bottom("top(footnotes)"),
            FrameSpec::new("folio")
                .left("left(content)")
                .right("right(content)")
                .top("bottom(footnotes)+3%ph")
                .bottom("bottom(footnotes)+5%ph"),
            FrameSpec::new("runningHead")
                .left("left(content)")
                .right("right(content)")
                .top("top(content)-8%ph")
                .bottom("top(content)-3%ph"),
            FrameSpec::new("footnotes")
                .left("left(content)")
                .right("right(content)")
                .height("0")
                .bottom("83.3%ph"),
        ]
    }

    pub fn odd_page(&self) -> bool {
        self.odd
    }

    /// End the current page and carry on at the start of a spread
    /// (SILE's `\open-spread`): on an odd page when `odd`, after at least
    /// one empty page when `double`. Pages ejected to get there get no
    /// running head or folio when `blank`.
    pub fn open_spread(doc: &mut DocumentBuilder, odd: bool, double: bool, blank: bool) -> Result<(), BuilderError> {
        fn book(doc: &mut DocumentBuilder) -> &mut Book {
            doc.class_mut::<Book>().expect("book class")
        }
        let options_met = |doc: &mut DocumentBuilder| {
            let b = book(doc);
            (!double || b.spread_counter > 1) && odd == b.odd
        };
        book(doc).spread_counter = 0;
        doc.leave_hmode(false)?;
        doc.start_hbox().end_hbox();
        doc.leave_hmode(false)?;
        if book(doc).spread_counter == 1 && options_met(doc) {
            doc.clear_vertical_queue();
            return Ok(());
        }
        let started_at_top = doc.vertical_queue_len() == 2;
        let counter_at_start = book(doc).spread_counter;
        loop {
            let counter = book(doc).spread_counter;
            if counter > 0 {
                doc.start_hbox().end_hbox();
                doc.leave_hmode(false)?;
                if blank && !(counter == counter_at_start && !started_at_top) {
                    let b = book(doc);
                    b.skip_head_this_page = true;
                    b.folio.state = FolioState::OffThisPage;
                }
            }
            doc.supereject()?;
            doc.leave_hmode(false)?;
            if options_met(doc) {
                return Ok(());
            }
        }
    }
}

/// How a heading is set (SILE's `numbering` option).
#[derive(Debug, Clone, Copy)]
pub struct Heading {
    pub numbering: bool,
}

impl Default for Heading {
    fn default() -> Self {
        Self { numbering: true }
    }
}

/// SILE's `plain.bigskipamount` and friends.
pub fn bigskip() -> Length {
    Length::new(Measurement::pt(12.0), Measurement::pt(4.0), Measurement::pt(4.0))
}

pub fn medskip() -> Length {
    Length::new(Measurement::pt(6.0), Measurement::pt(2.0), Measurement::pt(2.0))
}

pub fn smallskip() -> Length {
    Length::new(Measurement::pt(3.0), Measurement::pt(1.0), Measurement::pt(1.0))
}

/// Run `body` with the font changed by `font`, then put the font back.
pub fn with_font<C, E>(
    ctx: &mut C,
    font: impl FnOnce(&mut FontSpec),
    body: impl FnOnce(&mut C) -> Result<(), E>,
) -> Result<(), E>
where
    C: AsMut<DocumentBuilder>,
    E: From<BuilderError>,
{
    let saved = ctx.as_mut().font_spec().cloned();
    ctx.as_mut().update_font(font)?;
    let result = body(ctx);
    if let Some(saved) = saved {
        ctx.as_mut().set_font_spec(saved)?;
    }
    result
}

fn bold(size: f64) -> impl FnOnce(&mut FontSpec) {
    move |f| {
        f.weight = FontWeight(800);
        f.size = size;
    }
}

/// Number a heading at `level` and set the number, through the localized
/// message `msg` when given (SILE's `book:sectioning`).
fn sectioning(doc: &mut DocumentBuilder, heading: Heading, level: usize, msg: Option<&str>) {
    if !heading.numbering {
        return;
    }
    let counter = doc.multilevel_counter_mut("sectioning");
    counter.increment(Some(level), true);
    let number = counter.format(None);
    let text = msg
        .and_then(|id| messages::message(doc.language(), id, &[("number", &number)]))
        .unwrap_or(number);
    doc.add_text(text);
}

fn book(doc: &mut DocumentBuilder) -> &mut Book {
    doc.class_mut::<Book>().expect("book class")
}

impl Book {
    /// Start a chapter on a new odd page, titled by what `title` adds, and
    /// make the title the left running head (SILE's `\chapter`).
    pub fn chapter<C, E>(ctx: &mut C, heading: Heading, mut title: impl FnMut(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        let doc = ctx.as_mut();
        doc.new_paragraph()?;
        if book(doc).chapters_open_spread {
            Book::open_spread(doc, true, false, true)?;
        }
        doc.set_current_indent(Some(0.0));
        book(doc).right_head = None;
        *doc.counter_mut("footnote") = 1;
        with_font(ctx, bold(22.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 1, Some("book-chapter-title"));
            Ok(())
        })?;
        let doc = ctx.as_mut();
        book(doc).folio.state = FolioState::OffThisPage;
        doc.new_paragraph()?;
        doc.set_current_indent(Some(0.0));
        with_font(ctx, bold(22.0), &mut title)?;
        ctx.as_mut().begin_capture();
        let captured = with_font(ctx, |f| f.size = 9.0, &mut title);
        let doc = ctx.as_mut();
        let head = doc.end_capture();
        captured?;
        book(doc).left_head = Some(head);
        doc.add_vertical_penalty(10_000)?;
        doc.new_paragraph()?;
        doc.add_vertical_penalty(10_000)?;
        doc.add_explicit_vskip(bigskip())?;
        doc.add_vertical_penalty(10_000)?;
        doc.set_current_indent(Some(0.0));
        Ok(())
    }

    /// A numbered section heading, which also becomes the right running
    /// head while folios are shown (SILE's `\section`).
    pub fn section<C, E>(ctx: &mut C, heading: Heading, mut title: impl FnMut(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        Self::heading_start(ctx.as_mut(), bigskip())?;
        with_font(ctx, bold(15.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 2, None);
            ctx.as_mut().add_text(" ");
            title(ctx)
        })?;
        if book(ctx.as_mut()).folio.state == FolioState::On {
            ctx.as_mut().begin_capture();
            let captured = with_font(
                ctx,
                |f| {
                    f.size = 9.0;
                    f.style = FontStyle::Italic;
                },
                |ctx| {
                    let doc = ctx.as_mut();
                    let skips = doc.line_skips();
                    doc.set_line_skips(skips.aligned(TextAlign::Right));
                    if heading.numbering {
                        let number = doc.multilevel_counter_mut("sectioning").format(Some(2));
                        doc.add_text(number).add_text(" ");
                    }
                    let result = title(ctx);
                    let doc = ctx.as_mut();
                    doc.new_paragraph()?;
                    doc.set_line_skips(skips);
                    result
                },
            );
            let head = ctx.as_mut().end_capture();
            captured?;
            book(ctx.as_mut()).right_head = Some(head);
        }
        Ok(Self::heading_end(ctx.as_mut())?)
    }

    /// A numbered subsection heading (SILE's `\subsection`).
    pub fn subsection<C, E>(ctx: &mut C, heading: Heading, mut title: impl FnMut(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        Self::heading_start(ctx.as_mut(), medskip())?;
        with_font(ctx, bold(12.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 3, None);
            ctx.as_mut().add_text(" ");
            title(ctx)
        })?;
        Ok(Self::heading_end(ctx.as_mut())?)
    }

    fn heading_start(doc: &mut DocumentBuilder, skip: Length) -> Result<(), BuilderError> {
        doc.new_paragraph()?;
        doc.set_current_indent(Some(0.0));
        doc.add_explicit_vskip(skip)?;
        doc.add_penalty(-500);
        Ok(())
    }

    fn heading_end(doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        doc.new_paragraph()?;
        doc.add_vertical_penalty(10_000)?;
        doc.add_explicit_vskip(smallskip())?;
        doc.add_vertical_penalty(10_000)?;
        doc.set_current_indent(Some(0.0));
        Ok(())
    }
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentClass for Book {
    fn page_template(&self) -> PageTemplate {
        let frames = if self.odd { &self.right } else { &self.left };
        PageTemplate { frames: frames.clone(), first_content_frame: "content".to_string() }
    }

    fn new_page(&mut self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        self.folio.value += 1;
        self.spread_counter += 1;
        self.odd = !self.odd;
        Ok(())
    }

    fn end_page(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        let head = if self.odd { &self.right_head } else { &self.left_head };
        if let (false, Some(head)) = (self.skip_head_this_page, head) {
            doc.typeset_into("runningHead", |d| {
                d.use_toplevel();
                let skips = LineSkips { left: Length::zero(), right: Length::zero(), ..d.line_skips() };
                d.set_line_skips(skips).set_current_indent(Some(0.0));
                d.add_material(head)?;
                d.new_paragraph()?;
                Ok(())
            })?;
        }
        self.skip_head_this_page = false;
        self.folio.output(doc)
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use crate::font::FontSpec;
    use crate::frame::PaperSize;
    use crate::node::Node;
    use crate::pagebuilder::Page;

    pub fn gentium() -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000/GentiumPlus-R.ttf");
        std::fs::read(path).expect("committed test font")
    }

    pub fn doc(class: impl DocumentClass) -> DocumentBuilder {
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
        doc.load_font_data("body", gentium(), spec).unwrap();
        doc.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000"));
        doc.set_font("body").set_class(class);
        doc
    }

    pub fn text_in(page: &Page, frame: &str) -> String {
        fn collect(nodes: &[Node], out: &mut String) {
            for n in nodes {
                match n {
                    Node::NNode(n) => out.push_str(&n.text),
                    Node::VBox(b) => collect(&b.nodes, out),
                    Node::HBox(b) => collect(&b.nodes, out),
                    _ => {}
                }
            }
        }
        let mut out = String::new();
        for (_, nodes) in page.content.iter().filter(|(id, _)| id == frame) {
            collect(nodes, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use crate::frame::PaperSize;
    use crate::pagebuilder::Page;

    fn fill_pages(doc: &mut DocumentBuilder, pages: usize) {
        for _ in 0..pages {
            doc.add_text("Page.");
            doc.supereject().unwrap();
        }
    }

    #[test]
    fn plain_numbers_every_page_but_skipped_ones() {
        let mut d = doc(Plain::new());
        fill_pages(&mut d, 2);
        // The eject is only acted on at the next paragraph, so this is
        // still page 2.
        d.class_mut::<Plain>().unwrap().folio.state = FolioState::OffThisPage;
        d.add_text("Last.");
        let pages = d.into_pages().unwrap();
        let folios: Vec<String> = pages.iter().map(|p| text_in(p, "folio")).collect();
        assert_eq!(folios, ["1", "", "3"]);
        assert_eq!(text_in(&pages[2], "content"), "Last.");
    }

    #[test]
    fn book_pages_mirror_and_carry_running_heads() {
        let mut d = doc(Book::new());
        d.begin_capture();
        d.add_text("Right head");
        let head = d.end_capture();
        d.class_mut::<Book>().unwrap().right_head = Some(head);
        fill_pages(&mut d, 3);
        let pages = d.into_pages().unwrap();
        let width = PaperSize::A5.width;
        let left = |p: &Page| p.frame("content").unwrap().left;
        assert!((left(&pages[0]) - 0.083 * width).abs() < 0.01);
        assert!((left(&pages[1]) - 0.14 * width).abs() < 0.01);
        assert!((left(&pages[2]) - 0.083 * width).abs() < 0.01);
        let heads: Vec<String> = pages.iter().map(|p| text_in(p, "runningHead")).collect();
        assert_eq!(heads[0].replace(' ', ""), "Righthead");
        assert_eq!(heads[1], "");
    }

    #[test]
    fn open_spread_starts_on_an_odd_page_with_blank_pages_unnumbered() {
        let mut d = doc(Book::new());
        d.add_text("One.");
        Book::open_spread(&mut d, true, false, true).unwrap();
        d.add_text("Three.");
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(text_in(&pages[2], "content"), "Three.");
        assert_eq!(text_in(&pages[1], "folio"), "");
        assert_eq!(text_in(&pages[2], "folio"), "3");
    }
}

#[cfg(test)]
mod footnote_tests {
    use super::tests_support::*;
    use super::*;
    use crate::insertion::InsertionClass;
    use crate::length::Length;
    use crate::node::Node;

    fn footnote(d: &mut DocumentBuilder, text: &str) {
        d.push_typesetter(Some("footnotes")).unwrap();
        d.add_text(text);
        let nodes = d.pop_typesetter().unwrap();
        d.insert("footnote", nodes);
    }

    fn with_footnotes() -> DocumentBuilder {
        let mut d = doc(Book::new());
        let mut class = InsertionClass::new("footnotes", "content", 400.0);
        class.top_box = vec![Node::vglue(Length::pt(9.0))];
        class.inter_skip = 4.5;
        d.set_insertion_class("footnote", class);
        d
    }

    #[test]
    fn footnotes_go_to_their_frame_and_shorten_the_content() {
        let mut d = with_footnotes();
        d.add_text("Body.");
        footnote(&mut d, "Note one.");
        d.add_text("More.");
        footnote(&mut d, "Note two.");
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 1);
        let page = &pages[0];
        assert_eq!(text_in(page, "footnotes").replace(' ', ""), "Noteone.Notetwo.");
        assert_eq!(text_in(page, "content").replace(' ', ""), "Body.More.");
        let (content, notes) = (page.frame("content").unwrap(), page.frame("footnotes").unwrap());
        assert!(notes.height() > 9.0);
        assert!(content.bottom <= notes.top + 0.01, "content gave up room to the notes");
    }

    #[test]
    fn a_footnote_too_long_for_the_page_is_split() {
        let mut d = with_footnotes();
        d.add_text("Body.");
        footnote(&mut d, &"Long note text. ".repeat(400));
        let pages = d.into_pages().unwrap();
        assert!(pages.len() >= 2);
        assert!(!text_in(&pages[0], "footnotes").is_empty());
        assert!(!text_in(&pages[1], "footnotes").is_empty());
        assert_eq!(text_in(&pages[0], "content"), "Body.");
    }
}

#[cfg(test)]
mod heading_tests {
    use super::tests_support::*;
    use super::*;

    fn title(text: &'static str) -> impl FnMut(&mut DocumentBuilder) -> Result<(), BuilderError> {
        move |d| {
            d.add_text(text);
            Ok(())
        }
    }

    #[test]
    fn chapters_open_an_odd_page_and_head_the_left_pages() {
        let mut d = doc(Book::new());
        d.add_text("Intro.");
        Book::chapter(&mut d, Heading::default(), title("Begin")).unwrap();
        d.add_text("Body.");
        d.supereject().unwrap();
        d.add_text("More.");
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 4);
        assert_eq!(text_in(&pages[1], "content"), "");
        assert_eq!(text_in(&pages[2], "content"), "Chapter1BeginBody.");
        assert_eq!(text_in(&pages[2], "folio"), "");
        assert_eq!(text_in(&pages[3], "runningHead"), "Begin");
        assert_eq!(text_in(&pages[3], "folio"), "4");
    }

    #[test]
    fn sections_are_numbered_within_their_chapter() {
        let mut d = doc(Book::new());
        Book::chapter(&mut d, Heading::default(), title("One")).unwrap();
        Book::section(&mut d, Heading::default(), title("A")).unwrap();
        Book::section(&mut d, Heading::default(), title("B")).unwrap();
        Book::subsection(&mut d, Heading::default(), title("b")).unwrap();
        Book::chapter(&mut d, Heading { numbering: false }, title("Two")).unwrap();
        Book::chapter(&mut d, Heading::default(), title("Three")).unwrap();
        Book::section(&mut d, Heading::default(), title("C")).unwrap();
        d.add_text("End.");
        let text: String = d.into_pages().unwrap().iter().map(|p| text_in(p, "content")).collect();
        assert_eq!(text, "Chapter1One1.1A1.2B1.2.1bTwoChapter2Three2.1CEnd.");
    }

    #[test]
    fn sections_head_right_pages_while_folios_show() {
        let mut d = doc(Book::new());
        d.add_text("Intro.");
        Book::section(&mut d, Heading::default(), title("Methods")).unwrap();
        d.add_text("Text.");
        d.supereject().unwrap();
        d.add_text("Even.");
        d.supereject().unwrap();
        d.add_text("Odd.");
        let pages = d.into_pages().unwrap();
        assert_eq!(text_in(&pages[0], "runningHead"), "0.1Methods");
        assert_eq!(text_in(&pages[1], "runningHead"), "");
        assert_eq!(text_in(&pages[2], "runningHead"), "0.1Methods");
    }

    #[test]
    fn chapter_titles_are_localized() {
        let mut d = doc(Book::new());
        d.set_language("tr");
        Book::chapter(&mut d, Heading::default(), title("Selam")).unwrap();
        let pages = d.into_pages().unwrap();
        assert_eq!(text_in(pages.last().unwrap(), "content"), "Bölüm1Selam");
    }
}
