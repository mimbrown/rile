//! The OpenType MATH table, read once per font into owned data.

use std::collections::HashMap;

use crate::font::FontFace;

/// MATH constants in points at some font size. The `*_scale_down`
/// values are ratios.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Constants {
    pub script_scale_down: f64,
    pub script_script_scale_down: f64,
    pub axis_height: f64,
    pub accent_base_height: f64,
    pub flattened_accent_base_height: f64,
    pub subscript_shift_down: f64,
    pub subscript_top_max: f64,
    pub subscript_baseline_drop_min: f64,
    pub superscript_shift_up: f64,
    pub superscript_shift_up_cramped: f64,
    pub superscript_bottom_min: f64,
    pub superscript_baseline_drop_max: f64,
    pub sub_superscript_gap_min: f64,
    pub superscript_bottom_max_with_subscript: f64,
    pub space_after_script: f64,
    pub upper_limit_gap_min: f64,
    pub upper_limit_baseline_rise_min: f64,
    pub lower_limit_gap_min: f64,
    pub lower_limit_baseline_drop_min: f64,
    pub fraction_numerator_shift_up: f64,
    pub fraction_numerator_display_style_shift_up: f64,
    pub fraction_denominator_shift_down: f64,
    pub fraction_denominator_display_style_shift_down: f64,
    pub fraction_numerator_gap_min: f64,
    pub fraction_num_display_style_gap_min: f64,
    pub fraction_rule_thickness: f64,
    pub fraction_denominator_gap_min: f64,
    pub fraction_denom_display_style_gap_min: f64,
    pub skewed_fraction_horizontal_gap: f64,
    pub radical_vertical_gap: f64,
    pub radical_display_style_vertical_gap: f64,
    pub radical_rule_thickness: f64,
    pub radical_extra_ascender: f64,
    pub radical_kern_before_degree: f64,
    pub radical_kern_after_degree: f64,
    pub radical_degree_bottom_raise_percent: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Variant {
    pub glyph: u16,
    /// In font units.
    pub advance: u16,
}

#[derive(Debug, Default)]
pub(crate) struct MathTable {
    pub units_per_em: f64,
    /// In font units, with the scale downs in percent.
    constants: Constants,
    italics: HashMap<u16, i16>,
    pub vertical: HashMap<u16, Vec<Variant>>,
    pub horizontal: HashMap<u16, Vec<Variant>>,
}

impl MathTable {
    pub fn read(face: &FontFace) -> Option<Self> {
        let (data, index) = face.raw_data();
        let face = ttf_parser::Face::parse(data, index).ok()?;
        let math = face.tables().math?;
        let c = math.constants?;
        let constants = Constants {
            script_scale_down: c.script_percent_scale_down() as f64,
            script_script_scale_down: c.script_script_percent_scale_down() as f64,
            axis_height: c.axis_height().value as f64,
            accent_base_height: c.accent_base_height().value as f64,
            flattened_accent_base_height: c.flattened_accent_base_height().value as f64,
            subscript_shift_down: c.subscript_shift_down().value as f64,
            subscript_top_max: c.subscript_top_max().value as f64,
            subscript_baseline_drop_min: c.subscript_baseline_drop_min().value as f64,
            superscript_shift_up: c.superscript_shift_up().value as f64,
            superscript_shift_up_cramped: c.superscript_shift_up_cramped().value as f64,
            superscript_bottom_min: c.superscript_bottom_min().value as f64,
            superscript_baseline_drop_max: c.superscript_baseline_drop_max().value as f64,
            sub_superscript_gap_min: c.sub_superscript_gap_min().value as f64,
            superscript_bottom_max_with_subscript: c.superscript_bottom_max_with_subscript().value as f64,
            space_after_script: c.space_after_script().value as f64,
            upper_limit_gap_min: c.upper_limit_gap_min().value as f64,
            upper_limit_baseline_rise_min: c.upper_limit_baseline_rise_min().value as f64,
            lower_limit_gap_min: c.lower_limit_gap_min().value as f64,
            lower_limit_baseline_drop_min: c.lower_limit_baseline_drop_min().value as f64,
            fraction_numerator_shift_up: c.fraction_numerator_shift_up().value as f64,
            fraction_numerator_display_style_shift_up: c.fraction_numerator_display_style_shift_up().value as f64,
            fraction_denominator_shift_down: c.fraction_denominator_shift_down().value as f64,
            fraction_denominator_display_style_shift_down: c.fraction_denominator_display_style_shift_down().value as f64,
            fraction_numerator_gap_min: c.fraction_numerator_gap_min().value as f64,
            fraction_num_display_style_gap_min: c.fraction_num_display_style_gap_min().value as f64,
            fraction_rule_thickness: c.fraction_rule_thickness().value as f64,
            fraction_denominator_gap_min: c.fraction_denominator_gap_min().value as f64,
            fraction_denom_display_style_gap_min: c.fraction_denom_display_style_gap_min().value as f64,
            skewed_fraction_horizontal_gap: c.skewed_fraction_horizontal_gap().value as f64,
            radical_vertical_gap: c.radical_vertical_gap().value as f64,
            radical_display_style_vertical_gap: c.radical_display_style_vertical_gap().value as f64,
            radical_rule_thickness: c.radical_rule_thickness().value as f64,
            radical_extra_ascender: c.radical_extra_ascender().value as f64,
            radical_kern_before_degree: c.radical_kern_before_degree().value as f64,
            radical_kern_after_degree: c.radical_kern_after_degree().value as f64,
            radical_degree_bottom_raise_percent: c.radical_degree_bottom_raise_percent() as f64,
        };
        let mut italics = HashMap::new();
        if let Some(values) = math.glyph_info.and_then(|g| g.italic_corrections) {
            for gid in 0..face.number_of_glyphs() {
                if let Some(v) = values.get(ttf_parser::GlyphId(gid)) {
                    italics.insert(gid, v.value);
                }
            }
        }
        let (mut vertical, mut horizontal) = (HashMap::new(), HashMap::new());
        if let Some(variants) = math.variants {
            for gid in 0..face.number_of_glyphs() {
                let id = ttf_parser::GlyphId(gid);
                let list = |c: ttf_parser::math::GlyphConstruction| -> Vec<Variant> {
                    c.variants.into_iter().map(|v| Variant { glyph: v.variant_glyph.0, advance: v.advance_measurement }).collect()
                };
                if let Some(c) = variants.vertical_constructions.get(id) {
                    vertical.insert(gid, list(c));
                }
                if let Some(c) = variants.horizontal_constructions.get(id) {
                    horizontal.insert(gid, list(c));
                }
            }
        }
        Some(Self { units_per_em: face.units_per_em() as f64, constants, italics, vertical, horizontal })
    }

