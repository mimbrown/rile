use std::path::Path;
use std::sync::Arc;

use crate::color::Color;
use crate::font::{Direction, FontDatabase, FontError, FontFace, FontSpec};
use crate::frame::{FrameConstraint, PageLayout, PaperSize};
use crate::hyphenation::HyphenationDictionary;
use crate::length::Length;
use crate::linebreak::{self, BreakResult, LinebreakSettings};
use crate::measurement::Measurement;
use crate::node::{self, GlyphData, NNode, Node, VBox};
use crate::nodemaker::{self, Item, NodeMakerOptions, PunctSpace, Token};
use crate::pagebuilder::{PageBreakSettings, PageBuilder};
use crate::pdf::{Bookmark, PdfConfig, PdfError, PdfOutputter};
use crate::shaper::{self, GlyphItem, Shaper, SpaceSettings};

// ---------------------------------------------------------------------------
// TextAlign
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    #[default]
    Justify,
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum BuilderError {
    Font(FontError),
    Pdf(PdfError),
    NoFont(String),
    Layout(String),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Font(e) => write!(f, "{e}"),
            Self::Pdf(e) => write!(f, "{e}"),
            Self::NoFont(name) => write!(f, "no font registered with name \"{name}\""),
            Self::Layout(msg) => write!(f, "layout error: {msg}"),
        }
    }
}

impl std::error::Error for BuilderError {}

impl From<FontError> for BuilderError {
    fn from(e: FontError) -> Self {
        Self::Font(e)
    }
}

impl From<PdfError> for BuilderError {
    fn from(e: PdfError) -> Self {
        Self::Pdf(e)
    }
}

// ---------------------------------------------------------------------------
// FontEntry (internal)
// ---------------------------------------------------------------------------

struct RegisteredFont {
    spec: FontSpec,
    face: Arc<FontFace>,
}

// ---------------------------------------------------------------------------
// TextRun (internal)
// ---------------------------------------------------------------------------

struct TextRun {
    text: String,
    font_name: String,
    color: Option<Color>,
    language: String,
    tokens: NodeMakerOptions,
    letter_space: Option<Length>,
}

/// Paragraph material in the order it was added: text still to be shaped,
/// ready-made nodes, and boxes of further material set at natural width.
enum Inline {
    Text(TextRun),
    Node(Box<Node>),
    Box(Vec<Inline>),
}

// ---------------------------------------------------------------------------
// RunningText — a header or footer line, typeset afresh on every page
// ---------------------------------------------------------------------------

/// One line of running text for the header or footer frame. Each run is
/// `(registered font name, text)`; the placeholders `{page}` and `{pages}`
/// are replaced per page. The line is set in its own direction and
/// alignment, independent of the body's.
#[derive(Debug, Clone)]
pub struct RunningText {
    pub runs: Vec<(String, String)>,
    pub align: TextAlign,
    pub direction: Direction,
    pub color: Option<Color>,
}

impl RunningText {
    pub fn new(font: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            runs: vec![(font.into(), text.into())],
            align: TextAlign::Left,
            direction: Direction::LTR,
            color: None,
        }
    }

    fn resolved(&self, page: usize, pages: usize, language: &str) -> Vec<Inline> {
        let mut line = vec![Inline::Node(Box::new(Node::zerohbox())), Inline::Node(Box::new(Node::glue(Length::zero())))];
        line.extend(self.runs.iter().map(|(font, text)| {
            Inline::Text(TextRun {
                text: text
                    .replace("{page}", &page.to_string())
                    .replace("{pages}", &pages.to_string()),
                font_name: font.clone(),
                color: self.color,
                language: language.to_string(),
                tokens: NodeMakerOptions::for_language(language),
                letter_space: None,
            })
        }));
        line
    }
}

/// Baseline-to-baseline line spacing (`document.baselineskip` and
/// `document.lineskip` in SILE).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineSkip {
    pub skip: Length,
    /// Minimum gap between one line's depth and the next line's height.
    pub lineskip: f64,
}

impl BaselineSkip {
    fn leading_for(&self, height: f64, previous_depth: Option<f64>) -> Node {
        let Some(previous_depth) = previous_depth else {
            return Node::vglue(Length::zero());
        };
        let gap = self.skip.length.to_pt().unwrap_or(0.0) - height - previous_depth;
        if gap > self.lineskip {
            Node::vglue(Length::new(Measurement::pt(gap), self.skip.stretch, self.skip.shrink))
        } else {
            Node::vglue(Length::pt(self.lineskip))
        }
    }
}

/// The glue around every line (SILE's `document.lskip` and `document.rskip`)
/// and after the last one (`typesetter.parfillskip`). Alignment is expressed
/// through these, as in SILE.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSkips {
    pub left: Length,
    pub right: Length,
    pub par_fill: Length,
}

impl Default for LineSkips {
    fn default() -> Self {
        Self {
            left: Length::zero(),
            right: Length::zero(),
            par_fill: Length::new(Measurement::pt(0.0), Measurement::pt(10_000.0), Measurement::pt(0.0)),
        }
    }
}

impl LineSkips {
    /// Change alignment, keeping the fixed part of both margins (SILE's
    /// `\raggedright`, `\raggedleft`, `\center` and `\justified`).
    pub fn aligned(self, align: TextAlign) -> Self {
        let fixed = |l: Length| Length::from(l.length);
        let fill = |l: Length| Length::new(l.length, Measurement::pt(node::INFINITY), Measurement::pt(0.0));
        let (left, right) = match align {
            TextAlign::Justify => (fixed(self.left), fixed(self.right)),
            TextAlign::Left => (fixed(self.left), fill(self.right)),
            TextAlign::Right => (fill(self.left), fixed(self.right)),
            TextAlign::Center => (fill(self.left), fill(self.right)),
        };
        let par_fill = if align == TextAlign::Justify { Self::default().par_fill } else { Length::zero() };
        Self { left, right, par_fill }
    }
}

struct LaidOut {
    pages: Vec<crate::pagebuilder::Page>,
    layout: PageLayout,
    fonts: std::collections::BTreeMap<String, RegisteredFont>,
    bookmarks: Vec<Bookmark>,
    pdf_config: PdfConfig,
}

// ---------------------------------------------------------------------------
// DocumentBuilder
// ---------------------------------------------------------------------------

pub struct DocumentBuilder {
    // Page geometry
    paper: PaperSize,
    margins: [f64; 4], // top, right, bottom, left
    header_height: f64,
    footer_height: f64,
    frame_gap: f64,

