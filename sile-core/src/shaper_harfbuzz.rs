use harfbuzz_sys as hb;

use crate::font::{Direction, FontFace, FontSpec};
use crate::harfbuzz_ffi::{self, HbBlob, HbBuffer, HbFace, HbFont};
use crate::shaper::{extract_glyph_texts_from_clusters, GlyphItem, Shaper};

pub struct HarfBuzzShaper;

impl HarfBuzzShaper {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HarfBuzzShaper {
    fn default() -> Self {
        Self::new()
    }
}

impl Shaper for HarfBuzzShaper {
    fn shape(&self, text: &str, face: &FontFace, spec: &FontSpec) -> Vec<GlyphItem> {
        let (data, index) = face.raw_data();
        let blob = HbBlob::from_bytes(data);
        let hb_face = HbFace::new(&blob, index);
        let mut font = HbFont::new(&hb_face);
        font.set_variations(&face.variations(spec));

        let mut buffer = HbBuffer::new();
        buffer.add_str(text);

        buffer.set_direction(match spec.direction {
            Direction::LTR | Direction::Frame => hb::HB_DIRECTION_LTR,
            Direction::RTL => hb::HB_DIRECTION_RTL,
            Direction::TTB => hb::HB_DIRECTION_TTB,
        });

        if !spec.script.is_empty() {
            buffer.set_script(harfbuzz_ffi::script_from_string(&spec.script));
        }

        if !spec.language.is_empty() {
            buffer.set_language(&spec.language);
        }
        buffer.guess_segment_properties();

        let features: Vec<hb::hb_feature_t> = if spec.features.is_empty() {
            vec![]
        } else {
            spec.features
                .split(',')
                .filter_map(|s| harfbuzz_ffi::parse_feature(s.trim()))
                .collect()
        };

        harfbuzz_ffi::shape(&font, &mut buffer, &features);

        let infos = buffer.glyph_infos();
        let positions = buffer.glyph_positions();
        let scale = spec.size / face.units_per_em() as f64;

        let clusters: Vec<u32> = infos.iter().map(|i| i.cluster).collect();
        let texts = extract_glyph_texts_from_clusters(text, &clusters);

        let mut items = Vec::with_capacity(infos.len());
        for i in 0..infos.len() {
            let gid = infos[i].codepoint as u16;

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
                x_advance: font.glyph_h_advance(infos[i].codepoint) as f64 * scale,
                y_advance: positions[i].y_advance as f64 * scale,
                font_index: 0,
            });
        }
        if spec.direction == Direction::TTB {
            items.iter_mut().for_each(crate::shaper::set_upright);
        }
        items
    }
}
