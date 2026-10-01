use crate::frame::{FrameId, PageLayout};
use crate::measurement::Measurement;
use crate::node::Node;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const INF_BAD: i64 = 10_000;
const EJECT_PENALTY: i32 = -10_000;
const AWFUL_BAD: i64 = 1_073_741_823;
const DEPLORABLE: i64 = 100_000;

// ---------------------------------------------------------------------------
// PageBreakSettings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PageBreakSettings {
    pub tolerance: i64,
    pub line_penalty: i64,
    /// Penalty for breaking after a paragraph's first line (SILE's
    /// `typesetter.widowpenalty`).
    pub widow_penalty: i32,
    /// Penalty for breaking before a paragraph's last line (SILE's
    /// `typesetter.orphanpenalty`).
    pub orphan_penalty: i32,
    pub club_penalty: i32,
    pub inter_line_penalty: i32,
    /// Penalty for breaking after a hyphenated line.
    pub broken_penalty: i32,
    pub pre_display_penalty: i32,
    pub post_display_penalty: i32,
}

impl Default for PageBreakSettings {
    fn default() -> Self {
        Self {
            tolerance: 500,
            line_penalty: 10,
            widow_penalty: 3000,
            orphan_penalty: 3000,
            club_penalty: 150,
            inter_line_penalty: 0,
            broken_penalty: 100,
            pre_display_penalty: 10_000,
            post_display_penalty: 10_000,
        }
    }
}

// ---------------------------------------------------------------------------
// PageBreakResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PageBreakResult {
    pub break_index: usize,
    pub badness: i64,
    pub penalty: i32,
    pub cost: i64,
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Page {
    pub number: usize,
    pub frames: Vec<(FrameId, Vec<Node>)>,
}

impl Page {
    pub fn new(number: usize) -> Self {
        Self {
            number,
            frames: Vec::new(),
        }
    }

    pub fn add_frame_content(&mut self, id: FrameId, nodes: Vec<Node>) {
        self.frames.push((id, nodes));
    }
}

// ---------------------------------------------------------------------------
// PageBuilder
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PageBuilder {
    pub settings: PageBreakSettings,
    queue: Vec<Node>,
    pages: Vec<Page>,
    page_number: usize,
}

impl PageBuilder {
    pub fn new(settings: PageBreakSettings) -> Self {
        Self {
            settings,
            queue: Vec::new(),
            pages: Vec::new(),
            page_number: 0,
        }
    }

    pub fn enqueue(&mut self, node: Node) {
        self.queue.push(node);
    }

    pub fn enqueue_many(&mut self, nodes: impl IntoIterator<Item = Node>) {
        self.queue.extend(nodes);
    }

    pub fn queue(&self) -> &[Node] {
        &self.queue
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn into_pages(self) -> Vec<Page> {
        self.pages
    }

    /// Find the best page break in the queue for a frame of `target_height`
    /// (SILE's `findBestBreak`). Legal breaks are penalties below 10000 and
    /// glue after non-discardable material. Returns `None` until the queue
    /// holds a forced break or overflows the frame.
    pub fn find_break(&self, target_height: f64) -> Option<PageBreakResult> {
        let queue = &self.queue;
        let mut i = queue.iter().position(|n| !n.is_vglue()).unwrap_or(queue.len());
        let (mut height, mut stretch, mut shrink) = (0.0_f64, 0.0_f64, 0.0_f64);
        let mut least_cost = INF_BAD;
        let mut best: Option<PageBreakResult> = None;

        while i < queue.len() {
            let node = &queue[i];
            match node {
                Node::VBox(_) => height += pt(node.height()) + pt(node.depth()),
                Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) => {
                    height += g.height.length.to_pt().unwrap_or(0.0);
                    stretch += g.height.stretch.to_pt().unwrap_or(0.0);
                    shrink += g.height.shrink.to_pt().unwrap_or(0.0);
                }
                _ => {}
            }
            let pi = match node {
                Node::Penalty(p) => p.penalty,
                _ => 0,
            };
            let legal = match node {
                Node::Penalty(p) => (p.penalty as i64) < INF_BAD,
                n => n.is_vglue() && i > 0 && !queue[i - 1].is_discardable(),
            };
            if legal {
                let left = target_height - height;
                let badness = if height < target_height {
                    rate_badness(left, stretch)
                } else if left < shrink {
                    AWFUL_BAD
                } else {
                    rate_badness(-left, shrink)
                };
                let cost = if badness < AWFUL_BAD {
                    if pi <= EJECT_PENALTY {
                        pi as i64
                    } else if badness < INF_BAD {
                        badness + pi as i64
                    } else {
                        DEPLORABLE
                    }
                } else {
                    badness
                };
                let here = PageBreakResult { break_index: i, badness, penalty: pi, cost };
                if cost < least_cost {
                    least_cost = cost;
                    best = Some(here.clone());
                }
                if cost == AWFUL_BAD || pi <= EJECT_PENALTY {
                    return Some(best.unwrap_or(here));
                }
            }
            i += 1;
        }
        None
    }