    // Font system
    font_db: FontDatabase,
    fonts: std::collections::BTreeMap<String, RegisteredFont>,
    shaper: Box<dyn Shaper>,
    current_font: Option<String>,

    // Hyphenation
    hyphenation: HyphenationDictionary,
    language: String,

    // Style state
    current_color: Option<Color>,
    direction: Direction,
    skips: LineSkips,

    // Paragraph state
    paragraph: Vec<Inline>,
    open_boxes: Vec<Vec<Inline>>,
    paragraph_indent: f64,
    current_indent: Option<f64>,
    paragraph_skip: Length,
    leading: f64,
    baseline_skip: Option<BaselineSkip>,
    previous_depth: Option<f64>,
    space_settings: SpaceSettings,
    obey_spaces: bool,
    fixed_nbsp: bool,
    letter_space: Option<Length>,

    // Settings
    linebreak_settings: LinebreakSettings,
    page_break_settings: PageBreakSettings,

    // Accumulated vertical content
    vertical_queue: Vec<Node>,

    // Running header/footer (need header_height / footer_height > 0)
    header: Option<RunningText>,
    footer: Option<RunningText>,

    // PDF config
    pdf_config: PdfConfig,
    bookmarks: Vec<Bookmark>,
    page_count: usize,
}

impl DocumentBuilder {
    pub fn new(paper: PaperSize) -> Self {
        Self {
            paper,
            margins: [72.0; 4],
            header_height: 0.0,
            footer_height: 0.0,
            frame_gap: 0.0,
            font_db: FontDatabase::new(),
            fonts: std::collections::BTreeMap::new(),
            shaper: shaper::default_shaper(),
            current_font: None,
            hyphenation: HyphenationDictionary::new(),
            language: "en".to_string(),
            current_color: None,
            direction: Direction::LTR,
            skips: LineSkips::default(),
            paragraph: Vec::new(),
            open_boxes: Vec::new(),
            paragraph_indent: 20.0,
            current_indent: None,
            paragraph_skip: Length::zero(),
            leading: 2.0,
            baseline_skip: None,
            previous_depth: None,
            space_settings: SpaceSettings::default(),
            obey_spaces: false,
            fixed_nbsp: false,
            letter_space: None,
            linebreak_settings: LinebreakSettings::default(),
            page_break_settings: PageBreakSettings::default(),
            vertical_queue: Vec::new(),
            header: None,
            footer: None,
            pdf_config: PdfConfig::default(),
            bookmarks: Vec::new(),
            page_count: 0,
        }
    }

    // -- Page geometry -------------------------------------------------------

    pub fn set_page_size(&mut self, paper: PaperSize) -> &mut Self {
        self.paper = paper;
        self
    }

    pub fn set_margins(&mut self, top: f64, right: f64, bottom: f64, left: f64) -> &mut Self {
        self.margins = [top, right, bottom, left];
        self
    }

    pub fn set_header_height(&mut self, height: f64, gap: f64) -> &mut Self {
        self.header_height = height;
        self.frame_gap = gap;
        self
    }

    pub fn set_footer_height(&mut self, height: f64, gap: f64) -> &mut Self {
        self.footer_height = height;
        self.frame_gap = gap;
        self
    }

    /// The running header, set once per page into the header frame
    /// (`set_header_height` must reserve room for it).
    pub fn set_header(&mut self, header: RunningText) -> &mut Self {
        self.header = Some(header);
        self
    }

    /// The running footer (see `set_header`).
    pub fn set_footer(&mut self, footer: RunningText) -> &mut Self {
        self.footer = Some(footer);
        self
    }

    // -- Font management -----------------------------------------------------

    pub fn load_system_fonts(&mut self) -> &mut Self {
        self.font_db.load_system_fonts();
        self
    }

    pub fn load_font_file(
        &mut self,
        name: impl Into<String>,
        path: impl AsRef<Path>,
        spec: FontSpec,
    ) -> Result<&mut Self, BuilderError> {
        let data = std::fs::read(path.as_ref())
            .map_err(|e| FontError::Io(e.to_string()))?;
        self.load_font_data(name, data, spec)
    }

    pub fn load_font_data(
        &mut self,
        name: impl Into<String>,
        data: Vec<u8>,
        spec: FontSpec,
    ) -> Result<&mut Self, BuilderError> {
        let face = Arc::new(FontFace::from_bytes(data, 0)?);
        let name = name.into();
        self.fonts.insert(name, RegisteredFont { spec, face });
        Ok(self)
    }

    pub fn load_font_by_family(
        &mut self,
        name: impl Into<String>,
        spec: FontSpec,
    ) -> Result<&mut Self, BuilderError> {
        let face = self.font_db.resolve(&spec)?;
        let name = name.into();
        self.fonts.insert(name, RegisteredFont { spec, face });
        Ok(self)
    }

    pub fn set_font(&mut self, name: impl Into<String>) -> &mut Self {
        self.current_font = Some(name.into());
        self
    }

    pub fn set_font_size(&mut self, size: f64) -> &mut Self {
        if let Some(ref name) = self.current_font.clone()
            && let Some(entry) = self.fonts.get_mut(name) {
                entry.spec.size = size;
            }
        self
    }

    // -- Language and hyphenation --------------------------------------------

    pub fn set_language(&mut self, lang: impl Into<String>) -> &mut Self {
        self.language = lang.into();
        self.hyphenation.load_language(&self.language);
        self
    }

    /// Keep every space as its own glue, including leading ones.
    pub fn set_obey_spaces(&mut self, obey: bool) -> &mut Self {
        self.obey_spaces = obey;
        self
    }

    /// Treat U+00A0 as an ordinary glyph rather than a space-wide kern.
    pub fn set_fixed_nbsp(&mut self, fixed: bool) -> &mut Self {
        self.fixed_nbsp = fixed;
        self
    }

    /// Space added between every pair of characters.
    pub fn set_letter_space(&mut self, space: Option<Length>) -> &mut Self {
        self.letter_space = space;
        self
    }

    // -- Style ---------------------------------------------------------------

    pub fn set_color(&mut self, color: Color) -> &mut Self {
        self.current_color = Some(color);
        self
    }

    pub fn clear_color(&mut self) -> &mut Self {
        self.current_color = None;
        self
    }

    // -- Paragraph settings --------------------------------------------------

