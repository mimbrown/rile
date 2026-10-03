use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use unicode_linebreak::{BreakClass, break_property};

use crate::color::Color;
use crate::counter::{MultilevelCounter, PageNumber};
use crate::font::{Direction, FontDatabase, FontError, FontFace, FontSpec, FontStyle, FontWeight};
use crate::class::{DocumentClass, PageTemplate};
use crate::frame::PaperSize;
use crate::framespec::{self, Flow, FrameDirection, FrameGeometry, FrameSpec};
use crate::hyphenation::HyphenationDictionary;
use crate::insertion::{InsertionClass, PageInsertions, Stack};
use crate::length::Length;
use crate::linebreak::{self, BreakResult, LinebreakSettings};
use crate::measurement::Measurement;
use crate::node::{self, GlyphData, Ink, Leader, LinerStyle, LinkDest, NNode, Node, Stroke, VBox};
use crate::nodemaker::{self, Item, NodeMakerOptions, PunctSpace, Token};
use crate::pagebuilder::{self, Page, PageBreakSettings, Underlay};
use crate::image::{Background, BackgroundFill};
use crate::pdf::{Bookmark, PdfConfig, PdfError, PdfOutputter};
use crate::references::{self, CrossReferences, IndexMark, IndexPage, Label, TocEntry};
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
    InvalidMetadata(String),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Font(e) => write!(f, "{e}"),
            Self::Pdf(e) => write!(f, "{e}"),
            Self::NoFont(name) => write!(f, "no font registered with name \"{name}\""),
            Self::Layout(msg) => write!(f, "layout error: {msg}"),
            Self::InvalidMetadata(msg) => write!(f, "invalid PDF metadata: {msg}"),
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

/// SILE's italic correction: space added where text changes between
/// italic and upright, as a glue if either side has a break opportunity,
/// else as a kern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItalicCorrection {
    /// Take the correction out of a punctuation space at the change, as
    /// French puts before `!` (SILE's
    /// `typesetter.italicCorrection.punctuation`).
    pub punctuation: bool,
}

impl Default for ItalicCorrection {
    fn default() -> Self {
        Self { punctuation: true }
    }
}

/// A glyph's advance and ink extents in points.
#[derive(Debug, Clone, Copy)]
struct GlyphShape {
    width: f64,
    ink_width: f64,
    x_bearing: f64,
    height: f64,
    depth: f64,
}

/// The ends of a shaped run: each end's glyph, whether glue comes between
/// it and the run's edge, and the width of a punctuation space there.
struct RunEdges {
    italic: bool,
    first: (Option<GlyphShape>, bool, Option<f64>),
    last: (Option<GlyphShape>, bool, Option<f64>),
}

impl ItalicCorrection {
    fn between(&self, prev: &RunEdges, cur: &RunEdges) -> Option<Node> {
        let ((Some(p), p_glue, p_punct), (Some(c), c_glue, c_punct)) = (prev.last, cur.first) else { return None };
        let offset = if prev.italic && !cur.italic {
            if p.height <= 0.0 {
                return None;
            }
            let d = p.ink_width + p.x_bearing;
            let delta = if d > p.width { d - p.width } else { 0.0 };
            let offset = if p.height <= c.height { delta } else { delta * c.height / p.height };
            match c_punct.filter(|_| self.punctuation) {
                Some(w) => (offset - w).max(0.0),
                None => offset,
            }
        } else if !prev.italic && cur.italic {
            if p.depth <= 0.0 {
                return None;
            }
            let delta = (-c.x_bearing).max(0.0);
            let offset = if p.depth >= c.depth { delta } else { delta * p.depth / c.depth };
            match p_punct.filter(|_| self.punctuation) {
                Some(w) if w - offset <= 0.0 => 0.0,
                _ => offset,
            }
        } else {
            return None;
        };
        let width = Length::pt(offset);
        (offset != 0.0).then(|| if p_glue || c_glue { Node::glue(width) } else { Node::kern(width) })
    }
}

// ---------------------------------------------------------------------------
// TextRun (internal)
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct TextRun {
    text: String,
    font_name: String,
    color: Option<Color>,
    language: String,
    tokens: NodeMakerOptions,
    letter_space: Option<Length>,
    tracking: Option<f64>,
    /// Fonts tried in turn for characters the font lacks.
    fallbacks: Vec<String>,
    /// Opens a paragraph with a dialogue dash whose space is fixed.
    speaker_change: bool,
    /// Embedding level relative to the paragraph's, once bidi has split it.
    bidi_level: Option<u8>,
    tag: Option<u32>,
}

/// Paragraph material in the order it was added: text still to be shaped,
/// ready-made nodes, and boxes of further material set at natural width.
#[derive(Clone)]
enum Inline {
    Text(TextRun),
    Node(Box<Node>),
    Box(Group, Vec<Inline>),
    /// Prebreak, postbreak and replacement text.
    Discretionary(Box<[Option<TextRun>; 3]>),
}

/// How material collected between `start_*` and `end_hbox` is set.
#[derive(Clone)]
enum Group {
    HBox,
    /// Glue of this width (infinite when `None`) filled with copies of the box.
    Leaders(Option<Length>),
    Liner(LinerStyle),
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
                tracking: None,
                fallbacks: Vec::new(),
                speaker_change: false,
                bidi_level: None,
                tag: None,
            })
        }));
        line
    }
}

/// Baseline-to-baseline line spacing (`document.baselineskip` and
/// `document.lineskip` in SILE).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineSkip {
    /// `em` resolves against the font in force when lines are spaced.
    pub skip: Length,
    /// Minimum gap between one line's depth and the next line's height.
    pub lineskip: f64,
}

impl BaselineSkip {
    /// The skip with `em` taken as `em` points.
    pub fn skip_at(&self, em: f64) -> Length {
        resolve_em(self.skip, em)
    }

    fn leading_for(&self, height: f64, previous_depth: Option<f64>, em: f64) -> Node {
        let Some(previous_depth) = previous_depth else {
            return Node::vglue(Length::zero());
        };
        let skip = resolve_em(self.skip, em);
        let gap = skip.length.to_pt().unwrap_or(0.0) - height - previous_depth;
        if gap > self.lineskip {
            Node::vglue(Length::new(Measurement::pt(gap), skip.stretch, skip.shrink))
        } else {
            Node::vglue(Length::pt(self.lineskip))
        }
    }
}

/// A font to fall back on, as changes to the current font.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontFallback {
    pub family: Option<String>,
    pub filename: Option<String>,
    /// Absolute, or relative to the current font (`em`, `en`).
    pub size: Option<Measurement>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
    pub features: Option<String>,
}

impl FontFallback {
    fn apply(&self, current: &FontSpec) -> FontSpec {
        let mut spec = current.clone();
        if let Some(family) = &self.family {
            spec.family = Some(family.clone());
            spec.filename = None;
        }
        if let Some(filename) = &self.filename {
            spec.filename = Some(filename.clone());
        }
        if let Some(size) = self.size {
            spec.size = pt_of(&resolve_em(Length::new(size, Measurement::pt(0.0), Measurement::pt(0.0)), current.size));
        }
        if let Some(weight) = self.weight {
            spec.weight = weight;
        }
        if let Some(style) = self.style {
            spec.style = style;
        }
        if let Some(features) = &self.features {
            spec.features = features.clone();
        }
        spec
    }
}

/// How lines are spaced when SILE's `linespacing` package is in use.
/// Lengths may be relative to the font (`em`), resolved when lines are
/// set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineSpacingMethod {
    /// TeX's baselineskip and lineskip.
    Tex,
    /// A fixed distance from baseline to baseline.
    Fixed(Length),
    /// Space between the lowest point of one line and the highest of the
    /// next.
    FitGlyph(Length),
    /// Space between the fonts' descenders on one line and ascenders on
    /// the next.
    FitFont(Length),
    /// CSS's line-height model, from each font's line height.
    Css(Length),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSpacing {
    pub method: LineSpacingMethod,
    /// The first line's baseline is at least this far below the top of the
    /// frame.
    pub minimum_first_line: Length,
}

impl Default for LineSpacing {
    fn default() -> Self {
        Self { method: LineSpacingMethod::Tex, minimum_first_line: Length::zero() }
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

pub use crate::pagebuilder::SUPER_EJECT;

impl AsMut<DocumentBuilder> for DocumentBuilder {
    fn as_mut(&mut self) -> &mut DocumentBuilder {
        self
    }
}

/// Lines on a grid: where the frame's material has got to, and the spacing.
#[derive(Debug, Clone, Copy)]
struct Grid {
    spacing: f64,
    cursor: f64,
}

/// A laid out document, ready to output.
pub type PageHook = Box<dyn FnMut(&mut DocumentBuilder) -> Result<(), BuilderError>>;

pub struct Layout {
    pub pages: Vec<Page>,
    /// The table of contents and labels found on the way.
    pub references: CrossReferences,
    pub(crate) consulted_references: bool,
    paper: PaperSize,
    fonts: BTreeMap<String, RegisteredFont>,
    bookmarks: Vec<Bookmark>,
    pdf_config: PdfConfig,
    structure: Option<crate::structure::StructTree>,
}

impl Layout {
    pub fn render(self) -> Result<Vec<u8>, BuilderError> {
        let mut pdf = PdfOutputter::new(self.pdf_config);
        for (name, entry) in &self.fonts {
            pdf.register_font(name, Arc::clone(&entry.face), entry.face.variations(&entry.spec));
        }
        for bm in self.bookmarks {
            pdf.add_bookmark(bm);
        }
        if let Some(structure) = self.structure {
            pdf.set_structure(structure);
        }
        pdf.render_pages(&self.pages);
        Ok(pdf.finish()?)
    }

    /// Describe the layout in SILE's debug outputter format, for comparing
    /// against SILE's regression test expectations.
    pub fn render_debug(&self) -> String {
        let mut trace = crate::trace::TraceCanvas::new(self.paper);
        for (name, entry) in &self.fonts {
            trace.register_font(name, &entry.spec);
        }
        crate::render::draw_pages(&self.pages, &mut trace);
        trace.finish()
    }
}

/// The settings that shape text and paragraphs, which can be saved and
/// restored as a whole (SILE's settings state).
#[derive(Clone)]
pub struct Settings {
    font: Option<String>,
    language: String,
    color: Option<Color>,
    skips: LineSkips,
    paragraph_indent: f64,
    paragraph_skip: Length,
    leading: f64,
    baseline_skip: Option<BaselineSkip>,
    line_spacing: Option<LineSpacing>,
    space_settings: SpaceSettings,
    obey_spaces: bool,
    fixed_nbsp: bool,
    letter_space: Option<Length>,
    tracking: Option<f64>,
    ethiopic_centered: bool,
    fixed_space_after_dash: bool,
    soft_hyphens: bool,
    italic_correction: Option<ItalicCorrection>,
    replace_apostrophe_at_hyphenation: bool,
    break_width: Option<f64>,
    fallbacks: Vec<FontFallback>,
    /// The fallbacks applied to the current font, as registered fonts.
    fallback_fonts: Vec<String>,
    linebreak_settings: LinebreakSettings,
    math: crate::math::MathSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: None,
            language: "en".to_string(),
            color: None,
            skips: LineSkips::default(),
            paragraph_indent: 20.0,
            paragraph_skip: Length::zero(),
            leading: 2.0,
            baseline_skip: None,
            line_spacing: None,
            space_settings: SpaceSettings::default(),
            obey_spaces: false,
            fixed_nbsp: false,
            letter_space: None,
            tracking: None,
            ethiopic_centered: false,
            fixed_space_after_dash: true,
            soft_hyphens: true,
            italic_correction: None,
            replace_apostrophe_at_hyphenation: false,
            break_width: None,
            fallbacks: Vec::new(),
            fallback_fonts: Vec::new(),
            linebreak_settings: LinebreakSettings::default(),
            math: Default::default(),
        }
    }
}

/// Material recorded by `begin_capture` to be typeset later, possibly
/// elsewhere (a running head, say). Each paragraph keeps the settings it
/// was ended under.
#[derive(Clone, Default)]
pub struct Material {
    items: Vec<Captured>,
}

impl Material {
    fn inlines(&self) -> Vec<Inline> {
        self.items
            .iter()
            .flat_map(|item| match item {
                Captured::Paragraph { inlines, .. } | Captured::Inlines(inlines) => inlines.as_slice(),
                Captured::Vertical(_) => &[],
            })
            .cloned()
            .collect()
    }

    /// The material's text, without its formatting.
    pub fn text(&self) -> String {
        fn collect(inlines: &[Inline], out: &mut String) {
            for inline in inlines {
                match inline {
                    Inline::Text(run) => out.push_str(&run.text),
                    Inline::Box(_, inner) => collect(inner, out),
                    Inline::Discretionary(d) => out.extend(d[2].as_ref().map(|r| r.text.as_str())),
                    Inline::Node(_) => {}
                }
            }
        }
        let mut out = String::new();
        for item in &self.items {
            match item {
                Captured::Paragraph { inlines, .. } | Captured::Inlines(inlines) => collect(inlines, &mut out),
                Captured::Vertical(_) => {}
            }
        }
        out
    }
}

#[derive(Clone)]
enum Captured {
    Paragraph { inlines: Vec<Inline>, settings: Box<Settings> },
    Inlines(Vec<Inline>),
    Vertical(Box<Node>),
}

struct Capture {
    items: Vec<Captured>,
    paragraph: Vec<Inline>,
    open_boxes: Vec<(Group, Vec<Inline>)>,
    current_indent: Option<f64>,
}

struct SavedTypesetter {
    settings: Settings,
    paragraph: Vec<Inline>,
    open_boxes: Vec<(Group, Vec<Inline>)>,
    current_indent: Option<f64>,
    previous_depth: Option<f64>,
    queue: Vec<Node>,
    captures: Vec<Capture>,
    lists: Vec<crate::lists::ListLevel>,
    frame: String,
}

/// What a typesetter is working on, apart from settings, which all share.
#[derive(Default)]
struct FlowState {
    paragraph: Vec<Inline>,
    open_boxes: Vec<(Group, Vec<Inline>)>,
    current_indent: Option<f64>,
    previous_depth: Option<f64>,
    queue: Vec<Node>,
}

/// A typesetter per frame, kept level with each other (SILE's `parallel`
/// package), keyed by name in the order frames are output.
struct Parallel {
    flows: BTreeMap<String, ParallelFlow>,
    active: Option<String>,
}

struct ParallelFlow {
    frame: String,
    flow: FlowState,
    /// How much of the queue is level with the other flows.
    mark: usize,
}

/// The page being filled: its frames and the frame content flows into.
struct PageState {
    page: Page,
    frame: String,
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
    class: Option<Box<dyn DocumentClass>>,
    /// Frames used instead of the class's (SILE's `\switch-master`).
    master: Option<PageTemplate>,
    /// Direction of frames that don't set their own.
    direction: FrameDirection,
    /// Directions set on frames while typesetting (SILE's `\thisframeRTL`).
    frame_directions: BTreeMap<String, FrameDirection>,
    /// Shape as if in a frame of this direction (SILE's `\latin-in-tate`).
    frame_override: Option<FrameDirection>,
    bidi: bool,

    // Font system
    font_db: FontDatabase,
    fonts: BTreeMap<String, RegisteredFont>,
    shaper: Box<dyn Shaper>,

    // Hyphenation
    hyphenation: HyphenationDictionary,

    settings: Settings,
    toplevel: Option<Settings>,

    // Paragraph state
    paragraph: Vec<Inline>,
    open_boxes: Vec<(Group, Vec<Inline>)>,
    current_indent: Option<f64>,
    hanging: Option<(i32, f64)>,
    previous_depth: Option<f64>,
    captures: Vec<Capture>,

    page_break_settings: PageBreakSettings,

    // Page building
    vertical_queue: Vec<Node>,
    pub(crate) lists: crate::lists::Lists,
    pub(crate) tables: Vec<crate::table::TableState>,
    pub(crate) ruby: crate::ruby::Ruby,
    page: Option<PageState>,
    pages: Vec<Page>,
    last_penalty: i32,
    insertion_classes: std::collections::BTreeMap<String, InsertionClass>,
    insertions: PageInsertions,
    counters: BTreeMap<String, i64>,
    multilevel_counters: BTreeMap<String, MultilevelCounter>,
    /// Typesetting states set aside by `push_typesetter`.
    typesetters: Vec<SavedTypesetter>,
    parallel: Option<Parallel>,
    /// Frames added to every page's template (SILE's `class:declareFrame`).
    extra_frames: Vec<FrameSpec>,

    // Running header/footer (need header_height / footer_height > 0)
    header: Option<RunningText>,
    footer: Option<RunningText>,

    // PDF config
    pdf_config: PdfConfig,
    bookmarks: Vec<Bookmark>,
    destinations: usize,
    grid: Option<Grid>,
    grid_debug: Option<f64>,
    messages: crate::messages::Messages,
    background: Option<Background>,
    end_page_hooks: Vec<PageHook>,

    /// What the previous pass found, if there was one.
    previous_references: Option<CrossReferences>,
    consulted_references: std::cell::Cell<bool>,
    references: CrossReferences,
    pub(crate) math_tables: BTreeMap<String, Arc<crate::math::MathTable>>,
    pub(crate) structure: Option<crate::structure::StructTree>,
    pub(crate) untagged: usize,
}

impl DocumentBuilder {
    pub fn new(paper: PaperSize) -> Self {
        Self {
            paper,
            margins: [72.0; 4],
            header_height: 0.0,
            footer_height: 0.0,
            frame_gap: 0.0,
            class: None,
            master: None,
            direction: FrameDirection::LTR,
            frame_directions: BTreeMap::new(),
            frame_override: None,
            bidi: true,
            font_db: FontDatabase::new(),
            fonts: std::collections::BTreeMap::new(),
            shaper: shaper::default_shaper(),
            hyphenation: HyphenationDictionary::new(),
            settings: Settings::default(),
            toplevel: None,
            paragraph: Vec::new(),
            open_boxes: Vec::new(),
            current_indent: None,
            hanging: None,
            previous_depth: None,
            captures: Vec::new(),
            page_break_settings: PageBreakSettings::default(),
            vertical_queue: Vec::new(),
            lists: Default::default(),
            tables: Vec::new(),
            ruby: Default::default(),
            page: None,
            pages: Vec::new(),
            last_penalty: 0,
            insertion_classes: std::collections::BTreeMap::new(),
            insertions: PageInsertions::default(),
            counters: BTreeMap::new(),
            multilevel_counters: BTreeMap::new(),
            typesetters: Vec::new(),
            parallel: None,
            extra_frames: Vec::new(),
            header: None,
            footer: None,
            pdf_config: PdfConfig::default(),
            bookmarks: Vec::new(),
            destinations: 0,
            grid: None,
            grid_debug: None,
            messages: Default::default(),
            background: None,
            end_page_hooks: Vec::new(),
            previous_references: None,
            consulted_references: Default::default(),
            references: CrossReferences::default(),
            math_tables: BTreeMap::new(),
            structure: None,
            untagged: 0,
        }
    }