    /// Take the next page's material off the queue, glue set to fill
    /// `target_height`.
    fn take_page(&mut self, target_height: f64, flush: bool) -> Option<Vec<Node>> {
        if self.queue.is_empty() {
            return None;
        }
        let end = match self.find_break(target_height) {
            Some(br) => br.break_index + 1,
            None if flush => self.queue.len(),
            None => return None,
        };
        let mut content: Vec<Node> = self.queue.drain(..end).collect();
        while content.len() > 1 && content.last().is_some_and(Node::is_discardable) {
            content.pop();
        }
        Some(set_vertical_glue(content, target_height))
    }

    /// Build pages from the queue, distributing content into the target frame.
    pub fn build_pages(&mut self, layout: &PageLayout, target_frame: FrameId) -> Vec<Page> {
        let target_height = layout.frame(target_frame).height();
        let mut result_pages = Vec::new();
        while let Some(content) = self.take_page(target_height, true) {
            self.page_number += 1;
            let mut page = Page::new(self.page_number);
            page.add_frame_content(target_frame, content);
            result_pages.push(page);
        }
        self.pages.extend(result_pages.clone());
        result_pages
    }

    /// Build pages using a multi-frame layout, distributing content across
    /// chained frames. Content flows from one frame to the next via `frame.next`.
    pub fn build_pages_multi_frame(
        &mut self,
        layout: &PageLayout,
        start_frame: FrameId,
    ) -> Vec<Page> {
        let mut result_pages = Vec::new();
        while !self.queue.is_empty() {
            self.page_number += 1;
            let mut page = Page::new(self.page_number);
            let mut current_frame = start_frame;
            loop {
                let frame = layout.frame(current_frame);
                let Some(content) = self.take_page(frame.height(), true) else {
                    break;
                };
                page.add_frame_content(current_frame, content);
                match frame.next {
                    Some(next_id) if !self.queue.is_empty() => current_frame = next_id,
                    _ => break,
                }
            }
            result_pages.push(page);
        }
        self.pages.extend(result_pages.clone());
        result_pages
    }
}

fn pt(l: crate::length::Length) -> f64 {
    l.length.to_pt().unwrap_or(0.0)
}

/// SILE's `rateBadness`.
fn rate_badness(shortfall: f64, spring: f64) -> i64 {
    if spring == 0.0 {
        return INF_BAD;
    }
    ((100.0 * (shortfall / spring).abs().powi(3)).floor() as i64).min(INF_BAD)
}

