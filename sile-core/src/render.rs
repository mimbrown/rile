use crate::node::{HBox, Ink, Leader, NNode, Node};
use crate::color::Color;
use crate::font::Direction;
use crate::framespec::FrameGeometry;
use crate::pagebuilder::Page;

/// A drawing surface for laid-out pages. Coordinates are in points, measured
/// from the top-left corner of the page, with `y` growing downwards.
pub trait Canvas {
    fn begin_page(&mut self, width: f64, height: f64);
    fn end_page(&mut self);
    fn glyphs(&mut self, nnode: &NNode, x: f64, baseline_y: f64);
    /// A filled rectangle with its top-left corner at `(x, y)`.
    fn rule(&mut self, x: f64, y: f64, width: f64, height: f64);
    /// A frame's outline, for debugging layouts (SILE's `\showframe`).
    fn frame_outline(&mut self, _frame: &FrameGeometry) {}
    /// Ink what follows in `color` until `pop_color`.
    fn push_color(&mut self, _color: Color) {}
    fn pop_color(&mut self) {}
}

/// Walk every page and draw its frames' content onto `canvas`.
pub fn draw_pages(pages: &[Page], canvas: &mut impl Canvas) {
    for page in pages {
        canvas.begin_page(page.paper.width, page.paper.height);
        for (color, rules) in &page.underlay {
            canvas.push_color(*color);
            for [x, y, width, height] in rules {
                canvas.rule(*x, *y, *width, *height);
            }
            canvas.pop_color();
        }
        draw_page(page, canvas);
        for frame in &page.outlines {
            canvas.frame_outline(frame);
        }
        canvas.end_page();
    }
}

fn draw_page(page: &Page, canvas: &mut impl Canvas) {
    for (frame_id, nodes) in &page.content {
        let Some(frame) = page.frame(frame_id) else { continue };
        let mut cursor_y = frame.top;

        for node in nodes {
            match node {
                Node::VBox(vbox) => {
                    let height = pt(&vbox.height.length);
                    let depth = pt(&vbox.depth.length);
                    let rtl = frame.direction == Some(Direction::RTL);
                    let line = Line { ratio: vbox.ratio, end_edge: frame.right, rtl };
                    let start = if rtl { frame.right } else { frame.left };
                    draw_hlist(&vbox.nodes, start, cursor_y + height, &line, canvas);
                    cursor_y += height + depth;
                }
                Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) => {
                    cursor_y += pt(&g.height.length) + g.adjustment.to_pt().unwrap_or(0.0);
                }
                Node::VKern(k) => cursor_y += pt(&k.height.length),
                _ => {}
            }
        }
    }
}

struct Line {
    ratio: f64,
    /// Where leaders line up.
    end_edge: f64,
    /// Set from the right edge leftwards, each node in turn (SILE's RTL
    /// frames).
    rtl: bool,
}

impl Line {
    fn natural(&self) -> Line {
        Line { ratio: 0.0, end_edge: f64::INFINITY, rtl: self.rtl }
    }

    /// SILE's `rationWidth`.
    fn width(&self, node: &Node) -> f64 {
        let width = node.width();
        let (stretch, shrink) = (pt(&width.stretch), pt(&width.shrink));
        let mut w = pt(&width.length);
        if self.ratio > 0.0 && stretch > 0.0 {
            w += stretch * self.ratio;
        } else if self.ratio < 0.0 && shrink > 0.0 {
            w += shrink * self.ratio;
        }
        w
    }
}

/// Draw a line's nodes from `x`, scaling glue by the line's ratio. In
/// right-to-left lines `x` is the right end, and boxes are drawn from their
/// left edge after moving past them, as SILE's frames do.
fn draw_hlist(nodes: &[Node], mut x: f64, mut baseline_y: f64, line: &Line, canvas: &mut impl Canvas) -> f64 {
    let sign = if line.rtl { -1.0 } else { 1.0 };
    for node in nodes {
        match node {
            Node::NNode(nnode) => {
                let width = pt(&nnode.width.length);
                if line.rtl {
                    x -= width;
                    canvas.glyphs(nnode, x, baseline_y);
                } else {
                    canvas.glyphs(nnode, x, baseline_y);
                    x += width;
                }
            }
            Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => {
                let width = line.width(node);
                match &g.leader {
                    Some(Leader::Stroke(s)) => canvas.rule(x, baseline_y - s.raise, sign * width, s.thickness),
                    Some(Leader::Box(b)) => draw_leaders(b, x.min(x + sign * width), width, baseline_y, line, canvas),
                    None => {}
                }
                x += sign * width;
            }
            Node::Kern(_) => x += sign * line.width(node),
            Node::Discretionary(d) => x = draw_hlist(&d.replacement, x, baseline_y, line, canvas),
            Node::HBox(hbox) => match hbox.ink {
                Some(Ink::Rule) => {
                    let (width, height, depth) = (pt(&hbox.width.length), pt(&hbox.height.length), pt(&hbox.depth.length));
                    canvas.rule(x, baseline_y - height, sign * width, height + depth);
                    x += sign * width;
                }
                Some(Ink::Ruby(raise)) => {
                    let start = x + sign * pt(&hbox.width.length);
                    draw_hlist(&hbox.nodes, start, baseline_y - raise, line, canvas);
                }
                Some(Ink::Liner(s)) => {
                    let end = draw_hlist(&hbox.nodes, x, baseline_y, line, canvas);
                    canvas.rule(x, baseline_y - s.raise, end - x, s.thickness);
                    x = end;
                }
                _ => {
                    let width = line.width(node);
                    if line.rtl {
                        x -= width;
                        draw_hlist(&hbox.nodes, x, baseline_y, line, canvas);
                    } else {
                        draw_hlist(&hbox.nodes, x, baseline_y, line, canvas);
                        x += width;
                    }
                    baseline_y -= hbox.raise;
                }
            },
            _ => {}
        }
    }
    x
}

/// As many copies of `pattern` as fit between `x` and `x + width`, placed
/// so that copies on different lines line up from the frame's end edge
/// (SILE's `leader:outputYourself`).
fn draw_leaders(pattern: &HBox, x: f64, width: f64, baseline_y: f64, line: &Line, canvas: &mut impl Canvas) {
    let step = pt(&pattern.width.length);
    if step <= 0.0 || !line.end_edge.is_finite() {
        return;
    }
    let fit = line.end_edge - x;
    let max = (fit / step).floor();
    let skip = ((line.end_edge - x - width) * 1e6).floor() / 1e6;
    let repetitions = max - (skip / step).ceil();
    let mut x = x + fit - max * step;
    for _ in 0..repetitions.max(0.0) as usize {
        draw_hlist(&pattern.nodes, x, baseline_y, &line.natural(), canvas);
        x += step;
    }
}

fn pt(m: &crate::measurement::Measurement) -> f64 {
    m.to_pt().unwrap_or(0.0)
}
