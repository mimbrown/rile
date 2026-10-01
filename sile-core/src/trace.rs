//! A textual outputter matching SILE's `debug` outputter, so layouts can be
//! diffed against SILE's regression test expectations (`tests/*.expected`).

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::color::Color;
use crate::font::FontSpec;
use crate::frame::PaperSize;
use crate::node::NNode;
use crate::render::Canvas;

pub struct TraceCanvas {
    out: String,
    paper: PaperSize,
    fonts: BTreeMap<String, String>,
    pages: usize,
    cursor: (f64, f64),
    last_font: Option<String>,
}

impl TraceCanvas {
    pub fn new(paper: PaperSize) -> Self {
        Self {
            out: String::new(),
            paper,
            fonts: BTreeMap::new(),
            pages: 0,
            cursor: (0.0, 0.0),
            last_font: None,
        }
    }

    /// Register the SILE-style font key printed for nodes set in `name`.
    pub fn register_font(&mut self, name: &str, spec: &FontSpec) {
        let family = if spec.filename.is_some() { "" } else { spec.family.as_deref().unwrap_or("") };
        let style = match spec.style {
            crate::font::FontStyle::Normal => String::new(),
            other => other.to_string(),
        };
        let key = format!(
            "{family};{};{};{style};normal;{};{};{};{}",
            fmt_g(spec.size),
            spec.weight.0,
            spec.features,
            spec.variations,
            spec.direction,
            spec.filename.as_deref().unwrap_or("")
        );
        self.fonts.insert(name.to_string(), key);
    }

    pub fn finish(mut self) -> String {
        if self.pages == 0 {
            self.preamble();
        }
        self.line("End page");
        self.line("Finish");
        self.out
    }

    fn preamble(&mut self) {
        let (w, h) = (self.paper.width, self.paper.height);
        self.line(&format!("Set paper size \t{}\t{}", fmt_sig(w, 14), fmt_sig(h, 14)));
        self.line("Begin page");
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn move_to(&mut self, x: f64, y: f64) {
        if round(self.cursor.0) != round(x) {
            let _ = writeln!(self.out, "Mx \t{}", round(x));
        }
        if round(self.cursor.1) != round(y) {
            let _ = writeln!(self.out, "My \t{}", round(y));
        }
        self.cursor = (x, y);
    }
}

impl Canvas for TraceCanvas {
    fn begin_page(&mut self, _width: f64, _height: f64) {
        if self.pages == 0 {
            self.preamble();
        } else {
            self.line("New page");
        }
        self.pages += 1;
    }

    fn end_page(&mut self) {}

    fn glyphs(&mut self, nnode: &NNode, x: f64, baseline_y: f64) {
        if nnode.glyphs.is_empty() {
            return;
        }
        if let Some(color) = nnode.color {
            let _ = writeln!(self.out, "Push color\t{}", fmt_color(color));
        }
        self.move_to(x, baseline_y);
        let key = self.fonts.get(&nnode.font_key).cloned().unwrap_or_default();
        if self.last_font.as_ref() != Some(&key) {
            let _ = writeln!(self.out, "Set font \t{key}");
            self.last_font = Some(key);
        }
        let complex = nnode
            .glyphs
            .iter()
            .any(|g| g.x_offset != 0.0 || g.y_offset != 0.0 || g.width != g.x_advance);
        let mut buf = String::new();
        if complex {
            for g in &nnode.glyphs {
                let x = self.cursor.0 + g.width;
                if round(self.cursor.0) != round(x) {
                    let _ = writeln!(self.out, "Mx \t{}", round(g.width));
                }
                self.cursor.0 = x;
                let _ = write!(buf, "{} ", g.gid);
                if g.x_advance != 0.0 {
                    let _ = write!(buf, "a={} ", round(g.x_advance));
                }
                if g.x_offset != 0.0 {
                    let _ = write!(buf, "x={} ", round(g.x_offset));
                }
                if g.y_offset != 0.0 {
                    let _ = write!(buf, "y={} ", round(g.y_offset));
                }
            }
            buf.pop();
        } else {
            for g in &nnode.glyphs {
                let _ = write!(buf, "{} ", g.gid);
            }
            let width = nnode.width.to_pt().unwrap_or(0.0);
            let _ = write!(buf, "w={}", round(width));
        }
        let _ = writeln!(self.out, "T\t{buf}\t({})", nnode.text);
        if nnode.color.is_some() {
            let _ = writeln!(self.out, "Pop color");
        }
    }

    fn rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let _ = writeln!(self.out, "Draw line\t{}\t{}\t{}\t{}", round(x), round(y), round(width), round(height));
    }

    fn push_color(&mut self, color: Color) {
        let _ = writeln!(self.out, "Push color\t{}", fmt_color(color));
    }

    fn pop_color(&mut self) {
        self.line("Pop color");
    }
}

/// SILE's `SU.debug_round`: four decimals, nudged away from zero.
fn round(v: f64) -> String {
    let nudged = if v > 0.0 {
        v + 1e-14
    } else if v < 0.0 {
        v - 1e-14
    } else {
        v
    };
    let s = format!("{nudged:.4}");
    if s == "-0.0000" {
        "0.0000".to_string()
    } else {
        s
    }
}

/// C's `%g`: six significant digits, trailing zeros dropped.
/// SILE's `tostring` of a colour.
fn fmt_color(color: Color) -> String {
    let channels = match color {
        Color::Rgb { r, g, b } => vec![("r", r), ("g", g), ("b", b)],
        Color::Cmyk { c, m, y, k } => vec![("c", c), ("m", m), ("y", y), ("k", k)],
        Color::Grayscale { l } => vec![("l", l)],
    };
    let channels: Vec<String> = channels.into_iter().map(|(k, v)| format!("{k}{}", round(v))).collect();
    format!("C<{}>", channels.join(","))
}

fn fmt_g(v: f64) -> String {
    fmt_sig(v, 6)
}

/// C's `%.{digits}g` for numbers that don't need an exponent.
fn fmt_sig(v: f64, digits: i32) -> String {
    let int_digits = if v.abs() >= 1.0 { v.abs().log10().floor() as i32 + 1 } else { 1 };
    let decimals = (digits - int_digits).max(0) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_sile() {
        assert_eq!(round(35.76191), "35.7619");
        assert_eq!(round(18.54), "18.5400");
        assert_eq!(round(-0.00001), "0.0000");
    }

    #[test]
    fn font_sizes_print_like_percent_g() {
        assert_eq!(fmt_g(10.0), "10");
        assert_eq!(fmt_g(10.5), "10.5");
        assert_eq!(fmt_g(6.811523), "6.81152");
    }

    #[test]
    fn empty_document_trace() {
        let t = TraceCanvas::new(PaperSize::A4).finish();
        assert!(t.starts_with("Set paper size \t595.275597\t841.8897729\nBegin page\n"));
        assert!(t.ends_with("End page\nFinish\n"));
    }
}
