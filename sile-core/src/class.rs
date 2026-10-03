//! Document classes: the page template content flows into, and what
//! happens as each page starts and ends (folios, running heads).

use std::any::Any;

use crate::builder::{BuilderError, DocumentBuilder, LineSkips, Material, TextAlign};
use crate::counter::PageNumber;
use crate::date::DateTime;
use crate::font::{FontSpec, FontStyle, FontWeight};
use crate::framespec::{FrameDirection, FrameSpec};
use crate::length::Length;
use crate::measurement::Measurement;
use crate::structure::Role;

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

    /// The current page's number, if the class numbers pages.
    fn folio(&self) -> Option<PageNumber> {
        None
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
    /// The numbering system, such as `arabic` or `roman`.
    pub display: String,
    pub state: FolioState,
    pub frame: String,
}

impl Default for Folio {
    fn default() -> Self {
        Self { value: 1, display: "arabic".to_string(), state: FolioState::On, frame: "folio".to_string() }
    }
}

impl Folio {
    pub fn number(&self) -> PageNumber {
        PageNumber { value: self.value as i64, display: self.display.clone() }
    }

    pub fn output(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        match self.state {
            FolioState::OffThisPage => self.state = FolioState::On,
            FolioState::Off => {}
            FolioState::On => {
                let text = self.number().to_string();
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
    /// SILE's `tplain` and `jplain`: a content frame on a 50 by 30
    /// character grid, set vertically if `tate`. `jplain` also sets the
    /// language to Japanese and the font to Noto Sans CJK JP.
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

    fn folio(&self) -> Option<PageNumber> {
        Some(self.folio.number())
    }
}

// ---------------------------------------------------------------------------
// letter
// ---------------------------------------------------------------------------

/// SILE's `letter` class: one content frame, starting two inches down, and
/// no folios.
#[derive(Debug, Clone)]
pub struct Letter {
    pub frames: Vec<FrameSpec>,
}

/// Typesets one part of a letter.
pub type LetterPart<'a, C, E> = Box<dyn FnOnce(&mut C) -> Result<(), E> + 'a>;

/// What a letter starts with. The date is today's (`%A, %d %B`, UTC) when
/// not given.
pub struct LetterParts<'a, C, E> {
    pub date: Option<LetterPart<'a, C, E>>,
    pub sender: Option<LetterPart<'a, C, E>>,
    pub recipient: Option<LetterPart<'a, C, E>>,
    pub salutation: Option<LetterPart<'a, C, E>>,
}

impl<C, E> Default for LetterParts<'_, C, E> {
    fn default() -> Self {
        Self { date: None, sender: None, recipient: None, salutation: None }
    }
}

impl Letter {
    pub fn new() -> Self {
        Self { frames: vec![FrameSpec::new("content").left("5%pw").right("95%pw").top("2in").bottom("90%ph")] }
    }

    /// Set a letter (SILE's `\letter`): its date, sender, recipient and
    /// salutation, a big skip after each, then `body`, all ragged right.
    /// Paragraphs are not indented from here on.
    pub fn letter<C, E>(ctx: &mut C, parts: LetterParts<'_, C, E>, body: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        let doc = ctx.as_mut();
        doc.set_paragraph_indent(0.0).set_current_indent(Some(0.0));
        let saved = doc.settings().clone();
        let skips = doc.line_skips().aligned(TextAlign::Left);
        doc.set_line_skips(skips);
        let result = Self::parts(ctx, parts, body);
        ctx.as_mut().restore_settings(saved);
        result
    }

