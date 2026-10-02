use crate::node::{HBox, Ink, Leader, LinerStyle, LinkDest, NNode, Node};
use crate::color::Color;
use crate::framespec::{Flow, FrameDirection, FrameGeometry};
use crate::image::Image;
use crate::pagebuilder::{Page, Underlay};
use crate::svg_image::SvgFigure;
use crate::transform::{Matrix, Transform};

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
    /// A named place at `(x, y)`, the top of its line.
    fn destination(&mut self, _name: &str, _x: f64, _y: f64) {}
    /// `image` with its top-left corner at `(x, y)`.
    fn image(&mut self, _image: &Image, _x: f64, _y: f64, _width: f64, _height: f64) {}
    /// `figure` with its top-left corner at `(x, y)`, `baseline` being where
    /// its box stands.
    fn svg(&mut self, _figure: &SvgFigure, _x: f64, _y: f64, _baseline: f64, _width: f64, _height: f64) {}
    /// Draw what follows through `matrix` until `pop_transform`.
    fn push_transform(&mut self, _matrix: Matrix) {}
    fn pop_transform(&mut self) {}
    /// A link over `[left, top, right, bottom]`.
    fn link(&mut self, _rect: [f64; 4], _dest: &LinkDest) {}
}

/// Walk every page and draw its frames' content onto `canvas`.
pub fn draw_pages(pages: &[Page], canvas: &mut impl Canvas) {
    for page in pages {
        canvas.begin_page(page.paper.width, page.paper.height);
        for underlay in &page.underlay {
            draw_decoration(underlay, canvas);
        }
        draw_page(page, canvas);
        for overlay in &page.overlay {
            draw_decoration(overlay, canvas);
        }
        for frame in &page.outlines {
            canvas.frame_outline(frame);
        }
        canvas.end_page();
    }
}

fn draw_decoration(decoration: &Underlay, canvas: &mut impl Canvas) {
    match decoration {
        Underlay::Rules(color, rules) => {
            if let Some(color) = color {
                canvas.push_color(*color);
            }
            for [x, y, width, height] in rules {
                canvas.rule(*x, *y, *width, *height);
            }
            if color.is_some() {
                canvas.pop_color();
            }
        }
        Underlay::Image(image, [x, y, width, height]) => canvas.image(image, *x, *y, *width, *height),
        Underlay::Box(hbox, [x, y]) => {
            let mut c = Cursor { x: *x, y: *y, dir: FrameDirection::LTR };
            let line = Line { ratio: 1.0, end_edge: f64::INFINITY, height: pt(&hbox.height.length) };
            draw_hlist(&hbox.nodes, &mut c, &line, canvas);
        }
    }
}

fn draw_page(page: &Page, canvas: &mut dyn Canvas) {
    for (frame_id, nodes) in &page.content {
        let Some(frame) = page.frame(frame_id) else { continue };
        let mut c = Cursor::start(frame);
        for node in nodes {
            match node {
                Node::VBox(vbox) => {
                    let line = Line { ratio: vbox.ratio, end_edge: frame.right, height: pt(&vbox.height.length) };
                    c.advance_page(pt(&vbox.height.length));
                    draw_hlist(&vbox.nodes, &mut c, &line, canvas);
                    c.advance_page(pt(&vbox.depth.length));
                    c.new_line(frame);
                }
                Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) => {
                    c.advance_page(pt(&g.height.length) + g.adjustment.to_pt().unwrap_or(0.0));
                }
                Node::VKern(k) => c.advance_page(pt(&k.height.length)),
                _ => {}
            }
        }
    }
}

/// The pen in a frame, moved along the frame's writing and page directions
/// as SILE's frames move it.
#[derive(Debug, Clone, Copy)]
struct Cursor {
    x: f64,
    y: f64,
    dir: FrameDirection,
}

impl Cursor {
    fn start(frame: &FrameGeometry) -> Self {
        let dir = frame.direction.unwrap_or(FrameDirection::LTR);
        let mut c = Cursor { x: frame.left, y: frame.top, dir };
        c.new_line(frame);
        match dir.page {
            Flow::TTB => c.y = frame.top,
            Flow::LTR => c.x = frame.left,
            Flow::RTL => c.x = frame.right,
            Flow::BTT => c.y = frame.bottom,
        }
        c
    }

    fn new_line(&mut self, frame: &FrameGeometry) {
        match self.dir.writing {
            Flow::LTR => self.x = frame.left,
            Flow::RTL => self.x = frame.right,
            Flow::TTB => self.y = frame.top,
            Flow::BTT => self.y = frame.bottom,
        }
    }

