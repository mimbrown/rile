//! Readings set above their base text (SILE's `ruby` package).

use crate::builder::{BuilderError, Context, Typesetter};
use crate::font::FontWeight;
use crate::length::Length;
use crate::measurement::{Measurement, Unit};
use crate::node::{Ink, Node};

#[derive(Debug, Clone, Copy)]
pub struct RubySettings {
    /// How far the reading sits above the base.
    pub height: Measurement,
    /// Glue between back-to-back readings that are both Latin.
    pub latin_spacer: Measurement,
    /// Set readings with the font's `ruby` feature rather than in bold.
    pub opentype: bool,
}

impl Default for RubySettings {
    fn default() -> Self {
        Self { height: Measurement::new(1.0, Unit::Zw), latin_spacer: Measurement::new(0.25, Unit::Em), opentype: true }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Ruby {
    pub(crate) settings: RubySettings,
    last_reading: Option<String>,
}

fn is_latin(c: char) -> bool {
    matches!(c as u32, 0x21..=0x24F | 0x300..=0x36F | 0x1DC0..=0x1EFF | 0x2C60..=0x2C7F)
}

impl Typesetter {
    pub fn ruby_settings_mut(&mut self) -> &mut RubySettings {
        &mut self.ruby.settings
    }

    fn ruby_length(&self, m: Measurement) -> f64 {
        match m.unit {
            Unit::Zw => m.amount * self.zenkaku_width(),
            Unit::Em => m.amount * self.font_spec().map_or(10.0, |f| f.size),
            _ => m.to_pt_abs(),
        }
    }
}

/// Set `reading` above the material `base` adds, centring the narrower
/// of the two on the wider.
pub fn add_ruby<C, E>(ctx: &mut C, reading: &str, base: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
where
    C: Context,
    E: From<BuilderError>,
{
    let doc = ctx.arranger();
    let spacer = reading.chars().next().is_some_and(is_latin)
        && doc.recently_added(2, |n| matches!(n, Node::HBox(b) if matches!(b.ink, Some(Ink::Ruby(_)))))
        && doc.ruby.last_reading.as_ref().and_then(|r| r.chars().last()).is_some_and(is_latin);
    if spacer {
        let width = doc.ruby_length(doc.ruby.settings.latin_spacer);
        doc.add_glue(Length::pt(width));
    }
    let saved = doc.font_spec().cloned();
    let size = 0.6 * doc.zenkaku_width();
    let opentype = doc.ruby.settings.opentype;
    doc.start_hbox().update_font(|f| {
        f.size = size;
        if opentype {
            f.features = "+ruby".to_string();
        } else {
            f.weight = FontWeight(700);
        }
    })?;
    doc.add_text(reading);
    if let Some(saved) = saved {
        doc.set_font_spec(saved)?;
    }
    let mut ruby = doc.make_hbox()?;
    doc.start_hbox();
    base(ctx)?;
    let doc = ctx.arranger();
    let mut base = doc.make_hbox()?;
    if base.width.length > ruby.width.length {
        ruby.width = (base.width - ruby.width) / 2.0;
    } else {
        let half = (ruby.width - base.width) / 2.0;
        base.width = ruby.width;
        ruby.width = Length::zero();
        ruby.height = Length::zero();
        base.nodes.insert(0, Node::glue(half));
        base.nodes.push(Node::glue(half));
    }
    ruby.ink = Some(Ink::Ruby(doc.ruby_length(doc.ruby.settings.height)));
    doc.add_box(ruby).add_box(base);
    doc.ruby.last_reading = Some(reading.to_string());
    Ok(())
}
