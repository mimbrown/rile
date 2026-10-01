//! Document classes: the page template content flows into, and what
//! happens as each page starts and ends (folios, running heads).

use std::any::Any;

use crate::builder::{BuilderError, DocumentBuilder, LineSkips, Material, TextAlign};
use crate::framespec::FrameSpec;
use crate::length::Length;

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
        }
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
mod tests {
    use super::*;
    use crate::font::FontSpec;
    use crate::frame::PaperSize;
    use crate::node::Node;
    use crate::pagebuilder::Page;

    fn gentium() -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000/GentiumPlus-R.ttf");
        std::fs::read(path).expect("committed test font")
    }

    fn doc(class: impl DocumentClass) -> DocumentBuilder {
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
        doc.load_font_data("body", gentium(), spec).unwrap();
        doc.set_font("body").set_class(class);
        doc
    }

    fn text_in(page: &Page, frame: &str) -> String {
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
