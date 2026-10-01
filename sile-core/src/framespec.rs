//! Frames declared the way SILE declares them: each edge or extent is an
//! expression such as `left(content)`, `top(footnotes)+3%ph` or `86%pw`,
//! and the whole set is solved together.

use std::collections::BTreeMap;

use cassowary::strength::REQUIRED;
use cassowary::{Expression, Solver, Variable, WeightedRelation::EQ};

use crate::font::Direction;

/// One of the four ways text or lines can advance across a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    LTR,
    RTL,
    TTB,
    BTT,
}

impl Flow {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "LTR" => Some(Self::LTR),
            "RTL" => Some(Self::RTL),
            "TTB" => Some(Self::TTB),
            "BTT" => Some(Self::BTT),
            _ => None,
        }
    }
}

/// How a frame fills: the way text runs along a line, then the way lines
/// follow one another (SILE's frame `direction`, such as `RTL-TTB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDirection {
    pub writing: Flow,
    pub page: Flow,
}

impl FrameDirection {
    pub const LTR: Self = Self { writing: Flow::LTR, page: Flow::TTB };
    pub const RTL: Self = Self { writing: Flow::RTL, page: Flow::TTB };
    /// Vertical Japanese: columns down the page, from right to left.
    pub const TATE: Self = Self { writing: Flow::TTB, page: Flow::RTL };

    /// `WRITING-PAGE`, either part optional as in SILE.
    pub fn parse(s: &str) -> Option<Self> {
        let (writing, page) = match s.split_once('-') {
            Some((w, p)) => (Flow::parse(w)?, Flow::parse(p)?),
            None => (Flow::parse(s)?, Flow::TTB),
        };
        Some(Self { writing, page })
    }

    /// The direction text is shaped in.
    pub fn text(self) -> Direction {
        match self.writing {
            Flow::LTR => Direction::LTR,
            Flow::RTL => Direction::RTL,
            Flow::TTB | Flow::BTT => Direction::TTB,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self.writing, Flow::TTB | Flow::BTT)
    }
}

impl From<Direction> for FrameDirection {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::LTR => Self::LTR,
            Direction::RTL => Self::RTL,
            Direction::TTB => Self { writing: Flow::TTB, page: Flow::TTB },
        }
    }
}
use crate::frame::PaperSize;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameSpec {
    pub id: String,
    pub left: Option<String>,
    pub right: Option<String>,
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub width: Option<String>,
    pub height: Option<String>,
    /// Frame that content flows into when this one is full.
    pub next: Option<String>,
    pub direction: Option<FrameDirection>,
    /// Set as vertical Japanese: lines broken first-fit, one zenkaku tall
    /// (SILE's tate frames).
    pub tate: bool,
}

impl FrameSpec {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Self::default() }
    }

    pub fn left(mut self, e: impl Into<String>) -> Self {
        self.left = Some(e.into());
        self
    }
    pub fn right(mut self, e: impl Into<String>) -> Self {
        self.right = Some(e.into());
        self
    }
    pub fn top(mut self, e: impl Into<String>) -> Self {
        self.top = Some(e.into());
        self
    }
    pub fn bottom(mut self, e: impl Into<String>) -> Self {
        self.bottom = Some(e.into());
        self
    }
    pub fn width(mut self, e: impl Into<String>) -> Self {
        self.width = Some(e.into());
        self
    }
    pub fn height(mut self, e: impl Into<String>) -> Self {
        self.height = Some(e.into());
        self
    }
    pub fn next(mut self, id: impl Into<String>) -> Self {
        self.next = Some(id.into());
        self
    }

    /// The same frame on the facing page: edges given as absolute positions
    /// are reflected about the page's vertical axis (SILE's `mirrorMaster`).
    pub fn mirrored(&self) -> Self {
        let mut out = self.clone();
        let absolute = |e: &Option<String>| e.as_ref().filter(|e| !e.contains('(')).cloned();
        if let Some(right) = absolute(&self.right) {
            out.left = Some(format!("100%pw-({right})"));
        }
        if let Some(left) = absolute(&self.left) {
            out.right = Some(format!("100%pw-({left})"));
        }
        out
    }

    fn edges(&self) -> [(Edge, &Option<String>); 6] {
        [
            (Edge::Left, &self.left),
            (Edge::Right, &self.right),
            (Edge::Top, &self.top),
            (Edge::Bottom, &self.bottom),
            (Edge::Width, &self.width),
            (Edge::Height, &self.height),
        ]
    }
}

/// A solved frame, in points from the top-left of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameGeometry {
    pub id: String,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub next: Option<String>,
    pub direction: Option<FrameDirection>,
    /// Set as vertical Japanese: lines broken first-fit, one zenkaku tall
    /// (SILE's tate frames).
    pub tate: bool,
}

