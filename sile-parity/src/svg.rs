//! Draws trace pages as SVG using the real glyph outlines, so SILE's expected
//! output and ours can be looked at side by side.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;

use crate::fonts::Fonts;
use crate::trace::{Page, Trace};

/// Glyph outlines shared by every page drawn into one HTML document.
pub struct GlyphDefs<'a> {
    fonts: &'a Fonts,
    faces: BTreeMap<(String, u16, bool), Option<usize>>,
    data: Vec<Arc<Vec<u8>>>,
    used: BTreeMap<(usize, u32), String>,
}

impl<'a> GlyphDefs<'a> {
    pub fn new(fonts: &'a Fonts) -> Self {
        Self {
            fonts,
            faces: BTreeMap::new(),
            data: Vec::new(),
            used: BTreeMap::new(),
        }
    }

    fn face_index(&mut self, family: &str, weight: u16, italic: bool) -> Option<usize> {
        let key = (family.to_string(), weight, italic);
        if let Some(i) = self.faces.get(&key) {
            return *i;
        }
        let idx = self.fonts.data(family, weight, italic).map(|d| {
            self.data.push(d);
            self.data.len() - 1
        });
        self.faces.insert(key, idx);
        idx
    }

    fn face(&self, idx: usize) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(&self.data[idx], 0).ok()
    }

    /// Returns the `<use>` id for a glyph, recording its outline.
    fn glyph(&mut self, face: usize, gid: u32) -> Option<String> {
        let id = format!("g{face}-{gid}");
        if !self.used.contains_key(&(face, gid)) {
            let f = self.face(face)?;
            let mut path = PathBuilder(String::new());
            f.outline_glyph(ttf_parser::GlyphId(gid as u16), &mut path);
            self.used.insert((face, gid), path.0);
        }
        Some(id)
    }

    fn advance(&self, face: usize, gid: u32, size: f64) -> f64 {
        self.face(face)
            .and_then(|f| {
                let upem = f.units_per_em() as f64;
                f.glyph_hor_advance(ttf_parser::GlyphId(gid as u16))
                    .map(|a| a as f64 * size / upem)
            })
            .unwrap_or(size * 0.5)
    }

    fn upem(&self, face: usize) -> f64 {
        self.face(face)
            .map(|f| f.units_per_em() as f64)
            .unwrap_or(1000.0)
    }

    /// A hidden `<svg>` holding every outline used so far.
    pub fn defs(&self) -> String {
        let mut out = String::from("<svg width='0' height='0' style='position:absolute'><defs>");
        for ((face, gid), d) in &self.used {
            let _ = write!(out, "<path id='g{face}-{gid}' d='{d}'/>");
        }
        out.push_str("</defs></svg>");
        out
    }
}

/// The part of the page worth looking at: the content of every given page,
/// padded, so short tests are not lost in a mostly blank sheet.
pub fn view_box(trace: &Trace, pages: &[Option<&Page>]) -> (f64, f64, f64, f64) {
    let (w, h) = trace.paper;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for page in pages.iter().flatten() {
        for r in &page.runs {
            x0 = x0.min(r.x);
            x1 = x1.max(r.x + r.width);
            y0 = y0.min(r.y - r.size);
            y1 = y1.max(r.y + r.size * 0.4);
        }
        for r in &page.rules {
            x0 = x0.min(r.x);
            x1 = x1.max(r.x + r.width);
            y0 = y0.min(r.y);
            y1 = y1.max(r.y + r.depth.abs());
        }
    }
    if x0 > x1 {
        return (0.0, 0.0, w, h);
    }
    let pad = 12.0;
    let (x0, y0) = ((x0 - pad).max(0.0), (y0 - pad).max(0.0));
    let (x1, y1) = ((x1 + pad).min(w), (y1 + pad).min(h));
    (x0, y0, x1 - x0, y1 - y0)
}

fn open_svg(class: &str, vb: (f64, f64, f64, f64)) -> String {
    let (x, y, w, h) = vb;
    format!(
        "<svg class='page {class}' viewBox='{x:.2} {y:.2} {w:.2} {h:.2}' role='img'><rect class='paper' x='{x:.2}' y='{y:.2}' width='{w:.2}' height='{h:.2}'/>"
    )
}

/// One page as a standalone `<svg>`.
pub fn page(page: &Page, vb: (f64, f64, f64, f64), defs: &mut GlyphDefs) -> String {
    format!("{}{}</svg>", open_svg("", vb), layer(page, defs, "ink"))
}

/// SILE's page and ours drawn over each other in two colours.
pub fn overlay(
    expected: Option<&Page>,
    ours: Option<&Page>,
    vb: (f64, f64, f64, f64),
    defs: &mut GlyphDefs,
) -> String {
    let mut out = open_svg("overlay", vb);
    if let Some(p) = expected {
        out.push_str(&layer(p, defs, "sile"));
    }
    if let Some(p) = ours {
        out.push_str(&layer(p, defs, "ours"));
    }
    out.push_str("</svg>");
    out
}

fn layer(page: &Page, defs: &mut GlyphDefs, class: &str) -> String {
    let mut out = format!("<g class='{class}'>");
    for run in &page.runs {
        let Some(face) = defs.face_index(&run.family, run.weight, run.italic) else {
            let _ = write!(
                out,
                "<text x='{:.2}' y='{:.2}' font-size='{:.1}'>{}</text>",
                run.x,
                run.y,
                run.size,
                escape(&run.text)
            );
            continue;
        };
        let scale = run.size / defs.upem(face);
        let _ = write!(out, "<g><title>{}</title>", escape(&run.text));
        let mut x = run.x;
        for (i, gid) in run.gids.iter().enumerate() {
            if let Some(id) = defs.glyph(face, *gid) {
                let _ = write!(
                    out,
                    "<use href='#{id}' transform='translate({x:.3} {:.3}) scale({scale:.6} {:.6})'/>",
                    run.y, -scale
                );
            }
            x += match &run.advances {
                Some(a) => a.get(i).copied().unwrap_or(0.0),
                None => defs.advance(face, *gid, run.size),
            };
        }
        out.push_str("</g>");
    }
    for r in &page.rules {
        let _ = write!(
            out,
            "<rect class='rule' x='{:.2}' y='{:.2}' width='{:.2}' height='{:.2}'/>",
            r.x,
            r.y,
            r.width.max(0.1),
            r.depth.abs().max(0.1)
        );
    }
    out.push_str("</g>");
    out
}

struct PathBuilder(String);

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "M{x} {y}");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "L{x} {y}");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.0, "Q{x1} {y1} {x} {y}");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y}");
    }
    fn close(&mut self) {
        self.0.push('Z');
    }
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}
