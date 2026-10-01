use crate::length::Length;
use crate::measurement::Measurement;
use crate::node::Node;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const AWFUL_BAD: i64 = 1_073_741_823;
const INF_BAD: i64 = 10_000;
const EJECT_PENALTY: i32 = -10_000;
const NONE: usize = usize::MAX;

// ---------------------------------------------------------------------------
// FitnessClass
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
enum FitnessClass {
    Tight = 0,
    Decent = 1,
    Loose = 2,
    VeryLoose = 3,
}

const ALL_CLASSES: [FitnessClass; 4] = [
    FitnessClass::Tight,
    FitnessClass::Decent,
    FitnessClass::Loose,
    FitnessClass::VeryLoose,
];

// ---------------------------------------------------------------------------
// BreakType
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakType {
    Hyphenated,
    Unhyphenated,
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LinebreakSettings {
    pub pretolerance: Option<i64>,
    pub tolerance: i64,
    pub line_penalty: i64,
    pub hyphen_penalty: i64,
    pub double_hyphen_demerits: i64,
    pub final_hyphen_demerits: i64,
    pub adj_demerits: i64,
    pub looseness: i32,
    pub emergency_stretch: f64,
    pub hang_indent: f64,
    pub hang_after: i32,
    pub left_skip: Length,
    pub right_skip: Length,
    pub prev_graf: i32,
}

impl Default for LinebreakSettings {
    fn default() -> Self {
        Self {
            pretolerance: Some(100),
            tolerance: 500,
            line_penalty: 10,
            hyphen_penalty: 50,
            double_hyphen_demerits: 10_000,
            final_hyphen_demerits: 5_000,
            adj_demerits: 10_000,
            looseness: 0,
            emergency_stretch: 0.0,
            hang_indent: 0.0,
            hang_after: 0,
            left_skip: Length::zero(),
            right_skip: Length::zero(),
            prev_graf: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// BreakResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BreakResult {
    pub position: usize,
    pub width: f64,
    pub left: f64,
    pub right: f64,
    pub ratio: f64,
}

// ---------------------------------------------------------------------------
// Arena nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ActiveNode {
    next: usize,
    cur_break: usize,
    prev_break: usize, // NONE if no previous
    serial: u32,
    #[allow(dead_code)]
    ratio: f64,
    line_number: i32,
    #[allow(dead_code)]
    fitness: FitnessClass,
    total_demerits: i64,
    break_type: BreakType,
}

#[derive(Debug, Clone)]
struct DeltaNode {
    next: usize,
    width: Length,
}

#[derive(Debug, Clone)]
enum ArenaNode {
    Active(ActiveNode),
    Delta(DeltaNode),
}

// ---------------------------------------------------------------------------
// BestInClass
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct BestInClass {
    minimal_demerits: i64,
    node: usize,
    line: i32,
    ratio: f64,
}

impl Default for BestInClass {
    fn default() -> Self {
        Self {
            minimal_demerits: AWFUL_BAD,
            node: NONE,
            line: 0,
            ratio: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Pass
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    First,
    Second,
    Emergency,
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

fn rate_badness(shortfall: f64, spring: f64) -> i64 {
    if spring == 0.0 {
        return INF_BAD;
    }
    let bad = (100.0 * (shortfall / spring).abs().powi(3)).floor() as i64;
    bad.min(INF_BAD)
}

fn fit_class(shortfall: f64, stretch: f64, shrink: f64) -> (i64, FitnessClass) {
    if shortfall > 0.0 {
        let badness = if shortfall > 110.0 && stretch < 25.0 {
            INF_BAD
        } else {
            rate_badness(shortfall, stretch)
        };
        let class = if badness > 99 {
            FitnessClass::VeryLoose
        } else if badness > 12 {
            FitnessClass::Loose
        } else {
            FitnessClass::Decent
        };
        (badness, class)
    } else {
        let sf = -shortfall;
        let badness = if sf > shrink {
            INF_BAD + 1
        } else {
            rate_badness(sf, shrink)
        };
        let class = if badness > 12 {
            FitnessClass::Tight
        } else {
            FitnessClass::Decent
        };
        (badness, class)
    }
}

// ---------------------------------------------------------------------------
// LineBreaker
// ---------------------------------------------------------------------------

struct LineBreaker<'a> {
    nodes: Vec<Node>,
    hsize: f64,
    settings: &'a LinebreakSettings,

    arena: Vec<ArenaNode>,
    head: usize,

    active_width: Length,
    cur_active_width: Length,
    break_width: Length,
    background: Length,

    first_width: f64,
    second_width: f64,
    last_special_line: i32,
    easy_line: Option<i32>,

    best_in_class: [BestInClass; 4],
    minimum_demerits: i64,
    threshold: i64,
    pass: Pass,
    final_pass: bool,
    serial: u32,

    place: usize,
    no_break_yet: bool,
    prev_prev_r: usize,
    prev_r: usize,
    r: usize,
    old_l: i32,
    line_width: f64,
    badness: i64,
    fit_class_val: FitnessClass,
    artificial_demerits: bool,
    last_ratio: f64,

    best_bet: usize,
}

impl<'a> LineBreaker<'a> {
    fn new(nodes: &[Node], hsize: f64, settings: &'a LinebreakSettings) -> Self {
        Self {
            nodes: nodes.to_vec(),
            hsize,
            settings,
            arena: Vec::with_capacity(64),
            head: NONE,
            active_width: Length::zero(),
            cur_active_width: Length::zero(),
            break_width: Length::zero(),
            background: Length::zero(),
            first_width: hsize,
            second_width: hsize,
            last_special_line: 0,
            easy_line: Some(0),
            best_in_class: [BestInClass::default(); 4],
            minimum_demerits: AWFUL_BAD,
            threshold: 0,
            pass: Pass::First,
            final_pass: false,
            serial: 0,
            place: 0,
            no_break_yet: true,
            prev_prev_r: NONE,
            prev_r: NONE,
            r: NONE,
            old_l: 0,
            line_width: hsize,
            badness: 0,
            fit_class_val: FitnessClass::Decent,
            artificial_demerits: false,
            last_ratio: 0.0,
            best_bet: NONE,
        }
    }

    // -- Arena helpers --------------------------------------------------------

    fn alloc(&mut self, node: ArenaNode) -> usize {
        let idx = self.arena.len();
        self.arena.push(node);
        idx
    }

    fn next_of(&self, idx: usize) -> usize {
        match &self.arena[idx] {
            ArenaNode::Active(a) => a.next,
            ArenaNode::Delta(d) => d.next,
        }
    }

    fn set_next(&mut self, idx: usize, next: usize) {
        match &mut self.arena[idx] {
            ArenaNode::Active(a) => a.next = next,
            ArenaNode::Delta(d) => d.next = next,
        }
    }

    fn is_delta(&self, idx: usize) -> bool {
        matches!(&self.arena[idx], ArenaNode::Delta(_))
    }

    fn delta_width(&self, idx: usize) -> Length {
        match &self.arena[idx] {
            ArenaNode::Delta(d) => d.width,
            _ => panic!("not a delta node"),
        }
    }

    fn active(&self, idx: usize) -> &ActiveNode {
        match &self.arena[idx] {
            ArenaNode::Active(a) => a,
            _ => panic!("not an active node"),
        }
    }

    // -- Init ----------------------------------------------------------------

    fn init(&mut self) {
        self.trim_glue();
        self.active_width = Length::zero();
        self.cur_active_width = Length::zero();
        self.break_width = Length::zero();

        let rskip = self.settings.right_skip.absolute();
        let lskip = self.settings.left_skip.absolute();
        self.background = rskip + lskip;

        self.best_in_class = [BestInClass::default(); 4];
        self.minimum_demerits = AWFUL_BAD;

        self.setup_line_lengths();
    }

    fn trim_glue(&mut self) {
        if let Some(last) = self.nodes.last()
            && last.is_glue() {
                self.nodes.pop();
            }
        // A paragraph must end in parfillskip + eject; callers that hand us a
        // bare node list get one, so the final line is always breakable.
        let terminated = matches!(self.nodes.last(), Some(Node::Penalty(p)) if p.penalty <= EJECT_PENALTY);
        if !terminated {
            self.nodes.push(Node::hfillglue(Length::zero()));
            self.nodes.push(Node::penalty(EJECT_PENALTY));
        }
        self.nodes.push(Node::penalty(INF_BAD as i32));
    }

    fn setup_line_lengths(&mut self) {
        let hang_after = self.settings.hang_after;
        let hang_indent = self.settings.hang_indent;

        if hang_indent == 0.0 {
            self.last_special_line = 0;
            self.second_width = self.hsize;
        } else {
            self.last_special_line = hang_after.unsigned_abs() as i32;
            if hang_after < 0 {
                self.second_width = self.hsize;
                self.first_width = self.hsize - hang_indent.abs();
            } else {
                self.first_width = self.hsize;
                self.second_width = self.hsize - hang_indent.abs();
            }
        }

        if self.settings.looseness == 0 {
            self.easy_line = Some(self.last_special_line);
        } else {
            self.easy_line = Some(AWFUL_BAD as i32);
        }
    }

    fn setup_active_list(&mut self) {
        self.arena.clear();
        self.serial = 1;

        // Index 0: head sentinel
        self.head = self.alloc(ArenaNode::Active(ActiveNode {
            next: NONE, // will be set below
            cur_break: 0,
            prev_break: NONE,
            serial: 0,
            ratio: 0.0,
            line_number: AWFUL_BAD as i32,
            fitness: FitnessClass::Decent,
            total_demerits: 0,
            break_type: BreakType::Hyphenated,
        }));

        // Index 1: initial "END" node
        let end = self.alloc(ArenaNode::Active(ActiveNode {
            next: self.head, // circular: points back to head
            cur_break: 0,
            prev_break: NONE,
            serial: 0,
            ratio: 0.0,
            line_number: self.settings.prev_graf + 1,
            fitness: FitnessClass::Decent,
            total_demerits: 0,
            break_type: BreakType::Unhyphenated,
        }));

        // head.next = end
        self.set_next(self.head, end);
    }

    fn line_width_for(&self, line_number: i32) -> f64 {
        if let Some(easy) = self.easy_line
            && line_number > easy {
                return self.second_width;
            }
        if line_number > self.last_special_line {
            self.second_width
        } else {
            self.first_width
        }
    }

    // -- Core algorithm -------------------------------------------------------

    fn check_for_legal_break(&mut self, place: usize) {
        let previous_is_box = place > 0 && self.nodes[place - 1].is_box();

        let is_box = self.nodes[place].is_box();
        let is_glue = self.nodes[place].is_glue();
        let is_kern = self.nodes[place].is_kern();
        let is_disc = self.nodes[place].is_discretionary();
        let is_penalty = self.nodes[place].is_penalty();

        if is_box {
            let w = self.nodes[place].line_contribution();
            self.active_width += w;
        } else if is_glue {
            if previous_is_box {
                self.try_break();
            }
            let w = self.nodes[place].width();
            self.active_width += w;
        } else if is_kern {
            let w = self.nodes[place].width();
            self.active_width += w;
        } else if is_disc {
            let (pre_w, repl_w) = if let Node::Discretionary(d) = &self.nodes[place] {
                (d.prebreak_width(), d.replacement_width())
            } else {
                unreachable!()
            };
            self.active_width += pre_w;
            self.try_break();
            self.active_width -= pre_w;
            self.active_width += repl_w;
        } else if is_penalty {
            self.try_break();
        }
    }

    fn try_break(&mut self) {
        let pi: i32;
        let break_type: BreakType;

        if self.place >= self.nodes.len() {
            pi = EJECT_PENALTY;
            break_type = BreakType::Hyphenated;
        } else {
            let node = &self.nodes[self.place];
            if node.is_discretionary() {
                break_type = BreakType::Hyphenated;
                pi = self.settings.hyphen_penalty as i32;
            } else {
                break_type = BreakType::Unhyphenated;
                pi = if let Node::Penalty(p) = node {
                    p.penalty
                } else {
                    0
                };
            }
        };

        self.no_break_yet = true;
        self.prev_prev_r = NONE;
        self.prev_r = self.head;
        self.old_l = 0;
        self.cur_active_width = self.active_width;

        loop {
            // Inner loop: advance through delta nodes
            loop {
                self.r = self.next_of(self.prev_r);

                if self.is_delta(self.r) {
                    self.cur_active_width += self.delta_width(self.r);
                    self.prev_prev_r = self.prev_r;
                    self.prev_r = self.r;
                    continue;
                }
                break;
            }

            // r is now an active node (or head sentinel)
            let r_line_number = self.active(self.r).line_number;

            if r_line_number > self.old_l {
                if self.minimum_demerits < AWFUL_BAD
                    && (Some(self.old_l) != self.easy_line || self.r == self.head)
                {
                    self.create_new_active_nodes(break_type);
                }
                if self.r == self.head {
                    return;
                }

                // Update line width for this line number
                if let Some(easy) = self.easy_line {
                    if r_line_number > easy {
                        self.line_width = self.second_width;
                        self.old_l = AWFUL_BAD as i32 - 1;
                    } else {
                        self.old_l = r_line_number;
                        self.line_width = self.line_width_for(r_line_number);
                    }
                } else {
                    self.old_l = r_line_number;
                    self.line_width = self.line_width_for(r_line_number);
                }
            }

            self.consider_demerits(pi, break_type);
        }
    }

    fn consider_demerits(&mut self, pi: i32, break_type: BreakType) {
        self.artificial_demerits = false;

        let shortfall = self.line_width - self.cur_active_width.length.to_pt_abs();
        let stretch = self.cur_active_width.stretch.to_pt_abs();
        let shrink = self.cur_active_width.shrink.to_pt_abs();

        let (badness, fc) = fit_class(shortfall, stretch, shrink);
        self.badness = badness;
        self.fit_class_val = fc;

        if badness > INF_BAD || pi == EJECT_PENALTY {
            if self.final_pass
                && self.minimum_demerits == AWFUL_BAD
                && self.next_of(self.r) == self.head
                && self.prev_r == self.head
            {
                self.artificial_demerits = true;
            } else if badness > self.threshold {
                self.deactivate_r();
                return;
            }
        } else {
            self.prev_r = self.r;
            if badness > self.threshold {
                return;
            }
        }

        // Compute ratio for this break
        let shortfall_val = shortfall;
        let factor = if shortfall_val > 0.0 { stretch } else { shrink };
        self.last_ratio = if factor != 0.0 {
            shortfall_val / factor
        } else {
            AWFUL_BAD as f64
        };

        self.record_feasible(pi, break_type);

        // If badness was > inf_bad or eject, we already set artificial_demerits
        // and didn't set prev_r = r, so deactivate
        if badness > INF_BAD || pi == EJECT_PENALTY {
            if !self.artificial_demerits || badness > self.threshold {
                // already handled above in the first branch
            }
            self.deactivate_r();
        }
    }

    fn compute_demerits(&self, pi: i32, break_type: BreakType) -> i64 {
        if self.artificial_demerits {
            return 0;
        }

        let mut demerit = self.settings.line_penalty + self.badness;
        demerit = if demerit.abs() >= 10_000 {
            100_000_000
        } else {
            demerit * demerit
        };

        let pi64 = pi as i64;
        if pi > 0 {
            demerit += pi64 * pi64;
        } else if pi > EJECT_PENALTY {
            demerit -= pi64 * pi64;
        }

        if break_type == BreakType::Hyphenated
            && self.active(self.r).break_type == BreakType::Hyphenated
        {
            if self.place < self.nodes.len() {
                demerit += self.settings.double_hyphen_demerits;
            } else {
                demerit += self.settings.final_hyphen_demerits;
            }
        }

        demerit
    }

    fn record_feasible(&mut self, pi: i32, break_type: BreakType) {
        let demerit = self.compute_demerits(pi, break_type);
        let total = demerit + self.active(self.r).total_demerits;
        let fc = self.fit_class_val as usize;

        if total <= self.best_in_class[fc].minimal_demerits {
            let r_serial = self.active(self.r).serial;
            self.best_in_class[fc] = BestInClass {
                minimal_demerits: total,
                node: if r_serial > 0 { self.r } else { NONE },
                line: self.active(self.r).line_number,
                ratio: self.last_ratio,
            };
            if total < self.minimum_demerits {
                self.minimum_demerits = total;
            }
        }
    }

    fn create_new_active_nodes(&mut self, break_type: BreakType) {
        if self.no_break_yet {
            self.no_break_yet = false;
            self.break_width = self.background;

            if self.place < self.nodes.len() {
                let node = &self.nodes[self.place];
                if node.is_discretionary()
                    && let Node::Discretionary(d) = node {
                        self.break_width += d.prebreak_width();
                        self.break_width += d.postbreak_width();
                        self.break_width -= d.replacement_width();
                    }
            }

            // Skip non-box nodes after break point
            let mut p = self.place;
            while p < self.nodes.len() && !self.nodes[p].is_box() {
                if let Some(w) = self.node_width_opt(p) {
                    self.break_width -= w;
                }
                p += 1;
            }
        }

        // Add delta node before new active nodes
        if self.prev_r != self.head && self.is_delta(self.prev_r) {
            if let ArenaNode::Delta(d) = &mut self.arena[self.prev_r] {
                d.width -= self.cur_active_width;
                d.width += self.break_width;
            }
        } else if self.prev_r == self.head {
            self.active_width = self.break_width;
        } else {
            let new_delta = self.alloc(ArenaNode::Delta(DeltaNode {
                next: self.r,
                width: self.break_width - self.cur_active_width,
            }));
            self.set_next(self.prev_r, new_delta);
            self.prev_prev_r = self.prev_r;
            self.prev_r = new_delta;
        }

        // Adjust minimum demerits by adjdemerits
        let adj = self.settings.adj_demerits;
        if adj.abs() >= AWFUL_BAD - self.minimum_demerits {
            self.minimum_demerits = AWFUL_BAD - 1;
        } else {
            self.minimum_demerits += adj.abs();
        }

        // Create new active nodes for each fitness class
        for &class in &ALL_CLASSES {
            let ci = class as usize;
            let best = self.best_in_class[ci];
            if best.minimal_demerits <= self.minimum_demerits {
                self.serial += 1;
                let new_active = self.alloc(ArenaNode::Active(ActiveNode {
                    next: self.r,
                    cur_break: self.place,
                    prev_break: best.node,
                    serial: self.serial,
                    ratio: best.ratio,
                    line_number: best.line + 1,
                    fitness: class,
                    total_demerits: best.minimal_demerits,
                    break_type,
                }));
                self.set_next(self.prev_r, new_active);
                self.prev_r = new_active;
            }
            self.best_in_class[ci] = BestInClass::default();
        }

        self.minimum_demerits = AWFUL_BAD;

        // Add trailing delta if r is not head
        if self.r != self.head {
            let trailing_delta = self.alloc(ArenaNode::Delta(DeltaNode {
                next: self.r,
                width: self.cur_active_width - self.break_width,
            }));
            self.set_next(self.prev_r, trailing_delta);
            self.prev_prev_r = self.prev_r;
            self.prev_r = trailing_delta;
        }
    }

    fn deactivate_r(&mut self) {
        self.set_next(self.prev_r, self.next_of(self.r));

        if self.prev_r == self.head {
            let next = self.next_of(self.head);
            if next != self.head && self.is_delta(next) {
                self.active_width += self.delta_width(next);
                self.cur_active_width = self.active_width;
                let skip = self.next_of(next);
                self.set_next(self.head, skip);
            }
        } else if self.is_delta(self.prev_r) {
            let next = self.next_of(self.prev_r);
            if next == self.head {
                let dw = self.delta_width(self.prev_r);
                self.cur_active_width -= dw;
                if self.prev_prev_r != NONE {
                    self.set_next(self.prev_prev_r, self.head);
                }
                self.prev_r = self.prev_prev_r;
            } else if self.is_delta(next) {
                let next_dw = self.delta_width(next);
                self.cur_active_width += next_dw;
                // Merge deltas: prev_r.width += next.width
                let merged = self.delta_width(self.prev_r) + next_dw;
                if let ArenaNode::Delta(d) = &mut self.arena[self.prev_r] { d.width = merged }
                let skip = self.next_of(next);
                self.set_next(self.prev_r, skip);
            }
        }
    }

    /// Run one pass over the paragraph; true when it found breaks.
    fn try_pass(&mut self) -> bool {
        if self.threshold > INF_BAD {
            self.threshold = INF_BAD;
        }
        self.setup_active_list();
        self.active_width = self.background;
        let mut place = 0;
        while place < self.nodes.len() && self.next_of(self.head) != self.head {
            self.place = place;
            self.check_for_legal_break(place);
            place += 1;
        }
        place >= self.nodes.len() && self.try_final_break()
    }

    fn try_final_break(&mut self) -> bool {
        // TeX §899 calls try_break here; SILE deliberately does not, because
        // the paragraph already ends with an eject penalty. Calling it adds a
        // spurious break past the end of the node list.
        self.place = self.nodes.len();

        if self.next_of(self.head) == self.head {
            return false;
        }

        let mut r = self.next_of(self.head);
        let mut fewest_demerits = AWFUL_BAD;
        loop {
            if !self.is_delta(r) {
                let td = self.active(r).total_demerits;
                if td < fewest_demerits {
                    fewest_demerits = td;
                    self.best_bet = r;
                }
            }
            r = self.next_of(r);
            if r == self.head {
                break;
            }
        }

        if self.settings.looseness == 0 {
            return true;
        }

        // looseness != 0: not fully implemented (matches Lua XXX)
        true
    }

    fn post_line_break(&self) -> Vec<BreakResult> {
        if self.best_bet == NONE {
            return vec![];
        }

        // Count lines
        let mut nb_lines: i32 = 0;
        let mut p = self.best_bet;
        loop {
            nb_lines += 1;
            let pb = self.active(p).prev_break;
            if pb == NONE {
                break;
            }
            p = pb;
        }

        // Collect breaks in reverse, then reverse
        let mut breaks = Vec::with_capacity(nb_lines as usize);
        let mut p = self.best_bet;
        let mut line = 1;

        loop {
            let (left, right) = self.compute_indent(line, nb_lines);
            breaks.push(BreakResult {
                position: self.active(p).cur_break,
                width: self.hsize,
                left,
                right,
                ratio: self.active(p).ratio,
            });

            let pb = self.active(p).prev_break;
            if pb == NONE {
                break;
            }
            p = pb;
            line += 1;
        }

        breaks.reverse();
        breaks
    }

    fn compute_indent(&self, line: i32, nb_lines: i32) -> (f64, f64) {
        let hang_after = self.settings.hang_after;
        let hang_indent = self.settings.hang_indent;

        if hang_after == 0 || hang_indent == 0.0 {
            return (0.0, 0.0);
        }

        let indent = if hang_after > 0 {
            if line > nb_lines - hang_after {
                0.0
            } else {
                hang_indent
            }
        } else if line > nb_lines + hang_after {
            hang_indent
        } else {
            0.0
        };

        if indent > 0.0 {
            (indent, 0.0)
        } else if indent < 0.0 {
            (0.0, -indent)
        } else {
            (0.0, 0.0)
        }
    }

    fn node_width_opt(&self, idx: usize) -> Option<Length> {
        let node = &self.nodes[idx];
        if node.is_glue() || node.is_kern() || node.is_penalty() {
            Some(node.width())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn do_break(nodes: &[Node], hsize: f64, settings: &LinebreakSettings) -> Vec<BreakResult> {
    let mut lb = LineBreaker::new(nodes, hsize, settings);
    lb.init();

    match settings.pretolerance {
        Some(pt) if pt >= 0 => {
            lb.threshold = pt;
            lb.pass = Pass::First;
            lb.final_pass = false;
        }
        _ => {
            lb.threshold = settings.tolerance;
            lb.pass = Pass::Second;
            lb.final_pass = settings.emergency_stretch <= 0.0;
        }
    }

    while !lb.try_pass() {
        match lb.pass {
            Pass::First => {
                lb.pass = Pass::Second;
                lb.threshold = settings.tolerance;
            }
            Pass::Second => {
                lb.pass = Pass::Emergency;
                lb.background.stretch += Measurement::pt(settings.emergency_stretch);
                lb.final_pass = true;
            }
            Pass::Emergency => break,
        }
    }

    lb.post_line_break()
}

/// Break a paragraph the way SILE does: a first pass without hyphenation,
/// then, if no breaks fit `pretolerance`, `hyphenate` the nodes and break
/// them again. Returns the nodes the breaks refer to.
pub fn break_paragraph(
    nodes: Vec<Node>,
    hsize: f64,
    settings: &LinebreakSettings,
    hyphenate: impl FnOnce(Vec<Node>) -> Vec<Node>,
) -> (Vec<Node>, Vec<BreakResult>) {
    if let Some(pt) = settings.pretolerance.filter(|&pt| pt >= 0) {
        let mut lb = LineBreaker::new(&nodes, hsize, settings);
        lb.init();
        lb.threshold = pt;
        lb.pass = Pass::First;
        lb.final_pass = false;
        if lb.try_pass() {
            let breaks = lb.post_line_break();
            return (nodes, breaks);
        }
    }
    let nodes = hyphenate(nodes);
    let settings = LinebreakSettings { pretolerance: None, ..settings.clone() };
    let breaks = do_break(&nodes, hsize, &settings);
    (nodes, breaks)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::length::Length;
    use crate::measurement::Measurement;
    use crate::node::Node;

    // -- rate_badness ---------------------------------------------------------

    #[test]
    fn badness_zero_spring() {
        assert_eq!(rate_badness(10.0, 0.0), INF_BAD);
    }

    #[test]
    fn badness_perfect_fit() {
        assert_eq!(rate_badness(0.0, 1.0), 0);
    }

    #[test]
    fn badness_ratio_one() {
        // 100 * (1.0)^3 = 100
        assert_eq!(rate_badness(10.0, 10.0), 100);
    }

    #[test]
    fn badness_capped_at_inf_bad() {
        assert_eq!(rate_badness(100.0, 1.0), INF_BAD);
    }

    #[test]
    fn badness_small_shortfall() {
        // 100 * (1/10)^3 = 0.1 → floor = 0
        assert_eq!(rate_badness(1.0, 10.0), 0);
    }

    // -- fit_class ------------------------------------------------------------

    #[test]
    fn fit_class_very_loose() {
        let (b, c) = fit_class(200.0, 5.0, 5.0);
        assert_eq!(c, FitnessClass::VeryLoose);
        assert!(b > 99);
    }

    #[test]
    fn fit_class_loose() {
        // 100 * (6/10)^3 = 21.6 → floor = 21 > 12
        let (b, c) = fit_class(6.0, 10.0, 10.0);
        assert_eq!(c, FitnessClass::Loose);
        assert!(b > 12 && b <= 99);
    }

    #[test]
    fn fit_class_decent_positive() {
        let (b, c) = fit_class(1.0, 10.0, 10.0);
        assert_eq!(c, FitnessClass::Decent);
        assert!(b <= 12);
    }

    #[test]
    fn fit_class_tight() {
        // 100 * (6/10)^3 = 21.6 → floor = 21 > 12
        let (b, c) = fit_class(-6.0, 10.0, 10.0);
        assert_eq!(c, FitnessClass::Tight);
        assert!(b > 12);
    }

    #[test]
    fn fit_class_decent_negative() {
        let (b, c) = fit_class(-1.0, 10.0, 10.0);
        assert_eq!(c, FitnessClass::Decent);
        assert!(b <= 12);
    }

    // -- do_break: basic tests ------------------------------------------------

    fn nnode(text: &str, width: f64, height: f64, depth: f64) -> Node {
        Node::nnode(
            text,
            vec![],
            Some(Length::pt(width)),
            Some(Length::pt(height)),
            Some(Length::pt(depth)),
        )
    }

    fn glue(length: f64, stretch: f64, shrink: f64) -> Node {
        Node::glue(Length::new(
            Measurement::pt(length),
            Measurement::pt(stretch),
            Measurement::pt(shrink),
        ))
    }

    #[test]
    fn break_empty_input() {
        let result = do_break(&[], 100.0, &LinebreakSettings::default());
        // Empty after trimming: just the added penalty
        assert!(result.len() <= 1);
    }

    #[test]
    fn break_single_word() {
        let nodes = vec![nnode("Hello", 30.0, 7.0, 0.0)];
        let result = do_break(&nodes, 100.0, &LinebreakSettings::default());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].width, 100.0);
    }

    #[test]
    fn break_two_lines() {
        // Two words that together exceed the line width
        let nodes = vec![
            nnode("Hello", 60.0, 7.0, 0.0),
            glue(5.0, 2.0, 1.0),
            nnode("World", 60.0, 7.0, 0.0),
        ];
        let result = do_break(&nodes, 80.0, &LinebreakSettings::default());
        assert_eq!(result.len(), 2, "should break into 2 lines");
    }

    #[test]
    fn break_forced_penalty() {
        let nodes = vec![
            nnode("A", 20.0, 7.0, 0.0),
            Node::penalty(EJECT_PENALTY),
            nnode("B", 20.0, 7.0, 0.0),
        ];
        let result = do_break(&nodes, 200.0, &LinebreakSettings::default());
        assert!(result.len() >= 2, "eject penalty should force a break");
    }

    #[test]
    fn break_no_break_at_high_penalty() {
        // Everything fits on one line, penalty shouldn't create extra break
        let nodes = vec![
            nnode("A", 20.0, 7.0, 0.0),
            Node::penalty(INF_BAD as i32),
            nnode("B", 20.0, 7.0, 0.0),
        ];
        let result = do_break(&nodes, 200.0, &LinebreakSettings::default());
        assert_eq!(result.len(), 1, "inf_bad penalty should prevent break");
    }

    #[test]
    fn break_emergency_stretch() {
        // Very tight: each word is 95pt, line is 100pt, glue has no stretch
        let nodes = vec![
            nnode("Word1", 95.0, 7.0, 0.0),
            glue(5.0, 0.0, 0.0),
            nnode("Word2", 95.0, 7.0, 0.0),
        ];
        let mut settings = LinebreakSettings::default();
        settings.emergency_stretch = 100.0;
        let result = do_break(&nodes, 100.0, &settings);
        assert!(
            !result.is_empty(),
            "emergency stretch should allow breaking"
        );
    }

    // -- Sherlock Holmes paragraph (from spec/break_spec.lua) -----------------

    #[test]
    fn break_sherlock_paragraph() {
        let nodes = sherlock_nodes();
        // Standard TeX \hsize = 4in at 72.27pt/in
        let hsize = 289.07625;
        let settings = LinebreakSettings::default();
        let result = do_break(&nodes, hsize, &settings);

        assert!(
            !result.is_empty(),
            "should produce at least one line break"
        );

        for br in &result {
            assert!(
                br.position <= nodes.len() + 1,
                "break position {} out of bounds (len={})",
                br.position,
                nodes.len()
            );
        }

        // 31 words totalling ~632pt + ~68pt glue = ~700pt at 289pt wide ≈ 3 lines
        assert!(
            result.len() >= 2 && result.len() <= 5,
            "expected 2-5 lines, got {}",
            result.len()
        );
    }

    #[test]
    fn break_sherlock_narrow() {
        let nodes = sherlock_nodes();
        // Narrow: 150pt, should produce more lines (~700pt / 150pt ≈ 5)
        let settings = LinebreakSettings::default();
        let result = do_break(&nodes, 150.0, &settings);
        assert!(
            result.len() >= 4,
            "narrow hsize should produce at least 4 lines, got {}",
            result.len()
        );
    }

    fn sherlock_nodes() -> Vec<Node> {
        vec![
            nnode("To", 10.14648, 6.15234, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("Sherlock", 35.82031, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("Holmes", 30.79102, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("she", 13.99902, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("is", 6.57227, 6.6211, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("always", 27.59766, 7.56836, 2.44139),
            glue(2.20215, 1.10107, 0.73404),
            nnode("the", 13.5791, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("woman", 32.37305, 4.6875, 0.1953),
            glue(2.93619, 3.30322, 0.24467),  // sentence-ending
            nnode("I", 2.97852, 6.15234, 0.0),
            glue(2.20215, 1.09996, 0.73477),
            nnode("have", 19.26758, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("seldom", 29.45313, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("heard", 23.78906, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("him", 16.25977, 7.56836, 0.0),
            glue(2.20215, 1.10107, 0.73404),
            nnode("mention", 34.86816, 6.6211, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("her", 14.09668, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("under", 24.59473, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("any", 15.03906, 4.6875, 2.44139),
            glue(2.20215, 1.10107, 0.73404),
            nnode("other", 22.56836, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("name", 25.04883, 4.6875, 0.1953),
            glue(2.93619, 3.30322, 0.24467),  // sentence-ending
            nnode("In", 8.4961, 6.15234, 0.0),
            glue(2.20215, 1.10107, 0.73404),
            nnode("his", 12.08984, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("eyes", 17.83691, 4.6875, 2.44139),
            glue(2.20215, 1.10107, 0.73404),
            nnode("she", 13.99902, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("eclipses", 31.9043, 7.56836, 2.34373),
            glue(2.20215, 1.10107, 0.73404),
            nnode("and", 15.30762, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("predominates", 56.7334, 7.56836, 2.34373),
            glue(2.20215, 1.10107, 0.73404),
            nnode("the", 13.5791, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("whole", 24.93652, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("of", 8.13965, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("her", 14.09668, 7.56836, 0.14647),
            glue(2.20215, 1.10107, 0.73404),
            nnode("sex.", 15.6543, 4.6875, 0.1953),
        ]
    }

    // -- Hang indent ----------------------------------------------------------

    #[test]
    fn break_with_hang_indent() {
        let mut nodes = Vec::new();
        for i in 0..20 {
            if i > 0 {
                nodes.push(glue(5.0, 3.0, 1.0));
            }
            nodes.push(nnode(&format!("word{i}"), 30.0, 7.0, 0.0));
        }
        let mut settings = LinebreakSettings::default();
        settings.hang_after = 2;
        settings.hang_indent = 40.0;
        let result = do_break(&nodes, 200.0, &settings);
        assert!(result.len() >= 3, "should produce at least 3 lines");
    }

    // -- Discretionary break --------------------------------------------------

    #[test]
    fn break_at_discretionary() {
        let hyphen = Node::nnode(
            "-",
            vec![],
            Some(Length::pt(5.0)),
            Some(Length::pt(7.0)),
            Some(Length::pt(0.0)),
        );
        let nodes = vec![
            nnode("ex", 40.0, 7.0, 0.0),
            Node::discretionary(vec![hyphen], vec![], vec![]),
            nnode("tra", 40.0, 7.0, 0.0),
            glue(5.0, 2.0, 1.0),
            nnode("word", 40.0, 7.0, 0.0),
        ];
        let result = do_break(&nodes, 60.0, &LinebreakSettings::default());
        assert!(
            result.len() >= 2,
            "should break at discretionary, got {} lines",
            result.len()
        );
    }
}
