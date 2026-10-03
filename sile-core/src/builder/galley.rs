use std::ops::{Deref, DerefMut};

use super::*;

/// Type set without pages. Paragraphs are broken to the measure, or set
/// at their natural width when there is none, and stack as far as they
/// need; `take_frame` cuts off what fits a frame of a given height.
pub struct Galley {
    ts: Typesetter,
}

impl Deref for Galley {
    type Target = Typesetter;

    fn deref(&self) -> &Typesetter {
        &self.ts
    }
}

impl DerefMut for Galley {
    fn deref_mut(&mut self) -> &mut Typesetter {
        &mut self.ts
    }
}

impl AsMut<Galley> for Galley {
    fn as_mut(&mut self) -> &mut Galley {
        self
    }
}

impl Arranger for Galley {
    fn typesetter(&mut self) -> &mut Typesetter {
        &mut self.ts
    }
}

impl Default for Galley {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Galley {
    /// A galley `measure` wide, or as wide as its paragraphs when `None`.
    pub fn new(measure: Option<f64>) -> Self {
        Self::with_typesetter(Typesetter::new(), measure)
    }

    pub fn with_typesetter(mut ts: Typesetter, measure: Option<f64>) -> Self {
        ts.frame.line_length = measure.unwrap_or(f64::INFINITY);
        Self { ts }
    }

    pub fn into_typesetter(self) -> Typesetter {
        self.ts
    }

    /// The material that fits in `height`, broken at the best place, its
    /// glue set to fill the height unless it is the last of it. What
    /// doesn't fit stays for the next call; `None` once nothing is left.
    pub fn take_frame(&mut self, height: f64) -> Result<Option<Vec<Node>>, BuilderError> {
        self.leave_hmode(false)?;
        let queue = &mut self.ts.vertical_queue;
        Ok(match pagebuilder::find_break(queue, height, false, &mut pagebuilder::no_insertions) {
            Some(br) => Some(pagebuilder::set_vertical_glue(pagebuilder::split_page(queue, &br), height)),
            None => Some(trim_top(std::mem::take(queue))).filter(|rest| !rest.is_empty()),
        })
    }

    /// Everything set so far as one surface, as tall as its material and
    /// as wide as the measure, or as its widest line when there is none.
    pub fn lay_out(mut self) -> Result<Layout, BuilderError> {
        self.leave_hmode(false)?;
        let nodes = trim_top(std::mem::take(&mut self.ts.vertical_queue));
        let height = nodes.iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum();
        Ok(self.surfaces(vec![(nodes, height)]))
    }

    /// The material in frames `height` tall, one surface each, as many as
    /// it takes.
    pub fn lay_out_frames(mut self, height: f64) -> Result<Layout, BuilderError> {
        let mut frames = Vec::new();
        while let Some(nodes) = self.take_frame(height)? {
            frames.push((nodes, height));
        }
        Ok(self.surfaces(frames))
    }

    fn surfaces(self, frames: Vec<(Vec<Node>, f64)>) -> Layout {
        let measure = self.ts.frame.line_length;
        let widest = |nodes: &[Node]| nodes.iter().map(|n| pt_of(&n.width())).fold(0.0, f64::max);
        let pages = frames
            .into_iter()
            .enumerate()
            .map(|(i, (nodes, height))| {
                let width = if measure.is_finite() { measure } else { widest(&nodes) };
                let paper = PaperSize { width, height };
                let frame = FrameGeometry {
                    id: "content".into(),
                    left: 0.0,
                    top: 0.0,
                    right: width,
                    bottom: height,
                    next: None,
                    direction: Some(self.ts.frame.direction),
                    tate: self.ts.frame.tate,
                    balanced: false,
                };
                let mut page = Page::new(i + 1, paper, vec![frame]);
                page.add_frame_content("content", nodes);
                page
            })
            .collect::<Vec<_>>();
        Layout {
            paper: pages.first().map_or(PaperSize { width: 0.0, height: 0.0 }, |p| p.paper),
            pages,
            references: CrossReferences::default(),
            consulted_references: false,
            fonts: self.ts.fonts,
            bookmarks: Vec::new(),
            metadata: Metadata::default(),
            structure: self.ts.structure,
        }
    }
}

/// Drop what a frame skips at its top, as `set_vertical_glue` does.
fn trim_top(nodes: Vec<Node>) -> Vec<Node> {
    let top = nodes.iter().position(|n| !n.is_discardable() && !n.is_explicit()).unwrap_or(nodes.len());
    nodes.into_iter().skip(top).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn galley(measure: Option<f64>) -> Galley {
        let mut galley = Galley::new(measure);
        let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
        galley.load_font_data("body", crate::class::tests_support::gentium(), spec).unwrap();
        galley.set_font("body");
        galley
    }

    const TEXT: &str = "Lines are broken to the measure the galley is given, and stack as far down as they need to, with no page in sight.";

    fn lines(nodes: &[Node]) -> Vec<&VBox> {
        nodes.iter().filter_map(|n| if let Node::VBox(b) = n { Some(b) } else { None }).collect()
    }

    #[test]
    fn paragraphs_stack_as_far_as_they_need() {
        let mut g = galley(Some(120.0));
        g.add_text(TEXT);
        g.new_paragraph().unwrap();
        g.add_text(TEXT);
        let layout = g.lay_out().unwrap();
        assert_eq!(layout.pages.len(), 1);
        let page = &layout.pages[0];
        let nodes = &page.content[0].1;
        assert!(lines(nodes).len() > 6);
        assert!(lines(nodes).iter().all(|l| (pt_of(&l.width) - 120.0).abs() < 1e-6));
        let height: f64 = nodes.iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum();
        assert_eq!((page.paper.width, page.paper.height), (120.0, height));
    }

    #[test]
    fn without_a_measure_only_forced_breaks_break_lines() {
        let mut g = galley(None);
        g.add_text("A short line");
        g.add_penalty(-10_000);
        g.add_text("and another, a little longer");
        let layout = g.lay_out().unwrap();
        let nodes = &layout.pages[0].content[0].1;
        let lines = lines(nodes);
        assert_eq!(lines.len(), 2);
        assert!((layout.pages[0].paper.width - pt_of(&lines[0].width)).abs() < 1e-6);
    }

    #[test]
    fn bounded_frames_hand_on_what_does_not_fit() {
        let all = lines(&galley_nodes(TEXT)).len();
        let mut g = galley(Some(120.0));
        g.add_text(TEXT);
        g.new_paragraph().unwrap();
        g.add_text(TEXT);
        let layout = g.lay_out_frames(30.0).unwrap();
        let per_frame: Vec<usize> = layout.pages.iter().map(|p| lines(&p.content[0].1).len()).collect();
        assert!(per_frame.len() > 1, "{per_frame:?}");
        assert_eq!(per_frame.iter().sum::<usize>(), all);
        assert!(layout.pages.iter().all(|p| p.paper.height == 30.0));
    }

    fn galley_nodes(text: &str) -> Vec<Node> {
        let mut g = galley(Some(120.0));
        g.add_text(text);
        g.new_paragraph().unwrap();
        g.add_text(text);
        g.lay_out().unwrap().pages.remove(0).content.remove(0).1
    }
}