/// Drop the material SILE skips at the top of a frame (discardable or
/// explicit glue and penalties), then stretch or shrink the page's glue to
/// fill `target` (SILE's `setVerticalGlue`).
fn set_vertical_glue(content: Vec<Node>, target: f64) -> Vec<Node> {
    let top = content
        .iter()
        .position(|n| !n.is_discardable() && !n.is_explicit())
        .unwrap_or(content.len());
    let mut content: Vec<Node> = content.into_iter().skip(top).collect();
    let (mut total, mut stretch, mut shrink) = (0.0, 0.0, 0.0);
    for n in &content {
        total += pt(n.height()) + pt(n.depth());
        if let Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) = n {
            stretch += g.height.stretch.to_pt().unwrap_or(0.0);
            shrink += g.height.shrink.to_pt().unwrap_or(0.0);
        }
    }
    if total == 0.0 {
        return content;
    }
    let adjustment = target - total;
    let (amount, spring, per_glue): (f64, f64, fn(&crate::node::VGlue) -> f64) = if adjustment > 0.0 {
        (adjustment.min(stretch), stretch, |g| g.height.stretch.to_pt().unwrap_or(0.0))
    } else {
        (adjustment.max(-shrink), shrink, |g| g.height.shrink.to_pt().unwrap_or(0.0))
    };
    if spring > 0.0 {
        for n in &mut content {
            if let Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) = n {
                g.adjust(Measurement::pt(amount * per_glue(g) / spring));
            }
        }
    }
    content
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const INF_PENALTY: i32 = 10_000;
    use crate::frame::PaperSize;
    use crate::length::Length;
    use crate::measurement::Measurement;
    use crate::node::VBox;

    fn make_line(height: f64, depth: f64) -> Node {
        Node::VBox(VBox {
            width: Length::pt(300.0),
            height: Length::pt(height),
            depth: Length::pt(depth),
            nodes: vec![Node::hbox(300.0, height, depth)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
        })
    }

    fn make_vglue(height: f64) -> Node {
        Node::vglue(Length::new(
            Measurement::pt(height),
            Measurement::pt(height * 0.5),
            Measurement::pt(height * 0.3),
        ))
    }

    fn make_paragraph(num_lines: usize, line_height: f64, line_depth: f64) -> Vec<Node> {
        let mut nodes = Vec::new();
        for i in 0..num_lines {
            if i > 0 {
                nodes.push(make_vglue(2.0));
            }
            nodes.push(make_line(line_height, line_depth));
        }
        nodes
    }



    // -- PageBuilder find_break --

    #[test]
    fn find_break_empty() {
        let pb = PageBuilder::new(PageBreakSettings::default());
        assert!(pb.find_break(600.0).is_none());
    }

    #[test]
    fn find_break_at_penalty() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(make_vglue(2.0));
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(Node::penalty(0));
        pb.enqueue(make_line(12.0, 3.0));
        assert!(pb.find_break(35.0).is_none(), "no overflow yet");

        pb.enqueue(make_vglue(2.0));
        let result = pb.find_break(35.0).unwrap();
        assert_eq!(result.break_index, 3); // at the penalty
    }

    #[test]
    fn find_break_forced_eject() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(Node::penalty(EJECT_PENALTY));
        pb.enqueue(make_line(12.0, 3.0));

        let result = pb.find_break(600.0).unwrap();
        assert_eq!(result.penalty, EJECT_PENALTY);
        assert_eq!(result.break_index, 1);
    }

    #[test]
    fn find_break_no_break_at_inf_penalty() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(make_vglue(2.0));
        pb.enqueue(Node::penalty(INF_PENALTY));
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(Node::penalty(0));
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(make_vglue(2.0));

        let result = pb.find_break(33.0).unwrap();
        assert_eq!(result.break_index, 4);
    }

    #[test]
    fn page_glue_stretches_to_fill_frame() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(make_vglue(2.0));
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(Node::penalty(EJECT_PENALTY));
        let content = pb.take_page(32.5, false).unwrap();
        let Node::VGlue(g) = &content[1] else { panic!("expected glue") };
        assert_eq!(g.adjustment.to_pt(), Some(0.5));
    }

    // -- PageBuilder build_pages --

    #[test]
    fn build_single_page() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        let nodes = make_paragraph(5, 12.0, 3.0);
        pb.enqueue_many(nodes);
        // Add an eject penalty to force page
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();
        let pages = pb.build_pages(&layout, frame_id);

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].number, 1);
        assert_eq!(pages[0].frames.len(), 1);
        assert_eq!(pages[0].frames[0].0, frame_id);
    }

    #[test]
    fn build_multiple_pages() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());

        // Create enough lines to fill ~3 pages
        // A4 content height with 72pt margin ≈ 698pt
        // Each line: 12pt + 3pt + 2pt glue = 17pt → ~41 lines per page
        let nodes = make_paragraph(120, 12.0, 3.0);
        for node in &nodes {
            pb.enqueue(node.clone());
        }
        // Force final page
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();
        let pages = pb.build_pages(&layout, frame_id);

        assert!(
            pages.len() >= 2,
            "expected at least 2 pages, got {}",
            pages.len()
        );

        for page in &pages {
            assert_eq!(page.frames.len(), 1);
        }
    }

    #[test]
    fn build_pages_trims_discardables() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(make_vglue(2.0));
        pb.enqueue(Node::penalty(EJECT_PENALTY));
        pb.enqueue(make_vglue(2.0)); // leading discardable on next page
        pb.enqueue(make_line(12.0, 3.0));
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();
        let pages = pb.build_pages(&layout, frame_id);

        assert_eq!(pages.len(), 2);

        // First page: vglue and penalty trimmed from end
        let p1_content = &pages[0].frames[0].1;
        assert!(
            p1_content.last().unwrap().is_vbox(),
            "last node on page should be a vbox after trimming"
        );

        // Second page: leading vglue should be trimmed
        let p2_content = &pages[1].frames[0].1;
        assert!(
            p2_content.first().unwrap().is_vbox(),
            "first node on page should be a vbox after trimming"
        );
    }


    // -- Multi-frame page building --

    #[test]
    fn build_pages_multi_frame_with_columns() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());

        // Create 80 lines (~80×17pt = 1360pt). Each column is ~698pt tall,
        // so content should overflow into the second column.
        let nodes = make_paragraph(80, 12.0, 3.0);
        for node in &nodes {
            pb.enqueue(node.clone());
        }
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout = PageLayout::two_column(PaperSize::A4, 72.0, 0.0, 0.0, 0.0, 12.0);
        let ids = layout.frame_ids();
        // ids[1] = left_column, ids[2] = right_column (left flows to right)
        let start = ids[1];

        let pages = pb.build_pages_multi_frame(&layout, start);

        assert!(!pages.is_empty());

        // At least one page should have content in both columns
        let multi_frame_page = pages.iter().find(|p| p.frames.len() >= 2);
        assert!(
            multi_frame_page.is_some(),
            "at least one page should use both columns"
        );
    }

    #[test]
    fn build_pages_header_footer() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());

        // Enough content for ~2 pages of the body frame
        let nodes = make_paragraph(80, 12.0, 3.0);
        pb.enqueue_many(nodes);
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout =
            PageLayout::with_header_footer(PaperSize::A4, 72.0, 30.0, 20.0, 10.0);
        let content_id = layout.content_frame_id().unwrap();

        let pages = pb.build_pages(&layout, content_id);

        assert!(
            pages.len() >= 2,
            "expected at least 2 pages, got {}",
            pages.len()
        );
    }

    // -- End-to-end: paragraph → page break --

    #[test]
    fn end_to_end_paragraph_to_pages() {
        let mut pb = PageBuilder::new(PageBreakSettings::default());

        // Simulate 3 paragraphs with inter-paragraph glue and penalty
        for _para in 0..3 {
            pb.enqueue_many(make_paragraph(15, 12.0, 3.0));
            pb.enqueue(make_vglue(6.0)); // paragraph skip
            pb.enqueue(Node::penalty(0)); // allow break between paragraphs
        }
        pb.enqueue(Node::penalty(EJECT_PENALTY));

        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();
        let pages = pb.build_pages(&layout, frame_id);

        assert!(!pages.is_empty());
        // Verify all pages have content
        for page in &pages {
            assert!(!page.frames.is_empty());
            assert!(!page.frames[0].1.is_empty());
        }
    }
}
