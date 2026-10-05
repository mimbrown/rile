//! Draws a `Layout` as SVG: one standalone document per page (or galley
//! surface), glyphs as outlines from the fonts themselves.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;

use rile::builder::Layout;
use rile::color::Color;
use rile::font::{FontFace, Variations};
use rile::frame::FrameGeometry;
use rile::image::{Image, ImageFormat};
use rile::node::{LinkDest, NNode};
use rile::render::{Canvas, draw_pages};
use rile::svg_image::{SvgFigure, SvgOp};
use rile::transform::Matrix;

/// Every page of `layout` as an SVG document, sized in points.
pub fn render(layout: &Layout) -> Vec<String> {
    let fonts = layout.fonts().map(|(key, face, variations)| (key.to_string(), (Arc::clone(face), variations))).collect();
    let mut canvas = SvgCanvas { fonts, ..Default::default() };
    draw_pages(&layout.pages, &mut canvas);
    canvas.pages
}

#[derive(Default)]
struct SvgCanvas {
    fonts: BTreeMap<String, (Arc<FontFace>, Variations)>,
    /// Outlines drawn on the current page, by font key and glyph, with
    /// their ids.
    glyphs: BTreeMap<(String, u16), Option<usize>>,
    defs: String,
    body: String,
    size: (f64, f64),
    pages: Vec<String>,
}

impl SvgCanvas {
    fn glyph_id(&mut self, font_key: &str, gid: u16) -> Option<usize> {
        if let Some(id) = self.glyphs.get(&(font_key.to_string(), gid)) {
            return *id;
        }
        let id = self.fonts.get(font_key).and_then(|(face, variations)| {
            let (data, index) = face.raw_data();
            let mut face = ttf_parser::Face::parse(data, index).ok()?;
            for (tag, value) in variations {
                face.set_variation(ttf_parser::Tag::from_bytes(tag), *value);
            }
            let mut path = PathBuilder(String::new());
            face.outline_glyph(ttf_parser::GlyphId(gid), &mut path)?;
            let id = self.glyphs.len();
            let _ = write!(self.defs, "<path id=\"g{id}\" d=\"{}\"/>", path.0);
            Some(id)
        });
        self.glyphs.insert((font_key.to_string(), gid), id);
        id
    }

    fn path(&mut self, d: &str, fill: Option<Rgb>, stroke: Option<Pen>, even_odd: bool) {
        let _ = write!(self.body, "<path d=\"{d}\"");
        match fill {
            Some(rgb) => {
                let _ = write!(self.body, " fill=\"{}\"", hex(rgb));
                if even_odd {
                    self.body.push_str(" fill-rule=\"evenodd\"");
                }
            }
            None => self.body.push_str(" fill=\"none\""),
        }
        if let Some(Pen { rgb, width, join, cap }) = stroke {
            let join = ["miter", "round", "bevel"][join.min(2) as usize];
            let cap = ["butt", "round", "square"][cap.min(2) as usize];
            let _ = write!(self.body, " stroke=\"{}\" stroke-width=\"{width}\" stroke-linejoin=\"{join}\" stroke-linecap=\"{cap}\"", hex(rgb));
        }
        self.body.push_str("/>");
    }
}

impl Canvas for SvgCanvas {
    fn begin_page(&mut self, width: f64, height: f64) {
        self.size = (width, height);
        self.glyphs.clear();
        self.defs.clear();
        self.body.clear();
    }

