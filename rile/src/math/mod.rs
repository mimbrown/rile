//! Mathematical formulas: a tree of MathML-like elements, laid out from
//! the math font's OpenType MATH table, with a TeX-like syntax on top
//! (`parse_tex`).
// SILE: `math` package.

mod layout;
pub mod mathml;
mod operators;
mod table;
mod tex;
mod variants;

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::builder::{Arranger, BuilderError, LineSkips, Typesetter};
use crate::font::{Direction, FontFace, FontSpec, FontStyle, FontWeight};
use crate::length::Length;
use crate::measurement::Measurement;
use crate::node::{HBox, Ink, MathInk};
use crate::shaper::GlyphItem;
use crate::structure::Role;

pub use layout::{Figure, Glue, GlyphScale, MathItem};
pub use operators::symbol;
pub(crate) use table::MathTable;
pub use tex::{TexError, TexMath};

/// A formula, as MathML's elements.
#[derive(Debug, Clone, PartialEq)]
pub enum MathNode {
    /// `<mrow>`: children set side by side, spaced by their atom types.
    Row(Row),
    /// `<mi>`: italic when a single character, upright otherwise.
    Identifier(Token),
    /// `<mn>`.
    Number(Token),
    /// `<mo>`.
    Operator(Operator),
    /// `<mtext>`.
    Text(String),
    /// `<mspace>`.
    Space(Space),
    /// `<msub>`, `<msup>` and `<msubsup>`.
    Scripts { base: Box<MathNode>, sub: Option<Box<MathNode>>, sup: Option<Box<MathNode>> },
    /// `<munder>`, `<mover>` and `<munderover>`.
    UnderOver(UnderOver),
    /// `<mfrac>`.
    Fraction(Fraction),
    /// `<msqrt>` without an index, `<mroot>` with one.
    Root { radicand: Box<MathNode>, index: Option<Box<MathNode>> },
    /// `<mtable>`.
    Table(Table),
    /// `<mphantom>`: takes up the space of its content, drawing nothing.
    Phantom(Vec<MathNode>),
    /// `<mpadded>`.
    Padded(Padded),
    /// `<menclose>`.
    Enclose(Enclose),
}

impl MathNode {
    pub fn row(children: Vec<MathNode>) -> Self {
        MathNode::Row(Row { children, paired: false })
    }

    pub fn identifier(text: impl Into<String>) -> Self {
        MathNode::Identifier(Token { text: text.into(), variant: None })
    }

    pub fn number(text: impl Into<String>) -> Self {
        MathNode::Number(Token { text: text.into(), variant: None })
    }

    pub fn operator(text: impl Into<String>) -> Self {
        MathNode::Operator(Operator::new(text))
    }

    pub fn subscript(base: MathNode, sub: MathNode) -> Self {
        MathNode::Scripts { base: Box::new(base), sub: Some(Box::new(sub)), sup: None }
    }

    pub fn superscript(base: MathNode, sup: MathNode) -> Self {
        MathNode::Scripts { base: Box::new(base), sub: None, sup: Some(Box::new(sup)) }
    }

    pub fn sub_superscript(base: MathNode, sub: MathNode, sup: MathNode) -> Self {
        MathNode::Scripts { base: Box::new(base), sub: Some(Box::new(sub)), sup: Some(Box::new(sup)) }
    }

    pub fn fraction(numerator: MathNode, denominator: MathNode) -> Self {
        MathNode::Fraction(Fraction { numerator: Box::new(numerator), denominator: Box::new(denominator), line_thickness: None, bevelled: false })
    }

    pub fn sqrt(radicand: MathNode) -> Self {
        MathNode::Root { radicand: Box::new(radicand), index: None }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub children: Vec<MathNode>,
    /// Opens and closes with a pair of delimiters, which take their spacing
    /// from the inside and stretch to what they enclose.
    pub paired: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub variant: Option<MathVariant>,
}

/// An operator. What is left unset comes from the operator dictionary for
/// its text and form.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Operator {
    pub text: String,
    pub form: Option<Form>,
    pub atom: Option<Atom>,
    pub stretchy: Option<bool>,
    pub largeop: Option<bool>,
    pub movable_limits: Option<bool>,
    pub variant: Option<MathVariant>,
    /// Only honoured on invisible operators (U+2061–U+2064).
    pub lspace: Option<MathLength>,
    pub rspace: Option<MathLength>,
}

