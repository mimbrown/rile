//! Font specimens.
// SILE: `specimen` package.

use std::sync::Arc;

use super::{Arranger, BuilderError, Inline, Typesetter};
use crate::length::Length;
use crate::measurement::Measurement;
use crate::node::{GlyphData, NNode, Node};

const PANGRAMS: [&str; 6] = [
    "Sphinx of black quartz, judge my vow!",
    "The five boxing wizards jump quickly.",
    "Five quacking zephyrs jolt my wax bed.",
    "Pack my box with five dozen liquor jugs.",
    "Grumpy wizards make toxic brew for the evil queen and jack.",
    "Voix ambiguë d’un cœur qui au zéphyr préfère les jattes de kiwi.",
];

impl Typesetter {
    /// Every glyph of the current font after `.notdef`, in glyph order and
    /// an em apart.
    // SILE: `\repertoire`.
    pub fn add_repertoire(&mut self) -> Result<&mut Self, BuilderError> {
        let name = self.settings.font.clone().ok_or_else(|| BuilderError::NoFont(String::new()))?;
        let font = self.fonts.get(&name).ok_or_else(|| BuilderError::NoFont(name.clone()))?;
        let (face, size) = (Arc::clone(&font.face), font.spec.size);
        let key: Arc<str> = name.into();
        for gid in 1..face.glyph_count() {
            let advance = face.advance_width(gid).map_or(0.0, |a| face.scale_u(a, size));
            let glyph = GlyphData { gid, width: advance, x_advance: advance, ..Default::default() };
            let nnode = NNode::with_glyphs("", vec![glyph], Arc::clone(&key), size, advance, 1.2 * size, 0.0);
            self.push_inline(Inline::Node(Box::new(Node::NNode(nnode))));
            let space = Length::new(Measurement::pt(size - advance), Measurement::pt(1.0), Measurement::pt(1.0));
            self.push_inline(Inline::Node(Box::new(Node::glue(space))));
        }
        Ok(self)
    }
}

pub(crate) fn add_pangrams<A: Arranger + ?Sized>(a: &mut A) -> Result<(), BuilderError> {
    for pangram in PANGRAMS {
        a.add_text(format!("{pangram} "));
    }
    a.add_explicit_vskip(super::bigskip())?;
    Ok(())
}

pub(crate) fn set_to_width<A: Arranger + ?Sized>(a: &mut A, width: f64, text: &str) -> Result<(), BuilderError> {
    let name = a.settings.font.clone().ok_or_else(|| BuilderError::NoFont(String::new()))?;
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let font = a.fonts.get(&name).ok_or_else(|| BuilderError::NoFont(name.clone()))?;
        let natural: f64 = crate::word_shaping::shape(&*a.shaper, line, &font.face, &font.spec).iter().map(|g| g.width).sum();
        let size = font.spec.size;
        a.set_current_indent(Some(0.0));
        a.set_font_size(size * width / natural)?;
        a.add_text(line);
        let ended = a.new_paragraph().map(|_| ());
        a.set_font_size(size)?;
        ended?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;
    use crate::builder::Arranger;
    use crate::node::Node;

    fn lines(doc: crate::builder::Galley) -> Vec<crate::node::VBox> {
        let pages = doc.lay_out().unwrap().pages;
        pages.into_iter().flat_map(|p| p.content).filter(|(id, _)| id == "content").flat_map(|(_, n)| n).filter_map(|n| match n {
            Node::VBox(v) => Some(v),
            _ => None,
        }).collect()
    }

    #[test]
    fn the_repertoire_shows_every_glyph_but_notdef() {
        let mut d = galley();
        let glyphs = d.fonts.values().next().unwrap().face.glyph_count() as usize;
        d.add_repertoire().unwrap();
        let shown: usize = lines(d).iter().map(|l| l.nodes.iter().filter(|n| n.is_nnode()).count()).sum();
        assert_eq!(shown, glyphs - 1);
    }

    #[test]
    fn lines_set_to_width_fill_it() {
        let mut d = galley();
        d.set_to_width(200.0, "Gentium\nPlus").unwrap();
        let lines = lines(d);
        assert_eq!(lines.len(), 2);
        for line in lines {
            let width: f64 = line.nodes.iter().filter(|n| n.is_nnode()).map(|n| n.width().length.to_pt().unwrap()).sum();
            assert!((width - 200.0).abs() < 0.5, "{width}");
        }
    }
}