    fn end_page(&mut self) {
        let (w, h) = self.size;
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">",
            num(w),
            num(h),
            num(w),
            num(h)
        );
        if !self.defs.is_empty() {
            let _ = write!(out, "<defs>{}</defs>", self.defs);
        }
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        self.pages.push(out);
    }

    fn glyphs(&mut self, nnode: &NNode, x: f64, baseline_y: f64) {
        let Some((face, _)) = self.fonts.get(&*nnode.font_key) else { return };
        let scale = nnode.font_size / face.units_per_em() as f64;
        let _ = write!(self.body, "<g fill=\"{}\">", css_color(nnode.color.unwrap_or(Color::Grayscale { l: 0.0 })));
        let (mut pen_x, mut pen_y) = (x, baseline_y);
        for glyph in &nnode.glyphs {
            if let Some(id) = self.glyph_id(&nnode.font_key, glyph.gid) {
                let _ = write!(
                    self.body,
                    "<use xlink:href=\"#g{id}\" transform=\"matrix({} 0 0 {} {} {})\"/>",
                    num(scale),
                    num(-scale),
                    num(pen_x + glyph.x_offset),
                    num(pen_y - glyph.y_offset)
                );
            }
            if nnode.vertical {
                pen_y += glyph.width;
            } else {
                pen_x += glyph.width;
            }
        }
        self.body.push_str("</g>");
    }

    fn rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let _ = write!(self.body, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>", num(x), num(y), num(width), num(height));
    }

    fn frame_outline(&mut self, frame: &FrameGeometry) {
        let _ = write!(
            self.body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#c00\" stroke-width=\"0.5\"/>",
            num(frame.left),
            num(frame.top),
            num(frame.width()),
            num(frame.height())
        );
    }

    fn push_color(&mut self, color: Color) {
        let _ = write!(self.body, "<g fill=\"{}\">", css_color(color));
    }

    fn pop_color(&mut self) {
        self.body.push_str("</g>");
    }

    fn destination(&mut self, name: &str, x: f64, y: f64) {
        let _ = write!(self.body, "<g id=\"{}\" transform=\"translate({} {})\"/>", escape(name), num(x), num(y));
    }

    fn image(&mut self, image: &Image, x: f64, y: f64, width: f64, height: f64) {
        let mime = match image.format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            // SVG has no way to show a PDF page.
            ImageFormat::Pdf => return,
        };
        let _ = write!(
            self.body,
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" xlink:href=\"data:{mime};base64,{}\"/>",
            num(x),
            num(y),
            num(width),
            num(height),
            base64(&image.data)
        );
    }

    fn svg(&mut self, figure: &SvgFigure, x: f64, y: f64, _baseline: f64, _width: f64, _height: f64) {
        let _ = write!(self.body, "<g transform=\"translate({} {}) scale({})\">", num(x), num(y), num(figure.scale));
        let mut fill = (0.0, 0.0, 0.0);
        let mut pen = Pen { rgb: (0.0, 0.0, 0.0), width: 1.0, join: 0, cap: 0 };
        let mut d = String::new();
        for op in &figure.image.ops {
            match *op {
                SvgOp::Move(x, y) => {
                    let _ = write!(d, "M{x} {y}");
                }
                SvgOp::Line(x, y) => {
                    let _ = write!(d, "L{x} {y}");
                }
                SvgOp::Curve([x1, y1, x2, y2, x, y]) => {
                    let _ = write!(d, "C{x1} {y1} {x2} {y2} {x} {y}");
                }
                SvgOp::LineWidth(w) => pen.width = w,
                SvgOp::LineJoin(j) => pen.join = j,
                SvgOp::LineCap(c) => pen.cap = c,
                SvgOp::StrokeRgb(r, g, b) => pen.rgb = (r, g, b),
                SvgOp::FillRgb(r, g, b) => fill = (r, g, b),
                SvgOp::Close => d.push('Z'),
                SvgOp::Stroke => self.path(&std::mem::take(&mut d), None, Some(pen), false),
                SvgOp::CloseStroke => {
                    d.push('Z');
                    self.path(&std::mem::take(&mut d), None, Some(pen), false);
                }
                SvgOp::Fill => self.path(&std::mem::take(&mut d), Some(fill), None, false),
                SvgOp::FillEvenOdd => self.path(&std::mem::take(&mut d), Some(fill), None, true),
                SvgOp::FillStroke => self.path(&std::mem::take(&mut d), Some(fill), Some(pen), false),
            }
        }
        self.body.push_str("</g>");
    }

    fn push_transform(&mut self, matrix: Matrix) {
        let [a, b, c, d, e, f] = matrix.map(num);
        let _ = write!(self.body, "<g transform=\"matrix({a} {b} {c} {d} {e} {f})\">");
    }

    fn pop_transform(&mut self) {
        self.body.push_str("</g>");
    }

    fn link(&mut self, [left, top, right, bottom]: [f64; 4], dest: &LinkDest) {
        let href = match dest {
            LinkDest::Uri(uri) => escape(uri),
            LinkDest::Internal(name) => format!("#{}", escape(name)),
        };
        let _ = write!(
            self.body,
            "<a xlink:href=\"{href}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"transparent\"/></a>",
            num(left),
            num(top),
            num(right - left),
            num(bottom - top)
        );
    }
}

type Rgb = (f64, f64, f64);

#[derive(Clone, Copy)]
struct Pen {
    rgb: Rgb,
    width: f32,
    join: u8,
    cap: u8,
}

/// Up to three decimals, without trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn hex((r, g, b): Rgb) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

fn css_color(color: Color) -> String {
    hex(match color {
        Color::Rgb { r, g, b } => (r, g, b),
        Color::Cmyk { c, m, y, k } => ((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)),
        Color::Grayscale { l } => (l, l, l),
    })
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(12.34567), "12.346");
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }
}