impl Operator {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Space {
    pub width: MathLength,
    pub height: MathLength,
    pub depth: MathLength,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnderOver {
    pub base: Box<MathNode>,
    pub under: Option<Box<MathNode>>,
    pub over: Option<Box<MathNode>>,
    /// The over script is an accent: set at the base's size, close to it.
    pub accent: bool,
    pub accent_under: bool,
    /// Scripts go beside rather than under and over outside display style,
    /// as a base with movable limits would make them.
    pub movable_limits: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fraction {
    pub numerator: Box<MathNode>,
    pub denominator: Box<MathNode>,
    /// The font's rule thickness by default.
    pub line_thickness: Option<Dimen>,
    pub bevelled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub rows: Vec<Vec<MathNode>>,
    /// One per column, the last repeated; centered when empty.
    pub column_align: Vec<ColumnAlign>,
    /// 0.6 of the math font size by default.
    pub row_spacing: Option<MathLength>,
    pub column_spacing: Option<MathLength>,
    /// Cells of a table in display style are set in display style too.
    pub display_style: bool,
}

impl Table {
    pub fn new(rows: Vec<Vec<MathNode>>) -> Self {
        Self { rows, column_align: Vec::new(), row_spacing: None, column_spacing: None, display_style: true }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Padded {
    pub content: Box<MathNode>,
    pub width: Option<MathLength>,
    pub height: Option<MathLength>,
    pub depth: Option<MathLength>,
    pub lspace: Option<MathLength>,
    pub voffset: Option<MathLength>,
}

/// The formula as linear text, such as `x^2 + 1`, for readers that can't
/// see it (a tagged PDF's alternate description).
impl fmt::Display for MathNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn group(f: &mut fmt::Formatter<'_>, node: &MathNode, open: &str, close: &str) -> fmt::Result {
            let text = node.to_string();
            let text = text.trim();
            if text.chars().count() <= 1 || text.chars().all(char::is_alphanumeric) {
                f.write_str(text)
            } else {
                write!(f, "{open}{text}{close}")
            }
        }
        match self {
            MathNode::Row(row) => row.children.iter().try_for_each(|c| write!(f, "{c}")),
            MathNode::Identifier(t) | MathNode::Number(t) => f.write_str(&t.text),
            MathNode::Operator(o) => match o.text.as_str() {
                "\u{2061}" | "\u{2062}" | "\u{2063}" | "\u{2064}" => Ok(()),
                t if t.chars().all(|c| "()[]{}|‖⟨⟩⌊⌋⌈⌉,.;!'′".contains(c)) => f.write_str(t),
                t => write!(f, " {t} "),
            },
            MathNode::Text(t) => f.write_str(t),
            MathNode::Space(_) => f.write_str(" "),
            MathNode::Scripts { base, sub, sup } => {
                group(f, base, "(", ")")?;
                if let Some(sub) = sub {
                    f.write_str("_")?;
                    group(f, sub, "{", "}")?;
                }
                if let Some(sup) = sup {
                    f.write_str("^")?;
                    group(f, sup, "{", "}")?;
                }
                Ok(())
            }
            MathNode::UnderOver(u) => {
                group(f, &u.base, "(", ")")?;
                if let Some(under) = &u.under {
                    f.write_str("_")?;
                    group(f, under, "{", "}")?;
                }
                if let Some(over) = &u.over {
                    f.write_str("^")?;
                    group(f, over, "{", "}")?;
                }
                Ok(())
            }
            MathNode::Fraction(fr) => {
                group(f, &fr.numerator, "(", ")")?;
                f.write_str("/")?;
                group(f, &fr.denominator, "(", ")")
            }
            MathNode::Root { radicand, index } => {
                if let Some(index) = index {
                    write!(f, "root({index})")?;
                } else {
                    f.write_str("√")?;
                }
                group(f, radicand, "(", ")")
            }
            MathNode::Table(t) => {
                f.write_str("[")?;
                for (i, row) in t.rows.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    for (j, cell) in row.iter().enumerate() {
                        if j > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{cell}")?;
                    }
                }
                f.write_str("]")
            }
            MathNode::Phantom(_) => Ok(()),
            MathNode::Padded(p) => write!(f, "{}", p.content),
            MathNode::Enclose(e) => write!(f, "{}", e.content),
        }
    }
}

impl Default for MathNode {
    fn default() -> Self {
        MathNode::row(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enclose {
    pub content: Box<MathNode>,
    pub notations: Vec<Notation>,
    pub line_thickness: Option<Dimen>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notation {
    Box,
    UpDiagonalStrike,
    DownDiagonalStrike,
    NorthEastArrow,
}

impl FromStr for Notation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "box" => Notation::Box,
            "updiagonalstrike" => Notation::UpDiagonalStrike,
            "downdiagonalstrike" => Notation::DownDiagonalStrike,
            "northeastarrow" => Notation::NorthEastArrow,
            _ => return Err(format!("unsupported menclose notation {s}")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}

impl FromStr for ColumnAlign {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "left" => ColumnAlign::Left,
            "center" => ColumnAlign::Center,
            "right" => ColumnAlign::Right,
            _ => return Err(format!("invalid column alignment {s}")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Prefix,
    Infix,
    Postfix,
}

impl FromStr for Form {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "prefix" => Form::Prefix,
            "infix" => Form::Infix,
            "postfix" => Form::Postfix,
            _ => return Err(format!("unknown operator form {s}")),
        })
    }
}

/// The TeXbook's atom types, which decide the spacing between neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Atom {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
    Inner,
    Over,
    Under,
    Accent,
    BotAccent,
}

impl FromStr for Atom {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "ord" => Atom::Ord,
            "op" => Atom::Op,
            "bin" => Atom::Bin,
            "rel" => Atom::Rel,
            "open" => Atom::Open,
            "close" => Atom::Close,
            "punct" => Atom::Punct,
            "inner" => Atom::Inner,
            "over" => Atom::Over,
            "under" => Atom::Under,
            "accent" => Atom::Accent,
            "botaccent" => Atom::BotAccent,
            _ => return Err(format!("unknown atom type {s}")),
        })
    }
}