    /// Indent for paragraphs that start from now on.
    pub fn set_paragraph_indent(&mut self, indent: f64) -> &mut Self {
        self.paragraph_indent = indent;
        self
    }

    /// Indent for the next paragraph only (SILE's `current.parindent`);
    /// `Some(0.0)` is `\noindent`.
    pub fn set_current_indent(&mut self, indent: Option<f64>) -> &mut Self {
        self.current_indent = indent;
        self
    }

    /// Vertical glue after each paragraph (SILE's `document.parskip`).
    pub fn set_paragraph_skip(&mut self, skip: impl Into<Length>) -> &mut Self {
        self.paragraph_skip = skip.into();
        self
    }

    /// Space lines TeX/SILE style, baseline to baseline, instead of adding a
    /// fixed `leading` between them. Overrides `set_leading`.
    pub fn set_baseline_skip(&mut self, baseline_skip: Option<BaselineSkip>) -> &mut Self {
        self.baseline_skip = baseline_skip;
        self
    }

    pub fn set_leading(&mut self, leading: f64) -> &mut Self {
        self.leading = leading;
        self
    }

    pub fn set_direction(&mut self, direction: Direction) -> &mut Self {
        self.direction = direction;
        self
    }

    pub fn set_alignment(&mut self, alignment: TextAlign) -> &mut Self {
        self.skips = self.skips.aligned(alignment);
        self
    }

    pub fn line_skips(&self) -> LineSkips {
        self.skips
    }

    pub fn set_line_skips(&mut self, skips: LineSkips) -> &mut Self {
        self.skips = skips;
        self
    }

    pub fn set_space_settings(&mut self, settings: SpaceSettings) -> &mut Self {
        self.space_settings = settings;
        self
    }

    pub fn linebreak_settings_mut(&mut self) -> &mut LinebreakSettings {
        &mut self.linebreak_settings
    }

    pub fn page_break_settings_mut(&mut self) -> &mut PageBreakSettings {
        &mut self.page_break_settings
    }

    // -- Text ----------------------------------------------------------------

    /// Add text to the current paragraph, starting one if needed. Leading
    /// whitespace at the start of a paragraph or box is dropped.
    pub fn add_text(&mut self, text: impl Into<String>) -> &mut Self {
        let mut text = text.into().replace("\r\n", " ").replace(['\n', '\t'], " ");
        if self.current_list().is_empty() && !self.obey_spaces {
            text = text.trim_start().to_string();
            if text.is_empty() {
                return self;
            }
        }
        let tokens = NodeMakerOptions {
            obey_spaces: self.obey_spaces,
            fixed_nbsp: self.fixed_nbsp,
            letterspace: self.letter_space.is_some(),
            ..NodeMakerOptions::for_language(&self.language)
        };
        let run = TextRun {
            text,
            font_name: self.current_font.clone().unwrap_or_default(),
            color: self.current_color,
            language: self.language.clone(),
            tokens,
            letter_space: self.letter_space,
        };
        self.push_inline(Inline::Text(run));
        self
    }

    /// Breakable space that disappears at line breaks.
    pub fn add_glue(&mut self, width: impl Into<Length>) -> &mut Self {
        self.push_inline(Inline::Node(Box::new(Node::glue(width.into()))));
        self
    }

    /// Infinitely stretchable space that survives line breaks (`\hfill`).
    pub fn add_hfill(&mut self) -> &mut Self {
        let mut fill = Node::hfillglue(Length::zero());
        if let Node::HFillGlue(g) = &mut fill {
            g.explicit = true;
        }
        self.push_inline(Inline::Node(Box::new(fill)));
        self
    }

    /// Unbreakable fixed space.
    pub fn add_kern(&mut self, width: impl Into<Length>) -> &mut Self {
        self.push_inline(Inline::Node(Box::new(Node::kern(width.into()))));
        self
    }

    /// A line break penalty inside a paragraph, or a page break penalty
    /// between paragraphs. `-10000` forces the break, `10000` forbids it.
    pub fn add_penalty(&mut self, penalty: i32) -> &mut Self {
        if self.paragraph.is_empty() && self.open_boxes.is_empty() {
            self.vertical_queue.push(Node::penalty(penalty));
        } else {
            self.push_inline(Inline::Node(Box::new(Node::penalty(penalty))));
        }
        self
    }

    /// Start collecting material into a box set at its natural width;
    /// everything added until `end_hbox` goes inside it.
    pub fn start_hbox(&mut self) -> &mut Self {
        self.open_boxes.push(Vec::new());
        self
    }

    pub fn end_hbox(&mut self) -> &mut Self {
        if let Some(content) = self.open_boxes.pop() {
            self.push_inline(Inline::Box(content));
        }
        self
    }

    fn current_list(&self) -> &[Inline] {
        self.open_boxes.last().unwrap_or(&self.paragraph)
    }

    /// SILE's `initline`: a paragraph opens with a zero box and its indent.
    fn push_inline(&mut self, item: Inline) {
        if let Some(open) = self.open_boxes.last_mut() {
            open.push(item);
            return;
        }
        if self.paragraph.is_empty() {
            let indent = self.current_indent.take().unwrap_or(self.paragraph_indent);
            self.paragraph.push(Inline::Node(Box::new(Node::zerohbox())));
            self.paragraph.push(Inline::Node(Box::new(Node::glue(Length::pt(indent)))));
        }
        self.paragraph.push(item);
    }

    /// End the paragraph and add the paragraph skip after it. Does nothing
    /// right after vertical glue or a penalty, so skips are not doubled.
    pub fn new_paragraph(&mut self) -> Result<&mut Self, BuilderError> {
        if self.paragraph.is_empty()
            && self.vertical_queue.last().is_some_and(|n| n.is_vglue() || n.is_penalty())
        {
            return Ok(self);
        }
        self.current_indent = None;
        self.leave_hmode()?;
        self.vertical_queue.push(Node::vglue(self.paragraph_skip));
        Ok(self)
    }

    /// Break the pending paragraph into lines without ending it as a
    /// paragraph (no paragraph skip).
    fn leave_hmode(&mut self) -> Result<(), BuilderError> {
        self.open_boxes.clear();
        if self.paragraph.is_empty() {
            return Ok(());
        }
        let inlines = std::mem::take(&mut self.paragraph);
        let nodes = self.typeset_paragraph(&inlines)?;
        self.vertical_queue.extend(nodes);
        Ok(())
    }

    // -- Vertical material ---------------------------------------------------

