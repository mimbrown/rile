//! Font specimens (SILE's `specimen` package).

use std::sync::Arc;

use super::{BuilderError, DocumentBuilder, Inline, Typesetter};
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
    /// an em apart (SILE's `\repertoire`).
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

impl DocumentBuilder {
    /// Six pangrams, then a big skip (SILE's `\pangrams`).
    pub fn add_pangrams(&mut self) -> Result<&mut Self, BuilderError> {
        for pangram in PANGRAMS {
            self.add_text(format!("{pangram} "));
        }
        self.add_explicit_vskip(crate::class::bigskip())
    }

    /// Set each line of `text` as an unindented paragraph of its own, in the
    /// current font scaled to make it `width` wide (SILE's `\set-to-width`).
    pub fn set_to_width(&mut self, width: f64, text: &str) -> Result<&mut Self, BuilderError> {
        let name = self.settings.font.clone().ok_or_else(|| BuilderError::NoFont(String::new()))?;
        for line in text.split('\n').filter(|l| !l.is_empty()) {
            let font = self.fonts.get(&name).ok_or_else(|| BuilderError::NoFont(name.clone()))?;
            let natural: f64 = crate::word_shaping::shape(&*self.shaper, line, &font.face, &font.spec).iter().map(|g| g.width).sum();
            let size = font.spec.size;
            self.set_current_indent(Some(0.0));
            self.set_font_size(size * width / natural)?;
            self.add_text(line);
            let ended = self.new_paragraph().map(|_| ());
            self.set_font_size(size)?;
            ended?;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::class::tests_support::*;
    use crate::class::Plain;
    use crate::node::Node;

    fn lines(doc: crate::builder::DocumentBuilder) -> Vec<crate::node::VBox> {
        let pages = doc.into_pages().unwrap();
        pages.into_iter().flat_map(|p| p.content).filter(|(id, _)| id == "content").flat_map(|(_, n)| n).filter_map(|n| match n {
            Node::VBox(v) => Some(v),
            _ => None,
        }).collect()
    }

    #[test]
    fn the_repertoire_shows_every_glyph_but_notdef() {
        let mut d = doc(Plain::new());
        let glyphs = d.fonts.values().next().unwrap().face.glyph_count() as usize;
        d.add_repertoire().unwrap();
        let shown: usize = lines(d).iter().map(|l| l.nodes.iter().filter(|n| n.is_nnode()).count()).sum();
        assert_eq!(shown, glyphs - 1);
    }

    #[test]
    fn lines_set_to_width_fill_it() {
        let mut d = doc(Plain::new());
        d.set_to_width(200.0, "Gentium\nPlus").unwrap();
        let lines = lines(d);
        assert_eq!(lines.len(), 2);
        for line in lines {
            let width: f64 = line.nodes.iter().filter(|n| n.is_nnode()).map(|n| n.width().length.to_pt().unwrap()).sum();
            assert!((width - 200.0).abs() < 0.5, "{width}");
        }
    }
}
