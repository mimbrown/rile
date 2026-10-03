use super::*;

/// Vertical mode: ends paragraphs and adds vertical material to a
/// typesetter's vertical list, leaving it to the implementor to say where
/// that list goes (pages for `DocumentBuilder`, nowhere for `Galley`).
pub trait Arranger {
    fn typesetter(&mut self) -> &mut Typesetter;

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
        let ts = self.typesetter();
        if ts.captures.is_empty() && !ts.paragraph.is_empty() {
            self.before_lines()?;
        }
        if self.typesetter().end_paragraph()? {
            self.after_lines(independent)?;
        }
        Ok(())
    }

    /// End the paragraph and add the paragraph skip after it (SILE's
    /// `\par`). The skip is left out right after vertical glue or a
    /// penalty, so skips are not doubled.
    fn new_paragraph(&mut self) -> Result<&mut Self, BuilderError> {
        let ts = self.typesetter();
        let after_skip = ts.paragraph.is_empty() && ts.last_vertical().is_some_and(|n| n.is_vglue() || n.is_penalty());
        if !after_skip {
            ts.current_indent = None;
            self.leave_hmode(false)?;
            let ts = self.typesetter();
            let skip = ts.settings.paragraph_skip;
            ts.push_vglue_node(Node::vglue(skip));
        }
        self.leave_hmode(false)?;
        self.typesetter().hanging = None;
        Ok(self)
    }

    /// End the paragraph and say whether nothing is waiting for the current
    /// frame, so what comes next starts at its top (SILE's `\ifattop`).
    fn at_top_of_frame(&mut self) -> Result<bool, BuilderError> {
        self.leave_hmode(false)?;
        Ok(self.typesetter().vertical_queue.is_empty())
    }

    fn add_vskip(&mut self, amount: impl Into<Length>) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        self.typesetter().push_vertical(Node::vglue(amount.into()));
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
        self.typesetter().push_vglue_node(glue);
        Ok(self)
    }

    fn add_vfill(&mut self) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        let mut fill = Node::vfillglue(Length::zero());
        if let Node::VFillGlue(g) = &mut fill {
            g.explicit = true;
        }
        self.typesetter().push_vertical(fill);
        Ok(self)
    }

    fn add_page_break(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vertical_penalty(-10_000)
    }

    /// A page break penalty, ending any pending paragraph first.
    fn add_vertical_penalty(&mut self, penalty: i32) -> Result<&mut Self, BuilderError> {
        if !self.typesetter().paragraph.is_empty() {
            self.leave_hmode(false)?;
        }
        self.typesetter().push_vertical(Node::penalty(penalty));
        Ok(self)
    }

    /// Fill the page and force a new one (SILE's `\supereject`).
    fn supereject(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vfill()?;
        self.typesetter().add_penalty(SUPER_EJECT);
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
        self.typesetter().push_vertical(Node::VBox(vbox));
        Ok(self)
    }

    /// Set material recorded by `begin_capture` here.
    fn add_material(&mut self, material: &Material) -> Result<&mut Self, BuilderError> {
        for item in &material.items {
            match item {
                Captured::Paragraph { inlines, settings } => {
                    let ts = self.typesetter();
                    let current = std::mem::replace(&mut ts.settings, (**settings).clone());
                    ts.paragraph.extend(inlines.iter().cloned());
                    let result = self.leave_hmode(false);
                    self.typesetter().settings = current;
                    result?;
                }
                Captured::Inlines(inlines) => self.typesetter().paragraph.extend(inlines.iter().cloned()),
                Captured::Vertical(node) => self.typesetter().push_vertical((**node).clone()),
            }
        }
        Ok(self)
    }
}
