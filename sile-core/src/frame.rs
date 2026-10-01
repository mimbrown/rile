use std::collections::BTreeMap;

use cassowary::strength::{REQUIRED, STRONG};
use cassowary::{AddConstraintError, Solver, Variable, WeightedRelation::*};

use crate::node::Node;

// ---------------------------------------------------------------------------
// WritingDirection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WritingDirection {
    #[default]
    LtrTtb,
    RtlTtb,
    TtbLtr,
    TtbRtl,
    BttLtr,
    BttRtl,
    LtrBtt,
    RtlBtt,
}

impl WritingDirection {
    pub fn is_horizontal(self) -> bool {
        matches!(
            self,
            WritingDirection::LtrTtb
                | WritingDirection::RtlTtb
                | WritingDirection::LtrBtt
                | WritingDirection::RtlBtt
        )
    }

    pub fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }

    pub fn is_rtl(self) -> bool {
        matches!(
            self,
            WritingDirection::RtlTtb | WritingDirection::RtlBtt
        )
    }
}

// ---------------------------------------------------------------------------
// FrameId
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameId(pub u32);

impl std::fmt::Display for FrameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "frame:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Frame {
    pub id: FrameId,
    pub name: String,
    pub direction: WritingDirection,
    pub next: Option<FrameId>,
    pub balanced: bool,

    // Cassowary variables for constraint-based geometry
    pub var_left: Variable,
    pub var_top: Variable,
    pub var_right: Variable,
    pub var_bottom: Variable,

    // Resolved absolute positions (filled after solving)
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,

    // Content accumulated in this frame
    pub content: Vec<Node>,
}

impl Frame {
    pub fn new(id: FrameId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            direction: WritingDirection::default(),
            next: None,
            balanced: false,
            var_left: Variable::new(),
            var_top: Variable::new(),
            var_right: Variable::new(),
            var_bottom: Variable::new(),
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            content: Vec::new(),
        }
    }

    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }

    pub fn content_height(&self) -> f64 {
        self.content.iter().fold(0.0, |acc, node| {
            let h = node.height().to_pt().unwrap_or(0.0);
            let d = node.depth().to_pt().unwrap_or(0.0);
            acc + h + d
        })
    }

    pub fn remaining_height(&self) -> f64 {
        self.height() - self.content_height()
    }

    pub fn is_full(&self) -> bool {
        self.remaining_height() <= 0.0
    }

    pub fn clear_content(&mut self) {
        self.content.clear();
    }

    pub fn push_content(&mut self, node: Node) {
        self.content.push(node);
    }

    /// The main dimension for content flow in this frame's writing direction.
    pub fn flow_length(&self) -> f64 {
        if self.direction.is_horizontal() {
            self.height()
        } else {
            self.width()
        }
    }

    /// The cross dimension for line measurement.
    pub fn line_length(&self) -> f64 {
        if self.direction.is_horizontal() {
            self.width()
        } else {
            self.height()
        }
    }
}

impl Clone for Frame {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            direction: self.direction,
            next: self.next,
            balanced: self.balanced,
            var_left: Variable::new(),
            var_top: Variable::new(),
            var_right: Variable::new(),
            var_bottom: Variable::new(),
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            content: self.content.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// PaperSize
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperSize {
    pub width: f64,
    pub height: f64,
}

impl PaperSize {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub const A4: Self = Self::new(595.276, 841.89);
    pub const A5: Self = Self::new(419.528, 595.276);
    pub const A3: Self = Self::new(841.89, 1190.551);
    pub const LETTER: Self = Self::new(612.0, 792.0);
    pub const LEGAL: Self = Self::new(612.0, 1008.0);
    pub const B5: Self = Self::new(498.898, 708.661);