/// MathML's `mathvariant`: letters and digits swapped for the matching
/// Mathematical Alphanumeric Symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathVariant {
    Normal,
    Bold,
    Italic,
    BoldItalic,
    DoubleStruck,
    BoldFraktur,
    Script,
    BoldScript,
    Fraktur,
    SansSerif,
    BoldSansSerif,
    SansSerifItalic,
    SansSerifBoldItalic,
    Monospace,
}

impl FromStr for MathVariant {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "normal" => MathVariant::Normal,
            "bold" => MathVariant::Bold,
            "italic" => MathVariant::Italic,
            "bold-italic" => MathVariant::BoldItalic,
            "double-struck" => MathVariant::DoubleStruck,
            "bold-fraktur" => MathVariant::BoldFraktur,
            "script" => MathVariant::Script,
            "bold-script" => MathVariant::BoldScript,
            "fraktur" => MathVariant::Fraktur,
            "sans-serif" => MathVariant::SansSerif,
            "bold-sans-serif" => MathVariant::BoldSansSerif,
            "sans-serif-italic" => MathVariant::SansSerifItalic,
            "sans-serif-bold-italic" => MathVariant::SansSerifBoldItalic,
            "monospace" => MathVariant::Monospace,
            _ => return Err(format!("unsupported mathvariant {s}")),
        })
    }
}

/// Units math lengths can be given in. `mu` is 1/18 of the math font
/// size; `em`, `en` and `ex` are the math font's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MathUnit {
    #[default]
    Pt,
    Px,
    Mm,
    Cm,
    In,
    Em,
    En,
    Ex,
    Mu,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dimen {
    pub value: f64,
    pub unit: MathUnit,
}

impl Dimen {
    pub const ZERO: Dimen = Dimen { value: 0.0, unit: MathUnit::Pt };

    pub fn new(value: f64, unit: MathUnit) -> Self {
        Self { value, unit }
    }

    pub fn pt(value: f64) -> Self {
        Self::new(value, MathUnit::Pt)
    }

    pub fn mu(value: f64) -> Self {
        Self::new(value, MathUnit::Mu)
    }
}

