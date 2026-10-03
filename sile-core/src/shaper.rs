use unicode_bidi::{BidiInfo, Level};

use crate::font::{Direction, FontFace, FontSpec};
use crate::length::Length;
use crate::measurement::Measurement;

// ---------------------------------------------------------------------------
// GlyphItem
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GlyphItem {
    pub gid: u16,
    pub cluster: u32,
    pub text: std::sync::Arc<str>,
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub x_offset: f64,
    pub y_offset: f64,
    pub x_advance: f64,
    pub y_advance: f64,
    /// Which font this glyph belongs to: 0 = primary, 1+ = fallback index + 1.
    /// Set by `apply_fallbacks`; renderers use this to pick the correct face.
    pub font_index: u16,
}

// ---------------------------------------------------------------------------
// CharMetrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct CharMetrics {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

// ---------------------------------------------------------------------------
// SpaceSettings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpaceSettings {
    /// Size spaces from the shaped space glyph, stretchably; otherwise
    /// every space is `skip` or the font's space width, fixed.
    pub variable_spaces: bool,
    /// SILE's `document.spaceskip`.
    pub skip: Option<crate::length::Length>,
    pub enlargement_factor: f64,
    pub stretch_factor: f64,
    pub shrink_factor: f64,
}

impl Default for SpaceSettings {
    fn default() -> Self {
        Self {
            variable_spaces: true,
            skip: None,
            enlargement_factor: 1.0,
            stretch_factor: 0.5,
            shrink_factor: 1.0 / 3.0,
        }
    }
}

impl SpaceSettings {
    /// Glue for a space glyph of advance `width` (SILE's `shapespace`).
    pub fn space(&self, width: f64) -> crate::length::Length {
        if !self.variable_spaces {
            return crate::length::Length::pt(width.abs());
        }
        crate::length::Length::new(
            crate::measurement::Measurement::pt(width * self.enlargement_factor),
            crate::measurement::Measurement::pt(width.abs() * self.stretch_factor),
            crate::measurement::Measurement::pt(width.abs() * self.shrink_factor),
        )
    }

    /// A space between words, from the shaped space glyph's advance or,
    /// when spaces are not variable, from `measure` (SILE's
    /// `makeSpaceNode`).
    pub fn word_space(&self, width: f64, measure: impl FnOnce() -> f64) -> crate::length::Length {
        if self.variable_spaces { self.space(width) } else { self.measured(measure) }
    }

    /// `skip`, or a space the width `measure` gives (SILE's `measureSpace`).
    pub fn measured(&self, measure: impl FnOnce() -> f64) -> crate::length::Length {
        self.skip.unwrap_or_else(|| self.space(measure()))
    }
}

// ---------------------------------------------------------------------------
// Shaper trait
// ---------------------------------------------------------------------------

pub trait Shaper {
    fn shape(&self, text: &str, face: &FontFace, spec: &FontSpec) -> Vec<GlyphItem>;

    /// Limit shaping to these HarfBuzz shapers, such as `ot` to skip
    /// Graphite (SILE's `harfbuzz.subshapers`); all of them when empty.
    fn set_subshapers(&mut self, _shapers: &[&str]) {}

    /// Whether fonts with Graphite tables are shaped by them.
    fn uses_graphite(&self) -> bool {
        false
    }

    fn measure_char(&self, c: char, face: &FontFace, spec: &FontSpec) -> (CharMetrics, bool) {
        let items = self.shape(&c.to_string(), face, spec);
        let mut width = 0.0_f64;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut found = false;
        for item in &items {
            width += item.width;
            height = height.max(item.height);
            depth = depth.max(item.depth);
            if item.gid != 0 {
                found = true;
            }
        }
        (CharMetrics { width, height, depth }, found)
    }

    fn measure_space(
        &self,
        face: &FontFace,
        spec: &FontSpec,
        settings: &SpaceSettings,
    ) -> Length {
        let items = self.shape(" ", face, spec);
        let raw_width = items.first().map(|g| g.width).unwrap_or(0.0);

        if !settings.variable_spaces {
            return Length::pt(raw_width);
        }

        let base = raw_width.abs();
        Length::new(
            Measurement::pt(base * settings.enlargement_factor),
            Measurement::pt(base * settings.stretch_factor),
            Measurement::pt(base * settings.shrink_factor),
        )
    }
}

// ---------------------------------------------------------------------------
// RustyBuzzShaper
// ---------------------------------------------------------------------------

