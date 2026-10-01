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
                    let baseline_y = cursor_y + height;
                    let mut cursor_x = frame.left;

                    for hnode in &vbox.nodes {
                        match hnode {
                            Node::NNode(nnode) => {
                                canvas.glyphs(nnode, cursor_x, baseline_y);
                                cursor_x += pt(&nnode.width.length);
                            }
                            Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => {
                                let natural = pt(&g.width.length);
                                let scaled = if vbox.ratio > 0.0 {
                                    natural + pt(&g.width.stretch) * vbox.ratio
                                } else if vbox.ratio < 0.0 {
                                    natural + pt(&g.width.shrink) * vbox.ratio
                                } else {
                                    natural
                                };
                                cursor_x += scaled.max(0.0);
                            }
                            Node::Kern(k) => cursor_x += pt(&k.width.length),
                            Node::HBox(hbox) => cursor_x += pt(&hbox.width.length),
                            _ => {}
                        }
                    }

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

fn pt(m: &crate::measurement::Measurement) -> f64 {
    m.to_pt().unwrap_or(0.0)
}