    pub fn add_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode()?;
        self.vertical_queue.push(Node::vglue(amount.into()));
        Ok(self)
    }

    /// Vertical space kept even at the top or bottom of a page (SILE's
    /// `\skip` and `\smallskip` family).
    pub fn add_explicit_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode()?;
        let mut glue = Node::vglue(amount.into());
        if let Node::VGlue(g) = &mut glue {
            g.explicit = true;
        }
        self.vertical_queue.push(glue);
        Ok(self)
    }

    pub fn add_vfill(&mut self) -> Result<&mut Self, BuilderError> {
        self.leave_hmode()?;
        let mut fill = Node::vfillglue(Length::zero());
        if let Node::VFillGlue(g) = &mut fill {
            g.explicit = true;
        }
        self.vertical_queue.push(fill);
        Ok(self)
    }

    pub fn add_page_break(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vertical_penalty(-10_000)
    }

    /// A page break penalty, ending any pending paragraph first.
    pub fn add_vertical_penalty(&mut self, penalty: i32) -> Result<&mut Self, BuilderError> {
        self.leave_hmode()?;
        self.vertical_queue.push(Node::penalty(penalty));
        Ok(self)
    }

    pub fn add_rule(&mut self, width: f64, height: f64) -> Result<&mut Self, BuilderError> {
        self.leave_hmode()?;
        let vbox = VBox {
            width: Length::pt(width),
            height: Length::pt(height),
            depth: Length::zero(),
            nodes: vec![Node::hbox(width, height, 0.0)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
        };
        self.vertical_queue.push(Node::VBox(vbox));
        Ok(self)
    }

    // -- Bookmarks and links ------------------------------------------------

    pub fn add_bookmark(&mut self, title: impl Into<String>, level: u32) -> &mut Self {
        self.bookmarks.push(Bookmark {
            title: title.into(),
            page_index: self.page_count,
            level,
            y_position: self.margins[0],
        });
        self
    }

    // -- PDF config ----------------------------------------------------------

    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.pdf_config.title = Some(title.into());
        self
    }

    pub fn set_author(&mut self, author: impl Into<String>) -> &mut Self {
        self.pdf_config.author = Some(author.into());
        self
    }

    pub fn set_subject(&mut self, subject: impl Into<String>) -> &mut Self {
        self.pdf_config.subject = Some(subject.into());
        self
    }

    pub fn set_compress(&mut self, compress: bool) -> &mut Self {
        self.pdf_config.compress = compress;
        self
    }

    // -- Render --------------------------------------------------------------

    pub fn render(self) -> Result<Vec<u8>, BuilderError> {
        let laid = self.lay_out()?;
        let mut pdf = PdfOutputter::new(laid.pdf_config);
        for (name, entry) in &laid.fonts {
            pdf.register_font(name, Arc::clone(&entry.face));
        }
        for bm in laid.bookmarks {
            pdf.add_bookmark(bm);
        }
        pdf.render_pages(&laid.pages, &laid.layout);
        Ok(pdf.finish()?)
    }

    /// Lay the document out and describe it in SILE's debug outputter format,
    /// for comparing layout against SILE's regression test expectations.
    pub fn render_debug(self) -> Result<String, BuilderError> {
        let laid = self.lay_out()?;
        let mut trace = crate::trace::TraceCanvas::new(laid.layout.paper);
        for (name, entry) in &laid.fonts {
            let family = entry.spec.family.clone().unwrap_or_default();
            trace.register_font(name, family, &entry.spec);
        }
        crate::render::draw_pages(&laid.pages, &laid.layout, &mut trace);
        Ok(trace.finish())
    }

    fn lay_out(mut self) -> Result<LaidOut, BuilderError> {
        self.leave_hmode()?;

        // SILE's class:finish fills the last page and ejects it.
        self.add_vfill()?;
        self.vertical_queue.push(Node::penalty(-20_000));

        // Build page layout (header/footer frames included when reserved)
        let layout = self.build_layout()?;

        let content_frame_id = layout
            .content_frame_id()
            .ok_or_else(|| BuilderError::Layout("no content frame".to_string()))?;


        // Build pages
        let mut page_builder = PageBuilder::new(self.page_break_settings.clone());
        page_builder.enqueue_many(std::mem::take(&mut self.vertical_queue));
        let mut pages = page_builder.build_pages(&layout, content_frame_id);

        // Running header/footer: typeset per page (page numbers differ)
        let total = pages.len();
        for (name, running) in [("header", self.header.clone()), ("footer", self.footer.clone())] {
            let (Some(running), Some(frame_id)) = (running, layout.frame_id_by_name(name)) else {
                continue;
            };
            let hsize = layout.frame(frame_id).width();
            for page in pages.iter_mut() {
                let line = running.resolved(page.number, total, &self.language);
                let skips = LineSkips::default().aligned(running.align);
                let nodes = self.typeset_inlines(&line, hsize, running.direction, skips, &mut None)?;
                page.add_frame_content(frame_id, nodes);
            }
        }

        Ok(LaidOut {
            pages,
            layout,
            fonts: self.fonts,
            bookmarks: self.bookmarks,
            pdf_config: self.pdf_config,
        })
    }

    // -- Internal: paragraph typesetting ------------------------------------

    fn typeset_paragraph(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        let layout = self.build_layout()?;
        let content_frame_id = layout
            .content_frame_id()
            .ok_or_else(|| BuilderError::Layout("no content frame".to_string()))?;
        let hsize = layout.frame(content_frame_id).width();
        let mut previous_depth = self.previous_depth;
        let nodes =
            self.typeset_inlines(inlines, hsize, self.direction, self.skips, &mut previous_depth)?;
        self.previous_depth = previous_depth;
        Ok(nodes)
    }

    /// Shape, break and package paragraph material into lines of width
    /// `hsize` (SILE's `boxUpNodes`).
    fn typeset_inlines(
        &mut self,
        inlines: &[Inline],
        hsize: f64,
        direction: Direction,
        skips: LineSkips,
        previous_depth: &mut Option<f64>,
    ) -> Result<Vec<Node>, BuilderError> {
        let mut h_nodes = self.shape_inlines(inlines)?;
        while h_nodes.last().is_some_and(Node::is_discardable) {
            h_nodes.pop();
        }
        while h_nodes.first().is_some_and(Node::is_penalty) {
            h_nodes.remove(0);
        }
        if h_nodes.is_empty() {
            return Ok(Vec::new());
        }
        let mut par_fill = Node::glue(skips.par_fill);
        if let Node::Glue(g) = &mut par_fill {
            g.explicit = true;
        }
        h_nodes.push(par_fill);
        h_nodes.push(Node::penalty(-10_000));

        // Pre-hyphenate so we have a single consistent node list for both
        // linebreaking and line building. The linebreaker's internal hyphenation
        // pass modifies its own copy of the node list, making break positions
        // incompatible with the original. By pre-hyphenating we avoid that.
        let h_nodes = self.hyphenate(h_nodes);

        let mut lb_settings = self.linebreak_settings.clone();
        lb_settings.left_skip = skips.left;
        lb_settings.right_skip = skips.right;
        let breaks = linebreak::do_break(&h_nodes, hsize, &lb_settings, None);
        Ok(self.build_lines(&h_nodes, &breaks, direction, skips, previous_depth))
    }

    fn shape_inlines(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        let mut h_nodes = Vec::new();
        for item in inlines {
            match item {
                Inline::Text(run) => h_nodes.extend(self.shape_run(run)?),
                Inline::Node(node) => h_nodes.push((**node).clone()),
                Inline::Box(content) => {
                    let nodes = self.shape_inlines(content)?;
                    let width: f64 = nodes.iter().map(|n| pt_of(&n.width())).sum();
                    let mut hbox = node::HBox::new(
                        Length::pt(width),
                        node::max_node_dim(&nodes, node::Dim::Height),
                        node::max_node_dim(&nodes, node::Dim::Depth),
                    );
                    hbox.nodes = nodes;
                    h_nodes.push(Node::HBox(hbox));
                }
            }
        }
        Ok(h_nodes)
    }

    /// Shape one run and cut it into words, spaces and break penalties
    /// (SILE's unicode node maker).
    fn shape_run(&self, run: &TextRun) -> Result<Vec<Node>, BuilderError> {
        let font_entry = self
            .fonts
            .get(&run.font_name)
            .ok_or_else(|| BuilderError::NoFont(run.font_name.clone()))?;
        let face = Arc::clone(&font_entry.face);
        let spec = font_entry.spec.clone();

        // Shape the entire run at once so the shaping engine can apply
        // inter-word kerning (critical for nastaliq scripts where words
        // overlap horizontally based on their vertical positions).
        let mut glyphs = self.shaper.shape(&run.text, &face, &spec);
        let rtl = spec.direction == Direction::RTL;
        if rtl {
            glyphs.reverse();
        }
        let items: Vec<Item> = glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let start = g.cluster as usize;
                let end = glyphs.get(i + 1).map_or(run.text.len(), |n| n.cluster as usize);
                Item { text: run.text.get(start..end).unwrap_or(""), index: start }
            })
            .collect();

        let mut nodes = Vec::new();
        for token in nodemaker::tokenize(&items, run.tokens) {
            match token {
                Token::Word(range) => {
                    let text: String = items[range.clone()].iter().map(|i| i.text).collect();
                    let mut word = glyphs[range].to_vec();
                    if rtl {
                        word.reverse();
                    }
                    let mut nnode = self.build_nnode(&text, &word, &run.font_name, &spec, run.color);
                    nnode.language = run.language.clone();
                    nodes.push(Node::NNode(nnode));
                }
                Token::Space(i) => nodes.push(Node::glue(self.space_settings.space(glyphs[i].x_advance))),
                Token::NonBreakingSpace => {
                    let width = self.shaper.shape(" ", &face, &spec).iter().map(|g| g.x_advance).sum();
                    nodes.push(Node::kern(self.space_settings.space(width)));
                }
                Token::Penalty(p) => nodes.push(Node::penalty(p)),
                Token::RepeatedHyphen => {
                    let hyphen = self.shaper.shape("-", &face, &spec);
                    let mut nnode = self.build_nnode("-", &hyphen, &run.font_name, &spec, run.color);
                    nnode.language = run.language.clone();
                    nodes.push(Node::discretionary(vec![], vec![Node::NNode(nnode)], vec![]));
                }
                Token::LetterSpace => nodes.push(Node::kern(run.letter_space.unwrap_or_default())),
                Token::PunctSpace(kind) => {
                    let spc: f64 = self.shaper.shape(" ", &face, &spec).iter().map(|g| g.x_advance).sum();
                    let s = self.space_settings;
                    let (w, stretch, shrink) = match kind {
                        PunctSpace::Thin => (0.5 * s.enlargement_factor, 0.0, 0.0),
                        PunctSpace::Colon => (s.enlargement_factor, s.stretch_factor, s.shrink_factor),
                        PunctSpace::Guillemet => (0.8 * s.enlargement_factor, 0.3 * s.stretch_factor, 0.8 * s.shrink_factor),
                    };
                    nodes.push(Node::kern(Length::new(
                        Measurement::pt(w * spc),
                        Measurement::pt(stretch * spc),
                        Measurement::pt(shrink * spc),
                    )));
                }
            }
        }
        Ok(nodes)
    }

    /// Split words at their hyphenation points, with a discretionary at
    /// each (SILE's `hyphenate`).
    fn hyphenate(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Node::NNode(word) = &node else {
                out.push(node);
                continue;
            };
            let lang = if word.language.is_empty() { self.language.clone() } else { word.language.clone() };
            let mut segments = if word.text.chars().count() < self.hyphenation.min_word
                || !word.text.chars().any(char::is_alphabetic)
            {
                Vec::new()
            } else {
                self.hyphenation.hyphenate_word(&word.text, &lang)
            };
            if segments.len() <= 1 || !self.fonts.contains_key(&word.font_key) {
                out.push(node);
                continue;
            }
            let mut pieces = Vec::new();
            let mut syllables = 0;
            for j in 0..segments.len() {
                let point = (j + 1 < segments.len()).then(|| hyphenation_point(&lang, &mut segments, j));
                let nnodes: Vec<Node> = self.text_nodes(&segments[j], word).into_iter().filter(Node::is_nnode).collect();
                syllables += nnodes.len();
                pieces.extend(nnodes);
                if let Some((prebreak, replacement)) = point {
                    let replacement = replacement.map(|r| self.text_nodes(&r, word)).unwrap_or_default();
                    pieces.push(Node::discretionary(self.text_nodes(&prebreak, word), vec![], replacement));
                }
            }
            let parent = Arc::new(node::HyphenatedWord { word: word.clone(), syllables });
            for piece in &mut pieces {
                if let Node::NNode(n) = piece {
                    n.parent = Some(Arc::clone(&parent));
                }
            }
            out.extend(pieces);
        }
        out
    }

    /// Shape a short text in the style of `like` (SILE's `createNnodes`).
    fn text_nodes(&self, text: &str, like: &NNode) -> Vec<Node> {
        let run = TextRun {
            text: text.to_string(),
            font_name: like.font_key.clone(),
            color: like.color,
            language: like.language.clone(),
            tokens: NodeMakerOptions::for_language(&like.language),
            letter_space: None,
        };
        self.shape_run(&run).unwrap_or_default()
    }

    fn build_nnode(
        &self,
        text: &str,
        glyphs: &[GlyphItem],
        font_name: &str,
        spec: &FontSpec,
        color: Option<Color>,
    ) -> NNode {
        let mut width = 0.0;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut glyph_data = Vec::with_capacity(glyphs.len());

        for g in glyphs {
            width += g.x_advance;
            height = height.max(g.height);
            depth = depth.max(g.depth);
            glyph_data.push(GlyphData {
                gid: g.gid,
                x_advance: g.x_advance,
                y_advance: g.y_advance,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
                text: g.text.clone(),
            });
        }

        let mut nnode = NNode::with_glyphs(text, glyph_data, font_name, spec.size, width, height, depth);
        nnode.color = color;
        nnode.language = self.language.clone();
        nnode
    }

    /// Cut the node list at the breakpoints into lines, add the margin
    /// glue and package each line (SILE's `breakpointsToLines`).
    fn build_lines(
        &self,
        h_nodes: &[Node],
        breaks: &[BreakResult],
        direction: Direction,
        skips: LineSkips,
        previous_depth: &mut Option<f64>,
    ) -> Vec<Node> {
        let mut lines: Vec<(VBox, bool)> = Vec::new();
        let mut start = 0;
        let mut postbreak: Vec<Node> = Vec::new();

        for br in breaks {
            if br.position == 0 || h_nodes.is_empty() {
                continue;
            }
            let end = br.position.min(h_nodes.len() - 1);
            if start > end {
                continue;
            }
            let mut line: Vec<Node> = std::mem::take(&mut postbreak);
            line.extend(h_nodes[start..=end].iter().cloned());
            start = end + 1;
            // Lines holding nothing but discardables (e.g. two breaks in a
            // row) are dropped.
            if line.iter().all(Node::is_discardable) {
                continue;
            }
            let broken = matches!(line.last(), Some(Node::Discretionary(_)));
            if let Some(Node::Discretionary(d)) = line.last() {
                let d = d.clone();
                line.pop();
                line.extend(d.prebreak);
                postbreak = d.postbreak;
            }

            let (start_skip, end_skip, start_hang, end_hang) = match direction {
                Direction::RTL => (skips.right, skips.left, br.right, br.left),
                _ => (skips.left, skips.right, br.left, br.right),
            };
            let hung = |skip: Length, hang: f64| {
                if hang > 0.0 { Length::pt(pt_of(&skip) + hang) } else { skip }
            };
            let (start_skip, end_skip) = (hung(start_skip, start_hang), hung(end_skip, end_hang));
            while line.first().is_some_and(Node::is_discardable) {
                line.remove(0);
            }
            let mut line = rejoin_unbroken_words(line);
            let ratio = line_ratio(br.width, &line, start_skip + end_skip);
            line.insert(0, Node::glue(start_skip));
            line.insert(0, Node::zerohbox());
            line.push(Node::glue(end_skip));
            line.push(Node::zerohbox());
            if direction == Direction::RTL {
                line.reverse();
            }

            let vbox = VBox {
                width: Length::pt(br.width),
                height: node::max_node_dim(&line, node::Dim::Height),
                depth: node::max_node_dim(&line, node::Dim::Depth),
                nodes: line,
                ratio,
                misfit: false,
                explicit: false,
            };
            lines.push((vbox, broken));
        }

        let count = lines.len();
        let mut v_nodes = Vec::new();
        for (index, (vbox, broken)) in lines.into_iter().enumerate() {
            let (height, depth) = (pt_of(&vbox.height), pt_of(&vbox.depth));
            if let Some(bls) = self.baseline_skip {
                v_nodes.push(bls.leading_for(height, *previous_depth));
                *previous_depth = Some(depth);
            } else if index > 0 && self.leading > 0.0 {
                v_nodes.push(Node::vglue(Length::new(
                    Measurement::pt(self.leading),
                    Measurement::pt(self.leading * 0.5),
                    Measurement::pt(self.leading * 0.3),
                )));
            }
            v_nodes.push(Node::VBox(vbox));
            let settings = &self.page_break_settings;
            let penalty = if count > 1 && index == 0 {
                settings.widow_penalty
            } else if count > 1 && index == count - 2 {
                settings.orphan_penalty
            } else if broken {
                settings.broken_penalty
            } else {
                0
            };
            if penalty > 0 {
                v_nodes.push(Node::penalty(penalty));
            }
        }
        v_nodes
    }

    /// The page layout from the margins: a content frame, plus a header
    /// frame inside the top margin area and/or a footer frame inside the
    /// bottom one when heights were reserved (each pushes the content frame
    /// inward by its height plus the gap).
    fn build_layout(&self) -> Result<PageLayout, BuilderError> {
        let [top, right, bottom, left] = self.margins;
        let mut layout = PageLayout::new(self.paper);
        let mut constraints = Vec::new();
        let mut body_top = top;
        let mut body_bottom = self.paper.height - bottom;
        if self.header_height > 0.0 {
            let header = layout.add_frame("header");
            constraints.extend([
                FrameConstraint::Left(header, left),
                FrameConstraint::Top(header, top),
                FrameConstraint::Right(header, self.paper.width - right),
                FrameConstraint::Height(header, self.header_height),
            ]);
            body_top += self.header_height + self.frame_gap;
        }
        if self.footer_height > 0.0 {
            let footer = layout.add_frame("footer");
            constraints.extend([
                FrameConstraint::Left(footer, left),
                FrameConstraint::Bottom(footer, self.paper.height - bottom),
                FrameConstraint::Right(footer, self.paper.width - right),
                FrameConstraint::Height(footer, self.footer_height),
            ]);
            body_bottom -= self.footer_height + self.frame_gap;
        }
        let content_id = layout.add_frame("content");
        constraints.extend([
            FrameConstraint::Left(content_id, left),
            FrameConstraint::Top(content_id, body_top),
            FrameConstraint::Right(content_id, self.paper.width - right),
            FrameConstraint::Bottom(content_id, body_bottom),
        ]);
        layout
            .solve(&constraints)
            .map_err(|e| BuilderError::Layout(format!("constraint solver failed: {e:?}")))?;
        Ok(layout)
    }
}

