//! Numbered footnotes set at the foot of the page (SILE's `footnotes`
//! package).

use sile_core::builder::{BuilderError, Context};
use crate::DocumentBuilder;
use crate::insertion::InsertionClass;
use sile_core::length::Length;
use sile_core::node::Node;
use sile_core::structure::Role;

impl DocumentBuilder {
    /// Send footnotes to the `footnotes` frame, taking their room from
    /// `content`. Its skips are relative to the font in use now; `footnote`
    /// calls this the first time.
    pub fn use_footnotes(&mut self) -> &mut Self {
        if self.insertion_class_mut("footnote").is_some() {
            return self;
        }
        *self.counter_mut("footnote") = 1;
        let ex = self.x_height();
        let mut class = InsertionClass::new("footnotes", "content", 0.75 * self.paper().height);
        class.top_box = vec![Node::vglue(Length::pt(2.0 * ex))];
        class.inter_skip = ex;
        self.set_insertion_class("footnote", class)
    }

    /// A raised number here, and what `content` adds as the note, numbered
    /// and set with the document's own settings at 90% size (SILE's
    /// `\footnote`).
    pub fn footnote<C, E>(ctx: &mut C, content: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: Context<Arranger = DocumentBuilder>,
        E: From<BuilderError>,
    {
        let doc = ctx.arranger();
        doc.use_footnotes();
        let number = doc.counter_mut("footnote").to_string();
        let saved = doc.settings().clone();
        let (raise, size) = (0.7 * doc.x_height(), 1.5 * doc.x_height());
        doc.begin_structure(Role::Reference);
        doc.update_font(|f| f.size = size)?;
        doc.add_baseline_shift(raise).add_text(number.clone()).add_baseline_shift(-raise);
        doc.end_structure().restore_settings(saved.clone());

        doc.use_toplevel();
        doc.update_font(|f| f.size *= 0.9)?;
        let em = doc.font_spec().map_or(10.0, |f| f.size);
        doc.push_typesetter(Some("footnotes"))?;
        doc.begin_structure(Role::Note).set_current_indent(Some(0.0));
        doc.begin_structure(Role::Lbl).add_text(format!("{number}.")).end_structure();
        doc.add_glue(Length::pt(2.0 * em));
        let result = content(ctx);
        let doc = ctx.arranger();
        let nodes = doc.pop_typesetter();
        doc.end_structure().restore_settings(saved);
        result?;
        doc.insert("footnote", nodes?);
        *doc.counter_mut("footnote") += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::*;
    use crate::class::Plain;

    #[test]
    fn footnotes_are_numbered_and_go_to_their_frame() {
        let mut d = doc(Plain::new());
        d.add_text("Text");
        DocumentBuilder::footnote(&mut d, |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
            d.add_text("First note.");
            Ok(())
        })
        .unwrap();
        d.add_text(" and more");
        DocumentBuilder::footnote(&mut d, |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
            d.add_text("Second note.");
            Ok(())
        })
        .unwrap();
        let pages = d.into_pages().unwrap();
        let notes = text_in(&pages[0], "footnotes");
        assert!(notes == "1.Firstnote.2.Secondnote.", "{notes}");
        let body = text_in(&pages[0], "content");
        assert!(body.contains("Text1") && body.contains("more2") && !body.contains("note"), "{body}");
    }

    #[test]
    fn tagged_footnotes_are_notes_where_they_are_referenced() {
        let mut d = doc(Plain::new());
        d.set_tagged(true).add_text("Text");
        DocumentBuilder::footnote(&mut d, |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
            d.add_text("A note.");
            Ok(())
        })
        .unwrap();
        d.add_text(" goes on.");
        let tree = d.structure().unwrap().clone();
        let roles = |e: usize| -> Vec<Role> {
            tree.elements[e]
                .kids
                .iter()
                .filter_map(|k| match k {
                    sile_core::structure::StructKid::Element(c) => Some(tree.elements[*c].role),
                    _ => None,
                })
                .collect()
        };
        let p = tree.elements.iter().position(|e| e.role == Role::P).unwrap();
        assert_eq!(roles(p), [Role::Reference, Role::Note]);
        let note = tree.elements.iter().position(|e| e.role == Role::Note).unwrap();
        assert_eq!(roles(note), [Role::Lbl, Role::P]);
    }
}
