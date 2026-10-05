//! Styles as values: a `TextStyle` for runs of text and a `ParagraphStyle`
//! for whole paragraphs. Fields left `None` keep what surrounds them, and
//! applying a style through `span` or `paragraph` puts the settings back
//! when its body ends.

use super::*;

/// How a run of text looks. `None` keeps the surrounding value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyle {
    pub family: Option<String>,
    /// The font size in points.
    pub size: Option<f64>,
    /// A factor applied to the size, after `size`.
    pub scale: Option<f64>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
    /// OpenType features, such as `+smcp`.
    pub features: Option<String>,
    pub color: Option<Color>,
    /// The language, for shaping, hyphenation and line breaking.
    pub language: Option<String>,
    /// Space added between letters.
    pub letter_space: Option<Length>,
}

impl TextStyle {
    pub fn bold() -> Self {
        Self { weight: Some(FontWeight::BOLD), ..Default::default() }
    }

    pub fn italic() -> Self {
        Self { style: Some(FontStyle::Italic), ..Default::default() }
    }

    fn changes_font(&self) -> bool {
        self.family.is_some() || self.size.is_some() || self.scale.is_some() || self.weight.is_some() || self.style.is_some() || self.features.is_some()
    }
}

/// How a paragraph is set, including the text in it. `None` keeps the
/// surrounding value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParagraphStyle {
    pub text: TextStyle,
    pub align: Option<TextAlign>,
    /// Added to the surrounding left margin, so nested styles indent further.
    pub margin_left: Option<Length>,
    /// Added to the surrounding right margin.
    pub margin_right: Option<Length>,
    pub first_line_indent: Option<f64>,
    /// Space before the paragraph, dropped at the top of a frame.
    pub space_before: Option<Length>,
    /// Space after the paragraph, dropped at the bottom of a frame.
    pub space_after: Option<Length>,
    /// Space between the paragraphs set inside this one's body.
    pub paragraph_skip: Option<Length>,
    pub baseline_skip: Option<BaselineSkip>,
}

impl Typesetter {
    /// Change the current settings by `style`, until they are changed again.
    pub fn apply_text_style(&mut self, style: &TextStyle) -> Result<&mut Self, BuilderError> {
        if style.changes_font() {
            self.update_font(|f| {
                if let Some(family) = &style.family {
                    f.family = Some(family.clone());
                    f.filename = None;
                }
                if let Some(size) = style.size {
                    f.size = size;
                }
                if let Some(scale) = style.scale {
                    f.size *= scale;
                }
                if let Some(weight) = style.weight {
                    f.weight = weight;
                }
                if let Some(font_style) = style.style {
                    f.style = font_style;
                }
                if let Some(features) = &style.features {
                    f.features = features.clone();
                }
            })?;
        }
        if let Some(color) = style.color {
            self.set_color(color);
        }
        if let Some(language) = &style.language {
            self.set_language(language.clone());
        }
        if let Some(space) = style.letter_space {
            self.set_letter_space(Some(space));
        }
        Ok(self)
    }

    /// Change the current settings by `style`, until they are changed again.
    /// Space before and after is left to `paragraph`.
    pub fn apply_paragraph_style(&mut self, style: &ParagraphStyle) -> Result<&mut Self, BuilderError> {
        self.apply_text_style(&style.text)?;
        let mut skips = self.line_skips();
        if let Some(margin) = style.margin_left {
            skips.left += margin;
        }
        if let Some(margin) = style.margin_right {
            skips.right += margin;
        }
        if let Some(align) = style.align {
            skips = skips.aligned(align);
        }
        self.set_line_skips(skips);
        if let Some(indent) = style.first_line_indent {
            self.set_paragraph_indent(indent);
        }
        if let Some(skip) = style.paragraph_skip {
            self.set_paragraph_skip(skip);
        }
        if style.baseline_skip.is_some() {
            self.set_baseline_skip(style.baseline_skip);
        }
        Ok(self)
    }
}

