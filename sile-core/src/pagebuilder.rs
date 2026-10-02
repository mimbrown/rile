use crate::color::Color;
use crate::frame::PaperSize;
use crate::framespec::FrameGeometry;
use crate::measurement::Measurement;
use crate::node::Node;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const INF_BAD: i64 = 10_000;
const EJECT_PENALTY: i32 = -10_000;
/// SILE's `supereject` penalty: ends the page even when the frame has a
/// `next` frame.
pub const SUPER_EJECT: i32 = -20_000;
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
    /// Penalty at the node that ended the search: the forced break, or
    /// whatever overflowed the frame (SILE's `lastPenalty`).
    pub trigger_penalty: i32,
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// A finished page: its frames, and what was set into each.
#[derive(Debug, Clone)]
pub struct Page {
    pub number: usize,
    pub paper: PaperSize,
    pub frames: Vec<FrameGeometry>,
    /// Material set into frames, by frame id; each entry starts at its
    /// frame's top.
    pub content: Vec<(String, Vec<Node>)>,
    /// Frames to draw the outline of, as they were when asked for.
    pub outlines: Vec<FrameGeometry>,
    /// What is drawn before any content.
    pub underlay: Vec<Underlay>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Underlay {
    /// Rules (x, y, width, height), in a colour if given.
    Rules(Option<Color>, Vec<[f64; 4]>),
    /// An image over (x, y, width, height).
    Image(std::sync::Arc<crate::image::Image>, [f64; 4]),
}

impl Page {
    pub fn new(number: usize, paper: PaperSize, frames: Vec<FrameGeometry>) -> Self {
        Self { number, paper, frames, content: Vec::new(), outlines: Vec::new(), underlay: Vec::new() }
    }

    pub fn frame(&self, id: &str) -> Option<&FrameGeometry> {
        self.frames.iter().find(|f| f.id == id)
    }

