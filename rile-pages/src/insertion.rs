//! Material that leaves the main flow for another frame on the same page,
//! such as footnotes, taking its room from the frames it steals from.
// SILE: `insertions` package.

use std::collections::BTreeMap;

use rile::length::Length;
use rile::node::{Insertion, Node};
use rile::pagebuilder::{self, SUPER_EJECT};

/// How one kind of insertion is placed.
#[derive(Debug, Clone)]
pub struct InsertionClass {
    /// Frame the material is set into.
    pub insert_into: String,
    /// Frames that shrink to make room, each by this share of it.
    pub steal_from: Vec<(String, f64)>,
    pub max_height: f64,
    /// Vertical material set above the first insertion on a page.
    pub top_box: Vec<Node>,
    /// Space between insertions, baseline to top.
    pub inter_skip: f64,
    /// Page break penalty after the line an insertion comes from.
    pub penalty: i32,
}

impl InsertionClass {
    pub fn new(insert_into: impl Into<String>, steal_from: impl Into<String>, max_height: f64) -> Self {
        Self {
            insert_into: insert_into.into(),
            steal_from: vec![(steal_from.into(), 1.0)],
            max_height,
            top_box: vec![Node::vglue(Length::zero())],
            inter_skip: 0.0,
            penalty: -3000,
        }
    }
}

/// Stacked vertical material: the height runs to the last box's baseline,
/// whose depth becomes the depth.
// SILE: `vbox:append`.
#[derive(Debug, Clone, Default)]
pub struct Stack {
    pub nodes: Vec<Node>,
    pub height: f64,
    pub depth: f64,
}

impl Stack {
    pub fn of(nodes: Vec<Node>) -> Self {
        let mut stack = Self::default();
        stack.append(nodes);
        stack
    }

    pub fn append(&mut self, nodes: Vec<Node>) {
        self.height += self.depth;
        let mut last_depth = 0.0;
        for node in nodes {
            self.height += pt(node.height()) + pt(node.depth());
            if node.is_vbox() {
                last_depth = pt(node.depth());
            }
            self.nodes.push(node);
        }
        self.height -= last_depth;
        self.depth = last_depth;
    }
}

/// The insertions placed on the page being built.
#[derive(Debug, Default)]
pub struct PageInsertions {
    /// Material per class, in the order classes first appeared.
    pub boxes: Vec<(String, Stack)>,
    /// Room promised to insertions but not yet taken from each frame.
    pub shrinkage: BTreeMap<String, f64>,
}

impl PageInsertions {
    fn stack(&mut self, class: &str) -> &mut Stack {
        let index = match self.boxes.iter().position(|(c, _)| c == class) {
            Some(i) => i,
            None => {
                self.boxes.push((class.to_string(), Stack::default()));
                self.boxes.len() - 1
            }
        };
        &mut self.boxes[index].1
    }

    fn shrink(&mut self, class: &InsertionClass, amount: f64) {
        for (frame, ratio) in &class.steal_from {
            *self.shrinkage.entry(frame.clone()).or_default() += amount * ratio;
        }
    }

    pub fn shrinkage(&self, frame: &str) -> f64 {
        self.shrinkage.get(frame).copied().unwrap_or(0.0)
    }

    /// The page builder met the insertion at `index` of `queue` while
    /// filling `frame`. If it fits it is placed and the target shrinks; if
    /// only part fits, that part is placed and a break forced before the
    /// rest; otherwise a break is forced before the line it came from.
    // SILE: `processInsertion`.
    pub fn process(
        &mut self,
        classes: &BTreeMap<String, InsertionClass>,
        frame: &str,
        queue: &mut Vec<Node>,
        index: usize,
        total_height: f64,
        target: f64,
    ) -> f64 {
        let Node::Insertion(ins) = &mut queue[index] else { return target };
        if ins.seen {
            return target;
        }
        let Some(class) = classes.get(&ins.class) else { return target };
        while ins.nodes.len() > 1 && ins.nodes.last().is_some_and(Node::is_discardable) {
            ins.nodes.pop();
        }
        let stack = self.stack(&ins.class);
        let top = Stack::of(match stack.nodes.last() {
            None => class.top_box.clone(),
            Some(last) => vec![Node::vglue(Length::pt(class.inter_skip - pt(last.depth())))],
        });
        let top_height = top.height;
        let height = ins.content_height + top_height + top.depth + ins.content_depth;
        let effect = class.steal_from.iter().find(|(f, _)| f == frame).map_or(0.0, |(_, r)| height * r);

        if total_height + effect <= target && stack.height + height <= class.max_height {
            stack.append(top.nodes);
            stack.append(unbox(ins));
            ins.seen = true;
            self.shrink(class, height);
            return target - effect;
        }

        let max_size = (target - total_height).min(class.max_height) - top_height;
        let mut material = unbox(ins);
        if let Some(br) = pagebuilder::find_break(&mut material, max_size, true, &mut pagebuilder::no_insertions) {
            let first = Stack::of(pagebuilder::split_page(&mut material, &br));
            let rest = Stack::of(material);
            ins.nodes = rest.nodes;
            ins.content_height = rest.height;
            ins.content_depth = rest.depth;
            let stack = self.stack(&class_name(queue, index));
            stack.append(top.nodes);
            let amount = top_height + first.height + first.depth;
            stack.append(first.nodes);
            self.shrink(class, amount);
            queue.insert(index, Node::penalty(SUPER_EJECT));
            return target;
        }

        let line = (0..=index).rev().find(|&j| queue[j].is_vbox()).unwrap_or(0);
        for _ in line..=index {
            queue.insert(line, Node::penalty(SUPER_EJECT));
        }
        target
    }
}

fn class_name(queue: &[Node], index: usize) -> String {
    match &queue[index] {
        Node::Insertion(ins) => ins.class.clone(),
        _ => String::new(),
    }
}

/// The content when it is vertical material.
// SILE: `vbox:unbox`.
fn unbox(ins: &Insertion) -> Vec<Node> {
    if ins.nodes.iter().any(|n| n.is_vbox() || n.is_vglue()) {
        ins.nodes.clone()
    } else {
        vec![Node::Insertion(ins.clone())]
    }
}

fn pt(l: Length) -> f64 {
    l.length.to_pt().unwrap_or(0.0)
}
