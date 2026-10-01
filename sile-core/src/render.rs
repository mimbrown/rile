use crate::frame::PageLayout;
use crate::node::{NNode, Node};
use crate::pagebuilder::Page;

/// A drawing surface for laid-out pages. Coordinates are in points, measured
/// from the top-left corner of the page, with `y` growing downwards.
pub trait Canvas {
    fn begin_page(&mut self, width: f64, height: f64);
    fn end_page(&mut self);
    fn glyphs(&mut self, nnode: &NNode, x: f64, baseline_y: f64);
}

/// Walk every page and draw its frames' content onto `canvas`.
pub fn draw_pages(pages: &[Page], layout: &PageLayout, canvas: &mut impl Canvas) {
    for page in pages {
        canvas.begin_page(layout.paper.width, layout.paper.height);
        draw_page(page, layout, canvas);
        canvas.end_page();
    }
}

fn draw_page(page: &Page, layout: &PageLayout, canvas: &mut impl Canvas) {
    for (frame_id, nodes) in &page.frames {
        let frame = layout.frame(*frame_id);
        let mut cursor_y = frame.top;

        for node in nodes {
            match node {
                Node::VBox(vbox) => {
                    let height = pt(&vbox.height.length);
                    let depth = pt(&vbox.depth.length);
                    draw_hlist(&vbox.nodes, frame.left, cursor_y + height, vbox.ratio, canvas);
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

/// Draw a line's nodes from `x`, scaling glue by the line's `ratio` the way
/// SILE's `rationWidth` does.
fn draw_hlist(nodes: &[Node], mut x: f64, baseline_y: f64, ratio: f64, canvas: &mut impl Canvas) -> f64 {
    for node in nodes {
        match node {
            Node::NNode(nnode) => {
                canvas.glyphs(nnode, x, baseline_y);
                x += pt(&nnode.width.length);
            }
            Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => {
                let (stretch, shrink) = (pt(&g.width.stretch), pt(&g.width.shrink));
                x += pt(&g.width.length);
                if ratio > 0.0 && stretch > 0.0 {
                    x += stretch * ratio;
                } else if ratio < 0.0 && shrink > 0.0 {
                    x += shrink * ratio;
                }
            }
            Node::Kern(k) => x += pt(&k.width.length),
            Node::HBox(hbox) => {
                draw_hlist(&hbox.nodes, x, baseline_y, 0.0, canvas);
                x += pt(&hbox.width.length);
            }
            _ => {}
        }
    }
    x
}

fn pt(m: &crate::measurement::Measurement) -> f64 {
    m.to_pt().unwrap_or(0.0)
}