    pub fn landscape(self) -> Self {
        Self::new(self.height, self.width)
    }
}

// ---------------------------------------------------------------------------
// FrameConstraint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FrameConstraint {
    /// Frame left edge at absolute position.
    Left(FrameId, f64),
    /// Frame top edge at absolute position.
    Top(FrameId, f64),
    /// Frame right edge at absolute position.
    Right(FrameId, f64),
    /// Frame bottom edge at absolute position.
    Bottom(FrameId, f64),
    /// Frame width equals a value.
    Width(FrameId, f64),
    /// Frame height equals a value.
    Height(FrameId, f64),
    /// One frame's left == another frame's right + gap.
    LeftAfterRight(FrameId, FrameId, f64),
    /// One frame's top == another frame's bottom + gap.
    TopAfterBottom(FrameId, FrameId, f64),
    /// Two frames share the same left edge.
    AlignLeft(FrameId, FrameId),
    /// Two frames share the same right edge.
    AlignRight(FrameId, FrameId),
    /// Two frames share the same top edge.
    AlignTop(FrameId, FrameId),
    /// Two frames share the same bottom edge.
    AlignBottom(FrameId, FrameId),
    /// Two frames share the same width.
    EqualWidth(FrameId, FrameId),
    /// Two frames share the same height.
    EqualHeight(FrameId, FrameId),
}

// ---------------------------------------------------------------------------
// PageLayout
// ---------------------------------------------------------------------------

pub struct PageLayout {
    pub paper: PaperSize,
    pub frames: BTreeMap<FrameId, Frame>,
    next_id: u32,
    solver: Solver,
    page_left: Variable,
    page_top: Variable,
    page_right: Variable,
    page_bottom: Variable,
    solved: bool,
}

impl PageLayout {
    pub fn new(paper: PaperSize) -> Self {
        Self {
            paper,
            frames: BTreeMap::new(),
            next_id: 0,
            solver: Solver::new(),
            page_left: Variable::new(),
            page_top: Variable::new(),
            page_right: Variable::new(),
            page_bottom: Variable::new(),
            solved: false,
        }
    }

    pub fn add_frame(&mut self, name: impl Into<String>) -> FrameId {
        let id = FrameId(self.next_id);
        self.next_id += 1;
        let frame = Frame::new(id, name);
        self.frames.insert(id, frame);
        self.solved = false;
        id
    }

    pub fn frame(&self, id: FrameId) -> &Frame {
        &self.frames[&id]
    }

    pub fn frame_mut(&mut self, id: FrameId) -> &mut Frame {
        self.frames.get_mut(&id).expect("frame not found")
    }

    pub fn set_next(&mut self, from: FrameId, to: FrameId) {
        self.frames.get_mut(&from).expect("frame not found").next = Some(to);
    }

    pub fn set_direction(&mut self, id: FrameId, dir: WritingDirection) {
        self.frames.get_mut(&id).expect("frame not found").direction = dir;
    }

    pub fn solve(
        &mut self,
        constraints: &[FrameConstraint],
    ) -> Result<(), AddConstraintError> {
        self.solver.reset();
        self.solver = Solver::new();

        // Page boundary constraints (required)
        self.solver
            .add_constraint(self.page_left | EQ(REQUIRED) | 0.0)?;
        self.solver
            .add_constraint(self.page_top | EQ(REQUIRED) | 0.0)?;
        self.solver
            .add_constraint(self.page_right | EQ(REQUIRED) | self.paper.width)?;
        self.solver
            .add_constraint(self.page_bottom | EQ(REQUIRED) | self.paper.height)?;

        // Frame sanity constraints: right >= left, bottom >= top
        for frame in self.frames.values() {
            self.solver
                .add_constraint(frame.var_right | GE(REQUIRED) | frame.var_left)?;
            self.solver
                .add_constraint(frame.var_bottom | GE(REQUIRED) | frame.var_top)?;
            // Frames stay within page bounds (strong, not required, to allow bleeding)
            self.solver
                .add_constraint(frame.var_left | GE(STRONG) | 0.0)?;
            self.solver
                .add_constraint(frame.var_top | GE(STRONG) | 0.0)?;
            self.solver
                .add_constraint(frame.var_right | LE(STRONG) | self.paper.width)?;
            self.solver
                .add_constraint(frame.var_bottom | LE(STRONG) | self.paper.height)?;
        }

        for c in constraints {
            self.add_constraint(c)?;
        }

        // Read solved values
        for frame in self.frames.values_mut() {
            frame.left = self.solver.get_value(frame.var_left);
            frame.top = self.solver.get_value(frame.var_top);
            frame.right = self.solver.get_value(frame.var_right);
            frame.bottom = self.solver.get_value(frame.var_bottom);
        }

        self.solved = true;
        Ok(())
    }

