use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::color::Color;
use crate::counter::MultilevelCounter;
use crate::font::{Direction, FontDatabase, FontError, FontFace, FontSpec};
use crate::class::{DocumentClass, PageTemplate};
use crate::frame::PaperSize;
use crate::framespec::{self, FrameGeometry, FrameSpec};
use crate::hyphenation::HyphenationDictionary;
use crate::insertion::{InsertionClass, PageInsertions, Stack};
use crate::length::Length;
use crate::linebreak::{self, BreakResult, LinebreakSettings};
use crate::measurement::Measurement;
use crate::node::{self, GlyphData, Ink, Leader, NNode, Node, Stroke, VBox};
use crate::nodemaker::{self, Item, NodeMakerOptions, PunctSpace, Token};
use crate::pagebuilder::{self, Page, PageBreakSettings};
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

#[derive(Clone, PartialEq)]
struct TextRun {
    text: String,
    font_name: String,
    color: Option<Color>,
    language: String,
    tokens: NodeMakerOptions,
    letter_space: Option<Length>,
    tracking: Option<f64>,
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
    Liner(Stroke),
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

pub use crate::pagebuilder::SUPER_EJECT;

impl AsMut<DocumentBuilder> for DocumentBuilder {
    fn as_mut(&mut self) -> &mut DocumentBuilder {
        self
    }
}

struct LaidOut {
    pages: Vec<Page>,
    fonts: BTreeMap<String, RegisteredFont>,
    bookmarks: Vec<Bookmark>,
    pdf_config: PdfConfig,
}

/// The settings that shape text and paragraphs, which can be saved and
/// restored as a whole (SILE's settings state).
#[derive(Clone)]
pub struct Settings {
    font: Option<String>,
    language: String,
    color: Option<Color>,
    direction: Direction,
    skips: LineSkips,
    paragraph_indent: f64,
    paragraph_skip: Length,
    leading: f64,
    baseline_skip: Option<BaselineSkip>,
    space_settings: SpaceSettings,
    obey_spaces: bool,
    fixed_nbsp: bool,
    letter_space: Option<Length>,
    tracking: Option<f64>,
    linebreak_settings: LinebreakSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: None,
            language: "en".to_string(),
            color: None,
            direction: Direction::LTR,
            skips: LineSkips::default(),
            paragraph_indent: 20.0,
            paragraph_skip: Length::zero(),
            leading: 2.0,
            baseline_skip: None,
            space_settings: SpaceSettings::default(),
            obey_spaces: false,
            fixed_nbsp: false,
            letter_space: None,
            tracking: None,
            linebreak_settings: LinebreakSettings::default(),
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
    page: Option<PageState>,
    pages: Vec<Page>,
    last_penalty: i32,
    insertion_classes: std::collections::BTreeMap<String, InsertionClass>,
    insertions: PageInsertions,
    counters: BTreeMap<String, i64>,
    multilevel_counters: BTreeMap<String, MultilevelCounter>,
    /// Typesetting states set aside by `push_typesetter`.
    typesetters: Vec<SavedTypesetter>,

    // Running header/footer (need header_height / footer_height > 0)
    header: Option<RunningText>,
    footer: Option<RunningText>,

    // PDF config
    pdf_config: PdfConfig,
    bookmarks: Vec<Bookmark>,
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
            page: None,
            pages: Vec::new(),
            last_penalty: 0,
            insertion_classes: std::collections::BTreeMap::new(),
            insertions: PageInsertions::default(),
            counters: BTreeMap::new(),
            multilevel_counters: BTreeMap::new(),
            typesetters: Vec::new(),
            header: None,
            footer: None,
            pdf_config: PdfConfig::default(),
            bookmarks: Vec::new(),
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
        self.settings.font = Some(key);
        Ok(self)
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
    pub fn set_baseline_skip(&mut self, baseline_skip: Option<BaselineSkip>) -> &mut Self {
        self.settings.baseline_skip = baseline_skip;
        self
    }

    pub fn set_leading(&mut self, leading: f64) -> &mut Self {
        self.settings.leading = leading;
        self
    }

