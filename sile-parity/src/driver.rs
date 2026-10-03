//! Interprets a SIL tree by calling rile's `DocumentBuilder`, emulating
//! SILE's `plain` and `book` classes closely enough to compare layouts.
//! Anything outside the supported subset is reported rather than approximated.

use std::sync::Arc;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::FromStr;

use rile_pages::bible::Bible;
use rile::builder::{Arranger, BaselineSkip, BuilderError, Context, FontFallback, ItalicCorrection, LineSkips, LineSpacing, LineSpacingMethod, TextAlign, Typesetter};
use rile_pages::DocumentBuilder;
use rile::counter::format_number;
use rile::color::Color;
use rile_pages::class::{
    diglot, pecha, triglot, Book, Folio, FolioState, Hanmen, Heading, Letter, LetterPart, LetterParts, PageTemplate, Plain, DIGLOT_FLOWS, TRIGLOT_FLOWS,
};
use rile::chords;
use rile::date::DateTime;
use rile_pages::insertion::InsertionClass;
use rile::node::{HBox, Ink, LinkDest, Node, Stroke};
use rile_pages::toc::{DefaultTocStyle, TableOfContents};
use rile::pullquote::Pullquote;
use rile::dropcap::Dropcap;
use rile::svg_image::SvgImage;
use rile_pages::index::{DefaultIndexStyle, Indexer};
use rile::bibliography::{Bibliography, Cite};
use rile_pages::cropmarks::Cropmarks;
use rile::features::OtFeatures;
use rile::image::{Background, BackgroundFill, Image};
use rile::url::{url_pieces, UrlPenalties, UrlPiece};
use rile::font::{Direction, FontSpec, FontStyle, FontWeight};
use rile::frame::PaperSize;
use rile::frame::FrameDirection;
use rile_pages::framespec::FrameSpec;
use rile::length::Length;
use rile::lists::{ListKind, ListOptions};
use rile::math::mathml;
use rile::math::{MathLength, MathMode, MathNode, TexMath};
use rile::measurement::{Measurement, Unit};
use rile::node::INFINITY;
use rile::shaper::SpaceSettings;

use crate::fonts::Fonts;
use crate::ports::{self, Port};
use crate::sil::{self, Command, Content};

#[derive(Debug)]
pub enum Failure {
    /// The test uses features the reader or engine does not support yet.
    Unsupported(BTreeSet<String>),
    Error(String),
}

pub struct Corpus<'a> {
    pub fonts: &'a Fonts,
    pub font_dir: &'a Path,
    pub lorem: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub enum Format {
    Sil,
    Xml,
}

impl Format {
    /// SILE's content sniffing for inputs without a telling extension.
    pub fn detect(src: &str) -> Option<Self> {
        match src.trim_start().chars().next()? {
            '<' => Some(Format::Xml),
            '\\' => Some(Format::Sil),
            _ => None,
        }
    }
}

/// Typeset a SIL or XML document and return its trace in SILE's debug format.
pub fn run(name: &str, src: &str, format: Format, corpus: &Corpus) -> Result<String, Failure> {
    let tree = match format {
        Format::Sil => sil::parse(src),
        Format::Xml => crate::xml::parse(src),
    }
    .map_err(|e| Failure::Error(format!("parse: {}", e.0)))?;
    let tree = expand_includes(tree, &corpus.font_dir.with_file_name("sile").join("tests"))
        .map_err(Failure::Error)?;
    let mut missing = BTreeSet::new();
    let port = ports::port(name);
    check(&tree, corpus, port.as_ref(), &mut BTreeSet::new(), &mut missing);
    if !missing.is_empty() {
        return Err(Failure::Unsupported(missing));
    }
    let mut d = Driver::new(corpus).map_err(Failure::Error)?;
    d.port = port;
    d.process(&tree).map_err(Failure::Error)?;
    d.finish().map_err(Failure::Error)
}

