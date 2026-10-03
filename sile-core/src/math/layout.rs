//! Laying out a formula (SILE's `base-elements.lua`): the tree becomes
//! boxes, styled top-down (display, text, script and scriptscript styles,
//! cramped or not), measured bottom-up from the font's MATH table, then
//! flattened into positioned glyphs, rules and drawings.

use std::ops::{Add, Div, Mul, Neg, Sub};

use super::operators;
use super::table::{Constants, MathTable};
use super::variants;
use super::{Atom, ColumnAlign, Dimen, Form, MathLength, MathNode, MathUnit, MathVariant, Notation};
use crate::node::{GlyphData, NNode};
use crate::shaper::GlyphItem;
use crate::svg_image::SvgOp;

/// A horizontal length with stretch and shrink, which spaces inside a
/// formula lend to the line it is set in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Glue {
    pub natural: f64,
    pub stretch: f64,
    pub shrink: f64,
}

impl Glue {
    pub fn new(natural: f64) -> Self {
        Self { natural, ..Default::default() }
    }

    /// The length on a line set with glue ratio `ratio` (SILE's
    /// `scaleWidth`).
    pub fn at(&self, ratio: f64) -> f64 {
        if ratio < 0.0 && self.shrink > 0.0 {
            self.natural + self.shrink * ratio
        } else if ratio > 0.0 && self.stretch > 0.0 {
            self.natural + self.stretch * ratio
        } else {
            self.natural
        }
    }

    /// The longer of the two, by natural length.
    fn max(self, other: Glue) -> Glue {
        if other.natural > self.natural { other } else { self }
    }
}

impl Add for Glue {
    type Output = Glue;
    fn add(self, o: Glue) -> Glue {
        Glue { natural: self.natural + o.natural, stretch: self.stretch + o.stretch, shrink: self.shrink + o.shrink }
    }
}

impl Sub for Glue {
    type Output = Glue;
    fn sub(self, o: Glue) -> Glue {
        self + -o
    }
}

impl Add<f64> for Glue {
    type Output = Glue;
    fn add(self, o: f64) -> Glue {
        Glue { natural: self.natural + o, ..self }
    }
}

impl Sub<f64> for Glue {
    type Output = Glue;
    fn sub(self, o: f64) -> Glue {
        self + -o
    }
}

impl Neg for Glue {
    type Output = Glue;
    fn neg(self) -> Glue {
        self * -1.0
    }
}

impl Mul<f64> for Glue {
    type Output = Glue;
    fn mul(self, k: f64) -> Glue {
        Glue { natural: self.natural * k, stretch: self.stretch * k, shrink: self.shrink * k }
    }
}

impl Div<f64> for Glue {
    type Output = Glue;
    fn div(self, k: f64) -> Glue {
        self * (1.0 / k)
    }
}

