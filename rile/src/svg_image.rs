//! SVG drawings placed in the text: paths and basic shapes with flat fills and
//! strokes, turned into PDF path operators.
// SILE: `svg` package.

use std::fmt;
use std::sync::Arc;

use crate::builder::{BuilderError, Typesetter};
use crate::length::Length;
use crate::node::{HBox, Ink};

const PI: f32 = std::f32::consts::PI;
#[allow(clippy::excessive_precision)]
const KAPPA90: f32 = 0.552_284_749_3;

/// One PDF drawing operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SvgOp {
    Move(f32, f32),
    Line(f32, f32),
    Curve([f32; 6]),
    LineWidth(f32),
    LineJoin(u8),
    LineCap(u8),
    StrokeRgb(f64, f64, f64),
    FillRgb(f64, f64, f64),
    Close,
    Stroke,
    CloseStroke,
    Fill,
    FillEvenOdd,
    FillStroke,
}

/// A parsed drawing: operators in its own units, y growing downwards.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgImage {
    pub ops: Vec<SvgOp>,
    pub width: f64,
    pub height: f64,
}

impl fmt::Display for SvgImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for op in &self.ops {
            match op {
                SvgOp::Move(x, y) => write!(f, "{:.6} {:.6} m ", x, y)?,
                SvgOp::Line(x, y) => write!(f, "{:.6} {:.6} l ", x, y)?,
                SvgOp::Curve(c) => write!(f, "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ", c[0], c[1], c[2], c[3], c[4], c[5])?,
                SvgOp::LineWidth(w) => write!(f, "{:.6} w ", w)?,
                SvgOp::LineJoin(j) => write!(f, "{j} j ")?,
                SvgOp::LineCap(c) => write!(f, "{c} J ")?,
                SvgOp::StrokeRgb(r, g, b) => write!(f, "{r:.6} {g:.6} {b:.6} RG ")?,
                SvgOp::FillRgb(r, g, b) => write!(f, "{r:.6} {g:.6} {b:.6} rg ")?,
                SvgOp::Close => f.write_str("h ")?,
                SvgOp::Stroke => f.write_str("S ")?,
                SvgOp::CloseStroke => f.write_str("s ")?,
                SvgOp::Fill => f.write_str("f ")?,
                SvgOp::FillEvenOdd => f.write_str("f* ")?,
                SvgOp::FillStroke => f.write_str("B ")?,
            }
        }
        Ok(())
    }
}

/// An SVG drawing set in the text, scaled by `scale`.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgFigure {
    pub image: Arc<SvgImage>,
    pub scale: f64,
    /// Hang the drawing from the baseline rather than stand it on it.
    pub drop: bool,
}

impl SvgImage {
    /// Parse `src` with lengths in points at `density` dots per inch.
    pub fn parse(src: &str, density: f64) -> Self {
        let mut p = Parser::new(density as f32);
        p.parse_xml(src);
        p.scale_to_viewbox();
        let ops = p.shapes.iter().flat_map(Shape::ops).collect();
        Self { ops, width: p.width as f64, height: p.height as f64 }
    }
}