/// Replace `\include[src=...]` with the content of the file it names
/// (an included `\document` contributes its content only).
fn expand_includes(tree: Vec<Content>, dir: &Path) -> Result<Vec<Content>, String> {
    let mut out = Vec::with_capacity(tree.len());
    for item in tree {
        match item {
            Content::Command(cmd) if cmd.name == "include" => {
                let src = cmd.option("src").ok_or("include without src")?;
                let text = std::fs::read_to_string(dir.join(src)).map_err(|e| format!("include {src}: {e}"))?;
                let included = sil::parse(&text).map_err(|e| format!("parse {src}: {}", e.0))?;
                for c in expand_includes(included, dir)? {
                    match c {
                        Content::Command(doc) if doc.name == "document" => out.extend(doc.content.unwrap_or_default()),
                        other => out.push(other),
                    }
                }
            }
            Content::Command(mut cmd) => {
                if let Some(content) = cmd.content.take() {
                    cmd.content = Some(expand_includes(content, dir)?);
                }
                out.push(Content::Command(cmd));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Support check
// ---------------------------------------------------------------------------

const SIMPLE_COMMANDS: &[&str] = &[
    "par",
    "ruby",
    "latin-in-tate",
    "breakframevertical",
    "sync",
    "save-book-title",
    "save-chapter-number",
    "verse-number",
    "pdf:metadata",
    "pdf:destination",
    "rebox",
    "grid",
    "grid:debug",
    "no-grid",
    "fluent",
    "ftl",
    "add-font-feature",
    "remove-font-feature",
    "dropcap",
    "pullquote",
    "img",
    "svg",
    "raw",
    "cropmarks:setup",
    "indexentry",
    "printindex",
    "loadbibliography",
    "bibliographystyle",
    "cite",
    "cites",
    "nocite",
    "reference",
    "printbibliography",
    "rotate",
    "scalebox",
    "table",
    "background",
    "url",
    "href",
    "code",
    "pdf:bookmark",
    "pdf:link",
    "tocentry",
    "tableofcontents",
    "show-hanmen",
    "bidi-off",
    "bidi-on",
    "thisframeRTL",
    "thisframeLTR",
    "verbatim",
    "font:add-fallback",
    "font:remove-fallback",
    "font:clear-fallbacks",
    "itemize",
    "enumerate",
    "item",
    "color",
    "unichar",
    "em",
    "strong",
    "noindent",
    "neverindent",
    "indent",
    "smallskip",
    "blockquote",
    "medskip",
    "bigskip",
    "pagebreak",
    "framebreak",
    "eject",
    "supereject",
    "break",
    "cr",
    "nobreak",
    "novbreak",
    "allowbreak",
    "goodbreak",
    "filbreak",
    "penalty",
    "glue",
    "kern",
    "hfill",
    "vfill",
    "skip",
    "hbox",
    "quad",
    "qquad",
    "thinspace",
    "noop",
    "center",
    "raggedright",
    "raggedleft",
    "justified",
    "ragged",
    "lorem",
    "nofolios",
    "folios",
    "comment",
    "language",
    "nofoliothispage",
    "process",
    "open-double-page",
    "open-spread",
    "left-running-head",
    "right-running-head",
    "discretionary",
    "increment-counter",
    "set-counter",
    "show-counter",
    "increment-multilevel-counter",
    "set-multilevel-counter",
    "show-multilevel-counter",
    "footnote",
    "footnote:separator",
    "footnote:options",
    "raise",
    "lower",
    "hyphenator:add-exceptions",
    "chapter",
    "section",
    "subsection",
    "pagetemplate",
    "frame",
    "define-master-template",
    "switch-master",
    "switch-master-one-page",
    "showframe",
    "makecolumns",
    "ifattop",
    "repertoire",
    "pangrams",
    "set-to-width",
    "boustrophedon",
    "ch",
    "chordmode",
    "ifnotattop",
    "letter",
    "sender",
    "recipient",
    "salutation",
    "date",
    "balancecolumns",
    "hrule",
    "hrulefill",
    "fullrule",
    "underline",
    "strikethrough",
    "leaders",
    "dotfill",
    " ",
];

const FONT_OPTIONS: &[&str] =
    &["family", "size", "style", "weight", "language", "features", "variations", "filename", "adjust", "direction", "script"];

const SETTINGS: &[&str] = &[
    "ruby.opentype",
    "ruby.height",
    "ruby.latinspacer",
    "font.family",
    "font.size",
    "font.features",
    "font.variations",
    "font.filename",
    "font.style",
    "font.weight",
    "document.parindent",
    "document.parskip",
    "document.baselineskip",
    "document.lineskip",
    "document.lskip",
    "document.rskip",
    "document.language",
    "document.letterspaceglue",
    "shaper.tracking",
    "shaper.variablespaces",
    "languages.am.justification",
    "typesetter.fixedSpacingAfterInitialEmdash",
    "typesetter.softHyphen",
    "languages.tr.replaceApostropheAtHyphenation",
    "typesetter.breakwidth",
    "harfbuzz.subshapers",
    "dropcaps.bsratio",
    "linespacing.method",
    "linespacing.fixed.baselinedistance",
    "linespacing.fit-glyph.extra-space",
    "linespacing.fit-font.extra-space",
    "linespacing.css.line-height",
    "linespacing.minimumfirstlineposition",
    "document.spaceskip",
    "lists.parskip",
    "lists.enumerate.leftmargin",
    "lists.enumerate.labelindent",
    "lists.itemize.leftmargin",
    "typesetter.obeyspaces",
    "languages.fixedNbsp",
    "current.parindent",
    "typesetter.parfillskip",
    "linebreak.tolerance",
    "chordmode.offset",
    "shaper.complexspaces",
    "linebreak.pretolerance",
    "linebreak.emergencyStretch",
    "linebreak.hyphenPenalty",
    "linebreak.adjdemerits",
    "shaper.spaceenlargementfactor",
    "shaper.spacestretchfactor",
    "shaper.spaceshrinkfactor",
    "typesetter.italicCorrection",
    "math.font.family",
    "math.font.filename",
    "math.font.size",
    "math.font.weight",
    "math.font.script.feature",
    "math.displayskip",
    "math.predisplaypenalty",
    "math.postdisplaypenalty",
];

fn check(
    content: &[Content],
    corpus: &Corpus,
    port: Option<&Port>,
    defined: &mut BTreeSet<String>,
    missing: &mut BTreeSet<String>,
) {
    for c in content {
        let Content::Command(cmd) = c else { continue };
        match cmd.name.as_str() {
            "frame" => {
                for (k, v) in &cmd.options {
                    match k.as_str() {
                        "id" | "left" | "right" | "top" | "bottom" | "width" | "height" | "next" | "balanced" => {}
                        "direction" if FrameDirection::parse(v).is_some() => {}
                        "direction" => {
                            missing.insert(format!("frame[direction={v}]"));
                        }
                        _ => {
                            missing.insert(format!("frame[{k}]"));
                        }
                    }
                }
            }
            "pagetemplate" => {
                for (k, _) in &cmd.options {
                    if k != "first-content-frame" {
                        missing.insert(format!("pagetemplate[{k}]"));
                    }
                }
            }
            "document" => {
                let flows: &[(&str, &str)] = match cmd.option("class") {
                    Some("diglot") => &DIGLOT_FLOWS,
                    Some("triglot") => &TRIGLOT_FLOWS,
                    _ => &[],
                };
                defined.extend(flows.iter().map(|(name, _)| name.to_string()));
                if let Some(class) = cmd.option("class").filter(|c| !matches!(*c, "plain" | "book" | "bible" | "jplain" | "jbook" | "tplain" | "tbook" | "letter" | "diglot" | "triglot" | "pecha")) {
                    missing.insert(format!("class={class}"));
                }
                if let Some(p) = cmd.option("papersize")
                    && paper_size(p).is_none()
                {
                    missing.insert(format!("papersize={p}"));
                }
                for (k, v) in &cmd.options {
                    match k.as_str() {
                        "class" | "papersize" | "landscape" => {}
                        "sheetsize" if paper_size(v).is_some() => {}
                        "direction" if direction(v).is_some() => {}
                        "layout" if matches!(v.as_str(), "yoko" | "tate") => {}
                        _ => {
                            missing.insert(format!("document[{k}={v}]"));
                        }
                    }
                }
            }
            "use" => match cmd.option("module") {
                Some(
                    "packages.retrograde"
                    | "packages.lorem"
                    | "packages.footnotes"
                    | "packages.rules"
                    | "packages.leaders"
                    | "packages.masters"
                    | "packages.frametricks"
                    | "packages.balanced-frames"
                    | "packages.date"
                    | "packages.ifattop"
                    | "packages.chordmode"
                    | "packages.boustrophedon"
                    | "packages.complex-spaces"
                    | "packages.specimen"
                    | "packages.pagebuilder-bestfit"
                    | "packages.counters"
                    | "packages.color"
                    | "packages.unichar"
                    | "packages.lists"
                    | "packages.verbatim"
                    | "packages.linespacing"
                    | "packages.font-fallback"
                    | "packages.ruby"
                    | "packages.hanmenkyoshi"
                    | "packages.color-fonts"
                    | "packages.bidi"
                    | "packages.pdf"
                    | "packages.tableofcontents"
                    | "packages.rebox"
                    | "packages.url"
                    | "packages.svg"
                    | "packages.cropmarks"
                    | "packages.autodoc"
                    | "packages.indexer"
                    | "packages.bibtex"
                    | "packages.rotate"
                    | "packages.scalebox"
                    | "packages.simpletable"
                    | "packages.grid"
                    | "packages.features"
                    | "packages.dropcaps"
                    | "packages.pullquote"
                    | "packages.image"
                    | "packages.background"
                    | "packages.math",
                ) => {}
                Some(m) if m.starts_with("inc.") && port.is_some() => {}
                Some(m) => {
                    missing.insert(format!("use {m}"));
                }
                None => {
                    missing.insert("use".into());
                }
            },
            "font" => {
                for (k, v) in &cmd.options {
                    if k.starts_with(char::is_uppercase) {
                        let mut features = OtFeatures::default();
                        if features.load_option(k, v, false).is_err() {
                            missing.insert(format!("font[{k}={v}]"));
                        }
                    } else if !FONT_OPTIONS.contains(&k.as_str()) {
                        missing.insert(format!("font[{k}]"));
                    } else if k == "family" && !corpus.fonts.has_family(v) {
                        missing.insert(format!("font family {v}"));
                    }
                }
            }
            "set" => match cmd.option("parameter") {
                Some(p)
                    if (cmd.option("reset").is_some() || cmd.option("makedefault").is_some())
                        && !p.starts_with("font.") =>
                {
                    missing.insert(format!("set {p} default"));
                }
                Some(p) if SETTINGS.contains(&p) => {}
                Some(p) => {
                    missing.insert(format!("set {p}"));
                }
                None if cmd.content.is_some() => {}
                None => {
                    missing.insert("set".into());
                }
            },
            "define" => match cmd.option("command") {
                Some("open-spread") if !is_blank(cmd.content.as_deref().unwrap_or(&[])) => {
                    missing.insert("\\define open-spread".into());
                }
                Some(name) => {
                    defined.insert(name.to_string());
                }
                None => {
                    missing.insert("define".into());
                }
            },
            "lua" | "script" if port.is_some() => {}
            "math" | "mathml" => {
                if let Some(mode) = cmd.option("mode").filter(|m| !matches!(*m, "text" | "display")) {
                    missing.insert(format!("math[mode={mode}]"));
                }
                continue;
            }
            "verse-number" if !port.is_some_and(|p| p.command("bible:verse-number").is_some()) => {
                missing.insert("\\bible:verse-number".into());
            }
            name if port.is_some_and(|p| p.command(name).is_some()) => {}
            name if SIMPLE_COMMANDS.contains(&name) || defined.contains(name) => {}
            name if cmd.raw.is_some() => {
                missing.insert(format!("\\begin{{{name}}}"));
            }
            name => {
                missing.insert(format!("\\{name}"));
            }
        }
        if let Some(inner) = &cmd.content {
            check(inner, corpus, port, defined, missing);
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

/// SILE settings that the driver tracks itself. Lengths SILE keeps relative
/// (`1bs`, `1.2em`) stay as source text and are evaluated against the
/// current font whenever they are handed to the builder, as SILE does.
/// The font itself is the builder's.
#[derive(Debug, Clone)]
struct Settings {
    language: String,
    parindent: String,
    parskip: String,
    baselineskip: String,
    lineskip: String,
    letterspace: Option<String>,
    tracking: Option<f64>,
    obey_spaces: bool,
    fixed_nbsp: bool,
    /// Every newline ends a paragraph (`typesetter.parseppattern` "\n").
    obey_lines: bool,
    ethiopic_centered: bool,
    skips: LineSkips,
    space: SpaceSettings,
    line_spacing: Option<LineSpacingSettings>,
    dropcap_bs_ratio: Option<f64>,
}

/// SILE's `linespacing.*` settings, lengths kept relative to the font.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LineSpacingSettings {
    method: &'static str,
    fixed: Length,
    fit_glyph: Length,
    fit_font: Length,
    css: Length,
    minimum_first_line: Length,
}

impl Default for LineSpacingSettings {
    fn default() -> Self {
        let em = |n| Length::new(Measurement::new(n, Unit::Em), Measurement::pt(0.0), Measurement::pt(0.0));
        Self {
            method: "tex",
            fixed: em(1.2),
            fit_glyph: Length::zero(),
            fit_font: Length::zero(),
            css: em(1.2),
            minimum_first_line: Length::zero(),
        }
    }
}

impl LineSpacingSettings {
    fn spacing(&self) -> LineSpacing {
        let method = match self.method {
            "fixed" => LineSpacingMethod::Fixed(self.fixed),
            "fit-glyph" => LineSpacingMethod::FitGlyph(self.fit_glyph),
            "fit-font" => LineSpacingMethod::FitFont(self.fit_font),
            "css" => LineSpacingMethod::Css(self.css),
            _ => LineSpacingMethod::Tex,
        };
        LineSpacing { method, minimum_first_line: self.minimum_first_line }
    }
}

/// Settings as last handed to the builder.
#[derive(Debug, Clone, PartialEq)]
struct Synced {
    baseline_skip: BaselineSkip,
    indent: f64,
    parskip: Length,
    skips: LineSkips,
    language: String,
    letterspace: Option<Length>,
    tracking: Option<f64>,
    obey_spaces: bool,
    fixed_nbsp: bool,
    space: SpaceSettings,
    line_spacing: Option<LineSpacing>,
    ethiopic_centered: bool,
}

/// A driver error passed through a class's typesetting callbacks.
struct Failed(String);

impl From<BuilderError> for Failed {
    fn from(e: BuilderError) -> Self {
        Self(e.to_string())
    }
}

pub(crate) struct Driver<'a> {
    corpus: &'a Corpus<'a>,
    pub(crate) doc: DocumentBuilder,
    /// Rust standing in for the test's Lua, and how many of its Lua chunks
    /// have run.
    port: Option<Port>,
    lua_chunks: usize,
    /// Frames collected by an open `\pagetemplate`.
    page_frames: Option<Vec<FrameSpec>>,
    masters: BTreeMap<String, PageTemplate>,
    /// What `\set[reset=true]` goes back to, for the settings that have one.
    defaults: BTreeMap<String, String>,
    counter_display: BTreeMap<String, String>,
    /// Font fallbacks in force (SILE switches shaper around these).
    fallbacks: usize,
    hanmen: Option<Hanmen>,
    paper: PaperSize,
    settings: Settings,
    synced: Option<Synced>,
    /// Settings scopes entered; settings at depth 0 are the document's own.
    depth: usize,
    defines: BTreeMap<String, Vec<Content>>,
    /// Content of the `\define`d commands being expanded, for `\process`.
    macro_content: Vec<Vec<Content>>,
    /// The settings at depth 0, for material set with the document's own
    /// settings (SILE's `toplevelState`).
    toplevel: Option<Settings>,
    /// SILE release targeted by `packages.retrograde` (latest if unset).
    target: (u32, u32, u32),
    /// The grid package's spacing, once it is loaded.
    grid_spacing: Option<f64>,
    bibliography: Bibliography,
    tex: TexMath,
    /// The letter class's date, sender, recipient and salutation, as given.
    letter: Option<[Option<Vec<Content>>; 4]>,
    /// The diglot or triglot class's flows, each selected by a command of
    /// its name.
    parallel_flows: Vec<&'static str>,
    chord_offset: Option<String>,
}

impl Context for Driver<'_> {
    type Arranger = DocumentBuilder;

    fn arranger(&mut self) -> &mut DocumentBuilder {
        &mut self.doc
    }
}

impl<'a> Driver<'a> {
    fn new(corpus: &'a Corpus<'a>) -> Result<Self, String> {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.load_fonts_dir(corpus.font_dir);
        let spec = FontSpec { family: Some("Gentium Book".into()), direction: Direction::Frame, ..Default::default() };
        doc.set_font_spec(spec).map_err(|e| e.to_string())?;
        Ok(Self {
            corpus,
            doc,
            port: None,
            lua_chunks: 0,
            page_frames: None,
            masters: BTreeMap::new(),
            counter_display: BTreeMap::new(),
            fallbacks: 0,
            hanmen: None,
            defaults: [("font.family", "Gentium Book"), ("font.size", "10"), ("font.style", "normal"), ("font.weight", "400")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            paper: PaperSize::A4,
            settings: Settings {
                language: "en".into(),
                parindent: "1bs".into(),
                parskip: "0pt plus 1pt".into(),
                baselineskip: "1.2em plus 1pt".into(),
                lineskip: "1pt".into(),
                letterspace: None,
                tracking: None,
                obey_spaces: false,
                fixed_nbsp: false,
                obey_lines: false,
                ethiopic_centered: false,
                skips: LineSkips::default(),
                space: SpaceSettings::default(),
                line_spacing: None,
                dropcap_bs_ratio: None,
            },
            synced: None,
            depth: 0,
            defines: BTreeMap::new(),
            macro_content: Vec::new(),
            toplevel: None,
            target: (u32::MAX, 0, 0),
            grid_spacing: None,
            letter: None,
            parallel_flows: Vec::new(),
            chord_offset: None,
            bibliography: Bibliography::default(),
            tex: TexMath::new(),
        })
    }

    fn finish(mut self) -> Result<String, String> {
        self.par()?;
        self.doc.render_debug().map_err(|e| e.to_string())
    }

    /// An image file named as SILE's tests name them: from the test's
    /// directory, or SILE's own.
    fn image(&self, src: &str) -> Result<Arc<Image>, String> {
        Image::load(self.resolve(src)?, src).map(Arc::new).map_err(|e| e.to_string())
    }

    /// What SILE's `verbatim` environment sets, with `skips` at the sides.
    fn verbatim_settings(&mut self, (left, right): (Length, Length)) -> Result<(), String> {
        if self.defines.contains_key("verbatim:font") {
            self.command(&Command { name: "verbatim:font".into(), options: Vec::new(), content: None, raw: None })?;
        } else {
            let before = self.doc.font_spec().cloned();
            self.set_font_option("family", "Hack")?;
            self.adjust_font_size("ex-height", before)?;
        }
        self.settings.language = "und".into();
        self.settings.obey_lines = true;
        self.settings.obey_spaces = true;
        self.settings.skips.left = left;
        self.settings.skips.right = right;
        self.settings.parindent = "0pt".into();
        self.settings.parskip = "0pt".into();
        self.settings.space.skip = Some(self.length("1spc")?);
        self.settings.space.variable_spaces = false;
        Ok(())
    }

    /// SILE's `\autodoc:codeblock`: verbatim text between rules.
    fn codeblock(&mut self, code: &str) -> Result<(), String> {
        self.sync()?;
        self.doc.leave_hmode(false).map_err(|e| e.to_string())?;
        let parindent = self.dimen(&self.settings.parindent.clone())?;
        let side = |l: &Length| Length::pt(l.length.to_pt().unwrap_or(0.0) + parindent);
        let skips = (side(&self.settings.skips.left), side(&self.settings.skips.right));
        let line = |raise: &str| format!("\\raise[height={raise}]{{\\hrule[thickness=0.5pt, width=100%lw]}}");
        let before = sil::parse(&format!("{}\\novbreak{{}}", line("0.2ex"))).map_err(|e| e.0)?;
        let after = sil::parse(&format!("\\novbreak{{}}{}", line("1ex"))).map_err(|e| e.0)?;
        self.scoped(|d| {
            d.verbatim_settings(skips)?;
            d.process(&before)?;
            d.text(code.trim())?;
            d.process(&after)?;
            d.sync()?;
            d.doc.leave_hmode(false).map_err(|e| e.to_string())
        })
    }

    fn resolve(&self, src: &str) -> Result<std::path::PathBuf, String> {
        let root = self.corpus.font_dir.with_file_name("sile");
        [root.join("tests").join(src), root.join(src)].into_iter().find(|p| p.exists()).ok_or_else(|| format!("no file {src}"))
    }

    /// SILE's `svg` package: `data` drawn at the size `cmd` asks for.
    fn svg(&mut self, cmd: &Command, data: &str) -> Result<(), String> {
        let size = |d: &mut Self, key| cmd.option(key).map(|v| d.dimen(v)).transpose();
        let (width, height) = (size(self, "width")?, size(self, "height")?);
        let density = cmd.option("density").map_or(Ok(72.0), |v| v.parse::<f64>().map_err(|e| e.to_string()))?;
        let image = Arc::new(SvgImage::parse(data, density));
        self.sync()?;
        self.doc.add_svg(image, width, height, density, false).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn folio(&mut self) -> Option<&mut Folio> {
        if self.doc.class_mut::<Book>().is_some() {
            return self.doc.class_mut::<Book>().map(|b| &mut b.folio);
        }
        if self.doc.class_mut::<Bible>().is_some() {
            return self.doc.class_mut::<Bible>().map(|b| &mut b.book.folio);
        }
        self.doc.class_mut::<Plain>().map(|p| &mut p.folio)
    }

    fn set_folio_state(&mut self, state: FolioState) {
        if let Some(folio) = self.folio() {
            folio.state = state;
        }
    }

    /// SILE's `footnotes` package: insertions into the `footnotes` frame,
    /// taken from `content`. Its skips are relative to the font in use when
    /// it is first needed.
    pub(crate) fn footnote_class(&mut self) -> Result<(), String> {
        if self.doc.insertion_class_mut("footnote").is_some() {
            return Ok(());
        }
        *self.doc.counter_mut("footnote") = 1;
        let mut class = InsertionClass::new("footnotes", "content", 0.75 * self.paper.height);
        class.top_box = vec![Node::vglue(Length::pt(self.dimen("2ex")?))];
        class.inter_skip = self.dimen("1ex")?;
        self.doc.set_insertion_class("footnote", class);
        Ok(())
    }

    /// `\footnote`: a raised mark here, and the note, numbered and set with
    /// the document's own settings at 90% size, sent to the footnotes frame.
    fn footnote(&mut self, content: &[Content]) -> Result<(), String> {
        let err = |e: rile::builder::BuilderError| e.to_string();
        self.footnote_class()?;
        let number = self.doc.counter_mut("footnote").to_string();
        self.scoped(|d| {
            let raise = d.dimen("0.7ex")?;
            let size = d.dimen("1.5ex")?;
            d.update_font(|f| f.size = size)?;
            d.sync()?;
            d.doc.add_baseline_shift(raise);
            d.add_text(&number)?;
            d.doc.add_baseline_shift(-raise);
            Ok(())
        })?;
        let toplevel = self.toplevel.clone();
        let nodes = self.scoped(|d| {
            if let Some(toplevel) = toplevel {
                d.settings = toplevel;
            }
            d.doc.use_toplevel();
            d.synced = None;
            d.update_font(|f| f.size *= 0.9)?;
            d.sync()?;
            d.doc.push_typesetter(Some("footnotes")).map_err(err)?;
            d.doc.set_current_indent(Some(0.0));
            d.add_text(&format!("{number}."))?;
            let qquad = d.length("2em")?;
            d.doc.add_glue(qquad);
            d.process(content)?;
            d.doc.pop_typesetter().map_err(err)
        })?;
        self.doc.insert("footnote", nodes);
        *self.doc.counter_mut("footnote") += 1;
        Ok(())
    }

    fn book(&mut self) -> Result<&mut Book, String> {
        self.doc.class_mut::<Book>().ok_or_else(|| "not a book".to_string())
    }

    /// Hand the current settings to the builder before it uses them.
    pub(crate) fn sync(&mut self) -> Result<(), String> {
        let bls = self.settings.baselineskip.clone();
        let lineskip = self.settings.lineskip.clone();
        let parindent = self.settings.parindent.clone();
        let parskip = self.settings.parskip.clone();
        let letterspace = match self.settings.letterspace.clone() {
            Some(l) => Some(self.length(&l)?),
            None => None,
        };
        let now = Synced {
            baseline_skip: BaselineSkip { skip: self.em_length(&bls)?, lineskip: self.dimen(&lineskip)? },
            indent: self.dimen(&parindent)?,
            parskip: self.length(&parskip)?,
            skips: self.settings.skips,
            language: self.settings.language.clone(),
            letterspace,
            tracking: self.settings.tracking,
            obey_spaces: self.settings.obey_spaces,
            fixed_nbsp: self.settings.fixed_nbsp,
            space: self.settings.space,
            line_spacing: self.settings.line_spacing.map(|l| l.spacing()),
            ethiopic_centered: self.settings.ethiopic_centered,
        };
        // Only what changed, so that settings a class made around content
        // it hands back to us stay in force.
        let last = self.synced.take();
        macro_rules! push {
            ($field:ident, $apply:expr) => {
                if last.as_ref().is_none_or(|l| l.$field != now.$field) {
                    $apply;
                }
            };
        }
        let doc = &mut self.doc;
        push!(baseline_skip, doc.set_baseline_skip(Some(now.baseline_skip)));
        push!(indent, doc.set_paragraph_indent(now.indent));
        push!(parskip, doc.set_paragraph_skip(now.parskip));
        push!(skips, doc.set_line_skips(now.skips));
        push!(language, doc.set_language(now.language.clone()));
        push!(letterspace, doc.set_letter_space(now.letterspace));
        push!(tracking, doc.set_tracking(now.tracking));
        push!(obey_spaces, doc.set_obey_spaces(now.obey_spaces));
        push!(fixed_nbsp, doc.set_fixed_nbsp(now.fixed_nbsp));
        push!(space, doc.set_space_settings(now.space));
        push!(line_spacing, doc.set_line_spacing(now.line_spacing));
        push!(ethiopic_centered, doc.set_ethiopic_centered(now.ethiopic_centered));
        self.synced = Some(now);
        if self.depth == 0 {
            self.doc.mark_toplevel();
            self.toplevel = Some(self.settings.clone());
        }
        Ok(())
    }

    pub(crate) fn par(&mut self) -> Result<(), String> {
        self.sync()?;
        self.doc.new_paragraph().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) fn process(&mut self, content: &[Content]) -> Result<(), String> {
        for c in content {
            match c {
                Content::Text(t) => self.text(t)?,
                Content::Command(cmd) => self.command(cmd)?,
            }
        }
        Ok(())
    }

    pub(crate) fn text(&mut self, text: &str) -> Result<(), String> {
        if matches!(text, "\n" | "\r\n") {
            return Ok(());
        }
        if self.settings.obey_lines {
            let mut seen = true;
            let mut lines = text.split('\n').peekable();
            while let Some(line) = lines.next() {
                if !line.is_empty() {
                    self.add_text(line)?;
                    seen = true;
                }
                if lines.peek().is_some() {
                    if !seen {
                        self.sync()?;
                        self.doc.add_box(HBox::new(Length::zero(), Length::zero(), Length::zero()));
                    }
                    seen = false;
                    self.par()?;
                }
            }
            return Ok(());
        }
        let mut paragraphs = split_paragraphs(text).into_iter().peekable();
        while let Some(chunk) = paragraphs.next() {
            if !chunk.is_empty() {
                self.add_text(&chunk)?;
            }
            if paragraphs.peek().is_some() {
                self.par()?;
            }
        }
        Ok(())
    }

    /// SILE's `handleMath`: the formula inline, or displayed and numbered
    /// from the `equation` counter or `counter`, or as `number`.
    fn math(&mut self, cmd: &Command, node: &MathNode) -> Result<(), String> {
        let counter = cmd.option("counter").or(cmd.option("numbered").filter(|n| truthy(n)).map(|_| "equation"));
        let mode = match cmd.option("mode").unwrap_or("text") {
            "text" => MathMode::Text,
            "display" => {
                let number = match counter {
                    Some(id) => {
                        let counter = self.doc.counter_mut(id);
                        *counter += 1;
                        let value = *counter;
                        let display = self.counter_display.get(id).map_or("arabic", String::as_str);
                        Some(format_number(value, display).ok_or_else(|| format!("unknown display {display}"))?)
                    }
                    None => cmd.option("number").map(str::to_string),
                };
                MathMode::Display { number }
            }
            mode => return Err(format!("Unknown math mode {mode}")),
        };
        self.sync()?;
        self.doc.add_math(node, mode).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) fn add_text(&mut self, text: &str) -> Result<(), String> {
        self.sync()?;
        self.doc.add_text(text);
        Ok(())
    }

    pub(crate) fn update_font(&mut self, f: impl FnOnce(&mut FontSpec)) -> Result<(), String> {
        self.doc.update_font(f).map(|_| ()).map_err(|e| e.to_string())
    }

    /// SILE's `\lorem`: dummy Latin text.
    pub(crate) fn lorem(&mut self, words: usize) -> Result<(), String> {
        let text = lorem(self.corpus.lorem, words);
        self.scoped(|d| {
            d.settings.language = "la".to_string();
            d.text(&text)
        })
    }

    /// `\lorem[counter=true]`: each space replaced by a running count.
    fn numbered_lorem(&mut self, words: usize) -> Result<(), String> {
        let text = lorem(self.corpus.lorem, words);
        let mut count = 0;
        let mut numbered = String::new();
        for (i, part) in text.split(char::is_whitespace).enumerate() {
            if i > 0 && part.is_empty() {
                continue;
            }
            if i > 0 {
                count += 1;
                numbered.push_str(&format!(" {count} "));
            }
            numbered.push_str(part);
        }
        self.scoped(|d| {
            d.settings.language = "la".to_string();
            d.text(&numbered)
        })
    }

    /// Indent paragraphs by `value` from here on (SILE's
    /// `document.parindent`).
    pub(crate) fn set_parindent(&mut self, value: &str) {
        self.settings.parindent = value.to_string();
    }

    pub(crate) fn scoped<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        if self.depth == 0 {
            self.sync()?;
        }
        let saved = self.settings.clone();
        let font = self.doc.font_spec().cloned();
        let math = self.doc.math_settings().clone();
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        self.settings = saved;
        *self.doc.math_settings_mut() = math;
        if let Some(font) = font {
            self.doc.set_font_spec(font).map_err(|e| e.to_string())?;
        }
        r
    }

    /// `\center`, `\raggedright` and friends: set the margins for the
    /// content and end it as a paragraph.
    fn aligned(&mut self, align: TextAlign, content: &[Content]) -> Result<(), String> {
        self.scoped(|d| {
            let legacy = d.target < (0, 15, 0) && align != TextAlign::Justify;
            if legacy {
                let fill = || {
                    Length::new(
                        Measurement::pt(0.0),
                        Measurement::pt(INFINITY),
                        Measurement::pt(0.0),
                    )
                };
                let s = &mut d.settings.skips;
                match align {
                    TextAlign::Left => s.right = fill(),
                    TextAlign::Right => s.left = fill(),
                    _ => (s.left, s.right) = (fill(), fill()),
                }
                s.par_fill = Length::zero();
                d.settings.parindent = "0pt".into();
            } else {
                d.settings.skips = d.settings.skips.aligned(align);
                if align == TextAlign::Center {
                    d.settings.parindent = "0pt".into();
                    d.doc.set_current_indent(Some(0.0));
                }
            }
            d.process(content)?;
            d.par()
        })
    }

    /// SILE's `\\ch`: `name` set through `\\chordmode:chordfont` above `lyric`.
    fn chord(&mut self, name: &str, lyric: &[Content]) -> Result<(), String> {
        let offset = match self.chord_offset.clone() {
            Some(offset) => self.dimen(&offset)?,
            None => self.dimen("2ex")?,
        };
        self.sync()?;
        let name = vec![Content::Text(name.to_string())];
        let chord = |d: &mut Self| {
            let font = Command { name: "chordmode:chordfont".into(), options: Vec::new(), content: Some(name), raw: None };
            if d.defines.contains_key(&font.name) { d.command(&font) } else { d.process(font.content.as_deref().unwrap_or_default()) }.map_err(Failed)
        };
        chords::add_chord(self, offset, chord, |d| d.process(lyric).map_err(Failed)).map_err(|Failed(e)| e)
    }

    fn command(&mut self, cmd: &Command) -> Result<(), String> {
        if let Some(port) = &self.port {
            let inc = cmd.name == "use" && cmd.option("module").is_some_and(|m| m.starts_with("inc."));
            if inc || matches!(cmd.name.as_str(), "lua" | "script") {
                let chunk = port.chunks.get(self.lua_chunks).ok_or("no Rust port for this Lua")?;
                self.lua_chunks += 1;
                return chunk(self);
            }
            if let Some(command) = port.command(&cmd.name) {
                return command(self, cmd);
            }
        }
        let content = cmd.content.as_deref().unwrap_or(&[]);
        let opt = |k: &str| {
            cmd.option(k)
                .ok_or_else(|| format!("\\{} needs {k}", cmd.name))
        };
        let err = |e: rile::builder::BuilderError| e.to_string();
        match cmd.name.as_str() {
            "document" => {
                if let Some(p) = cmd.option("papersize") {
                    self.paper = paper_size(p).ok_or("bad papersize")?;
                }
                if cmd.option("landscape").is_some_and(truthy) {
                    self.paper = self.paper.landscape();
                }
                self.doc.set_page_size(self.paper);
                if let Some(sheet) = cmd.option("sheetsize") {
                    self.doc.set_sheet_size(Some(paper_size(sheet).ok_or("bad sheetsize")?));
                }
                match cmd.option("class") {
                    Some("book") => self.doc.set_class(Book::new()),
                    Some("bible") => self.doc.set_class(Bible::new()),
                    Some("letter") => {
                        self.letter = Some(Default::default());
                        self.doc.set_class(Letter::new())
                    }
                    Some("jbook" | "tbook") => self.doc.set_class(Book::japanese(cmd.option("layout") == Some("tate"))),
                    Some("jplain" | "tplain") => self.doc.set_class(Plain::japanese(cmd.option("layout") == Some("tate"))),
                    _ => self.doc.set_class(Plain::new()),
                };
                match cmd.option("class") {
                    Some("diglot") => {
                        diglot(&mut self.doc).map_err(err)?;
                        self.parallel_flows = DIGLOT_FLOWS.iter().map(|(name, _)| *name).collect();
                    }
                    Some("pecha") => {
                        pecha(&mut self.doc).map_err(err)?;
                        self.set_default("document.language", "bo")?;
                        self.set("document.parindent", "0pt")?;
                        self.settings.skips = self.settings.skips.aligned(TextAlign::Right);
                    }
                    Some("triglot") => {
                        triglot(&mut self.doc).map_err(err)?;
                        self.set("linebreak.tolerance", "5000")?;
                        self.set("document.parindent", "0pt")?;
                        self.parallel_flows = TRIGLOT_FLOWS.iter().map(|(name, _)| *name).collect();
                    }
                    _ => {}
                }
                if let Some(class) = cmd.option("class").filter(|c| matches!(*c, "jplain" | "jbook" | "tplain" | "tbook")) {
                    let grid = if class.ends_with("book") { Hanmen::BOOK } else { Hanmen::PLAIN };
                    self.hanmen = Some(grid);
                    self.doc.set_bidi(false);
                    self.set("document.baselineskip", &format!("{}pt", grid.baseline_skip()))?;
                    self.set("document.parskip", "0pt")?;
                    self.set("document.parindent", "10pt")?;
                    if class.starts_with('j') {
                        self.set_default("document.language", "ja")?;
                        self.set_default("font.family", "Noto Sans CJK JP")?;
                    }
                }
                if let Some(dir) = cmd.option("direction").and_then(direction) {
                    self.doc.set_direction(dir);
                    self.update_font(|f| f.direction = dir)?;
                }
                self.process(content)?;
            }
            "use" => match cmd.option("module") {
                Some("packages.retrograde") => self.retrograde(cmd.option("target").unwrap_or(""))?,
                Some("packages.ruby") => {
                    let fallback = FontFallback { family: Some("Noto Sans CJK JP".into()), ..Default::default() };
                    self.add_fallback(fallback)?;
                }
                Some("packages.linespacing") => {
                    self.settings.line_spacing.get_or_insert_with(Default::default);
                }
                Some("packages.grid") => {
                    self.grid_spacing = Some(self.dimen(cmd.option("spacing").unwrap_or("1bs"))?);
                }
                Some("packages.complex-spaces") => {
                    self.doc.set_complex_spaces(true);
                }
                Some("packages.boustrophedon") => {
                    self.doc.break_between_letters("grc");
                }
                Some("packages.pagebuilder-bestfit") => {
                    self.doc.set_best_fit_pages(true);
                }
                _ => {}
            },
            "par" => self.par()?,
            "bidi-off" | "bidi-on" => {
                self.doc.set_bidi(cmd.name == "bidi-on");
            }
            "thisframeRTL" | "thisframeLTR" => {
                let dir = if cmd.name == "thisframeRTL" { Direction::RTL } else { Direction::LTR };
                self.sync()?;
                self.doc.set_frame_direction(dir.into());
                self.update_font(|f| f.direction = dir)?;
                self.doc.leave_hmode(false).map_err(err)?;
            }
            " " => self.add_text(" ")?,
            "comment" => {}
            "noop" => self.process(content)?,
            "nofolios" => self.set_folio_state(FolioState::Off),
            "folios" => self.set_folio_state(FolioState::On),
            "nofoliothispage" => self.set_folio_state(FolioState::OffThisPage),
            "define" => {
                let name = opt("command")?;
                if name == "open-spread"
                    && let Some(book) = self.doc.class_mut::<Book>()
                {
                    book.chapters_open_spread = false;
                }
                self.defines.insert(name.to_string(), content.to_vec());
            }
            "process" => {
                if let Some(inner) = self.macro_content.pop() {
                    let r = self.process(&inner);
                    self.macro_content.push(inner);
                    r?;
                }
            }
            "open-spread" if self.defines.contains_key("open-spread") => {}
            "open-double-page" | "open-spread" => {
                let flag = |k: &str, default: bool| cmd.option(k).map_or(default, truthy);
                let (odd, double, blank) = if cmd.name == "open-spread" {
                    (flag("odd", true), flag("double", true), flag("blank", true))
                } else {
                    (true, false, false)
                };
                self.sync()?;
                Book::open_spread(&mut self.doc, odd, double, blank).map_err(err)?;
            }
            "left-running-head" | "right-running-head" => {
                self.book()?;
                self.doc.begin_capture();
                let r = self.scoped(|d| d.process(content));
                let material = self.doc.end_capture();
                r?;
                let book = self.book()?;
                if cmd.name == "left-running-head" {
                    book.left_head = Some(material);
                } else {
                    book.right_head = Some(material);
                }
            }
            "math" => {
                let src = match &cmd.raw {
                    Some(raw) => raw.clone(),
                    None => content.iter().filter_map(|c| if let Content::Text(t) = c { Some(t.as_str()) } else { None }).collect(),
                };
                let node = self.tex.parse(&src).map_err(|e| e.to_string())?;
                self.math(cmd, &node)?;
            }
            "mathml" => {
                let mut element = mathml::Element::new("mathml");
                element.children = math_content(content);
                let node = element.to_node()?;
                self.math(cmd, &node)?;
            }
            "discretionary" => {
                self.sync()?;
                self.doc.add_discretionary(cmd.option("prebreak"), cmd.option("postbreak"), cmd.option("replacement"));
            }
            "increment-counter" | "set-counter" | "show-counter" => {
                let id = opt("id")?.to_string();
                if let Some(display) = cmd.option("display") {
                    self.counter_display.insert(id.clone(), display.to_string());
                }
                let value = cmd.option("value").map(|v| v.parse::<i64>().map_err(|_| "bad value")).transpose()?;
                let counter = self.doc.counter_mut(&id);
                match (cmd.name.as_str(), value) {
                    ("increment-counter", _) => *counter += 1,
                    ("set-counter", Some(value)) => *counter = value,
                    ("show-counter", _) => {
                        let display = self.counter_display.get(&id).map_or("arabic", String::as_str);
                        let text = format_number(*counter, display).ok_or_else(|| format!("unknown display {display}"))?;
                        self.add_text(&text)?;
                    }
                    _ => {}
                }
            }
            "increment-multilevel-counter" | "set-multilevel-counter" | "show-multilevel-counter" => {
                let level = cmd.option("level").map(|l| l.parse::<usize>()).transpose().map_err(|_| "bad level")?;
                let counter = self.doc.multilevel_counter_mut(opt("id")?);
                match cmd.name.as_str() {
                    "increment-multilevel-counter" => {
                        counter.increment(level, cmd.option("reset").is_none_or(truthy))
                    }
                    "set-multilevel-counter" => {
                        let value = opt("value")?.parse().map_err(|_| "bad value")?;
                        counter.set(level.ok_or("set-multilevel-counter needs level")?, value);
                    }
                    _ => {
                        let text = counter.format(level);
                        self.add_text(&text)?;
                    }
                }
            }
            "noindent" => {
                self.doc.set_current_indent(Some(0.0));
                self.process(content)?;
            }
            "neverindent" => {
                self.settings.parindent = "0pt".into();
                self.doc.set_current_indent(Some(0.0));
                self.process(content)?;
            }
            "indent" => {
                self.doc.set_current_indent(None);
                self.process(content)?;
            }
            "smallskip" | "medskip" | "bigskip" => {
                let amount = match cmd.name.as_str() {
                    "smallskip" => "3pt plus 1pt minus 1pt",
                    "medskip" => "6pt plus 2pt minus 2pt",
                    _ => "12pt plus 4pt minus 4pt",
                };
                let amount = self.length(amount)?;
                self.sync()?;
                self.doc.add_explicit_vskip(amount).map_err(err)?;
            }
            "skip" => {
                let height = self.length(opt("height")?)?;
                self.sync()?;
                if cmd.option("discardable").is_some_and(truthy) {
                    self.doc.add_vskip(height).map_err(err)?;
                } else {
                    self.doc.add_explicit_vskip(height).map_err(err)?;
                }
            }
            "vfill" => {
                self.sync()?;
                self.doc.add_vfill().map_err(err)?;
            }
            "pagebreak" | "framebreak" => {
                self.sync()?;
                let penalty = if cmd.name == "pagebreak" {
                    -20_000
                } else {
                    -10_000
                };
                self.doc.add_vertical_penalty(penalty).map_err(err)?;
            }
            "eject" | "supereject" | "filbreak" => {
                self.sync()?;
                self.doc.add_vfill().map_err(err)?;
                let penalty = match cmd.name.as_str() {
                    "eject" => -10_000,
                    "supereject" => -20_000,
                    _ => -200,
                };
                self.doc.add_penalty(penalty);
            }
            "novbreak" => {
                self.sync()?;
                self.doc.add_vertical_penalty(10_000).map_err(err)?;
            }
            "break" | "nobreak" | "allowbreak" | "goodbreak" | "cr" | "penalty" => {
                let penalty = match cmd.name.as_str() {
                    "break" | "cr" => -10_000,
                    "nobreak" => 10_000,
                    "allowbreak" => 0,
                    "goodbreak" => -500,
                    _ => opt("penalty")?
                        .parse()
                        .map_err(|_| "bad penalty".to_string())?,
                };
                self.sync()?;
                if cmd.name == "cr" {
                    self.doc.add_hfill();
                }
                if cmd.option("vertical").is_some_and(truthy) {
                    self.doc.add_vertical_penalty(penalty).map_err(err)?;
                } else {
                    self.doc.add_penalty(penalty);
                }
            }
            "glue" | "quad" | "qquad" | "thinspace" => {
                let width = match cmd.name.as_str() {
                    "glue" => opt("width")?,
                    "quad" => "1em",
                    "qquad" => "2em",
                    _ => "0.16667em",
                };
                let width = self.length(width)?;
                self.sync()?;
                self.doc.add_glue(width);
            }
            "kern" => {
                let width = self.length(opt("width")?)?;
                self.sync()?;
                self.doc.add_kern(width);
            }
            "hfill" => {
                self.sync()?;
                self.doc.add_hfill();
            }
            "hbox" => {
                self.sync()?;
                self.doc.start_hbox();
                self.process(content)?;
                self.doc.end_hbox();
            }
            "pagetemplate" => {
                self.sync()?;
                let outer = self.page_frames.replace(Vec::new());
                let result = self.scoped(|d| d.process(content));
                let frames = std::mem::replace(&mut self.page_frames, outer).unwrap_or_default();
                result?;
                self.doc.declare_page_frames(&frames).map_err(err)?;
                if let Some(first) = cmd.option("first-content-frame") {
                    self.doc.set_content_frame(first).map_err(err)?;
                }
            }
            "define-master-template" => {
                let outer = self.page_frames.replace(Vec::new());
                let result = self.scoped(|d| d.process(content));
                let frames = std::mem::replace(&mut self.page_frames, outer).unwrap_or_default();
                result?;
                let first_content_frame = opt("first-content-frame")?.to_string();
                self.masters.insert(opt("id")?.to_string(), PageTemplate { frames, first_content_frame });
            }
            "switch-master" | "switch-master-one-page" => {
                let id = opt("id")?;
                let master = self.masters.get(id).cloned().ok_or_else(|| format!("no master {id}"))?;
                self.sync()?;
                if cmd.name == "switch-master" {
                    self.doc.set_master(master).map_err(err)?;
                } else {
                    self.doc.set_page_master(&master).map_err(err)?;
                }
            }
            "makecolumns" => {
                let columns = cmd.option("columns").and_then(|c| c.parse().ok()).unwrap_or(2);
                let gutter = self.dimen(cmd.option("gutter").unwrap_or("3%pw"))?;
                self.sync()?;
                self.doc.make_columns(columns, gutter).map_err(err)?;
            }
            "ch" => self.chord(opt("name")?, content)?,
            "repertoire" | "pangrams" => {
                self.sync()?;
                match cmd.name.as_str() {
                    "repertoire" => self.doc.add_repertoire().map(|_| ()),
                    _ => self.doc.add_pangrams().map(|_| ()),
                }
                .map_err(err)?;
            }
            "set-to-width" => {
                let width = self.dimen(opt("width")?)?;
                self.sync()?;
                self.doc.set_to_width(width, &sil::plain_text(content)).map_err(err)?;
            }
            "boustrophedon" => {
                self.sync()?;
                self.doc.leave_hmode(false).map_err(err)?;
                self.doc.set_boustrophedon(true);
                let result = self.process(content).and_then(|_| self.doc.leave_hmode(false).map_err(err));
                self.doc.set_boustrophedon(false);
                result?;
            }
            "chordmode" => {
                for c in content {
                    let Content::Text(text) = c else {
                        self.process(std::slice::from_ref(c))?;
                        continue;
                    };
                    for (chord, text) in chords::parse(text) {
                        let text = [Content::Text(text)];
                        match chord {
                            Some(chord) => self.chord(&chord, &text)?,
                            None => self.process(&text)?,
                        }
                    }
                }
            }
            "ifattop" | "ifnotattop" => {
                self.sync()?;
                if self.doc.at_top_of_frame().map_err(err)? == (cmd.name == "ifattop") {
                    self.process(content)?;
                }
            }
            "balancecolumns" => {
                self.sync()?;
                self.doc.balance_columns().map_err(err)?;
            }
            "showframe" => {
                let id = cmd.option("id").filter(|id| *id != "all");
                self.doc.show_frame(id).map_err(err)?;
            }
            "frame" => {
                let mut spec = FrameSpec::new(opt("id")?);
                for (k, v) in &cmd.options {
                    let v = Some(v.clone());
                    match k.as_str() {
                        "left" => spec.left = v,
                        "right" => spec.right = v,
                        "top" => spec.top = v,
                        "bottom" => spec.bottom = v,
                        "width" => spec.width = v,
                        "height" => spec.height = v,
                        "next" => spec.next = v,
                        "balanced" => spec.balanced = matches!(v.as_deref(), Some("1" | "true" | "yes")),
                        "direction" => spec.direction = v.as_deref().and_then(FrameDirection::parse),
                        _ => {}
                    }
                }
                match &mut self.page_frames {
                    Some(frames) => frames.push(spec),
                    None => {
                        self.sync()?;
                        self.doc.declare_page_frames(&[spec]).map_err(err)?;
                    }
                }
            }
            "hrule" => {
                let mut dim = |name| cmd.option(name).map_or(Ok(0.0), |v| self.dimen(v));
                let (width, height, depth) = (dim("width")?, dim("height")?, dim("depth")?);
                self.sync()?;
                self.doc.add_hrule(width, height, depth);
            }
            "hrulefill" => {
                let thickness = cmd.option("thickness").map(|t| self.dimen(t)).transpose()?;
                let stroke = match cmd.option("position") {
                    Some("underline") => {
                        self.sync()?;
                        let (position, default) = self.doc.underline_metrics();
                        Stroke { raise: position, thickness: thickness.unwrap_or(default) }
                    }
                    Some("strikethrough") => {
                        self.sync()?;
                        let (position, default) = self.doc.strikeout_metrics();
                        let thickness = thickness.unwrap_or(default);
                        Stroke { raise: position + thickness / 2.0, thickness }
                    }
                    Some(other) => return Err(format!("unknown hrulefill position {other}")),
                    None => Stroke {
                        raise: self.dimen(cmd.option("raise").unwrap_or("0"))?,
                        thickness: thickness.unwrap_or(0.2),
                    },
                };
                self.sync()?;
                self.doc.add_hrulefill(stroke);
            }
            "fullrule" => {
                let thickness = self.dimen(cmd.option("thickness").unwrap_or("0.2pt"))?;
                let raise = self.dimen(cmd.option("raise").unwrap_or("0.5em"))?;
                self.sync()?;
                self.doc.leave_hmode(false).map_err(err)?;
                self.doc.set_current_indent(Some(0.0));
                self.doc.add_hrulefill(Stroke { raise, thickness });
                self.doc.leave_hmode(false).map_err(err)?;
            }
            "underline" | "strikethrough" => {
                self.sync()?;
                if cmd.name == "underline" {
                    self.doc.start_underline();
                } else {
                    self.doc.start_strikethrough();
                }
                self.process(content)?;
                self.doc.end_hbox();
            }
            "leaders" => {
                let width = cmd.option("width").map(|w| self.length(w)).transpose()?;
                self.sync()?;
                self.doc.start_leaders(width);
                self.process(content)?;
                self.doc.end_hbox();
            }
            "dotfill" => {
                self.sync()?;
                self.doc.add_dotfill();
            }
            "blockquote" => {
                let smallskip = self.length("3pt plus 1pt minus 1pt")?;
                self.par()?;
                self.doc.add_explicit_vskip(smallskip).map_err(err)?;
                self.scoped(|d| {
                    let indent = Measurement::pt(d.dimen("2em")?);
                    let s = &mut d.settings.skips;
                    s.left.length += indent;
                    s.right.length += indent;
                    d.update_font(|f| f.size *= 0.95)?;
                    d.process(content)?;
                    d.par()
                })?;
                self.doc.add_explicit_vskip(smallskip).map_err(err)?;
            }
            "center" => self.aligned(TextAlign::Center, content)?,
            "raggedright" => self.aligned(TextAlign::Left, content)?,
            "raggedleft" => self.aligned(TextAlign::Right, content)?,
            "justified" => self.aligned(TextAlign::Justify, content)?,
            "ragged" => {
                let left = cmd.option("left").is_some_and(truthy);
                let right = cmd.option("right").is_some_and(truthy);
                let align = match (left, right) {
                    (true, true) => TextAlign::Center,
                    (false, true) => TextAlign::Right,
                    (true, false) => TextAlign::Left,
                    _ => TextAlign::Justify,
                };
                self.aligned(align, content)?;
            }
            "em" => self.scoped(|d| {
                d.update_font(|f| {
                    f.style = if f.style == FontStyle::Italic { FontStyle::Normal } else { FontStyle::Italic }
                })?;
                d.process(content)
            })?,
            "itemize" | "enumerate" => {
                let kind = if cmd.name == "itemize" { ListKind::Itemize } else { ListKind::Enumerate };
                let options = ListOptions {
                    start: cmd.option("start").map(|v| v.parse().map_err(|_| format!("bad start {v}"))).transpose()?,
                    display: cmd.option("display").map(str::to_string),
                    before: cmd.option("before").map(str::to_string),
                    after: cmd.option("after").map(str::to_string),
                    bullet: cmd.option("bullet").map(str::to_string),
                };
                self.sync()?;
                self.doc.begin_list(kind, &options).map_err(err)?;
                self.scoped(|d| {
                    d.settings.skips = d.doc.line_skips();
                    d.settings.parindent = "0pt".into();
                    for item in content {
                        let Content::Command(c) = item else { continue };
                        if c.name == "item" {
                            d.doc.begin_item(c.option("bullet")).map_err(err)?;
                            d.process(c.content.as_deref().unwrap_or(&[]))?;
                            d.sync()?;
                            d.doc.end_item().map_err(err)?;
                        } else {
                            d.command(c)?;
                        }
                    }
                    Ok(())
                })?;
                self.sync()?;
                self.doc.end_list().map_err(err)?;
            }
            "verbatim" => {
                self.sync()?;
                self.doc.push_vglue(Length::pt(6.0));
                self.doc.leave_hmode(false).map_err(err)?;
                self.scoped(|d| {
                    let fixed = |l: Length| Length::new(l.length, Measurement::pt(0.0), Measurement::pt(0.0));
                    let skips = (fixed(d.settings.skips.left), fixed(d.settings.skips.right));
                    d.verbatim_settings(skips)?;
                    d.process(content)?;
                    d.sync()?;
                    d.doc.leave_hmode(false).map_err(err)
                })?;
            }
            "ruby" => {
                let reading = opt("reading")?.to_string();
                self.sync()?;
                rile::ruby::add_ruby(self, &reading, |d| d.process(content).map_err(Failed)).map_err(|Failed(e)| e)?;
            }
            "save-book-title" | "save-chapter-number" => {
                let text = sil::plain_text(content);
                let bible = self.doc.class_mut::<Bible>().ok_or("needs the bible class")?;
                if cmd.name == "save-book-title" {
                    bible.save_book_title(text);
                } else {
                    bible.save_chapter_number(text);
                }
            }
            "verse-number" => {
                self.sync()?;
                if let Some(label) = self.port.as_ref().and_then(|p| p.command("bible:verse-number")) {
                    label(self, cmd)?;
                }
                Bible::verse_number(&mut self.doc, &sil::plain_text(content)).map_err(err)?;
            }
            "breakframevertical" => {
                let offset = cmd.option("offset").map(|o| self.dimen(o)).transpose()?;
                self.sync()?;
                self.doc.break_frame_vertical(offset).map_err(err)?;
            }
            name if self.parallel_flows.contains(&name) => {
                self.sync()?;
                self.doc.select_parallel(name).map_err(err)?;
            }
            "sync" => {
                self.sync()?;
                self.doc.sync_parallel().map_err(err)?;
            }
            "pdf:metadata" => {
                match self.doc.set_pdf_metadata(opt("key")?, opt("value")?) {
                    Ok(_) | Err(rile::builder::BuilderError::InvalidMetadata(_)) => {}
                    Err(e) => return Err(err(e)),
                }
            }
            "code" => self.scoped(|d| {
                let before = d.doc.font_spec().cloned();
                d.set_font_option("family", cmd.option("family").unwrap_or("Hack"))?;
                match cmd.option("size") {
                    Some(size) => d.set_font_option("size", size)?,
                    None => d.adjust_font_size(cmd.option("adjust").unwrap_or("ex-height"), before)?,
                }
                d.process(content)
            })?,
            "url" => {
                let url = sil::plain_text(content);
                let pieces = url_pieces(&url, UrlPenalties::default())
                    .into_iter()
                    .map(|piece| match piece {
                        UrlPiece::Text(text) => Content::Text(text.to_string()),
                        UrlPiece::Penalty(p) => Content::Command(Command {
                            name: "penalty".into(),
                            options: vec![("penalty".into(), p.to_string())],
                            content: None,
                            raw: None,
                        }),
                    })
                    .collect();
                let style = if self.defines.contains_key("urlstyle") { "urlstyle" } else { "code" };
                self.scoped(|d| {
                    d.settings.language = cmd.option("language").unwrap_or("und").to_string();
                    d.command(&Command { name: style.into(), options: Vec::new(), content: Some(pieces), raw: None })
                })?;
            }
            "href" => {
                let src = match cmd.option("src") {
                    Some(src) => src.to_string(),
                    None => sil::plain_text(content),
                };
                self.sync()?;
                self.doc.start_link(LinkDest::Uri(src));
                if cmd.option("src").is_some() {
                    self.process(content)?;
                } else {
                    let url = Command { name: "url".into(), options: cmd.options.clone(), content: Some(content.to_vec()), raw: None };
                    self.command(&url)?;
                }
                self.sync()?;
                self.doc.end_hbox();
            }
            "img" => {
                let image = self.image(opt("src")?)?;
                let size = |d: &mut Self, key| cmd.option(key).map(|v| d.dimen(v)).transpose().map(|v| v.filter(|v| *v > 0.0));
                let (width, height) = (size(self, "width")?, size(self, "height")?);
                self.sync()?;
                self.doc.add_image(image, width, height);
            }
            "cropmarks:setup" => {
                let mut cropmarks = Cropmarks::default();
                if let Some(header) = self.defines.get("cropmarks:header") {
                    if header.iter().any(|c| matches!(c, sil::Content::Command(_))) {
                        return Err("cropmarks:header with commands".into());
                    }
                    let text = sil::plain_text(header);
                    cropmarks.header = Arc::new(move |doc, _| {
                        doc.add_text(text.clone());
                        Ok(())
                    });
                } else {
                    return Err("cropmarks:header with the file name and date".into());
                }
                self.sync()?;
                cropmarks.install(&mut self.doc);
            }
            "svg" => {
                let data = std::fs::read_to_string(self.resolve(opt("src")?)?).map_err(|e| e.to_string())?;
                self.svg(cmd, &data)?;
            }
            "raw" => {
                let raw = cmd.raw.clone().or_else(|| Some(sil::plain_text(content))).unwrap_or_default();
                match opt("type")? {
                    "svg" => self.svg(cmd, &raw)?,
                    "autodoc:codeblock" => self.codeblock(&raw)?,
                    "verbatim" => self.command(&Command { name: "verbatim".into(), options: Vec::new(), content: Some(vec![sil::Content::Text(raw)]), raw: None })?,
                    "text" => self.scoped(|d| {
                        d.settings.obey_lines = true;
                        d.settings.obey_spaces = true;
                        d.text(&raw)
                    })?,
                    other => return Err(format!("no inline handler for '{other}'")),
                }
            }
            "background" => {
                let background = if cmd.option("disable").is_some_and(truthy) {
                    None
                } else {
                    let fill = match (cmd.option("src"), cmd.option("color")) {
                        (Some(src), _) => BackgroundFill::Image(self.image(src)?),
                        (None, Some(color)) => BackgroundFill::Color(Color::parse(color)?),
                        (None, None) => return Err("background needs a color or src".into()),
                    };
                    Some(Background { fill, all_pages: cmd.option("allpages").is_none_or(truthy) })
                };
                self.doc.set_background(background).map_err(err)?;
            }
            "add-font-feature" | "remove-font-feature" => {
                let mut features = OtFeatures::parse(&self.doc.font_spec().map(|f| f.features.clone()).unwrap_or_default());
                for (k, v) in &cmd.options {
                    features.load_option(k, v, cmd.name == "remove-font-feature")?;
                }
                self.set_font_option("features", &features.to_string())?;
            }
            "grid" | "grid:debug" => {
                if let Some(spacing) = cmd.option("spacing") {
                    let spacing = self.dimen(spacing)?;
                    if cmd.name == "grid" {
                        self.grid_spacing = Some(spacing);
                    }
                }
                let spacing = match (cmd.name.as_str(), cmd.option("spacing")) {
                    ("grid:debug", Some(s)) => self.dimen(s)?,
                    _ => self.grid_spacing.ok_or("grid package not loaded")?,
                };
                self.sync()?;
                if cmd.name == "grid" {
                    self.doc.start_grid(spacing).map_err(err)?;
                } else {
                    self.doc.show_grid(spacing).map_err(err)?;
                }
            }
            "no-grid" => {
                self.doc.end_grid();
            }
            "ftl" => {
                let lang = cmd.option("locale").map_or_else(|| self.settings.language.clone(), String::from);
                self.doc.add_messages(&lang, cmd.raw.as_deref().unwrap_or_default());
            }
            "fluent" => {
                let id = sil::plain_text(content);
                let args: Vec<(&str, &str)> = cmd.options.iter().filter(|(k, _)| k != "locale").map(|(k, v)| (k.as_str(), v.as_str())).collect();
                self.sync()?;
                let saved = cmd.option("locale").map(|locale| {
                    let saved = self.doc.language().to_string();
                    self.doc.set_language(locale);
                    saved
                });
                let message = self.doc.message(id.trim(), &args);
                if let Some(saved) = saved {
                    self.doc.set_language(saved);
                }
                let message = message.ok_or_else(|| format!("no message {id}"))?;
                let tree = crate::xml::parse(&format!("<fluent>{message}</fluent>")).map_err(|e| e.0)?;
                for c in &tree {
                    if let Content::Command(root) = c {
                        self.process(root.content.as_deref().unwrap_or_default())?;
                    }
                }
            }
            "dropcap" => {
                let mut dropcap = Dropcap { bs_ratio: self.settings.dropcap_bs_ratio, ..Default::default() };
                let mut font = Vec::new();
                for (key, value) in &cmd.options {
                    match key.as_str() {
                        "lines" => dropcap.lines = value.parse().map_err(|_| format!("bad lines {value}"))?,
                        "join" => dropcap.join = truthy(value),
                        "standoff" => dropcap.standoff = Some(self.dimen(value)?),
                        "raise" => dropcap.raise = self.dimen(value)?,
                        "shift" => dropcap.shift = self.dimen(value)?,
                        "size" => dropcap.size = Some(self.dimen(value)?),
                        "scale" => dropcap.scale = value.parse().map_err(|_| format!("bad scale {value}"))?,
                        "strict" => dropcap.strict = truthy(value),
                        "depthadjust" => dropcap.depth_adjust = value.clone(),
                        "color" => dropcap.color = Some(Color::parse(value)?),
                        "family" | "style" | "weight" | "features" => font.push((key.clone(), value.clone())),
                        other => return Err(format!("dropcap option {other}")),
                    }
                }
                dropcap.font = Arc::new(move |f| {
                    for (key, value) in &font {
                        match key.as_str() {
                            "family" => f.family = Some(value.clone()),
                            "style" => f.style = if value.eq_ignore_ascii_case("italic") { FontStyle::Italic } else { FontStyle::Normal },
                            "weight" => f.weight = FontWeight(value.parse().unwrap_or(400)),
                            _ => f.features = value.clone(),
                        }
                    }
                });
                self.sync()?;
                dropcap.typeset(self, |d| d.process(content).map_err(Failed)).map_err(|Failed(e)| e)?;
            }
            "pullquote" => {
                let mut quote = Pullquote { author: cmd.option("author").map(String::from), ..Default::default() };
                if let Some(scale) = cmd.option("scale") {
                    quote.scale = scale.parse().map_err(|_| format!("bad scale {scale}"))?;
                }
                if let Some(color) = cmd.option("color") {
                    quote.color = Color::parse(color)?;
                }
                if let Some(setback) = cmd.option("setback") {
                    quote.setback = Some(self.dimen(setback)?);
                }
                if self.target < (0, 15, 0) {
                    quote.attribution = Arc::new(|doc, author| {
                        doc.update_font(|f| f.style = FontStyle::Italic)?;
                        let fill = Length::new(Measurement::pt(0.0), Measurement::pt(INFINITY), Measurement::pt(0.0));
                        let skips = doc.line_skips();
                        doc.set_line_skips(LineSkips { left: fill, par_fill: Length::zero(), ..skips }).set_paragraph_indent(0.0);
                        doc.add_text(format!("— {author}"));
                        Ok(())
                    });
                }
                self.sync()?;
                self.scoped(|d| quote.typeset(d, |d| d.process(content).map_err(Failed)).map_err(|Failed(e)| e))?;
            }
            "indexentry" => {
                let label = cmd.option("label").map_or_else(|| sil::plain_text(content), String::from);
                self.sync()?;
                self.doc.add_index_entry(cmd.option("index").unwrap_or("main"), label);
            }
            "printindex" => {
                self.sync()?;
                Indexer::default().typeset(&mut self.doc, cmd.option("index").unwrap_or("main"), &DefaultIndexStyle).map_err(err)?;
            }
            "loadbibliography" => {
                let path = self.resolve(opt("file")?)?;
                let bib = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                self.bibliography.load_bibtex(&bib).map_err(err)?;
            }
            "bibliographystyle" => {
                let (style, lang) = (opt("style")?, cmd.option("lang"));
                if self.bibliography.set_style(style, lang).is_err() {
                    let path = self.resolve(&format!("packages/bibtex/csl/styles/{style}.csl"))?;
                    let xml = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                    self.bibliography.set_csl_style(&xml, lang).map_err(err)?;
                }
            }
            "nocite" => self.bibliography.nocite(&citation_key(cmd, content)).map_err(err)?,
            "cite" => {
                self.sync()?;
                self.bibliography.cite(&mut self.doc, vec![cite(cmd, content)]).map_err(err)?;
            }
            "cites" => {
                let mut cites = Vec::new();
                for child in content {
                    match child {
                        Content::Command(c) if c.name == "cite" => cites.push(cite(c, c.content.as_deref().unwrap_or(&[]))),
                        Content::Command(c) if c.name == "nocite" => {
                            self.bibliography.nocite(&citation_key(c, c.content.as_deref().unwrap_or(&[]))).map_err(err)?
                        }
                        Content::Command(_) => return Err("only \\cite and \\nocite are allowed in \\cites".into()),
                        _ => {}
                    }
                }
                self.sync()?;
                self.bibliography.cite(&mut self.doc, cites).map_err(err)?;
            }
            "reference" => {
                self.sync()?;
                self.bibliography.reference(&mut self.doc, &citation_key(cmd, content)).map_err(err)?;
            }
            "printbibliography" => {
                self.sync()?;
                let cited = cmd.option("cited").is_none_or(truthy);
                self.bibliography.typeset(&mut self.doc, cited).map_err(err)?;
            }
            "rotate" | "scalebox" => {
                self.sync()?;
                self.doc.start_hbox();
                self.process(content)?;
                self.sync()?;
                let hbox = self.doc.make_hbox().map_err(err)?;
                let number = |key: &str, default: f64| cmd.option(key).map_or(Ok(default), |v| v.trim().parse::<f64>().map_err(|e| format!("{key}: {e}")));
                if cmd.name == "rotate" {
                    let angle = opt("angle")?.trim().parse::<f64>().map_err(|e| format!("angle: {e}"))?;
                    self.doc.add_rotated(hbox, angle);
                } else {
                    self.doc.add_scaled(hbox, number("xratio", 1.0)?, number("yratio", 1.0)?).map_err(err)?;
                }
            }
            "table" => {
                let mut rows = Vec::new();
                self.sync()?;
                let indent = self.doc.paragraph_indent();
                self.doc.set_paragraph_indent(0.0);
                for row in content.iter().filter_map(|c| match c {
                    sil::Content::Command(c) if c.name == "tr" => Some(c),
                    _ => None,
                }) {
                    let mut cells = Vec::new();
                    for cell in row.content.iter().flatten().filter_map(|c| match c {
                        sil::Content::Command(c) if c.name == "td" => Some(c),
                        _ => None,
                    }) {
                        self.doc.start_hbox();
                        self.process(cell.content.as_deref().unwrap_or(&[]))?;
                        self.sync()?;
                        cells.push(self.doc.make_hbox().map_err(err)?);
                    }
                    rows.push(cells);
                }
                self.doc.set_paragraph_indent(indent);
                self.doc.add_simple_table(rows).map_err(err)?;
            }
            "rebox" => {
                self.sync()?;
                self.doc.start_hbox();
                self.process(content)?;
                let mut hbox = self.doc.make_hbox().map_err(err)?;
                for (key, dimension) in [("width", &mut hbox.width), ("height", &mut hbox.height), ("depth", &mut hbox.depth)] {
                    if let Some(value) = cmd.option(key) {
                        *dimension = self.length(value)?;
                    }
                }
                if cmd.option("phantom").is_some_and(truthy) {
                    hbox.ink = Some(Ink::Phantom);
                }
                self.doc.add_box(hbox);
            }
            "pdf:destination" => {
                self.sync()?;
                self.doc.add_destination(opt("name")?);
            }
            "pdf:bookmark" => {
                let level = cmd.option("level").map_or(Ok(1), str::parse).map_err(|e: std::num::ParseIntError| e.to_string())?;
                self.doc.add_bookmark_at(opt("title")?, level, opt("dest")?);
            }
            "pdf:link" => {
                let dest = opt("dest")?.to_string();
                let dest = if cmd.option("external").is_some_and(truthy) { LinkDest::Uri(dest) } else { LinkDest::Internal(dest) };
                self.sync()?;
                self.doc.start_link(dest);
                self.process(content)?;
                self.doc.end_hbox();
            }
            "tocentry" => {
                let level = cmd.option("level").map_or(Ok(1), str::parse).map_err(|e: std::num::ParseIntError| e.to_string())?;
                self.sync()?;
                self.doc.add_toc_entry(level, cmd.option("number").map(String::from), sil::plain_text(content));
            }
            "tableofcontents" => {
                let depth = cmd.option("depth").map_or(Ok(3), str::parse).map_err(|e: std::num::ParseIntError| e.to_string())?;
                let toc = TableOfContents { depth, linking: cmd.option("linking").is_none_or(truthy) };
                self.sync()?;
                toc.typeset(&mut self.doc, &DefaultTocStyle).map_err(err)?;
            }
            "latin-in-tate" => {
                self.sync()?;
                Typesetter::add_latin_in_tate(self, |d| d.process(content).map_err(Failed)).map_err(|Failed(e)| e)?;
            }
            "show-hanmen" => {
                let grid = self.hanmen.ok_or("show-hanmen called on a frame with no hanmen")?;
                self.doc.show_hanmen(&grid).map_err(err)?;
            }
            "font:add-fallback" => {
                let mut fallback = FontFallback::default();
                for (k, v) in &cmd.options {
                    match k.as_str() {
                        "family" => fallback.family = Some(v.clone()),
                        "size" => {
                            fallback.size = Some(match v.trim() {
                                v if v.ends_with("em") || v.ends_with("en") => Measurement::from_str(v).map_err(|_| format!("bad size {v}"))?,
                                v => Measurement::pt(self.dimen(v)?),
                            })
                        }
                        "weight" => fallback.weight = Some(FontWeight(v.parse().map_err(|_| format!("bad weight {v}"))?)),
                        "style" => fallback.style = Some(if v.eq_ignore_ascii_case("italic") { FontStyle::Italic } else { FontStyle::Normal }),
                        "features" => fallback.features = Some(v.clone()),
                        other => return Err(format!("fallback option {other}")),
                    }
                }
                self.add_fallback(fallback)?;
            }
            "font:remove-fallback" => {
                self.doc.remove_font_fallback();
                self.fallbacks = self.fallbacks.saturating_sub(1);
                if self.fallbacks == 0 {
                    self.doc.leave_hmode(true).map_err(err)?;
                }
            }
            "font:clear-fallbacks" => {
                if self.fallbacks > 0 {
                    self.doc.clear_font_fallbacks();
                    self.doc.leave_hmode(true).map_err(err)?;
                    self.fallbacks = 0;
                }
            }
            "unichar" => {
                let arg = sil::plain_text(content);
                let arg = arg.trim();
                let cp = match arg.strip_prefix("U+").or_else(|| arg.strip_prefix("u+")).or_else(|| arg.strip_prefix("0x")).or_else(|| arg.strip_prefix("0X")) {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => arg.parse(),
                };
                let c = cp.ok().and_then(char::from_u32).ok_or_else(|| format!("bad codepoint {arg}"))?;
                self.sync()?;
                self.doc.add_char(c);
            }
            "color" => {
                let color = Color::parse(cmd.option("color").unwrap_or("black"))?;
                let previous = self.doc.color();
                self.doc.set_color(color);
                let r = self.process(content);
                match previous {
                    Some(c) => self.doc.set_color(c),
                    None => self.doc.clear_color(),
                };
                r?
            }
            "strong" => self.scoped(|d| {
                d.update_font(|f| f.weight = FontWeight(700))?;
                d.process(content)
            })?,
            "font" => {
                let apply = |d: &mut Self| -> Result<(), String> {
                    let before = d.doc.font_spec().cloned();
                    let (named, options): (Vec<_>, Vec<_>) = cmd.options.iter().partition(|(k, _)| k.starts_with(char::is_uppercase));
                    if !named.is_empty() {
                        let mut features = OtFeatures::parse(&before.as_ref().map(|f| f.features.clone()).unwrap_or_default());
                        for (k, v) in &named {
                            features.load_option(k, v, false)?;
                        }
                        let features = match cmd.option("features") {
                            Some(explicit) => format!("{explicit};{features}"),
                            None => features.to_string(),
                        };
                        d.set_font_option("features", &features)?;
                    }
                    for (k, v) in options.iter().filter(|(k, _)| named.is_empty() || k != "features") {
                        if k != "adjust" {
                            d.set_font_option(k, v)?;
                        }
                        if k == "family" {
                            d.update_font(|f| f.filename = None)?;
                        }
                    }
                    if let Some(adjust) = cmd.option("adjust") {
                        d.adjust_font_size(adjust, before)?;
                    }
                    Ok(())
                };
                if cmd.content.is_some() {
                    self.scoped(|d| {
                        apply(d)?;
                        d.process(content)
                    })?;
                } else {
                    apply(self)?;
                }
            }
            "set" => {
                let parameter = cmd.option("parameter");
                let value = cmd.option("value").unwrap_or("");
                if let Some(p) = parameter.filter(|_| cmd.option("reset").is_some_and(truthy)) {
                    self.reset_setting(p)?;
                } else if let Some(p) = parameter.filter(|_| cmd.option("makedefault").is_some_and(truthy)) {
                    self.set_default(p, value)?;
                } else if cmd.content.is_some() {
                    self.scoped(|d| {
                        if let Some(p) = parameter {
                            d.set(p, value)?;
                        }
                        d.process(content)
                    })?;
                } else if let Some(p) = parameter {
                    self.set(p, value)?;
                }
            }
            "language" => {
                let lang = opt("main")?.to_string();
                if cmd.content.is_some() {
                    self.scoped(|d| {
                        d.settings.language = lang;
                        d.process(content)
                    })?;
                } else {
                    self.settings.language = lang;
                }
            }
            "lorem" => {
                let words = cmd.option("words").and_then(|w| w.parse().ok()).unwrap_or(50);
                if cmd.option("counter").is_some_and(truthy) {
                    self.numbered_lorem(words)?;
                } else {
                    self.lorem(words)?;
                }
            }
            "raise" | "lower" => {
                let height = self.dimen(opt("height")?)?;
                let height = if cmd.name == "raise" { height } else { -height };
                self.doc.add_baseline_shift(height);
                self.process(content)?;
                self.doc.add_baseline_shift(-height);
            }
            "hyphenator:add-exceptions" => {
                let lang = cmd.option("lang").map_or_else(|| self.settings.language.clone(), str::to_string);
                let words: String = content
                    .iter()
                    .filter_map(|c| match c {
                        Content::Text(t) => Some(t.as_str()),
                        Content::Command(_) => None,
                    })
                    .collect();
                self.doc.add_hyphenation_exceptions(&lang, words.split_whitespace());
            }
            "date" | "sender" | "recipient" | "salutation" if self.letter.is_some() => {
                let i = ["date", "sender", "recipient", "salutation"].iter().position(|n| *n == cmd.name).expect("part");
                self.letter.as_mut().expect("letter")[i] = Some(content.to_vec());
            }
            "date" => {
                let format = cmd.option("format").unwrap_or("%c");
                let time = match cmd.option("time") {
                    Some(t) => DateTime::from_unix(t.parse().map_err(|_| format!("bad time {t}"))?, 0),
                    None => DateTime::now_utc(),
                };
                self.sync()?;
                self.doc.add_text(time.format(format).map_err(|e| e.to_string())?);
            }
            "letter" => {
                let [date, sender, recipient, salutation] = self.letter.clone().ok_or("\\letter outside the letter class")?;
                self.set("document.parindent", "0pt")?;
                self.set("current.parindent", "0pt")?;
                self.sync()?;
                let part = |c: Option<Vec<Content>>| {
                    c.map(|c| Box::new(move |d: &mut Self| d.process(&c).map_err(Failed)) as LetterPart<'_, Self, Failed>)
                };
                let parts = LetterParts { date: part(date), sender: part(sender), recipient: part(recipient), salutation: part(salutation) };
                Letter::letter(self, parts, |d| d.process(content).map_err(Failed)).map_err(|Failed(e)| e)?;
            }
            "chapter" | "section" | "subsection" => {
                self.book()?;
                self.sync()?;
                let heading = Heading { numbering: cmd.option("numbering").is_none_or(truthy), toc: cmd.option("toc").is_none_or(truthy) };
                let title = |d: &mut Self| d.scoped(|d| d.process(content)).map_err(Failed);
                match cmd.name.as_str() {
                    "chapter" => Book::chapter(self, heading, title),
                    "section" => Book::section(self, heading, title),
                    _ => Book::subsection(self, heading, title),
                }
                .map_err(|Failed(e)| e)?;
            }
            "footnote" => self.footnote(content)?,
            "footnote:separator" => {
                self.footnote_class()?;
                let nodes = self.scoped(|d| {
                    d.sync()?;
                    d.doc.push_typesetter(None).map_err(err)?;
                    d.process(content)?;
                    d.doc.pop_typesetter().map_err(err)
                })?;
                if let Some(class) = self.doc.insertion_class_mut("footnote") {
                    class.top_box = nodes;
                }
            }
            "footnote:options" => {
                self.footnote_class()?;
                let max = cmd.option("maxHeight").map(|v| self.dimen(v)).transpose()?;
                let skip = cmd.option("interInsertionSkip").map(|v| self.dimen(v)).transpose()?;
                let class = self.doc.insertion_class_mut("footnote").expect("footnote class");
                if let Some(max) = max {
                    class.max_height = max;
                }
                if let Some(skip) = skip {
                    class.inter_skip = skip;
                }
            }
            name if self.defines.contains_key(name) => {
                let body = self.defines[name].clone();
                self.macro_content.push(content.to_vec());
                let r = self.process(&body);
                self.macro_content.pop();
                r?;
            }
            other => return Err(format!("unhandled command \\{other}")),
        }
        Ok(())
    }

    fn set_font_option(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "family" => self.update_font(|f| f.family = Some(value.to_string()))?,
            "size" => {
                let size = self.dimen(value)?;
                self.update_font(|f| f.size = size)?
            }
            "style" => {
                let style = if value.eq_ignore_ascii_case("italic") { FontStyle::Italic } else { FontStyle::Normal };
                self.update_font(|f| f.style = style)?
            }
            "weight" => {
                let weight = value.parse().map_err(|_| format!("bad weight {value}"))?;
                self.update_font(|f| f.weight = FontWeight(weight))?
            }
            "language" => self.settings.language = value.to_string(),
            "features" => self.update_font(|f| f.features = value.to_string())?,
            "variations" => self.update_font(|f| f.variations = value.to_string())?,
            "direction" => {
                let dir = direction(value).ok_or_else(|| format!("bad direction {value}"))?;
                self.update_font(|f| f.direction = dir)?
            }
            "script" => self.update_font(|f| f.script = value.to_string())?,
            "filename" => {
                let path = match value.strip_prefix(".fonts/") {
                    Some(file) => self.corpus.font_dir.join("extra").join(file),
                    None => self.corpus.font_dir.with_file_name("sile").join(value),
                };
                let path = path.to_string_lossy().into_owned();
                self.update_font(|f| f.filename = (!value.is_empty()).then_some(path))?
            }
            _ => return Err(format!("font option {key}")),
        }
        Ok(())
    }

    fn add_fallback(&mut self, fallback: FontFallback) -> Result<(), String> {
        self.sync()?;
        if self.fallbacks == 0 {
            self.doc.leave_hmode(true).map_err(|e| e.to_string())?;
        }
        self.doc.add_font_fallback(fallback).map_err(|e| e.to_string())?;
        self.fallbacks += 1;
        Ok(())
    }

    /// SILE's `\font[adjust=...]`, applied after the font's other options:
    /// scale the font so its ex or cap height matches the previous font's.
    fn adjust_font_size(&mut self, adjust: &str, before: Option<FontSpec>) -> Result<(), String> {
        let adjust = adjust.trim();
        let (amount, metric) = adjust.split_at(adjust.find(|c: char| c.is_alphabetic()).unwrap_or(0));
        let ratio: f64 = if amount.trim().is_empty() { 1.0 } else { amount.trim().parse().map_err(|_| format!("bad adjust {adjust}"))? };
        if metric != "ex-height" {
            return Err(format!("font adjust {metric}"));
        }
        let new = self.dimen("1ex")?;
        let (Some(before), Some(after)) = (before, self.doc.font_spec().cloned()) else {
            return Ok(());
        };
        self.doc.set_font_spec(before).map_err(|e| e.to_string())?;
        let current = self.dimen("1ex")?;
        let size = after.size * ratio * current / new;
        self.doc.set_font_spec(FontSpec { size, ..after }).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) fn reset_setting(&mut self, parameter: &str) -> Result<(), String> {
        let value = self.defaults.get(parameter).cloned().ok_or_else(|| format!("no default for {parameter}"))?;
        self.set(parameter, &value)
    }

    pub(crate) fn set_default(&mut self, parameter: &str, value: &str) -> Result<(), String> {
        self.defaults.insert(parameter.to_string(), value.to_string());
        self.set(parameter, value)
    }

    pub(crate) fn set(&mut self, parameter: &str, value: &str) -> Result<(), String> {
        let num = || {
            value
                .parse::<f64>()
                .map_err(|_| format!("bad number {value}"))
        };
        match parameter {
            "font.family" => self.set_font_option("family", value)?,
            "font.size" => self.set_font_option("size", value)?,
            "font.style" => self.set_font_option("style", value)?,
            "font.weight" => self.set_font_option("weight", value)?,
            "font.features" => self.set_font_option("features", value)?,
            "font.variations" => self.set_font_option("variations", value)?,
            "font.filename" => self.set_font_option("filename", value)?,
            "typesetter.italicCorrection" => {
                self.doc.set_italic_correction(truthy(value).then(ItalicCorrection::default));
            }
            "math.font.family" => self.doc.math_settings_mut().family = value.to_string(),
            "math.font.filename" => self.doc.math_settings_mut().filename = Some(value.to_string()).filter(|v| !v.is_empty()),
            "math.font.size" => self.doc.math_settings_mut().size = Some(num()?),
            "math.font.weight" => self.doc.math_settings_mut().weight = num()? as u16,
            "math.font.script.feature" => {
                self.doc.math_settings_mut().script_feature = Some(value.to_string()).filter(|v| !v.is_empty())
            }
            "math.displayskip" => self.doc.math_settings_mut().display_skip = MathLength::from_str(value)?,
            "math.predisplaypenalty" => self.doc.math_settings_mut().pre_display_penalty = num()? as i32,
            "math.postdisplaypenalty" => self.doc.math_settings_mut().post_display_penalty = num()? as i32,
            "document.language" => self.settings.language = value.to_string(),
            "document.parindent" => self.settings.parindent = value.to_string(),
            "chordmode.offset" => self.chord_offset = Some(value.to_string()),
            "shaper.complexspaces" => {
                self.doc.set_complex_spaces(truthy(value));
            }
            "document.parskip" => self.settings.parskip = value.to_string(),
            "document.baselineskip" => self.settings.baselineskip = value.to_string(),
            "document.lineskip" => self.settings.lineskip = value.to_string(),
            "document.letterspaceglue" => self.settings.letterspace = Some(value.to_string()).filter(|v| !v.is_empty()),
            "ruby.opentype" => self.doc.ruby_settings_mut().opentype = truthy(value),
            "ruby.height" | "ruby.latinspacer" => {
                let m = Measurement::from_str(value.trim()).map_err(|_| format!("bad {parameter} {value}"))?;
                let settings = self.doc.ruby_settings_mut();
                if parameter == "ruby.height" {
                    settings.height = m;
                } else {
                    settings.latin_spacer = m;
                }
            }
            "lists.parskip" => {
                let skip = self.length(value)?;
                self.doc.list_settings_mut().parskip = skip;
            }
            "lists.enumerate.leftmargin" => self.doc.list_settings_mut().enumerate_margin = Measurement::from_str(value).map_err(|_| format!("bad length {value}"))?,
            "lists.enumerate.labelindent" => self.doc.list_settings_mut().enumerate_label_indent = Measurement::from_str(value).map_err(|_| format!("bad length {value}"))?,
            "lists.itemize.leftmargin" => self.doc.list_settings_mut().itemize_margin = Measurement::from_str(value).map_err(|_| format!("bad length {value}"))?,
            "shaper.tracking" => self.settings.tracking = if value.is_empty() { None } else { Some(num()?) },
            "typesetter.obeyspaces" => self.settings.obey_spaces = value == "true",
            "languages.fixedNbsp" => self.settings.fixed_nbsp = value == "true",
            "document.lskip" => self.settings.skips.left = self.length(value)?,
            "document.rskip" => self.settings.skips.right = self.length(value)?,
            "typesetter.parfillskip" => self.settings.skips.par_fill = self.length(value)?,
            "current.parindent" => {
                let indent = self.dimen(value)?;
                self.doc.set_current_indent(Some(indent));
            }
            "linebreak.tolerance" => self.doc.linebreak_settings_mut().tolerance = num()? as i64,
            "linebreak.pretolerance" => {
                self.doc.linebreak_settings_mut().pretolerance = Some(num()? as i64)
            }
            "linebreak.hyphenPenalty" => {
                self.doc.linebreak_settings_mut().hyphen_penalty = num()? as i64
            }
            "linebreak.adjdemerits" => {
                self.doc.linebreak_settings_mut().adj_demerits = num()? as i64
            }
            "linebreak.emergencyStretch" => {
                self.doc.linebreak_settings_mut().emergency_stretch = self.dimen(value)?
            }
            "shaper.spaceenlargementfactor"
            | "shaper.spacestretchfactor"
            | "shaper.spaceshrinkfactor" => {
                let s = &mut self.settings.space;
                match parameter {
                    "shaper.spaceenlargementfactor" => s.enlargement_factor = num()?,
                    "shaper.spacestretchfactor" => s.stretch_factor = num()?,
                    _ => s.shrink_factor = num()?,
                }
            }
            p if p.starts_with("linespacing.") => {
                let length = if value.is_empty() || p == "linespacing.method" { Length::zero() } else { self.relative_length(value)? };
                let l = self.settings.line_spacing.get_or_insert_with(Default::default);
                match p {
                    "linespacing.method" => {
                        l.method = ["tex", "fixed", "fit-glyph", "fit-font", "css"]
                            .into_iter()
                            .find(|m| *m == value)
                            .ok_or_else(|| format!("line spacing method {value}"))?
                    }
                    "linespacing.fixed.baselinedistance" => l.fixed = length,
                    "linespacing.fit-glyph.extra-space" => l.fit_glyph = length,
                    "linespacing.fit-font.extra-space" => l.fit_font = length,
                    "linespacing.css.line-height" => l.css = length,
                    "linespacing.minimumfirstlineposition" => l.minimum_first_line = length,
                    _ => return Err(format!("setting {p}")),
                }
            }
            "typesetter.fixedSpacingAfterInitialEmdash" => {
                self.doc.set_fixed_space_after_dash(truthy(value));
            }
            "typesetter.softHyphen" => {
                self.doc.set_soft_hyphens(truthy(value));
            }
            "languages.tr.replaceApostropheAtHyphenation" => {
                self.doc.set_replace_apostrophe_at_hyphenation(truthy(value));
            }
            "harfbuzz.subshapers" => {
                let shapers: Vec<&str> = value.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                self.doc.set_subshapers(&shapers);
            }
            "dropcaps.bsratio" => {
                self.settings.dropcap_bs_ratio = (!value.is_empty()).then(|| value.parse()).transpose().map_err(|_| format!("bad bsratio {value}"))?;
            }
            "typesetter.breakwidth" => {
                let width = if value.is_empty() { None } else { Some(self.dimen(value)?) };
                self.doc.set_break_width(width);
            }
            "languages.am.justification" => match value {
                "left" | "centered" => self.settings.ethiopic_centered = value == "centered",
                _ => return Err(format!("Amharic justification {value}")),
            },
            "shaper.variablespaces" => self.settings.space.variable_spaces = truthy(value),
            "document.spaceskip" => {
                self.settings.space.skip = if value.is_empty() { None } else { Some(self.length(value)?) }
            }
            other => return Err(format!("setting {other}")),
        }
        Ok(())
    }

    /// `packages.retrograde`: restore defaults from older SILE releases.
    fn retrograde(&mut self, target: &str) -> Result<(), String> {
        self.target = semver(target);
        let family = self.defaults.get("font.family").map(String::as_str);
        if self.target < (0, 15, 14) && family == Some("Gentium Book") {
            self.update_font(|f| f.family = Some("Gentium Plus".into()))?;
            self.defaults.insert("font.family".into(), "Gentium Plus".into());
        }
        if self.target < (0, 15, 0) {
            self.settings.parindent = "20pt".into();
            self.settings.space.enlargement_factor = 1.2;
        }
        if self.target < (0, 15, 10) {
            self.doc.page_break_settings_mut().broken_penalty = 0;
        }
        Ok(())
    }

    /// A SILE length such as `2em plus 1em minus 0.5em`, absolutized against
    /// the current font and page.
    pub(crate) fn length(&mut self, value: &str) -> Result<Length, String> {
        let (natural, rest) = match value.split_once(" plus ") {
            Some((n, r)) => (n, Some(r)),
            None => (value, None),
        };
        let (natural, shrink) = match natural.split_once(" minus ") {
            Some((n, m)) => (n, Some(m)),
            None => (natural, None),
        };
        let (stretch, shrink) = match rest.map(|r| r.split_once(" minus ")) {
            Some(Some((p, m))) => (Some(p), Some(m)),
            Some(None) => (rest, shrink),
            None => (None, shrink),
        };
        let part = |d: &mut Self, v: Option<&str>| v.map_or(Ok(0.0), |v| d.dimen(v));
        Ok(Length::new(
            Measurement::pt(self.dimen(natural)?),
            Measurement::pt(part(self, stretch)?),
            Measurement::pt(part(self, shrink)?),
        ))
    }

    /// A SILE length whose `em` parts stay relative to the font.
    fn relative_length(&mut self, value: &str) -> Result<Length, String> {
        let absolute = self.length(value)?;
        let parts: Vec<&str> = value.split(" plus ").flat_map(|p| p.split(" minus ")).collect();
        let relative = |d: &mut Self, part: Option<&&str>, abs: Measurement| -> Result<Measurement, String> {
            match part.map(|p| p.trim()) {
                Some(p) if p.ends_with("em") => Ok(Measurement::new(d.dimen(p)? / d.dimen("1em")?, Unit::Em)),
                _ => Ok(abs),
            }
        };
        let (stretch, shrink) = match (value.contains(" plus "), value.contains(" minus ")) {
            (true, true) => (parts.get(1), parts.get(2)),
            (true, false) => (parts.get(1), None),
            (false, true) => (None, parts.get(1)),
            _ => (None, None),
        };
        Ok(Length::new(
            relative(self, parts.first(), absolute.length)?,
            relative(self, stretch, absolute.stretch)?,
            relative(self, shrink, absolute.shrink)?,
        ))
    }

    /// One dimension in any of SILE's units, in points.
    /// `length`, leaving `em` for the builder to resolve against the font
    /// in force where it is used.
    fn em_length(&mut self, value: &str) -> Result<Length, String> {
        let mut parts = value.splitn(2, " plus ");
        let natural = parts.next().unwrap_or("").trim();
        let em = natural.strip_suffix("em").and_then(|n| n.parse::<f64>().ok());
        let mut length = self.length(value)?;
        if let Some(n) = em.filter(|_| !natural.contains(" minus ")) {
            length.length = Measurement::new(n, Unit::Em);
        }
        Ok(length)
    }

    pub(crate) fn dimen(&mut self, value: &str) -> Result<f64, String> {
        let value = value.trim();
        let split = value
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+')))
            .unwrap_or(value.len());
        let (n, unit) = value.split_at(split);
        let n: f64 = if n.is_empty() && !unit.is_empty() {
            1.0
        } else {
            n.parse().map_err(|_| format!("bad length {value}"))?
        };
        let font = self.doc.font_spec().cloned().unwrap_or_default();
        let family = font.family.unwrap_or_default();
        let size = font.size;
        let metrics = || {
            self.corpus
                .fonts
                .em_metrics(&family, font.weight.0, font.style == FontStyle::Italic)
                .ok_or_else(|| format!("no metrics for {family}"))
        };
        let (frame_w, frame_h) = self.doc.frame_size().map_err(|e| e.to_string())?;
        Ok(match unit.trim() {
            "" | "pt" => n,
            "mm" => n * 72.0 / 25.4,
            "cm" => n * 72.0 / 2.54,
            "in" => n * 72.0,
            "em" => n * size,
            "en" => n * size / 2.0,
            "ex" => n * metrics()?.x_height * size,
            "spc" => n * metrics()?.space * size,
            "bs" => {
                let bls = self.settings.baselineskip.clone();
                n * self.dimen(bls.split(" plus ").next().unwrap_or("0"))?
            }
            "%pw" => n / 100.0 * self.paper.width,
            "%ph" => n / 100.0 * self.paper.height,
            "%fw" => n / 100.0 * frame_w,
            "%fh" => n / 100.0 * frame_h,
            "%pmax" => n / 100.0 * self.paper.width.max(self.paper.height),
            "%pmin" => n / 100.0 * self.paper.width.min(self.paper.height),
            "%fmax" => n / 100.0 * frame_w.max(frame_h),
            "%fmin" => n / 100.0 * frame_w.min(frame_h),
            "%lw" => {
                let s = self.settings.skips;
                n / 100.0 * frame_w
                    - s.left.length.to_pt().unwrap_or(0.0)
                    - s.right.length.to_pt().unwrap_or(0.0)
            }
            other => return Err(format!("unsupported unit {other} in {value}")),
        })
    }
}

fn is_blank(content: &[Content]) -> bool {
    content.iter().all(|c| matches!(c, Content::Text(t) if t.trim().is_empty()))
}

fn truthy(v: &str) -> bool {
    matches!(v, "true" | "yes" | "1")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn split_paragraphs(text: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut lines = text.split('\n').peekable();
    let mut blank_run = false;
    let mut first = true;
    while let Some(line) = lines.next() {
        if !first && line.trim().is_empty() && lines.peek().is_some() {
            if !blank_run {
                out.push(String::new());
                blank_run = true;
            }
            continue;
        }
        if !first && !blank_run {
            out.last_mut().unwrap().push('\n');
        }
        blank_run = false;
        first = false;
        out.last_mut().unwrap().push_str(line);
    }
    out
}

fn lorem(source: &str, words: usize) -> String {
    let ends: Vec<usize> = source
        .split_whitespace()
        .map(|w| w.as_ptr() as usize - source.as_ptr() as usize + w.len())
        .collect();
    if ends.is_empty() {
        return String::new();
    }
    let rest = words % ends.len();
    let tail = if rest == 0 { "" } else { &source[..ends[rest - 1]] };
    source.repeat(words / ends.len()) + tail
}

/// The lorem ipsum text from SILE's `packages/lorem/init.lua`.
pub fn lorem_source(init_lua: &str) -> String {
    init_lua
        .split_once("local lorem = [[")
        .and_then(|(_, rest)| rest.split_once("]]"))
        .map(|(text, _)| text.strip_prefix('\n').unwrap_or(text).to_string())
        .unwrap_or_default()
}

fn semver(v: &str) -> (u32, u32, u32) {
    let mut parts = v
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn paper_size(name: &str) -> Option<PaperSize> {
    name.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_split_on_blank_lines() {
        assert_eq!(split_paragraphs("a\nb\n\n  \nc"), vec!["a\nb", "c"]);
        assert_eq!(split_paragraphs("\nHello\n"), vec!["\nHello\n"]);
    }

    #[test]
    fn paper_sizes() {
        assert_eq!(paper_size("a7").unwrap().width, 209.76378179999998);
        let custom = paper_size("15cm x 6cm").unwrap();
        assert!((custom.width - 425.197).abs() < 0.01);
    }

    #[test]
    fn retrograde_versions_compare() {
        assert!(semver("v0.14.17") < (0, 15, 0));
        assert!(semver("0.15.14") >= (0, 15, 14));
    }
}

fn direction(value: &str) -> Option<Direction> {
    match value.to_ascii_uppercase().as_str() {
        "LTR" | "LTR-TTB" => Some(Direction::LTR),
        "RTL" | "RTL-TTB" => Some(Direction::RTL),
        _ => None,
    }
}

fn citation_key(cmd: &Command, content: &[Content]) -> String {
    cmd.option("key").map_or_else(|| sil::plain_text(content), String::from)
}

fn cite(cmd: &Command, content: &[Content]) -> Cite {
    let locator = cmd.options.iter().find(|(k, _)| k != "key").cloned();
    Cite { locator, ..Cite::new(citation_key(cmd, content)) }
}

/// MathML markup as SIL or XML commands.
fn math_content(content: &[Content]) -> Vec<mathml::Content> {
    let elements = content.iter().any(|c| matches!(c, Content::Command(_)));
    content
        .iter()
        .filter_map(|c| match c {
            Content::Text(t) if elements && t.trim().is_empty() => None,
            Content::Text(t) => Some(mathml::Content::Text(t.clone())),
            Content::Command(cmd) => {
                let mut element = mathml::Element::new(cmd.name.clone());
                element.options = cmd.options.clone();
                element.children = math_content(cmd.content.as_deref().unwrap_or(&[]));
                Some(mathml::Content::Element(element))
            }
        })
        .collect()
}