    fn add_constraint(&mut self, c: &FrameConstraint) -> Result<(), AddConstraintError> {
        match c {
            FrameConstraint::Left(id, val) => {
                let v = self.frames[id].var_left;
                self.solver.add_constraint(v | EQ(STRONG) | *val)
            }
            FrameConstraint::Top(id, val) => {
                let v = self.frames[id].var_top;
                self.solver.add_constraint(v | EQ(STRONG) | *val)
            }
            FrameConstraint::Right(id, val) => {
                let v = self.frames[id].var_right;
                self.solver.add_constraint(v | EQ(STRONG) | *val)
            }
            FrameConstraint::Bottom(id, val) => {
                let v = self.frames[id].var_bottom;
                self.solver.add_constraint(v | EQ(STRONG) | *val)
            }
            FrameConstraint::Width(id, val) => {
                let f = &self.frames[id];
                self.solver
                    .add_constraint((f.var_right - f.var_left) | EQ(STRONG) | *val)
            }
            FrameConstraint::Height(id, val) => {
                let f = &self.frames[id];
                self.solver
                    .add_constraint((f.var_bottom - f.var_top) | EQ(STRONG) | *val)
            }
            FrameConstraint::LeftAfterRight(left_id, right_id, gap) => {
                let l = self.frames[left_id].var_left;
                let r = self.frames[right_id].var_right;
                self.solver
                    .add_constraint((l - r) | EQ(STRONG) | *gap)
            }
            FrameConstraint::TopAfterBottom(top_id, bottom_id, gap) => {
                let t = self.frames[top_id].var_top;
                let b = self.frames[bottom_id].var_bottom;
                self.solver
                    .add_constraint((t - b) | EQ(STRONG) | *gap)
            }
            FrameConstraint::AlignLeft(a, b) => {
                let va = self.frames[a].var_left;
                let vb = self.frames[b].var_left;
                self.solver.add_constraint(va | EQ(STRONG) | vb)
            }
            FrameConstraint::AlignRight(a, b) => {
                let va = self.frames[a].var_right;
                let vb = self.frames[b].var_right;
                self.solver.add_constraint(va | EQ(STRONG) | vb)
            }
            FrameConstraint::AlignTop(a, b) => {
                let va = self.frames[a].var_top;
                let vb = self.frames[b].var_top;
                self.solver.add_constraint(va | EQ(STRONG) | vb)
            }
            FrameConstraint::AlignBottom(a, b) => {
                let va = self.frames[a].var_bottom;
                let vb = self.frames[b].var_bottom;
                self.solver.add_constraint(va | EQ(STRONG) | vb)
            }
            FrameConstraint::EqualWidth(a, b) => {
                let fa = &self.frames[a];
                let fb = &self.frames[b];
                self.solver.add_constraint(
                    (fa.var_right - fa.var_left) | EQ(STRONG) | (fb.var_right - fb.var_left),
                )
            }
            FrameConstraint::EqualHeight(a, b) => {
                let fa = &self.frames[a];
                let fb = &self.frames[b];
                self.solver.add_constraint(
                    (fa.var_bottom - fa.var_top) | EQ(STRONG) | (fb.var_bottom - fb.var_top),
                )
            }
        }
    }