    fn advance(&mut self, flow: Flow, amount: f64) {
        match flow {
            Flow::LTR => self.x += amount,
            Flow::RTL => self.x -= amount,
            Flow::TTB => self.y += amount,
            Flow::BTT => self.y -= amount,
        }
    }

    fn advance_writing(&mut self, amount: f64) {
        self.advance(self.dir.writing, amount);
    }

    fn advance_page(&mut self, amount: f64) {
        self.advance(self.dir.page, amount);
    }

    /// Boxes are drawn from their start edge, which in right-to-left lines
    /// is only reached by moving past them first.
    fn backwards(&self) -> bool {
        self.dir.writing == Flow::RTL
    }
}

struct Line {
    ratio: f64,
    /// Where leaders line up.
    end_edge: f64,
    height: f64,
}

impl Line {
    const NATURAL: Line = Line { ratio: 0.0, end_edge: f64::INFINITY, height: 0.0 };

    /// SILE's `rationWidth`.
    fn width(&self, node: &Node) -> f64 {
        self.width_of(&node.width())
    }

    fn width_of(&self, width: &crate::length::Length) -> f64 {
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

/// Draw a line's nodes from the cursor, scaling glue by the line's ratio.
fn draw_hlist(nodes: &[Node], c: &mut Cursor, line: &Line, canvas: &mut dyn Canvas) {
    for node in nodes {
        match node {
            Node::NNode(nnode) => {
                let width = pt(&nnode.width.length);
                if c.backwards() {
                    c.advance_writing(width);
                }
                let sideways = matches!(c.dir.writing, Flow::TTB | Flow::BTT) && !nnode.vertical;
                if sideways {
                    canvas.push_transform([0.0, 1.0, -1.0, 0.0, c.x + c.y, c.y - c.x]);
                }
                canvas.glyphs(nnode, c.x, c.y);
                if sideways {
                    canvas.pop_transform();
                }
                if !c.backwards() {
                    c.advance_writing(width);
                }
            }
            Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => {
                let width = line.width(node);
                match &g.leader {
                    Some(Leader::Stroke(s)) => {
                        let ox = c.x;
                        c.advance_page(-s.raise);
                        c.advance_writing(width);
                        canvas.rule(ox, c.y, c.x - ox, s.thickness);
                        c.advance_page(s.raise);
                    }
                    Some(Leader::Box(b)) => {
                        let from = if c.backwards() { c.x - width } else { c.x };
                        draw_leaders(b, from, width, c, line, canvas);
                        c.advance_writing(width);
                    }
                    None => c.advance_writing(width),
                }
            }
            Node::Kern(_) => c.advance_writing(line.width(node)),
            Node::Discretionary(d) => draw_hlist(&d.replacement, c, line, canvas),
            Node::HBox(hbox) => match &hbox.ink {
                Some(Ink::Rule) => {
                    let (height, depth) = (pt(&hbox.height.length), pt(&hbox.depth.length));
                    c.advance_page(-height);
                    let (ox, oy) = (c.x, c.y);
                    c.advance_writing(line.width(node));
                    c.advance_page(height + depth);
                    canvas.rule(ox, oy, c.x - ox, c.y - oy);
                    c.advance_page(-depth);
                }
                Some(Ink::Liner(LinerStyle::Stroke(s))) => {
                    let (ox, oy) = (c.x, c.y);
                    draw_hlist(&hbox.nodes, c, line, canvas);
                    canvas.rule(ox, oy - s.raise, c.x - ox, s.thickness);
                }
                Some(Ink::Liner(LinerStyle::Link(dest))) => {
                    let (ox, oy) = (c.x, c.y);
                    draw_hlist(&hbox.nodes, c, line, canvas);
                    let rect = [ox.min(c.x), oy - pt(&hbox.height.length), ox.max(c.x), oy + pt(&hbox.depth.length)];
                    canvas.link(rect, dest);
                }
                Some(Ink::Image(image)) => {
                    let (width, height) = (line.width(node), pt(&hbox.height.length));
                    if c.backwards() {
                        c.advance_writing(width);
                    }
                    canvas.image(image, c.x, c.y - height, width, height);
                    if !c.backwards() {
                        c.advance_writing(width);
                    }
                }
                Some(Ink::Svg(figure)) => {
                    let width = line.width(node);
                    let height = if figure.drop { 0.0 } else { pt(&hbox.height.length) };
                    if c.backwards() {
                        c.advance_writing(width);
                    }
                    canvas.svg(figure, c.x, c.y - height, c.y, width, height);
                    if !c.backwards() {
                        c.advance_writing(width);
                    }
                }
                Some(Ink::Transform(transform, shift)) => {
                    let width = line.width(node);
                    let mut inner = *c;
                    inner.advance_writing(-shift);
                    canvas.push_transform(transform.matrix(inner.x, inner.y));
                    if let Transform::Scale { x, .. } = transform
                        && *x < 0.0
                    {
                        inner.advance_writing(width / x);
                    }
                    draw_hlist(&hbox.nodes, &mut inner, line, canvas);
                    canvas.pop_transform();
                    c.advance_writing(width);
                }
                Some(Ink::Phantom) => c.advance_writing(line.width(node)),
                Some(Ink::Destination(name)) => canvas.destination(name, c.x, c.y - line.height),
                Some(Ink::Liner(LinerStyle::Custom(painter))) => {
                    (painter.0)(&mut Pen { cursor: c, canvas, line, hbox });
                }
                Some(Ink::LatinInTate(zw)) => {
                    c.advance_writing(-0.5 * zw);
                    c.advance_page(0.25 * zw);
                    let y = c.y;
                    let saved = *c;
                    draw_hlist(&hbox.nodes, c, line, canvas);
                    *c = saved;
                    c.y = y;
                    c.advance_writing(pt(&hbox.width.length) + 0.5 * zw);
                    c.advance_page(-0.25 * zw);
                }
                Some(Ink::Ruby(raise)) => {
                    let saved = *c;
                    c.advance_writing(pt(&hbox.width.length));
                    c.advance_page(-raise);
                    draw_hlist(&hbox.nodes, c, line, canvas);
                    *c = saved;
                }
                _ => {
                    let width = line.width(node);
                    if c.backwards() {
                        c.advance_writing(width);
                    }
                    let saved = *c;
                    draw_hlist(&hbox.nodes, c, line, canvas);
                    *c = saved;
                    if !c.backwards() {
                        c.advance_writing(width);
                    }
                    c.advance_page(-hbox.raise);
                }
            },
            _ => {}
        }
    }
}

/// As many copies of `pattern` as fit between `x` and `x + width`, placed
/// so that copies on different lines line up from the frame's end edge
/// (SILE's `leader:outputYourself`).
fn draw_leaders(pattern: &HBox, x: f64, width: f64, c: &Cursor, line: &Line, canvas: &mut dyn Canvas) {
    let step = pt(&pattern.width.length);
    if step <= 0.0 || !line.end_edge.is_finite() {
        return;
    }
    let fit = line.end_edge - x;
    let max = (fit / step).floor();
    let skip = ((line.end_edge - x - width) * 1e6).floor() / 1e6;
    let repetitions = max - (skip / step).ceil();
    let mut copy = Cursor { x: x + fit - max * step, ..*c };
    for _ in 0..repetitions.max(0.0) as usize {
        let start = copy;
        draw_hlist(&pattern.nodes, &mut copy, &Line::NATURAL, canvas);
        copy = Cursor { x: start.x + step, ..start };
    }
}

/// What a custom liner draws with: the pen where its box starts on the
/// line, and the canvas.
pub struct Pen<'a> {
    cursor: &'a mut Cursor,
    canvas: &'a mut dyn Canvas,
    line: &'a Line,
    hbox: &'a HBox,
}

impl Pen<'_> {
    pub fn x(&self) -> f64 {
        self.cursor.x
    }

    pub fn y(&self) -> f64 {
        self.cursor.y
    }

    /// The box's width at the line's glue ratio.
    pub fn width(&self) -> f64 {
        self.line.width_of(&self.hbox.width)
    }

    pub fn height(&self) -> f64 {
        pt(&self.hbox.height.length)
    }

    pub fn depth(&self) -> f64 {
        pt(&self.hbox.depth.length)
    }

    pub fn canvas(&mut self) -> &mut dyn Canvas {
        self.canvas
    }

    pub fn advance_writing(&mut self, amount: f64) {
        self.cursor.advance_writing(amount);
    }

    /// Draw the wrapped content from the pen, moving it past the content.
    pub fn draw_content(&mut self) {
        draw_hlist(&self.hbox.nodes, self.cursor, self.line, self.canvas);
    }
}

fn pt(m: &crate::measurement::Measurement) -> f64 {
    m.to_pt().unwrap_or(0.0)
}