impl Typesetter {
    /// Set `image` in the text at `width` or `height` in points, keeping
    /// its proportions, drawn at `density` dots per inch.
    // SILE: `\svg`.
    pub fn add_svg(
        &mut self,
        image: Arc<SvgImage>,
        width: Option<f64>,
        height: Option<f64>,
        density: f64,
        drop: bool,
    ) -> Result<&mut Self, BuilderError> {
        let scale = match (width, height) {
            (Some(_), Some(_)) => {
                return Err(BuilderError::Layout("SVG aspect ratios can't be changed: give a width or a height, not both".into()));
            }
            (Some(w), None) => w / image.width,
            (None, Some(h)) => h / image.height,
            (None, None) => 1.0,
        };
        let (w, h) = (image.width * scale, image.height * scale);
        let figure = SvgFigure { image, scale: scale * density / 72.0, drop };
        let hbox = HBox { ink: Some(Ink::Svg(figure)), ..HBox::new(Length::pt(w), Length::pt(h), Length::zero()) };
        Ok(self.add_figure(hbox))
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Paint {
    None,
    Color(u32),
}

#[derive(Clone, Copy, PartialEq)]
enum Units {
    User,
    Px,
    Pt,
    Pc,
    Mm,
    Cm,
    In,
    Percent,
    Em,
    Ex,
}

#[derive(Clone)]
struct Attrib {
    xform: [f32; 6],
    fill_color: u32,
    stroke_color: u32,
    stroke_width: f32,
    even_odd: bool,
    font_size: f32,
    /// 0 none, 1 colour, 2 gradient.
    has_fill: u8,
    has_stroke: u8,
}

struct Path {
    pts: Vec<[f32; 2]>,
    closed: bool,
    bounds: [f32; 4],
}

struct Shape {
    fill: Paint,
    stroke: Paint,
    stroke_width: f32,
    even_odd: bool,
    bounds: [f32; 4],
    paths: Vec<Path>,
}

impl Shape {
    // SILE: what `svg_to_ps` writes for the shape.
    fn ops(&self) -> Vec<SvgOp> {
        let mut out = Vec::new();
        let mut oper = SvgOp::CloseStroke;
        let rgb = |c: u32| ((c & 0xff) as f64 / 256.0, ((c >> 8) & 0xff) as f64 / 256.0, ((c >> 16) & 0xff) as f64 / 256.0);
        for path in &self.paths {
            let mut last = None;
            let mut i = 0;
            while i + 1 < path.pts.len() {
                let p = &path.pts[i..i + 4];
                if last != Some(p[0]) {
                    out.push(SvgOp::Move(p[0][0], p[0][1]));
                }
                out.push(SvgOp::Curve([p[1][0], p[1][1], p[2][0], p[2][1], p[3][0], p[3][1]]));
                last = Some(p[3]);
                i += 3;
            }
            if !path.closed {
                oper = SvgOp::Stroke;
            }
            if let Paint::Color(c) = self.stroke {
                let (r, g, b) = rgb(c);
                out.push(SvgOp::LineWidth(self.stroke_width));
                out.push(SvgOp::StrokeRgb(r, g, b));
            }
            if let Paint::Color(c) = self.fill {
                let (r, g, b) = rgb(c);
                out.push(SvgOp::FillRgb(r, g, b));
                oper = if self.even_odd { SvgOp::FillEvenOdd } else { SvgOp::Fill };
                if self.stroke != Paint::None {
                    oper = SvgOp::FillStroke;
                } else {
                    out.push(SvgOp::Close);
                }
            }
        }
        out.push(oper);
        out
    }
}

struct Parser {
    attr: Vec<Attrib>,
    pts: Vec<[f32; 2]>,
    plist: Vec<Path>,
    shapes: Vec<Shape>,
    width: f32,
    height: f32,
    view: [f32; 4],
    align: (u8, u8, u8),
    dpi: f32,
    defs: bool,
}

const ALIGN_MIN: u8 = 0;
const ALIGN_MAX: u8 = 2;
const ALIGN_NONE: u8 = 0;
const ALIGN_MEET: u8 = 1;
const ALIGN_SLICE: u8 = 2;

impl Parser {
    fn new(dpi: f32) -> Self {
        let root = Attrib {
            xform: IDENTITY,
            fill_color: 0,
            stroke_color: 0,
            stroke_width: 1.0,
            even_odd: false,
            font_size: 0.0,
            has_fill: 1,
            has_stroke: 0,
        };
        Self {
            attr: vec![root],
            pts: Vec::new(),
            plist: Vec::new(),
            shapes: Vec::new(),
            width: 0.0,
            height: 0.0,
            view: [0.0; 4],
            align: (0, 0, ALIGN_NONE),
            dpi,
            defs: false,
        }
    }

    fn attr(&mut self) -> &mut Attrib {
        self.attr.last_mut().unwrap()
    }

    fn push_attr(&mut self) {
        if self.attr.len() < 128 {
            let top = self.attr.last().unwrap().clone();
            self.attr.push(top);
        }
    }

    fn pop_attr(&mut self) {
        if self.attr.len() > 1 {
            self.attr.pop();
        }
    }

    fn parse_xml(&mut self, input: &str) {
        let mut in_tag = false;
        let mut mark = 0;
        for (i, c) in input.char_indices() {
            if c == '<' && !in_tag {
                mark = i + 1;
                in_tag = true;
            } else if c == '>' && in_tag {
                self.element(&input[mark..i]);
                in_tag = false;
            }
        }
    }

    fn element(&mut self, s: &str) {
        let s = s.trim_start_matches(is_space);
        let (s, end_tag) = match s.strip_prefix('/') {
            Some(rest) => (rest, true),
            None => (s, false),
        };
        if s.is_empty() || s.starts_with(['?', '!']) {
            return;
        }
        let name_end = s.find(is_space).unwrap_or(s.len());
        let name = &s[..name_end];
        let mut rest = s.get(name_end + 1..).unwrap_or("");
        let mut attrs = Vec::new();
        let mut end = end_tag;
        while !end && !rest.is_empty() {
            rest = rest.trim_start_matches(is_space);
            if rest.is_empty() {
                break;
            }
            if rest.starts_with('/') {
                end = true;
                break;
            }
            let key_end = rest.find(|c: char| is_space(c) || c == '=').unwrap_or(rest.len());
            let key = &rest[..key_end];
            rest = rest.get(key_end + 1..).unwrap_or("");
            let Some(q) = rest.find(['"', '\'']) else {
                attrs.push((key, ""));
                break;
            };
            let quote = rest.as_bytes()[q] as char;
            rest = &rest[q + 1..];
            let v_end = rest.find(quote).unwrap_or(rest.len());
            attrs.push((key, &rest[..v_end]));
            rest = rest.get(v_end + 1..).unwrap_or("");
        }
        if !end_tag {
            self.start_element(name, &attrs);
        }
        if end {
            self.end_element(name);
        }
    }

    fn start_element(&mut self, el: &str, attrs: &[(&str, &str)]) {
        if self.defs && !matches!(el, "linearGradient" | "radialGradient" | "stop") {
            return;
        }
        match el {
            "g" => {
                self.push_attr();
                self.parse_attribs(attrs);
            }
            "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" => {
                self.push_attr();
                match el {
                    "path" => self.parse_path(attrs),
                    "rect" => self.parse_rect(attrs),
                    "circle" => self.parse_ellipse(attrs, true),
                    "ellipse" => self.parse_ellipse(attrs, false),
                    "line" => self.parse_line(attrs),
                    _ => self.parse_poly(attrs, el == "polygon"),
                }
                self.pop_attr();
            }
            "linearGradient" | "radialGradient" | "stop" => {
                for (k, v) in attrs {
                    if *k != "id" {
                        self.parse_attr(k, v);
                    }
                }
            }
            "defs" => self.defs = true,
            "svg" => self.parse_svg(attrs),
            _ => {}
        }
    }

    fn end_element(&mut self, el: &str) {
        match el {
            "g" => self.pop_attr(),
            "defs" => self.defs = false,
            _ => {}
        }
    }

    fn actual_length(&self) -> f32 {
        let (w, h) = (self.view[2], self.view[3]);
        (w * w + h * h).sqrt() / 2.0f32.sqrt()
    }

    fn to_pixels(&self, (value, units): (f32, Units), orig: f32, length: f32) -> f32 {
        let attr = self.attr.last().unwrap();
        match units {
            Units::User | Units::Px => value,
            Units::Pt => value / 72.0 * self.dpi,
            Units::Pc => value / 6.0 * self.dpi,
            Units::Mm => value / 25.4 * self.dpi,
            Units::Cm => value / 2.54 * self.dpi,
            Units::In => value * self.dpi,
            Units::Em => value * attr.font_size,
            Units::Ex => value * attr.font_size * 0.52,
            Units::Percent => orig + value / 100.0 * length,
        }
    }

    fn coordinate(&self, s: &str, orig: f32, length: f32) -> f32 {
        self.to_pixels(coordinate_raw(s), orig, length)
    }

    fn parse_attribs(&mut self, attrs: &[(&str, &str)]) {
        for (k, v) in attrs {
            self.parse_attr(k, v);
        }
    }

    fn parse_attr(&mut self, name: &str, value: &str) -> bool {
        match name {
            "style" => self.parse_style(value),
            "display" | "opacity" | "fill-opacity" | "stroke-opacity" | "stroke-dasharray" | "stroke-dashoffset"
            | "stroke-linecap" | "stroke-linejoin" | "stop-color" | "stop-opacity" | "offset" | "id" => {}
            "fill" => {
                let (has, color) = paint(value);
                let attr = self.attr();
                attr.has_fill = has;
                if let Some(c) = color {
                    attr.fill_color = c;
                }
            }
            "stroke" => {
                let (has, color) = paint(value);
                let attr = self.attr();
                attr.has_stroke = has;
                if let Some(c) = color {
                    attr.stroke_color = c;
                }
            }
            "stroke-width" => {
                let w = self.coordinate(value, 0.0, self.actual_length());
                self.attr().stroke_width = w;
            }
            "fill-rule" => self.attr().even_odd = value == "evenodd",
            "font-size" => {
                let size = self.coordinate(value, 0.0, self.actual_length());
                self.attr().font_size = size;
            }
            "transform" => {
                let t = parse_transform(value);
                let attr = self.attr();
                attr.xform = multiply(t, attr.xform);
            }
            _ => return false,
        }
        true
    }

    fn parse_style(&mut self, s: &str) {
        for decl in s.split(';') {
            let decl = decl.trim_matches(is_space);
            let (name, value) = match decl.find(':') {
                Some(i) => (decl[..i].trim_end_matches(is_space), decl[i..].trim_start_matches(|c| c == ':' || is_space(c))),
                None => (decl, ""),
            };
            self.parse_attr(name, value);
        }
    }

    fn move_to(&mut self, x: f32, y: f32) {
        match self.pts.last_mut() {
            Some(p) => *p = [x, y],
            None => self.pts.push([x, y]),
        }
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(&[px, py]) = self.pts.last() {
            let (dx, dy) = (x - px, y - py);
            self.pts.push([px + dx / 3.0, py + dy / 3.0]);
            self.pts.push([x - dx / 3.0, y - dy / 3.0]);
            self.pts.push([x, y]);
        }
    }

    fn cubic_to(&mut self, c: [f32; 6]) {
        self.pts.extend([[c[0], c[1]], [c[2], c[3]], [c[4], c[5]]]);
    }

    fn add_path(&mut self, closed: bool) {
        if self.pts.len() < 4 {
            return;
        }
        if closed {
            let [x, y] = self.pts[0];
            self.line_to(x, y);
        }
        let xform = self.attr.last().unwrap().xform;
        let pts: Vec<[f32; 2]> = self.pts.iter().map(|&[x, y]| xform_point(x, y, &xform)).collect();
        let mut bounds = [0.0; 4];
        let mut i = 0;
        while i + 1 < pts.len() {
            let b = curve_bounds(&pts[i..i + 4]);
            bounds = if i == 0 { b } else { union(bounds, b) };
            i += 3;
        }
        self.plist.insert(0, Path { pts, closed, bounds });
    }

    fn add_shape(&mut self) {
        if self.plist.is_empty() {
            return;
        }
        let attr = self.attr.last().unwrap();
        let paths = std::mem::take(&mut self.plist);
        let bounds = paths.iter().skip(1).fold(paths[0].bounds, |b, p| union(b, p.bounds));
        self.shapes.push(Shape {
            fill: if attr.has_fill == 1 { Paint::Color(attr.fill_color) } else { Paint::None },
            stroke: if attr.has_stroke == 1 { Paint::Color(attr.stroke_color) } else { Paint::None },
            stroke_width: attr.stroke_width * average_scale(&attr.xform),
            even_odd: attr.even_odd,
            bounds,
            paths,
        });
    }

    fn parse_path(&mut self, attrs: &[(&str, &str)]) {
        let mut d = None;
        for (k, v) in attrs {
            if *k == "d" {
                d = Some(*v);
            } else {
                self.parse_attribs(&[(k, v)]);
            }
        }
        if let Some(mut s) = d {
            self.pts.clear();
            let (mut cpx, mut cpy, mut cpx2, mut cpy2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut closed = false;
            let mut cmd = '\0';
            let mut args = [0.0f32; 10];
            let mut nargs = 0;
            let mut rargs = 0;
            loop {
                let (item, rest) = next_path_item(s);
                s = rest;
                let Some(item) = item else { break };
                let first = item.chars().next().unwrap();
                if "0123456789+-.eE".contains(first) {
                    if nargs < 10 {
                        args[nargs] = atof(item);
                        nargs += 1;
                    }
                    if nargs >= rargs {
                        let rel = cmd.is_ascii_lowercase();
                        let a = &args;
                        match cmd {
                            'm' | 'M' => {
                                (cpx, cpy) = if rel { (cpx + a[0], cpy + a[1]) } else { (a[0], a[1]) };
                                self.move_to(cpx, cpy);
                                cmd = if rel { 'l' } else { 'L' };
                                rargs = args_per_element(cmd);
                                (cpx2, cpy2) = (cpx, cpy);
                            }
                            'l' | 'L' | 'h' | 'H' | 'v' | 'V' => {
                                match cmd {
                                    'l' | 'L' => (cpx, cpy) = if rel { (cpx + a[0], cpy + a[1]) } else { (a[0], a[1]) },
                                    'h' | 'H' => cpx = if rel { cpx + a[0] } else { a[0] },
                                    _ => cpy = if rel { cpy + a[0] } else { a[0] },
                                }
                                self.line_to(cpx, cpy);
                                (cpx2, cpy2) = (cpx, cpy);
                            }
                            'c' | 'C' => {
                                let o = if rel { (cpx, cpy) } else { (0.0, 0.0) };
                                let c = [o.0 + a[0], o.1 + a[1], o.0 + a[2], o.1 + a[3], o.0 + a[4], o.1 + a[5]];
                                self.cubic_to(c);
                                (cpx2, cpy2, cpx, cpy) = (c[2], c[3], c[4], c[5]);
                            }
                            's' | 'S' => {
                                let o = if rel { (cpx, cpy) } else { (0.0, 0.0) };
                                let (cx2, cy2, x2, y2) = (o.0 + a[0], o.1 + a[1], o.0 + a[2], o.1 + a[3]);
                                self.cubic_to([2.0 * cpx - cpx2, 2.0 * cpy - cpy2, cx2, cy2, x2, y2]);
                                (cpx2, cpy2, cpx, cpy) = (cx2, cy2, x2, y2);
                            }
                            'q' | 'Q' | 't' | 'T' => {
                                let o = if rel { (cpx, cpy) } else { (0.0, 0.0) };
                                let (x1, y1) = (cpx, cpy);
                                let (cx, cy, x2, y2) = if cmd.eq_ignore_ascii_case(&'q') {
                                    (o.0 + a[0], o.1 + a[1], o.0 + a[2], o.1 + a[3])
                                } else {
                                    (2.0 * x1 - cpx2, 2.0 * y1 - cpy2, o.0 + a[0], o.1 + a[1])
                                };
                                self.cubic_to([
                                    x1 + 2.0 / 3.0 * (cx - x1),
                                    y1 + 2.0 / 3.0 * (cy - y1),
                                    x2 + 2.0 / 3.0 * (cx - x2),
                                    y2 + 2.0 / 3.0 * (cy - y2),
                                    x2,
                                    y2,
                                ]);
                                (cpx2, cpy2, cpx, cpy) = (cx, cy, x2, y2);
                            }
                            'a' | 'A' => {
                                (cpx, cpy) = self.arc_to(cpx, cpy, a, rel);
                                (cpx2, cpy2) = (cpx, cpy);
                            }
                            _ => {
                                if nargs >= 2 {
                                    (cpx, cpy) = (a[nargs - 2], a[nargs - 1]);
                                    (cpx2, cpy2) = (cpx, cpy);
                                }
                            }
                        }
                        nargs = 0;
                    }
                } else {
                    cmd = first;
                    rargs = args_per_element(cmd);
                    if cmd == 'M' || cmd == 'm' {
                        if !self.pts.is_empty() {
                            self.add_path(closed);
                        }
                        self.pts.clear();
                        closed = false;
                        nargs = 0;
                    } else if cmd == 'Z' || cmd == 'z' {
                        if !self.pts.is_empty() {
                            [cpx, cpy] = self.pts[0];
                            (cpx2, cpy2) = (cpx, cpy);
                            self.add_path(true);
                        }
                        self.pts.clear();
                        self.move_to(cpx, cpy);
                        closed = false;
                        nargs = 0;
                    }
                }
            }
            if !self.pts.is_empty() {
                self.add_path(closed);
            }
        }
        self.add_shape();
    }

    fn arc_to(&mut self, cpx: f32, cpy: f32, args: &[f32; 10], rel: bool) -> (f32, f32) {
        let (mut rx, mut ry) = (args[0].abs(), args[1].abs());
        let rotx = args[2] / 180.0 * PI;
        let fa = args[3].abs() > 1e-6;
        let fs = args[4].abs() > 1e-6;
        let (x1, y1) = (cpx, cpy);
        let (x2, y2) = if rel { (cpx + args[5], cpy + args[6]) } else { (args[5], args[6]) };
        let (dx, dy) = (x1 - x2, y1 - y2);
        let d = (dx * dx + dy * dy).sqrt();
        if d < 1e-6 || rx < 1e-6 || ry < 1e-6 {
            self.line_to(x2, y2);
            return (x2, y2);
        }
        let (sinrx, cosrx) = (rotx.sin(), rotx.cos());
        let x1p = cosrx * dx / 2.0 + sinrx * dy / 2.0;
        let y1p = -sinrx * dx / 2.0 + cosrx * dy / 2.0;
        let sq = |v: f32| v * v;
        let d = sq(x1p) / sq(rx) + sq(y1p) / sq(ry);
        if d > 1.0 {
            let d = d.sqrt();
            rx *= d;
            ry *= d;
        }
        let mut s = 0.0;
        let sa = (sq(rx) * sq(ry) - sq(rx) * sq(y1p) - sq(ry) * sq(x1p)).max(0.0);
        let sb = sq(rx) * sq(y1p) + sq(ry) * sq(x1p);
        if sb > 0.0 {
            s = (sa / sb).sqrt();
        }
        if fa == fs {
            s = -s;
        }
        let cxp = s * rx * y1p / ry;
        let cyp = s * -ry * x1p / rx;
        let cx = (x1 + x2) / 2.0 + cosrx * cxp - sinrx * cyp;
        let cy = (y1 + y2) / 2.0 + sinrx * cxp + cosrx * cyp;
        let ux = (x1p - cxp) / rx;
        let uy = (y1p - cyp) / ry;
        let vx = (-x1p - cxp) / rx;
        let vy = (-y1p - cyp) / ry;
        let a1 = vecang(1.0, 0.0, ux, uy);
        let mut da = vecang(ux, uy, vx, vy);
        if fa {
            da = if da > 0.0 { da - 2.0 * PI } else { 2.0 * PI + da };
        }
        let t = [cosrx, sinrx, -sinrx, cosrx, cx, cy];
        let ndivs = (da.abs() / (PI * 0.5) + 1.0) as i32;
        let hda = (da / ndivs as f32) / 2.0;
        let mut kappa = (4.0 / 3.0 * (1.0 - hda.cos()) / hda.sin()).abs();
        if da < 0.0 {
            kappa = -kappa;
        }
        let (mut px, mut py, mut ptanx, mut ptany) = (0.0, 0.0, 0.0, 0.0);
        for i in 0..=ndivs {
            let a = a1 + da * (i as f32 / ndivs as f32);
            let (dx, dy) = (a.cos(), a.sin());
            let [x, y] = xform_point(dx * rx, dy * ry, &t);
            let (vx, vy) = (-dy * rx * kappa, dx * ry * kappa);
            let (tanx, tany) = (vx * t[0] + vy * t[2], vx * t[1] + vy * t[3]);
            if i > 0 {
                self.cubic_to([px + ptanx, py + ptany, x - tanx, y - tany, x, y]);
            }
            (px, py, ptanx, ptany) = (x, y, tanx, tany);
        }
        (x2, y2)
    }

    fn other_attrs<'a>(&mut self, attrs: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        attrs.iter().filter(|(k, v)| !self.parse_attr(k, v)).copied().collect()
    }

    fn parse_rect(&mut self, attrs: &[(&str, &str)]) {
        let (mut x, mut y, mut w, mut h, mut rx, mut ry) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, -1.0f32, -1.0f32);
        let [ox, oy, vw, vh] = self.view;
        for (k, v) in self.other_attrs(attrs) {
            match k {
                "x" => x = self.coordinate(v, ox, vw),
                "y" => y = self.coordinate(v, oy, vh),
                "width" => w = self.coordinate(v, 0.0, vw),
                "height" => h = self.coordinate(v, 0.0, vh),
                "rx" => rx = self.coordinate(v, 0.0, vw).abs(),
                "ry" => ry = self.coordinate(v, 0.0, vh).abs(),
                _ => {}
            }
        }
        if rx < 0.0 && ry > 0.0 {
            rx = ry;
        }
        if ry < 0.0 && rx > 0.0 {
            ry = rx;
        }
        rx = rx.max(0.0).min_c(w / 2.0);
        ry = ry.max(0.0).min_c(h / 2.0);
        if w != 0.0 && h != 0.0 {
            self.pts.clear();
            if rx < 0.00001 || ry < 0.0001 {
                self.move_to(x, y);
                self.line_to(x + w, y);
                self.line_to(x + w, y + h);
                self.line_to(x, y + h);
            } else {
                let k = 1.0 - KAPPA90;
                self.move_to(x + rx, y);
                self.line_to(x + w - rx, y);
                self.cubic_to([x + w - rx * k, y, x + w, y + ry * k, x + w, y + ry]);
                self.line_to(x + w, y + h - ry);
                self.cubic_to([x + w, y + h - ry * k, x + w - rx * k, y + h, x + w - rx, y + h]);
                self.line_to(x + rx, y + h);
                self.cubic_to([x + rx * k, y + h, x, y + h - ry * k, x, y + h - ry]);
                self.line_to(x, y + ry);
                self.cubic_to([x, y + ry * k, x + rx * k, y, x + rx, y]);
            }
            self.add_path(true);
            self.add_shape();
        }
    }

    fn parse_ellipse(&mut self, attrs: &[(&str, &str)], circle: bool) {
        let (mut cx, mut cy, mut rx, mut ry) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let [ox, oy, vw, vh] = self.view;
        let len = self.actual_length();
        for (k, v) in self.other_attrs(attrs) {
            match (k, circle) {
                ("cx", _) => cx = self.coordinate(v, ox, vw),
                ("cy", _) => cy = self.coordinate(v, oy, vh),
                ("r", true) => {
                    rx = self.coordinate(v, 0.0, len).abs();
                    ry = rx;
                }
                ("rx", false) => rx = self.coordinate(v, 0.0, vw).abs(),
                ("ry", false) => ry = self.coordinate(v, 0.0, vh).abs(),
                _ => {}
            }
        }
        if rx > 0.0 && ry > 0.0 {
            self.pts.clear();
            let k = KAPPA90;
            self.move_to(cx + rx, cy);
            self.cubic_to([cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry]);
            self.cubic_to([cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy]);
            self.cubic_to([cx - rx, cy - ry * k, cx - rx * k, cy - ry, cx, cy - ry]);
            self.cubic_to([cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy]);
            self.add_path(true);
            self.add_shape();
        }
    }

    fn parse_line(&mut self, attrs: &[(&str, &str)]) {
        let (mut x1, mut y1, mut x2, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let [ox, oy, vw, vh] = self.view;
        for (k, v) in self.other_attrs(attrs) {
            match k {
                "x1" => x1 = self.coordinate(v, ox, vw),
                "y1" => y1 = self.coordinate(v, oy, vh),
                "x2" => x2 = self.coordinate(v, ox, vw),
                "y2" => y2 = self.coordinate(v, oy, vh),
                _ => {}
            }
        }
        self.pts.clear();
        self.move_to(x1, y1);
        self.line_to(x2, y2);
        self.add_path(false);
        self.add_shape();
    }

    fn parse_poly(&mut self, attrs: &[(&str, &str)], closed: bool) {
        self.pts.clear();
        let mut npts = 0;
        for (k, v) in self.other_attrs(attrs) {
            if k != "points" {
                continue;
            }
            let mut s = v;
            let mut args = [0.0f32; 2];
            let mut nargs = 0;
            while !s.is_empty() {
                let (item, rest) = next_path_item(s);
                s = rest;
                args[nargs] = item.map_or(0.0, atof);
                nargs += 1;
                if nargs >= 2 {
                    if npts == 0 {
                        self.move_to(args[0], args[1]);
                    } else {
                        self.line_to(args[0], args[1]);
                    }
                    nargs = 0;
                    npts += 1;
                }
            }
        }
        self.add_path(closed);
        self.add_shape();
    }

    fn parse_svg(&mut self, attrs: &[(&str, &str)]) {
        for (k, v) in self.other_attrs(attrs) {
            match k {
                "width" => self.width = self.coordinate(v, 0.0, 1.0),
                "height" => self.height = self.coordinate(v, 0.0, 1.0),
                "viewBox" => {
                    let mut s = v;
                    for (i, slot) in self.view.iter_mut().enumerate() {
                        if i > 0 {
                            let skipped = s.trim_start_matches(['%', ',', ' ', '\t']);
                            if skipped.len() == s.len() {
                                break;
                            }
                            s = skipped;
                        }
                        match scan_float(s) {
                            Some((f, rest)) => {
                                *slot = f;
                                s = rest;
                            }
                            None => break,
                        }
                    }
                }
                "preserveAspectRatio" => {
                    if v.contains("none") {
                        self.align.2 = ALIGN_NONE;
                    } else {
                        let pick = |min: &str, mid: &str, max: &str, cur: u8| {
                            if v.contains(min) {
                                ALIGN_MIN
                            } else if v.contains(mid) {
                                1
                            } else if v.contains(max) {
                                ALIGN_MAX
                            } else {
                                cur
                            }
                        };
                        self.align.0 = pick("xMin", "xMid", "xMax", self.align.0);
                        self.align.1 = pick("yMin", "yMid", "yMax", self.align.1);
                        self.align.2 = if v.contains("slice") { ALIGN_SLICE } else { ALIGN_MEET };
                    }
                }
                _ => {}
            }
        }
    }

    fn scale_to_viewbox(&mut self) {
        let bounds = match self.shapes.split_first() {
            None => [0.0; 4],
            Some((first, rest)) => rest.iter().fold(first.bounds, |b, s| union(b, s.bounds)),
        };
        if self.view[2] == 0.0 {
            if self.width > 0.0 {
                self.view[2] = self.width;
            } else {
                self.view[0] = bounds[0];
                self.view[2] = bounds[2] - bounds[0];
            }
        }
        if self.view[3] == 0.0 {
            if self.height > 0.0 {
                self.view[3] = self.height;
            } else {
                self.view[1] = bounds[1];
                self.view[3] = bounds[3] - bounds[1];
            }
        }
        if self.width == 0.0 {
            self.width = self.view[2];
        }
        if self.height == 0.0 {
            self.height = self.view[3];
        }
        let [minx, miny, vw, vh] = self.view;
        let (mut tx, mut ty) = (-minx, -miny);
        let mut sx = if vw > 0.0 { self.width / vw } else { 0.0 };
        let mut sy = if vh > 0.0 { self.height / vh } else { 0.0 };
        let us = 1.0 / self.to_pixels((1.0, Units::Pt), 0.0, 1.0);
        let align = |content: f32, container: f32, kind: u8| match kind {
            ALIGN_MIN => 0.0,
            ALIGN_MAX => container - content,
            _ => (container - content) * 0.5,
        };
        if self.align.2 == ALIGN_MEET || self.align.2 == ALIGN_SLICE {
            let s = if self.align.2 == ALIGN_MEET { sx.min_c(sy) } else { sx.max_c(sy) };
            (sx, sy) = (s, s);
            tx += align(vw * sx, self.width, self.align.0) / sx;
            ty += align(vh * sy, self.height, self.align.1) / sy;
        }
        sx *= us;
        sy *= us;
        let avgs = (sx + sy) / 2.0;
        for shape in &mut self.shapes {
            for path in &mut shape.paths {
                for p in &mut path.pts {
                    *p = [(p[0] + tx) * sx, (p[1] + ty) * sy];
                }
            }
            shape.stroke_width *= avgs;
        }
    }
}

/// nanosvg's `fminf`/`fmaxf`, which keep the first argument on ties and NaN.
trait MinMax {
    fn min_c(self, other: Self) -> Self;
    fn max_c(self, other: Self) -> Self;
}

impl MinMax for f32 {
    fn min_c(self, other: f32) -> f32 {
        if self < other { self } else { other }
    }
    fn max_c(self, other: f32) -> f32 {
        if self > other { self } else { other }
    }
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `t` followed by `s`.
fn multiply(t: [f32; 6], s: [f32; 6]) -> [f32; 6] {
    [
        t[0] * s[0] + t[1] * s[2],
        t[0] * s[1] + t[1] * s[3],
        t[2] * s[0] + t[3] * s[2],
        t[2] * s[1] + t[3] * s[3],
        t[4] * s[0] + t[5] * s[2] + s[4],
        t[4] * s[1] + t[5] * s[3] + s[5],
    ]
}

fn xform_point(x: f32, y: f32, t: &[f32; 6]) -> [f32; 2] {
    [x * t[0] + y * t[2] + t[4], x * t[1] + y * t[3] + t[5]]
}

fn average_scale(t: &[f32; 6]) -> f32 {
    let sx = (t[0] * t[0] + t[2] * t[2]).sqrt();
    let sy = (t[1] * t[1] + t[3] * t[3]).sqrt();
    (sx + sy) * 0.5
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0].min_c(b[0]), a[1].min_c(b[1]), a[2].max_c(b[2]), a[3].max_c(b[3])]
}

fn curve_bounds(c: &[[f32; 2]]) -> [f32; 4] {
    let (v0, v1, v2, v3) = (c[0], c[1], c[2], c[3]);
    let mut b = [v0[0].min_c(v3[0]), v0[1].min_c(v3[1]), v0[0].max_c(v3[0]), v0[1].max_c(v3[1])];
    let inside = |p: [f32; 2], b: &[f32; 4]| p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3];
    if inside(v1, &b) && inside(v2, &b) {
        return b;
    }
    const EPS: f64 = 1e-12;
    for i in 0..2 {
        let (p0, p1, p2, p3) = (v0[i] as f64, v1[i] as f64, v2[i] as f64, v3[i] as f64);
        let a = -3.0 * p0 + 9.0 * p1 - 9.0 * p2 + 3.0 * p3;
        let bb = 6.0 * p0 - 12.0 * p1 + 6.0 * p2;
        let c = 3.0 * p1 - 3.0 * p0;
        let mut roots = Vec::new();
        if a.abs() < EPS {
            if bb.abs() > EPS {
                roots.push(-c / bb);
            }
        } else {
            let disc = bb * bb - 4.0 * c * a;
            if disc > EPS {
                roots.push((-bb + disc.sqrt()) / (2.0 * a));
                roots.push((-bb - disc.sqrt()) / (2.0 * a));
            }
        }
        for t in roots.into_iter().filter(|t| *t > EPS && *t < 1.0 - EPS) {
            let it = 1.0 - t;
            let v = (it * it * it * p0 + 3.0 * it * it * t * p1 + 3.0 * it * t * t * p2 + t * t * t * p3) as f32;
            b[i] = b[i].min_c(v);
            b[2 + i] = b[2 + i].max_c(v);
        }
    }
    b
}