    // -- Page geometry -------------------------------------------------------

    pub fn paper(&self) -> PaperSize {
        self.paper
    }

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

    /// Make the fonts in `dir` available to `set_font_spec`.
    pub fn load_fonts_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.font_db.load_fonts_dir(dir.as_ref());
        self
    }

    pub fn set_font(&mut self, name: impl Into<String>) -> &mut Self {
        self.settings.font = Some(name.into());
        self
    }

    /// The current font.
    pub fn font_spec(&self) -> Option<&FontSpec> {
        self.fonts.get(self.settings.font.as_deref()?).map(|f| &f.spec)
    }

    /// Switch to the font `spec` describes, from the fonts already
    /// registered or else the font database (SILE's `\font`).
    pub fn set_font_spec(&mut self, spec: FontSpec) -> Result<&mut Self, BuilderError> {
        let key = self.register_font_spec(spec)?;
        self.settings.font = Some(key);
        self.refresh_fallbacks()?;
        Ok(self)
    }

    /// Fall back on the current font changed by `fallback` for characters
    /// the fonts so far lack (SILE's `\font:add-fallback`).
    pub fn add_font_fallback(&mut self, fallback: FontFallback) -> Result<&mut Self, BuilderError> {
        self.settings.fallbacks.push(fallback);
        self.refresh_fallbacks()?;
        Ok(self)
    }

    /// Drop the last fallback added (SILE's `\font:remove-fallback`).
    pub fn remove_font_fallback(&mut self) -> &mut Self {
        self.settings.fallbacks.pop();
        self.settings.fallback_fonts.pop();
        self
    }

    pub fn clear_font_fallbacks(&mut self) -> &mut Self {
        self.settings.fallbacks.clear();
        self.settings.fallback_fonts.clear();
        self
    }

    fn refresh_fallbacks(&mut self) -> Result<(), BuilderError> {
        let Some(current) = self.font_spec().cloned() else {
            return Ok(());
        };
        let fallbacks = self.settings.fallbacks.clone();
        self.settings.fallback_fonts = fallbacks
            .iter()
            .map(|f| self.register_font_spec(f.apply(&current)))
            .collect::<Result<_, _>>()?;
        Ok(())
    }

    pub(crate) fn register_font_spec(&mut self, spec: FontSpec) -> Result<String, BuilderError> {
        let key = spec.cache_key();
        if !self.fonts.contains_key(&key) {
            let same_face = |f: &&RegisteredFont| {
                f.spec.family.as_deref().map(str::to_lowercase) == spec.family.as_deref().map(str::to_lowercase)
                    && f.spec.weight == spec.weight
                    && f.spec.style == spec.style
                    && f.spec.filename == spec.filename
            };
            let face = match self.fonts.values().find(same_face) {
                Some(f) => Arc::clone(&f.face),
                None => self.font_db.resolve(&spec)?,
            };
            self.fonts.insert(key.clone(), RegisteredFont { spec, face });
        }
        Ok(key)
    }

    pub(crate) fn registered_face(&self, key: &str) -> Option<(&FontSpec, &Arc<FontFace>)> {
        self.fonts.get(key).map(|f| (&f.spec, &f.face))
    }

    pub(crate) fn shaper(&self) -> &dyn Shaper {
        self.shaper.as_ref()
    }

    /// The math font and display settings (SILE's `math.*`).
    pub fn math_settings(&self) -> &crate::math::MathSettings {
        &self.settings.math
    }

    pub fn math_settings_mut(&mut self) -> &mut crate::math::MathSettings {
        &mut self.settings.math
    }

    pub fn space_settings(&self) -> &SpaceSettings {
        &self.settings.space_settings
    }

    /// Change some aspects of the current font.
    pub fn update_font(&mut self, f: impl FnOnce(&mut FontSpec)) -> Result<&mut Self, BuilderError> {
        let mut spec = self.font_spec().cloned().unwrap_or_default();
        f(&mut spec);
        self.set_font_spec(spec)
    }

    pub fn set_font_size(&mut self, size: f64) -> Result<&mut Self, BuilderError> {
        self.update_font(|spec| spec.size = size)
    }

    // -- Language and hyphenation --------------------------------------------

    pub fn hyphenation_mut(&mut self) -> &mut HyphenationDictionary {
        &mut self.hyphenation
    }

    pub fn language(&self) -> &str {
        &self.settings.language
    }

    pub fn set_language(&mut self, lang: impl Into<String>) -> &mut Self {
        self.settings.language = lang.into();
        self.hyphenation.load_language(&self.settings.language);
        self
    }

    /// Words whose hyphenation points are given by `-`, overriding the
    /// language's patterns.
    pub fn add_hyphenation_exceptions<'a>(&mut self, lang: &str, words: impl IntoIterator<Item = &'a str>) -> &mut Self {
        self.hyphenation.add_exceptions(lang, words);
        self
    }

    /// Keep every space as its own glue, including leading ones.
    pub fn set_obey_spaces(&mut self, obey: bool) -> &mut Self {
        self.settings.obey_spaces = obey;
        self
    }

    /// Treat U+00A0 as an ordinary glyph rather than a space-wide kern.
    pub fn set_fixed_nbsp(&mut self, fixed: bool) -> &mut Self {
        self.settings.fixed_nbsp = fixed;
        self
    }

    /// Space added between every pair of characters.
    pub fn set_letter_space(&mut self, space: Option<Length>) -> &mut Self {
        self.settings.letter_space = space;
        self
    }

    /// Space Ethiopic word separators on both sides rather than after
    /// (SILE's `languages.am.justification` "centered").
    pub fn set_ethiopic_centered(&mut self, centered: bool) -> &mut Self {
        self.settings.ethiopic_centered = centered;
        self
    }

    /// Scale every glyph's advance (SILE's `shaper.tracking`).
    pub fn set_tracking(&mut self, tracking: Option<f64>) -> &mut Self {
        self.settings.tracking = tracking;
        self
    }

    // -- Style ---------------------------------------------------------------

    pub fn color(&self) -> Option<Color> {
        self.settings.color
    }

    pub fn set_color(&mut self, color: Color) -> &mut Self {
        self.settings.color = Some(color);
        self
    }

    pub fn clear_color(&mut self) -> &mut Self {
        self.settings.color = None;
        self
    }

    // -- Paragraph settings --------------------------------------------------

    /// Indent for paragraphs that start from now on.
    pub fn set_paragraph_indent(&mut self, indent: f64) -> &mut Self {
        self.settings.paragraph_indent = indent;
        self
    }

    pub fn paragraph_indent(&self) -> f64 {
        self.settings.paragraph_indent
    }

    /// Indent for the next paragraph only (SILE's `current.parindent`);
    /// `Some(0.0)` is `\noindent`.
    /// Mark this point in the text with `value`, which `page_info` returns
    /// for the page it ends up on. Like SILE's `\info`, the marker doesn't
    /// start a paragraph, so one added first leaves it unindented.
    pub fn add_info<T: std::any::Any + Send + Sync>(&mut self, category: &str, value: T) -> &mut Self {
        let info = node::Info { category: category.to_string(), value: std::sync::Arc::new(value) };
        self.push_marker(liner_mark(Ink::Info(info)));
        self
    }

    fn push_marker(&mut self, marker: Node) {
        let marker = Inline::Node(Box::new(marker));
        match self.open_boxes.last_mut() {
            Some((_, open)) => open.push(marker),
            None => self.paragraph.push(marker),
        }
    }

    /// The values of `category` markers on the page being finished, in the
    /// order they were output (for use while it ends).
    pub fn page_info<T: std::any::Any + Clone>(&self, category: &str) -> Vec<T> {
        fn collect<T: std::any::Any + Clone>(nodes: &[Node], category: &str, out: &mut Vec<T>) {
            for node in nodes {
                match node {
                    Node::VBox(b) => collect(&b.nodes, category, out),
                    Node::HBox(b) => match &b.ink {
                        Some(Ink::Info(info)) if info.category == category => {
                            out.extend(info.value.downcast_ref::<T>().cloned());
                        }
                        _ => collect(&b.nodes, category, out),
                    },
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Some(state) = &self.page {
            for (_, nodes) in &state.page.content {
                collect(nodes, category, &mut out);
            }
        }
        out
    }

    pub fn set_current_indent(&mut self, indent: Option<f64>) -> &mut Self {
        self.current_indent = indent;
        self
    }

    /// Hanging indentation for the current paragraph only: lines after the
    /// first `after` (before the last `-after` when negative) are indented
    /// by `indent`, from the right when negative (SILE's `current.hangAfter`
    /// and `current.hangIndent`).
    pub fn set_hanging(&mut self, after: i32, indent: f64) -> &mut Self {
        self.hanging = Some((after, indent));
        self
    }

    /// Vertical glue after each paragraph (SILE's `document.parskip`).
    pub fn set_paragraph_skip(&mut self, skip: impl Into<Length>) -> &mut Self {
        self.settings.paragraph_skip = skip.into();
        self
    }

    /// Space lines TeX/SILE style, baseline to baseline, instead of adding a
    /// fixed `leading` between them. Overrides `set_leading`.
    pub fn baseline_skip(&self) -> Option<BaselineSkip> {
        self.settings.baseline_skip
    }

    pub fn set_baseline_skip(&mut self, baseline_skip: Option<BaselineSkip>) -> &mut Self {
        self.settings.baseline_skip = baseline_skip;
        self
    }

    /// Space lines with SILE's `linespacing` package rather than the
    /// baseline skip alone.
    pub fn set_line_spacing(&mut self, spacing: Option<LineSpacing>) -> &mut Self {
        self.settings.line_spacing = spacing;
        self
    }

    pub fn set_leading(&mut self, leading: f64) -> &mut Self {
        self.settings.leading = leading;
        self
    }

    /// The direction of every frame that doesn't set its own.
    pub fn set_direction(&mut self, direction: impl Into<FrameDirection>) -> &mut Self {
        let direction = direction.into();
        self.direction = direction;
        let template = self.page_template();
        if let Some(state) = self.page.as_mut() {
            for frame in &mut state.page.frames {
                let fixed = self.frame_directions.contains_key(&frame.id)
                    || template.frames.iter().any(|t| t.id == frame.id && t.direction.is_some());
                if !fixed {
                    frame.direction = Some(direction);
                }
            }
        }
        self
    }

    /// Set the writing direction of the frame being filled, here and on
    /// later pages.
    pub fn set_frame_direction(&mut self, direction: FrameDirection) -> &mut Self {
        if let Some(state) = self.page.as_mut() {
            self.frame_directions.insert(state.frame.clone(), direction);
            if let Some(frame) = state.page.frames.iter_mut().find(|f| f.id == state.frame) {
                frame.direction = Some(direction);
            }
        }
        self
    }

    fn resolve_direction(&self, frame: &mut FrameGeometry) {
        frame.direction = self.frame_directions.get(&frame.id).copied().or(frame.direction).or(Some(self.direction));
    }

    /// The direction of the frame being filled, or before the first page,
    /// of the frame it will start in.
    pub fn frame_direction(&self) -> FrameDirection {
        if let Some(direction) = self.frame_override {
            return direction;
        }
        if self.page.is_some() {
            return self.current_frame().and_then(|f| f.direction).unwrap_or(self.direction);
        }
        let template = self.page_template();
        let first = template.frames.iter().find(|f| f.id == template.first_content_frame);
        let id = first.map(|f| f.id.as_str()).unwrap_or_default();
        self.frame_directions.get(id).copied().or(first.and_then(|f| f.direction)).unwrap_or(self.direction)
    }

    /// Lines in vertical Japanese frames are broken first-fit and set one
    /// zenkaku tall (SILE's `tate` typesetter).
    fn in_tate_frame(&self) -> bool {
        self.frame_override.is_none() && self.current_frame().is_some_and(|f| f.tate)
    }

    /// The direction text runs in the frame being filled.
    pub fn writing_direction(&self) -> Direction {
        self.frame_direction().text()
    }

    /// Reorder mixed-direction paragraphs with the Unicode bidi algorithm
    /// (on by default, as in SILE).
    pub fn set_bidi(&mut self, on: bool) -> &mut Self {
        self.bidi = on;
        self
    }

    pub fn set_alignment(&mut self, alignment: TextAlign) -> &mut Self {
        self.settings.skips = self.settings.skips.aligned(alignment);
        self
    }

    pub fn line_skips(&self) -> LineSkips {
        self.settings.skips
    }

    pub fn set_line_skips(&mut self, skips: LineSkips) -> &mut Self {
        self.settings.skips = skips;
        self
    }

    pub fn set_space_settings(&mut self, settings: SpaceSettings) -> &mut Self {
        self.settings.space_settings = settings;
        self
    }

    pub fn linebreak_settings_mut(&mut self) -> &mut LinebreakSettings {
        &mut self.settings.linebreak_settings
    }

    pub fn page_break_settings_mut(&mut self) -> &mut PageBreakSettings {
        &mut self.page_break_settings
    }

    // -- Text ----------------------------------------------------------------

    /// Add text to the current paragraph, starting one if needed. Leading
    /// whitespace at the start of a paragraph or box is dropped.
    pub fn add_text(&mut self, text: impl Into<String>) -> &mut Self {
        let mut text = text.into().replace("\r\n", " ").replace(['\n', '\t'], " ");
        if text.contains('\u{AD}') {
            if !self.settings.soft_hyphens {
                return self.add_text(text.replace('\u{AD}', ""));
            }
            for (i, part) in text.split('\u{AD}').enumerate() {
                if i > 0 {
                    self.add_discretionary(Some("-"), None, None);
                }
                if !part.is_empty() {
                    self.add_text(part);
                }
            }
            return self;
        }
        let mut speaker_change = false;
        if self.current_list().is_empty() && !self.settings.obey_spaces {
            text = text.trim_start().to_string();
            if text.is_empty() {
                return self;
            }
            if self.settings.fixed_space_after_dash
                && self.open_boxes.is_empty()
                && let Some(rest) = text.strip_prefix('\u{2014}')
                && rest.starts_with([' ', '\u{00A0}', '\u{202F}'])
            {
                text = format!("\u{2014} {}", rest.trim_start_matches([' ', '\u{00A0}', '\u{202F}']));
                speaker_change = true;
            }
        }
        let mut run = self.text_run(text);
        run.speaker_change = speaker_change;
        self.push_inline(Inline::Text(run));
        self
    }

    /// Whether a paragraph opening with an em dash and a space, marking a
    /// change of speaker, keeps that space fixed (SILE's
    /// `typesetter.fixedSpacingAfterInitialEmdash`, on by default).
    /// Whether soft hyphens (U+00AD) in text are places it may break with
    /// a hyphen, or are dropped (SILE's `typesetter.softHyphen`, on by
    /// default).
    pub fn set_soft_hyphens(&mut self, on: bool) -> &mut Self {
        self.settings.soft_hyphens = on;
        self
    }

    /// Whether to space text set in italics from upright text next to it
    /// by how far the glyphs at the change lean over (SILE's
    /// `typesetter.italicCorrection`, off by default). Read when the
    /// paragraph is set.
    pub fn set_italic_correction(&mut self, correction: Option<ItalicCorrection>) -> &mut Self {
        self.settings.italic_correction = correction;
        self
    }

    /// Whether a Turkish word broken at an apostrophe sets a hyphen in
    /// place of the apostrophe, rather than the apostrophe alone (SILE's
    /// `languages.tr.replaceApostropheAtHyphenation`, off by default).
    pub fn set_replace_apostrophe_at_hyphenation(&mut self, on: bool) -> &mut Self {
        self.settings.replace_apostrophe_at_hyphenation = on;
        self
    }

    /// Break lines to this width instead of the frame's (SILE's
    /// `typesetter.breakwidth`).
    pub fn set_break_width(&mut self, width: Option<f64>) -> &mut Self {
        self.settings.break_width = width;
        self
    }

    fn break_width(&self) -> f64 {
        self.settings.break_width.unwrap_or_else(|| self.current_frame().map_or(0.0, FrameGeometry::line_length))
    }

    pub fn set_fixed_space_after_dash(&mut self, fixed: bool) -> &mut Self {
        self.settings.fixed_space_after_dash = fixed;
        self
    }

    /// Add a character to the text just added when it is in the same
    /// style, so that combining marks shape with their base (SILE's
    /// `\unichar`).
    pub fn add_char(&mut self, c: char) -> &mut Self {
        let run = self.text_run(c.to_string());
        let list = match self.open_boxes.last_mut() {
            Some((_, list)) => list,
            None => &mut self.paragraph,
        };
        if list.len() > 1
            && let Some(Inline::Text(last)) = list.last_mut()
            && (TextRun { text: run.text.clone(), ..last.clone() }) == run
        {
            last.text.push(c);
            return self;
        }
        self.push_inline(Inline::Text(run));
        self
    }

    fn text_run(&mut self, text: String) -> TextRun {
        let tokens = NodeMakerOptions {
            obey_spaces: self.settings.obey_spaces,
            fixed_nbsp: self.settings.fixed_nbsp,
            letterspace: self.settings.letter_space.is_some(),
            ethiopic_centered: self.settings.ethiopic_centered,
            ..NodeMakerOptions::for_language(&self.settings.language)
        };
        let font_name = self.settings.font.clone().unwrap_or_default();
        let fallbacks = self.settings.fallback_fonts.clone();
        let (font_name, fallbacks) = match self.fonts.get(&font_name).map(|f| f.spec.direction) {
            Some(Direction::Frame) => {
                let direction = self.writing_direction();
                let mut resolve = |name: String| self.font_in_direction(&name, direction).unwrap_or(name);
                let font_name = resolve(font_name);
                (font_name, fallbacks.into_iter().map(resolve).collect())
            }
            _ => (font_name, fallbacks),
        };
        TextRun {
            text,
            font_name,
            color: self.settings.color,
            language: self.settings.language.clone(),
            tokens,
            letter_space: self.settings.letter_space,
            tracking: self.settings.tracking,
            fallbacks,
            speaker_change: false,
            bidi_level: None,
            tag: self.current_tag(),
        }
    }

    /// A break opportunity: `prebreak` ends the line and `postbreak` starts
    /// the next if the paragraph breaks here, and `replacement` is set if it
    /// does not (SILE's `\discretionary`).
    pub fn add_discretionary(&mut self, prebreak: Option<&str>, postbreak: Option<&str>, replacement: Option<&str>) -> &mut Self {
        let mut run = |t: Option<&str>| t.map(|t| self.text_run(t.to_string()));
        let parts = Box::new([run(prebreak), run(postbreak), run(replacement)]);
        self.push_inline(Inline::Discretionary(parts));
        self
    }

    /// Breakable space that disappears at line breaks.
    pub fn add_glue(&mut self, width: impl Into<Length>) -> &mut Self {
        self.push_inline(Inline::Node(Box::new(Node::glue(width.into()))));
        self
    }

    /// Move the baseline of what follows up by `height` (down when
    /// negative), without changing the line's height (SILE's `\raise`).
    pub fn add_baseline_shift(&mut self, height: f64) -> &mut Self {
        let mut shift = node::HBox::new(Length::zero(), Length::zero(), Length::zero());
        shift.raise = height;
        self.push_inline(Inline::Node(Box::new(Node::HBox(shift))));
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

    /// Space that stretches and shrinks without limit (`\hss`).
    pub fn add_hss(&mut self) -> &mut Self {
        self.push_inline(Inline::Node(Box::new(Node::hssglue(Length::zero()))));
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
            self.push_vertical(Node::penalty(penalty));
        } else {
            self.push_inline(Inline::Node(Box::new(Node::penalty(penalty))));
        }
        self
    }

    /// Start collecting material into a box set at its natural width;
    /// everything added until `end_hbox` goes inside it.
    pub fn start_hbox(&mut self) -> &mut Self {
        self.open_boxes.push((Group::HBox, Vec::new()));
        self
    }

    /// Close the innermost `start_hbox`, `start_leaders` or `start_liner`.
    pub fn end_hbox(&mut self) -> &mut Self {
        if let Some((group, content)) = self.open_boxes.pop() {
            let link = matches!(group, Group::Liner(LinerStyle::Link(_)));
            self.push_inline(Inline::Box(group, content));
            if link && self.current_role() == Some(crate::structure::Role::Link) {
                self.end_structure();
            }
        }
        self
    }

    /// Close the innermost `start_hbox` and set its material now, handing
    /// the box back instead of adding it (SILE's `makeHbox`).
    pub fn make_hbox(&mut self) -> Result<node::HBox, BuilderError> {
        let content = self.open_boxes.pop().map(|(_, content)| content).unwrap_or_default();
        Ok(natural_hbox(self.shape_inlines(&content)?))
    }

    /// Add a ready-made box to the paragraph.
    pub fn add_box(&mut self, hbox: node::HBox) -> &mut Self {
        self.push_inline(Inline::Node(Box::new(Node::HBox(hbox))));
        self
    }

    /// Dots spaced a quarter em apart filling the rest of the line (SILE's
    /// `\dotfill`).
    pub fn add_dotfill(&mut self) -> &mut Self {
        let kern = Length::pt(0.25 * self.font_spec().map_or(10.0, |f| f.size));
        self.start_leaders(None).add_kern(kern).add_text(".").add_kern(kern).end_hbox()
    }

    /// Fill glue of `width` (as much as possible when `None`) with copies of
    /// the material added until `end_hbox`, lined up across lines by the
    /// frame's end edge (SILE's `\leaders`).
    pub fn start_leaders(&mut self, width: Option<Length>) -> &mut Self {
        self.open_boxes.push((Group::Leaders(width), Vec::new()));
        self
    }

    /// Draw `stroke` along the material added until `end_hbox`, across line
    /// breaks (SILE's liners).
    pub fn start_liner(&mut self, style: impl Into<LinerStyle>) -> &mut Self {
        self.open_boxes.push((Group::Liner(style.into()), Vec::new()));
        self
    }

    pub fn start_underline(&mut self) -> &mut Self {
        let (raise, thickness) = self.underline_metrics();
        self.start_liner(Stroke { raise, thickness })
    }

    pub fn start_strikethrough(&mut self) -> &mut Self {
        let (position, thickness) = self.strikeout_metrics();
        self.start_liner(Stroke { raise: position + thickness / 2.0, thickness })
    }

    /// The current font's underline position (its top above the baseline)
    /// and thickness, in points.
    pub fn underline_metrics(&self) -> (f64, f64) {
        self.font_metrics(|f| (f.underline_position(), f.underline_thickness()))
    }

    /// The current font's strikeout position (its middle above the baseline)
    /// and thickness, in points.
    pub fn strikeout_metrics(&self) -> (f64, f64) {
        self.font_metrics(|f| (f.strikeout_position(), f.strikeout_size()))
    }

    /// Add the messages in Fluent source `ftl` to `lang` (SILE's `\ftl`).
    pub fn add_messages(&mut self, lang: &str, ftl: &str) -> &mut Self {
        self.messages.add(lang, ftl);
        self
    }

    /// Message `id` in the current language, with the document's own
    /// messages over SILE's (SILE's `\fluent`).
    pub fn message(&self, id: &str, args: &[(&str, &str)]) -> Option<String> {
        self.messages.message(self.language(), id, args)
    }

    /// Limit shaping to these HarfBuzz shapers (SILE's `harfbuzz.subshapers`).
    pub fn set_subshapers(&mut self, shapers: &[&str]) -> &mut Self {
        self.shaper.set_subshapers(shapers);
        self
    }

    /// The current font's x-height in points (SILE's `ex`), half an em if
    /// the font doesn't say.
    pub fn x_height(&self) -> f64 {
        let Some(font) = self.settings.font.as_deref().and_then(|f| self.fonts.get(f)) else {
            return 0.0;
        };
        font.face.x_height().map_or(font.spec.size / 2.0, |h| font.face.scale(h, font.spec.size))
    }

    /// The current font's space width in points (SILE's `spc`).
    pub fn space_width(&self) -> f64 {
        let Some(font) = self.settings.font.as_deref().and_then(|f| self.fonts.get(f)) else {
            return 0.0;
        };
        let advance = font.face.glyph_id(' ').and_then(|g| font.face.advance_width(g));
        advance.map_or(font.spec.size / 4.0, |a| font.face.scale_u(a, font.spec.size))
    }

    /// The current font's ascender and descender (positive below the
    /// baseline) in points.
    pub fn font_extents(&self) -> (f64, f64) {
        let (ascender, descender) = self.font_metrics(|f| (f.ascender(), f.descender()));
        (ascender, -descender)
    }

    fn font_metrics(&self, metrics: impl Fn(&FontFace) -> (i16, i16)) -> (f64, f64) {
        let Some(font) = self.settings.font.as_deref().and_then(|f| self.fonts.get(f)) else {
            return (0.0, 0.0);
        };
        let (a, b) = metrics(&font.face);
        (font.face.scale(a, font.spec.size), font.face.scale(b, font.spec.size))
    }

    /// A box of solid ink on the baseline (SILE's `\hrule`).
    pub fn add_hrule(&mut self, width: f64, height: f64, depth: f64) -> &mut Self {
        let mut rule = node::HBox::new(Length::pt(width), Length::pt(height), Length::pt(depth));
        rule.ink = Some(Ink::Rule);
        self.push_inline(Inline::Node(Box::new(Node::HBox(rule))));
        self
    }

    /// Infinitely stretchable space filled with a rule (SILE's `\hrulefill`).
    pub fn add_hrulefill(&mut self, stroke: Stroke) -> &mut Self {
        let mut fill = Node::hfillglue(Length::zero());
        if let Node::HFillGlue(g) = &mut fill {
            g.explicit = true;
            g.leader = Some(Leader::Stroke(stroke));
        }
        self.push_inline(Inline::Node(Box::new(fill)));
        self
    }

    fn current_list(&self) -> &[Inline] {
        self.open_boxes.last().map_or(&self.paragraph, |(_, list)| list)
    }

    /// Whether one of the last `n` things added to the paragraph or box is
    /// a node `f` picks out.
    pub(crate) fn recently_added(&self, n: usize, f: impl Fn(&Node) -> bool) -> bool {
        self.current_list().iter().rev().take(n).any(|i| matches!(i, Inline::Node(node) if f(node)))
    }

    /// The width of a full-width character in the current font (SILE's
    /// `zw` unit), or the font size if it has none.
    pub fn zenkaku_width(&self) -> f64 {
        let Some(font) = self.settings.font.as_ref().and_then(|name| self.fonts.get(name)) else {
            return 10.0;
        };
        let glyphs = self.shaper.shape("あ", &font.face, &font.spec);
        if glyphs.is_empty() || glyphs.iter().any(|g| g.gid == 0) {
            return font.spec.size;
        }
        glyphs.iter().map(|g| g.width).sum::<f64>() * self.settings.tracking.unwrap_or(1.0)
    }

    /// Set what `content` adds as Latin text lying on its side in vertical
    /// Japanese, word by word, after a little space; elsewhere it is set as
    /// is (SILE's `\\latin-in-tate`).
    pub fn add_latin_in_tate<C, E>(ctx: &mut C, content: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        if ctx.as_mut().frame_direction().writing != Flow::TTB {
            return content(ctx);
        }
        let doc = ctx.as_mut();
        let saved = doc.settings.clone();
        let indent = doc.current_indent.unwrap_or(doc.settings.paragraph_indent);
        doc.set_language("und").update_font(|f| f.direction = Direction::LTR)?;
        doc.start_hbox();
        let result = content(ctx);
        let doc = ctx.as_mut();
        let inlines = doc.open_boxes.pop().map(|(_, content)| content).unwrap_or_default();
        result?;
        doc.frame_override = Some(FrameDirection::LTR);
        let nodes = doc.shape_inlines(&inlines);
        doc.frame_override = None;
        doc.settings = saved;
        let zw = doc.zenkaku_width();
        doc.add_glue(Length::new(Measurement::pt(0.5 * zw), Measurement::pt(0.25 * zw), Measurement::pt(0.25 * zw)));
        // The inner material is set as a paragraph of its own, indent included.
        doc.add_glue(Length::pt(indent));
        for mut node in nodes? {
            if node.is_glue() || node.is_kern() {
                doc.push_inline(Inline::Node(Box::new(node)));
            } else if pt_of(&node.line_contribution()) > 0.0 {
                if let Node::NNode(n) = &mut node {
                    for g in &mut n.glyphs {
                        (g.x_advance, g.x_offset, g.y_offset) = (g.width, 0.0, 0.0);
                    }
                }
                let mut hbox = natural_hbox(vec![node]);
                hbox.ink = Some(Ink::LatinInTate(zw));
                doc.add_box(hbox);
            }
        }
        Ok(())
    }

    /// Draw the character grid of the frame being filled under the page's
    /// content (SILE's `\show-hanmen`).
    pub fn show_hanmen(&mut self, hanmen: &crate::class::Hanmen) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let frame = self.current_frame().expect("current frame").clone();
        let (grid, gap) = (hanmen.gridsize, hanmen.linegap);
        let mut rules = Vec::new();
        let mut g = frame.top;
        while g < frame.bottom {
            rules.push([frame.left, g - 0.25, frame.width(), 0.5]);
            let mut l = frame.left;
            while l <= frame.right {
                rules.push([l - 0.25, g + grid - 0.25, 0.5, -grid]);
                l += grid;
            }
            g += grid;
            rules.push([frame.left, g - 0.25, frame.width(), 0.5]);
            g += gap;
        }
        let color = Color::Rgb { r: 1.0, g: 0.9, b: 0.9 };
        let page = &mut self.page.as_mut().expect("page").page;
        page.underlay.push(Underlay::Rules(Some(color), rules));
        Ok(self)
    }

    /// SILE's `initline`: a paragraph opens with a zero box and its indent.
    fn push_inline(&mut self, mut item: Inline) {
        if let Inline::Node(node) = &mut item
            && let Node::HBox(hbox) = &mut **node
            && hbox.tag.is_none()
            && matches!(hbox.ink, Some(Ink::Image(_) | Ink::Svg(_) | Ink::Math(_)))
        {
            hbox.tag = self.current_tag();
        }
        if let Some((_, open)) = self.open_boxes.last_mut() {
            open.push(item);
            return;
        }
        if self.paragraph.is_empty() {
            let indent = self.current_indent.take().unwrap_or(self.settings.paragraph_indent);
            self.paragraph.push(Inline::Node(Box::new(Node::zerohbox())));
            self.paragraph.push(Inline::Node(Box::new(Node::glue(Length::pt(indent)))));
        }
        self.paragraph.push(item);
    }

    /// End the paragraph and add the paragraph skip after it (SILE's
    /// `\par`). The skip is left out right after vertical glue or a
    /// penalty, so skips are not doubled.
    pub fn new_paragraph(&mut self) -> Result<&mut Self, BuilderError> {
        let after_skip = self.paragraph.is_empty()
            && self.last_vertical().is_some_and(|n| n.is_vglue() || n.is_penalty());
        if !after_skip {
            self.current_indent = None;
            self.leave_hmode(false)?;
            self.push_vglue_node(Node::vglue(self.settings.paragraph_skip));
        }
        self.leave_hmode(false)?;
        self.hanging = None;
        Ok(self)
    }

    /// Break the pending paragraph into lines without ending it as a
    /// paragraph (no paragraph skip), then fill the current frame if it is
    /// full. `independent` only breaks the lines.
    pub fn leave_hmode(&mut self, independent: bool) -> Result<(), BuilderError> {
        self.open_boxes.clear();
        self.end_tagged_paragraph();
        if let Some(capture) = self.captures.last_mut() {
            if !self.paragraph.is_empty() {
                capture.items.push(Captured::Paragraph {
                    inlines: std::mem::take(&mut self.paragraph),
                    settings: Box::new(self.settings.clone()),
                });
            }
            return Ok(());
        }
        if !self.paragraph.is_empty() {
            let inlines = std::mem::take(&mut self.paragraph);
            let nodes = self.typeset_paragraph(&inlines)?;
            self.vertical_queue.extend(nodes);
        }
        if independent || !self.typesetters.is_empty() {
            return Ok(());
        }
        if self.build_page()? {
            self.init_next_frame()?;
        }
        Ok(())
    }

    fn push_vertical(&mut self, node: Node) {
        match self.captures.last_mut() {
            Some(capture) => capture.items.push(Captured::Vertical(Box::new(node))),
            None => self.vertical_queue.push(node),
        }
    }

    pub(crate) fn add_vertical(&mut self, node: Node) {
        self.push_vertical(node);
    }

    /// Lines made outside the line breaker, spaced as a paragraph's are.
    pub(crate) fn add_lines(&mut self, lines: Vec<(VBox, bool, Vec<Node>)>) {
        let mut previous_depth = self.previous_depth;
        for node in self.stack_lines(lines, &mut previous_depth) {
            self.push_vertical(node);
        }
        self.previous_depth = previous_depth;
    }

    /// A rule with space `above` and `below` it, after which the next line
    /// is set without leading.
    pub(crate) fn add_rule_line(&mut self, indent: f64, width: f64, thickness: f64, [above, below]: [f64; 2]) {
        let mut rule = node::HBox::new(Length::pt(width), Length::pt(thickness), Length::zero());
        rule.ink = Some(Ink::Rule);
        self.push_vertical(Node::vglue(Length::pt(above)));
        self.push_vertical(Node::VBox(VBox::new(vec![Node::kern(Length::pt(indent)), Node::HBox(rule)], Length::pt(indent + width))));
        self.push_vertical(Node::penalty(10_000));
        self.push_vertical(Node::vglue(Length::pt(below)));
        self.previous_depth = None;
    }

    /// `material` set on one line at its natural width.
    pub(crate) fn natural_width(&mut self, material: &Material) -> Result<f64, BuilderError> {
        let nodes = self.shape_inlines(&material.inlines())?;
        Ok(pt_of(&natural_hbox(nodes).width))
    }

    /// Vertical glue added straight to the vertical list, ahead of the
    /// lines of any paragraph still in progress (SILE's `pushVglue`).
    pub fn push_vglue(&mut self, height: impl Into<Length>) -> &mut Self {
        self.push_vglue_node(Node::vglue(height.into()));
        self
    }

    /// Push vertical glue; on a grid it is fixed and followed by the space
    /// to the next grid line (SILE's grid typesetter).
    fn push_vglue_node(&mut self, mut glue: Node) {
        if self.grid.is_none() {
            return self.push_vertical(glue);
        }
        if let Node::VGlue(g) = &mut glue {
            g.height = Length::from(g.height.length);
            self.grid.as_mut().expect("grid").cursor += pt_of(&g.height);
        }
        self.push_vertical(glue);
        let make_up = self.grid_make_up();
        self.push_vertical(make_up);
    }

    fn grid_make_up(&mut self) -> Node {
        let grid = self.grid.as_mut().expect("grid");
        let add = (grid.spacing - grid.cursor).rem_euclid(grid.spacing);
        grid.cursor += add;
        Node::VGlue(node::VGlue { height: Length::pt(add), grid_leading: true, ..Default::default() })
    }

    /// Set lines on a grid `spacing` apart from the top of each frame, with
    /// vertical space rounded up to fit (SILE's `\grid`).
    pub fn start_grid(&mut self, spacing: f64) -> Result<&mut Self, BuilderError> {
        self.grid = Some(Grid { spacing, cursor: 0.0 });
        self.ensure_page()?;
        self.grid_new_frame();
        Ok(self)
    }

    /// Go back to ordinary line spacing (SILE's `\no-grid`).
    pub fn end_grid(&mut self) -> &mut Self {
        self.grid = None;
        self
    }

    /// Rule the grid lines in this frame and every one after it (SILE's
    /// `\grid:debug`).
    pub fn show_grid(&mut self, spacing: f64) -> Result<&mut Self, BuilderError> {
        self.grid_debug = Some(spacing);
        self.ensure_page()?;
        self.paint_grid();
        Ok(self)
    }

    fn paint_grid(&mut self) {
        let (Some(spacing), Some(frame)) = (self.grid_debug, self.current_frame()) else { return };
        let mut rules = Vec::new();
        let mut y = spacing;
        while y < frame.height() {
            rules.push([frame.left, frame.top + y, frame.width(), 0.1]);
            y += spacing;
        }
        self.page.as_mut().expect("page").page.underlay.push(Underlay::Rules(None, rules));
    }

    /// Start the grid afresh at the top of a frame (SILE's
    /// `startGridInFrame`).
    fn grid_new_frame(&mut self) {
        let Some(grid) = self.grid.as_mut() else { return };
        grid.cursor = 0.0;
        if self.vertical_queue.is_empty() {
            self.vertical_queue.push(Node::VBox(VBox::default()));
            self.previous_depth = Some(0.0);
            return;
        }
        let keep = self.vertical_queue.iter().position(|n| !n.is_discardable() && !matches!(n, Node::VGlue(g) if g.grid_leading));
        self.vertical_queue.drain(..keep.unwrap_or(self.vertical_queue.len()));
        if let Some(first) = self.vertical_queue.first() {
            let height = pt_of(&first.height());
            self.grid.as_mut().expect("grid").cursor += height;
            let make_up = self.grid_make_up();
            self.vertical_queue.splice(0..0, [Node::VBox(VBox::default()), make_up]);
        }
    }

    /// Length of the vertical list material is going to.
    pub(crate) fn vertical_len(&self) -> usize {
        match self.captures.last() {
            Some(capture) => capture.items.len(),
            None => self.vertical_queue.len(),
        }
    }

    /// Remove the `i`th item of the vertical list if it is a node `is`
    /// accepts.
    pub(crate) fn remove_vertical(&mut self, i: usize, is: impl Fn(&Node) -> bool) -> Option<Node> {
        match self.captures.last_mut() {
            Some(capture) => match capture.items.get(i) {
                Some(Captured::Vertical(node)) if is(node) => match capture.items.remove(i) {
                    Captured::Vertical(node) => Some(*node),
                    _ => None,
                },
                _ => None,
            },
            None => self.vertical_queue.get(i).is_some_and(&is).then(|| self.vertical_queue.remove(i)),
        }
    }

    fn last_vertical(&self) -> Option<&Node> {
        match self.captures.last() {
            Some(capture) => match capture.items.last() {
                Some(Captured::Vertical(node)) => Some(&**node),
                _ => None,
            },
            None => self.vertical_queue.last(),
        }
    }

    /// Whether nothing is waiting to be set, horizontally or vertically.
    pub fn is_queue_empty(&self) -> bool {
        self.paragraph.is_empty() && self.vertical_queue.is_empty()
    }

    // -- Vertical material ---------------------------------------------------

    pub fn add_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        self.push_vertical(Node::vglue(amount.into()));
        Ok(self)
    }

    /// Vertical space kept even at the top or bottom of a page (SILE's
    /// `\skip` and `\smallskip` family).
    pub fn add_explicit_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut glue = Node::vglue(amount.into());
        if let Node::VGlue(g) = &mut glue {
            g.explicit = true;
        }
        self.push_vglue_node(glue);
        Ok(self)
    }

    pub fn add_vfill(&mut self) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut fill = Node::vfillglue(Length::zero());
        if let Node::VFillGlue(g) = &mut fill {
            g.explicit = true;
        }
        self.push_vertical(fill);
        Ok(self)
    }

    pub fn add_page_break(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vertical_penalty(-10_000)
    }

    /// A page break penalty, ending any pending paragraph first.
    pub fn add_vertical_penalty(&mut self, penalty: i32) -> Result<&mut Self, BuilderError> {
        if !self.paragraph.is_empty() {
            self.leave_hmode(false)?;
        }
        self.push_vertical(Node::penalty(penalty));
        Ok(self)
    }

    /// Fill the page and force a new one (SILE's `\supereject`).
    pub fn supereject(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vfill()?;
        self.add_penalty(SUPER_EJECT);
        Ok(self)
    }

    pub fn add_rule(&mut self, width: f64, height: f64) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let vbox = VBox {
            width: Length::pt(width),
            height: Length::pt(height),
            depth: Length::zero(),
            nodes: vec![{
                let mut rule = Node::hbox(width, height, 0.0);
                if let Node::HBox(b) = &mut rule {
                    b.ink = Some(Ink::Rule);
                }
                rule
            }],
            ratio: 0.0,
            misfit: false,
            explicit: false,
        };
        self.push_vertical(Node::VBox(vbox));
        Ok(self)
    }

    /// Drop everything waiting to go on the page.
    pub fn clear_vertical_queue(&mut self) -> &mut Self {
        self.vertical_queue.clear();
        self
    }

    /// Number of items waiting to go on the page.
    pub fn vertical_queue_len(&self) -> usize {
        self.vertical_queue.len()
    }

    // -- Pages and frames ----------------------------------------------------

    /// Lay pages out with `class`: its page template, and its hooks at
    /// every page start and end. Set before adding content.
    pub fn set_class(&mut self, class: impl DocumentClass) -> &mut Self {
        self.class = Some(Box::new(class));
        self
    }

    pub fn class_mut<C: DocumentClass>(&mut self) -> Option<&mut C> {
        self.class.as_deref_mut()?.as_any_mut().downcast_mut::<C>()
    }

    /// Number of the page being filled, counting from 1.
    pub fn page_number(&self) -> usize {
        self.pages.len() + 1
    }

    /// Width and height of the frame content is flowing into.
    pub fn frame_size(&mut self) -> Result<(f64, f64), BuilderError> {
        self.ensure_page()?;
        let frame = self.current_frame().expect("current frame");
        Ok((frame.width(), frame.height()))
    }

    fn ensure_page(&mut self) -> Result<(), BuilderError> {
        if self.page.is_none() {
            self.start_page()?;
        }
        Ok(())
    }

    fn page_template(&self) -> PageTemplate {
        let mut template = match (&self.master, &self.class) {
            (Some(master), _) => master.clone(),
            (None, Some(class)) => class.page_template(),
            (None, None) => self.default_template(),
        };
        template.frames.retain(|f| !self.extra_frames.iter().any(|e| e.id == f.id));
        template.frames.extend(self.extra_frames.iter().cloned());
        template
    }

    /// Split the current frame where the material so far ends, or `offset`
    /// below its top: the frame ends there and a new one, `<id>_`, takes
    /// the rest, with what follows going into it unless `offset` is given
    /// (SILE's `\breakframevertical`).
    pub fn break_frame_vertical(&mut self, offset: Option<f64>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let height = match offset {
            Some(offset) => offset,
            None => {
                self.leave_hmode(true)?;
                let queue = std::mem::take(&mut self.vertical_queue);
                let height = queue.iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum();
                let id = self.page.as_ref().expect("page").frame.clone();
                self.output(&id, queue);
                height
            }
        };
        let state = self.page.as_mut().expect("page");
        let index = state.page.frames.iter().position(|f| f.id == state.frame).ok_or_else(|| BuilderError::Layout("no current frame".into()))?;
        let mut rest = state.page.frames[index].clone();
        rest.id = format!("{}_", rest.id);
        rest.top += height;
        rest.direction = None;
        let frame = &mut state.page.frames[index];
        frame.bottom = frame.top + height;
        frame.next = Some(rest.id.clone());
        let rest_id = rest.id.clone();
        self.resolve_direction(&mut rest);
        let state = self.page.as_mut().expect("page");
        state.page.frames.push(rest);
        if offset.is_none() {
            state.frame = rest_id;
            self.previous_depth = None;
        }
        Ok(self)
    }

    /// Add frames to every page from this one on (SILE's
    /// `class:declareFrame`).
    pub fn declare_frames(&mut self, frames: &[FrameSpec]) -> Result<&mut Self, BuilderError> {
        self.extra_frames.retain(|e| !frames.iter().any(|f| f.id == e.id));
        self.extra_frames.extend(frames.iter().cloned());
        self.declare_page_frames(frames)
    }

    fn start_page(&mut self) -> Result<(), BuilderError> {
        let template = self.page_template();
        let mut frames = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &template.frames)
            .map_err(|e| BuilderError::Layout(e.to_string()))?;
        for frame in &mut frames {
            self.resolve_direction(frame);
        }
        if !frames.iter().any(|f| f.id == template.first_content_frame) {
            return Err(BuilderError::Layout(format!(
                "no frame {}",
                template.first_content_frame
            )));
        }
        self.insertions = PageInsertions::default();
        self.page = Some(PageState {
            page: Page::new(self.page_number(), self.paper, frames),
            frame: template.first_content_frame,
        });
        self.paint_background();
        Ok(())
    }

    /// Fill this page behind its content and, if `all_pages`, the pages
    /// after it; `None` stops filling later pages (SILE's `\background`).
    pub fn set_background(&mut self, background: Option<Background>) -> Result<&mut Self, BuilderError> {
        self.background = background;
        if self.page.is_some() {
            self.paint_background();
        } else {
            self.ensure_page()?;
        }
        Ok(self)
    }

    /// Call `hook` as each page ends, after the class's own end of page,
    /// with the page still current (SILE's `endpage` hooks).
    pub fn add_end_page_hook(&mut self, hook: impl FnMut(&mut DocumentBuilder) -> Result<(), BuilderError> + 'static) -> &mut Self {
        self.end_page_hooks.push(Box::new(hook));
        self
    }

    /// Draw `decoration` over the current page's content.
    pub fn add_overlay(&mut self, decoration: Underlay) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        if let Some(state) = self.page.as_mut() {
            state.page.overlay.push(decoration);
        }
        Ok(self)
    }

    fn paint_background(&mut self) {
        let Some(background) = &self.background else { return };
        let Some(state) = self.page.as_mut() else { return };
        let page = [0.0, 0.0, self.paper.width, self.paper.height];
        state.page.underlay.push(match &background.fill {
            BackgroundFill::Color(color) => Underlay::Rules(Some(*color), vec![page]),
            BackgroundFill::Image(image) => Underlay::Image(image.clone(), page),
        });
        if !background.all_pages {
            self.background = None;
        }
    }

    /// Declare or redeclare frames on the current page only; the next page
    /// goes back to the class's frames (SILE's `\frame`). Expressions may
    /// refer to the page's other frames.
    pub fn declare_page_frames(&mut self, frames: &[FrameSpec]) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let template = self.page_template();
        let mut specs: Vec<FrameSpec> =
            template.frames.into_iter().filter(|t| !frames.iter().any(|f| f.id == t.id)).collect();
        specs.extend(frames.iter().cloned());
        let solved = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &specs).map_err(|e| BuilderError::Layout(e.to_string()))?;
        let mut solved: Vec<FrameGeometry> = solved.into_iter().filter(|g| frames.iter().any(|f| f.id == g.id)).collect();
        for frame in &mut solved {
            self.resolve_direction(frame);
        }
        let page = &mut self.page.as_mut().expect("page").page;
        for frame in solved {
            match page.frames.iter_mut().find(|g| g.id == frame.id) {
                Some(existing) => *existing = frame,
                None => page.frames.push(frame),
            }
        }
        Ok(self)
    }

    /// Carry on in frame `id` of the current page, which becomes the start
    /// of this page's content frames (SILE's `\pagetemplate`).
    pub fn set_content_frame(&mut self, id: &str) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        if state.page.frame(id).is_none() {
            return Err(BuilderError::Layout(format!("no frame {id}")));
        }
        state.frame = id.to_string();
        Ok(self)
    }

    /// Use `template`'s frames instead of the class's from now on, starting
    /// with the current page (SILE's `\switch-master`).
    pub fn set_master(&mut self, template: PageTemplate) -> Result<&mut Self, BuilderError> {
        self.master = Some(template.clone());
        self.replace_page_frames(&template)
    }

    /// Use `template`'s frames for the rest of the current page only, after
    /// shipping what is waiting into the current frame (SILE's
    /// `\switch-master-one-page`).
    pub fn set_page_master(&mut self, template: &PageTemplate) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        self.chuck()?;
        self.replace_page_frames(template)?;
        self.leave_hmode(false)?;
        Ok(self)
    }

    /// Ship everything waiting into the current frame as it is (SILE's
    /// `chuck`).
    fn chuck(&mut self) -> Result<(), BuilderError> {
        self.leave_hmode(true)?;
        if !self.vertical_queue.is_empty() {
            let id = self.page.as_ref().expect("page").frame.clone();
            let nodes = std::mem::take(&mut self.vertical_queue);
            self.output(&id, nodes);
        }
        Ok(())
    }

    fn replace_page_frames(&mut self, template: &PageTemplate) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let frames = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &template.frames).map_err(|e| BuilderError::Layout(e.to_string()))?;
        let state = self.page.as_mut().expect("page");
        state.page.frames.retain(|f| state.page.content.iter().any(|(id, _)| *id == f.id));
        state.page.frames.retain(|f| !frames.iter().any(|g| g.id == f.id));
        state.page.frames.extend(frames);
        self.set_content_frame(&template.first_content_frame)
    }

    /// Split the current frame into `columns` equal columns separated by
    /// `gutter`, for the rest of this page (SILE's `\makecolumns`).
    pub fn make_columns(&mut self, columns: usize, gutter: f64) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        let Some(frame) = state.page.frame(&state.frame).cloned() else { return Ok(self) };
        if columns < 2 {
            return Ok(self);
        }
        let width = (frame.width() - gutter * (columns - 1) as f64) / columns as f64;
        let mut new_frames = Vec::new();
        let mut previous = frame.id.clone();
        for i in 1..columns {
            let left = frame.left + i as f64 * (width + gutter);
            let gutter_frame = FrameGeometry {
                id: format!("{}_gutter{i}", frame.id),
                left: left - gutter,
                right: left,
                next: None,
                ..frame.clone()
            };
            let column = FrameGeometry { id: format!("{}_col{i}", frame.id), left, right: left + width, next: None, ..frame.clone() };
            new_frames.push((previous.clone(), column.id.clone()));
            previous = column.id.clone();
            state.page.frames.push(gutter_frame);
            state.page.frames.push(column);
        }
        for (from, to) in new_frames {
            if let Some(f) = state.page.frames.iter_mut().find(|f| f.id == from) {
                f.next = Some(to);
            }
        }
        if let Some(f) = state.page.frames.iter_mut().find(|f| f.id == frame.id) {
            f.right = f.left + width;
        }
        Ok(self)
    }

    /// Outline frame `id` on the current page, or all its frames when
    /// `None` (SILE's `\showframe`).
    pub fn show_frame(&mut self, id: Option<&str>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let page = &mut self.page.as_mut().expect("page").page;
        let frames: Vec<FrameGeometry> = page.frames.iter().filter(|f| id.is_none_or(|id| f.id == id)).cloned().collect();
        page.outlines.extend(frames);
        Ok(self)
    }

    fn current_frame(&self) -> Option<&FrameGeometry> {
        let state = self.page.as_ref()?;
        state.page.frame(&state.frame)
    }

    /// Fill the current frame if the queue holds enough to (SILE's
    /// `buildPage`). Insertions met on the way are placed on this page and
    /// shrink the frames they steal from.
    fn build_page(&mut self) -> Result<bool, BuilderError> {
        if self.vertical_queue.is_empty() {
            return Ok(false);
        }
        self.ensure_page()?;
        let frame = self.current_frame().expect("current frame");
        let id = frame.id.clone();
        let target = frame.target_length() - self.insertions.shrinkage(&id);
        let (classes, insertions) = (&self.insertion_classes, &mut self.insertions);
        let mut on_insertion = |queue: &mut Vec<Node>, i, height, target| {
            insertions.process(classes, &id, queue, i, height, target)
        };
        let br = if self.grid.is_some() {
            pagebuilder::find_grid_break(&mut self.vertical_queue, target, &mut on_insertion)
        } else {
            pagebuilder::find_break(&mut self.vertical_queue, target, false, &mut on_insertion)
        };
        let Some(br) = br else {
            return Ok(false);
        };
        self.last_penalty = br.trigger_penalty;
        let nodes = pagebuilder::split_page(&mut self.vertical_queue, &br);
        self.commit_shrinkage();
        let target = self.current_frame().expect("current frame").height() - self.insertions.shrinkage(&id);
        self.output(&id, pagebuilder::set_vertical_glue(nodes, target));
        Ok(true)
    }

    /// Take the room promised to this page's insertions from the frames
    /// they steal from, at the bottom.
    fn commit_shrinkage(&mut self) {
        let Some(state) = self.page.as_mut() else { return };
        for (class, _) in &self.insertions.boxes {
            let Some(class) = self.insertion_classes.get(class) else { continue };
            for (frame, _) in &class.steal_from {
                let shrinkage = self.insertions.shrinkage.insert(frame.clone(), 0.0).unwrap_or(0.0);
                if let Some(frame) = state.page.frames.iter_mut().find(|f| &f.id == frame) {
                    frame.bottom -= shrinkage;
                }
            }
        }
    }

    /// Grow each insertion frame upwards by what was placed in it and set
    /// the material there.
    fn output_insertions(&mut self) {
        let boxes = std::mem::take(&mut self.insertions.boxes);
        let Some(state) = self.page.as_mut() else { return };
        for (class, stack) in &boxes {
            let Some(class) = self.insertion_classes.get(class) else { continue };
            if let Some(frame) = state.page.frames.iter_mut().find(|f| f.id == class.insert_into) {
                frame.top -= stack.height + stack.depth;
            }
        }
        for (class, stack) in boxes {
            let Some(class) = self.insertion_classes.get(&class) else { continue };
            state.page.add_frame_content(class.insert_into.clone(), stack.nodes);
        }
    }

    /// Declare a kind of insertion (footnotes, say).
    pub fn set_insertion_class(&mut self, name: impl Into<String>, class: InsertionClass) -> &mut Self {
        self.insertion_classes.insert(name.into(), class);
        self
    }

    pub fn insertion_class_mut(&mut self, name: &str) -> Option<&mut InsertionClass> {
        self.insertion_classes.get_mut(name)
    }

    /// Send vertical material to the frame of insertion class `class`, from
    /// the current point of the paragraph (SILE's `class:insert`).
    pub fn insert(&mut self, class: &str, nodes: Vec<Node>) -> &mut Self {
        let penalty = self.insertion_classes.get(class).map_or(-3000, |c| c.penalty);
        let stack = Stack::of(nodes);
        let insertion = Node::Insertion(node::Insertion {
            class: class.to_string(),
            nodes: stack.nodes,
            content_height: stack.height,
            content_depth: stack.depth,
            seen: false,
        });
        let migrating = node::Migrating { material: vec![Node::penalty(penalty), insertion], ..Default::default() };
        self.push_inline(Inline::Node(Box::new(Node::Migrating(migrating))));
        self
    }

    fn output(&mut self, frame: &str, nodes: Vec<Node>) {
        if let Some(state) = self.page.as_mut() {
            state.page.add_frame_content(frame, nodes);
        }
    }

    /// Move on to the next frame, or end the page and start a new one.
    fn init_next_frame(&mut self) -> Result<(), BuilderError> {
        if self.vertical_queue.is_empty() {
            self.previous_depth = None;
        }
        let old_width = self.current_frame().map(FrameGeometry::line_length);
        let next = self.current_frame().and_then(|f| f.next.clone());
        match next {
            Some(next) if self.last_penalty > SUPER_EJECT => {
                self.page.as_mut().expect("page").frame = next;
            }
            _ => {
                self.end_page()?;
                self.new_page()?;
            }
        }
        let new_width = self.current_frame().map(FrameGeometry::line_length);
        if !self.vertical_queue.is_empty() && old_width.zip(new_width).is_some_and(|(a, b)| (a - b).abs() > 1e-6) {
            self.push_back()?;
            if self.typesetters.is_empty() && self.build_page()? {
                self.init_next_frame()?;
            }
        } else if let Some(first) = self.vertical_queue.first() {
            let lead = match self.settings.line_spacing {
                Some(spacing) => {
                    let em = self.font_spec().map_or(10.0, |f| f.size);
                    let min = pt_of(&resolve_em(spacing.minimum_first_line, em));
                    (min > 0.0).then(|| Node::vkern(Length::pt(min - pt_of(&first.height()))))
                }
                None => Some(Node::vglue(Length::zero())),
            };
            if let Some(lead) = lead {
                self.vertical_queue.insert(0, lead);
            }
        }
        self.grid_new_frame();
        self.paint_grid();
        Ok(())
    }

    /// Re-break the lines waiting for the page to the width of the frame
    /// they now go to (SILE's `pushBack`). Lines run together into one
    /// paragraph until their margins change, as in SILE; inter-line glue and
    /// penalties go.
    fn push_back(&mut self) -> Result<(), BuilderError> {
        let queue = std::mem::take(&mut self.vertical_queue);
        self.previous_depth = None;
        let mut nodes: Vec<Node> = Vec::new();
        let mut margins: Option<(Length, Length)> = None;
        for v in queue {
            match v {
                Node::VBox(vbox) if !vbox.explicit && vbox.nodes.len() >= 4 => {
                    let n = vbox.nodes.len();
                    let line_margins = (vbox.nodes[1].width(), vbox.nodes[n - 2].width());
                    if margins.is_some_and(|m| m != line_margins) {
                        self.rebreak(&mut nodes, margins)?;
                    }
                    margins = Some(line_margins);
                    if nodes.is_empty() {
                        nodes.push(Node::zerohbox());
                    }
                    for (i, node) in vbox.nodes.into_iter().enumerate() {
                        let keep = match &node {
                            _ if i == 0 || i == 1 || i == n - 2 => false,
                            Node::Discretionary(_) => i == 2,
                            Node::Penalty(_) => false,
                            _ => true,
                        };
                        if keep {
                            nodes.push(node);
                        }
                    }
                }
                Node::VBox(_) | Node::Insertion(_) | Node::Migrating(_) => {
                    self.rebreak(&mut nodes, margins)?;
                    self.vertical_queue.push(v);
                }
                _ => {}
            }
        }
        self.rebreak(&mut nodes, margins)
    }

    fn rebreak(&mut self, nodes: &mut Vec<Node>, margins: Option<(Length, Length)>) -> Result<(), BuilderError> {
        while nodes.last().is_some_and(|n| n.is_penalty() || n.is_zero()) {
            nodes.pop();
        }
        if nodes.is_empty() {
            return Ok(());
        }
        let hsize = self.break_width();
        let (left, right) = margins.unwrap_or_default();
        let skips = LineSkips { left, right, ..self.settings.skips };
        let mut previous_depth = self.previous_depth;
        let direction = self.writing_direction();
        let lines = self.break_nodes(std::mem::take(nodes), hsize, direction, false, skips, &mut previous_depth);
        self.previous_depth = previous_depth;
        self.vertical_queue.extend(lines);
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), BuilderError> {
        self.untagged(Self::end_page_untagged)
    }

    fn end_page_untagged(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        self.output_insertions();
        self.collect_references();
        if let Some(mut class) = self.class.take() {
            let result = class.end_page(self);
            self.class = Some(class);
            result?;
        }
        let mut hooks = std::mem::take(&mut self.end_page_hooks);
        let result = hooks.iter_mut().try_for_each(|hook| hook(self));
        hooks.append(&mut self.end_page_hooks);
        self.end_page_hooks = hooks;
        result?;
        if let Some(state) = self.page.take() {
            self.pages.push(state.page);
        }
        Ok(())
    }

    fn new_page(&mut self) -> Result<(), BuilderError> {
        if let Some(mut class) = self.class.take() {
            let result = self.untagged(|doc| class.new_page(doc));
            self.class = Some(class);
            result?;
        }
        self.start_page()
    }

    /// Fill the last page and end it (SILE's `class:finish`).
    fn finish(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        if let Some(mut parallel) = self.parallel.take() {
            self.deactivate(&mut parallel);
            self.parallel_page_break(&mut parallel)?;
            self.pop_typesetter()?;
            self.vertical_queue.clear();
        }
        self.new_paragraph()?;
        self.add_vfill()?;
        while !self.is_queue_empty() {
            self.supereject()?;
            self.leave_hmode(true)?;
            self.build_page()?;
            if !self.is_queue_empty() {
                self.init_next_frame()?;
            }
        }
        self.end_page()
    }

    /// Typeset into several frames side by side, each named flow going to
    /// its frame (`(name, frame)`), selected with `select_parallel` and
    /// kept level with `sync_parallel` (SILE's `parallel` package). As in
    /// SILE, what is added before a flow is selected goes nowhere, and so
    /// does material not yet on a page when this is called.
    pub fn begin_parallel(&mut self, flows: &[(&str, &str)]) -> Result<&mut Self, BuilderError> {
        self.push_typesetter(None)?;
        let flows = flows
            .iter()
            .map(|(name, frame)| (name.to_string(), ParallelFlow { frame: frame.to_string(), flow: FlowState::default(), mark: 0 }))
            .collect();
        self.parallel = Some(Parallel { flows, active: None });
        Ok(self)
    }

    pub fn select_parallel(&mut self, name: &str) -> Result<&mut Self, BuilderError> {
        let mut parallel = self.parallel.take().ok_or_else(|| BuilderError::Layout("no parallel flows".into()))?;
        if !parallel.flows.contains_key(name) {
            self.parallel = Some(parallel);
            return Err(BuilderError::Layout(format!("no parallel flow {name}")));
        }
        self.deactivate(&mut parallel);
        parallel.active = Some(name.to_string());
        self.activate(&mut parallel);
        self.parallel = Some(parallel);
        Ok(self)
    }

    /// Pad the flows with space so that what comes next in each starts at
    /// the same height, or end the page if any has filled it (SILE's
    /// `\sync`).
    pub fn sync_parallel(&mut self) -> Result<&mut Self, BuilderError> {
        let mut parallel = self.parallel.take().ok_or_else(|| BuilderError::Layout("no parallel flows".into()))?;
        self.deactivate(&mut parallel);
        let result = self.sync_flows(&mut parallel);
        self.activate(&mut parallel);
        self.parallel = Some(parallel);
        result.map(|_| self)
    }

    fn sync_flows(&mut self, parallel: &mut Parallel) -> Result<(), BuilderError> {
        let mut any_break = false;
        for p in parallel.flows.values_mut() {
            self.enter_flow(p);
            let result = self.leave_hmode(true);
            if result.is_ok() {
                let target = self.current_frame().map_or(0.0, FrameGeometry::target_length);
                let mut lines = self.vertical_queue.clone();
                any_break |= pagebuilder::find_break(&mut lines, target, false, &mut pagebuilder::no_insertions).is_some();
            }
            p.flow = self.take_flow();
            result?;
        }
        if any_break {
            return self.parallel_page_break(parallel);
        }
        let new_material = |p: &ParallelFlow| p.flow.queue[p.mark..].iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum::<f64>();
        let tallest = parallel.flows.values().map(new_material).fold(0.0, f64::max);
        for p in parallel.flows.values_mut() {
            let glue = tallest - new_material(p);
            if glue > 0.0 {
                p.flow.queue.push(Node::vglue(Length::pt(glue)));
            }
            p.mark = p.flow.queue.len();
        }
        Ok(())
    }

    /// Output each flow's levelled material and start a new page, levelling
    /// what is left (SILE's `parallelPagebreak`).
    fn parallel_page_break(&mut self, parallel: &mut Parallel) -> Result<(), BuilderError> {
        for p in parallel.flows.values_mut() {
            self.enter_flow(p);
            let result = if !self.vertical_queue.is_empty() && p.mark == 0 {
                self.build_page().map(|_| ())
            } else {
                let lines = self.vertical_queue.drain(..p.mark.min(self.vertical_queue.len())).collect();
                self.output(&p.frame, lines);
                Ok(())
            };
            p.flow = self.take_flow();
            result?;
        }
        self.end_page()?;
        for p in parallel.flows.values_mut() {
            p.mark = 0;
        }
        self.new_page()?;
        self.sync_flows(parallel)
    }

    fn enter_flow(&mut self, p: &mut ParallelFlow) {
        let flow = std::mem::take(&mut p.flow);
        self.put_flow(flow);
        if let Some(state) = self.page.as_mut() {
            state.frame = p.frame.clone();
        }
    }

    fn activate(&mut self, parallel: &mut Parallel) {
        if let Some(p) = parallel.active.as_ref().and_then(|a| parallel.flows.get_mut(a)) {
            self.enter_flow(p);
        }
    }

    fn deactivate(&mut self, parallel: &mut Parallel) {
        let flow = self.take_flow();
        if let Some(p) = parallel.active.as_ref().and_then(|a| parallel.flows.get_mut(a)) {
            p.flow = flow;
        }
    }

    fn take_flow(&mut self) -> FlowState {
        FlowState {
            paragraph: std::mem::take(&mut self.paragraph),
            open_boxes: std::mem::take(&mut self.open_boxes),
            current_indent: self.current_indent.take(),
            previous_depth: self.previous_depth.take(),
            queue: std::mem::take(&mut self.vertical_queue),
        }
    }

    fn put_flow(&mut self, flow: FlowState) {
        self.paragraph = flow.paragraph;
        self.open_boxes = flow.open_boxes;
        self.current_indent = flow.current_indent;
        self.previous_depth = flow.previous_depth;
        self.vertical_queue = flow.queue;
    }

    /// Set the current paragraph and vertical list aside and start afresh,
    /// with lines as wide as `frame` (the current frame when `None`), until
    /// `pop_typesetter` (SILE's `typesetter:pushState`).
    pub fn push_typesetter(&mut self, frame: Option<&str>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        let flow = match frame {
            Some(frame) => std::mem::replace(&mut state.frame, frame.to_string()),
            None => state.frame.clone(),
        };
        self.typesetters.push(SavedTypesetter {
            settings: self.settings.clone(),
            paragraph: std::mem::take(&mut self.paragraph),
            open_boxes: std::mem::take(&mut self.open_boxes),
            current_indent: self.current_indent.take(),
            previous_depth: self.previous_depth.take(),
            queue: std::mem::take(&mut self.vertical_queue),
            captures: std::mem::take(&mut self.captures),
            lists: std::mem::take(&mut self.lists.levels),
            frame: flow,
        });
        Ok(self)
    }

    /// End the paragraph begun since `push_typesetter`, restore what it set
    /// aside, and return the vertical material set meanwhile.
    pub fn pop_typesetter(&mut self) -> Result<Vec<Node>, BuilderError> {
        let result = self.leave_hmode(true);
        let Some(saved) = self.typesetters.pop() else {
            return Ok(Vec::new());
        };
        let nodes = std::mem::replace(&mut self.vertical_queue, saved.queue);
        if let Some(state) = self.page.as_mut() {
            state.frame = saved.frame;
        }
        self.settings = saved.settings;
        self.paragraph = saved.paragraph;
        self.open_boxes = saved.open_boxes;
        self.current_indent = saved.current_indent;
        self.previous_depth = saved.previous_depth;
        self.captures = saved.captures;
        self.lists.levels = saved.lists;
        result?;
        Ok(nodes)
    }

    /// Typeset whatever `f` adds straight into `frame` on the current page,
    /// apart from the main flow, with settings restored afterwards (SILE's
    /// `typesetNaturally`).
    pub fn typeset_into(
        &mut self,
        frame: &str,
        f: impl FnOnce(&mut Self) -> Result<(), BuilderError>,
    ) -> Result<(), BuilderError> {
        self.push_typesetter(Some(frame))?;
        let result = f(self);
        let nodes = self.pop_typesetter()?;
        result?;
        let top = nodes
            .iter()
            .position(|n| !n.is_discardable() && !n.is_explicit())
            .unwrap_or(nodes.len());
        self.output(frame, nodes.into_iter().skip(top).collect());
        Ok(())
    }

    // -- Settings and captured material ---------------------------------------

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn restore_settings(&mut self, settings: Settings) -> &mut Self {
        self.settings = settings;
        self
    }

    /// Remember the current settings as the document's own, for material
    /// set outside the flow (folios, running heads) to start from.
    pub fn mark_toplevel(&mut self) -> &mut Self {
        self.toplevel = Some(self.settings.clone());
        self
    }

    /// Switch to the document's own settings (SILE's `toplevelState`).
    pub fn use_toplevel(&mut self) -> &mut Self {
        if let Some(settings) = self.toplevel.clone() {
            self.settings = settings;
        }
        self
    }

    /// Record what is added from here on instead of typesetting it, until
    /// `end_capture`.
    pub fn begin_capture(&mut self) -> &mut Self {
        self.captures.push(Capture {
            items: Vec::new(),
            paragraph: std::mem::take(&mut self.paragraph),
            open_boxes: std::mem::take(&mut self.open_boxes),
            current_indent: self.current_indent.take(),
        });
        self
    }

    pub fn end_capture(&mut self) -> Material {
        let Some(capture) = self.captures.pop() else {
            return Material::default();
        };
        let mut items = capture.items;
        while let Some((group, open)) = self.open_boxes.pop() {
            self.push_inline(Inline::Box(group, open));
        }
        let pending = std::mem::replace(&mut self.paragraph, capture.paragraph);
        if !pending.is_empty() {
            items.push(Captured::Inlines(pending));
        }
        self.open_boxes = capture.open_boxes;
        self.current_indent = capture.current_indent;
        Material { items }
    }

    /// Add captured material here, as it was added where it was recorded.
    pub fn add_material(&mut self, material: &Material) -> Result<&mut Self, BuilderError> {
        for item in &material.items {
            match item {
                Captured::Paragraph { inlines, settings } => {
                    let current = std::mem::replace(&mut self.settings, (**settings).clone());
                    self.paragraph.extend(inlines.iter().cloned());
                    let result = self.leave_hmode(false);
                    self.settings = current;
                    result?;
                }
                Captured::Inlines(inlines) => self.paragraph.extend(inlines.iter().cloned()),
                Captured::Vertical(node) => self.push_vertical((**node).clone()),
            }
        }
        Ok(self)
    }

    // -- Counters -------------------------------------------------------------

    /// A document-wide counter such as `footnote`, starting at 0.
    pub fn counter_mut(&mut self, id: &str) -> &mut i64 {
        self.counters.entry(id.to_string()).or_default()
    }

    /// A document-wide multilevel counter such as `sectioning`.
    pub fn multilevel_counter_mut(&mut self, id: &str) -> &mut MultilevelCounter {
        self.multilevel_counters.entry(id.to_string()).or_default()
    }

    // -- Destinations, links and bookmarks ----------------------------------

    /// Name this point for links and bookmarks to go to.
    pub fn add_destination(&mut self, name: impl Into<String>) -> &mut Self {
        self.push_marker(liner_mark(Ink::Destination(name.into())));
        self
    }

    /// Link what follows, until `end_hbox`, to `dest` (SILE's `\pdf:link`).
    pub fn start_link(&mut self, dest: LinkDest) -> &mut Self {
        self.begin_structure(crate::structure::Role::Link);
        self.start_liner(LinerStyle::Link(dest))
    }

    /// Bookmark this point in the document outline.
    pub fn add_bookmark(&mut self, title: impl Into<String>, level: u32) -> &mut Self {
        let dest = self.new_destination();
        self.add_bookmark_at(title, level, dest)
    }

    /// Bookmark the destination `dest` in the document outline.
    pub fn add_bookmark_at(&mut self, title: impl Into<String>, level: u32, dest: impl Into<String>) -> &mut Self {
        self.bookmarks.push(Bookmark { title: title.into(), level, dest: dest.into() });
        self
    }

    fn new_destination(&mut self) -> String {
        self.destinations += 1;
        let name = format!("dest{}", self.destinations);
        self.add_destination(name.clone());
        name
    }

    // -- Tables of contents and cross references ----------------------------

    /// Give this pass what the previous one found, for `references`.
    pub fn set_references(&mut self, previous: Option<CrossReferences>) -> &mut Self {
        self.previous_references = previous;
        self
    }

    /// What the previous pass found, or `None` on the first pass.
    pub fn references(&self) -> Option<&CrossReferences> {
        self.consulted_references.set(true);
        self.previous_references.as_ref()
    }

    /// Enter a heading in the table of contents and the document outline,
    /// as on the page this point ends up on (SILE's `\tocentry`).
    pub fn add_toc_entry(&mut self, level: usize, number: Option<String>, label: impl Into<String>) -> &mut Self {
        let label = label.into();
        let dest = self.new_destination();
        self.add_bookmark_at(label.clone(), level as u32, dest.clone());
        let entry = TocEntry { label, level, number, page: String::new(), dest: Some(dest) };
        self.add_info(references::TOC, entry)
    }

    /// Label this point so a later pass can refer to its page, and to
    /// `value` (a section or figure number, say).
    pub fn add_label(&mut self, name: impl Into<String>, value: Option<String>) -> &mut Self {
        let name = name.into();
        let dest = format!("label:{name}");
        self.add_destination(dest.clone());
        self.add_info(references::LABELS, (name, Label { page: String::new(), value, dest }))
    }

    /// Enter `label` in the index called `index` (SILE's `main` by default),
    /// as on the page this point ends up on (SILE's `\indexentry`).
    pub fn add_index_entry(&mut self, index: impl Into<String>, label: impl Into<String>) -> &mut Self {
        let link = self.new_destination();
        self.add_info(references::INDEX, IndexMark { index: index.into(), label: label.into(), link })
    }

    /// Note the references on the page being finished, numbered as the
    /// class numbers it.
    fn collect_references(&mut self) {
        let number = self.class.as_ref().and_then(|c| c.folio()).unwrap_or_else(|| PageNumber::arabic(self.pages.len() as i64 + 1));
        let page = number.to_string();
        for mut entry in self.page_info::<TocEntry>(references::TOC) {
            entry.page = page.clone();
            self.references.toc.push(entry);
        }
        for (name, mut label) in self.page_info::<(String, Label)>(references::LABELS) {
            label.page = page.clone();
            self.references.labels.entry(name).or_insert(label);
        }
        for mark in self.page_info::<IndexMark>(references::INDEX) {
            let pages = self.references.index.entry(mark.index).or_default().entry(mark.label).or_default();
            if pages.last().is_none_or(|p| p.page != number) {
                pages.push(IndexPage { page: number.clone(), link: Some(mark.link) });
            }
        }
    }

    // -- PDF config ----------------------------------------------------------

    /// Print pages centred on sheets of `sheet`, when given (SILE's
    /// `sheetsize` class option).
    pub fn set_sheet_size(&mut self, sheet: Option<PaperSize>) -> &mut Self {
        self.pdf_config.sheet = sheet;
        self
    }

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

    /// Set a document info entry by its PDF key (SILE's `\pdf:metadata`).
    /// Dates must be PDF dates; `Trapped` is not text and can't be set.
    pub fn set_pdf_metadata(&mut self, key: &str, value: &str) -> Result<&mut Self, BuilderError> {
        let invalid = |what: String| Err(BuilderError::InvalidMetadata(what));
        match key {
            "Title" => self.pdf_config.title = Some(value.into()),
            "Author" => self.pdf_config.author = Some(value.into()),
            "Subject" => self.pdf_config.subject = Some(value.into()),
            "Trapped" => return invalid("Trapped can't be set as text".into()),
            "CreationDate" | "ModDate" if !is_pdf_date(value) => return invalid(format!("{key} {value:?} is not a PDF date")),
            _ => self.pdf_config.info.push((key.into(), value.into())),
        }
        Ok(self)
    }

    pub fn set_compress(&mut self, compress: bool) -> &mut Self {
        self.pdf_config.compress = compress;
        self
    }

    // -- Render --------------------------------------------------------------

    pub fn render(self) -> Result<Vec<u8>, BuilderError> {
        self.lay_out()?.render()
    }

    /// Lay the document out and describe it in SILE's debug outputter format,
    /// for comparing layout against SILE's regression test expectations.
    pub fn render_debug(self) -> Result<String, BuilderError> {
        Ok(self.lay_out()?.render_debug())
    }

    /// Lay the document out and hand back its pages.
    pub fn into_pages(self) -> Result<Vec<Page>, BuilderError> {
        Ok(self.lay_out()?.pages)
    }

    /// Finish the document and lay out its last pages.
    pub fn lay_out(mut self) -> Result<Layout, BuilderError> {
        self.finish()?;
        let mut pages = std::mem::take(&mut self.pages);

        // Running header/footer: typeset per page (page numbers differ)
        let total = pages.len();
        for (name, running) in [("header", self.header.clone()), ("footer", self.footer.clone())] {
            let Some(running) = running else { continue };
            for page in pages.iter_mut() {
                let Some(hsize) = page.frame(name).map(FrameGeometry::line_length) else { continue };
                let line = running.resolved(page.number, total, &self.settings.language);
                let skips = LineSkips::default().aligned(running.align);
                let nodes = self.typeset_inlines(&line, hsize, running.direction, skips, &mut None)?;
                page.add_frame_content(name, nodes);
            }
        }

        Ok(Layout {
            pages,
            references: self.references,
            consulted_references: self.consulted_references.get(),
            paper: self.paper,
            fonts: self.fonts,
            bookmarks: self.bookmarks,
            pdf_config: self.pdf_config,
            structure: self.structure,
        })
    }

    // -- Internal: paragraph typesetting ------------------------------------

    fn typeset_paragraph(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        self.ensure_page()?;
        let hsize = self.break_width();
        let mut previous_depth = self.previous_depth;
        let nodes = self.typeset_inlines(inlines, hsize, self.writing_direction(), self.settings.skips, &mut previous_depth)?;
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
        let bidi = self.bidi && direction != Direction::TTB;
        let mut h_nodes = if bidi {
            let inlines = self.split_bidi_runs(inlines, direction)?;
            self.shape_inlines(&inlines)?
        } else {
            self.shape_inlines(inlines)?
        };
        if self.is_untagged() {
            node::clear_tags(&mut h_nodes);
        }
        mark_word_spaces(&mut h_nodes);
        Ok(self.break_nodes(h_nodes, hsize, direction, bidi, skips, previous_depth))
    }

    /// Break shaped paragraph material into lines (the rest of SILE's
    /// `boxUpNodes`).
    fn break_nodes(
        &mut self,
        mut h_nodes: Vec<Node>,
        hsize: f64,
        direction: Direction,
        reorder: bool,
        skips: LineSkips,
        previous_depth: &mut Option<f64>,
    ) -> Vec<Node> {
        let mut j = h_nodes.len();
        while j > 0 {
            j -= 1;
            if h_nodes[j].is_migrating() {
                continue;
            }
            if !h_nodes[j].is_discardable() {
                break;
            }
            h_nodes.remove(j);
        }
        while h_nodes.first().is_some_and(Node::is_penalty) {
            h_nodes.remove(0);
        }
        if h_nodes.is_empty() {
            return Vec::new();
        }
        let mut par_fill = Node::glue(skips.par_fill);
        if let Node::Glue(g) = &mut par_fill {
            g.explicit = true;
        }
        h_nodes.push(par_fill);
        h_nodes.push(Node::penalty(-10_000));

        let mut lb_settings = self.settings.linebreak_settings.clone();
        lb_settings.left_skip = skips.left;
        lb_settings.right_skip = skips.right;
        if let Some((after, indent)) = self.hanging {
            (lb_settings.hang_after, lb_settings.hang_indent) = (after, indent);
        }
        let (h_nodes, breaks) = if self.in_tate_frame() {
            let breaks = linebreak::first_fit(&h_nodes, hsize);
            (h_nodes, breaks)
        } else {
            linebreak::break_paragraph(h_nodes, hsize, &lb_settings, |nodes| self.hyphenate(nodes))
        };
        self.build_lines(h_nodes, &breaks, direction, reorder, skips, previous_depth)
    }

    fn shape_inlines(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        let mut h_nodes = Vec::new();
        let mut previous: Option<RunEdges> = None;
        for item in inlines {
            let Inline::Text(run) = item else {
                previous = None;
                self.shape_inline(item, &mut h_nodes)?;
                continue;
            };
            let mut punct = Vec::new();
            let nodes = self.shape_run_marking(run, &mut punct)?;
            if let Some(correction) = self.settings.italic_correction {
                let edges = self.run_edges(&run.font_name, &nodes, &punct);
                if let Some(previous) = &previous
                    && let Some(node) = correction.between(previous, &edges)
                {
                    h_nodes.push(node);
                }
                previous = Some(edges);
            }
            h_nodes.extend(nodes);
        }
        Ok(h_nodes)
    }

    /// What the italic correction looks at in a shaped run: whether its
    /// font leans, its first and last glyphs, and whether glue or a
    /// punctuation space comes before the first or after the last.
    fn run_edges(&self, font_name: &str, nodes: &[Node], punct: &[usize]) -> RunEdges {
        let italic = self.fonts.get(font_name).is_some_and(|f| f.face.italic_angle() != 0.0);
        let edge = |indices: &mut dyn Iterator<Item = usize>, last: bool| {
            let (mut glue, mut punct_width) = (false, None);
            for i in indices {
                match &nodes[i] {
                    Node::NNode(n) => {
                        let glyph = if last { n.glyphs.last() } else { n.glyphs.first() };
                        let shape = glyph.and_then(|g| self.glyph_shape(&n.font_key, n.font_size, g));
                        return (shape, glue, punct_width);
                    }
                    Node::Kern(k) if punct.contains(&i) => punct_width = k.width.length.to_pt(),
                    Node::Glue(_) => glue = true,
                    _ => {}
                }
            }
            (None, glue, punct_width)
        };
        RunEdges { italic, first: edge(&mut (0..nodes.len()), false), last: edge(&mut (0..nodes.len()).rev(), true) }
    }

    fn glyph_shape(&self, font_key: &str, size: f64, glyph: &GlyphData) -> Option<GlyphShape> {
        let face = &self.fonts.get(font_key)?.face;
        let s = size / face.units_per_em() as f64;
        let b = face.glyph_bounding_box(glyph.gid).unwrap_or_default();
        Some(GlyphShape {
            width: glyph.width,
            ink_width: (b.x_max as f64 - b.x_min as f64) * s,
            x_bearing: b.x_min as f64 * s,
            height: b.y_max as f64 * s,
            depth: -(b.y_min as f64) * s,
        })
    }

    fn shape_inline(&mut self, item: &Inline, h_nodes: &mut Vec<Node>) -> Result<(), BuilderError> {
        {
            match item {
                Inline::Text(run) => h_nodes.extend(self.shape_run(run)?),
                Inline::Node(node) => h_nodes.push((**node).clone()),
                Inline::Discretionary(parts) => {
                    let mut lists = Vec::with_capacity(3);
                    for part in parts.iter() {
                        lists.push(match part {
                            Some(run) => vec![Node::HBox(natural_hbox(self.shape_run(run)?))],
                            None => Vec::new(),
                        });
                    }
                    let [prebreak, postbreak, replacement]: [Vec<Node>; 3] = lists.try_into().expect("three parts");
                    h_nodes.push(Node::discretionary(prebreak, postbreak, replacement));
                }
                Inline::Box(Group::Liner(stroke), content) => {
                    h_nodes.push(liner_mark(Ink::LinerStart(stroke.clone())));
                    h_nodes.extend(self.shape_inlines(content)?);
                    h_nodes.push(liner_mark(Ink::LinerEnd));
                }
                Inline::Box(group, content) => {
                    let mut hbox = natural_hbox(self.shape_inlines(content)?);
                    if matches!(group, Group::Leaders(_)) {
                        node::clear_tags(&mut hbox.nodes);
                    }
                    h_nodes.push(match group {
                        Group::Leaders(width) => {
                            let mut glue = match width {
                                Some(w) => Node::glue(*w),
                                None => Node::hfillglue(Length::zero()),
                            };
                            if let Node::Glue(g) | Node::HFillGlue(g) = &mut glue {
                                g.explicit = true;
                                g.leader = Some(Leader::Box(Box::new(hbox)));
                            }
                            glue
                        }
                        _ => Node::HBox(hbox),
                    });
                }
            }
        }
        Ok(())
    }

    /// Cut the paragraph's text where its bidi embedding level changes and
    /// shape each piece in the direction of its level (SILE's
    /// `splitNodelistIntoBidiRuns`). Other material counts as an object
    /// replacement character.
    fn split_bidi_runs(&mut self, inlines: &[Inline], direction: Direction) -> Result<Vec<Inline>, BuilderError> {
        let mut text = String::new();
        let mut spans = Vec::with_capacity(inlines.len());
        for inline in inlines {
            let start = text.len();
            match inline {
                Inline::Text(run) => text.push_str(&run.text),
                _ => text.push('\u{FFFC}'),
            }
            spans.push(start..text.len());
        }
        let levels = shaper::bidi_levels(&text, direction);
        let base = u8::from(direction == Direction::RTL);
        let mut out = Vec::with_capacity(inlines.len());
        for (inline, span) in inlines.iter().zip(spans) {
            let Inline::Text(run) = inline else {
                out.push(inline.clone());
                continue;
            };
            let mut lo = span.start;
            while lo < span.end {
                let level = levels[lo];
                let hi = (lo..span.end).find(|&i| levels[i] != level).unwrap_or(span.end);
                let dir = if level % 2 == 1 { Direction::RTL } else { Direction::LTR };
                let mut piece = run.clone();
                piece.text = text[lo..hi].to_string();
                piece.bidi_level = Some(level.saturating_sub(base));
                piece.speaker_change &= lo == span.start;
                piece.font_name = self.font_in_direction(&run.font_name, dir)?;
                for fallback in &mut piece.fallbacks {
                    *fallback = self.font_in_direction(fallback, dir)?;
                }
                out.push(Inline::Text(piece));
                lo = hi;
            }
        }
        Ok(out)
    }

    fn font_in_direction(&mut self, name: &str, direction: Direction) -> Result<String, BuilderError> {
        let font = self.fonts.get(name).ok_or_else(|| BuilderError::NoFont(name.to_string()))?;
        if font.spec.direction == direction {
            return Ok(name.to_string());
        }
        let spec = FontSpec { direction, ..font.spec.clone() };
        self.register_font_spec(spec)
    }

    /// Shape one run and cut it into words, spaces and break penalties
    /// (SILE's unicode node maker).
    fn shape_run(&self, run: &TextRun) -> Result<Vec<Node>, BuilderError> {
        self.shape_run_marking(run, &mut Vec::new())
    }

    /// `shape_run`, noting where punctuation spaces went in `punct`.
    fn shape_run_marking(&self, run: &TextRun, punct: &mut Vec<usize>) -> Result<Vec<Node>, BuilderError> {
        let fonts = std::iter::once(&run.font_name)
            .chain(&run.fallbacks)
            .map(|name| self.fonts.get(name).map(|f| (name.as_str(), f)).ok_or_else(|| BuilderError::NoFont(name.clone())))
            .collect::<Result<Vec<_>, _>>()?;
        let tracking = run.tracking.unwrap_or(1.0);
        let language: Arc<str> = run.language.as_str().into();
        let mut shaped = self.shape_with_fallbacks(&run.text, &fonts, &run.language, run.color);
        for s in &mut shaped {
            s.glyph.width *= tracking;
        }
        if fonts.iter().any(|(_, f)| f.face.has_color_layers()) {
            shaped = shaped.into_iter().flat_map(|s| color_layers(&fonts[s.font].1.face, s)).collect();
        }
        let glyphs: Vec<GlyphItem> = shaped.iter().map(|s| s.glyph.clone()).collect();
        let items: Vec<Item> = shaped.iter().map(|s| s.item).collect();

        let mut nodes = Vec::with_capacity(shaped.len());
        let mut lo = 0;
        while lo < shaped.len() {
            let (font, color) = (shaped[lo].font, shaped[lo].color);
            let hi = (lo..shaped.len()).find(|&i| (shaped[i].font, shaped[i].color) != (font, color)).unwrap_or(shaped.len());
            let (font_name, entry) = fonts[font];
            let font_key: Arc<str> = font_name.into();
            let (face, spec) = (&entry.face, &entry.spec);
            let mut zenkaku = None;
            let space = || crate::word_shaping::shape(&*self.shaper, " ", face, spec).iter().map(|g| g.width).sum::<f64>() * tracking;
            for token in nodemaker::tokenize(&items[lo..hi], run.tokens) {
                match token {
                    Token::Word(range) => {
                        let range = range.start + lo..range.end + lo;
                        let text: String = items[range.clone()].iter().map(|i| i.text).collect();
                        let mut nnode = self.build_nnode(&text, &glyphs[range], &font_key, spec, color);
                        nnode.language = Arc::clone(&language);
                        nnode.bidi_level = run.bidi_level;
                        nnode.tag = run.tag;
                        nodes.push(Node::NNode(nnode));
                    }
                    Token::Space(i) => nodes.push(Node::glue(self.settings.space_settings.word_space(glyphs[i + lo].width, space))),
                    Token::NonBreakingSpace => nodes.push(Node::kern(self.settings.space_settings.measured(space))),
                    Token::Penalty(p) => nodes.push(Node::penalty(p)),
                    Token::RepeatedHyphen => {
                        let hyphen = crate::word_shaping::shape(&*self.shaper, "-", face, spec);
                        let mut nnode = self.build_nnode("-", &hyphen, &font_key, spec, color);
                        nnode.language = Arc::clone(&language);
                        nnode.bidi_level = run.bidi_level;
                        nnode.tag = run.tag;
                        nodes.push(Node::discretionary(vec![], vec![Node::NNode(nnode)], vec![]));
                    }
                    Token::LetterSpace => nodes.push(Node::kern(run.letter_space.unwrap_or_default())),
                    Token::Zenkaku { breakable, width, stretch, shrink } => {
                        let zw = zenkaku.get_or_insert_with(|| self.zenkaku_width());
                        let length = Length::new(Measurement::pt(width * *zw), Measurement::pt(stretch * *zw), Measurement::pt(shrink * *zw));
                        nodes.push(if breakable { Node::glue(length) } else { Node::kern(length) });
                    }
                    Token::PunctSpace(kind) => {
                        let spc = space();
                        let s = self.settings.space_settings;
                        let (w, stretch, shrink) = match kind {
                            PunctSpace::Thin => (0.5 * s.enlargement_factor, 0.0, 0.0),
                            PunctSpace::Colon => (s.enlargement_factor, s.stretch_factor, s.shrink_factor),
                            PunctSpace::Guillemet => (0.8 * s.enlargement_factor, 0.3 * s.stretch_factor, 0.8 * s.shrink_factor),
                        };
                        punct.push(nodes.len());
                        nodes.push(Node::kern(Length::new(
                            Measurement::pt(w * spc),
                            Measurement::pt(stretch * spc),
                            Measurement::pt(shrink * spc),
                        )));
                    }
                }
            }
            lo = hi;
        }
        if run.speaker_change
            && let Some(Node::Glue(g)) = nodes.get(1)
        {
            nodes[1] = Node::kern(Length::new(g.width.length, Measurement::pt(0.0), Measurement::pt(0.0)));
        }
        Ok(nodes)
    }

    /// Shape `text` in the first font, reshaping what it has no glyphs for
    /// in the next, in logical order (SILE's fallback shaper). Like SILE,
    /// the n-th stretch to be shaped falls back to the font after the n-th.
    fn shape_with_fallbacks<'t>(&self, text: &'t str, fonts: &[(&str, &RegisteredFont)], language: &str, color: Option<Color>) -> Vec<Shaped<'t>> {
        struct Pending {
            font: usize,
            start: usize,
            stop: usize,
        }
        let mut runs = std::collections::VecDeque::from([Pending { font: 0, start: 0, stop: text.len() }]);
        let mut shaped: Vec<Shaped> = Vec::with_capacity(text.len());
        let mut popped = 0;
        while let Some(run) = runs.pop_front() {
            let (_, font) = fonts[run.font];
            let chunk = &text[run.start..run.stop];
            let mut glyphs = if font.spec.language.is_empty() {
                let spec = FontSpec { language: language.to_string(), ..font.spec.clone() };
                crate::word_shaping::shape(&*self.shaper, chunk, &font.face, &spec)
            } else {
                crate::word_shaping::shape(&*self.shaper, chunk, &font.face, &font.spec)
            };
            if font.spec.direction == Direction::RTL {
                glyphs.reverse();
            }
            let ends: Vec<usize> = (0..glyphs.len())
                .map(|i| glyphs.get(i + 1).map_or(chunk.len(), |n| n.cluster as usize))
                .collect();
            let next_font = (popped + 1 < fonts.len()).then_some(popped + 1);
            let mut pending: Option<Pending> = None;
            for (mut glyph, end) in glyphs.into_iter().zip(ends) {
                let index = glyph.cluster as usize;
                let found = glyph.gid != 0;
                if !found && pending.is_none() {
                    if let Some(font) = next_font {
                        pending = Some(Pending { font, start: run.start + index, stop: run.start + index });
                        continue;
                    }
                } else if !found {
                    continue;
                } else if let Some(mut p) = pending.take() {
                    p.stop = run.start + index;
                    runs.push_back(p);
                }
                glyph.cluster += run.start as u32;
                let item = Item { text: text.get(run.start + index..run.start + end.max(index)).unwrap_or(""), index: run.start + index };
                shaped.push(Shaped { glyph, item, font: run.font, color });
            }
            if let Some(mut p) = pending {
                p.stop = run.stop;
                runs.push_back(p);
            }
            popped += 1;
        }
        shaped.sort_by_key(|s| s.item.index);
        shaped
    }

    /// Split words at their hyphenation points, with a discretionary at
    /// each (SILE's `hyphenate`).
    fn hyphenate(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Node::NNode(word) = node else {
                out.push(node);
                continue;
            };
            let lang = if word.language.is_empty() { self.settings.language.clone() } else { word.language.to_string() };
            let mut segments = self.hyphenation.hyphenate_word(&word.text, &lang);
            if segments.len() <= 1 || !self.fonts.contains_key(&*word.font_key) {
                out.push(Node::NNode(word));
                continue;
            }
            let mut pieces = Vec::with_capacity(2 * segments.len());
            let mut syllables = 0;
            for j in 0..segments.len() {
                let point = (j + 1 < segments.len()).then(|| hyphenation_point(&lang, &mut segments, j, self.settings.replace_apostrophe_at_hyphenation));
                let nnodes: Vec<Node> = self.text_nodes(&segments[j], &word).into_iter().filter(Node::is_nnode).collect();
                syllables += nnodes.len();
                pieces.extend(nnodes);
                if let Some((prebreak, replacement)) = point {
                    let replacement = replacement.map(|r| self.text_nodes(&r, &word)).unwrap_or_default();
                    pieces.push(Node::discretionary(self.text_nodes(&prebreak, &word), vec![], replacement));
                }
            }
            if let Some(Node::NNode(last)) = pieces.iter_mut().rev().find(|p| p.is_nnode()) {
                last.space_after = word.space_after;
            }
            let parent = Arc::new(node::HyphenatedWord { word, syllables });
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
        let word = self.word_node(text, like);
        #[cfg(debug_assertions)]
        if let Some(word) = &word {
            let nodes = self.run_nodes(text, like);
            debug_assert_eq!(format!("{nodes:?}"), format!("{:?}", [word]), "{text:?} as one word");
        }
        word.map_or_else(|| self.run_nodes(text, like), |w| vec![w])
    }

    /// `text` as a single word in `like`'s font, when it is the hyphen or letters the
    /// node maker would keep together and the font draws them plainly.
    fn word_node(&self, text: &str, like: &NNode) -> Option<Node> {
        let tokens = NodeMakerOptions::for_language(&like.language);
        let letters = |c: char| c.is_alphabetic() && matches!(break_property(c as u32), BreakClass::Alphabetic | BreakClass::HebrewLetter);
        if text.is_empty() || tokens.japanese || tokens.ethiopic || tokens.letterspace || !((text == "-" && !tokens.repeated_hyphen) || text.chars().all(letters)) {
            return None;
        }
        let entry = self.fonts.get(&*like.font_key)?;
        if entry.face.has_color_layers() {
            return None;
        }
        let mut glyphs = if entry.spec.language.is_empty() {
            let spec = FontSpec { language: like.language.to_string(), ..entry.spec.clone() };
            crate::word_shaping::shape(&*self.shaper, text, &entry.face, &spec)
        } else {
            crate::word_shaping::shape(&*self.shaper, text, &entry.face, &entry.spec)
        };
        if entry.spec.direction == Direction::RTL {
            glyphs.reverse();
        }
        glyphs.sort_by_key(|g| g.cluster);
        let mut nnode = self.build_nnode(text, &glyphs, &like.font_key, &entry.spec, like.color);
        nnode.language = like.language.clone();
        nnode.bidi_level = like.bidi_level;
        nnode.tag = like.tag;
        Some(Node::NNode(nnode))
    }

    fn run_nodes(&self, text: &str, like: &NNode) -> Vec<Node> {
        let run = TextRun {
            text: text.to_string(),
            font_name: like.font_key.to_string(),
            color: like.color,
            language: like.language.to_string(),
            tokens: NodeMakerOptions::for_language(&like.language),
            letter_space: None,
            tracking: None,
            fallbacks: Vec::new(),
            speaker_change: false,
            bidi_level: like.bidi_level,
            tag: like.tag,
        };
        self.shape_run(&run).unwrap_or_default()
    }

    fn build_nnode(
        &self,
        text: &str,
        glyphs: &[GlyphItem],
        font_name: &Arc<str>,
        spec: &FontSpec,
        color: Option<Color>,
    ) -> NNode {
        let mut width = 0.0;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut glyph_data = Vec::with_capacity(glyphs.len());
        let misfit = match self.frame_direction().writing {
            Flow::TTB => spec.direction == Direction::LTR,
            _ => spec.direction == Direction::TTB,
        };
        let upright = (spec.direction == Direction::TTB) != misfit;

        for g in glyphs {
            if upright {
                width += g.height;
                height = height.max(g.width);
            } else {
                width += g.width;
                height = height.max(g.height);
                depth = depth.max(g.depth);
            }
            glyph_data.push(GlyphData {
                gid: g.gid,
                width: g.width,
                x_advance: g.x_advance,
                y_advance: g.y_advance,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
                text: g.text.clone(),
            });
        }

        let mut nnode = NNode::with_glyphs(text, glyph_data, Arc::clone(font_name), spec.size, width, height, depth);
        nnode.misfit = misfit;
        nnode.vertical = spec.direction == Direction::TTB;
        nnode.color = color;
        nnode
    }

    /// The space before `vbox` under SILE's `linespacing` package.
    fn line_spacing_leading(&mut self, spacing: LineSpacing, vbox: &VBox, previous_depth: Option<f64>, v_nodes: &mut [Node]) -> Option<Node> {
        let em = self.font_spec().map_or(10.0, |f| f.size);
        let height = pt_of(&vbox.height);
        let Some(previous_depth) = previous_depth else {
            let first = resolve_em(spacing.minimum_first_line, em).length.to_pt().unwrap_or(0.0);
            return (first > 0.0).then(|| Node::vkern(Length::pt(first - height)));
        };
        Some(match spacing.method {
            LineSpacingMethod::Tex => self.settings.baseline_skip?.leading_for(height, Some(previous_depth), em),
            LineSpacingMethod::FitGlyph(extra) => Node::vglue(resolve_em(extra, em)),
            LineSpacingMethod::Fixed(distance) => {
                let d = resolve_em(distance, em);
                Node::vglue(Length::new(Measurement::pt(pt_of(&d) - height - previous_depth), d.stretch, d.shrink))
            }
            LineSpacingMethod::FitFont(extra) => {
                let extra = resolve_em(extra, em);
                let (_, descender, _) = self.previous_vbox(v_nodes).map_or((0.0, 0.0, 0.0), |p| self.line_metrics(&p.nodes, None));
                let (ascender, _, _) = self.line_metrics(&vbox.nodes, None);
                Node::vglue(extra + Length::pt(descender + ascender - height - previous_depth))
            }
            LineSpacingMethod::Css(line_height) => {
                if let Some(previous) = self.previous_vbox(v_nodes) {
                    let (ascender, descender, lh) = self.line_metrics(&previous.nodes, Some(line_height));
                    let half = (lh - ascender - descender) / 2.0;
                    let previous = self.previous_vbox_mut(v_nodes).expect("previous line");
                    previous.height += Length::pt(half);
                    previous.depth += Length::pt(half);
                }
                Node::vglue(Length::zero())
            }
        })
    }

    fn previous_vbox(&self, v_nodes: &[Node]) -> Option<VBox> {
        let last = |nodes: &[Node]| nodes.iter().rev().find_map(|n| if let Node::VBox(v) = n { Some(v.clone()) } else { None });
        last(v_nodes).or_else(|| match self.captures.last() {
            Some(capture) => capture.items.iter().rev().find_map(|i| match i {
                Captured::Vertical(n) => match &**n {
                    Node::VBox(v) => Some(v.clone()),
                    _ => None,
                },
                _ => None,
            }),
            None => last(&self.vertical_queue),
        })
    }

    fn previous_vbox_mut<'v>(&'v mut self, v_nodes: &'v mut [Node]) -> Option<&'v mut VBox> {
        if v_nodes.iter().any(|n| matches!(n, Node::VBox(_))) {
            return v_nodes.iter_mut().rev().find_map(|n| if let Node::VBox(v) = n { Some(v) } else { None });
        }
        match self.captures.last_mut() {
            Some(capture) => capture.items.iter_mut().rev().find_map(|i| match i {
                Captured::Vertical(n) => match &mut **n {
                    Node::VBox(v) => Some(v),
                    _ => None,
                },
                _ => None,
            }),
            None => self.vertical_queue.iter_mut().rev().find_map(|n| if let Node::VBox(v) = n { Some(v) } else { None }),
        }
    }

    /// The largest ascender, descender and CSS line height of the fonts set
    /// directly on a line (SILE's `getLineMetrics`).
    fn line_metrics(&self, nodes: &[Node], line_height: Option<Length>) -> (f64, f64, f64) {
        let mut metrics = (0.0_f64, 0.0_f64, 0.0_f64);
        for node in nodes {
            let Node::NNode(n) = node else { continue };
            let Some(font) = self.fonts.get(&*n.font_key) else { continue };
            let size = font.spec.size;
            metrics.0 = metrics.0.max(font.face.scale(font.face.ascender(), size));
            metrics.1 = metrics.1.max(-font.face.scale(font.face.descender(), size));
            if let Some(lh) = line_height {
                metrics.2 = metrics.2.max(pt_of(&resolve_em(lh, size)));
            }
        }
        metrics
    }

    /// Cut the node list at the breakpoints into lines, add the margin
    /// glue and package each line (SILE's `breakpointsToLines`).
    fn build_lines(
        &mut self,
        h_nodes: Vec<Node>,
        breaks: &[BreakResult],
        direction: Direction,
        reorder: bool,
        skips: LineSkips,
        previous_depth: &mut Option<f64>,
    ) -> Vec<Node> {
        let mut lines: Vec<(VBox, bool, Vec<Node>)> = Vec::new();
        let mut start = 0;
        let mut postbreak: Vec<Node> = Vec::new();
        let mut open_liners: Vec<LinerStyle> = Vec::new();
        let count = h_nodes.len();
        let mut h_nodes = h_nodes.into_iter();

        for br in breaks {
            if br.position == 0 || count == 0 {
                continue;
            }
            let end = br.position.min(count - 1);
            if start > end {
                continue;
            }
            let mut line: Vec<Node> = std::mem::take(&mut postbreak);
            line.extend(h_nodes.by_ref().take(end + 1 - start));
            start = end + 1;
            // Lines holding nothing but discardables (e.g. two breaks in a
            // row) are dropped.
            if line.iter().all(Node::is_discardable) {
                continue;
            }
            let broken = matches!(line.last(), Some(Node::Discretionary(_)));
            if broken
                && let Some(Node::Discretionary(d)) = line.pop()
            {
                line.extend(d.prebreak);
                postbreak = d.postbreak;
            }

            let (start_skip, end_skip, start_hang, end_hang) = match direction {
                Direction::LTR => (skips.left, skips.right, br.left, br.right),
                _ => (skips.right, skips.left, br.right, br.left),
            };
            let hung = |skip: Length, hang: f64| {
                if hang > 0.0 { Length::pt(pt_of(&skip) + hang) } else { skip }
            };
            let (start_skip, end_skip) = (hung(start_skip, start_hang), hung(end_skip, end_hang));
            while line.first().is_some_and(Node::is_discardable) {
                line.remove(0);
            }
            let mut migrating = Vec::new();
            line.retain(|n| match n {
                Node::Migrating(m) => {
                    migrating.extend(m.material.iter().cloned());
                    false
                }
                _ => true,
            });
            let line = rejoin_unbroken_words(line);
            let mut line = reopen_liners(line, &mut open_liners);
            let ratio = line_ratio(br.width, &line, start_skip + end_skip);
            let mut framed = Vec::with_capacity(line.len() + 4);
            framed.extend([Node::zerohbox(), Node::glue(start_skip)]);
            framed.append(&mut line);
            framed.extend([Node::glue(end_skip), Node::zerohbox()]);
            let mut line = rebox_liners(framed);
            if reorder {
                line = reorder_bidi(line, direction);
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
            lines.push((vbox, broken, migrating));
        }

        self.stack_lines(lines, previous_depth)
    }

    /// Lines one under another, with the leading, migrating material and
    /// widow, orphan and broken-line penalties between them.
    fn stack_lines(&mut self, lines: Vec<(VBox, bool, Vec<Node>)>, previous_depth: &mut Option<f64>) -> Vec<Node> {
        let count = lines.len();
        let mut v_nodes = Vec::new();
        let tate = self.in_tate_frame().then(|| self.zenkaku_width());
        for (index, (mut vbox, broken, migrating)) in lines.into_iter().enumerate() {
            let (height, depth) = (pt_of(&vbox.height), pt_of(&vbox.depth));
            if self.grid.is_some() {
                match *previous_depth {
                    Some(previous) => {
                        self.grid.as_mut().expect("grid").cursor += height + previous;
                        v_nodes.push(self.grid_make_up());
                    }
                    None => v_nodes.push(Node::vglue(Length::zero())),
                }
                *previous_depth = Some(depth);
            } else if let (Some(zw), Some(bls)) = (tate, self.settings.baseline_skip) {
                vbox.height = Length::pt(zw);
                let skip = resolve_em(bls.skip, self.font_spec().map_or(10.0, |f| f.size));
                v_nodes.push(Node::vglue(Length::new(Measurement::pt(pt_of(&skip) - zw), skip.stretch, skip.shrink)));
            } else if let Some(spacing) = self.settings.line_spacing {
                let leading = self.line_spacing_leading(spacing, &vbox, *previous_depth, &mut v_nodes);
                v_nodes.extend(leading);
                *previous_depth = Some(depth);
            } else if let Some(bls) = self.settings.baseline_skip {
                v_nodes.push(bls.leading_for(height, *previous_depth, self.font_spec().map_or(10.0, |f| f.size)));
                *previous_depth = Some(depth);
            } else if index > 0 && self.settings.leading > 0.0 {
                v_nodes.push(Node::vglue(Length::new(
                    Measurement::pt(self.settings.leading),
                    Measurement::pt(self.settings.leading * 0.5),
                    Measurement::pt(self.settings.leading * 0.3),
                )));
            }
            v_nodes.push(Node::VBox(vbox));
            v_nodes.extend(migrating);
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

    /// The page template from the margins: a content frame, plus a header
    /// frame inside the top margin area and/or a footer frame inside the
    /// bottom one when heights were reserved (each pushes the content frame
    /// inward by its height plus the gap).
    fn default_template(&self) -> PageTemplate {
        let [top, right, bottom, left] = self.margins;
        let pt = |v: f64| format!("{v}pt");
        let (left, right) = (pt(left), pt(self.paper.width - right));
        let mut frames = Vec::new();
        let mut body_top = top;
        let mut body_bottom = self.paper.height - bottom;
        if self.header_height > 0.0 {
            frames.push(
                FrameSpec::new("header")
                    .left(&left)
                    .right(&right)
                    .top(pt(top))
                    .height(pt(self.header_height)),
            );
            body_top += self.header_height + self.frame_gap;
        }
        if self.footer_height > 0.0 {
            frames.push(
                FrameSpec::new("footer")
                    .left(&left)
                    .right(&right)
                    .bottom(pt(body_bottom))
                    .height(pt(self.footer_height)),
            );
            body_bottom -= self.footer_height + self.frame_gap;
        }
        frames.push(
            FrameSpec::new("content")
                .left(left)
                .right(right)
                .top(pt(body_top))
                .bottom(pt(body_bottom)),
        );
        PageTemplate { frames, first_content_frame: "content".to_string() }
    }
}

// ---------------------------------------------------------------------------
// Cluster-based text extraction
// ---------------------------------------------------------------------------

/// Put a line's nodes in display order: runs at each embedding level above
/// the paragraph's are reversed, innermost last, material without a level
/// of its own takes its neighbours' when they agree, and words set against
/// the paragraph's direction get their glyphs reversed (SILE's bidi
/// `reorder`, parity quirk included).
fn reorder_bidi(mut line: Vec<Node>, direction: Direction) -> Vec<Node> {
    let own: Vec<Option<u8>> = line.iter().map(|n| if let Node::NNode(n) = n { n.bidi_level } else { None }).collect();
    let levels: Vec<u8> = (0..own.len())
        .map(|i| {
            own[i].unwrap_or_else(|| {
                let left = own[..i].iter().rev().find_map(|l| *l);
                let right = own[i + 1..].iter().find_map(|l| *l);
                if left == right { left.unwrap_or(0) } else { 0 }
            })
        })
        .collect();
    let base = u8::from(direction == Direction::RTL);
    for (node, level) in line.iter_mut().zip(&levels) {
        if level % 2 == base {
            continue;
        }
        match node {
            Node::NNode(n) => n.glyphs.reverse(),
            Node::Discretionary(d) => {
                for n in d.replacement.iter_mut().chain(&mut d.prebreak).chain(&mut d.postbreak) {
                    if let Node::NNode(n) = n {
                        n.glyphs.reverse();
                    }
                }
            }
            _ => {}
        }
    }
    let top = levels.iter().copied().max().unwrap_or(0);
    if top == 0 {
        return line;
    }
    let n = line.len();
    let mut matrix: Vec<usize> = (0..n).collect();
    for level in 1..=top {
        let mut start = None;
        for i in 0..n {
            if levels[i] >= level {
                match start {
                    None => start = Some(i),
                    Some(s) if i == n - 1 => {
                        matrix[s..=i].reverse();
                        start = None;
                    }
                    Some(_) => {}
                }
            } else if let Some(s) = start.take() {
                matrix[s..i].reverse();
            }
        }
    }
    let mut out: Vec<Option<Node>> = (0..n).map(|_| None).collect();
    for (node, m) in line.into_iter().zip(matrix) {
        out[m] = Some(node);
    }
    out.into_iter().flatten().collect()
}

fn mark_word_spaces(nodes: &mut [Node]) {
    let mut word = None;
    for i in 0..nodes.len() {
        match &nodes[i] {
            Node::NNode(_) => word = Some(i),
            Node::Glue(_) => {
                if let Some(Node::NNode(n)) = word.take().map(|w| &mut nodes[w]) {
                    n.space_after = true;
                }
            }
            Node::Penalty(_) | Node::Kern(_) => {}
            Node::HBox(b) if matches!(b.ink, Some(Ink::LinerStart(_) | Ink::LinerEnd | Ink::Destination(_) | Ink::Info(_))) => {}
            _ => word = None,
        }
    }
    if let Some(Node::NNode(n)) = word.map(|w| &mut nodes[w]) {
        n.space_after = true;
    }
}

/// Put back whole any hyphenated word whose syllables all landed on this
/// line, so it is set as shaped rather than syllable by syllable.
fn rejoin_unbroken_words(line: Vec<Node>) -> Vec<Node> {
    let mut words = Vec::new();
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
                words.push((i, end, Arc::clone(parent)));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    if words.is_empty() {
        return line;
    }
    let mut out = Vec::with_capacity(line.len());
    let mut words = words.into_iter().peekable();
    for (i, node) in line.into_iter().enumerate() {
        match words.peek() {
            Some((start, end, parent)) if i >= *start => {
                if i == *start {
                    out.push(Node::NNode(parent.word.clone()));
                }
                if i + 1 == *end {
                    words.next();
                }
            }
            _ => out.push(node),
        }
    }
    out
}

fn ink(node: &Node) -> Option<&Ink> {
    match node {
        Node::HBox(b) => b.ink.as_ref(),
        _ => None,
    }
}

/// Reopen at the line's first content the liners still open from earlier
/// lines, and close the ones still open after its last content (SILE's
/// `_repeatEnterLiners` and `_repeatLeaveLiners`).
fn reopen_liners(line: Vec<Node>, open: &mut Vec<LinerStyle>) -> Vec<Node> {
    let is_content = |n: &Node| !n.is_discardable() && !(n.is_glue() && !n.is_explicit()) && !n.is_zero();
    let mut out = Vec::with_capacity(line.len());
    let mut seen_liner = false;
    let mut last_content = None;
    for node in line {
        if is_content(&node) {
            last_content = Some(out.len());
        }
        if !seen_liner && last_content.is_some() {
            if !open.is_empty() {
                out.extend(open.iter().map(|s| liner_mark(Ink::LinerStart(s.clone()))));
                seen_liner = true;
            }
            last_content = Some(out.len());
        }
        match ink(&node) {
            Some(Ink::LinerStart(style)) => {
                open.push(style.clone());
                seen_liner = true;
            }
            Some(Ink::LinerEnd) => {
                open.pop();
            }
            _ => {}
        }
        out.push(node);
    }
    if let Some(i) = last_content {
        for _ in open.iter() {
            out.insert(i + 1, liner_mark(Ink::LinerEnd));
        }
    }
    out
}

/// Wrap the content between liner markers in boxes that draw their stroke
/// (SILE's `_reboxLiners`).
fn rebox_liners(line: Vec<Node>) -> Vec<Node> {
    if !line.iter().any(|n| matches!(ink(n), Some(Ink::LinerEnd))) {
        return line;
    }
    let mut out = Vec::with_capacity(line.len());
    let mut stack: Vec<node::HBox> = Vec::new();
    let append = |b: &mut node::HBox, n: Node| {
        b.width += natural_width(&n);
        b.height = Length::pt(pt_of(&b.height).max(pt_of(&n.height())));
        b.depth = Length::pt(pt_of(&b.depth).max(pt_of(&n.depth())));
        b.nodes.push(n);
    };
    for node in line {
        match ink(&node) {
            Some(Ink::LinerStart(style)) => {
                stack.push(node::HBox { ink: Some(Ink::Liner(style.clone())), ..Default::default() });
            }
            Some(Ink::LinerEnd) => {
                let Some(b) = stack.pop() else { continue };
                if b.nodes.is_empty() {
                    continue;
                }
                match stack.last_mut() {
                    Some(parent) => append(parent, Node::HBox(b)),
                    None => out.push(Node::HBox(b)),
                }
            }
            _ => match stack.last_mut() {
                Some(b) => append(b, node),
                None => out.push(node),
            },
        }
    }
    out
}

fn natural_width(node: &Node) -> Length {
    match node {
        Node::Discretionary(d) => d.replacement.iter().map(Node::width).fold(Length::zero(), |a, b| a + b),
        other => other.width(),
    }
}

/// `D:` and digits, then a `HH'mm'` offset (as SILE checks it).
fn is_pdf_date(date: &str) -> bool {
    let Some(rest) = date.strip_prefix("D:") else { return false };
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    let squashed: String = rest[digits..].chars().filter(|c| !c.is_whitespace()).collect();
    let offset = squashed.strip_prefix('-').unwrap_or("");
    let offset = offset.strip_suffix('\'').unwrap_or(offset);
    digits > 0
        && offset.len() == 5
        && offset.as_bytes()[2] == b'\''
        && offset.bytes().enumerate().all(|(i, b)| i == 2 || b.is_ascii_digit())
}

/// `length` with any `em` parts in points for a font of size `em`.
fn resolve_em(length: Length, em: f64) -> Length {
    let part = |m: Measurement| match m.unit {
        crate::measurement::Unit::Em => Measurement::pt(m.amount * em),
        crate::measurement::Unit::En => Measurement::pt(m.amount * em / 2.0),
        _ => m,
    };
    Length::new(part(length.length), part(length.stretch), part(length.shrink))
}

fn natural_hbox(nodes: Vec<Node>) -> node::HBox {
    let width = nodes.iter().map(natural_width).fold(Length::zero(), |a, b| a + b);
    let mut hbox = node::HBox::new(
        width,
        node::max_node_dim(&nodes, node::Dim::Height),
        node::max_node_dim(&nodes, node::Dim::Depth),
    );
    hbox.nodes = nodes;
    hbox
}

fn liner_mark(ink: Ink) -> Node {
    Node::HBox(node::HBox { ink: Some(ink), ..Default::default() })
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
        natural += natural_width(n);
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

/// A glyph shaped for a run of text, with the font (an index into the
/// run's fonts) and colour it is set in.
struct Shaped<'t> {
    glyph: GlyphItem,
    item: Item<'t>,
    font: usize,
    color: Option<Color>,
}

/// One glyph per layer for a glyph the font draws in coloured layers, the
/// last carrying the advance and text (SILE's `harfbuzzWithColor`).
fn color_layers<'t>(face: &FontFace, s: Shaped<'t>) -> Vec<Shaped<'t>> {
    let Some(layers) = face.color_layers(s.glyph.gid) else {
        return vec![s];
    };
    let last = layers.len().saturating_sub(1);
    layers
        .into_iter()
        .enumerate()
        .map(|(j, (gid, color))| {
            let top = j == last;
            Shaped {
                glyph: GlyphItem {
                    gid,
                    width: if top { s.glyph.width } else { 0.0 },
                    height: if top { s.glyph.height } else { 0.0 },
                    ..s.glyph.clone()
                },
                item: Item { text: if top { s.item.text } else { "" }, index: s.item.index },
                font: s.font,
                color: color.or(s.color),
            }
        })
        .collect()
}