impl FromStr for Dimen {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
        let (number, unit) = s.split_at(split);
        let value: f64 = number.trim().parse().map_err(|_| format!("bad length {s}"))?;
        let unit = match unit.trim() {
            "" | "pt" => MathUnit::Pt,
            "px" => MathUnit::Px,
            "mm" => MathUnit::Mm,
            "cm" => MathUnit::Cm,
            "in" => MathUnit::In,
            "em" => MathUnit::Em,
            "en" => MathUnit::En,
            "ex" => MathUnit::Ex,
            "mu" => MathUnit::Mu,
            u => return Err(format!("unsupported unit {u} in math")),
        };
        Ok(Dimen { value, unit })
    }
}

/// A length with stretch and shrink, as math spaces have.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MathLength {
    pub natural: Dimen,
    pub stretch: Dimen,
    pub shrink: Dimen,
}

impl MathLength {
    pub fn new(natural: Dimen) -> Self {
        Self { natural, ..Default::default() }
    }

    /// TeX's thin space, 3mu.
    pub fn thin() -> Self {
        Self::new(Dimen::mu(3.0))
    }

    /// TeX's medium space, 4mu plus 2mu minus 4mu.
    pub fn med() -> Self {
        Self { natural: Dimen::mu(4.0), stretch: Dimen::mu(2.0), shrink: Dimen::mu(4.0) }
    }

    /// TeX's thick space, 5mu plus 5mu.
    pub fn thick() -> Self {
        Self { natural: Dimen::mu(5.0), stretch: Dimen::mu(5.0), shrink: Dimen::ZERO }
    }

    fn negated(self) -> Self {
        let neg = |d: Dimen| Dimen { value: -d.value, ..d };
        Self { natural: neg(self.natural), stretch: neg(self.stretch), shrink: neg(self.shrink) }
    }
}

/// Parses `thin`, `med`, `thick` (or their negations, `-thin`…) and
/// lengths such as `4mu plus 2mu minus 4mu`.
impl FromStr for MathLength {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (negative, name) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let named = match name {
            "thin" => Some(Self::thin()),
            "med" => Some(Self::med()),
            "thick" => Some(Self::thick()),
            _ => None,
        };
        if let Some(l) = named {
            return Ok(if negative { l.negated() } else { l });
        }
        let (natural, rest) = s.split_once(" plus ").map_or((s, None), |(n, r)| (n, Some(r)));
        let (natural, shrink) = natural.split_once(" minus ").map_or((natural, None), |(n, m)| (n, Some(m)));
        let (stretch, shrink) = match rest.map(|r| r.split_once(" minus ").map_or((r, None), |(p, m)| (p, Some(m)))) {
            Some((p, m)) => (Some(p), m.or(shrink)),
            None => (None, shrink),
        };
        let dimen = |d: Option<&str>| d.map_or(Ok(Dimen::ZERO), str::parse);
        Ok(Self { natural: natural.parse()?, stretch: dimen(stretch)?, shrink: dimen(shrink)? })
    }
}

impl fmt::Display for MathUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MathUnit::Pt => "pt",
            MathUnit::Px => "px",
            MathUnit::Mm => "mm",
            MathUnit::Cm => "cm",
            MathUnit::In => "in",
            MathUnit::Em => "em",
            MathUnit::En => "en",
            MathUnit::Ex => "ex",
            MathUnit::Mu => "mu",
        })
    }
}

/// How a formula sits: in the line, or on its own line, centered.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum MathMode {
    #[default]
    Text,
    Display {
        /// Set flush right on the formula's line, as "(n)" by default.
        number: Option<String>,
    },
}

/// The math font and display spacing.
// SILE: `math.*` settings.
#[derive(Debug, Clone, PartialEq)]
pub struct MathSettings {
    pub family: String,
    pub filename: Option<String>,
    pub weight: u16,
    /// The OpenType feature for script-size alternates (`ssty`), applied
    /// as 1 in scripts and 2 in scripts of scripts.
    pub script_feature: Option<String>,
    /// The math font size; by default it matches the text font's x-height.
    pub size: Option<f64>,
    /// Space above and below a displayed formula, with `ex` and `em` of
    /// the text font.
    pub display_skip: MathLength,
    pub pre_display_penalty: i32,
    pub post_display_penalty: i32,
}