fn vecang(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    let mag = |x: f32, y: f32| (x * x + y * y).sqrt();
    let r = ((ux * vx + uy * vy) / (mag(ux, uy) * mag(vx, vy))).clamp(-1.0, 1.0);
    (if ux * vy < uy * vx { -1.0 } else { 1.0 }) * r.acos()
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

fn args_per_element(cmd: char) -> usize {
    match cmd {
        'v' | 'V' | 'h' | 'H' => 1,
        'm' | 'M' | 'l' | 'L' | 't' | 'T' => 2,
        'q' | 'Q' | 's' | 'S' => 4,
        'c' | 'C' => 6,
        'a' | 'A' => 7,
        _ => 0,
    }
}

/// The number at the start of `s`, as nanosvg reads it: sign, digits,
/// fraction and exponent, each optional.
fn number_prefix(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = 0;
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    if i < b.len() && matches!(b[i], b'-' | b'+') {
        i += 1;
    }
    i = digits(i);
    if i < b.len() && b[i] == b'.' {
        i = digits(i + 1);
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        i += 1;
        if i < b.len() && matches!(b[i], b'-' | b'+') {
            i += 1;
        }
        i = digits(i);
    }
    &s[..i]
}

fn next_path_item(s: &str) -> (Option<&str>, &str) {
    let s = s.trim_start_matches(|c| is_space(c) || c == ',');
    let Some(c) = s.chars().next() else { return (None, s) };
    if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
        let n = number_prefix(s);
        (Some(n), &s[n.len()..])
    } else {
        (Some(&s[..c.len_utf8()]), &s[c.len_utf8()..])
    }
}