    /// Create a plain layout: single content frame with margins.
    pub fn plain(paper: PaperSize, margin: f64) -> Self {
        let mut layout = Self::new(paper);
        let content = layout.add_frame("content");
        let constraints = vec![
            FrameConstraint::Left(content, margin),
            FrameConstraint::Top(content, margin),
            FrameConstraint::Right(content, paper.width - margin),
            FrameConstraint::Bottom(content, paper.height - margin),
        ];
        layout.solve(&constraints).expect("plain layout solve failed");
        layout
    }

    /// Create a layout with header, body, and footer frames.
    pub fn with_header_footer(
        paper: PaperSize,
        margin: f64,
        header_height: f64,
        footer_height: f64,
        gap: f64,
    ) -> Self {
        let mut layout = Self::new(paper);
        let header = layout.add_frame("header");
        let content = layout.add_frame("content");
        let footer = layout.add_frame("footer");

        // Content flows from header → content → footer
        layout.set_next(header, content);
        layout.set_next(content, footer);

        let body_top = margin + header_height + gap;
        let body_bottom = paper.height - margin - footer_height - gap;

        let constraints = vec![
            // Header
            FrameConstraint::Left(header, margin),
            FrameConstraint::Top(header, margin),
            FrameConstraint::Right(header, paper.width - margin),
            FrameConstraint::Height(header, header_height),
            // Content
            FrameConstraint::Left(content, margin),
            FrameConstraint::Top(content, body_top),
            FrameConstraint::Right(content, paper.width - margin),
            FrameConstraint::Bottom(content, body_bottom),
            // Footer
            FrameConstraint::Left(footer, margin),
            FrameConstraint::Bottom(footer, paper.height - margin),
            FrameConstraint::Right(footer, paper.width - margin),
            FrameConstraint::Height(footer, footer_height),
        ];

        layout.solve(&constraints).expect("header/footer layout solve failed");
        layout
    }

    /// Create a two-column layout with header and footer.
    pub fn two_column(
        paper: PaperSize,
        margin: f64,
        header_height: f64,
        footer_height: f64,
        gap: f64,
        column_gap: f64,
    ) -> Self {
        let mut layout = Self::new(paper);
        let header = layout.add_frame("header");
        let left_col = layout.add_frame("left_column");
        let right_col = layout.add_frame("right_column");
        let footer = layout.add_frame("footer");

        layout.set_next(left_col, right_col);

        let body_top = margin + header_height + gap;
        let body_bottom = paper.height - margin - footer_height - gap;
        let content_width = paper.width - 2.0 * margin;
        let col_width = (content_width - column_gap) / 2.0;

        let constraints = vec![
            // Header
            FrameConstraint::Left(header, margin),
            FrameConstraint::Top(header, margin),
            FrameConstraint::Right(header, paper.width - margin),
            FrameConstraint::Height(header, header_height),
            // Left column
            FrameConstraint::Left(left_col, margin),
            FrameConstraint::Top(left_col, body_top),
            FrameConstraint::Width(left_col, col_width),
            FrameConstraint::Bottom(left_col, body_bottom),
            // Right column
            FrameConstraint::LeftAfterRight(right_col, left_col, column_gap),
            FrameConstraint::Top(right_col, body_top),
            FrameConstraint::Width(right_col, col_width),
            FrameConstraint::Bottom(right_col, body_bottom),
            // Footer
            FrameConstraint::Left(footer, margin),
            FrameConstraint::Bottom(footer, paper.height - margin),
            FrameConstraint::Right(footer, paper.width - margin),
            FrameConstraint::Height(footer, footer_height),
        ];

        layout.solve(&constraints).expect("two-column layout solve failed");
        layout
    }

    pub fn frame_ids(&self) -> Vec<FrameId> {
        let mut ids: Vec<_> = self.frames.keys().copied().collect();
        ids.sort_by_key(|id| id.0);
        ids
    }