    /// The constants at `size` points.
    pub fn constants(&self, size: f64) -> Constants {
        let s = size / self.units_per_em;
        let c = &self.constants;
        Constants {
            script_scale_down: c.script_scale_down / 100.0,
            script_script_scale_down: c.script_script_scale_down / 100.0,
            axis_height: c.axis_height * s,
            accent_base_height: c.accent_base_height * s,
            flattened_accent_base_height: c.flattened_accent_base_height * s,
            subscript_shift_down: c.subscript_shift_down * s,
            subscript_top_max: c.subscript_top_max * s,
            subscript_baseline_drop_min: c.subscript_baseline_drop_min * s,
            superscript_shift_up: c.superscript_shift_up * s,
            superscript_shift_up_cramped: c.superscript_shift_up_cramped * s,
            superscript_bottom_min: c.superscript_bottom_min * s,
            superscript_baseline_drop_max: c.superscript_baseline_drop_max * s,
            sub_superscript_gap_min: c.sub_superscript_gap_min * s,
            superscript_bottom_max_with_subscript: c.superscript_bottom_max_with_subscript * s,
            space_after_script: c.space_after_script * s,
            upper_limit_gap_min: c.upper_limit_gap_min * s,
            upper_limit_baseline_rise_min: c.upper_limit_baseline_rise_min * s,
            lower_limit_gap_min: c.lower_limit_gap_min * s,
            lower_limit_baseline_drop_min: c.lower_limit_baseline_drop_min * s,
            fraction_numerator_shift_up: c.fraction_numerator_shift_up * s,
            fraction_numerator_display_style_shift_up: c.fraction_numerator_display_style_shift_up * s,
            fraction_denominator_shift_down: c.fraction_denominator_shift_down * s,
            fraction_denominator_display_style_shift_down: c.fraction_denominator_display_style_shift_down * s,
            fraction_numerator_gap_min: c.fraction_numerator_gap_min * s,
            fraction_num_display_style_gap_min: c.fraction_num_display_style_gap_min * s,
            fraction_rule_thickness: c.fraction_rule_thickness * s,
            fraction_denominator_gap_min: c.fraction_denominator_gap_min * s,
            fraction_denom_display_style_gap_min: c.fraction_denom_display_style_gap_min * s,
            skewed_fraction_horizontal_gap: c.skewed_fraction_horizontal_gap * s,
            radical_vertical_gap: c.radical_vertical_gap * s,
            radical_display_style_vertical_gap: c.radical_display_style_vertical_gap * s,
            radical_rule_thickness: c.radical_rule_thickness * s,
            radical_extra_ascender: c.radical_extra_ascender * s,
            radical_kern_before_degree: c.radical_kern_before_degree * s,
            radical_kern_after_degree: c.radical_kern_after_degree * s,
            radical_degree_bottom_raise_percent: c.radical_degree_bottom_raise_percent / 100.0,
        }
    }

    /// The italic correction of `gid` in points at `size`.
    pub fn italic_correction(&self, gid: u16, size: f64) -> Option<f64> {
        self.italics.get(&gid).map(|v| *v as f64 * size / self.units_per_em)
    }
}