/// C's `atof`: the longest number at the start of `s`, or 0.
fn atof(s: &str) -> f32 {
    strtod_prefix(s).map_or(0.0, |(v, _)| v as f32)
}

/// C's `strtod`, skipping leading space: the value and what follows it.
fn strtod_prefix(s: &str) -> Option<(f64, &str)> {
    let s = s.trim_start_matches(is_space);
    let mut n = number_prefix(s);
    while !n.is_empty() {
        if let Ok(v) = n.parse::<f64>() {
            return Some((v, &s[n.len()..]));
        }
        n = &n[..n.len() - 1];
    }
    None
}

/// `sscanf("%f")`.
fn scan_float(s: &str) -> Option<(f32, &str)> {
    let s = s.trim_start_matches(is_space);
    let mut n = number_prefix(s);
    while !n.is_empty() {
        if let Ok(v) = n.parse::<f32>() {
            return Some((v, &s[n.len()..]));
        }
        n = &n[..n.len() - 1];
    }
    None
}

fn coordinate_raw(s: &str) -> (f32, Units) {
    let Some((value, rest)) = scan_float(s) else { return (0.0, Units::User) };
    let u = rest.trim_start_matches(is_space);
    let units = match u.get(..2).unwrap_or(u.get(..1).unwrap_or("")) {
        "px" => Units::Px,
        "pt" => Units::Pt,
        "pc" => Units::Pc,
        "mm" => Units::Mm,
        "cm" => Units::Cm,
        "in" => Units::In,
        "em" => Units::Em,
        "ex" => Units::Ex,
        x if x.starts_with('%') => Units::Percent,
        _ => Units::User,
    };
    (value, units)
}