/// Set what `body` adds in `style`, then put the settings back.
pub fn span<C, E>(ctx: &mut C, style: &TextStyle, body: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
where
    C: Context,
    E: From<BuilderError>,
{
    let saved = ctx.arranger().settings().clone();
    let result = match ctx.arranger().apply_text_style(style) {
        Ok(_) => body(ctx),
        Err(e) => Err(e.into()),
    };
    ctx.arranger().restore_settings(saved);
    result
}

/// End the paragraph in progress, then set what `body` adds as paragraphs
/// in `style`, and put the settings back.
pub fn paragraph<C, E>(ctx: &mut C, style: &ParagraphStyle, body: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
where
    C: Context,
    E: From<BuilderError>,
{
    let a = ctx.arranger();
    a.new_paragraph()?;
    if let Some(space) = style.space_before {
        a.add_vskip(space)?;
    }
    let saved = a.settings().clone();
    let result = match a.apply_paragraph_style(style) {
        Ok(_) => body(ctx).and_then(|_| ctx.arranger().new_paragraph().map(|_| ()).map_err(E::from)),
        Err(e) => Err(e.into()),
    };
    let a = ctx.arranger();
    a.restore_settings(saved);
    result?;
    if let Some(space) = style.space_after {
        a.add_vskip(space)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn font(g: &Galley) -> FontSpec {
        g.font_spec().cloned().unwrap()
    }

    #[test]
    fn spans_change_the_font_and_put_it_back() {
        let mut g = galley();
        let before = font(&g);
        span(&mut g, &TextStyle { scale: Some(0.5), ..TextStyle::bold() }, |g: &mut Galley| -> Result<(), BuilderError> {
            assert_eq!(font(g).size, before.size / 2.0);
            assert_eq!(font(g).weight, FontWeight::BOLD);
            g.add_text("bold");
            Ok(())
        })
        .unwrap();
        assert_eq!(font(&g), before);
    }

    #[test]
    fn paragraph_styles_nest_and_restore() {
        let mut g = galley();
        let quote = ParagraphStyle { margin_left: Some(Length::pt(20.0)), first_line_indent: Some(0.0), text: TextStyle::italic(), ..Default::default() };
        paragraph(&mut g, &quote, |g: &mut Galley| -> Result<(), BuilderError> {
            g.add_text("Outer");
            paragraph(g, &quote, |g: &mut Galley| -> Result<(), BuilderError> {
                assert_eq!(g.line_skips().left.to_pt_abs(), 40.0);
                g.add_text("Inner");
                Ok(())
            })
        })
        .unwrap();
        assert_eq!(g.line_skips().left.to_pt_abs(), 0.0);
        assert_eq!(font(&g).style, FontStyle::Normal);
        g.add_text("After");
        let text = text_in(&g.lay_out().unwrap().pages[0], "content");
        assert!(text.contains("Outer") && text.contains("Inner") && text.contains("After"), "{text}");
    }

    #[test]
    fn styled_paragraphs_are_indented_by_their_margin() {
        let mut g = galley();
        g.set_paragraph_indent(0.0);
        g.add_text("Plain");
        paragraph(&mut g, &ParagraphStyle { margin_left: Some(Length::pt(30.0)), ..Default::default() }, |g: &mut Galley| -> Result<(), BuilderError> {
            g.add_text("Indented");
            Ok(())
        })
        .unwrap();
        let trace = g.lay_out().unwrap().render_debug();
        let x_of = |word: &str| -> f64 {
            let lines: Vec<&str> = trace.lines().collect();
            let at = lines.iter().position(|l| l.ends_with(&format!("({word})"))).unwrap();
            lines[..at].iter().rev().find_map(|l| l.strip_prefix("Mx \t")).map_or(0.0, |x| x.parse().unwrap())
        };
        assert!((x_of("Indented") - x_of("Plain") - 30.0).abs() < 0.01);
    }
}
