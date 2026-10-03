use std::fmt::Write;

use crate::font::FontFace;
use crate::shaper::GlyphItem;

// ---------------------------------------------------------------------------
// SvgPathBuilder
// ---------------------------------------------------------------------------

struct SvgPathBuilder {
    path: String,
    scale: f64,
    x_off: f64,
    y_off: f64,
}

impl SvgPathBuilder {
    fn new(scale: f64, x_off: f64, y_off: f64) -> Self {
        Self {
            path: String::new(),
            scale,
            x_off,
            y_off,
        }
    }

    fn sx(&self, x: f32) -> f64 {
        x as f64 * self.scale + self.x_off
    }

    fn sy(&self, y: f32) -> f64 {
        -(y as f64 * self.scale) + self.y_off
    }
}

impl ttf_parser::OutlineBuilder for SvgPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.path, "M{:.2} {:.2} ", self.sx(x), self.sy(y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.path, "L{:.2} {:.2} ", self.sx(x), self.sy(y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "Q{:.2} {:.2} {:.2} {:.2} ",
            self.sx(x1),
            self.sy(y1),
            self.sx(x),
            self.sy(y)
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(
            self.path,
            "C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            self.sx(x1),
            self.sy(y1),
            self.sx(x2),
            self.sy(y2),
            self.sx(x),
            self.sy(y)
        );
    }

    fn close(&mut self) {
        self.path.push_str("Z ");
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render shaped glyphs to an SVG document string.
///
/// Extracts actual glyph outlines from the font and positions them according
/// to the shaper output. Useful for visual verification of shaping and bidi.
///
/// When `fallback_faces` is non-empty, outlines not found in the primary face
/// are looked up in each fallback in order.
pub fn render_glyphs_to_svg(
    items: &[GlyphItem],
    face: &FontFace,
    font_size: f64,
) -> String {
    render_glyphs_to_svg_with_fallbacks(items, face, &[], font_size)
}

pub fn render_glyphs_to_svg_with_fallbacks(
    items: &[GlyphItem],
    face: &FontFace,
    fallback_faces: &[&FontFace],
    font_size: f64,
) -> String {
    let margin = font_size;

    // Compute vertical extent from actual glyph positions (handles Nastaliq etc.)
    let mut max_above = font_size;
    let mut max_below = font_size * 0.3;
    for item in items {
        max_above = max_above.max(item.y_offset + item.height);
        max_below = max_below.max(item.depth - item.y_offset).max(0.0);
    }
    let baseline_y = margin + max_above;

    let mut paths = String::new();
    let mut cursor_x = margin;

    // Build list of all faces: [primary, fallback0, fallback1, ...]
    let all_faces: Vec<&FontFace> = std::iter::once(face).chain(fallback_faces.iter().copied()).collect();

    for item in items {
        let gid = ttf_parser::GlyphId(item.gid);
        let x_off = cursor_x + item.x_offset;
        let y_off = baseline_y - item.y_offset;

        // Use font_index to pick the correct face for this glyph
        let target_face = all_faces.get(item.font_index as usize).unwrap_or(&face);
        let outlined = {
            let (fd, fi) = target_face.raw_data();
            ttf_parser::Face::parse(fd, fi).ok().and_then(|f| {
                let s = font_size / f.units_per_em() as f64;
                try_outline_glyph(&f, s, gid, x_off, y_off)
            })
        };

        if let Some(path_data) = outlined {
            let _ = writeln!(
                paths,
                "  <path d=\"{}\" fill=\"black\"/>",
                path_data
            );
        }

        cursor_x += item.width;
    }

    let width = cursor_x + margin;
    let height = baseline_y + max_below + margin;

    // Baseline guide
    let _ = writeln!(
        paths,
        "  <line x1=\"{}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ccc\" stroke-width=\"0.5\"/>",
        margin, baseline_y, width - margin, baseline_y
    );

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.0} {:.0}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n\
         {}</svg>\n",
        width, height, width, height, paths
    )
}