pub struct RustyBuzzShaper;

impl RustyBuzzShaper {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RustyBuzzShaper {
    fn default() -> Self {
        Self::new()
    }
}

impl Shaper for RustyBuzzShaper {
    fn shape(&self, text: &str, face: &FontFace, spec: &FontSpec) -> Vec<GlyphItem> {
        let Some(cached) = face.rustybuzz_face() else { return vec![] };
        let variations: Vec<rustybuzz::Variation> = face
            .variations(spec)
            .into_iter()
            .map(|(tag, value)| rustybuzz::Variation { tag: rustybuzz::ttf_parser::Tag::from_bytes(&tag), value })
            .collect();
        let varied;
        let rb_face = if variations.is_empty() {
            cached
        } else {
            let mut f = cached.clone();
            f.set_variations(&variations);
            varied = f;
            &varied
        };

        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);

        buffer.set_direction(match spec.direction {
            Direction::LTR | Direction::Frame => rustybuzz::Direction::LeftToRight,
            Direction::RTL => rustybuzz::Direction::RightToLeft,
            Direction::TTB => rustybuzz::Direction::TopToBottom,
        });

        if !spec.script.is_empty()
            && let Ok(script) = spec.script.parse::<rustybuzz::Script>() {
                buffer.set_script(script);
            }

        if !spec.language.is_empty()
            && let Ok(lang) = spec.language.parse::<rustybuzz::Language>() {
                buffer.set_language(lang);
            }

        buffer.guess_segment_properties();
        let features = parse_features(&spec.features);
        let (direction, script, language) = (buffer.direction(), buffer.script(), buffer.language());
        let plan = || rustybuzz::ShapePlan::new(rb_face, direction, Some(script), language.as_ref(), &features);
        let plan = if variations.is_empty() {
            let key = format!("{direction:?} {} {} {}", script.tag(), language.as_ref().map_or("", |l| l.as_str()), spec.features);
            face.shape_plan(key, plan)
        } else {
            std::sync::Arc::new(plan())
        };
        let glyph_buffer = rustybuzz::shape_with_plan(rb_face, &plan, buffer);

        let infos = glyph_buffer.glyph_infos();
        let positions = glyph_buffer.glyph_positions();
        let scale = spec.size / face.units_per_em() as f64;

        let texts = extract_glyph_texts(text, infos);

        let mut items = Vec::with_capacity(infos.len());
        for i in 0..infos.len() {
            let gid = infos[i].glyph_id as u16;

            let (height, depth) = face
                .glyph_bounding_box(gid)
                .map(|bb| (bb.y_max as f64 * scale, -(bb.y_min as f64) * scale))
                .unwrap_or((0.0, 0.0));

            items.push(GlyphItem {
                gid,
                cluster: infos[i].cluster,
                text: texts[i].clone(),
                width: positions[i].x_advance as f64 * scale,
                height,
                depth,
                x_offset: positions[i].x_offset as f64 * scale,
                y_offset: positions[i].y_offset as f64 * scale,
                x_advance: rb_face.glyph_hor_advance(rustybuzz::ttf_parser::GlyphId(gid)).map_or(0.0, |a| a as f64 * scale),
                y_advance: positions[i].y_advance as f64 * scale,
                font_index: 0,
            });
        }
        if spec.direction == Direction::TTB {
            items.iter_mut().for_each(set_upright);
        }
        items
    }
}