    fn parts<C, E>(ctx: &mut C, parts: LetterParts<'_, C, E>, body: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        let skip = |ctx: &mut C| ctx.as_mut().add_explicit_vskip(bigskip()).map(|_| ());
        match parts.date {
            Some(date) => date(ctx)?,
            None => {
                let today = DateTime::now_utc().format("%A, %d %B").expect("valid format");
                ctx.as_mut().add_text(today);
            }
        }
        ctx.as_mut().new_paragraph()?;
        skip(ctx)?;
        if let Some(sender) = parts.sender {
            sender(ctx)?;
            skip(ctx)?;
        }
        for part in [parts.recipient, parts.salutation] {
            if let Some(part) = part {
                part(ctx)?;
            }
            skip(ctx)?;
        }
        body(ctx)?;
        ctx.as_mut().new_paragraph()?;
        Ok(())
    }
}

impl Default for Letter {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentClass for Letter {
    fn page_template(&self) -> PageTemplate {
        PageTemplate { frames: self.frames.clone(), first_content_frame: "content".to_string() }
    }
}

// ---------------------------------------------------------------------------
// diglot and triglot
// ---------------------------------------------------------------------------

/// The parallel flows of a diglot and the frames they go to.
pub const DIGLOT_FLOWS: [(&str, &str); 2] = [("left", "a"), ("right", "b")];
/// The parallel flows of a triglot and the frames they go to.
pub const TRIGLOT_FLOWS: [(&str, &str); 3] = [("left", "a"), ("middle", "b"), ("right", "c")];

impl Plain {
    /// SILE's `diglot`: frames `a` and `b` side by side, the folio below
    /// both.
    pub fn diglot() -> Self {
        let mut frames: Vec<FrameSpec> = Self::frameset().into_iter().filter(|f| f.id != "folio").collect();
        frames.extend([
            FrameSpec::new("a").left("8.3%pw").right("48%pw").top("11.6%ph").bottom("80%ph"),
            FrameSpec::new("b").left("52%pw").right("100%pw-left(a)").top("top(a)").bottom("bottom(a)"),
            FrameSpec::new("folio").left("left(a)").right("right(b)").top("bottom(a)+3%ph").bottom("bottom(a)+8%ph"),
        ]);
        Self { folio: Folio::default(), frames }
    }
}

impl Book {
    /// SILE's `triglot`: frames `a`, `b` and `c` side by side, the folio
    /// below the first two. Unlike the book's own frames they are not
    /// mirrored on left pages, so the flows keep their order.
    pub fn triglot() -> Self {
        let mut book = Self::new();
        let columns = [
            FrameSpec::new("a").left("5%pw").right("28%pw").top("11.6%ph").bottom("80%ph"),
            FrameSpec::new("b").left("33%pw").right("60%pw").top("top(a)").bottom("bottom(a)"),
            FrameSpec::new("c").left("66%pw").right("95%pw").top("top(a)").bottom("bottom(a)"),
            FrameSpec::new("folio").left("left(a)").right("right(b)").top("bottom(a)+3%pw").bottom("bottom(a)+8%ph"),
        ];
        for frames in [&mut book.right, &mut book.left] {
            frames.retain(|f| f.id != "folio");
            frames.extend(columns.iter().cloned());
        }
        book
    }
}

/// Make `doc` a diglot (SILE's `diglot` class) and start its `left` and
/// `right` flows, selected with `select_parallel` and levelled with
/// `sync_parallel`.
pub fn diglot(doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
    doc.set_class(Plain::diglot());
    doc.begin_parallel(&DIGLOT_FLOWS)?;
    Ok(())
}

/// Make `doc` a triglot (SILE's `triglot` class): unindented paragraphs
/// broken with a tolerance of 5000, in `left`, `middle` and `right` flows.
pub fn triglot(doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
    doc.set_class(Book::triglot());
    doc.linebreak_settings_mut().tolerance = 5000;
    doc.set_paragraph_indent(0.0);
    doc.begin_parallel(&TRIGLOT_FLOWS)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// pecha
// ---------------------------------------------------------------------------

/// SILE's `pecha`: Tibetan loose-leaf pages with the content frame
/// outlined and page numbers in Tibetan numerals turned down the margin to
/// its right. `runningHead`, in the left margin, is the user's to fill.
#[derive(Debug, Clone)]
pub struct Pecha {
    pub folio: Folio,
    pub frames: Vec<FrameSpec>,
}

impl Pecha {
    pub fn new() -> Self {
        Self {
            folio: Folio::default(),
            frames: vec![
                FrameSpec::new("content").left("5%pw").right("95%pw").top("5%ph").bottom("90%ph"),
                FrameSpec::new("folio").left("right(content)").width("2.5%pw").top("top(content)").height("height(content)"),
                FrameSpec::new("runningHead").right("left(content)").width("2.5%pw").top("top(content)").height("height(content)"),
            ],
        }
    }
}

impl Default for Pecha {
    fn default() -> Self {
        Self::new()
    }
}

/// Make `doc` a pecha (SILE's `pecha` class): in Tibetan, lines set flush
/// right without indents.
pub fn pecha(doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
    doc.set_class(Pecha::new()).set_language("bo");
    let skips = doc.line_skips().aligned(TextAlign::Right);
    doc.set_line_skips(skips).set_paragraph_indent(0.0);
    Ok(())
}

/// `n` in Tibetan digits.
pub fn tibetan_number(n: usize) -> String {
    n.to_string().chars().map(|d| char::from_u32(0x0f20 + d.to_digit(10).unwrap_or(0)).unwrap_or(d)).collect()
}

impl DocumentClass for Pecha {
    fn page_template(&self) -> PageTemplate {
        PageTemplate { frames: self.frames.clone(), first_content_frame: "content".to_string() }
    }

    fn new_page(&mut self, _doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        self.folio.value += 1;
        Ok(())
    }

    fn end_page(&mut self, doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        doc.show_frame(Some("content"))?;
        let Some(frame) = doc.frame("folio").cloned() else { return Ok(()) };
        let saved = doc.settings().clone();
        doc.use_toplevel().start_hbox().add_text(tibetan_number(self.folio.value));
        let number = doc.make_hbox();
        doc.restore_settings(saved);
        let number = crate::transform::rotated(number?, -90.0);
        let pt = |l: &Length| l.length.to_pt().unwrap_or(0.0);
        let (width, height, depth) = (pt(&number.width), pt(&number.height), pt(&number.depth));
        let x = frame.left + (frame.width() - width) / 2.0;
        let baseline = frame.top + (frame.height() - height - depth) / 2.0 + height;
        let mut folio = crate::node::HBox::new(number.width.clone(), number.height.clone(), number.depth.clone());
        folio.nodes.push(crate::node::Node::HBox(number));
        doc.add_overlay(crate::pagebuilder::Underlay::Box(folio, [x, baseline]))?;
        Ok(())
    }

    fn folio(&self) -> Option<PageNumber> {
        Some(self.folio.number())
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

    /// SILE's `tbook` and `jbook`: a content frame on a 40 by 35 character
    /// grid, set vertically if `tate`. `jbook` also sets the language to
    /// Japanese and the font to Noto Sans CJK JP.
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

/// How a heading is set (SILE's `numbering` and `toc` options).
#[derive(Debug, Clone, Copy)]
pub struct Heading {
    pub numbering: bool,
    /// Enter the heading in the table of contents.
    pub toc: bool,
}

impl Default for Heading {
    fn default() -> Self {
        Self { numbering: true, toc: true }
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
fn sectioning(doc: &mut DocumentBuilder, heading: Heading, level: usize, msg: Option<&str>, label: String) {
    let number = heading.numbering.then(|| {
        let counter = doc.multilevel_counter_mut("sectioning");
        counter.increment(Some(level), true);
        counter.format(None)
    });
    if heading.toc {
        doc.add_toc_entry(level, number.clone(), label);
    }
    let Some(number) = number else { return };
    let text = msg
        .and_then(|id| doc.message(id, &[("number", &number)]))
        .unwrap_or(number);
    doc.add_text(text);
}

/// What `title` sets, as text for the table of contents.
fn title_text<C, E>(ctx: &mut C, title: &mut impl FnMut(&mut C) -> Result<(), E>) -> Result<String, E>
where
    C: AsMut<DocumentBuilder>,
{
    ctx.as_mut().begin_capture();
    let result = title(ctx);
    let text = ctx.as_mut().end_capture().text();
    result.map(|_| text)
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
        let label = title_text(ctx, &mut title)?;
        ctx.as_mut().begin_structure(Role::H1);
        with_font(ctx, bold(22.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 1, Some("book-chapter-title"), label);
            Ok(())
        })?;
        let doc = ctx.as_mut();
        book(doc).folio.state = FolioState::OffThisPage;
        Self::chapter_post(doc)?;
        let titled = with_font(ctx, bold(22.0), &mut title);
        ctx.as_mut().end_structure();
        titled?;
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
        let label = title_text(ctx, &mut title)?;
        ctx.as_mut().begin_structure(Role::H2);
        let titled = with_font(ctx, bold(15.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 2, None, label);
            ctx.as_mut().add_text(" ");
            title(ctx)
        });
        ctx.as_mut().end_structure();
        titled?;
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
        let label = title_text(ctx, &mut title)?;
        ctx.as_mut().begin_structure(Role::H3);
        let titled = with_font(ctx, bold(12.0), |ctx| {
            sectioning(ctx.as_mut(), heading, 3, None, label);
            ctx.as_mut().add_text(" ");
            title(ctx)
        });
        ctx.as_mut().end_structure();
        titled?;
        Ok(Self::heading_end(ctx.as_mut())?)
    }

    /// Between a chapter's number and its title: a new unindented
    /// paragraph, but Japanese and Esperanto have their own (SILE's
    /// `book:chapter:post` and its per-language variants).
    fn chapter_post(doc: &mut DocumentBuilder) -> Result<(), BuilderError> {
        match doc.language() {
            "ja" => {
                doc.add_explicit_vskip(medskip())?;
            }
            "eo" => {
                doc.add_text("a");
                doc.add_explicit_vskip(medskip())?;
            }
            _ => {
                doc.new_paragraph()?;
                doc.set_current_indent(Some(0.0));
            }
        }
        Ok(())
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

    fn folio(&self) -> Option<PageNumber> {
        Some(self.folio.number())
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
        Book::chapter(&mut d, Heading { numbering: false, ..Default::default() }, title("Two")).unwrap();
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

#[cfg(test)]
mod letter_tests {
    use super::tests_support::*;
    use super::*;

    fn text(text: &'static str) -> Option<LetterPart<'static, DocumentBuilder, BuilderError>> {
        Some(Box::new(move |d: &mut DocumentBuilder| {
            d.add_text(text);
            Ok(())
        }))
    }

    #[test]
    fn letters_set_their_parts_in_order_below_the_letterhead_space() {
        let mut d = doc(Letter::new());
        let parts = LetterParts { date: text("1 May"), recipient: text("Jo"), salutation: text("Dear Jo,"), ..Default::default() };
        Letter::letter(&mut d, parts, |d| {
            d.add_text("Hello.");
            Ok(())
        })
        .unwrap();
        d.add_text("Yours.");
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(text_in(&pages[0], "content"), "1MayJoDearJo,Hello.Yours.");
        assert_eq!(pages[0].frame("content").unwrap().top, 144.0);
        assert!(pages[0].content.iter().all(|(id, _)| id == "content"), "no folio");
    }

    #[test]
    fn letters_are_dated_today_by_default() {
        let mut d = doc(Letter::new());
        Letter::letter(&mut d, LetterParts::default(), |_| Ok::<_, BuilderError>(())).unwrap();
        let today = DateTime::now_utc().format("%A,%d%B").unwrap();
        assert_eq!(text_in(&d.into_pages().unwrap()[0], "content"), today);
    }
}

#[cfg(test)]
mod glot_tests {
    use super::tests_support::*;
    use super::*;

    #[test]
    fn diglots_set_each_flow_in_its_own_column() {
        let mut d = doc(Plain::new());
        diglot(&mut d).unwrap();
        d.select_parallel("left").unwrap().add_text("Left.");
        d.select_parallel("right").unwrap().add_text("Right.");
        d.sync_parallel().unwrap();
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(text_in(&pages[0], "a"), "Left.");
        assert_eq!(text_in(&pages[0], "b"), "Right.");
        assert_eq!(text_in(&pages[0], "folio"), "1");
        let frame = |id| pages[0].frame(id).unwrap();
        assert!(frame("a").right < frame("b").left);
        assert_eq!(frame("folio").left, frame("a").left);
    }

    #[test]
    fn triglot_columns_keep_their_order_on_left_pages() {
        let mut d = doc(Plain::new());
        triglot(&mut d).unwrap();
        for _ in 0..80 {
            for flow in ["left", "middle", "right"] {
                d.select_parallel(flow).unwrap().add_text(flow);
            }
            d.sync_parallel().unwrap();
        }
        let pages = d.into_pages().unwrap();
        assert!(pages.len() > 1);
        for page in &pages[..2] {
            let left = |id| page.frame(id).unwrap().left;
            assert!(left("a") < left("b") && left("b") < left("c"));
            assert!(text_in(page, "c").starts_with("right"));
        }
    }
}

#[cfg(test)]
mod pecha_tests {
    use super::tests_support::*;
    use super::*;

    #[test]
    fn pecha_pages_are_numbered_in_tibetan_down_the_right_margin() {
        assert_eq!(tibetan_number(120), "༡༢༠");
        let mut d = doc(Plain::new());
        pecha(&mut d).unwrap();
        d.add_text("Text.");
        d.supereject().unwrap();
        d.add_text("More.");
        let pages = d.into_pages().unwrap();
        assert_eq!(pages.len(), 2);
        let crate::pagebuilder::Underlay::Box(folio, [x, _]) = &pages[1].overlay[0] else { panic!("no folio") };
        let crate::node::Node::HBox(number) = &folio.nodes[0] else { panic!("no number") };
        let text: String = number.nodes.iter().filter_map(|n| match n {
            crate::node::Node::NNode(n) => Some(n.text.to_string()),
            _ => None,
        }).collect();
        assert_eq!(text, "༢");
        assert!(*x > pages[1].frame("content").unwrap().right);
        assert_eq!(pages[1].outlines.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["content"]);
        let (content, folio) = (pages[0].frame("content").unwrap(), pages[0].frame("folio").unwrap());
        assert_eq!((folio.left, folio.top, folio.bottom), (content.right, content.top, content.bottom));
    }
}