// ---------------------------------------------------------------------------
// Cluster-based text extraction
// ---------------------------------------------------------------------------

/// Put back whole any hyphenated word whose syllables all landed on this
/// line, so it is set as shaped rather than syllable by syllable.
fn rejoin_unbroken_words(line: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        if let Node::NNode(NNode { parent: Some(parent), .. }) = &line[i] {
            let same = |n: &Node| match n {
                Node::NNode(NNode { parent: Some(p), .. }) => Arc::ptr_eq(p, parent),
                Node::Discretionary(_) => true,
                _ => false,
            };
            let mut end = i;
            let mut syllables = 0;
            while end < line.len() && same(&line[end]) {
                if line[end].is_nnode() {
                    syllables += 1;
                }
                end += 1;
            }
            while end > i && line[end - 1].is_discretionary() {
                end -= 1;
            }
            if syllables == parent.syllables {
                out.push(Node::NNode(parent.word.clone()));
                i = end;
                continue;
            }
        }
        out.push(line[i].clone());
        i += 1;
    }
    out
}

fn pt_of(l: &Length) -> f64 {
    l.length.to_pt().unwrap_or(0.0)
}

/// How far the line's glue must stretch (positive) or shrink (negative) to
/// fill `width` (SILE's `computeLineRatio`). Trailing glue does not count.
fn line_ratio(width: f64, line: &[Node], margins: Length) -> f64 {
    let end = line
        .iter()
        .rposition(|n| !(n.is_glue() || n.is_zero()))
        .map_or(0, |i| i + 1);
    let mut natural = margins;
    for n in &line[..end] {
        natural += match n {
            Node::Discretionary(d) => d.replacement.iter().map(Node::width).fold(Length::zero(), |a, b| a + b),
            other => other.width(),
        };
    }
    let left = width - pt_of(&natural);
    let flex = if left < 0.0 { natural.shrink } else { natural.stretch };
    let flex = flex.to_pt().unwrap_or(0.0);
    if flex == 0.0 {
        return 0.0;
    }
    (left / flex).max(-1.0)
}