/// How a `fill` or `stroke` value paints: 0 none, 1 a colour, 2 a gradient.
fn paint(value: &str) -> (u8, Option<u32>) {
    if value == "none" {
        (0, None)
    } else if value.starts_with("url(") {
        (2, None)
    } else {
        (1, Some(parse_color(value)))
    }
}

fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

fn parse_color(s: &str) -> u32 {
    let s = s.trim_start_matches(' ');
    if let Some(hex) = s.strip_prefix('#') {
        let digits = &hex[..hex.find(is_space).unwrap_or(hex.len())];
        let lead = |d: &str| {
            let end = d.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(d.len());
            u32::from_str_radix(&d[..end], 16).unwrap_or(0)
        };
        let mut c = match digits.len() {
            6 => lead(hex),
            3 => {
                let c = lead(hex);
                let c = (c & 0xf) | ((c & 0xf0) << 4) | ((c & 0xf00) << 8);
                c | (c << 4)
            }
            _ => 0,
        };
        c &= 0xff_ffff;
        return rgb((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
    }
    if let Some(args) = s.strip_prefix("rgb(") {
        let mut vals = Vec::new();
        let mut percent = false;
        let mut rest = args;
        for i in 0..3 {
            let rs = rest.trim_start_matches(is_space);
            let n = number_prefix(rs);
            let int_len = n.find(['.', 'e', 'E']).unwrap_or(n.len());
            let Ok(v) = rs[..int_len].parse::<i32>() else { break };
            vals.push(v);
            rest = &rs[int_len..];
            if i < 2 {
                let sep_end = rest.find(|c| !matches!(c, '%' | ',' | ' ' | '\t')).unwrap_or(rest.len());
                if sep_end == 0 {
                    break;
                }
                if i == 0 {
                    percent = rest[..sep_end].contains('%');
                }
                rest = &rest[sep_end..];
            }
        }
        let get = |i: usize| vals.get(i).copied().unwrap_or(-1);
        let (r, g, b) = (get(0), get(1), get(2));
        let (r, g, b) = if percent { (r * 255 / 100, g * 255 / 100, b * 255 / 100) } else { (r, g, b) };
        return rgb(r as u32, g as u32, b as u32);
    }
    match s {
        "red" => rgb(255, 0, 0),
        "green" => rgb(0, 128, 0),
        "blue" => rgb(0, 0, 255),
        "yellow" => rgb(255, 255, 0),
        "cyan" => rgb(0, 255, 255),
        "magenta" => rgb(255, 0, 255),
        "black" => rgb(0, 0, 0),
        "white" => rgb(255, 255, 255),
        _ => rgb(128, 128, 128),
    }
}

fn parse_transform(s: &str) -> [f32; 6] {
    let mut xform = IDENTITY;
    let mut rest = s;
    while !rest.is_empty() {
        let kinds = ["matrix", "translate", "scale", "rotate", "skewX", "skewY"];
        let Some(kind) = kinds.iter().find(|k| rest.starts_with(*k)) else {
            rest = &rest[rest.chars().next().unwrap().len_utf8()..];
            continue;
        };
        let (args, len) = transform_args(rest);
        let deg = |a: f32| a / 180.0 * PI;
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let t = match *kind {
            "matrix" if args.len() == 6 => Some([a(0), a(1), a(2), a(3), a(4), a(5)]),
            "matrix" => None,
            "translate" => Some([1.0, 0.0, 0.0, 1.0, a(0), if args.len() == 1 { 0.0 } else { a(1) }]),
            "scale" => Some([a(0), 0.0, 0.0, if args.len() == 1 { a(0) } else { a(1) }, 0.0, 0.0]),
            "skewX" => Some([1.0, 0.0, deg(a(0)).tan(), 1.0, 0.0, 0.0]),
            "skewY" => Some([1.0, deg(a(0)).tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => {
                let (cs, sn) = (deg(a(0)).cos(), deg(a(0)).sin());
                let mut m = IDENTITY;
                if args.len() > 1 {
                    m = multiply(m, [1.0, 0.0, 0.0, 1.0, -a(1), -a(2)]);
                }
                m = multiply(m, [cs, sn, -sn, cs, 0.0, 0.0]);
                if args.len() > 1 {
                    m = multiply(m, [1.0, 0.0, 0.0, 1.0, a(1), a(2)]);
                }
                Some(m)
            }
        };
        if let Some(t) = t {
            xform = multiply(t, xform);
        }
        rest = &rest[len.max(1).min(rest.len())..];
    }
    xform
}

/// The numbers inside the parentheses at the start of `s`, and how far
/// the closing one is from the start.
fn transform_args(s: &str) -> (Vec<f32>, usize) {
    let (Some(open), Some(close)) = (s.find('('), s.find(')')) else { return (Vec::new(), 1) };
    let mut args = Vec::new();
    let mut p = &s[open..close];
    while let Some(c) = p.chars().next() {
        if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let n = number_prefix(p);
            args.push(atof(n));
            p = &p[n.len().max(1)..];
        } else {
            p = &p[c.len_utf8()..];
        }
    }
    (args, close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_path_fills_like_sile() {
        let svg = SvgImage::parse(r#"<svg viewBox="0 0 150 150"><path d="M 20 20 L 40 20 L 40 40 z" fill="black"/></svg>"#, 72.0);
        assert_eq!((svg.width, svg.height), (150.0, 150.0));
        assert_eq!(
            svg.to_string(),
            "20.000000 20.000000 m 26.666666 20.000000 33.333332 20.000000 40.000000 20.000000 c \
             40.000000 26.666666 40.000000 33.333332 40.000000 40.000000 c \
             33.333332 33.333332 26.666666 26.666666 20.000000 20.000000 c 0.000000 0.000000 0.000000 rg h f "
        );
    }

    #[test]
    fn lines_are_filled_black_unless_told_not_to() {
        let line = r#"<svg width="10pt" height="10pt"><line x1="0" y1="0" x2="10" y2="0" stroke="red"FILL/></svg>"#;
        let stroked = "0.000000 0.000000 m 3.333333 0.000000 6.666667 0.000000 10.000000 0.000000 c 1.000000 w 0.996094 0.000000 0.000000 RG ";
        assert_eq!(SvgImage::parse(&line.replace("FILL", ""), 72.0).to_string(), format!("{stroked}0.000000 0.000000 0.000000 rg B "));
        assert_eq!(SvgImage::parse(&line.replace("FILL", r#" fill="none""#), 72.0).to_string(), format!("{stroked}S "));
    }
}