/// Vertical metrics as SILE takes them: the glyph's vertical advance is
/// its height and its glyph advance, its horizontal advance its width, and
/// offsets are dropped.
pub(crate) fn set_upright(item: &mut GlyphItem) {
    let advance = -item.y_advance;
    item.width = item.x_advance;
    item.x_advance = advance;
    item.height = advance;
    item.depth = 0.0;
    item.x_offset = 0.0;
    item.y_offset = 0.0;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_features(features_str: &str) -> Vec<rustybuzz::Feature> {
    if features_str.is_empty() {
        return vec![];
    }
    features_str
        .split([',', ';'])
        .filter_map(|s| s.trim().parse::<rustybuzz::Feature>().ok())
        .collect()
}

/// Map glyph cluster indices back to source text substrings.
/// Works with any shaper that produces cluster values (byte offsets into `text`).
pub(crate) fn extract_glyph_texts_from_clusters(text: &str, clusters: &[u32]) -> Vec<std::sync::Arc<str>> {
    if clusters.is_empty() {
        return vec![];
    }

    let mut sorted = clusters.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let text_len = text.len() as u32;
    let mut cluster_end = std::collections::HashMap::with_capacity(sorted.len());
    for i in 0..sorted.len() {
        let end = if i + 1 < sorted.len() {
            sorted[i + 1]
        } else {
            text_len
        };
        cluster_end.insert(sorted[i], end);
    }

    clusters
        .iter()
        .map(|&cluster| {
            let start = cluster as usize;
            let end = *cluster_end.get(&cluster).unwrap_or(&text_len) as usize;
            let end = end.min(text.len());
            let start = start.min(end);
            text[start..end].into()
        })
        .collect()
}

fn extract_glyph_texts(text: &str, infos: &[rustybuzz::GlyphInfo]) -> Vec<std::sync::Arc<str>> {
    let clusters: Vec<u32> = infos.iter().map(|g| g.cluster).collect();
    extract_glyph_texts_from_clusters(text, &clusters)
}

/// Apply tracking (letter-spacing) to shaped glyphs. Modifies `width` but
/// preserves `x_advance` / `y_advance` as the original shaper values.
pub fn apply_tracking(items: &mut [GlyphItem], factor: f64) {
    for item in items.iter_mut() {
        item.width *= factor;
    }
}

/// Re-shape any gid=0 glyphs using fallback fonts. Works on any shaped
/// output (plain, bidi, etc.). Modifies items in place.
pub fn apply_fallbacks(
    shaper: &dyn Shaper,
    items: &mut [GlyphItem],
    fallbacks: &[(&FontFace, &FontSpec)],
) {
    if fallbacks.is_empty() || items.iter().all(|g| g.gid != 0) {
        return;
    }

    for (fb_idx, &(fb_face, fb_spec)) in fallbacks.iter().enumerate() {
        let mut all_resolved = true;
        for item in items.iter_mut() {
            if item.gid != 0 {
                continue;
            }
            let fb_items = shaper.shape(&item.text, fb_face, fb_spec);
            if let Some(fb) = fb_items.first() {
                if fb.gid != 0 {
                    *item = fb.clone();
                    item.font_index = (fb_idx + 1) as u16;
                } else {
                    all_resolved = false;
                }
            }
        }
        if all_resolved {
            break;
        }
    }
}

/// Shape text with font fallback. Tries the primary face first; any glyphs
/// with gid=0 are re-shaped with each fallback face in order.
pub fn shape_with_fallbacks(
    shaper: &dyn Shaper,
    text: &str,
    primary_face: &FontFace,
    primary_spec: &FontSpec,
    fallbacks: &[(&FontFace, &FontSpec)],
) -> Vec<GlyphItem> {
    let mut items = shaper.shape(text, primary_face, primary_spec);
    apply_fallbacks(shaper, &mut items, fallbacks);
    items
}

// ---------------------------------------------------------------------------
// Shaper selection
// ---------------------------------------------------------------------------

/// Returns the default shaper for the current build configuration.
///
/// With the `harfbuzz` feature this returns a `HarfBuzzShaper` backed by the
/// native C HarfBuzz library (required for Graphite fonts). Otherwise it
/// returns the pure-Rust `RustyBuzzShaper`.
pub fn default_shaper() -> Box<dyn Shaper> {
    #[cfg(feature = "harfbuzz")]
    {
        Box::new(crate::shaper_harfbuzz::HarfBuzzShaper::new())
    }

    #[cfg(not(feature = "harfbuzz"))]
    {
        Box::new(RustyBuzzShaper::new())
    }
}

// ---------------------------------------------------------------------------
// Bidi run splitting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BidiRun {
    pub text: String,
    pub direction: Direction,
    pub level: u8,
}

/// Split text into bidi runs in visual display order using the Unicode
/// Bidirectional Algorithm (UAX #9). Each run has a consistent direction.
pub fn split_bidi_runs(text: &str, default_direction: Option<Direction>) -> Vec<BidiRun> {
    if text.is_empty() {
        return vec![];
    }

    let default_level = default_direction.map(|d| match d {
        Direction::RTL => Level::rtl(),
        _ => Level::ltr(),
    });

    let bidi_info = BidiInfo::new(text, default_level);
    let mut runs = Vec::new();

    for para in &bidi_info.paragraphs {
        let line = para.range.clone();
        let (_levels, visual_runs) = bidi_info.visual_runs(para, line);

        for run_range in visual_runs {
            let run_text = &text[run_range.clone()];
            let level = bidi_info.levels[run_range.start];
            runs.push(BidiRun {
                text: run_text.to_string(),
                direction: if level.is_rtl() { Direction::RTL } else { Direction::LTR },
                level: level.number(),
            });
        }
    }

    runs
}

