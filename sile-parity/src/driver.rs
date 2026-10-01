//! Interprets a SIL tree by calling sile-core's `DocumentBuilder`, emulating
//! SILE's `plain` and `book` classes closely enough to compare layouts.
//! Anything outside the supported subset is reported rather than approximated.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::FromStr;

use sile_core::builder::{BaselineSkip, BuilderError, DocumentBuilder, LineSkips, TextAlign};
use sile_core::class::{Book, Folio, FolioState, Heading, Plain};
use sile_core::insertion::InsertionClass;
use sile_core::node::Node;
use sile_core::font::{FontSpec, FontStyle, FontWeight};
use sile_core::frame::PaperSize;
use sile_core::length::Length;
use sile_core::measurement::{Measurement, Unit};
use sile_core::node::INFINITY;
use sile_core::shaper::SpaceSettings;

use crate::fonts::Fonts;
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
pub fn run(src: &str, format: Format, corpus: &Corpus) -> Result<String, Failure> {
    let tree = match format {
        Format::Sil => sil::parse(src),
        Format::Xml => crate::xml::parse(src),
    }
    .map_err(|e| Failure::Error(format!("parse: {}", e.0)))?;
    let mut missing = BTreeSet::new();
    check(&tree, corpus, &mut BTreeSet::new(), &mut missing);
    if !missing.is_empty() {
        return Err(Failure::Unsupported(missing));
    }
    let mut d = Driver::new(corpus).map_err(Failure::Error)?;
    d.process(&tree).map_err(Failure::Error)?;
    d.finish().map_err(Failure::Error)
}

// ---------------------------------------------------------------------------
// Support check
// ---------------------------------------------------------------------------

const SIMPLE_COMMANDS: &[&str] = &[
    "par",
    "em",
    "strong",
    "noindent",
    "neverindent",
    "indent",
    "smallskip",
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
    " ",
];

const FONT_OPTIONS: &[&str] = &["family", "size", "style", "weight", "language"];

const SETTINGS: &[&str] = &[
    "font.family",
    "font.size",
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
    "typesetter.obeyspaces",
    "languages.fixedNbsp",
    "current.parindent",
    "typesetter.parfillskip",
    "linebreak.tolerance",
    "linebreak.pretolerance",
    "linebreak.emergencyStretch",
    "linebreak.hyphenPenalty",
    "linebreak.adjdemerits",
    "shaper.spaceenlargementfactor",
    "shaper.spacestretchfactor",
    "shaper.spaceshrinkfactor",
];

fn check(
    content: &[Content],
    corpus: &Corpus,
    defined: &mut BTreeSet<String>,
    missing: &mut BTreeSet<String>,
) {
    for c in content {
        let Content::Command(cmd) = c else { continue };
        match cmd.name.as_str() {
            "document" => {
                if let Some(class) = cmd.option("class").filter(|c| !matches!(*c, "plain" | "book")) {
                    missing.insert(format!("class={class}"));
                }
                if let Some(p) = cmd.option("papersize")
                    && paper_size(p).is_none()
                {
                    missing.insert(format!("papersize={p}"));
                }
                for (k, _) in &cmd.options {
                    if !matches!(k.as_str(), "class" | "papersize") {
                        missing.insert(format!("document[{k}]"));
                    }
                }
            }
            "use" => match cmd.option("module") {
                Some("packages.retrograde" | "packages.lorem" | "packages.footnotes") => {}
                Some(m) => {
                    missing.insert(format!("use {m}"));
                }
                None => {
                    missing.insert("use".into());
                }
            },
            "font" => {
                for (k, v) in &cmd.options {
                    if !FONT_OPTIONS.contains(&k.as_str()) {
                        missing.insert(format!("font[{k}]"));
                    } else if k == "family" && !corpus.fonts.has_family(v) {
                        missing.insert(format!("font family {v}"));
                    }
                }
            }
            "set" => match cmd.option("parameter") {
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
                Some(name) => {
                    defined.insert(name.to_string());
                }
                None => {
                    missing.insert("define".into());
                }
            },
            name if SIMPLE_COMMANDS.contains(&name) || defined.contains(name) => {}
            name if cmd.raw.is_some() => {
                missing.insert(format!("\\begin{{{name}}}"));
            }
            name => {
                missing.insert(format!("\\{name}"));
            }
        }
        if let Some(inner) = &cmd.content {
            check(inner, corpus, defined, missing);
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
    obey_spaces: bool,
    fixed_nbsp: bool,
    skips: LineSkips,
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
    obey_spaces: bool,
    fixed_nbsp: bool,
}

/// A driver error passed through a class's typesetting callbacks.
struct Failed(String);

impl From<BuilderError> for Failed {
    fn from(e: BuilderError) -> Self {
        Self(e.to_string())
    }
}

struct Driver<'a> {
    corpus: &'a Corpus<'a>,
    doc: DocumentBuilder,
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
    space_settings: SpaceSettings,
    /// SILE release targeted by `packages.retrograde` (latest if unset).
    target: (u32, u32, u32),
}

impl AsMut<DocumentBuilder> for Driver<'_> {
    fn as_mut(&mut self) -> &mut DocumentBuilder {
        &mut self.doc
    }
}