impl Default for MathSettings {
    fn default() -> Self {
        Self {
            family: "Libertinus Math".into(),
            filename: None,
            weight: 400,
            script_feature: Some("ssty".into()),
            size: None,
            display_skip: MathLength { natural: Dimen::new(2.0, MathUnit::Ex), stretch: Dimen::pt(1.0), shrink: Dimen::ZERO },
            pre_display_penalty: 10_000,
            post_display_penalty: -50,
        }
    }
}

struct BuilderFont<'a> {
    doc: &'a mut Typesetter,
    base: FontSpec,
}

impl layout::MathFont for BuilderFont<'_> {
    fn shape(&mut self, text: &str, size: f64, features: &str) -> (String, Vec<GlyphItem>) {
        let mut spec = FontSpec { size, ..self.base.clone() };
        if !features.is_empty() {
            spec.features = features.to_string();
        }
        let Ok(key) = self.doc.register_font_spec(spec) else { return (String::new(), Vec::new()) };
        let Some((spec, face)) = self.doc.registered_face(&key) else { return (key, Vec::new()) };
        let glyphs = self.doc.shaper().shape(text, face, spec);
        (key, glyphs)
    }

    fn glyph_dimensions(&self, gid: u16, size: f64) -> layout::GlyphDimensions {
        let Some((_, face)) = self.doc.registered_face(&self.base.cache_key()) else { return Default::default() };
        let s = size / face.units_per_em() as f64;
        let advance = face.advance_width(gid).map_or(0.0, |a| a as f64 * s);
        match face.glyph_bounding_box(gid) {
            Some(b) => layout::GlyphDimensions {
                width: (b.x_max as f64 - b.x_min as f64) * s,
                height: b.y_max as f64 * s,
                depth: -(b.y_min as f64) * s,
                advance,
            },
            None => layout::GlyphDimensions { advance, ..Default::default() },
        }
    }

    fn measure_char(&mut self, c: char) -> (f64, f64, f64) {
        let Some((spec, face)) = self.doc.registered_face(&self.base.cache_key()) else { return (0.0, 0.0, 0.0) };
        let (m, _) = self.doc.shaper().measure_char(c, face, spec);
        (m.width, m.height, m.depth)
    }
}

fn ex_height(face: &FontFace, size: f64) -> f64 {
    face.x_height().map_or(size / 2.0, |h| face.scale(h, size))
}

impl Typesetter {
    /// The math font for the current settings: sized to match the text
    /// font's x-height unless `MathSettings::size` is set.
    fn math_font(&mut self) -> Result<(FontSpec, Arc<FontFace>), BuilderError> {
        let m = self.math_settings().clone();
        let current = self.font_spec().cloned().unwrap_or_default();
        let mut spec = FontSpec {
            family: if m.filename.is_some() { current.family.clone() } else { Some(m.family.clone()) },
            filename: m.filename.clone(),
            weight: FontWeight(m.weight),
            style: FontStyle::Normal,
            script: "math".into(),
            direction: Direction::LTR,
            ..current.clone()
        };
        let face = |doc: &mut Self, spec: &FontSpec| -> Result<Arc<FontFace>, BuilderError> {
            let key = doc.register_font_spec(spec.clone())?;
            Ok(Arc::clone(doc.registered_face(&key).expect("registered").1))
        };
        let mut math_face = face(self, &spec)?;
        let size = match m.size {
            Some(size) => size,
            None => current.size * self.x_height() / ex_height(&math_face, current.size),
        };
        if size != spec.size {
            spec.size = size;
            math_face = face(self, &spec)?;
        }
        Ok((spec, math_face))
    }

    fn math_table(&mut self, spec: &FontSpec, face: &FontFace) -> Result<Arc<MathTable>, BuilderError> {
        let key = format!("{:?};{};{:?}", spec.family, spec.weight.0, spec.filename);
        if let Some(t) = self.math_tables.get(&key) {
            return Ok(Arc::clone(t));
        }
        let table = MathTable::read(face).map(Arc::new).ok_or_else(|| {
            BuilderError::Layout(format!("You must use a math font for math rendering: {} has no MATH table", spec.family.as_deref().unwrap_or("the font")))
        })?;
        self.math_tables.insert(key, Arc::clone(&table));
        Ok(table)
    }
}