impl FrameGeometry {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrameError {
    Parse { frame: String, expr: String },
    Unsolvable(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Parse { frame, expr } => write!(f, "frame {frame}: can't parse {expr:?}"),
            FrameError::Unsolvable(e) => write!(f, "frames can't be solved: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    Width,
    Height,
}

impl Edge {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "left" => Edge::Left,
            "right" => Edge::Right,
            "top" => Edge::Top,
            "bottom" => Edge::Bottom,
            "width" => Edge::Width,
            "height" => Edge::Height,
            _ => return None,
        })
    }
}

/// Solve a page's frames. Frames named in expressions but not declared are
/// created unconstrained, as SILE does. `em` is the current font size.
pub fn solve(paper: PaperSize, em: f64, specs: &[FrameSpec]) -> Result<Vec<FrameGeometry>, FrameError> {
    let mut vars: BTreeMap<(String, Edge), Variable> = BTreeMap::new();
    let mut constraints = Vec::new();
    for spec in specs {
        for (edge, expr) in spec.edges() {
            let Some(expr) = expr else { continue };
            let parsed = Parser { src: expr, pos: 0, paper, em }
                .parse()
                .ok_or_else(|| FrameError::Parse { frame: spec.id.clone(), expr: expr.clone() })?;
            constraints.push(((spec.id.clone(), edge), parsed));
        }
    }

    let mut var = |frame: &str, edge: Edge| *vars.entry((frame.to_string(), edge)).or_insert_with(Variable::new);
    let mut solver = Solver::new();
    let add = |solver: &mut Solver, c| solver.add_constraint(c).map_err(|e| FrameError::Unsolvable(format!("{e:?}")));
    let mut frames: Vec<String> = specs.iter().map(|s| s.id.clone()).collect();
    for (_, expr) in &constraints {
        expr.frames(&mut frames);
    }
    for id in &frames {
        let (l, r, t, b, w, h) = (
            var(id, Edge::Left),
            var(id, Edge::Right),
            var(id, Edge::Top),
            var(id, Edge::Bottom),
            var(id, Edge::Width),
            var(id, Edge::Height),
        );
        add(&mut solver, w | EQ(REQUIRED) | (r - l))?;
        add(&mut solver, h | EQ(REQUIRED) | (b - t))?;
    }
    for ((frame, edge), expr) in &constraints {
        let lhs = var(frame, *edge);
        let rhs = expr.to_expression(&mut var);
        add(&mut solver, lhs | EQ(REQUIRED) | rhs)?;
    }

    let value = |solver: &Solver, frame: &str, edge: Edge| vars.get(&(frame.to_string(), edge)).map_or(0.0, |v| solver.get_value(*v));
    Ok(specs
        .iter()
        .map(|spec| FrameGeometry {
            id: spec.id.clone(),
            left: value(&solver, &spec.id, Edge::Left),
            top: value(&solver, &spec.id, Edge::Top),
            right: value(&solver, &spec.id, Edge::Right),
            bottom: value(&solver, &spec.id, Edge::Bottom),
            next: spec.next.clone(),
            direction: spec.direction,
            tate: spec.tate,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Edge(Edge, String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn frames(&self, out: &mut Vec<String>) {
        match self {
            Expr::Number(_) => {}
            Expr::Edge(_, f) => {
                if !out.contains(f) {
                    out.push(f.clone());
                }
            }
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) => {
                a.frames(out);
                b.frames(out);
            }
        }
    }

    fn constant(&self) -> Option<f64> {
        match self {
            Expr::Number(n) => Some(*n),
            Expr::Edge(..) => None,
            Expr::Add(a, b) => Some(a.constant()? + b.constant()?),
            Expr::Sub(a, b) => Some(a.constant()? - b.constant()?),
            Expr::Mul(a, b) => Some(a.constant()? * b.constant()?),
            Expr::Div(a, b) => Some(a.constant()? / b.constant()?),
        }
    }

    fn to_expression(&self, var: &mut impl FnMut(&str, Edge) -> Variable) -> Expression {
        match self {
            Expr::Number(n) => Expression::from_constant(*n),
            Expr::Edge(edge, frame) => Expression::from(var(frame, *edge)),
            Expr::Add(a, b) => a.to_expression(var) + b.to_expression(var),
            Expr::Sub(a, b) => a.to_expression(var) - b.to_expression(var),
            Expr::Mul(a, b) => match (a.constant(), b.constant()) {
                (Some(k), _) => b.to_expression(var) * k,
                (_, Some(k)) => a.to_expression(var) * k,
                _ => a.to_expression(var),
            },
            Expr::Div(a, b) => a.to_expression(var) / b.constant().unwrap_or(1.0),
        }
    }
}

/// SILE's frame parser grammar, right-recursive as in the original (so
/// `a-b+c` reads as `a-(b+c)`).
struct Parser<'a> {
    src: &'a str,
    pos: usize,
    paper: PaperSize,
    em: f64,
}

impl Parser<'_> {
    fn parse(mut self) -> Option<Expr> {
        let e = self.additive()?;
        self.ws();
        (self.pos == self.src.len()).then_some(e)
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn ws(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.rest().starts_with(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn additive(&mut self) -> Option<Expr> {
        let lhs = self.multiplicative()?;
        if self.eat('+') {
            Some(Expr::Add(Box::new(lhs), Box::new(self.additive()?)))
        } else if self.eat('-') {
            Some(Expr::Sub(Box::new(lhs), Box::new(self.additive()?)))
        } else {
            Some(lhs)
        }
    }

    fn multiplicative(&mut self) -> Option<Expr> {
        let lhs = self.primary()?;
        if self.eat('*') {
            Some(Expr::Mul(Box::new(lhs), Box::new(self.multiplicative()?)))
        } else if self.eat('/') {
            Some(Expr::Div(Box::new(lhs), Box::new(self.multiplicative()?)))
        } else {
            Some(lhs)
        }
    }

    fn primary(&mut self) -> Option<Expr> {
        self.ws();
        if self.eat('(') {
            let e = self.additive()?;
            return self.eat(')').then_some(e);
        }
        let rest = self.rest();
        let word_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        if let Some(edge) = Edge::parse(&rest[..word_len]) {
            self.pos += word_len;
            if !self.eat('(') {
                return None;
            }
            self.ws();
            let rest = self.rest();
            let id_len = rest.find(|c: char| c == ')' || c.is_whitespace()).unwrap_or(rest.len());
            let id = rest[..id_len].to_string();
            self.pos += id_len;
            return self.eat(')').then_some(Expr::Edge(edge, id));
        }
        self.measurement()
    }

    fn measurement(&mut self) -> Option<Expr> {
        let rest = self.rest();
        let num_len = rest
            .char_indices()
            .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && c == '-')))
            .map_or(rest.len(), |(i, _)| i);
        let amount: f64 = rest[..num_len].parse().ok()?;
        self.pos += num_len;
        self.ws();
        let rest = self.rest();
        let unit_len = rest.find(|c: char| !(c.is_ascii_alphabetic() || c == '%')).unwrap_or(rest.len());
        let unit = &rest[..unit_len];
        let PaperSize { width, height } = self.paper;
        let factor = match unit {
            "" | "pt" => 1.0,
            "mm" => 72.0 / 25.4,
            "cm" => 72.0 / 2.54,
            "in" => 72.0,
            "px" => 0.75,
            "%pw" => width / 100.0,
            "%ph" => height / 100.0,
            "%pmin" => width.min(height) / 100.0,
            "%pmax" => width.max(height) / 100.0,
            "em" => self.em,
            "en" => self.em / 2.0,
            _ => return None,
        };
        self.pos += unit_len;
        Some(Expr::Number(amount * factor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn solves_sile_plain_frames() {
        let paper = PaperSize::new(200.0, 100.0);
        let specs = [
            FrameSpec::new("content").left("5%pw").right("95%pw").top("5%ph").bottom("top(footnotes)"),
            FrameSpec::new("folio")
                .left("left(content)")
                .right("right(content)")
                .top("bottom(footnotes)+2%ph")
                .bottom("97%ph"),
            FrameSpec::new("footnotes").left("left(content)").right("right(content)").height("0").bottom("90%ph"),
        ];
        let frames = solve(paper, 10.0, &specs).unwrap();
        let content = &frames[0];
        assert!(close(content.left, 10.0) && close(content.right, 190.0));
        assert!(close(content.top, 5.0) && close(content.bottom, 90.0));
        let folio = &frames[1];
        assert!(close(folio.top, 92.0) && close(folio.bottom, 97.0) && close(folio.left, 10.0));
    }

    #[test]
    fn mirroring_reflects_absolute_edges() {
        let spec = FrameSpec::new("content").left("8.3%pw").right("86%pw");
        let m = spec.mirrored();
        assert_eq!(m.left.as_deref(), Some("100%pw-(86%pw)"));
        assert_eq!(m.right.as_deref(), Some("100%pw-(8.3%pw)"));
        let frames = solve(PaperSize::new(1000.0, 10.0), 10.0, &[m]).unwrap();
        assert!(close(frames[0].left, 140.0) && close(frames[0].right, 917.0));
    }

    #[test]
    fn bad_expressions_are_errors() {
        let specs = [FrameSpec::new("a").left("left(b")];
        assert!(matches!(solve(PaperSize::A4, 10.0, &specs), Err(FrameError::Parse { .. })));
    }
}