    pub fn add_frame_content(&mut self, id: impl Into<String>, nodes: Vec<Node>) {
        self.content.push((id.into(), nodes));
    }
}

// ---------------------------------------------------------------------------
// PageBuilder
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PageBuilder {
    pub settings: PageBreakSettings,
    queue: Vec<Node>,
}

impl PageBuilder {
    pub fn new(settings: PageBreakSettings) -> Self {
        Self { settings, queue: Vec::new() }
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

    pub fn find_break(&mut self, target_height: f64) -> Option<PageBreakResult> {
        find_break(&mut self.queue, target_height, false, &mut no_insertions)
    }

    pub fn take_page(&mut self, target_height: f64, flush: bool) -> Option<Vec<Node>> {
        take_page(&mut self.queue, target_height, flush).map(|(nodes, _)| nodes)
    }
}

/// Called when the page builder meets an insertion at `index`, with the
/// height so far and the current target. It may rewrite the queue (to
/// force a break before the insertion) and returns the new target.
pub type InsertionHook<'a> = dyn FnMut(&mut Vec<Node>, usize, f64, f64) -> f64 + 'a;

pub fn no_insertions(_: &mut Vec<Node>, _: usize, _: f64, target: f64) -> f64 {
    target
}

/// Find the best page break in the queue for a frame of `target_height`
/// (SILE's `findBestBreak`). Legal breaks are penalties below 10000 and
/// glue after non-discardable material. Returns `None` until the queue
/// holds a forced break or overflows the frame, unless `force` asks for the
/// best break so far.
pub fn find_break(
    queue: &mut Vec<Node>,
    target_height: f64,
    force: bool,
    on_insertion: &mut InsertionHook,
) -> Option<PageBreakResult> {
    let mut target = target_height;
    let mut i = queue.iter().position(|n| !n.is_vglue()).unwrap_or(queue.len());
    let (mut height, mut stretch, mut shrink) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut least_cost = INF_BAD;
    let mut best: Option<PageBreakResult> = None;
    let mut pi = 0;

    while i < queue.len() {
        match &queue[i] {
            node @ Node::VBox(_) => height += pt(node.height()) + pt(node.depth()),
            Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) => {
                height += g.height.length.to_pt().unwrap_or(0.0);
                stretch += g.height.stretch.to_pt().unwrap_or(0.0);
                shrink += g.height.shrink.to_pt().unwrap_or(0.0);
            }
            Node::Insertion(_) => target = on_insertion(queue, i, height, target),
            _ => {}
        }
        let node = &queue[i];
        pi = match node {
            Node::Penalty(p) => p.penalty,
            _ => 0,
        };
        let legal = match node {
            Node::Penalty(p) => (p.penalty as i64) < INF_BAD,
            n => n.is_vglue() && i > 0 && !queue[i - 1].is_discardable(),
        };
        if legal {
            let left = target - height;
            let badness = if height < target {
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
            let here = PageBreakResult { break_index: i, badness, penalty: pi, cost, trigger_penalty: pi };
            if cost < least_cost {
                least_cost = cost;
                best = Some(here.clone());
            }
            if cost == AWFUL_BAD || pi <= EJECT_PENALTY {
                return Some(PageBreakResult { trigger_penalty: pi, ..best.unwrap_or(here) });
            }
        }
        i += 1;
    }
    if force {
        return best.map(|b| PageBreakResult { trigger_penalty: pi, ..b });
    }
    None
}

/// Take the material up to and including the break off the queue, without
/// the discardables it ends with.
/// SILE's grid page builder: fill the frame line by line and break before
/// the first node that overflows it or is a penalty.
pub fn find_grid_break(queue: &mut Vec<Node>, target_height: f64, on_insertion: &mut InsertionHook) -> Option<PageBreakResult> {
    let mut target = target_height;
    let mut i = queue.iter().position(|n| !n.is_vglue()).unwrap_or(queue.len());
    let mut height = 0.0;
    let mut best = None;
    while i < queue.len() {
        match &queue[i] {
            node @ Node::VBox(_) => height += pt(node.height()) + pt(node.depth()),
            node if node.is_vglue() => height += pt(node.height()),
            Node::Insertion(_) => target = on_insertion(queue, i, height, target),
            _ => {}
        }
        let left = target - height;
        let mut badness = if left < 0.0 { 1_000_000.0 } else { 0.0 };
        if let Node::Penalty(p) = &queue[i] {
            badness = if p.penalty < -3000 { 100_000.0 } else { -left * left - p.penalty as f64 };
        }
        if badness > 0.0 {
            let break_index = best.unwrap_or(0);
            return Some(PageBreakResult { break_index, badness: 0, penalty: 0, cost: 0, trigger_penalty: 1000 });
        }
        best = Some(i);
        i += 1;
    }
    None
}

pub fn split_page(queue: &mut Vec<Node>, br: &PageBreakResult) -> Vec<Node> {
    let mut content: Vec<Node> = queue.drain(..=br.break_index).collect();
    while content.len() > 1 && content.last().is_some_and(Node::is_discardable) {
        content.pop();
    }
    content
}

/// Take a frame's worth of material off the queue, glue set to fill
/// `target_height`, with the penalty that ended the search. With `flush`,
/// everything goes when there is no break yet.
pub fn take_page(queue: &mut Vec<Node>, target_height: f64, flush: bool) -> Option<(Vec<Node>, i32)> {
    if queue.is_empty() {
        return None;
    }
    let (content, penalty) = match find_break(queue, target_height, false, &mut no_insertions) {
        Some(br) => (split_page(queue, &br), br.trigger_penalty),
        None if flush => {
            let all = PageBreakResult { break_index: queue.len() - 1, badness: 0, penalty: 0, cost: 0, trigger_penalty: 0 };
            (split_page(queue, &all), 0)
        }
        None => return None,
    };
    Some((set_vertical_glue(content, target_height), penalty))
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
pub fn set_vertical_glue(content: Vec<Node>, target: f64) -> Vec<Node> {
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
        let mut pb = PageBuilder::new(PageBreakSettings::default());
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

    #[test]
    fn overflow_reports_the_penalty_that_ended_the_search() {
        let mut queue = make_paragraph(10, 12.0, 3.0);
        queue.insert(5, Node::penalty(-500));
        let (page, penalty) = take_page(&mut queue, 50.0, false).unwrap();
        assert_eq!(penalty, 0, "the overflowing glue, not the break taken");
        assert_eq!(page.iter().filter(|n| matches!(n, Node::VBox(_))).count(), 3);
        assert!(!queue.is_empty());
    }
}