pub(crate) fn add_math<A: Arranger + ?Sized>(a: &mut A, formula: &MathNode, mode: MathMode) -> Result<(), BuilderError> {
    let own = a.current_role() != Some(Role::Formula);
    if own {
        a.begin_structure(Role::Formula).set_alt_text(formula.to_string());
    }
    let saved = a.settings().clone();
    let result = add_math_in(a, formula, mode);
    a.restore_settings(saved);
    if own {
        a.end_structure();
    }
    result?;
    Ok(())
}

fn add_math_in<A: Arranger + ?Sized>(a: &mut A, formula: &MathNode, mode: MathMode) -> Result<(), BuilderError> {
    let (text_size, text_x_height) = (a.font_spec().map_or(10.0, |f| f.size), a.x_height());
    let (spec, face) = a.math_font()?;
    a.set_font_spec(spec.clone())?;
    let table = a.math_table(&spec, &face)?;
    let x_height = a.x_height();
    let settings = a.math_settings().clone();
    let display = matches!(mode, MathMode::Display { .. });
    let laid = {
        let mut font = BuilderFont { doc: &mut *a, base: spec.clone() };
        let mut ctx = layout::Context {
            font: &mut font,
            table: &table,
            size: spec.size,
            x_height,
            script_feature: settings.script_feature.as_deref(),
        };
        layout::lay_out(formula, display, &mut ctx)
    };
    let pt = Measurement::pt;
    let mut hbox = HBox::new(Length::new(pt(laid.width.natural), pt(laid.width.stretch), pt(laid.width.shrink)), Length::pt(laid.height), Length::pt(laid.depth));
    hbox.ink = Some(Ink::Math(MathInk(Arc::new(laid.items))));
    let MathMode::Display { number } = mode else {
        a.add_box(hbox);
        return Ok(());
    };
    let resolve = |d: Dimen| match d.unit {
        MathUnit::Ex => d.value * text_x_height,
        MathUnit::Em => d.value * text_size,
        MathUnit::En => d.value * text_size / 2.0,
        MathUnit::Mu => d.value * spec.size / 18.0,
        MathUnit::Px => d.value * 0.75,
        MathUnit::Mm => d.value * 72.0 / 25.4,
        MathUnit::Cm => d.value * 72.0 / 2.54,
        MathUnit::In => d.value * 72.0,
        MathUnit::Pt => d.value,
    };
    let s = settings.display_skip;
    let skip = Length::new(pt(resolve(s.natural)), pt(resolve(s.stretch)), pt(resolve(s.shrink)));
    a.add_vertical_penalty(settings.pre_display_penalty)?;
    a.add_explicit_vskip(skip)?;
    a.add_vertical_penalty(settings.pre_display_penalty)?;
    let display_settings = a.settings().clone();
    let skips = a.line_skips();
    a.set_paragraph_indent(0.0);
    a.set_current_indent(Some(0.0));
    a.set_line_skips(LineSkips {
        left: Length::new(skips.left.length, pt(crate::node::INFINITY), pt(0.0)),
        right: Length::from(skips.right.length),
        par_fill: Length::zero(),
    });
    let mut space = *a.space_settings();
    space.skip = Some(Length::pt(a.space_width()));
    a.set_space_settings(space);
    a.add_box(hbox);
    a.add_hfill();
    if let Some(number) = number {
        a.add_text("(").add_text(number).add_text(")");
    }
    a.add_vertical_penalty(settings.post_display_penalty)?;
    a.restore_settings(display_settings);
    a.add_explicit_vskip(skip)?;
    a.add_vertical_penalty(settings.post_display_penalty)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_lengths_parse_named_spaces_and_glue() {
        assert_eq!("-thin".parse::<MathLength>().unwrap().natural, Dimen::mu(-3.0));
        let l: MathLength = "4mu plus 2mu minus 4mu".parse().unwrap();
        assert_eq!(l, MathLength::med());
        assert_eq!("2pt".parse::<MathLength>().unwrap().natural, Dimen::pt(2.0));
        assert_eq!("0".parse::<MathLength>().unwrap().natural, Dimen::ZERO);
    }

    #[test]
    fn formulas_read_as_linear_text() {
        let mut tex = TexMath::new();
        let formula = tex.parse(r"x^2 + \frac{a+b}{c} = \sqrt{y_1}").unwrap();
        assert_eq!(formula.to_string(), "x^2 + (a + b)/c = √(y_1)");
        assert_eq!(tex.parse(r"\int_0^1 f").unwrap().to_string(), "∫_0^1f");
    }
}