// ---------------------------------------------------------------------------
// Word splitting
// ---------------------------------------------------------------------------

#[cfg(test)]
fn split_words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    let mut in_word = false;

    for (i, c) in text.char_indices() {
        let is_ws = c.is_whitespace();
        if in_word && is_ws {
            words.push(&text[start..i]);
            words.push(&text[i..i + c.len_utf8()]);
            start = i + c.len_utf8();
            in_word = false;
        } else if !in_word && is_ws {
            words.push(&text[i..i + c.len_utf8()]);
            start = i + c.len_utf8();
        } else if !in_word && !is_ws {
            start = i;
            in_word = true;
        }
    }

    if in_word && start < text.len() {
        words.push(&text[start..]);
    }

    words
}

/// What a hyphenation point after `segments[j]` sets before the break, and
/// what it sets when the word stays whole, adjusting the segments for
/// languages whose spelling changes at a break (SILE's `hyphenateSegments`).
fn hyphenation_point(lang: &str, segments: &mut [String], j: usize) -> (String, Option<String>) {
    let base = lang.split(['-', '_']).next().unwrap_or(lang);
    match base {
        // Catalan punt volat: "l·l" breaks as "l-" / "l".
        "ca" => {
            for (ending, cut, hyphen) in [("ŀ", "ŀ", "l-"), ("Ŀ", "Ŀ", "L-"), ("l·", "l·", "l-"), ("L·", "L·", "L-")] {
                if let Some(stem) = segments[j].strip_suffix(ending) {
                    segments[j] = stem.to_string();
                    return (hyphen.to_string(), Some(cut.to_string()));
                }
            }
        }
        // Turkish: a break at an apostrophe keeps the apostrophe, no hyphen.
        "tr" => {
            if let Some(next) = segments.get(j + 1)
                && let Some(apostrophe) = next.chars().next().filter(|c| matches!(c, '\'' | '’'))
            {
                segments[j + 1] = next[apostrophe.len_utf8()..].to_string();
                return (apostrophe.to_string(), Some(apostrophe.to_string()));
            }
        }
        _ => {}
    }
    ("-".to_string(), None)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn load_any_system_font() -> Option<(Vec<u8>, String)> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let info = db.faces().next()?;
        let family = info.families.first()?.0.clone();
        let id = info.id;
        let mut data_out: Option<(Vec<u8>, u32)> = None;
        db.with_face_data(id, |data, index| {
            data_out = Some((data.to_vec(), index));
        });
        let (data, _index) = data_out?;
        Some((data, family))
    }

    fn builder_with_font() -> Option<DocumentBuilder> {
        let (data, family) = load_any_system_font()?;
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        let spec = FontSpec {
            family: Some(family),
            size: 12.0,
            ..Default::default()
        };
        doc.load_font_data("body", data, spec).ok()?;
        doc.set_font("body");
        Some(doc)
    }

    // -- Robustness ----------------------------------------------------------

    #[test]
    fn overfull_word_does_not_panic() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_margins(72.0, 250.0, 72.0, 250.0);
        doc.add_text("x".repeat(400));
        doc.new_paragraph().unwrap();
        doc.add_text(format!("a b {} c d", "y".repeat(400)));
        assert!(doc.render().is_ok());
    }

    #[test]
    fn output_is_deterministic_with_many_fonts() {
        let render = || {
            let (data, family) = load_any_system_font()?;
            let mut doc = DocumentBuilder::new(PaperSize::A4);
            for name in ["a", "b", "c", "d", "e"] {
                let spec = FontSpec { family: Some(family.clone()), size: 12.0, ..Default::default() };
                doc.load_font_data(name, data.clone(), spec).ok()?;
                doc.set_font(name);
                doc.add_text("Hello world ");
            }
            doc.render().ok()
        };
        let Some(first) = render() else { return };
        for _ in 0..4 {
            assert_eq!(render().unwrap(), first);
        }
    }

    #[test]
    fn debug_trace_lists_each_word() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.add_text("Hello world");
        let trace = doc.render_debug().unwrap();
        assert!(trace.starts_with("Set paper size"));
        assert!(trace.contains("\t(Hello)\n") && trace.contains("\t(world)\n"), "{trace}");
        assert!(trace.ends_with("End page\nFinish\n"));
    }

    // -- Construction --------------------------------------------------------

    #[test]
    fn new_builder() {
        let doc = DocumentBuilder::new(PaperSize::A4);
        assert!((doc.paper.width - 595.276).abs() < 0.01);
    }

    #[test]
    fn set_margins() {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.set_margins(50.0, 60.0, 70.0, 80.0);
        assert_eq!(doc.margins, [50.0, 60.0, 70.0, 80.0]);
    }

    #[test]
    fn set_page_size() {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.set_page_size(PaperSize::LETTER);
        assert!((doc.paper.width - 612.0).abs() < 0.01);
    }

    // -- Font loading --------------------------------------------------------

    #[test]
    fn load_font() {
        let (data, family) = match load_any_system_font() {
            Some(v) => v,
            None => return,
        };
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        let spec = FontSpec {
            family: Some(family),
            size: 12.0,
            ..Default::default()
        };
        assert!(doc.load_font_data("body", data, spec).is_ok());
        assert!(doc.fonts.contains_key("body"));
    }

    #[test]
    fn set_font_size() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_font_size(24.0);
        assert_eq!(doc.fonts["body"].spec.size, 24.0);
    }

    // -- Text and paragraph --------------------------------------------------

    #[test]
    fn add_text_creates_run() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.add_text("  Hello");
        assert!(matches!(&doc.paragraph[0], Inline::Node(n) if n.is_zerohbox()));
        assert!(matches!(&doc.paragraph[2], Inline::Text(r) if r.text == "Hello"));
    }

    #[test]
    fn new_paragraph_flushes_runs() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.add_text("Hello, world.");
        assert!(doc.new_paragraph().is_ok());
        assert!(doc.paragraph.is_empty());
        assert!(!doc.vertical_queue.is_empty());
    }

    #[test]
    fn empty_paragraph_adds_only_discardable_skip() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        assert!(doc.new_paragraph().is_ok());
        assert!(doc.new_paragraph().is_ok());
        assert_eq!(doc.vertical_queue.len(), 1);
        assert!(doc.vertical_queue[0].is_discardable());
    }

    // -- Full render ---------------------------------------------------------

    #[test]
    fn render_hello_world() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_title("Hello");
        doc.add_text("Hello, world.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.len() > 100);
    }

    #[test]
    fn render_multi_paragraph() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.add_text("First paragraph with some text.");
        doc.new_paragraph().unwrap();
        doc.add_text("Second paragraph with more text.");
        doc.new_paragraph().unwrap();
        doc.add_text("Third paragraph wrapping it up.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_long_text() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        let text = "To Sherlock Holmes she is always the woman. I have seldom heard \
                    him mention her under any other name. In his eyes she eclipses \
                    and predominates the whole of her sex. It was not that he felt \
                    any emotion akin to love for Irene Adler. All emotions, and that \
                    one particularly, were abhorrent to his cold, precise but \
                    admirably balanced mind. He was, I take it, the most perfect \
                    reasoning and observing machine that the world has seen, but as \
                    a lover he would have placed himself in a false position.";
        doc.add_text(text);
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_color() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_color(Color::Rgb { r: 1.0, g: 0.0, b: 0.0 });
        doc.add_text("Red text.");
        doc.clear_color();
        doc.add_text(" Normal text.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_page_break() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.add_text("Page one content.");
        doc.new_paragraph().unwrap();
        doc.add_page_break().unwrap();
        doc.add_text("Page two content.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_metadata() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_title("My Document")
            .set_author("Test Author")
            .set_subject("Testing")
            .set_compress(false);
        doc.add_text("Content.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_custom_margins() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_margins(36.0, 36.0, 36.0, 36.0);
        doc.add_text("Narrow margins.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_paragraph_indent() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_paragraph_indent(40.0);
        doc.add_text("Indented paragraph.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_paragraph_skip() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_paragraph_skip(12.0);
        doc.add_text("First paragraph.");
        doc.new_paragraph().unwrap();
        doc.add_text("Second paragraph.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_with_bookmark() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.add_bookmark("Chapter 1", 0);
        doc.add_text("Chapter content.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_empty_document() {
        let doc = DocumentBuilder::new(PaperSize::A4);
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_multiple_fonts() {
        let (data, family) = match load_any_system_font() {
            Some(v) => v,
            None => return,
        };
        let mut doc = DocumentBuilder::new(PaperSize::A4);

        let spec1 = FontSpec {
            family: Some(family.clone()),
            size: 12.0,
            ..Default::default()
        };
        doc.load_font_data("body", data.clone(), spec1).unwrap();

        let spec2 = FontSpec {
            family: Some(family),
            size: 18.0,
            weight: crate::font::FontWeight::BOLD,
            ..Default::default()
        };
        doc.load_font_data("heading", data, spec2).unwrap();

        doc.set_font("heading");
        doc.add_text("Heading Text");
        doc.new_paragraph().unwrap();

        doc.set_font("body");
        doc.add_text("Body text in normal size.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn render_letter_size() {
        let mut doc = match builder_with_font() {
            Some(d) => d,
            None => return,
        };
        doc.set_page_size(PaperSize::LETTER);
        doc.add_text("US Letter content.");
        let pdf = doc.render().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    // -- Word splitting tests ------------------------------------------------

    #[test]
    fn split_words_simple() {
        let words = split_words("Hello World");
        assert_eq!(words, vec!["Hello", " ", "World"]);
    }

    #[test]
    fn split_words_single() {
        let words = split_words("Hello");
        assert_eq!(words, vec!["Hello"]);
    }

    #[test]
    fn split_words_multiple_spaces() {
        let words = split_words("Hello  World");
        // Two spaces become two separator entries
        assert_eq!(words, vec!["Hello", " ", " ", "World"]);
    }

    #[test]
    fn split_words_empty() {
        let words = split_words("");
        assert!(words.is_empty());
    }

    #[test]
    fn split_words_only_spaces() {
        let words = split_words("   ");
        assert!(words.is_empty() || words.iter().all(|w| w.trim().is_empty()));
    }

    #[test]
    fn split_words_leading_space() {
        let words = split_words(" Hello");
        // Leading space is handled
        assert!(words.contains(&"Hello"));
    }

    #[test]
    fn split_words_trailing_space() {
        let words = split_words("Hello ");
        assert_eq!(words[0], "Hello");
    }
}