impl<'a> Driver<'a> {
    fn new(corpus: &'a Corpus<'a>) -> Result<Self, String> {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.load_fonts_dir(corpus.font_dir);
        let spec = FontSpec { family: Some("Gentium Book".into()), ..Default::default() };
        doc.set_font_spec(spec).map_err(|e| e.to_string())?;
        Ok(Self {
            corpus,
            doc,
            paper: PaperSize::A4,
            settings: Settings {
                language: "en".into(),
                parindent: "1bs".into(),
                parskip: "0pt plus 1pt".into(),
                baselineskip: "1.2em plus 1pt".into(),
                lineskip: "1pt".into(),
                letterspace: None,
                obey_spaces: false,
                fixed_nbsp: false,
                skips: LineSkips::default(),
            },
            synced: None,
            depth: 0,
            defines: BTreeMap::new(),
            macro_content: Vec::new(),
            toplevel: None,
            space_settings: SpaceSettings::default(),
            target: (u32::MAX, 0, 0),
        })
    }

    fn finish(mut self) -> Result<String, String> {
        self.par()?;
        self.doc.render_debug().map_err(|e| e.to_string())
    }

    fn folio(&mut self) -> Option<&mut Folio> {
        if self.doc.class_mut::<Book>().is_some() {
            return self.doc.class_mut::<Book>().map(|b| &mut b.folio);
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
    fn footnote_class(&mut self) -> Result<(), String> {
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
        let err = |e: sile_core::builder::BuilderError| e.to_string();
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
    fn sync(&mut self) -> Result<(), String> {
        let bls = self.settings.baselineskip.clone();
        let lineskip = self.settings.lineskip.clone();
        let parindent = self.settings.parindent.clone();
        let parskip = self.settings.parskip.clone();
        let letterspace = match self.settings.letterspace.clone() {
            Some(l) => Some(self.length(&l)?),
            None => None,
        };
        let now = Synced {
            baseline_skip: BaselineSkip { skip: self.length(&bls)?, lineskip: self.dimen(&lineskip)? },
            indent: self.dimen(&parindent)?,
            parskip: self.length(&parskip)?,
            skips: self.settings.skips,
            language: self.settings.language.clone(),
            letterspace,
            obey_spaces: self.settings.obey_spaces,
            fixed_nbsp: self.settings.fixed_nbsp,
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
        push!(obey_spaces, doc.set_obey_spaces(now.obey_spaces));
        push!(fixed_nbsp, doc.set_fixed_nbsp(now.fixed_nbsp));
        self.synced = Some(now);
        if self.depth == 0 {
            self.doc.mark_toplevel();
            self.toplevel = Some(self.settings.clone());
        }
        Ok(())
    }

    fn par(&mut self) -> Result<(), String> {
        self.sync()?;
        self.doc.new_paragraph().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn process(&mut self, content: &[Content]) -> Result<(), String> {
        for c in content {
            match c {
                Content::Text(t) => self.text(t)?,
                Content::Command(cmd) => self.command(cmd)?,
            }
        }
        Ok(())
    }

    fn text(&mut self, text: &str) -> Result<(), String> {
        if matches!(text, "\n" | "\r\n") {
            return Ok(());
        }
        let mut paragraphs = split_paragraphs(text).into_iter().peekable();
        while let Some(chunk) = paragraphs.next() {
            self.add_text(&chunk)?;
            if paragraphs.peek().is_some() {
                self.par()?;
            }
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), String> {
        self.sync()?;
        self.doc.add_text(text);
        Ok(())
    }

    fn update_font(&mut self, f: impl FnOnce(&mut FontSpec)) -> Result<(), String> {
        self.doc.update_font(f).map(|_| ()).map_err(|e| e.to_string())
    }

    fn scoped<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        if self.depth == 0 {
            self.sync()?;
        }
        let saved = self.settings.clone();
        let font = self.doc.font_spec().cloned();
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        self.settings = saved;
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

    fn command(&mut self, cmd: &Command) -> Result<(), String> {
        let content = cmd.content.as_deref().unwrap_or(&[]);
        let opt = |k: &str| {
            cmd.option(k)
                .ok_or_else(|| format!("\\{} needs {k}", cmd.name))
        };
        let err = |e: sile_core::builder::BuilderError| e.to_string();
        match cmd.name.as_str() {
            "document" => {
                if let Some(p) = cmd.option("papersize") {
                    self.paper = paper_size(p).ok_or("bad papersize")?;
                    self.doc.set_page_size(self.paper);
                }
                match cmd.option("class") {
                    Some("book") => self.doc.set_class(Book::new()),
                    _ => self.doc.set_class(Plain::new()),
                };
                self.process(content)?;
            }
            "use" => {
                if cmd.option("module") == Some("packages.retrograde") {
                    self.retrograde(cmd.option("target").unwrap_or(""))?;
                }
            }
            "par" => self.par()?,
            " " => self.add_text(" ")?,
            "comment" => {}
            "noop" => self.process(content)?,
            "nofolios" => self.set_folio_state(FolioState::Off),
            "folios" => self.set_folio_state(FolioState::On),
            "nofoliothispage" => self.set_folio_state(FolioState::OffThisPage),
            "define" => {
                let name = opt("command")?;
                self.defines.insert(name.to_string(), content.to_vec());
            }
            "process" => {
                if let Some(inner) = self.macro_content.pop() {
                    let r = self.process(&inner);
                    self.macro_content.push(inner);
                    r?;
                }
            }
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
            "strong" => self.scoped(|d| {
                d.update_font(|f| f.weight = FontWeight(700))?;
                d.process(content)
            })?,
            "font" => {
                let apply = |d: &mut Self| -> Result<(), String> {
                    for (k, v) in &cmd.options {
                        d.set_font_option(k, v)?;
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
                if cmd.content.is_some() {
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
                if let Some(lang) = cmd.option("main") {
                    self.settings.language = lang.to_string();
                }
                self.process(content)?;
            }
            "lorem" => {
                let words = cmd
                    .option("words")
                    .and_then(|w| w.parse().ok())
                    .unwrap_or(50);
                let text = lorem(self.corpus.lorem, words);
                self.scoped(|d| {
                    d.settings.language = "la".to_string();
                    d.text(&text)
                })?;
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
            "chapter" | "section" | "subsection" => {
                self.book()?;
                self.sync()?;
                let heading = Heading { numbering: cmd.option("numbering").is_none_or(truthy) };
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
            _ => return Err(format!("font option {key}")),
        }
        Ok(())
    }

    fn set(&mut self, parameter: &str, value: &str) -> Result<(), String> {
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
            "document.language" => self.settings.language = value.to_string(),
            "document.parindent" => self.settings.parindent = value.to_string(),
            "document.parskip" => self.settings.parskip = value.to_string(),
            "document.baselineskip" => self.settings.baselineskip = value.to_string(),
            "document.lineskip" => self.settings.lineskip = value.to_string(),
            "document.letterspaceglue" => self.settings.letterspace = Some(value.to_string()),
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
                let mut s = self.space_settings;
                match parameter {
                    "shaper.spaceenlargementfactor" => s.enlargement_factor = num()?,
                    "shaper.spacestretchfactor" => s.stretch_factor = num()?,
                    _ => s.shrink_factor = num()?,
                }
                self.doc.set_space_settings(s);
                self.space_settings = s;
            }
            other => return Err(format!("setting {other}")),
        }
        Ok(())
    }

    /// `packages.retrograde`: restore defaults from older SILE releases.
    fn retrograde(&mut self, target: &str) -> Result<(), String> {
        self.target = semver(target);
        if self.target < (0, 15, 14) {
            self.update_font(|f| f.family = Some("Gentium Plus".into()))?;
        }
        if self.target < (0, 15, 0) {
            self.settings.parindent = "20pt".into();
            self.space_settings.enlargement_factor = 1.2;
            self.doc.set_space_settings(self.space_settings);
        }
        Ok(())
    }

    /// A SILE length such as `2em plus 1em minus 0.5em`, absolutized against
    /// the current font and page.
    fn length(&mut self, value: &str) -> Result<Length, String> {
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

    /// One dimension in any of SILE's units, in points.
    fn dimen(&mut self, value: &str) -> Result<f64, String> {
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

fn measure(value: &str, font_size: f64) -> Result<f64, String> {
    let value = value.trim();
    if let Ok(n) = value.parse::<f64>() {
        return Ok(n);
    }
    if let Some(bs) = value.strip_suffix("bs") {
        let n: f64 = bs
            .trim()
            .parse()
            .map_err(|_| format!("bad length {value}"))?;
        return Ok(n * 1.2 * font_size);
    }
    let m = Measurement::from_str(value).map_err(|_| format!("bad length {value}"))?;
    match m.unit {
        Unit::Em => Ok(m.amount * font_size),
        _ => m
            .to_pt()
            .ok_or_else(|| format!("unsupported length {value}")),
    }
}

fn paper_size(name: &str) -> Option<PaperSize> {
    let named = match name.trim().to_lowercase().as_str() {
        "a3" => Some((841.8897728999999, 1190.551194)),
        "a4" => Some((595.275597, 841.8897728999999)),
        "a5" => Some((419.52756359999995, 595.275597)),
        "a6" => Some((297.6377985, 419.52756359999995)),
        "a7" => Some((209.76378179999998, 297.6377985)),
        "a8" => Some((147.40157639999998, 209.76378179999998)),
        "b5" => Some((498.89764319999995, 708.661425)),
        "b6" => Some((354.3307125, 498.89764319999995)),
        "letter" => Some((612.0, 792.0)),
        "halfletter" => Some((396.0, 612.0)),
        _ => None,
    };
    if let Some((w, h)) = named {
        return Some(PaperSize::new(w, h));
    }
    let (w, h) = name.split_once(" x ")?;
    Some(PaperSize::new(
        measure(w, 10.0).ok()?,
        measure(h, 10.0).ok()?,
    ))
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
