use std::ops::DerefMut;

use super::*;
use crate::lists::{ListKind, ListOptions};
#[cfg(feature = "math")]
use crate::math::{MathMode, MathNode};
use crate::node::HBox;
use crate::structure::Role;
use crate::table::CellAlign;

/// What content callbacks are given: an arranger, or a frontend's own
/// state around one, so that content code written over a `Context` serves
/// both.
pub trait Context {
    type Arranger: Arranger;

    fn arranger(&mut self) -> &mut Self::Arranger;
}

impl<A: Arranger> Context for A {
    type Arranger = A;

    fn arranger(&mut self) -> &mut A {
        self
    }
}

/// Vertical mode: ends paragraphs and adds vertical material to a
/// typesetter's vertical list, leaving it to the implementor to say where
/// that list goes (pages for sile-pages' `DocumentBuilder`, nowhere for
/// `Galley`).
pub trait Arranger: DerefMut<Target = Typesetter> {
    /// Called before a paragraph is broken into lines, to set the
    /// typesetter's frame context.
    fn before_lines(&mut self) -> Result<(), BuilderError> {
        Ok(())
    }

    /// Called once lines are on the vertical list; `independent` asks only
    /// for the lines.
    fn after_lines(&mut self, _independent: bool) -> Result<(), BuilderError> {
        Ok(())
    }

    /// Break the pending paragraph into lines without ending it as a
    /// paragraph (no paragraph skip), then let the material go on (for
    /// pages, fill the current frame if it is full). `independent` only
    /// breaks the lines.
    fn leave_hmode(&mut self, independent: bool) -> Result<(), BuilderError> {
        let ts: &mut Typesetter = self;
        if ts.captures.is_empty() && !ts.paragraph.is_empty() {
            self.before_lines()?;
        }
        if self.end_paragraph()? {
            self.after_lines(independent)?;
        }
        Ok(())
    }

    /// End the paragraph and add the paragraph skip after it (SILE's
    /// `\par`). The skip is left out right after vertical glue or a
    /// penalty, so skips are not doubled.
    fn new_paragraph(&mut self) -> Result<&mut Self, BuilderError> {
        let ts: &mut Typesetter = self;
        let after_skip = ts.paragraph.is_empty() && ts.last_vertical().is_some_and(|n| n.is_vglue() || n.is_penalty());
        if !after_skip {
            ts.current_indent = None;
            self.leave_hmode(false)?;
            let ts: &mut Typesetter = self;
            let skip = ts.settings.paragraph_skip;
            ts.push_vglue_node(Node::vglue(skip));
        }
        self.leave_hmode(false)?;
        self.hanging = None;
        Ok(self)
    }

    /// End the paragraph and say whether nothing is waiting for the current
    /// frame, so what comes next starts at its top (SILE's `\ifattop`).
    fn at_top_of_frame(&mut self) -> Result<bool, BuilderError> {
        self.leave_hmode(false)?;
        Ok(self.vertical_queue.is_empty())
    }

