//! Interprets a SIL tree by calling sile-core's `DocumentBuilder`, emulating
//! SILE's `plain` class closely enough to compare layouts. Anything outside
//! the supported subset is reported rather than approximated.

use std::collections::BTreeSet;
use std::str::FromStr;

use sile_core::builder::{BaselineSkip, DocumentBuilder, RunningText, TextAlign};
use sile_core::font::{FontSpec, FontStyle, FontWeight};
use sile_core::frame::PaperSize;
use sile_core::length::Length;
use sile_core::measurement::{Measurement, Unit};
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
    pub lorem: &'a str,
}

/// Typeset a SIL document and return its trace in SILE's debug format.
pub fn run(src: &str, corpus: &Corpus) -> Result<String, Failure> {
    let tree = sil::parse(src).map_err(|e| Failure::Error(format!("parse: {}", e.0)))?;
    let mut missing = BTreeSet::new();
    check(&tree, corpus, &mut missing);
    if !missing.is_empty() {
        return Err(Failure::Unsupported(missing));
    }
    let mut d = Driver::new(corpus);
    d.process(&tree).map_err(Failure::Error)?;
    d.finish().map_err(Failure::Error)
}

// ---------------------------------------------------------------------------
// Support check
// ---------------------------------------------------------------------------

const SIMPLE_COMMANDS: &[&str] = &[
    "par",
    "em",
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
    "center",
    "raggedright",
    "raggedleft",
    "justified",
    "lorem",
    "nofolios",
    "folios",
    "comment",
    "language",
    " ",
];

const FONT_OPTIONS: &[&str] = &["family", "size", "style", "weight", "language"];

const SETTINGS: &[&str] = &[
    "font.family",
    "font.size",
    "font.style",
    "font.weight",
    "document.parindent",
    "document.language",
    "linebreak.tolerance",
    "linebreak.pretolerance",
    "linebreak.emergencyStretch",
    "linebreak.hyphenPenalty",
    "linebreak.adjdemerits",
    "shaper.spaceenlargementfactor",
    "shaper.spacestretchfactor",
    "shaper.spaceshrinkfactor",
];