/// The embedding level of each byte of `text` in a paragraph set in
/// `direction`, with trailing whitespace back at the paragraph's level.
pub fn bidi_levels(text: &str, direction: Direction) -> Vec<u8> {
    let level = if direction == Direction::RTL { Level::rtl() } else { Level::ltr() };
    let info = BidiInfo::new(text, Some(level));
    let mut levels = vec![level.number(); text.len()];
    for para in &info.paragraphs {
        let reordered = info.reordered_levels(para, para.range.clone());
        for i in para.range.clone() {
            levels[i] = reordered[i].number();
        }
    }
    levels
}

/// Shape text with bidi support. Splits the input into directional runs
/// using the Unicode BiDi Algorithm, shapes each run with the correct
/// direction, and returns glyphs in visual display order.
pub fn shape_bidi(
    shaper: &dyn Shaper,
    text: &str,
    face: &FontFace,
    spec: &FontSpec,
    default_direction: Option<Direction>,
) -> Vec<GlyphItem> {
    let runs = split_bidi_runs(text, default_direction);

    if runs.len() == 1 && runs[0].direction == spec.direction {
        return shaper.shape(text, face, spec);
    }

    let mut items = Vec::new();
    for run in &runs {
        let mut run_spec = spec.clone();
        run_spec.direction = run.direction;
        let run_items = shaper.shape(&run.text, face, &run_spec);
        items.extend(run_items);
    }

    items
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn load_system_font_and_spec() -> Option<(FontFace, FontSpec)> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let info = db.faces().next()?;
        let family = info.families.first()?.0.clone();
        let id = info.id;
        let mut data_out: Option<(Vec<u8>, u32)> = None;
        db.with_face_data(id, |data, index| {
            data_out = Some((data.to_vec(), index));
        });
        let (data, index) = data_out?;
        let face = FontFace::from_bytes(data, index).ok()?;
        let spec = FontSpec {
            family: Some(family),
            size: 12.0,
            ..Default::default()
        };
        Some((face, spec))
    }

    // -- Feature parsing -----------------------------------------------------

    #[test]
    fn parse_empty_features() {
        assert!(parse_features("").is_empty());
    }

    #[test]
    fn parse_single_feature() {
        let features = parse_features("kern");
        assert_eq!(features.len(), 1);
    }

    #[test]
    fn parse_multiple_features() {
        let features = parse_features("+kern,-liga,+dlig");
        assert_eq!(features.len(), 3);
    }

    // -- Shaping -------------------------------------------------------------

    #[test]
    fn shape_hello() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("Hello", &face, &spec);
        assert_eq!(items.len(), 5);
        for item in &items {
            assert!(item.width > 0.0, "each glyph should have positive width");
            assert_ne!(item.gid, 0, "each glyph should be found");
        }
    }

    #[test]
    fn shape_single_char() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("A", &face, &spec);
        assert_eq!(items.len(), 1);
        assert_ne!(items[0].gid, 0);
        assert!(items[0].width > 0.0);
        assert!(items[0].height > 0.0);
        assert_eq!(&*items[0].text, "A");
    }

    #[test]
    fn shape_preserves_cluster_text() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("AB", &face, &spec);
        assert_eq!(items.len(), 2);
        assert_eq!(&*items[0].text, "A");
        assert_eq!(&*items[1].text, "B");
    }

    #[test]
    fn shape_space_has_width() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape(" ", &face, &spec);
        assert_eq!(items.len(), 1);
        assert!(items[0].width > 0.0, "space should have positive width");
    }

    // -- Measure char --------------------------------------------------------

    #[test]
    fn measure_char_a() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let (metrics, found) = shaper.measure_char('A', &face, &spec);
        assert!(found);
        assert!(metrics.width > 0.0);
        assert!(metrics.height > 0.0);
    }

    #[test]
    fn measure_char_ab_wider_than_a() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let (a_metrics, _) = shaper.measure_char('A', &face, &spec);
        let items = shaper.shape("AB", &face, &spec);
        let ab_width: f64 = items.iter().map(|g| g.width).sum();
        assert!(ab_width > a_metrics.width);
    }

    // -- Measure space -------------------------------------------------------

    #[test]
    fn measure_space_variable() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let settings = SpaceSettings::default();
        let space = shaper.measure_space(&face, &spec, &settings);
        let w = space.to_pt_abs();
        assert!(w > 0.0, "space width should be positive");
    }

    #[test]
    fn measure_space_fixed() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let settings = SpaceSettings {
            variable_spaces: false,
            ..Default::default()
        };
        let space = shaper.measure_space(&face, &spec, &settings);
        // Fixed spaces have zero stretch/shrink
        let stretch = space.stretch.to_pt_abs();
        let shrink = space.shrink.to_pt_abs();
        assert_eq!(stretch, 0.0);
        assert_eq!(shrink, 0.0);
    }

    // -- Tracking ------------------------------------------------------------

    #[test]
    fn apply_tracking_scales_width() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let mut items = shaper.shape("A", &face, &spec);
        let original = items[0].width;
        let original_advance = items[0].x_advance;
        apply_tracking(&mut items, 1.5);
        let expected = original * 1.5;
        assert!((items[0].width - expected).abs() < 1e-10);
        // x_advance should be unchanged
        assert_eq!(items[0].x_advance, original_advance);
    }

    // -- Direction -----------------------------------------------------------

    #[test]
    fn shape_with_rtl() {
        let (face, mut spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        spec.direction = Direction::RTL;
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("AB", &face, &spec);
        // RTL shaping should still produce glyphs
        assert!(!items.is_empty());
        for item in &items {
            assert!(item.width > 0.0);
        }
    }

    // -- Fallback ------------------------------------------------------------

    #[test]
    fn shape_with_fallbacks_no_missing() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shape_with_fallbacks(&shaper, "Hello", &face, &spec, &[]);
        assert_eq!(items.len(), 5);
    }

    // -- Empty input ---------------------------------------------------------

    #[test]
    fn shape_empty_string() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shaper.shape("", &face, &spec);
        assert!(items.is_empty());
    }

    // -- Bidi run splitting ---------------------------------------------------

    #[test]
    fn split_bidi_empty() {
        let runs = split_bidi_runs("", None);
        assert!(runs.is_empty());
    }

    #[test]
    fn split_bidi_pure_ltr() {
        let runs = split_bidi_runs("Hello World", Some(Direction::LTR));
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].direction, Direction::LTR);
        assert_eq!(runs[0].text, "Hello World");
    }

    #[test]
    fn split_bidi_pure_rtl() {
        // Hebrew: שלום
        let runs = split_bidi_runs("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", Some(Direction::RTL));
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].direction, Direction::RTL);
    }

    #[test]
    fn split_bidi_mixed_produces_multiple_runs() {
        // "Hello שלום World" — Latin, Hebrew, Latin
        let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD} World";
        let runs = split_bidi_runs(text, Some(Direction::LTR));
        assert!(runs.len() >= 2, "mixed text should produce multiple runs");
        assert!(runs.iter().any(|r| r.direction == Direction::RTL));
        assert!(runs.iter().any(|r| r.direction == Direction::LTR));
    }

    #[test]
    fn split_bidi_auto_detect_rtl_paragraph() {
        // Start with Hebrew — auto-detect should pick RTL paragraph level
        let text = "\u{05E9}\u{05DC}\u{05D5}\u{05DD} Hello";
        let runs = split_bidi_runs(text, None);
        assert!(!runs.is_empty());
        // First strong char is Hebrew, so paragraph level should be RTL
        assert!(runs.iter().any(|r| r.direction == Direction::RTL));
    }

    // -- shape_bidi -----------------------------------------------------------

    #[test]
    fn shape_bidi_pure_ltr() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shape_bidi(&shaper, "Hello", &face, &spec, Some(Direction::LTR));
        assert_eq!(items.len(), 5);
        for item in &items {
            assert!(item.width > 0.0);
        }
    }

    #[test]
    fn shape_bidi_mixed_text() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD} World";
        let items = shape_bidi(&shaper, text, &face, &spec, Some(Direction::LTR));
        assert!(!items.is_empty());
        for item in &items {
            assert!(item.width > 0.0 || item.gid == 0, "each glyph should have width or be missing");
        }
    }

    #[test]
    fn shape_bidi_empty() {
        let (face, spec) = match load_system_font_and_spec() {
            Some(v) => v,
            None => return,
        };
        let shaper = RustyBuzzShaper::new();
        let items = shape_bidi(&shaper, "", &face, &spec, None);
        assert!(items.is_empty());
    }
}
