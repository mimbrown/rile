//! Marks showing where to trim a page printed on a bigger sheet, with a
//! line of text above them (SILE's `cropmarks` package).

use std::sync::Arc;

use sile_core::builder::BuilderError;
use crate::DocumentBuilder;
use sile_core::pagebuilder::Underlay;

pub type CropHeader = Arc<dyn Fn(&mut DocumentBuilder, usize) -> Result<(), BuilderError> + Send + Sync>;

#[derive(Clone)]
pub struct Cropmarks {
    /// How far the page's content bleeds past its edges; the marks stay
    /// outside it.
    pub bleed: f64,
    /// Adds the text set above the marks, given the sheet's number. SILE's
    /// shows the input file and the date too.
    pub header: CropHeader,
}

impl Default for Cropmarks {
    fn default() -> Self {
        Self {
            bleed: 0.0,
            header: Arc::new(|doc, sheet| {
                doc.add_text(sheet.to_string());
                Ok(())
            }),
        }
    }
}

impl Cropmarks {
    /// Mark every page from the current one on (SILE's `\cropmarks:setup`).
    pub fn install(self, doc: &mut DocumentBuilder) {
        let mut sheet = 1;
        doc.add_end_page_hook(move |doc| {
            self.output(doc, sheet)?;
            sheet += 1;
            Ok(())
        });
    }

    fn output(&self, doc: &mut DocumentBuilder, sheet: usize) -> Result<(), BuilderError> {
        let (w, h) = (doc.paper().width, doc.paper().height);
        let size = 20.0;
        let offset = (self.bleed / 2.0).max(10.0);
        let rules = vec![
            [-offset, 0.0, -size, 0.5],
            [0.0, -offset, 0.5, -size],
            [w + offset, 0.0, size, 0.5],
            [w, -offset, 0.5, -size],
            [-offset, h, -size, 0.5],
            [0.0, h + offset, 0.5, size],
            [w + offset, h, size, 0.5],
            [w, h + offset, 0.5, size],
        ];
        doc.add_overlay(Underlay::Rules(None, rules))?;

        let saved = doc.settings().clone();
        doc.start_hbox();
        let header = doc.update_font(|f| f.size = 6.0).map(|_| ()).and_then(|_| (self.header)(doc, sheet));
        let hbox = doc.make_hbox();
        doc.restore_settings(saved);
        header?;
        doc.add_overlay(Underlay::Box(hbox?, [offset, -offset - 4.0]))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::doc;
    use crate::class::Plain;
    use sile_core::frame::PaperSize;

    #[test]
    fn pages_are_marked_and_centred_on_their_sheets() {
        let mut d = doc(Plain::new());
        d.set_sheet_size(Some(PaperSize::A4));
        Cropmarks::default().install(&mut d);
        d.add_text("Text.");
        let layout = d.lay_out().unwrap();
        let trace = layout.render_debug();
        assert!(trace.contains("Draw line\t-10.0000\t0.0000\t-20.0000\t0.5000"));
        assert!(trace.contains("(1)"));
    }
}