/// What a hyphenation point after `segments[j]` sets before the break, and
/// what it sets when the word stays whole, adjusting the segments for
/// languages whose spelling changes at a break (SILE's `hyphenateSegments`).
fn hyphenation_point(lang: &str, segments: &mut [String], j: usize, replace_apostrophe: bool) -> (String, Option<String>) {
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
        // Turkish: a break at an apostrophe keeps the apostrophe, no
        // hyphen, unless set to swap the apostrophe for a hyphen.
        "tr" => {
            if let Some(next) = segments.get(j + 1)
                && let Some(apostrophe) = next.chars().next().filter(|c| matches!(c, '\'' | '’'))
            {
                segments[j + 1] = next[apostrophe.len_utf8()..].to_string();
                if replace_apostrophe {
                    return ("-".to_string(), None);
                }
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

    /// The committed Gentium Plus, so tests don't depend on what fonts the
    /// machine has.
    fn load_any_system_font() -> Option<(Vec<u8>, String)> {
        Some((crate::class::tests_support::gentium(), "Gentium Plus".to_string()))
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

    #[test]
    fn turkish_breaks_at_an_apostrophe_keep_it_or_swap_it_for_a_hyphen() {
        let segments = || vec!["İstanbul".to_string(), "’dan".to_string()];
        let mut kept = segments();
        assert_eq!(hyphenation_point("tr", &mut kept, 0, false), ("’".into(), Some("’".into())));
        assert_eq!(kept[1], "dan");
        let mut swapped = segments();
        assert_eq!(hyphenation_point("tr", &mut swapped, 0, true), ("-".into(), None));
        assert_eq!(hyphenation_point("en", &mut segments(), 0, true), ("-".into(), None));
    }

    #[test]
    fn vertical_frames_shape_downwards_and_turn_latin_on_its_side() {
        let mut doc = crate::class::tests_support::doc(crate::class::Plain::japanese(true));
        doc.update_font(|f| f.direction = Direction::Frame).unwrap();
        doc.add_text("tate");
        DocumentBuilder::add_latin_in_tate(&mut doc, |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
            d.add_text("yoko");
            Ok(())
        })
        .unwrap();
        doc.new_paragraph().unwrap();
        doc.set_compress(false);
        let layout = doc.lay_out().unwrap();
        let trace = layout.render_debug();
        assert!(trace.contains(";TTB;\n"), "{trace}");
        assert!(trace.contains(";LTR;\n"), "{trace}");
        let pdf = String::from_utf8_lossy(&layout.render().unwrap()).into_owned();
        assert_eq!(pdf.matches("0 -1 1 0 ").count(), 1);
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

    fn rule_lines(trace: &str) -> Vec<Vec<f64>> {
        trace
            .lines()
            .filter_map(|l| l.strip_prefix("Draw line\t"))
            .map(|l| l.split('\t').map(|v| v.parse().unwrap()).collect())
            .collect()
    }

    #[test]
    fn underline_spans_line_breaks() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.add_text("Before ").start_underline();
        doc.add_text("underlined words ".repeat(12)).end_hbox().add_text("after.");
        let rules = rule_lines(&doc.render_debug().unwrap());
        assert!(rules.len() >= 2, "one stroke per line: {rules:?}");
        assert!(rules[1][1] > rules[0][1]);
    }

    #[test]
    fn hrule_and_hrulefill_draw_rules() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.add_hrule(20.0, 5.0, 1.0).add_text("x");
        doc.add_hrulefill(Stroke { raise: 0.0, thickness: 0.2 });
        let rules = rule_lines(&doc.render_debug().unwrap());
        assert_eq!(rules.len(), 2);
        assert_eq!((rules[0][2], rules[0][3]), (20.0, 6.0));
        assert_eq!(rules[1][3], 0.2);
        assert!(rules[1][2] > 100.0, "the fill takes the rest of the line");
    }

    #[test]
    fn leaders_repeat_their_box() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.add_text("A").start_leaders(None).add_text(".").end_hbox().add_text("B");
        let trace = doc.render_debug().unwrap();
        assert!(trace.matches("\t(.)\n").count() > 10, "{trace}");
    }

    #[test]
    fn tracking_scales_advances_but_not_glyph_advances() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.add_text("mini");
        let plain = doc.render_debug().unwrap();
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_tracking(Some(1.5)).add_text("mini");
        let tracked = doc.render_debug().unwrap();
        assert!(plain.contains(" w="), "{plain}");
        assert!(tracked.contains(" a="), "{tracked}");
    }

    #[test]
    fn bidi_reverses_runs_against_the_paragraph_direction() {
        let word = |text: &str, level| {
            let glyphs = text.chars().map(|c| GlyphData { gid: c as u16, ..Default::default() }).collect();
            let mut n = NNode::with_glyphs(text, glyphs, "f", 10.0, 1.0, 0.0, 0.0);
            n.bidi_level = Some(level);
            Node::NNode(n)
        };
        let line = vec![word("ab", 0), Node::glue(Length::pt(1.0)), word("cd", 1), Node::glue(Length::pt(1.0)), word("ef", 1)];
        let texts = |line: &[Node]| -> Vec<String> {
            line.iter()
                .map(|n| match n {
                    Node::NNode(n) => n.glyphs.iter().map(|g| char::from(g.gid as u8)).collect(),
                    _ => " ".to_string(),
                })
                .collect()
        };
        assert_eq!(texts(&reorder_bidi(line.clone(), Direction::LTR)), ["ab", " ", "fe", " ", "dc"]);
        assert_eq!(texts(&reorder_bidi(line, Direction::RTL)), ["ba", " ", "ef", " ", "cd"]);
    }

    #[test]
    fn rtl_frames_set_lines_from_the_right() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_direction(Direction::RTL).set_paragraph_indent(0.0).add_text("one min");
        let trace = doc.render_debug().unwrap();
        let x = |label: &str| {
            let at = trace.find(&format!("({label})")).unwrap();
            let mx = trace[..at].rfind("Mx \t").unwrap();
            trace[mx + 4..].lines().next().unwrap().parse::<f64>().unwrap()
        };
        assert!(trace.find("(min)") < trace.find("(one)"), "{trace}");
        assert!(x("one") < x("min"), "{trace}");
    }

    #[test]
    fn ruby_readings_sit_above_a_base_widened_to_fit() {
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_paragraph_indent(0.0);
        DocumentBuilder::add_ruby(&mut doc, "reading", |d| Ok::<_, BuilderError>(d.add_text("x")).map(|_| ())).unwrap();
        let trace = doc.render_debug().unwrap();
        let y = |label: &str| {
            let at = trace.find(&format!("({label})")).unwrap();
            let my = trace[..at].rfind("My \t").unwrap();
            trace[my + 4..].lines().next().unwrap().parse::<f64>().unwrap()
        };
        assert!(y("x") - y("reading") > 11.0, "{trace}");
        assert!(trace.find("(reading)") < trace.find("(x)"), "{trace}");
    }

    #[test]
    fn list_items_hang_their_labels_in_the_margin() {
        use crate::lists::{ListKind, ListOptions};
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_paragraph_indent(0.0);
        doc.begin_list(ListKind::Enumerate, &ListOptions::default()).unwrap();
        for text in ["one", "two"] {
            doc.begin_item(None).unwrap().add_text(text);
            doc.end_item().unwrap();
        }
        doc.end_list().unwrap();
        let trace = doc.render_debug().unwrap();
        let x = |label: &str| {
            let at = trace.find(&format!("({label})")).unwrap();
            let mx = trace[..at].rfind("Mx \t").unwrap();
            trace[mx + 4..].lines().next().unwrap().parse::<f64>().unwrap()
        };
        assert!((x("1") - x("2")).abs() < 1e-6, "{trace}");
        assert!((x("one") - x("1") - 18.0).abs() < 1e-6, "{trace}");
    }

    #[test]
    fn fixed_line_spacing_sets_baselines_apart() {
        let Some(mut doc) = builder_with_font() else { return };
        let spacing = LineSpacing { method: LineSpacingMethod::Fixed(Length::pt(30.0)), ..Default::default() };
        doc.set_line_spacing(Some(spacing)).add_text("one");
        doc.new_paragraph().unwrap();
        doc.add_text("two");
        let trace = doc.render_debug().unwrap();
        let ys: Vec<f64> = trace.lines().filter_map(|l| l.strip_prefix("My \t")).map(|y| y.parse().unwrap()).collect();
        assert!((ys[1] - ys[0] - 30.0).abs() < 1e-3, "{trace}");
    }

    #[test]
    fn make_columns_splits_the_frame_evenly() {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.make_columns(3, 10.0).unwrap();
        let pages = doc.into_pages().unwrap();
        let frames = &pages[0].frames;
        let width = |id: &str| frames.iter().find(|f| f.id == id).unwrap().width();
        assert!((width("content") - width("content_col1")).abs() < 1e-9);
        assert!((width("content") - width("content_col2")).abs() < 1e-9);
        assert_eq!(width("content_gutter1"), 10.0);
    }

    #[test]
    fn page_frames_apply_to_the_current_page_only() {
        let Some(mut doc) = builder_with_font() else { return };
        let narrow = FrameSpec::new("a").left("100pt").right("300pt").top("100pt").bottom("200pt").next("b");
        let wide = FrameSpec::new("b").left("50pt").right("500pt").top("300pt").bottom("700pt");
        doc.declare_page_frames(&[narrow, wide]).unwrap();
        doc.set_content_frame("a").unwrap();
        doc.add_text("word ".repeat(1500));
        doc.new_paragraph().unwrap();
        let pages = doc.into_pages().unwrap();
        assert!(pages.len() > 1);
        let ids = |p: &Page| p.content.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&pages[0]), ["a", "b"]);
        assert!(ids(&pages[1]).iter().all(|id| id == "content"));
    }

    #[test]
    fn lines_moving_to_a_wider_frame_are_broken_again() {
        let Some(mut doc) = builder_with_font() else { return };
        let narrow = FrameSpec::new("a").left("100pt").right("200pt").top("100pt").bottom("150pt").next("b");
        let wide = FrameSpec::new("b").left("100pt").right("500pt").top("300pt").bottom("700pt");
        doc.declare_page_frames(&[narrow, wide]).unwrap();
        doc.set_content_frame("a").unwrap();
        doc.add_text("word ".repeat(60));
        doc.new_paragraph().unwrap();
        let pages = doc.into_pages().unwrap();
        let (_, wide_lines) = pages[0].content.iter().find(|(id, _)| id == "b").unwrap();
        let first_line = wide_lines.iter().find_map(|n| match n {
            Node::VBox(v) => Some(v.nodes.iter().filter(|n| n.is_nnode()).count()),
            _ => None,
        });
        assert!(first_line.unwrap() > 10, "lines in b use b's width");
    }

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
        doc.set_font_size(24.0).unwrap();
        assert_eq!(doc.font_spec().unwrap().size, 24.0);
        assert_eq!(doc.fonts["body"].spec.size, 12.0);
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
    fn pdf_dates_need_an_offset() {
        assert!(is_pdf_date("D:19990209153925 - 08 ' 00 '"));
        assert!(is_pdf_date("D:19990209153925-08'00"));
        assert!(!is_pdf_date("should fail"));
        assert!(!is_pdf_date("D:19990209153925"));
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

#[cfg(test)]
mod parallel_tests {
    use super::*;
    use crate::class::tests_support::*;
    use crate::class::Plain;

    /// How far down its frame the line holding `text` starts.
    fn offset_of(page: &Page, frame: &str, text: &str) -> Option<f64> {
        let (_, nodes) = page.content.iter().find(|(id, _)| id == frame)?;
        let mut y = 0.0;
        for node in nodes {
            if let Node::VBox(b) = node
                && b.nodes.iter().any(|n| matches!(n, Node::NNode(n) if n.text == text))
            {
                return Some(y);
            }
            y += pt_of(&node.height()) + pt_of(&node.depth());
        }
        None
    }

    #[test]
    fn sync_lines_up_what_comes_next_in_each_flow() {
        let mut d = doc(Plain::new());
        d.declare_frames(&[
            FrameSpec::new("left").top("top(content)").bottom("bottom(content)").left("left(content)").right("48%pw"),
            FrameSpec::new("right").top("top(content)").bottom("bottom(content)").left("52%pw").right("right(content)"),
        ])
        .unwrap();
        d.begin_parallel(&[("left", "left"), ("right", "right")]).unwrap();
        d.select_parallel("left").unwrap().add_text("One");
        d.new_paragraph().unwrap().add_text("Two");
        d.select_parallel("right").unwrap().add_text("Un");
        d.sync_parallel().unwrap();
        d.select_parallel("left").unwrap().add_text("Three");
        d.select_parallel("right").unwrap().add_text("Trois");
        d.sync_parallel().unwrap();
        let pages = d.into_pages().unwrap();
        let left = offset_of(&pages[0], "left", "Three").unwrap();
        assert!(left > 0.0);
        assert!((left - offset_of(&pages[0], "right", "Trois").unwrap()).abs() < 1e-6);
    }
}
