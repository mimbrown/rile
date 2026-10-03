//! Initial capitals sunk into the first lines of a paragraph.
// SILE: `dropcaps` package.

use std::sync::Arc;

use crate::builder::{with_font, BuilderError, Context};
use crate::color::Color;
use crate::font::FontSpec;
use crate::length::Length;
use crate::node::HBox;

pub type FontChange = Arc<dyn Fn(&mut FontSpec) + Send + Sync>;

pub struct Dropcap {
    /// How many lines the capital spans.
    pub lines: usize,
    /// Whether the text after the capital runs on from it on the first line.
    pub join: bool,
    /// Space between the capital and the lines beside it; 1spc when `None`.
    pub standoff: Option<f64>,
    pub raise: f64,
    pub shift: f64,
    /// Font size of the capital; worked out from `lines` when `None`.
    pub size: Option<f64>,
    pub scale: f64,
    /// Fit descenders within `lines`, rather than adding lines for them.
    pub strict: bool,
    /// Text that shouldn't go below the baseline, to correct the depth of
    /// fonts whose capitals all do, outside strict mode.
    pub depth_adjust: String,
    pub color: Option<Color>,
    /// How far below the baseline a capital may go, as a fraction of the
    /// baseline skip, before it takes another line (outside strict mode);
    /// from the font's metrics when `None`.
    pub bs_ratio: Option<f64>,
    /// Changes to the font the capital is set in.
    pub font: FontChange,
}

impl Default for Dropcap {
    fn default() -> Self {
        Self {
            lines: 3,
            join: false,
            standoff: None,
            raise: 0.0,
            shift: 0.0,
            size: None,
            scale: 1.0,
            strict: true,
            depth_adjust: "I".into(),
            color: None,
            bs_ratio: None,
            font: Arc::new(|_| {}),
        }
    }
}

impl Dropcap {
    /// Set what `content` adds as the capital of the paragraph that starts
    /// here. `content` is called more than once, to measure it first.
    // SILE: `\dropcap`.
    pub fn typeset<C, E>(&self, ctx: &mut C, mut content: impl FnMut(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: Context,
        E: From<BuilderError>,
    {
        let doc = ctx.arranger();
        let em = doc.font_spec().map_or(10.0, |f| f.size);
        let bs = doc.baseline_skip().map_or(1.2 * em, |b| b.skip_at(em).length.to_pt().unwrap_or(0.0));
        let current_size = doc.font_spec().map_or(10.0, |f| f.size);
        let standoff = self.standoff.unwrap_or_else(|| doc.space_width());
        let depth_adjustment = if self.strict {
            0.0
        } else {
            let text = self.depth_adjust.clone();
            pt(&self.shape(ctx, None, None, |c: &mut C| {
                c.arranger().add_text(text.clone());
                Ok(())
            })?.depth)
        };

        let measured = self.shape(ctx, None, None, &mut content)?;
        let extra_height = (self.lines - 1) as f64 * bs;
        let mut height = pt(&measured.height) + depth_adjustment;
        let target_height = (height - depth_adjustment) * self.scale + extra_height;
        if self.strict {
            height += pt(&measured.depth);
        }
        let size = self.size.unwrap_or(target_height / height * current_size);
        let target_width = pt(&measured.width) / current_size * size;
        let hbox = self.shape(ctx, Some(size), self.color, &mut content)?;

        let doc = ctx.arranger();
        let mut lines = self.lines;
        let raise = if self.strict {
            self.raise + pt(&hbox.depth)
        } else {
            let compensation = depth_adjustment * size / current_size;
            let extra_depth = pt(&hbox.depth) - compensation;
            let ratio = self.bs_ratio.unwrap_or_else(|| {
                let (ascender, descender) = doc.font_extents();
                descender / (ascender + descender)
            });
            let tolerance = ratio * bs;
            if extra_depth > tolerance {
                lines += ((extra_depth - tolerance) / bs).ceil() as usize;
            }
            self.raise + compensation
        };

        let join_offset = if self.join { standoff } else { 0.0 };
        doc.set_hanging(-(lines as i32), target_width + join_offset);
        doc.set_current_indent(Some(0.0));
        doc.start_hbox();
        doc.add_glue(Length::pt(self.shift - target_width - join_offset));
        doc.add_baseline_shift(raise - extra_height);
        doc.add_box(hbox);
        doc.add_baseline_shift(extra_height - raise);
        let mut wrapper = doc.make_hbox()?;
        (wrapper.width, wrapper.height, wrapper.depth) = (Length::pt(-join_offset), Length::zero(), Length::zero());
        doc.add_box(wrapper);
        Ok(())
    }

    fn shape<C, E>(
        &self,
        ctx: &mut C,
        size: Option<f64>,
        color: Option<Color>,
        content: impl FnOnce(&mut C) -> Result<(), E>,
    ) -> Result<HBox, E>
    where
        C: Context,
        E: From<BuilderError>,
    {
        let doc = ctx.arranger();
        let saved = doc.settings().clone();
        if let Some(color) = color {
            doc.set_color(color);
        }
        doc.start_hbox();
        let font = self.font.clone();
        let result = with_font(
            ctx,
            move |f| {
                font(f);
                if let Some(size) = size {
                    f.size = size;
                }
            },
            content,
        );
        let doc = ctx.arranger();
        let hbox = doc.make_hbox();
        doc.restore_settings(saved);
        result?;
        Ok(hbox?)
    }
}

fn pt(length: &Length) -> f64 {
    length.length.to_pt().unwrap_or(0.0)
}