/// Something a formula draws, placed from its origin: the pen position on
/// the baseline. `x` is stretched with the line.
#[derive(Debug, Clone)]
pub enum MathItem {
    Glyphs {
        x: Glue,
        /// The baseline.
        y: f64,
        nnode: Box<NNode>,
        /// A stretchy glyph without a large enough variant is scaled by
        /// these ratios about the box's baseline.
        scale: Option<GlyphScale>,
    },
    Rule {
        x: Glue,
        /// The top edge.
        y: f64,
        width: Glue,
        height: f64,
    },
    Figure {
        x: Glue,
        baseline: f64,
        /// How far above the baseline the drawing's origin is.
        height: f64,
        figure: Figure,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphScale {
    pub x: f64,
    pub y: f64,
    pub origin_y: f64,
}

/// Strokes drawn as PDF paths, in points from the drawing's origin, y
/// growing downwards.
#[derive(Debug, Clone, PartialEq)]
pub enum Figure {
    Radical { offset: Glue, symbol_width: Glue, height: f64, depth: f64, short_height: f64, symbol_depth: f64, extra_ascender: f64, thickness: f64 },
    Bevel { width: Glue, height: f64, depth: f64, thickness: f64 },
    Enclose { width: Glue, height: f64, depth: f64, thickness: f64, offset: f64, arrow: f64, notations: Vec<Notation> },
}

impl Figure {
    /// The drawing and its width on a line with glue ratio `ratio`.
    pub fn ops(&self, ratio: f64) -> (Vec<SvgOp>, f64) {
        let p = |v: f64| v as f32;
        match self {
            Figure::Radical { offset, symbol_width, height, depth, short_height, symbol_depth, extra_ascender, thickness } => {
                let (s0, sw) = (offset.at(ratio), symbol_width.at(ratio));
                let dsh = height - short_height;
                let ops = vec![
                    SvgOp::LineWidth(p(*thickness)),
                    SvgOp::LineJoin(1),
                    SvgOp::Move(p(sw + s0), p(*extra_ascender)),
                    SvgOp::Line(p(s0 + sw * 0.9), p(*extra_ascender)),
                    SvgOp::Line(p(s0 + sw * 0.4), p(height + depth + symbol_depth)),
                    SvgOp::Line(p(s0 + sw * 0.2), p(dsh)),
                    SvgOp::Line(p(s0 + sw * 0.1), p(dsh + 0.5)),
                    SvgOp::Stroke,
                ];
                (ops, sw)
            }
            Figure::Bevel { width, height, depth, thickness } => {
                let w = width.at(ratio);
                let rd = thickness / 2.0;
                let ops = vec![
                    SvgOp::LineWidth(p(*thickness)),
                    SvgOp::LineCap(1),
                    SvgOp::Move(0.0, p(depth + height - rd)),
                    SvgOp::Line(p(w), p(rd)),
                    SvgOp::Stroke,
                ];
                (ops, w)
            }
            Figure::Enclose { width, height, depth, thickness, offset, arrow, notations } => {
                let (w, h, o) = (width.at(ratio), height + depth, *offset);
                let mut ops = Vec::new();
                let line = |ops: &mut Vec<SvgOp>, from: (f64, f64), to: (f64, f64)| {
                    ops.extend([SvgOp::Move(p(from.0), p(from.1)), SvgOp::Line(p(to.0), p(to.1)), SvgOp::Stroke]);
                };
                for n in notations {
                    ops.push(SvgOp::LineWidth(p(*thickness)));
                    match n {
                        Notation::Box => {
                            ops.push(SvgOp::LineJoin(1));
                            ops.extend([
                                SvgOp::Move(0.0, 0.0),
                                SvgOp::Line(p(w), 0.0),
                                SvgOp::Line(p(w), p(h)),
                                SvgOp::Line(0.0, p(h)),
                                SvgOp::Close,
                                SvgOp::Stroke,
                            ]);
                        }
                        Notation::UpDiagonalStrike => {
                            ops.push(SvgOp::LineCap(1));
                            line(&mut ops, (o, h - o), (w - o, o));
                        }
                        Notation::DownDiagonalStrike => {
                            ops.push(SvgOp::LineCap(1));
                            line(&mut ops, (o, o), (w - o, h - o));
                        }
                        Notation::NorthEastArrow => {
                            ops.extend([SvgOp::LineJoin(1), SvgOp::LineCap(1)]);
                            let angle = (h - 2.0 * o).atan2(w - 2.0 * o);
                            let spread = std::f64::consts::PI / 7.0;
                            line(&mut ops, (o, h - o), (w - o, o));
                            for a in [angle - spread, angle + spread] {
                                line(&mut ops, (w - o - arrow * a.cos(), o + arrow * a.sin()), (w - o, o));
                            }
                        }
                    }
                }
                (ops, w)
            }
        }
    }
}

/// Shaping in the math font, provided by the document builder.
pub(crate) trait MathFont {
    /// Shape `text` at `size` with OpenType `features`, returning the key
    /// of the font used and the glyphs.
    fn shape(&mut self, text: &str, size: f64, features: &str) -> (String, Vec<GlyphItem>);
    /// Ink extents and advance of a glyph at `size`.
    fn glyph_dimensions(&self, gid: u16, size: f64) -> GlyphDimensions;
    /// Width, height and depth of `c` at the base size.
    fn measure_char(&mut self, c: char) -> (f64, f64, f64);
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GlyphDimensions {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub advance: f64,
}

pub(crate) struct Context<'a> {
    pub font: &'a mut dyn MathFont,
    pub table: &'a MathTable,
    /// The math font size.
    pub size: f64,
    pub x_height: f64,
    pub script_feature: Option<&'a str>,
}

type Mode = u8;
const DISPLAY: Mode = 0;
const TEXT: Mode = 2;
const SCRIPT: Mode = 4;
const SCRIPT_SCRIPT: Mode = 6;

fn is_display(m: Mode) -> bool {
    m <= 1
}

fn is_cramped(m: Mode) -> bool {
    m % 2 == 1
}

fn is_script(m: Mode) -> bool {
    m == SCRIPT || m == SCRIPT + 1
}

fn is_script_script(m: Mode) -> bool {
    m >= SCRIPT_SCRIPT
}

fn superscript_mode(m: Mode) -> Mode {
    match m {
        0 | 2 => SCRIPT,
        1 | 3 => SCRIPT + 1,
        4 | 6 => SCRIPT_SCRIPT,
        _ => SCRIPT_SCRIPT + 1,
    }
}

fn subscript_mode(m: Mode) -> Mode {
    if m <= 3 { SCRIPT + 1 } else { SCRIPT_SCRIPT + 1 }
}

fn numerator_mode(m: Mode) -> Mode {
    match m {
        0 => TEXT,
        1 => TEXT + 1,
        2 => SCRIPT,
        3 => SCRIPT + 1,
        4 | 6 => SCRIPT_SCRIPT,
        _ => SCRIPT_SCRIPT + 1,
    }
}

fn denominator_mode(m: Mode) -> Mode {
    match m {
        0 | 1 => TEXT + 1,
        2 | 3 => SCRIPT + 1,
        _ => SCRIPT_SCRIPT + 1,
    }
}

fn accent_mode(m: Mode) -> Mode {
    match m {
        0 => TEXT,
        1 => TEXT + 1,
        m => m,
    }
}

fn radicand_mode(m: Mode) -> Mode {
    if is_cramped(m) { m } else { m + 1 }
}

fn degree_mode(m: Mode) -> Mode {
    match m {
        0 | 2 | 4 | 6 => SCRIPT_SCRIPT,
        _ => SCRIPT_SCRIPT + 1,
    }
}

impl Context<'_> {
    fn constants(&self) -> Constants {
        self.table.constants(self.size)
    }

    fn scale_down(&self, mode: Mode) -> f64 {
        let c = self.table.constants(self.size);
        if is_script(mode) {
            c.script_scale_down
        } else if is_script_script(mode) {
            c.script_script_scale_down
        } else {
            1.0
        }
    }

    fn resolve(&self, d: Dimen) -> f64 {
        d.value
            * match d.unit {
                MathUnit::Pt => 1.0,
                MathUnit::Px => 0.75,
                MathUnit::Mm => 72.0 / 25.4,
                MathUnit::Cm => 72.0 / 2.54,
                MathUnit::In => 72.0,
                MathUnit::Em => self.size,
                MathUnit::En => self.size / 2.0,
                MathUnit::Ex => self.x_height,
                MathUnit::Mu => self.size / 18.0,
            }
    }

    fn glue(&self, l: &MathLength) -> Glue {
        Glue { natural: self.resolve(l.natural), stretch: self.resolve(l.stretch), shrink: self.resolve(l.shrink) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TextKind {
    Number,
    Identifier,
    Operator,
    String,
}

#[derive(Debug, Clone)]
struct Glyph {
    gid: u16,
    width: f64,
    advance: f64,
    height: f64,
    depth: f64,
    x_offset: f64,
    y_offset: f64,
    text: std::sync::Arc<str>,
}

#[derive(Debug, Clone)]
struct TextBox {
    kind: TextKind,
    text: String,
    stretchy: bool,
    font_key: String,
    size: f64,
    glyphs: Vec<Glyph>,
    /// The font's height and depth of a display operator recentred on the
    /// axis, which drawing compensates for.
    font_extents: Option<(f64, f64)>,
    vert_scaling: Option<(f64, f64)>,
    horiz_scaling: Option<f64>,
}

#[derive(Debug, Clone)]
struct ScriptsBox {
    base: MBox,
    sub: Option<MBox>,
    sup: Option<MBox>,
    /// Set for `<munder>`, `<mover>` and `<munderover>`, with whether the
    /// over and under scripts are accents.
    under_over: Option<(bool, bool)>,
    /// An under/over laid out beside its base, its limits being movable.
    as_scripts: bool,
}

#[derive(Debug, Clone)]
struct FractionBox {
    numerator: MBox,
    denominator: MBox,
    line_thickness: Option<Dimen>,
    bevelled: bool,
    padding: f64,
    axis: f64,
    rule: f64,
}

#[derive(Debug, Clone)]
struct TableBox {
    rows: Vec<MBox>,
    column_align: Vec<ColumnAlign>,
    row_spacing: Option<MathLength>,
    column_spacing: Option<MathLength>,
    display_style: bool,
}

#[derive(Debug, Clone)]
struct SqrtBox {
    radicand: MBox,
    degree: Option<MBox>,
    thickness: f64,
    vertical_gap: f64,
    extra_ascender: f64,
    symbol_width: Glue,
    symbol_depth: f64,
    short_height: f64,
    offset: Glue,
}

#[derive(Debug, Clone)]
struct PaddedBox {
    content: MBox,
    width: Option<MathLength>,
    height: Option<MathLength>,
    depth: Option<MathLength>,
    lspace: Option<MathLength>,
    voffset: Option<MathLength>,
}

#[derive(Debug, Clone)]
struct EncloseBox {
    content: MBox,
    notations: Vec<Notation>,
    line_thickness: Option<Dimen>,
    rule: f64,
    offset: f64,
    arrow: f64,
}

#[derive(Debug, Clone)]
enum Kind {
    /// A horizontal list; `row` for table rows, which get no spacing.
    Stack { children: Vec<MBox>, paired: bool, phantom: bool, row: bool },
    Scripts(Box<ScriptsBox>),
    Space(super::Space),
    Text(Box<TextBox>),
    Fraction(Box<FractionBox>),
    Table(Box<TableBox>),
    Sqrt(Box<SqrtBox>),
    Padded(Box<PaddedBox>),
    Enclose(Box<EncloseBox>),
}

#[derive(Debug, Clone)]
struct MBox {
    kind: Kind,
    mode: Mode,
    atom: Atom,
    width: Glue,
    height: f64,
    depth: f64,
    rel_x: Glue,
    rel_y: f64,
    width_for_subscript: Option<Glue>,
    largeop: bool,
    movable_limits: bool,
}

impl MBox {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            mode: DISPLAY,
            atom: Atom::Ord,
            width: Glue::default(),
            height: 0.0,
            depth: 0.0,
            rel_x: Glue::default(),
            rel_y: 0.0,
            width_for_subscript: None,
            largeop: false,
            movable_limits: false,
        }
    }

    fn stack(children: Vec<MBox>) -> Self {
        Self::new(Kind::Stack { children, paired: false, phantom: false, row: false })
    }

    fn space(width: MathLength) -> Self {
        Self::new(Kind::Space(super::Space { width, ..Default::default() }))
    }

    fn is_terminal(&self) -> bool {
        matches!(self.kind, Kind::Space(_) | Kind::Text(_))
    }

    fn children_mut(&mut self) -> Vec<&mut MBox> {
        match &mut self.kind {
            Kind::Stack { children, .. } => children.iter_mut().collect(),
            Kind::Scripts(s) => {
                let s = &mut **s;
                if s.under_over.is_some() {
                    s.sup.iter_mut().chain(std::iter::once(&mut s.base)).chain(s.sub.iter_mut()).collect()
                } else {
                    std::iter::once(&mut s.base).chain(s.sub.iter_mut()).chain(s.sup.iter_mut()).collect()
                }
            }
            Kind::Space(_) | Kind::Text(_) => Vec::new(),
            Kind::Fraction(f) => vec![&mut f.numerator, &mut f.denominator],
            Kind::Table(t) => t.rows.iter_mut().collect(),
            Kind::Sqrt(s) => {
                let s = &mut **s;
                s.degree.iter_mut().chain(std::iter::once(&mut s.radicand)).collect()
            }
            Kind::Padded(p) => vec![&mut p.content],
            Kind::Enclose(e) => vec![&mut e.content],
        }
    }

    fn has_children(&self) -> bool {
        match &self.kind {
            Kind::Stack { children, .. } => !children.is_empty(),
            Kind::Space(_) | Kind::Text(_) => false,
            _ => true,
        }
    }

    fn stretchy_operator(&mut self) -> Option<&mut TextBox> {
        match &mut self.kind {
            Kind::Text(t) if t.kind == TextKind::Operator && t.stretchy => Some(t),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// From the tree to boxes
// ---------------------------------------------------------------------------

fn is_space_like(node: &MathNode) -> bool {
    match node {
        MathNode::Text(_) | MathNode::Space(_) => true,
        MathNode::Row(r) => r.children.iter().all(is_space_like),
        MathNode::Phantom(c) => c.iter().all(is_space_like),
        MathNode::Padded(p) => is_space_like(&p.content),
        _ => false,
    }
}

/// The forms operators take from their position (MathML Core's
/// "embellished operator" rules, as SILE applies them).
fn embellished_forms(children: &[&MathNode], grouping: bool, scripted: bool) -> Vec<Option<Form>> {
    let mut forms = vec![None; children.len()];
    let (mut last_child, mut last_mo) = (None, None);
    for (i, n) in children.iter().enumerate() {
        if let MathNode::Operator(op) = n {
            last_mo = Some(i);
            forms[i] = op.form.or(if grouping && last_child.is_none() {
                Some(Form::Prefix)
            } else if scripted && last_child.is_some() {
                Some(Form::Postfix)
            } else {
                None
            });
        }
        if !is_space_like(n) {
            last_child = Some(i);
        }
    }
    if let Some(i) = last_mo
        && forms[i].is_none()
        && grouping
        && last_child == Some(i)
    {
        forms[i] = Some(Form::Postfix);
    }
    forms
}

fn build_children(children: &[&MathNode], grouping: bool, scripted: bool) -> Vec<MBox> {
    let forms = embellished_forms(children, grouping, scripted);
    children.iter().zip(forms).filter_map(|(n, f)| build(n, f)).collect()
}

fn build_one(node: &MathNode, form: Option<Form>) -> MBox {
    build(node, form).unwrap_or_else(|| MBox::stack(Vec::new()))
}

fn unwrap_single_row(b: MBox) -> MBox {
    match b.kind {
        Kind::Stack { mut children, .. } if children.len() == 1 => unwrap_single_row(children.pop().unwrap()),
        _ => b,
    }
}

fn text_box(kind: TextKind, text: String, variant: MathVariant) -> MBox {
    let mut text = if variant == MathVariant::Normal { text } else { variants::convert(&text, variant) };
    if kind == TextKind::Operator && text == "-" {
        text = "−".into();
    }
    MBox::new(Kind::Text(Box::new(TextBox {
        kind,
        text,
        stretchy: false,
        font_key: String::new(),
        size: 0.0,
        glyphs: Vec::new(),
        font_extents: None,
        vert_scaling: None,
        horiz_scaling: None,
    })))
}

fn build(node: &MathNode, form: Option<Form>) -> Option<MBox> {
    Some(match node {
        MathNode::Row(row) => {
            let children: Vec<&MathNode> = row.children.iter().collect();
            let mut b = MBox::stack(build_children(&children, true, false));
            if let Kind::Stack { paired, .. } = &mut b.kind {
                *paired = row.paired;
            }
            b
        }
        MathNode::Phantom(children) => {
            let children: Vec<&MathNode> = children.iter().collect();
            MBox::new(Kind::Stack { children: build_children(&children, true, false), paired: false, phantom: true, row: false })
        }
        MathNode::Identifier(t) => {
            let variant = t.variant.unwrap_or(if t.text.chars().count() == 1 { MathVariant::Italic } else { MathVariant::Normal });
            text_box(TextKind::Identifier, t.text.clone(), variant)
        }
        MathNode::Number(t) => {
            let text = match t.text.strip_prefix('-') {
                Some(rest) => format!("−{rest}"),
                None => t.text.clone(),
            };
            text_box(TextKind::Number, text, t.variant.unwrap_or(MathVariant::Normal))
        }
        MathNode::Text(s) => {
            let mut text = String::new();
            for c in s.chars() {
                if !c.is_whitespace() {
                    text.push(c);
                } else if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            text_box(TextKind::String, text, MathVariant::Normal)
        }
        MathNode::Operator(op) => return operator_box(op, op.form.or(form).unwrap_or(Form::Infix)),
        MathNode::Space(s) => MBox::new(Kind::Space(*s)),
        MathNode::Scripts { base, sub, sup } => {
            let mut children = vec![&**base];
            children.extend(sub.as_deref());
            children.extend(sup.as_deref());
            let mut built = build_children(&children, false, true).into_iter();
            let base = built.next().unwrap_or_else(|| MBox::stack(Vec::new()));
            let sub = sub.as_ref().and_then(|_| built.next());
            let sup = sup.as_ref().and_then(|_| built.next());
            let atom = base.atom;
            let mut b = MBox::new(Kind::Scripts(Box::new(ScriptsBox { base, sub, sup, under_over: None, as_scripts: false })));
            b.atom = atom;
            b
        }
        MathNode::UnderOver(u) => {
            let mut children = vec![&*u.base];
            children.extend(u.under.as_deref());
            children.extend(u.over.as_deref());
            let mut built = build_children(&children, false, true).into_iter();
            let base = unwrap_single_row(built.next().unwrap_or_else(|| MBox::stack(Vec::new())));
            let not_empty = |b: &MBox| b.is_terminal() || b.has_children();
            let sub = u.under.as_ref().and_then(|_| built.next()).filter(not_empty);
            let sup = u.over.as_ref().and_then(|_| built.next()).filter(not_empty);
            let atom = base.atom;
            let mut b = MBox::new(Kind::Scripts(Box::new(ScriptsBox {
                base,
                sub,
                sup,
                under_over: Some((u.accent, u.accent_under)),
                as_scripts: false,
            })));
            b.atom = atom;
            b.movable_limits = u.movable_limits;
            b
        }
        MathNode::Fraction(f) => MBox::new(Kind::Fraction(Box::new(FractionBox {
            numerator: build_one(&f.numerator, None),
            denominator: build_one(&f.denominator, None),
            line_thickness: f.line_thickness,
            bevelled: f.bevelled,
            padding: 0.0,
            axis: 0.0,
            rule: 0.0,
        }))),
        MathNode::Root { radicand, index } => {
            let radicand = match index {
                Some(_) => build_one(radicand, None),
                None => match &**radicand {
                    MathNode::Row(_) => build_one(radicand, None),
                    other => MBox::stack(build_children(&[other], true, false)),
                },
            };
            MBox::new(Kind::Sqrt(Box::new(SqrtBox {
                radicand,
                degree: index.as_deref().map(|i| build_one(i, None)),
                thickness: 0.0,
                vertical_gap: 0.0,
                extra_ascender: 0.0,
                symbol_width: Glue::default(),
                symbol_depth: 0.0,
                short_height: 0.0,
                offset: Glue::default(),
            })))
        }
        MathNode::Table(t) => {
            let columns = t.rows.iter().map(Vec::len).max().unwrap_or(0);
            let rows = t
                .rows
                .iter()
                .map(|cells| {
                    let mut cells: Vec<MBox> = cells
                        .iter()
                        .map(|c| match c {
                            MathNode::Row(_) => build_one(c, None),
                            other => MBox::stack(build_children(&[other], true, false)),
                        })
                        .collect();
                    cells.resize_with(columns, || MBox::stack(Vec::new()));
                    MBox::new(Kind::Stack { children: cells, paired: false, phantom: false, row: true })
                })
                .collect();
            let mut column_align = t.column_align.clone();
            if column_align.is_empty() {
                column_align = vec![ColumnAlign::Center; columns];
            } else {
                let last = *column_align.last().unwrap();
                column_align.resize(columns, last);
            }
            MBox::new(Kind::Table(Box::new(TableBox {
                rows,
                column_align,
                row_spacing: t.row_spacing,
                column_spacing: t.column_spacing,
                display_style: t.display_style,
            })))
        }
        MathNode::Padded(p) => MBox::new(Kind::Padded(Box::new(PaddedBox {
            content: row_of(&p.content),
            width: p.width,
            height: p.height,
            depth: p.depth,
            lspace: p.lspace,
            voffset: p.voffset,
        }))),
        MathNode::Enclose(e) => MBox::new(Kind::Enclose(Box::new(EncloseBox {
            content: row_of(&e.content),
            notations: e.notations.clone(),
            line_thickness: e.line_thickness,
            rule: 0.0,
            offset: 0.0,
            arrow: 0.0,
        }))),
    })
}

/// `node` as the anonymous row MathML wraps the content of some elements
/// in.
fn row_of(node: &MathNode) -> MBox {
    match node {
        MathNode::Row(_) => build_one(node, None),
        other => MBox::stack(build_children(&[other], true, false)),
    }
}

fn operator_box(op: &super::Operator, form: Form) -> Option<MBox> {
    let mut text = op.text.clone();
    if text.chars().count() == 1 {
        text = variants::make_non_combining(&text);
    }
    let entry = operators::operator(&text);
    let defaults = entry.and_then(|e| e.form(form));
    let atom = op.atom.or(entry.map(|e| e.atom)).unwrap_or(Atom::Ord);
    let mu = |v: f64| MathLength::new(Dimen::mu(v));
    let lspace = op.lspace.or(defaults.map(|d| mu(d.lspace)));
    let rspace = op.rspace.or(defaults.map(|d| mu(d.rspace)));
    let mut chars = text.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && ('\u{2061}'..='\u{2064}').contains(&c)
    {
        let zero = |l: Option<MathLength>| l.is_none_or(|l| l.natural.value == 0.0);
        return match (zero(lspace), zero(rspace)) {
            (true, true) => None,
            (_, true) => Some(MBox::space(lspace.unwrap())),
            (true, _) => Some(MBox::space(rspace.unwrap())),
            _ => Some(MBox::stack(vec![MBox::space(lspace.unwrap()), MBox::space(rspace.unwrap())])),
        };
    }
    let mut b = text_box(TextKind::Operator, text, op.variant.unwrap_or(MathVariant::Normal));
    b.atom = atom;
    b.largeop = op.largeop.or(defaults.map(|d| d.largeop)).unwrap_or(false);
    b.movable_limits = op.movable_limits.or(defaults.map(|d| d.movable_limits)).unwrap_or(false);
    if let Kind::Text(t) = &mut b.kind {
        t.stretchy = op.stretchy.or(defaults.map(|d| d.stretchy)).unwrap_or(false);
    }
    Some(b)
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpaceKind {
    Thin,
    Med,
    Thick,
}

enum Rule {
    Space(SpaceKind, bool),
    Impossible,
}

/// The TeXbook's spacing table (p. 170): the space between two atoms, and
/// whether it is left out in scripts.
fn spacing(left: Atom, right: Atom) -> Option<Rule> {
    use Atom::*;
    use SpaceKind::*;
    let s = |k, not_script| Some(Rule::Space(k, not_script));
    match (left, right) {
        (Ord, Op) => s(Thin, false),
        (Ord, Bin) => s(Med, true),
        (Ord, Rel) => s(Thick, true),
        (Ord, Inner) => s(Thin, true),
        (Op, Ord | Op) => s(Thin, false),
        (Op, Bin) => Some(Rule::Impossible),
        (Op, Rel) => s(Thick, true),
        (Op, Inner) => s(Thin, true),
        (Bin, Ord | Op | Open | Inner) => s(Med, true),
        (Bin, Bin | Rel | Close | Punct) => Some(Rule::Impossible),
        (Rel, Ord | Op | Open | Inner) => s(Thick, true),
        (Rel, Bin) => Some(Rule::Impossible),
        (Open, Bin) => Some(Rule::Impossible),
        (Close, Op) => s(Thin, false),
        (Close, Bin) => s(Med, true),
        (Close, Rel) => s(Thick, true),
        (Close, Inner) => s(Thin, true),
        (Punct, Ord | Op | Rel | Open | Close | Punct | Inner) => s(Thin, true),
        (Punct, Bin) => Some(Rule::Impossible),
        (Inner, Ord | Open | Punct | Inner) => s(Thin, true),
        (Inner, Op) => s(Thin, false),
        (Inner, Bin) => s(Med, true),
        (Inner, Rel) => s(Thick, true),
        _ => None,
    }
}

/// The box whose atom faces a neighbour: the delimiter of a paired row.
fn facing(b: &mut MBox, last: bool) -> &mut MBox {
    let paired = matches!(&b.kind, Kind::Stack { paired: true, children, .. } if !children.is_empty());
    if !paired {
        return b;
    }
    let Kind::Stack { children, .. } = &mut b.kind else { unreachable!() };
    if last { children.last_mut().unwrap() } else { children.first_mut().unwrap() }
}

/// Space the atoms of a row as the TeXbook does, treating binary operators
/// with nothing to operate on as ordinary (p. 133).
fn insert_spaces(children: &mut Vec<MBox>, mode: Mode) {
    if let Some(first) = children.first_mut()
        && first.atom == Atom::Bin
    {
        first.atom = Atom::Ord;
    }
    let mut spaces = Vec::new();
    for i in 0..children.len().saturating_sub(1) {
        let (left, right) = children.split_at_mut(i + 1);
        let v = facing(&mut left[i], true);
        let v2 = facing(&mut right[0], false);
        let mut rule = spacing(v.atom, v2.atom);
        if let Some(Rule::Impossible) = rule {
            if v2.atom == Atom::Bin {
                v2.atom = Atom::Ord;
            } else {
                v.atom = Atom::Ord;
            }
            rule = spacing(v.atom, v2.atom);
        }
        if let Some(Rule::Space(kind, not_script)) = rule
            && !(not_script && (is_script(mode) || is_script_script(mode)))
        {
            spaces.push((i + 1, kind));
        }
    }
    for (i, kind) in spaces.into_iter().rev() {
        let width = match kind {
            SpaceKind::Thin => MathLength::thin(),
            SpaceKind::Med => MathLength::med(),
            SpaceKind::Thick => MathLength::thick(),
        };
        // Inserted spaces keep the default display style, so they are not
        // scaled down in scripts (as in SILE).
        children.insert(i, MBox::space(width));
    }
}

impl MBox {
    fn style(&mut self) {
        self.style_children();
        for c in self.children_mut() {
            c.style();
        }
    }

    fn style_children(&mut self) {
        let mode = self.mode;
        match &mut self.kind {
            Kind::Stack { children, row, .. } => {
                for c in children.iter_mut() {
                    c.mode = mode;
                }
                if !*row {
                    insert_spaces(children, mode);
                }
            }
            Kind::Scripts(s) => {
                s.base.mode = mode;
                let (accent, accent_under) = s.under_over.unwrap_or((false, false));
                if let Some(sub) = &mut s.sub {
                    sub.mode = if accent_under { accent_mode(mode) } else { subscript_mode(mode) };
                }
                if let Some(sup) = &mut s.sup {
                    sup.mode = if accent { accent_mode(mode) } else { superscript_mode(mode) };
                }
            }
            Kind::Fraction(f) => {
                f.numerator.mode = numerator_mode(mode);
                f.denominator.mode = denominator_mode(mode);
            }
            Kind::Table(t) => {
                let m = if mode == DISPLAY && t.display_style { DISPLAY } else { TEXT };
                for r in &mut t.rows {
                    r.mode = m;
                }
            }
            Kind::Sqrt(s) => {
                s.radicand.mode = radicand_mode(mode);
                if let Some(d) = &mut s.degree {
                    d.mode = degree_mode(mode);
                }
            }
            Kind::Padded(p) => p.content.mode = mode,
            Kind::Enclose(e) => e.content.mode = mode,
            Kind::Space(_) | Kind::Text(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

fn rightmost_gid(b: &MBox) -> u16 {
    match &b.kind {
        Kind::Stack { children, .. } => children.last().map_or(0, rightmost_gid),
        Kind::Text(t) => t.glyphs.last().map_or(0, |g| g.gid),
        _ => 0,
    }
}

impl MBox {
    fn shape_tree(&mut self, ctx: &mut Context) {
        for c in self.children_mut() {
            c.shape_tree(ctx);
        }
        self.shape(ctx);
    }

    fn shape(&mut self, ctx: &mut Context) {
        let mode = self.mode;
        let sd = ctx.scale_down(mode);
        let c = ctx.constants();
        let (width, height, depth) = match &mut self.kind {
            Kind::Stack { row: true, .. } => return,
            Kind::Stack { children, .. } => {
                let (mut h, mut d) = (0.0_f64, 0.0_f64);
                for (i, n) in children.iter_mut().enumerate() {
                    n.rel_y = 0.0;
                    (h, d) = if i == 0 { (n.height, n.depth) } else { (h.max(n.height), d.max(n.depth)) };
                }
                for n in children.iter_mut() {
                    if n.stretchy_operator().is_some() && n.vertical_stretch(ctx, d, h) {
                        h = h.max(n.height);
                        d = d.max(n.depth);
                    }
                }
                let mut w = Glue::default();
                for n in children.iter_mut() {
                    n.rel_x = w;
                    w = w + n.width;
                }
                (w, h, d)
            }
            Kind::Scripts(s) if s.under_over.is_none() => subscript_shape(s, mode, ctx, false),
            Kind::Scripts(s) => under_over_shape(s, mode, ctx),
            Kind::Space(space) => {
                let (w, h, d) = (ctx.glue(&space.width), ctx.glue(&space.height), ctx.glue(&space.depth));
                (w * sd, h.natural * sd, d.natural * sd)
            }
            Kind::Text(_) => return self.shape_text(ctx),
            Kind::Fraction(f) if f.bevelled => {
                let h_skew = c.skewed_fraction_horizontal_gap * sd;
                let v_skew_up = if is_cramped(mode) { c.superscript_shift_up_cramped } else { c.superscript_shift_up } * sd;
                f.rule = f.line_thickness.map_or(c.fraction_rule_thickness * sd, |d| ctx.resolve(d));
                let (n, d) = (&mut f.numerator, &mut f.denominator);
                n.rel_x = Glue::default();
                n.rel_y = -v_skew_up;
                d.rel_x = n.width + h_skew;
                d.rel_y = 0.0;
                f.padding = h_skew;
                (n.width + d.width + h_skew, (n.height + v_skew_up).max(d.height), (n.depth - v_skew_up).max(d.depth))
            }
            Kind::Fraction(f) => {
                f.padding = 0.75;
                let (n, d) = (&mut f.numerator, &mut f.denominator);
                let (widest, other) = if d.width.natural > n.width.natural { (d, n) } else { (n, d) };
                widest.rel_x = Glue::new(f.padding);
                other.rel_x = (widest.width - other.width) / 2.0 + f.padding;
                let width = widest.width + 2.0 * f.padding;
                f.axis = c.axis_height * sd;
                f.rule = f.line_thickness.map_or(c.fraction_rule_thickness * sd, |d| ctx.resolve(d));
                let (num_gap, den_gap, num_shift, den_shift) = if is_display(mode) {
                    (
                        c.fraction_num_display_style_gap_min,
                        c.fraction_denom_display_style_gap_min,
                        c.fraction_numerator_display_style_shift_up,
                        c.fraction_denominator_display_style_shift_down,
                    )
                } else {
                    (c.fraction_numerator_gap_min, c.fraction_denominator_gap_min, c.fraction_numerator_shift_up, c.fraction_denominator_shift_down)
                };
                let (axis, half) = (f.axis, f.rule / 2.0);
                let (n, d) = (&mut f.numerator, &mut f.denominator);
                n.rel_y = -axis - half - (num_gap * sd + n.depth).max(num_shift * sd - axis - half);
                d.rel_y = -axis + half + (den_gap * sd + d.height).max(den_shift * sd + axis - half);
                (width, n.height - n.rel_y, d.rel_y + d.depth)
            }
            Kind::Table(t) => {
                let spacing = ctx.size * 0.6;
                let row_spacing = t.row_spacing.map_or(spacing, |l| ctx.glue(&l).natural);
                let column_spacing = t.column_spacing.map_or(Glue::new(spacing), |l| ctx.glue(&l));
                let rows = t.rows.len();
                for row in &mut t.rows {
                    let (mut h, mut d) = (0.0_f64, 0.0_f64);
                    for cell in row.children_mut() {
                        h = h.max(cell.height);
                        d = d.max(cell.depth);
                    }
                    row.height = h;
                    row.depth = d;
                }
                let gap = |i: usize| if i + 1 == rows { 0.0 } else { row_spacing };
                let vert: f64 = t.rows.iter().enumerate().map(|(i, r)| r.height + r.depth + gap(i)).sum();
                let mut so_far = 0.0;
                for (i, row) in t.rows.iter_mut().enumerate() {
                    row.rel_y = so_far + row.height - vert;
                    so_far += row.height + row.depth + gap(i);
                }
                let columns = t.column_align.len();
                let mut x = Glue::default();
                for col in 0..columns {
                    let mut column_width = Glue::default();
                    for row in &mut t.rows {
                        if let Some(cell) = row.children_mut().into_iter().nth(col)
                            && cell.width.natural > column_width.natural
                        {
                            column_width = cell.width;
                        }
                    }
                    for row in &mut t.rows {
                        if let Some(cell) = row.children_mut().into_iter().nth(col) {
                            cell.rel_x = match t.column_align[col] {
                                ColumnAlign::Left => x,
                                ColumnAlign::Center => x + (column_width - cell.width) / 2.0,
                                ColumnAlign::Right => x + (column_width - cell.width),
                            };
                        }
                    }
                    x = x + column_width;
                    if col + 1 < columns {
                        x = x + column_spacing;
                    }
                }
                let axis = c.axis_height * sd;
                for row in &mut t.rows {
                    row.rel_y += vert / 2.0 - axis;
                    row.width = x;
                }
                (x, vert / 2.0 + axis, vert / 2.0 - axis)
            }
            Kind::Sqrt(s) => {
                s.thickness = c.radical_rule_thickness * sd;
                s.vertical_gap = if is_display(mode) { c.radical_display_style_vertical_gap } else { c.radical_vertical_gap } * sd;
                s.extra_ascender = c.radical_extra_ascender * sd;
                let (gw, gh, gd) = ctx.font.measure_char('√');
                let ratio = (s.radicand.height + s.radicand.depth) / (gh + gd);
                let ad_hoc = if ratio > 1.0 { ratio.ln() } else { 0.0 } * s.vertical_gap;
                let mut symbol_height = gh * sd;
                s.symbol_depth = (gd + ad_hoc) * sd;
                s.symbol_width = Glue::new((gw + ad_hoc) * sd);
                if s.radicand.height > symbol_height {
                    symbol_height = s.radicand.height;
                }
                s.short_height = symbol_height * c.radical_degree_bottom_raise_percent;
                s.offset = Glue::default();
                if let Some(degree) = &mut s.degree {
                    degree.rel_y = -c.radical_degree_bottom_raise_percent * symbol_height;
                    s.short_height -= c.radical_extra_ascender * sd;
                    s.offset = degree.width + c.radical_kern_before_degree * sd + c.radical_kern_after_degree * sd;
                }
                s.radicand.rel_x = s.symbol_width + s.offset;
                (s.radicand.width + s.symbol_width + s.offset, symbol_height + s.vertical_gap + s.extra_ascender, s.radicand.depth)
            }
            Kind::Padded(p) => {
                let clamp = |l: &Option<MathLength>| l.map(|l| ctx.resolve(l.natural).max(0.0));
                let content = &mut p.content;
                content.rel_x = Glue::new(clamp(&p.lspace).unwrap_or(0.0));
                content.rel_y = -p.voffset.map_or(0.0, |l| ctx.resolve(l.natural));
                (
                    clamp(&p.width).map_or(content.width, Glue::new),
                    clamp(&p.height).unwrap_or(content.height),
                    clamp(&p.depth).unwrap_or(content.depth),
                )
            }
            Kind::Enclose(e) => {
                let (hpad, vpad) = (0.25 * ctx.size, 0.4 * ctx.x_height);
                e.content.rel_x = Glue::new(hpad);
                e.rule = e.line_thickness.map_or(c.fraction_rule_thickness * sd, |d| ctx.resolve(d));
                e.offset = 0.1 * ctx.size;
                e.arrow = 0.8 * ctx.x_height;
                (e.content.width + 2.0 * hpad, e.content.height + vpad, e.content.depth + vpad)
            }
        };
        self.width = width;
        self.height = height;
        self.depth = depth;
    }

    fn shape_text(&mut self, ctx: &mut Context) {
        let mode = self.mode;
        let scale = ctx.scale_down(mode);
        let largeop = self.largeop;
        let Kind::Text(t) = &mut self.kind else { return };
        t.size = ctx.size * scale;
        let features = match ctx.script_feature {
            Some(f) if is_script(mode) => format!("+{f}=1"),
            Some(f) if is_script_script(mode) => format!("+{f}=2"),
            _ => String::new(),
        };
        let (key, items) = ctx.font.shape(&t.text, t.size, &features);
        t.font_key = key;
        t.glyphs = items
            .into_iter()
            .map(|g| Glyph {
                gid: g.gid,
                width: g.width,
                advance: g.x_advance,
                height: g.height,
                depth: g.depth,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
                text: g.text,
            })
            .collect();
        if is_display(mode)
            && largeop
            && let Some(first) = t.glyphs.first_mut()
            && let Some(variants) = ctx.table.vertical.get(&first.gid)
        {
            let mut biggest = None;
            let mut m = 0;
            for v in variants {
                if v.advance > m {
                    biggest = Some(v);
                    m = v.advance;
                }
            }
            if let Some(v) = biggest {
                let dim = ctx.font.glyph_dimensions(v.glyph, t.size);
                first.gid = v.glyph;
                first.width = dim.width;
                first.advance = dim.advance;
                let axis = ctx.table.constants(t.size).axis_height * scale;
                let size = dim.height + dim.depth;
                first.height = size / 2.0 + axis;
                first.depth = size / 2.0 - axis;
                t.font_extents = Some((dim.height, dim.depth));
            }
        }
        let Some(last) = t.glyphs.last() else {
            (self.width, self.height, self.depth) = (Glue::default(), 0.0, 0.0);
            return;
        };
        let mut width: f64 = t.glyphs.iter().map(|g| g.advance).sum();
        let for_subscript = width;
        if let Some(it) = ctx.table.italic_correction(last.gid, t.size) {
            width += it * scale;
        }
        let height = t.glyphs.iter().map(|g| g.height).fold(f64::NEG_INFINITY, f64::max);
        let depth = t.glyphs.iter().map(|g| g.depth).fold(f64::NEG_INFINITY, f64::max);
        self.width = Glue::new(width);
        self.width_for_subscript = Some(Glue::new(for_subscript));
        self.height = height;
        self.depth = depth;
    }

    /// Swap the glyph for the variant closest to `target` points along the
    /// stretch direction, if one comes closer than the glyph itself.
    fn stretchy_reshape(&mut self, ctx: &mut Context, target: f64, vertical: bool) -> bool {
        let current_size = if vertical { self.depth + self.height } else { self.width.natural };
        let Kind::Text(t) = &mut self.kind else { return false };
        let Some(first) = t.glyphs.first_mut() else { return false };
        let upem = ctx.table.units_per_em;
        let required = target * upem / t.size;
        let constructions = if vertical { &ctx.table.vertical } else { &ctx.table.horizontal };
        let Some(variants) = constructions.get(&first.gid) else { return false };
        let current = current_size * upem / t.size;
        let mut m = required - current;
        let mut closest = None;
        for v in variants {
            let diff = (v.advance as f64 - required).abs();
            if diff < m {
                closest = Some(v);
                m = diff;
            }
        }
        let Some(v) = closest else { return false };
        let dim = ctx.font.glyph_dimensions(v.glyph, t.size);
        first.gid = v.glyph;
        first.width = dim.width;
        first.height = dim.height;
        first.depth = dim.depth;
        first.advance = dim.advance;
        self.width = Glue::new(dim.advance);
        self.height = dim.height;
        self.depth = dim.depth;
        true
    }

    /// Stretch a delimiter over a row's height and depth, centred on the
    /// axis, scaling the closest variant to fit.
    fn vertical_stretch(&mut self, ctx: &mut Context, depth: f64, height: f64) -> bool {
        if !self.stretchy_reshape(ctx, depth + height, true) {
            return false;
        }
        let sd = ctx.scale_down(self.mode);
        let Kind::Text(t) = &mut self.kind else { return false };
        let axis = ctx.table.constants(t.size).axis_height * sd;
        let mid = (height - axis).max(depth + axis);
        let ratio = 2.0 * mid / (self.height + self.depth);
        t.vert_scaling = Some((ratio, self.height - (mid + axis) / ratio));
        self.height = mid + axis;
        self.depth = mid - axis;
        true
    }

    /// Stretch an accent or brace over `width`, scaling it to fit; glyphs
    /// are never shrunk.
    fn horizontal_stretch(&mut self, ctx: &mut Context, width: Glue) -> bool {
        let stretched = self.stretchy_reshape(ctx, width.natural, false);
        if !stretched && width.natural < self.width.natural {
            return false;
        }
        let natural = self.width.natural;
        if let Kind::Text(t) = &mut self.kind {
            t.horiz_scaling = Some(width.natural / natural);
        }
        self.width = width;
        true
    }
}

fn stretch_to_base(part: &mut MBox, base_width: Glue, ctx: &mut Context) {
    if !part.has_children() {
        if part.stretchy_operator().is_some() {
            part.horizontal_stretch(ctx, base_width);
        }
        return;
    }
    if !matches!(&part.kind, Kind::Scripts(s) if s.under_over.is_some()) {
        return;
    }
    let mut stretched = false;
    for elt in part.children_mut() {
        if elt.stretchy_operator().is_some() && elt.horizontal_stretch(ctx, base_width) {
            stretched = true;
        }
    }
    if stretched {
        part.shape(ctx);
    }
}

fn subscript_shape(s: &mut ScriptsBox, mode: Mode, ctx: &Context, under_over: bool) -> (Glue, f64, f64) {
    let c = ctx.constants();
    let sd = ctx.scale_down(mode);
    let ScriptsBox { base, sub, sup, as_scripts, .. } = s;
    base.rel_x = Glue::default();
    base.rel_y = 0.0;
    let mut width = base.width_for_subscript.unwrap_or(base.width);
    let it = italic_correction(base, under_over, mode, ctx) * sd;
    let base_symbol = base.is_terminal();
    let largeop = base.largeop;
    let limits = *as_scripts || largeop;
    let sub_shift = if limits { -it } else { 0.0 };
    let sup_shift = if limits { 0.0 } else { it };
    if let Some(sub) = sub.as_mut() {
        sub.rel_x = width + sub_shift;
        sub.rel_y = (c.subscript_shift_down * sd)
            .max(if base_symbol { 0.0 } else { base.depth + c.subscript_baseline_drop_min * sd })
            .max(sub.height - c.subscript_top_max * sd);
        if under_over || largeop {
            sub.rel_y = sub.rel_y.max(base.depth + c.subscript_baseline_drop_min * sd);
        }
    }
    if let Some(sup) = sup.as_mut() {
        sup.rel_x = width + sup_shift;
        let shift_up = if is_cramped(mode) { c.superscript_shift_up_cramped } else { c.superscript_shift_up } * sd;
        sup.rel_y = -shift_up
            .max(if base_symbol { 0.0 } else { base.height - c.superscript_baseline_drop_max * sd })
            .max(sup.depth + c.superscript_bottom_min * sd);
        if under_over || largeop {
            sup.rel_y = -(-sup.rel_y).max(base.height - c.superscript_baseline_drop_max * sd);
        }
    }
    if let (Some(sub), Some(sup)) = (sub.as_mut(), sup.as_mut()) {
        let gap = sub.rel_y - sub.height - sup.rel_y - sup.depth;
        if gap < c.sub_superscript_gap_min * sd {
            sub.rel_y = c.sub_superscript_gap_min * sd + sub.height + sup.rel_y + sup.depth;
            let psi = c.superscript_bottom_max_with_subscript * sd + sup.rel_y + sup.depth;
            if psi > 0.0 {
                sup.rel_y -= psi;
                sub.rel_y -= psi;
            }
        }
    }
    let sub_w = sub.as_ref().map_or(Glue::default(), |b| b.width + sub_shift);
    let sup_w = sup.as_ref().map_or(Glue::default(), |b| b.width + sup_shift);
    width = width + sub_w.max(sup_w) + c.space_after_script * sd;
    let height = base.height.max(sub.as_ref().map_or(0.0, |b| b.height - b.rel_y)).max(sup.as_ref().map_or(0.0, |b| b.height - b.rel_y));
    let depth = base.depth.max(sub.as_ref().map_or(0.0, |b| b.depth + b.rel_y)).max(sup.as_ref().map_or(0.0, |b| b.depth + b.rel_y));
    (width, height, depth)
}

fn italic_correction(base: &MBox, under_over: bool, mode: Mode, ctx: &Context) -> f64 {
    let gid = rightmost_gid(base);
    if gid == 0 {
        return 0.0;
    }
    let Some(mut c) = ctx.table.italic_correction(gid, ctx.size) else { return 0.0 };
    if under_over
        && base.largeop
        && is_display(mode)
        && let Kind::Text(t) = &base.kind
    {
        c *= t.size / ctx.size;
    }
    c
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Base,
    Sub,
    Sup,
}

fn under_over_shape(s: &mut ScriptsBox, mode: Mode, ctx: &mut Context) -> (Glue, f64, f64) {
    let c = ctx.constants();
    let sd = ctx.scale_down(mode);
    let it = italic_correction(&s.base, true, mode, ctx) * sd;
    if !is_display(mode) && s.base.movable_limits {
        s.as_scripts = true;
        return subscript_shape(s, mode, ctx, true);
    }
    let (accent, accent_under) = s.under_over.unwrap_or_default();
    let ScriptsBox { base, sub, sup, .. } = s;
    base.rel_y = 0.0;
    if let Some(sub) = sub.as_mut() {
        stretch_to_base(sub, base.width, ctx);
        sub.rel_y = base.depth
            + if accent_under {
                sub.height + c.lower_limit_gap_min * sd
            } else {
                (sub.height + c.lower_limit_gap_min * sd).max(c.lower_limit_baseline_drop_min * sd)
            };
    }
    if let Some(sup) = sup.as_mut() {
        stretch_to_base(sup, base.width, ctx);
        if accent {
            sup.rel_y = -base.height - (c.accent_base_height * sd - base.height).max(0.0);
            let heuristics = 0.5 * c.flattened_accent_base_height + 0.5 * c.accent_base_height;
            if sup.height > heuristics * sd {
                sup.rel_y += c.accent_base_height * sd;
            }
        } else {
            sup.rel_y = -base.height - (c.upper_limit_gap_min * sd + sup.depth).max(c.upper_limit_baseline_rise_min * sd);
        }
    }
    let wb = base.width.natural;
    let ws = sub.as_ref().map(|b| b.width.natural);
    let wp = sup.as_ref().map(|b| b.width.natural);
    let (widest, a, b) = match (ws, wp) {
        (Some(ws), Some(wp)) if ws > wb && ws > wp => (Part::Sub, Some(Part::Base), Some(Part::Sup)),
        (Some(ws), Some(_)) if ws > wb => (Part::Sup, Some(Part::Base), Some(Part::Sub)),
        (Some(ws), None) if ws > wb => (Part::Sub, Some(Part::Base), None),
        (_, Some(wp)) if wb > wp => (Part::Base, sub.as_ref().map(|_| Part::Sub), Some(Part::Sup)),
        (_, Some(_)) => (Part::Sup, Some(Part::Base), sub.as_ref().map(|_| Part::Sub)),
        (_, None) => (Part::Base, sub.as_ref().map(|_| Part::Sub), None),
    };
    let widths = [base.width, sub.as_ref().map_or(Glue::default(), |b| b.width), sup.as_ref().map_or(Glue::default(), |b| b.width)];
    let mut rel = [None; 3];
    rel[widest as usize] = Some(Glue::default());
    let center = widths[widest as usize] / 2.0;
    for p in [a, b].into_iter().flatten() {
        rel[p as usize] = Some(center - widths[p as usize] / 2.0);
    }
    if let Some(r) = rel[0] {
        base.rel_x = r;
    }
    if let (Some(r), Some(sub)) = (rel[1], sub.as_mut()) {
        sub.rel_x = r;
    }
    if let (Some(r), Some(sup)) = (rel[2], sup.as_mut()) {
        sup.rel_x = r;
    }
    if let Some(sup) = sup.as_mut() {
        sup.rel_x = sup.rel_x + it / 2.0;
    }
    if let Some(sub) = sub.as_mut() {
        sub.rel_x = sub.rel_x - it / 2.0;
    }
    let width = base
        .width
        .max(sub.as_ref().map_or(Glue::default(), |b| b.width))
        .max(sup.as_ref().map_or(Glue::default(), |b| b.width));
    let height = sup.as_ref().map_or(base.height, |b| b.height - b.rel_y);
    let depth = sub.as_ref().map_or(base.depth, |b| b.rel_y + b.depth);
    (width, height, depth)
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

impl MBox {
    fn children(&self) -> Vec<&MBox> {
        match &self.kind {
            Kind::Stack { children, .. } => children.iter().collect(),
            Kind::Scripts(s) => {
                if s.under_over.is_some() {
                    s.sup.iter().chain(std::iter::once(&s.base)).chain(s.sub.iter()).collect()
                } else {
                    std::iter::once(&s.base).chain(s.sub.iter()).chain(s.sup.iter()).collect()
                }
            }
            Kind::Space(_) | Kind::Text(_) => Vec::new(),
            Kind::Fraction(f) => vec![&f.numerator, &f.denominator],
            Kind::Table(t) => t.rows.iter().collect(),
            Kind::Sqrt(s) => s.degree.iter().chain(std::iter::once(&s.radicand)).collect(),
            Kind::Padded(p) => vec![&p.content],
            Kind::Enclose(e) => vec![&e.content],
        }
    }

    fn output(&self, x: Glue, y: f64, items: &mut Vec<MathItem>) {
        match &self.kind {
            Kind::Stack { phantom: true, .. } => return,
            Kind::Text(t) if !t.glyphs.is_empty() => {
                let mut gy = y;
                if is_display(self.mode)
                    && self.largeop
                    && let Some((_, font_depth)) = t.font_extents
                {
                    gy += t.glyphs[0].depth - font_depth;
                }
                if let Some((_, offset)) = t.vert_scaling {
                    gy += offset;
                }
                let glyphs = t
                    .glyphs
                    .iter()
                    .map(|g| GlyphData {
                        gid: g.gid,
                        width: g.width,
                        x_advance: g.advance,
                        y_advance: 0.0,
                        x_offset: g.x_offset,
                        y_offset: g.y_offset,
                        text: g.text.clone(),
                    })
                    .collect();
                let nnode = NNode::with_glyphs(t.text.clone(), glyphs, t.font_key.clone(), t.size, self.width.natural, self.height, self.depth);
                let scale = (t.horiz_scaling.is_some() || t.vert_scaling.is_some()).then(|| GlyphScale {
                    x: t.horiz_scaling.unwrap_or(1.0),
                    y: t.vert_scaling.map_or(1.0, |(r, _)| r),
                    origin_y: y,
                });
                items.push(MathItem::Glyphs { x, y: gy, nnode: Box::new(nnode), scale });
            }
            Kind::Fraction(f) if f.bevelled => items.push(MathItem::Figure {
                x: x + f.numerator.rel_x + f.numerator.width,
                baseline: y,
                height: self.height,
                figure: Figure::Bevel { width: Glue::new(f.padding), height: self.height, depth: self.depth, thickness: f.rule },
            }),
            Kind::Fraction(f) if f.rule > 0.0 => items.push(MathItem::Rule {
                x: x + f.padding,
                y: y - f.axis - f.rule / 2.0,
                width: self.width - 2.0 * f.padding,
                height: f.rule,
            }),
            Kind::Sqrt(s) => {
                items.push(MathItem::Figure {
                    x,
                    baseline: y,
                    height: self.height,
                    figure: Figure::Radical {
                        offset: s.offset,
                        symbol_width: s.symbol_width,
                        height: self.height,
                        depth: self.depth,
                        short_height: s.short_height,
                        symbol_depth: s.symbol_depth,
                        extra_ascender: s.extra_ascender,
                        thickness: s.thickness,
                    },
                });
                items.push(MathItem::Rule {
                    x: x + s.offset + s.symbol_width,
                    y: y - self.height + s.extra_ascender - s.thickness / 2.0,
                    width: s.radicand.width,
                    height: s.thickness,
                });
            }
            Kind::Enclose(e) if !e.notations.is_empty() => items.push(MathItem::Figure {
                x,
                baseline: y,
                height: self.height,
                figure: Figure::Enclose {
                    width: self.width,
                    height: self.height,
                    depth: self.depth,
                    thickness: e.rule,
                    offset: e.offset,
                    arrow: e.arrow,
                    notations: e.notations.clone(),
                },
            }),
            _ => {}
        }
        for c in self.children() {
            c.output(x + c.rel_x, y + c.rel_y, items);
        }
    }
}

/// A formula laid out: its box and what it draws.
pub(crate) struct LaidOut {
    pub width: Glue,
    pub height: f64,
    pub depth: f64,
    pub items: Vec<MathItem>,
}

pub(crate) fn lay_out(node: &MathNode, display: bool, ctx: &mut Context) -> LaidOut {
    let mut root = row_of(node);
    root.mode = if display { DISPLAY } else { TEXT };
    root.style();
    root.shape_tree(ctx);
    let mut items = Vec::new();
    root.output(Glue::default(), 0.0, &mut items);
    LaidOut { width: root.width, height: root.height, depth: root.depth, items }
}