    fn add_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        self.push_vertical(Node::vglue(amount.into()));
        Ok(self)
    }

    /// Vertical space kept even at the top or bottom of a page (SILE's
    /// `\skip` and `\smallskip` family).
    fn add_explicit_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut glue = Node::vglue(amount.into());
        if let Node::VGlue(g) = &mut glue {
            g.explicit = true;
        }
        self.push_vglue_node(glue);
        Ok(self)
    }

    fn add_vfill(&mut self) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut fill = Node::vfillglue(Length::zero());
        if let Node::VFillGlue(g) = &mut fill {
            g.explicit = true;
        }
        self.push_vertical(fill);
        Ok(self)
    }

    fn add_page_break(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vertical_penalty(-10_000)
    }

    /// A page break penalty, ending any pending paragraph first.
    fn add_vertical_penalty(&mut self, penalty: i32) -> Result<&mut Self, BuilderError> {
        if !self.paragraph.is_empty() {
            self.leave_hmode(false)?;
        }
        self.push_vertical(Node::penalty(penalty));
        Ok(self)
    }

    /// Fill the page and force a new one (SILE's `\supereject`).
    fn supereject(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vfill()?;
        self.add_penalty(SUPER_EJECT);
        Ok(self)
    }

    fn add_rule(&mut self, width: f64, height: f64) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut rule = Node::hbox(width, height, 0.0);
        if let Node::HBox(b) = &mut rule {
            b.ink = Some(Ink::Rule);
        }
        let vbox = VBox {
            width: Length::pt(width),
            height: Length::pt(height),
            depth: Length::zero(),
            nodes: vec![rule],
            ratio: 0.0,
            misfit: false,
            explicit: false,
            reversed: false,
        };
        self.push_vertical(Node::VBox(vbox));
        Ok(self)
    }

    /// Set material recorded by `begin_capture` here.
    fn add_material(&mut self, material: &Material) -> Result<&mut Self, BuilderError> {
        for item in &material.items {
            match item {
                Captured::Paragraph { inlines, settings } => {
                    let ts: &mut Typesetter = self;
                    let current = std::mem::replace(&mut ts.settings, (**settings).clone());
                    ts.paragraph.extend(inlines.iter().cloned());
                    let result = self.leave_hmode(false);
                    self.settings = current;
                    result?;
                }
                Captured::Inlines(inlines) => self.paragraph.extend(inlines.iter().cloned()),
                Captured::Vertical(node) => self.push_vertical((**node).clone()),
            }
        }
        Ok(self)
    }

    /// Start a list; items follow with `begin_item` / `end_item`.
    fn begin_list(&mut self, kind: ListKind, options: &ListOptions) -> Result<&mut Self, BuilderError> {
        crate::lists::begin_list(self, kind, options)?;
        Ok(self)
    }

    fn end_list(&mut self) -> Result<&mut Self, BuilderError> {
        crate::lists::end_list(self)?;
        Ok(self)
    }

    /// Start an item of the innermost list with its label hung in the
    /// margin; `bullet` overrides the list's bullet for this item.
    fn begin_item(&mut self, bullet: Option<&str>) -> Result<&mut Self, BuilderError> {
        crate::lists::begin_item(self, bullet)?;
        Ok(self)
    }

    /// Start an item marked with a checkbox, ticked when `done`, in place of
    /// the list's label.
    fn begin_task_item(&mut self, done: bool) -> Result<&mut Self, BuilderError> {
        crate::lists::begin_task_item(self, done)?;
        Ok(self)
    }

    fn end_item(&mut self) -> Result<&mut Self, BuilderError> {
        crate::lists::end_item(self)?;
        Ok(self)
    }

    /// Start a table with these columns; rows are added with
    /// `begin_table_row` and laid out at `end_table`.
    fn begin_table(&mut self, columns: &[CellAlign]) -> Result<&mut Self, BuilderError> {
        crate::table::begin_table(self, columns)?;
        Ok(self)
    }

    /// Set the table: columns at their natural widths when they fit,
    /// otherwise narrowed (widest first) with their cells wrapped ragged.
    fn end_table(&mut self) -> Result<&mut Self, BuilderError> {
        crate::table::end_table(self)?;
        Ok(self)
    }

    #[cfg(feature = "math")]
    /// Typeset `formula` in the text, or displayed on its own line,
    /// centred, with an optional number flush right (SILE's `\math` and
    /// `\mathml`).
    fn add_math(&mut self, formula: &MathNode, mode: MathMode) -> Result<&mut Self, BuilderError> {
        crate::math::add_math(self, formula, mode)?;
        Ok(self)
    }

    /// Set `rows` of cells as a table: each column as wide as its widest
    /// cell, a small skip after each row (SILE's `simpletable`).
    fn add_simple_table(&mut self, rows: Vec<Vec<HBox>>) -> Result<&mut Self, BuilderError> {
        crate::transform::add_simple_table(self, rows)?;
        Ok(self)
    }

    fn with_structure<T>(&mut self, role: Role, f: impl FnOnce(&mut Self) -> Result<T, BuilderError>) -> Result<T, BuilderError> {
        crate::structure::with_structure(self, role, f)
    }

    fn untagged<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        crate::structure::untagged(self, f)
    }

    /// Six pangrams, then a big skip (SILE's `\pangrams`).
    fn add_pangrams(&mut self) -> Result<&mut Self, BuilderError> {
        super::specimen::add_pangrams(self)?;
        Ok(self)
    }

    /// Set each line of `text` as an unindented paragraph of its own, in the
    /// current font scaled to make it `width` wide (SILE's `\set-to-width`).
    fn set_to_width(&mut self, width: f64, text: &str) -> Result<&mut Self, BuilderError> {
        super::specimen::set_to_width(self, width, text)?;
        Ok(self)
    }
}