fn check(content: &[Content], corpus: &Corpus, missing: &mut BTreeSet<String>) {
    for c in content {
        let Content::Command(cmd) = c else { continue };
        match cmd.name.as_str() {
            "document" => {
                if let Some(class) = cmd.option("class").filter(|c| *c != "plain") {
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
                Some("packages.retrograde" | "packages.lorem") => {}
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
                None => {
                    missing.insert("set".into());
                }
            },
            name if SIMPLE_COMMANDS.contains(&name) => {}
            name if cmd.raw.is_some() => {
                missing.insert(format!("\\begin{{{name}}}"));
            }
            name => {
                missing.insert(format!("\\{name}"));
            }
        }
        if let Some(inner) = &cmd.content {
            check(inner, corpus, missing);
        }
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Style {
    family: String,
    size: f64,
    weight: u16,
    italic: bool,
}

#[derive(Debug, Clone)]
struct Settings {
    style: Style,
    parindent: f64,
    align: TextAlign,
    language: String,
}

struct Driver<'a> {
    corpus: &'a Corpus<'a>,
    doc: DocumentBuilder,
    paper: PaperSize,
    settings: Settings,
    registered: BTreeSet<String>,
    /// Runs of the paragraph being collected: (font name, text).
    par: Vec<(String, String)>,
    /// Indent captured when the current paragraph started.
    par_indent: Option<f64>,
    skip_next_indent: bool,
    folios: bool,
    space_settings: SpaceSettings,
}

impl<'a> Driver<'a> {
    fn new(corpus: &'a Corpus<'a>) -> Self {
        let size = 10.0;
        Self {
            corpus,
            doc: DocumentBuilder::new(PaperSize::A4),
            paper: PaperSize::A4,
            settings: Settings {
                style: Style {
                    family: "Gentium Book".into(),
                    size,
                    weight: 400,
                    italic: false,
                },
                parindent: 1.2 * size,
                align: TextAlign::Justify,
                language: "en".into(),
            },
            registered: BTreeSet::new(),
            par: Vec::new(),
            par_indent: None,
            skip_next_indent: false,
            folios: true,
            space_settings: SpaceSettings::default(),
        }
    }

    fn finish(mut self) -> Result<String, String> {
        self.end_paragraph()?;
        self.apply_geometry()?;
        self.doc.render_debug().map_err(|e| e.to_string())
    }

    /// SILE's plain class: content 5%..95% wide and 5%..90% high, folio frame
    /// 92%..97% high. The content frame is the same with or without folios.
    fn apply_geometry(&mut self) -> Result<(), String> {
        let [w, h] = [self.paper.width, self.paper.height];
        if self.folios {
            self.doc.set_margins(0.05 * h, 0.05 * w, 0.03 * h, 0.05 * w);
            self.doc.set_footer_height(0.05 * h, 0.02 * h);
            let default = Style {
                family: self.settings.style.family.clone(),
                size: 10.0,
                weight: 400,
                italic: false,
            };
            let font = self.font_name(&default)?;
            let mut folio = RunningText::new(font, "{page}");
            folio.align = TextAlign::Center;
            self.doc.set_footer(folio);
        } else {
            self.doc.set_margins(0.05 * h, 0.05 * w, 0.10 * h, 0.05 * w);
            self.doc.set_footer_height(0.0, 0.0);
        }
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
        let mut paragraphs = split_paragraphs(text).into_iter().peekable();
        while let Some(chunk) = paragraphs.next() {
            let collapsed = collapse_whitespace(&chunk);
            if !collapsed.trim().is_empty() || !self.par.is_empty() {
                self.add_text(&collapsed)?;
            }
            if paragraphs.peek().is_some() {
                self.end_paragraph()?;
            }
        }
        Ok(())
    }

    fn add_text(&mut self, text: &str) -> Result<(), String> {
        if self.par.is_empty() {
            let text = text.trim_start();
            if text.is_empty() {
                return Ok(());
            }
            self.par_indent = Some(if self.skip_next_indent {
                0.0
            } else {
                self.settings.parindent
            });
            self.skip_next_indent = false;
            let font = self.font_name(&self.settings.style.clone())?;
            self.par.push((font, text.to_string()));
            return Ok(());
        }
        let font = self.font_name(&self.settings.style.clone())?;
        match self.par.last_mut() {
            Some((f, t)) if *f == font => t.push_str(text),
            _ => self.par.push((font, text.to_string())),
        }
        Ok(())
    }

    fn end_paragraph(&mut self) -> Result<(), String> {
        if let Some((_, last)) = self.par.last_mut() {
            let trimmed = last.trim_end().len();
            last.truncate(trimmed);
        }
        let runs = std::mem::take(&mut self.par);
        if runs.iter().all(|(_, t)| t.is_empty()) {
            self.par_indent = None;
            return Ok(());
        }
        self.doc
            .set_paragraph_indent(self.par_indent.take().unwrap_or(0.0));
        self.doc.set_alignment(self.settings.align);
        self.doc.set_language(self.settings.language.clone());
        for (font, text) in runs {
            self.doc.set_font(font);
            self.doc.add_text(text);
        }
        self.doc.new_paragraph().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn font_name(&mut self, style: &Style) -> Result<String, String> {
        let name = format!(
            "{}|{}|{}|{}",
            style.family, style.weight, style.italic, style.size
        );
        if self.registered.insert(name.clone()) {
            let data = self
                .corpus
                .fonts
                .data(&style.family, style.weight, style.italic)
                .ok_or_else(|| format!("font not available: {style:?}"))?;
            let spec = FontSpec {
                family: Some(style.family.clone()),
                size: style.size,
                weight: FontWeight(style.weight),
                style: if style.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..Default::default()
            };
            self.doc
                .load_font_data(name.clone(), data.as_ref().clone(), spec)
                .map_err(|e| e.to_string())?;
        }
        Ok(name)
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self) -> Result<(), String>) -> Result<(), String> {
        let saved = self.settings.clone();
        let r = f(self);
        self.settings = saved;
        r
    }

    fn command(&mut self, cmd: &Command) -> Result<(), String> {
        let content = cmd.content.as_deref().unwrap_or(&[]);
        match cmd.name.as_str() {
            "document" => {
                if let Some(p) = cmd.option("papersize") {
                    self.paper = paper_size(p).ok_or("bad papersize")?;
                    self.doc.set_page_size(self.paper);
                }
                self.apply_geometry()?;
                let size = self.settings.style.size;
                self.doc.set_baseline_skip(Some(BaselineSkip {
                    skip: Length::new(
                        Measurement::pt(1.2 * size),
                        Measurement::pt(1.0),
                        Measurement::pt(0.0),
                    ),
                    lineskip: 1.0,
                }));
                self.process(content)?;
            }
            "use" => {
                if cmd.option("module") == Some("packages.retrograde") {
                    self.retrograde(cmd.option("target").unwrap_or(""));
                }
            }
            "par" => self.end_paragraph()?,
            " " => self.add_text(" ")?,
            "comment" => {}
            "nofolios" => self.folios = false,
            "folios" => self.folios = true,
            "noindent" => self.skip_next_indent = true,
            "neverindent" => {
                self.settings.parindent = 0.0;
                self.skip_next_indent = true;
                self.process(content)?;
            }
            "indent" => self.process(content)?,
            "smallskip" | "medskip" | "bigskip" => {
                self.end_paragraph()?;
                let amount = match cmd.name.as_str() {
                    "smallskip" => 3.0,
                    "medskip" => 6.0,
                    _ => 12.0,
                };
                self.doc.add_vskip(amount);
            }
            "pagebreak" | "framebreak" | "eject" | "supereject" => {
                self.end_paragraph()?;
                self.doc.add_page_break();
            }
            "center" | "raggedright" | "raggedleft" | "justified" => {
                self.end_paragraph()?;
                let align = match cmd.name.as_str() {
                    "center" => TextAlign::Center,
                    "raggedright" => TextAlign::Left,
                    "raggedleft" => TextAlign::Right,
                    _ => TextAlign::Justify,
                };
                self.scoped(|d| {
                    d.settings.align = align;
                    if align == TextAlign::Center {
                        d.settings.parindent = 0.0;
                    }
                    d.process(content)?;
                    d.end_paragraph()
                })?;
            }
            "em" => self.scoped(|d| {
                d.settings.style.italic = !d.settings.style.italic;
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
                let parameter = cmd.option("parameter").unwrap_or("");
                let value = cmd.option("value").unwrap_or("");
                if cmd.content.is_some() {
                    self.scoped(|d| {
                        d.set(parameter, value)?;
                        d.process(content)
                    })?;
                } else {
                    self.set(parameter, value)?;
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
                self.add_text(&text)?;
            }
            other => return Err(format!("unhandled command \\{other}")),
        }
        Ok(())
    }

    fn set_font_option(&mut self, key: &str, value: &str) -> Result<(), String> {
        let style = &mut self.settings.style;
        match key {
            "family" => style.family = value.to_string(),
            "size" => style.size = measure(value, style.size)?,
            "style" => style.italic = value.eq_ignore_ascii_case("italic"),
            "weight" => style.weight = value.parse().map_err(|_| format!("bad weight {value}"))?,
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
            "document.parindent" => {
                self.settings.parindent = measure(value, self.settings.style.size)?
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
                self.doc.linebreak_settings_mut().emergency_stretch =
                    measure(value, self.settings.style.size)?
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
    fn retrograde(&mut self, target: &str) {
        let target = semver(target);
        if target < (0, 15, 14) {
            self.settings.style.family = "Gentium Plus".into();
        }
        if target < (0, 15, 0) {
            self.settings.parindent = 20.0;
            self.space_settings.enlargement_factor = 1.2;
            self.doc.set_space_settings(self.space_settings);
        }
    }
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

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

fn lorem(source: &str, words: usize) -> String {
    let all: Vec<&str> = source.split_whitespace().collect();
    if all.is_empty() {
        return String::new();
    }
    all.iter()
        .cycle()
        .take(words)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The lorem ipsum text from SILE's `packages/lorem/init.lua`.
pub fn lorem_source(init_lua: &str) -> String {
    init_lua
        .split_once("local lorem = [[")
        .and_then(|(_, rest)| rest.split_once("]]"))
        .map(|(text, _)| text.to_string())
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
    fn whitespace_collapses() {
        assert_eq!(collapse_whitespace("\n a \n b  "), " a b ");
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
