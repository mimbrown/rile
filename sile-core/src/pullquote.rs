//! Quotations set off from the text with large quotation marks (SILE's
//! `pullquote` package).

use std::sync::Arc;

use crate::builder::{Arranger, BuilderError, DocumentBuilder, LineSkips, TextAlign};
use crate::color::Color;
use crate::font::FontStyle;
use crate::length::Length;

pub type Attribution = Arc<dyn Fn(&mut DocumentBuilder, &str) -> Result<(), BuilderError> + Send + Sync>;

pub struct Pullquote {
    pub author: Option<String>,
    /// How much bigger than the text the marks are.
    pub scale: f64,
    pub color: Color,
    /// How far the quote is set in from both sides; 2em when `None`.
    pub setback: Option<f64>,
    pub mark_family: String,
    /// Sets the author line, ragged left in italics by default.
    pub attribution: Attribution,
}

impl Default for Pullquote {
    fn default() -> Self {
        Self {
            author: None,
            scale: 3.0,
            color: Color::Rgb { r: 0.6, g: 0.6, b: 0.6 },
            setback: None,
            mark_family: "Libertinus Serif".into(),
            attribution: Arc::new(|doc, author| {
                doc.update_font(|f| f.style = FontStyle::Italic)?;
                let skips = doc.line_skips();
                doc.set_line_skips(skips.aligned(TextAlign::Right));
                doc.add_text(format!("— {author}"));
                doc.new_paragraph()?;
                Ok(())
            }),
        }
    }
}

impl Pullquote {
    /// Set what `content` adds as the quote.
    pub fn typeset<C, E>(&self, ctx: &mut C, content: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        let doc = ctx.as_mut();
        doc.leave_hmode(false)?;
        let saved = doc.settings().clone();
        let result = self.quote(ctx, content);
        let doc = ctx.as_mut();
        let ended = result.and_then(|_| {
            doc.leave_hmode(false)?;
            if let Some(author) = &self.author {
                (self.attribution)(doc, author)?;
            }
            Ok(())
        });
        doc.restore_settings(saved);
        ended
    }

    fn quote<C, E>(&self, ctx: &mut C, content: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        let doc = ctx.as_mut();
        let setback = self.setback.unwrap_or_else(|| 2.0 * doc.font_spec().map_or(10.0, |f| f.size));
        let skips = doc.line_skips();
        doc.set_line_skips(LineSkips { left: Length::pt(setback), right: Length::pt(setback), ..skips });
        doc.set_current_indent(Some(0.0));
        self.mark(doc, true, setback)?;
        doc.set_current_indent(None);
        content(ctx)?;
        self.mark(ctx.as_mut(), false, setback)?;
        Ok(())
    }

    fn mark(&self, doc: &mut DocumentBuilder, open: bool, setback: f64) -> Result<(), BuilderError> {
        let saved = doc.settings().clone();
        doc.update_font(|f| f.family = Some(self.mark_family.clone()))?;
        let shift = -(if open { self.scale + 1.0 } else { self.scale }) * doc.x_height();
        doc.add_baseline_shift(shift);
        doc.update_font(|f| f.size *= self.scale)?;
        doc.set_color(self.color);
        let mark = if open { "“" } else { "”" };
        let boxed = |doc: &mut DocumentBuilder| -> Result<_, BuilderError> {
            doc.start_hbox().add_text(mark);
            doc.make_hbox()
        };
        if open {
            doc.add_glue(Length::pt(-setback));
            let mut hbox = boxed(doc)?;
            (hbox.width, hbox.height) = (Length::pt(setback), Length::zero());
            doc.add_box(hbox);
        } else {
            doc.add_hfill();
            let mut hbox = boxed(doc)?;
            let width = hbox.width.length.to_pt().unwrap_or(0.0);
            doc.add_glue(Length::pt(setback - width));
            hbox.height = Length::zero();
            doc.add_box(hbox).add_glue(Length::pt(-setback));
        }
        doc.add_baseline_shift(-shift);
        doc.restore_settings(saved);
        Ok(())
    }
}