    pub fn content_frame(&self) -> Option<&Frame> {
        self.frames.values().find(|f| f.name == "content")
    }

    pub fn content_frame_id(&self) -> Option<FrameId> {
        self.frame_id_by_name("content")
    }

    pub fn frame_id_by_name(&self, name: &str) -> Option<FrameId> {
        self.frames.values().find(|f| f.name == name).map(|f| f.id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.5
    }

    // -- PaperSize --

    #[test]
    fn paper_size_a4() {
        let a4 = PaperSize::A4;
        assert!(approx_eq(a4.width, 595.276));
        assert!(approx_eq(a4.height, 841.89));
    }

    #[test]
    fn paper_size_landscape() {
        let l = PaperSize::A4.landscape();
        assert!(approx_eq(l.width, 841.89));
        assert!(approx_eq(l.height, 595.276));
    }

    // -- WritingDirection --

    #[test]
    fn writing_direction_defaults_horizontal() {
        let d = WritingDirection::default();
        assert!(d.is_horizontal());
        assert!(!d.is_vertical());
    }

    #[test]
    fn writing_direction_rtl() {
        assert!(WritingDirection::RtlTtb.is_rtl());
        assert!(!WritingDirection::LtrTtb.is_rtl());
    }

    #[test]
    fn writing_direction_vertical() {
        assert!(WritingDirection::TtbLtr.is_vertical());
        assert!(WritingDirection::TtbRtl.is_vertical());
    }

    // -- Plain layout --

    #[test]
    fn plain_layout_single_frame() {
        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        assert_eq!(layout.frames.len(), 1);
        let f = layout.content_frame().unwrap();
        assert!(approx_eq(f.left, 72.0));
        assert!(approx_eq(f.top, 72.0));
        assert!(approx_eq(f.right, 595.276 - 72.0));
        assert!(approx_eq(f.bottom, 841.89 - 72.0));
        assert!(approx_eq(f.width(), 595.276 - 144.0));
        assert!(approx_eq(f.height(), 841.89 - 144.0));
    }

    // -- Header/footer layout --

    #[test]
    fn header_footer_layout() {
        let layout =
            PageLayout::with_header_footer(PaperSize::A4, 72.0, 30.0, 20.0, 10.0);
        assert_eq!(layout.frames.len(), 3);

        let ids = layout.frame_ids();
        let header = layout.frame(ids[0]);
        let content = layout.frame(ids[1]);
        let footer = layout.frame(ids[2]);

        assert_eq!(header.name, "header");
        assert_eq!(content.name, "content");
        assert_eq!(footer.name, "footer");

        // Header at top
        assert!(approx_eq(header.top, 72.0));
        assert!(approx_eq(header.height(), 30.0));

        // Content between header and footer
        assert!(approx_eq(content.top, 72.0 + 30.0 + 10.0));
        assert!(approx_eq(content.bottom, 841.89 - 72.0 - 20.0 - 10.0));

        // Footer at bottom
        assert!(approx_eq(footer.height(), 20.0));
        assert!(approx_eq(footer.bottom, 841.89 - 72.0));

        // Frame chaining
        assert_eq!(header.next, Some(content.id));
        assert_eq!(content.next, Some(footer.id));
        assert_eq!(footer.next, None);
    }

    // -- Two-column layout --

    #[test]
    fn two_column_layout() {
        let layout =
            PageLayout::two_column(PaperSize::LETTER, 72.0, 24.0, 18.0, 8.0, 12.0);
        assert_eq!(layout.frames.len(), 4);

        let ids = layout.frame_ids();
        let left_col = layout.frame(ids[1]);
        let right_col = layout.frame(ids[2]);

        // Both columns equal width
        let expected_col_width = (612.0 - 144.0 - 12.0) / 2.0;
        assert!(approx_eq(left_col.width(), expected_col_width));
        assert!(approx_eq(right_col.width(), expected_col_width));

        // Right column starts after left column + gap
        assert!(approx_eq(right_col.left, left_col.right + 12.0));

        // Left flows to right
        assert_eq!(left_col.next, Some(right_col.id));
    }

    // -- Custom constraints --

    #[test]
    fn custom_constraints() {
        let mut layout = PageLayout::new(PaperSize::A4);
        let a = layout.add_frame("a");
        let b = layout.add_frame("b");

        let constraints = vec![
            FrameConstraint::Left(a, 50.0),
            FrameConstraint::Top(a, 50.0),
            FrameConstraint::Width(a, 200.0),
            FrameConstraint::Height(a, 300.0),
            FrameConstraint::AlignLeft(b, a),
            FrameConstraint::TopAfterBottom(b, a, 20.0),
            FrameConstraint::EqualWidth(b, a),
            FrameConstraint::Height(b, 100.0),
        ];

        layout.solve(&constraints).unwrap();

        let fa = layout.frame(a);
        assert!(approx_eq(fa.left, 50.0));
        assert!(approx_eq(fa.top, 50.0));
        assert!(approx_eq(fa.width(), 200.0));
        assert!(approx_eq(fa.height(), 300.0));

        let fb = layout.frame(b);
        assert!(approx_eq(fb.left, 50.0));
        assert!(approx_eq(fb.top, 370.0));
        assert!(approx_eq(fb.width(), 200.0));
        assert!(approx_eq(fb.height(), 100.0));
    }

    // -- Frame content tracking --

    #[test]
    fn frame_content_height() {
        let mut layout = PageLayout::plain(PaperSize::A4, 72.0);
        let id = layout.content_frame_id().unwrap();
        let frame = layout.frame_mut(id);

        assert_eq!(frame.content_height(), 0.0);
        assert!(!frame.is_full());

        frame.push_content(Node::hbox(100.0, 12.0, 3.0));
        assert!((frame.content_height() - 15.0).abs() < 0.01);

        frame.push_content(Node::hbox(100.0, 12.0, 3.0));
        assert!((frame.content_height() - 30.0).abs() < 0.01);
    }

    // -- Frame flow/line lengths --

    #[test]
    fn frame_flow_horizontal() {
        let layout = PageLayout::plain(PaperSize::A4, 72.0);
        let f = layout.content_frame().unwrap();
        assert!(approx_eq(f.line_length(), f.width()));
        assert!(approx_eq(f.flow_length(), f.height()));
    }

    #[test]
    fn frame_flow_vertical() {
        let mut layout = PageLayout::plain(PaperSize::A4, 72.0);
        let id = layout.content_frame_id().unwrap();
        layout.set_direction(id, WritingDirection::TtbLtr);
        let f = layout.frame(id);
        assert!(approx_eq(f.line_length(), f.height()));
        assert!(approx_eq(f.flow_length(), f.width()));
    }

    // -- FrameId display --

    #[test]
    fn frame_id_display() {
        assert_eq!(FrameId(3).to_string(), "frame:3");
    }

    // -- AlignRight / AlignBottom --

    #[test]
    fn align_right_bottom() {
        let mut layout = PageLayout::new(PaperSize::LETTER);
        let a = layout.add_frame("a");
        let b = layout.add_frame("b");

        let constraints = vec![
            FrameConstraint::Left(a, 50.0),
            FrameConstraint::Top(a, 50.0),
            FrameConstraint::Right(a, 300.0),
            FrameConstraint::Bottom(a, 400.0),
            FrameConstraint::AlignRight(b, a),
            FrameConstraint::AlignBottom(b, a),
            FrameConstraint::Width(b, 100.0),
            FrameConstraint::Height(b, 50.0),
        ];

        layout.solve(&constraints).unwrap();

        let fb = layout.frame(b);
        assert!(approx_eq(fb.right, 300.0));
        assert!(approx_eq(fb.bottom, 400.0));
    }
}