    pub fn set_direction(&mut self, direction: Direction) -> &mut Self {
        self.settings.direction = direction;
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
        if self.current_list().is_empty() && !self.settings.obey_spaces {
            text = text.trim_start().to_string();
            if text.is_empty() {
                return self;
            }
        }
        let run = self.text_run(text);
        self.push_inline(Inline::Text(run));
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

    fn text_run(&self, text: String) -> TextRun {
        let tokens = NodeMakerOptions {
            obey_spaces: self.settings.obey_spaces,
            fixed_nbsp: self.settings.fixed_nbsp,
            letterspace: self.settings.letter_space.is_some(),
            ..NodeMakerOptions::for_language(&self.settings.language)
        };
        TextRun {
            text,
            font_name: self.settings.font.clone().unwrap_or_default(),
            color: self.settings.color,
            language: self.settings.language.clone(),
            tokens,
            letter_space: self.settings.letter_space,
            tracking: self.settings.tracking,
        }
    }

    /// A break opportunity: `prebreak` ends the line and `postbreak` starts
    /// the next if the paragraph breaks here, and `replacement` is set if it
    /// does not (SILE's `\discretionary`).
    pub fn add_discretionary(&mut self, prebreak: Option<&str>, postbreak: Option<&str>, replacement: Option<&str>) -> &mut Self {
        let run = |t: Option<&str>| t.map(|t| self.text_run(t.to_string()));
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
            self.push_inline(Inline::Box(group, content));
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

    /// Fill glue of `width` (as much as possible when `None`) with copies of
    /// the material added until `end_hbox`, lined up across lines by the
    /// frame's end edge (SILE's `\leaders`).
    pub fn start_leaders(&mut self, width: Option<Length>) -> &mut Self {
        self.open_boxes.push((Group::Leaders(width), Vec::new()));
        self
    }

    /// Draw `stroke` along the material added until `end_hbox`, across line
    /// breaks (SILE's liners).
    pub fn start_liner(&mut self, stroke: Stroke) -> &mut Self {
        self.open_boxes.push((Group::Liner(stroke), Vec::new()));
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

    /// SILE's `initline`: a paragraph opens with a zero box and its indent.
    fn push_inline(&mut self, item: Inline) {
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
            self.push_vertical(Node::vglue(self.settings.paragraph_skip));
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

    /// Vertical glue added straight to the vertical list, ahead of the
    /// lines of any paragraph still in progress (SILE's `pushVglue`).
    pub fn push_vglue(&mut self, height: impl Into<Length>) -> &mut Self {
        self.push_vertical(Node::vglue(height.into()));
        self
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
        self.push_vertical(glue);
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
        match (&self.master, &self.class) {
            (Some(master), _) => master.clone(),
            (None, Some(class)) => class.page_template(),
            (None, None) => self.default_template(),
        }
    }

    fn start_page(&mut self) -> Result<(), BuilderError> {
        let template = self.page_template();
        let frames = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &template.frames)
            .map_err(|e| BuilderError::Layout(e.to_string()))?;
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
        Ok(())
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
        let page = &mut self.page.as_mut().expect("page").page;
        for frame in solved.into_iter().filter(|g| frames.iter().any(|f| f.id == g.id)) {
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
        let target = frame.height() - self.insertions.shrinkage(&id);
        let (classes, insertions) = (&self.insertion_classes, &mut self.insertions);
        let mut on_insertion = |queue: &mut Vec<Node>, i, height, target| {
            insertions.process(classes, &id, queue, i, height, target)
        };
        let Some(br) = pagebuilder::find_break(&mut self.vertical_queue, target, false, &mut on_insertion) else {
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
        let old_width = self.current_frame().map(FrameGeometry::width);
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
        let new_width = self.current_frame().map(FrameGeometry::width);
        if !self.vertical_queue.is_empty() && old_width.zip(new_width).is_some_and(|(a, b)| (a - b).abs() > 1e-6) {
            self.push_back()?;
            if self.typesetters.is_empty() && self.build_page()? {
                self.init_next_frame()?;
            }
        } else if !self.vertical_queue.is_empty() {
            self.vertical_queue.insert(0, Node::vglue(Length::zero()));
        }
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
        let hsize = self.current_frame().map_or(0.0, FrameGeometry::width);
        let (left, right) = margins.unwrap_or_default();
        let skips = LineSkips { left, right, ..self.settings.skips };
        let mut previous_depth = self.previous_depth;
        let lines = self.break_nodes(std::mem::take(nodes), hsize, self.settings.direction, skips, &mut previous_depth);
        self.previous_depth = previous_depth;
        self.vertical_queue.extend(lines);
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        self.output_insertions();
        if let Some(mut class) = self.class.take() {
            let result = class.end_page(self);
            self.class = Some(class);
            result?;
        }
        if let Some(state) = self.page.take() {
            self.pages.push(state.page);
        }
        Ok(())
    }

    fn new_page(&mut self) -> Result<(), BuilderError> {
        if let Some(mut class) = self.class.take() {
            let result = class.new_page(self);
            self.class = Some(class);
            result?;
        }
        self.start_page()
    }

    /// Fill the last page and end it (SILE's `class:finish`).
    fn finish(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
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

    // -- Bookmarks and links ------------------------------------------------

    pub fn add_bookmark(&mut self, title: impl Into<String>, level: u32) -> &mut Self {
        self.bookmarks.push(Bookmark {
            title: title.into(),
            page_index: self.pages.len(),
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
        pdf.render_pages(&laid.pages);
        Ok(pdf.finish()?)
    }

    /// Lay the document out and describe it in SILE's debug outputter format,
    /// for comparing layout against SILE's regression test expectations.
    pub fn render_debug(self) -> Result<String, BuilderError> {
        let paper = self.paper;
        let laid = self.lay_out()?;
        let mut trace = crate::trace::TraceCanvas::new(paper);
        for (name, entry) in &laid.fonts {
            trace.register_font(name, &entry.spec);
        }
        crate::render::draw_pages(&laid.pages, &mut trace);
        Ok(trace.finish())
    }

    /// Lay the document out and hand back its pages.
    pub fn into_pages(self) -> Result<Vec<Page>, BuilderError> {
        Ok(self.lay_out()?.pages)
    }

    fn lay_out(mut self) -> Result<LaidOut, BuilderError> {
        self.finish()?;
        let mut pages = std::mem::take(&mut self.pages);

        // Running header/footer: typeset per page (page numbers differ)
        let total = pages.len();
        for (name, running) in [("header", self.header.clone()), ("footer", self.footer.clone())] {
            let Some(running) = running else { continue };
            for page in pages.iter_mut() {
                let Some(hsize) = page.frame(name).map(|f| f.width()) else { continue };
                let line = running.resolved(page.number, total, &self.settings.language);
                let skips = LineSkips::default().aligned(running.align);
                let nodes = self.typeset_inlines(&line, hsize, running.direction, skips, &mut None)?;
                page.add_frame_content(name, nodes);
            }
        }

        Ok(LaidOut {
            pages,
            fonts: self.fonts,
            bookmarks: self.bookmarks,
            pdf_config: self.pdf_config,
        })
    }

    // -- Internal: paragraph typesetting ------------------------------------

    fn typeset_paragraph(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        self.ensure_page()?;
        let hsize = self.current_frame().map_or(0.0, |f| f.width());
        let mut previous_depth = self.previous_depth;
        let nodes = self.typeset_inlines(
            inlines,
            hsize,
            self.settings.direction,
            self.settings.skips,
            &mut previous_depth,
        )?;
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
        let h_nodes = self.shape_inlines(inlines)?;
        Ok(self.break_nodes(h_nodes, hsize, direction, skips, previous_depth))
    }

    /// Break shaped paragraph material into lines (the rest of SILE's
    /// `boxUpNodes`).
    fn break_nodes(
        &mut self,
        mut h_nodes: Vec<Node>,
        hsize: f64,
        direction: Direction,
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
        let (h_nodes, breaks) = linebreak::break_paragraph(h_nodes, hsize, &lb_settings, |nodes| self.hyphenate(nodes));
        self.build_lines(&h_nodes, &breaks, direction, skips, previous_depth)
    }

    fn shape_inlines(&mut self, inlines: &[Inline]) -> Result<Vec<Node>, BuilderError> {
        let mut h_nodes = Vec::new();
        for item in inlines {
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
                    h_nodes.push(liner_mark(Ink::LinerStart(*stroke)));
                    h_nodes.extend(self.shape_inlines(content)?);
                    h_nodes.push(liner_mark(Ink::LinerEnd));
                }
                Inline::Box(group, content) => {
                    let hbox = natural_hbox(self.shape_inlines(content)?);
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
        let tracking = run.tracking.unwrap_or(1.0);
        for g in &mut glyphs {
            g.width *= tracking;
        }
        let space = || self.shaper.shape(" ", &face, &spec).iter().map(|g| g.width).sum::<f64>() * tracking;
        let rtl = spec.direction == Direction::RTL;
        if rtl {
            glyphs.reverse();
        }
        let mut items: Vec<Item> = glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| {
                let start = g.cluster as usize;
                let end = glyphs.get(i + 1).map_or(run.text.len(), |n| n.cluster as usize);
                Item { text: run.text.get(start..end).unwrap_or(""), index: start }
            })
            .collect();
        let mut colors = vec![run.color; glyphs.len()];
        if face.has_color_layers() {
            (glyphs, items, colors) = color_layers(&face, glyphs, items, run.color);
        }

        let mut nodes = Vec::new();
        let mut lo = 0;
        while lo < glyphs.len() {
            let color = colors[lo];
            let hi = (lo..glyphs.len()).find(|&i| colors[i] != color).unwrap_or(glyphs.len());
            for token in nodemaker::tokenize(&items[lo..hi], run.tokens) {
                match token {
                    Token::Word(range) => {
                        let range = range.start + lo..range.end + lo;
                        let text: String = items[range.clone()].iter().map(|i| i.text).collect();
                        let mut word = glyphs[range].to_vec();
                        if rtl {
                            word.reverse();
                        }
                        let mut nnode = self.build_nnode(&text, &word, &run.font_name, &spec, color);
                        nnode.language = run.language.clone();
                        nodes.push(Node::NNode(nnode));
                    }
                    Token::Space(i) => nodes.push(Node::glue(self.settings.space_settings.word_space(glyphs[i + lo].width, space))),
                    Token::NonBreakingSpace => nodes.push(Node::kern(self.settings.space_settings.measured(space))),
                    Token::Penalty(p) => nodes.push(Node::penalty(p)),
                    Token::RepeatedHyphen => {
                        let hyphen = self.shaper.shape("-", &face, &spec);
                        let mut nnode = self.build_nnode("-", &hyphen, &run.font_name, &spec, color);
                        nnode.language = run.language.clone();
                        nodes.push(Node::discretionary(vec![], vec![Node::NNode(nnode)], vec![]));
                    }
                    Token::LetterSpace => nodes.push(Node::kern(run.letter_space.unwrap_or_default())),
                    Token::PunctSpace(kind) => {
                        let spc = space();
                        let s = self.settings.space_settings;
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
            lo = hi;
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
            let lang = if word.language.is_empty() { self.settings.language.clone() } else { word.language.clone() };
            let mut segments = self.hyphenation.hyphenate_word(&word.text, &lang);
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
            tracking: None,
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
            width += g.width;
            height = height.max(g.height);
            depth = depth.max(g.depth);
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

        let mut nnode = NNode::with_glyphs(text, glyph_data, font_name, spec.size, width, height, depth);
        nnode.color = color;
        nnode.language = self.settings.language.clone();
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
        let mut lines: Vec<(VBox, bool, Vec<Node>)> = Vec::new();
        let mut start = 0;
        let mut postbreak: Vec<Node> = Vec::new();
        let mut open_liners: Vec<Stroke> = Vec::new();

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
            line.insert(0, Node::glue(start_skip));
            line.insert(0, Node::zerohbox());
            line.push(Node::glue(end_skip));
            line.push(Node::zerohbox());
            let mut line = rebox_liners(line);
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
            lines.push((vbox, broken, migrating));
        }

        let count = lines.len();
        let mut v_nodes = Vec::new();
        for (index, (vbox, broken, migrating)) in lines.into_iter().enumerate() {
            let (height, depth) = (pt_of(&vbox.height), pt_of(&vbox.depth));
            if let Some(bls) = self.settings.baseline_skip {
                v_nodes.push(bls.leading_for(height, *previous_depth));
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

fn ink(node: &Node) -> Option<Ink> {
    match node {
        Node::HBox(b) => b.ink,
        _ => None,
    }
}

/// Reopen at the line's first content the liners still open from earlier
/// lines, and close the ones still open after its last content (SILE's
/// `_repeatEnterLiners` and `_repeatLeaveLiners`).
fn reopen_liners(line: Vec<Node>, open: &mut Vec<Stroke>) -> Vec<Node> {
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
                out.extend(open.iter().map(|&s| liner_mark(Ink::LinerStart(s))));
                seen_liner = true;
            }
            last_content = Some(out.len());
        }
        match ink(&node) {
            Some(Ink::LinerStart(stroke)) => {
                open.push(stroke);
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
    if !line.iter().any(|n| ink(n).is_some_and(|i| i == Ink::LinerEnd)) {
        return line;
    }
    let mut out = Vec::with_capacity(line.len());
    let mut stack: Vec<node::HBox> = Vec::new();
    let append = |b: &mut node::HBox, n: Node| {
        b.width = Length::pt(pt_of(&b.width) + pt_of(&natural_width(&n)));
        b.height = Length::pt(pt_of(&b.height).max(pt_of(&n.height())));
        b.depth = Length::pt(pt_of(&b.depth).max(pt_of(&n.depth())));
        b.nodes.push(n);
    };
    for node in line {
        match ink(&node) {
            Some(Ink::LinerStart(stroke)) => {
                stack.push(node::HBox { ink: Some(Ink::Liner(stroke)), ..Default::default() });
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

fn natural_hbox(nodes: Vec<Node>) -> node::HBox {
    let width: f64 = nodes.iter().map(|n| pt_of(&natural_width(n))).sum();
    let mut hbox = node::HBox::new(
        Length::pt(width),
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

/// What a hyphenation point after `segments[j]` sets before the break, and
/// what it sets when the word stays whole, adjusting the segments for
/// languages whose spelling changes at a break (SILE's `hyphenateSegments`).
/// Replace each glyph the font draws in coloured layers with one glyph
/// per layer, the last carrying the advance and text (SILE's
/// `harfbuzzWithColor` shaper).
fn color_layers<'t>(
    face: &FontFace,
    glyphs: Vec<GlyphItem>,
    items: Vec<Item<'t>>,
    color: Option<Color>,
) -> (Vec<GlyphItem>, Vec<Item<'t>>, Vec<Option<Color>>) {
    let mut out = (Vec::new(), Vec::new(), Vec::new());
    for (g, item) in glyphs.into_iter().zip(items) {
        let Some(layers) = face.color_layers(g.gid) else {
            out.0.push(g);
            out.1.push(item);
            out.2.push(color);
            continue;
        };
        let last = layers.len().saturating_sub(1);
        for (j, (gid, layer_color)) in layers.into_iter().enumerate() {
            let top = j == last;
            out.0.push(GlyphItem {
                gid,
                width: if top { g.width } else { 0.0 },
                height: if top { g.height } else { 0.0 },
                ..g.clone()
            });
            out.1.push(Item { text: if top { item.text } else { "" }, index: item.index });
            out.2.push(layer_color.or(color));
        }
    }
    out
}

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
        doc.add_text("road");
        let plain = doc.render_debug().unwrap();
        let Some(mut doc) = builder_with_font() else { return };
        doc.set_tracking(Some(1.5)).add_text("road");
        let tracked = doc.render_debug().unwrap();
        assert!(plain.contains(" w="), "{plain}");
        assert!(tracked.contains(" a="), "{tracked}");
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