fn try_outline_glyph(
    face: &ttf_parser::Face,
    scale: f64,
    gid: ttf_parser::GlyphId,
    x_off: f64,
    y_off: f64,
) -> Option<String> {
    let mut builder = SvgPathBuilder::new(scale, x_off, y_off);
    face.outline_glyph(gid, &mut builder)?;
    if builder.path.is_empty() {
        return None;
    }
    Some(builder.path.trim().to_string())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{Direction, FontSpec};
    use crate::shaper::{apply_fallbacks, shape_bidi, RustyBuzzShaper, Shaper};

    fn gentium_and_spec() -> Option<(FontFace, FontSpec)> {
        let face = FontFace::from_bytes(crate::test_support::gentium(), 0).ok()?;
        let family = "Gentium Plus".to_string();
        let spec = FontSpec {
            family: Some(family),
            size: 48.0,
            ..Default::default()
        };
        Some((face, spec))
    }

    fn write_svg(name: &str, svg: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("sile_test_{name}.svg"));
        std::fs::write(&path, svg).expect("failed to write SVG");
        eprintln!("wrote {}", path.display());
    }

    #[test]
    fn render_hello_world() {
        let (face, spec) = match gentium_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("Hello, World!", &face, &spec);
        let svg = render_glyphs_to_svg(&items, &face, spec.size);
        assert!(svg.contains("<path"));
        write_svg("hello", &svg);
    }

    #[test]
    fn render_bidi_mixed() {
        let (face, spec) = match gentium_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD} World";
        let items = shape_bidi(&shaper, text, &face, &spec, Some(Direction::LTR));
        let svg = render_glyphs_to_svg(&items, &face, spec.size);
        assert!(svg.contains("<path"));
        write_svg("bidi", &svg);
    }

    #[test]
    fn render_empty() {
        let (face, _spec) = match gentium_and_spec() {
            Some(v) => v,
            None => return,
        };
        let svg = render_glyphs_to_svg(&[], &face, 48.0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<line")); // baseline
    }

    fn load_font_file(path: &str, index: u32) -> Option<FontFace> {
        let data = std::fs::read(path).ok()?;
        FontFace::from_bytes(data, index).ok()
    }

    #[test]
    fn render_nastaliq_urdu() {
        let face = match load_font_file("/System/Library/Fonts/NotoNastaliq.ttc", 0) {
            Some(f) => f,
            None => return,
        };
        let spec = FontSpec {
            size: 48.0,
            direction: Direction::RTL,
            script: "Arab".to_string(),
            language: "URD".to_string(),
            ..Default::default()
        };
        let shaper = RustyBuzzShaper::new();
        // "سلام دنیا" = Hello World in Urdu
        let text = "\u{0633}\u{0644}\u{0627}\u{0645} \u{062F}\u{0646}\u{06CC}\u{0627}";
        let items = shaper.shape(text, &face, &spec);
        assert!(!items.is_empty(), "nastaliq shaping should produce glyphs");
        let svg = render_glyphs_to_svg(&items, &face, spec.size);
        assert!(svg.contains("<path"));
        write_svg("nastaliq", &svg);
    }

    #[test]
    fn render_nastaliq_bismillah() {
        let face = match load_font_file("/System/Library/Fonts/NotoNastaliq.ttc", 0) {
            Some(f) => f,
            None => return,
        };
        let spec = FontSpec {
            size: 48.0,
            direction: Direction::RTL,
            script: "Arab".to_string(),
            language: "URD".to_string(),
            ..Default::default()
        };
        let shaper = RustyBuzzShaper::new();
        // بسم اللہ الرحمٰن الرحیم
        let text = "\u{0628}\u{0633}\u{0645} \u{0627}\u{0644}\u{0644}\u{06C1} \u{0627}\u{0644}\u{0631}\u{062D}\u{0645}\u{0670}\u{0646} \u{0627}\u{0644}\u{0631}\u{062D}\u{06CC}\u{0645}";
        let items = shaper.shape(text, &face, &spec);
        assert!(!items.is_empty());
        let svg = render_glyphs_to_svg(&items, &face, spec.size);
        write_svg("nastaliq_bismillah", &svg);
    }

    #[test]
    fn render_nastaliq_bidi_mixed() {
        let nastaliq = match load_font_file("/System/Library/Fonts/NotoNastaliq.ttc", 0) {
            Some(f) => f,
            None => return,
        };
        let (latin_face, latin_spec) = match gentium_and_spec() {
            Some(v) => v,
            None => return,
        };
        let spec = FontSpec {
            size: 48.0,
            direction: Direction::RTL,
            script: "Arab".to_string(),
            language: "URD".to_string(),
            ..Default::default()
        };
        let shaper = RustyBuzzShaper::new();
        // Mixed: "اردو Urdu زبان" (Urdu [word "Urdu" in Latin] language)
        let text = "\u{0627}\u{0631}\u{062F}\u{0648} Urdu \u{0632}\u{0628}\u{0627}\u{0646}";
        let mut items = shape_bidi(&shaper, text, &nastaliq, &spec, Some(Direction::RTL));
        // Fallback to system font for Latin glyphs
        let fb_spec = FontSpec { size: 48.0, ..latin_spec };
        apply_fallbacks(&shaper, &mut items, &[(&latin_face, &fb_spec)]);
        let svg = render_glyphs_to_svg_with_fallbacks(
            &items, &nastaliq, &[&latin_face], spec.size,
        );
        write_svg("nastaliq_bidi", &svg);
    }
}
