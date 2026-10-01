//! SILE node types.
//!
//! This module defines the typed node graph used throughout the layout pipeline.
//! It corresponds to `types/node.lua` in the Lua implementation.
//!
//! The node hierarchy (Lua → Rust):
//! - `box` (abstract)     → common fields on each struct
//! - `hbox`               → [`HBox`]
//! - `zerohbox`           → [`Node::ZeroHBox`] (an HBox with zero dims)
//! - `nnode`              → [`NNode`]
//! - `unshaped`           → [`Unshaped`]
//! - `discretionary`      → [`Discretionary`]
//! - `alternative`        → [`Alternative`]
//! - `glue`               → [`Glue`] (discardable)
//! - `kern`               → [`Kern`] (non-discardable glue)
//! - `hfillglue`          → [`Node::HFillGlue`]
//! - `hssglue`            → [`Node::HssGlue`]
//! - `vglue`              → [`VGlue`] (discardable)
//! - `vkern`              → [`VKern`] (non-discardable vglue)
//! - `vfillglue`          → [`Node::VFillGlue`]
//! - `vssglue`            → [`Node::VssGlue`]
//! - `zerovglue`          → [`Node::ZeroVGlue`]
//! - `penalty`            → [`Penalty`]
//! - `vbox`               → [`VBox`]
//! - `migrating`          → [`Migrating`]

use crate::color::Color;
use crate::length::Length;
use crate::measurement::Measurement;

/// Infinity value used for fill glues (matches the Lua `1e13`).
pub const INFINITY: f64 = 1e13;

// ─── GlyphData ────────────────────────────────────────────────────────────────

/// Positioned glyph data from the shaping pipeline, used by PDF/rendering output.
#[derive(Debug, Clone, Default)]
pub struct GlyphData {
    pub gid: u16,
    /// How far the glyph moves the pen; the font's advance unless tracking
    /// changed it.
    pub width: f64,
    pub x_advance: f64,
    pub y_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
    /// The source text of the cluster this glyph starts (empty for the
    /// remaining glyphs of a multi-glyph cluster).
    pub text: String,
}

// ─── Helper functions (mirrors _maxnode / SU.sum) ────────────────────────────

/// Returns the maximum value of a dimension across all nodes.
/// Returns `Length::zero()` if the slice is empty.
pub fn max_node_dim(nodes: &[Node], dim: Dim) -> Length {
    nodes
        .iter()
        .map(|n| match dim {
            Dim::Width => n.width(),
            Dim::Height => n.height(),
            Dim::Depth => n.depth(),
        })
        .fold(Length::zero(), |acc, l| {
            // Compare absolute pt values; keep the larger.
            let acc_pt = acc.to_pt().unwrap_or(0.0);
            let l_pt = l.to_pt().unwrap_or(0.0);
            if l_pt > acc_pt { l } else { acc }
        })
}

/// Returns the sum of widths across all nodes.
pub fn sum_widths(nodes: &[Node]) -> Length {
    nodes.iter().map(|n| n.width()).fold(Length::zero(), |a, b| a + b)
}

/// Dimension selector for [`max_node_dim`].
pub enum Dim {
    Width,
    Height,
    Depth,
}

// ─── Node enum ───────────────────────────────────────────────────────────────

/// A node in the SILE layout graph.
#[derive(Debug, Clone)]
pub enum Node {
    HBox(HBox),
    /// An hbox with zero width, height, and depth.
    ZeroHBox(HBox),
    NNode(NNode),
    Unshaped(Unshaped),
    Discretionary(Discretionary),
    Alternative(Alternative),
    Glue(Glue),
    Kern(Kern),
    HFillGlue(Glue),
    HssGlue(Glue),
    VGlue(VGlue),
    VKern(VKern),
    VFillGlue(VGlue),
    VssGlue(VGlue),
    ZeroVGlue(VGlue),
    Penalty(Penalty),
    VBox(VBox),
    Migrating(Migrating),
    Insertion(Insertion),
}

impl Node {
    /// The type name, matching the Lua `node.type` field.
    pub fn node_type(&self) -> &'static str {
        match self {
            Node::HBox(_) => "hbox",
            Node::ZeroHBox(_) => "zerohbox",
            Node::NNode(_) => "nnode",
            Node::Unshaped(_) => "unshaped",
            Node::Discretionary(_) => "discretionary",
            Node::Alternative(_) => "alternative",
            Node::Glue(_) => "glue",
            Node::Kern(_) => "kern",
            Node::HFillGlue(_) => "hfillglue",
            Node::HssGlue(_) => "hssglue",
            Node::VGlue(_) => "vglue",
            Node::VKern(_) => "vkern",
            Node::VFillGlue(_) => "vfillglue",
            Node::VssGlue(_) => "vssglue",
            Node::ZeroVGlue(_) => "zerovglue",
            Node::Penalty(_) => "penalty",
            Node::VBox(_) => "vbox",
            Node::Migrating(_) => "migrating",
            Node::Insertion(_) => "insertion",
        }
    }

    pub fn width(&self) -> Length {
        match self {
            Node::HBox(n) | Node::ZeroHBox(n) => n.width,
            Node::NNode(n) => n.width,
            Node::Unshaped(_) => Length::zero(),
            Node::Discretionary(n) => n.width,
            Node::Alternative(n) => n.width,
            Node::Glue(n) | Node::HFillGlue(n) | Node::HssGlue(n) => n.width,
            Node::Kern(n) => n.width,
            Node::VGlue(n) | Node::VFillGlue(n) | Node::VssGlue(n) | Node::ZeroVGlue(n) => {
                n.width
            }
            Node::VKern(n) => n.width,
            Node::Penalty(n) => n.width,
            Node::VBox(n) => n.width,
            Node::Migrating(n) => n.width,
            Node::Insertion(_) => Length::zero(),
        }
    }

    pub fn height(&self) -> Length {
        match self {
            Node::HBox(n) | Node::ZeroHBox(n) => n.height,
            Node::NNode(n) => n.height,
            Node::Unshaped(_) => Length::zero(),
            Node::Discretionary(n) => n.height,
            Node::Alternative(n) => n.height,
            Node::Glue(n) | Node::HFillGlue(n) | Node::HssGlue(n) => n.height,
            Node::Kern(n) => n.height,
            Node::VGlue(n) | Node::VFillGlue(n) | Node::VssGlue(n) | Node::ZeroVGlue(n) => {
                n.height
            }
            Node::VKern(n) => n.height,
            Node::Penalty(n) => n.height,
            Node::VBox(n) => n.height,
            Node::Migrating(n) => n.height,
            Node::Insertion(_) => Length::zero(),
        }
    }

    pub fn depth(&self) -> Length {
        match self {
            Node::HBox(n) | Node::ZeroHBox(n) => n.depth,
            Node::NNode(n) => n.depth,
            Node::Unshaped(_) => Length::zero(),
            Node::Discretionary(n) => n.depth,
            Node::Alternative(n) => n.depth,
            Node::Glue(n) | Node::HFillGlue(n) | Node::HssGlue(n) => n.depth,
            Node::Kern(n) => n.depth,
            Node::VGlue(n) | Node::VFillGlue(n) | Node::VssGlue(n) | Node::ZeroVGlue(n) => {
                n.depth
            }
            Node::VKern(n) => n.depth,
            Node::Penalty(n) => n.depth,
            Node::VBox(n) => n.depth,
            Node::Migrating(n) => n.depth,
            Node::Insertion(_) => Length::zero(),
        }
    }

    /// Equivalent to `box.misfit`: returns `height` instead of `width` for misfits.
    pub fn line_contribution(&self) -> Length {
        if self.is_misfit() { self.height() } else { self.width() }
    }

    pub fn is_misfit(&self) -> bool {
        match self {
            Node::HBox(n) | Node::ZeroHBox(n) => n.misfit,
            Node::NNode(n) => n.misfit,
            _ => false,
        }
    }

    /// Glue and penalties vanish at breaks, except glue the user asked for
    /// explicitly (SILE's `pushExplicitGlue`).
    pub fn is_discardable(&self) -> bool {
        match self {
            Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => !g.explicit,
            Node::VGlue(g) | Node::VFillGlue(g) | Node::VssGlue(g) | Node::ZeroVGlue(g) => {
                !g.explicit
            }
            Node::Penalty(_) | Node::Insertion(_) => true,
            _ => false,
        }
    }

    pub fn is_explicit(&self) -> bool {
        match self {
            Node::Glue(n) | Node::HFillGlue(n) | Node::HssGlue(n) => n.explicit,
            Node::VGlue(n) | Node::VFillGlue(n) | Node::VssGlue(n) | Node::ZeroVGlue(n) => {
                n.explicit
            }
            _ => false,
        }
    }

    // Type-tag helpers (mirrors `is_hbox`, `is_box`, etc. in Lua)

    pub fn is_hbox(&self) -> bool {
        matches!(
            self,
            Node::HBox(_) | Node::ZeroHBox(_) | Node::NNode(_) | Node::Migrating(_)
        )
    }

    pub fn is_box(&self) -> bool {
        self.is_hbox() || matches!(self, Node::VBox(_))
    }

    pub fn is_nnode(&self) -> bool {
        matches!(self, Node::NNode(_))
    }

    pub fn is_glue(&self) -> bool {
        matches!(self, Node::Glue(_) | Node::HFillGlue(_) | Node::HssGlue(_))
    }

    pub fn is_kern(&self) -> bool {
        matches!(self, Node::Kern(_))
    }

    pub fn is_vglue(&self) -> bool {
        matches!(
            self,
            Node::VGlue(_) | Node::VFillGlue(_) | Node::VssGlue(_) | Node::ZeroVGlue(_)
        )
    }

    pub fn is_vkern(&self) -> bool {
        matches!(self, Node::VKern(_))
    }

    pub fn is_penalty(&self) -> bool {
        matches!(self, Node::Penalty(_))
    }

    pub fn is_vbox(&self) -> bool {
        matches!(self, Node::VBox(_))
    }

    pub fn is_unshaped(&self) -> bool {
        matches!(self, Node::Unshaped(_))
    }

    pub fn is_discretionary(&self) -> bool {
        matches!(self, Node::Discretionary(_))
    }

    pub fn is_alternative(&self) -> bool {
        matches!(self, Node::Alternative(_))
    }

    pub fn is_migrating(&self) -> bool {
        matches!(self, Node::Migrating(_))
    }

    pub fn is_insertion(&self) -> bool {
        matches!(self, Node::Insertion(_))
    }

    pub fn is_zerohbox(&self) -> bool {
        matches!(self, Node::ZeroHBox(_))
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Node::ZeroHBox(_) | Node::ZeroVGlue(_))
    }

    /// Text debug representation, matching `box:toText()` in Lua.
    pub fn to_text(&self) -> String {
        match self {
            Node::HBox(_) | Node::ZeroHBox(_) => "hbox".to_string(),
            Node::NNode(n) => n.text.clone(),
            Node::Unshaped(n) => n.text.clone(),
            Node::Glue(_)
            | Node::HFillGlue(_)
            | Node::HssGlue(_)
            | Node::Kern(_) => " ".to_string(),
            Node::Penalty(_) => "(!)".to_string(),
            Node::Discretionary(n) => {
                if n.used { "-".to_string() } else { "_".to_string() }
            }
            Node::VBox(n) => {
                let inner: String = n.nodes.iter().map(|nd| nd.to_text()).collect();
                format!("VB[{inner}]")
            }
            Node::VGlue(_)
            | Node::VFillGlue(_)
            | Node::VssGlue(_)
            | Node::ZeroVGlue(_)
            | Node::VKern(_) => " ".to_string(),
            Node::Alternative(_) => "alternative".to_string(),
            Node::Migrating(_) => "migrating".to_string(),
            Node::Insertion(_) => "insertion".to_string(),
        }
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Node::HBox(n) | Node::ZeroHBox(n) => write!(f, "{n}"),
            Node::NNode(n) => write!(f, "{n}"),
            Node::Unshaped(n) => write!(f, "{n}"),
            Node::Discretionary(n) => write!(f, "{n}"),
            Node::Alternative(n) => write!(f, "{n}"),
            Node::Glue(n) => {
                if n.explicit {
                    write!(f, "E:G<{}>", n.width)
                } else {
                    write!(f, "G<{}>", n.width)
                }
            }
            Node::HFillGlue(n) | Node::HssGlue(n) => {
                if n.explicit {
                    write!(f, "E:G<{}>", n.width)
                } else {
                    write!(f, "G<{}>", n.width)
                }
            }
            Node::Kern(n) => write!(f, "K<{}>", n.width),
            Node::VGlue(n) | Node::VFillGlue(n) | Node::VssGlue(n) | Node::ZeroVGlue(n) => {
                if n.explicit {
                    write!(f, "E:VG<{}>", n.height)
                } else {
                    write!(f, "VG<{}>", n.height)
                }
            }
            Node::VKern(n) => write!(f, "VK<{}>", n.height),
            Node::Penalty(n) => write!(f, "{n}"),
            Node::VBox(n) => write!(f, "{n}"),
            Node::Migrating(n) => write!(f, "<M: {:?}>", n.material),
            Node::Insertion(n) => write!(f, "I<{}>", n.class),
        }
    }
}

// ─── HBox ────────────────────────────────────────────────────────────────────

/// A horizontal box node.
#[derive(Debug, Clone, Default)]
pub struct HBox {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub misfit: bool,
    pub explicit: bool,
    /// Horizontal content drawn inside the box (empty for plain spacers).
    pub nodes: Vec<Node>,
    /// Moves the baseline of everything after the box up by this much
    /// (SILE's `\raise`).
    pub raise: f64,
    pub ink: Option<Ink>,
}

/// A horizontal line whose top edge is `raise` above the baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub raise: f64,
    pub thickness: f64,
}

/// What an hbox draws besides its content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ink {
    /// The whole box is solid ink (SILE's `\hrule`).
    Rule,
    /// Content drawn at the line's glue ratio with a stroke along it
    /// (SILE's liners: `\underline`, `\strikethrough`).
    Liner(Stroke),
    /// Zero-size markers around liner content in a paragraph. Each line
    /// wraps what falls between them in its own `Liner` box.
    LinerStart(Stroke),
    LinerEnd,
    /// A ruby reading: its content is drawn raised by this much, a box
    /// width on from where it stands, and the pen goes back to where it was
    /// (SILE's `ruby` package).
    Ruby(f64),
}

impl HBox {
    pub fn new(width: Length, height: Length, depth: Length) -> Self {
        Self { width, height, depth, ..Default::default() }
    }
}

impl std::fmt::Display for HBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "H<{}>^{}-{}v", self.width, self.height, self.depth)
    }
}

// ─── NNode ───────────────────────────────────────────────────────────────────

/// A shaped text node. Contains typed glyph sub-boxes.
#[derive(Debug, Clone)]
pub struct NNode {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub misfit: bool,
    pub explicit: bool,
    /// The text this node represents.
    pub text: String,
    /// The shaped sub-boxes (typically [`Node::HBox`] glyph boxes).
    pub nodes: Vec<Node>,
    pub language: String,
    /// Font registry key for PDF rendering (empty if not set).
    pub font_key: String,
    /// Font size in points for PDF rendering.
    pub font_size: f64,
    /// Positioned glyphs from the shaper, used for PDF output.
    pub glyphs: Vec<GlyphData>,
    /// Optional color override for this text node.
    pub color: Option<Color>,
    /// For a syllable produced by hyphenation, the whole word it came from.
    /// Unbroken words are drawn from the parent, keeping kerning across
    /// hyphenation points.
    pub parent: Option<std::sync::Arc<HyphenatedWord>>,
    /// Embedding level relative to the paragraph's, when bidi assigned one.
    pub bidi_level: Option<u8>,
}

/// A hyphenated word and the number of syllables it was split into.
#[derive(Debug)]
pub struct HyphenatedWord {
    pub word: NNode,
    pub syllables: usize,
}

impl NNode {
    /// Construct an NNode, computing width/height/depth from `nodes` when they are zero.
    pub fn new(
        text: impl Into<String>,
        nodes: Vec<Node>,
        width: Option<Length>,
        height: Option<Length>,
        depth: Option<Length>,
    ) -> Self {
        let computed_width = width
            .filter(|l| l.to_pt().map(|v| v != 0.0).unwrap_or(false))
            .unwrap_or_else(|| sum_widths(&nodes));
        let computed_height = height
            .filter(|l| l.to_pt().map(|v| v != 0.0).unwrap_or(false))
            .unwrap_or_else(|| max_node_dim(&nodes, Dim::Height));
        let computed_depth = depth
            .filter(|l| l.to_pt().map(|v| v != 0.0).unwrap_or(false))
            .unwrap_or_else(|| max_node_dim(&nodes, Dim::Depth));

        Self {
            width: computed_width,
            height: computed_height,
            depth: computed_depth,
            misfit: false,
            explicit: false,
            text: text.into(),
            nodes,
            language: String::new(),
            font_key: String::new(),
            font_size: 0.0,
            glyphs: Vec::new(),
            color: None,
            parent: None,
            bidi_level: None,
        }
    }

    /// Construct an NNode with glyph data for PDF rendering.
    pub fn with_glyphs(
        text: impl Into<String>,
        glyphs: Vec<GlyphData>,
        font_key: impl Into<String>,
        font_size: f64,
        width: f64,
        height: f64,
        depth: f64,
    ) -> Self {
        Self {
            width: Length::pt(width),
            height: Length::pt(height),
            depth: Length::pt(depth),
            misfit: false,
            explicit: false,
            text: text.into(),
            nodes: Vec::new(),
            language: String::new(),
            font_key: font_key.into(),
            font_size,
            glyphs,
            color: None,
            parent: None,
            bidi_level: None,
        }
    }
}

impl std::fmt::Display for NNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "N<{}>^{}-{}v({})",
            self.width, self.height, self.depth, self.text
        )
    }
}

// ─── Unshaped ────────────────────────────────────────────────────────────────

/// A text node that has not yet been shaped by the shaper.
///
/// Unshaped nodes have no valid width; shaping resolves them into [`NNode`]s.
#[derive(Debug, Clone)]
pub struct Unshaped {
    pub height: Length,
    pub depth: Length,
    pub text: String,
    pub language: String,
}

impl Unshaped {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            height: Length::zero(),
            depth: Length::zero(),
            text: text.into(),
            language: String::new(),
        }
    }
}

impl std::fmt::Display for Unshaped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "U({})", self.text)
    }
}

// ─── Discretionary ───────────────────────────────────────────────────────────

/// A potential line-break point (hyphenation opportunity).
///
/// - `prebreak`: output at the end of the line before the break (e.g. a hyphen).
/// - `postbreak`: output at the start of the following line (e.g. a repeated hyphen).
/// - `replacement`: output when no break occurs at this point.
#[derive(Debug, Clone, Default)]
pub struct Discretionary {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub prebreak: Vec<Node>,
    pub postbreak: Vec<Node>,
    pub replacement: Vec<Node>,
    pub used: bool,
    pub is_prebreak: bool,
}

impl Discretionary {
    pub fn new(
        prebreak: Vec<Node>,
        postbreak: Vec<Node>,
        replacement: Vec<Node>,
    ) -> Self {
        Self {
            prebreak,
            postbreak,
            replacement,
            ..Default::default()
        }
    }

    pub fn mark_as_prebreak(&mut self) {
        self.used = true;
        self.is_prebreak = true;
    }

    pub fn clone_as_postbreak(&self) -> Self {
        assert!(self.used, "Cannot clone a non-used discretionary as postbreak");
        Self {
            prebreak: self.prebreak.clone(),
            postbreak: self.postbreak.clone(),
            replacement: self.replacement.clone(),
            used: true,
            is_prebreak: false,
            ..Default::default()
        }
    }

    pub fn prebreak_width(&self) -> Length {
        self.prebreak.iter().map(|n| n.width()).fold(Length::zero(), |a, b| a + b)
    }

    pub fn postbreak_width(&self) -> Length {
        self.postbreak.iter().map(|n| n.width()).fold(Length::zero(), |a, b| a + b)
    }

    pub fn replacement_width(&self) -> Length {
        self.replacement.iter().map(|n| n.width()).fold(Length::zero(), |a, b| a + b)
    }

    pub fn prebreak_height(&self) -> Length {
        max_node_dim(&self.prebreak, Dim::Height)
    }

    pub fn postbreak_height(&self) -> Length {
        max_node_dim(&self.postbreak, Dim::Height)
    }

    pub fn replacement_height(&self) -> Length {
        max_node_dim(&self.replacement, Dim::Height)
    }

    pub fn replacement_depth(&self) -> Length {
        max_node_dim(&self.replacement, Dim::Depth)
    }
}

impl std::fmt::Display for Discretionary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pre: String = self.prebreak.iter().map(|n| n.to_string()).collect();
        let post: String = self.postbreak.iter().map(|n| n.to_string()).collect();
        let repl: String = self.replacement.iter().map(|n| n.to_string()).collect();
        write!(f, "D({pre}|{post}|{repl})")
    }
}

// ─── Alternative ─────────────────────────────────────────────────────────────

/// A node that selects one of several layout options.
///
/// Note: this is considered experimental / broken in the Lua source.
#[derive(Debug, Clone, Default)]
pub struct Alternative {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub options: Vec<Node>,
    pub selected: Option<usize>,
}

impl Alternative {
    pub fn min_width(&self) -> Length {
        self.options
            .iter()
            .map(|n| n.width())
            .min_by(|a, b| {
                a.to_pt()
                    .unwrap_or(0.0)
                    .partial_cmp(&b.to_pt().unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_default()
    }
}

impl std::fmt::Display for Alternative {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let opts: Vec<String> = self.options.iter().map(|n| n.to_string()).collect();
        write!(f, "A({})", opts.join(" / "))
    }
}

// ─── Glue ────────────────────────────────────────────────────────────────────

/// A horizontal flexible space (discardable at line breaks).
#[derive(Debug, Clone, Default)]
pub struct Glue {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub explicit: bool,
    /// Ink filling the glue's set width.
    pub leader: Option<Leader>,
}

#[derive(Debug, Clone)]
pub enum Leader {
    /// A rule (SILE's `\hrulefill`).
    Stroke(Stroke),
    /// Copies of a box, aligned to the frame's end edge (SILE's `\leaders`).
    Box(Box<HBox>),
}

impl Glue {
    pub fn new(width: Length) -> Self {
        Self { width, ..Default::default() }
    }
}

// ─── Kern ────────────────────────────────────────────────────────────────────

/// A non-discardable horizontal space (not a break opportunity).
#[derive(Debug, Clone, Default)]
pub struct Kern {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
}

impl Kern {
    pub fn new(width: Length) -> Self {
        Self { width, ..Default::default() }
    }
}

// ─── VGlue ───────────────────────────────────────────────────────────────────

/// A vertical flexible space (discardable at page breaks).
#[derive(Debug, Clone, Default)]
pub struct VGlue {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub explicit: bool,
    pub adjustment: Measurement,
}

impl VGlue {
    pub fn new(height: Length) -> Self {
        Self { height, ..Default::default() }
    }

    pub fn adjust(&mut self, adjustment: Measurement) {
        self.adjustment = adjustment;
    }
}

// ─── VKern ───────────────────────────────────────────────────────────────────

/// A non-discardable vertical space (not a page-break opportunity).
#[derive(Debug, Clone, Default)]
pub struct VKern {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
}

impl VKern {
    pub fn new(height: Length) -> Self {
        Self { height, ..Default::default() }
    }
}

// ─── Penalty ─────────────────────────────────────────────────────────────────

/// A break-point hint.
///
/// Values in `[-10_000, 10_000]`. Positive = undesirable break; negative =
/// desirable break. `10_000` forbids a break; `-10_000` forces one.
#[derive(Debug, Clone, Default)]
pub struct Penalty {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub penalty: i32,
}

impl Penalty {
    pub fn new(penalty: i32) -> Self {
        Self { penalty, ..Default::default() }
    }
}

impl std::fmt::Display for Penalty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "P({})", self.penalty)
    }
}

// ─── VBox ────────────────────────────────────────────────────────────────────

/// A vertical box: a stacked sequence of horizontal lines and vertical spacing.
#[derive(Debug, Clone, Default)]
pub struct VBox {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub nodes: Vec<Node>,
    pub ratio: f64,
    pub misfit: bool,
    pub explicit: bool,
}

impl VBox {
    /// Construct a VBox. Height and depth are computed from `nodes`.
    pub fn new(nodes: Vec<Node>, width: Length) -> Self {
        let height = max_node_dim(&nodes, Dim::Height);
        let depth = max_node_dim(&nodes, Dim::Depth);
        Self { width, height, depth, nodes, ratio: 0.0, misfit: false, explicit: false }
    }

    /// Append a node (or a VBox's contents) to this VBox, updating dimensions.
    ///
    /// Matches `vbox:append()` in Lua.
    pub fn append(&mut self, node: Node) {
        let nodes_to_add: Vec<Node> = match node {
            Node::VBox(vb) if vb.nodes.iter().any(|n| n.is_vbox() || n.is_vglue()) => vb.nodes,
            _ => vec![node],
        };

        self.height = self.height.absolute();
        // Add current depth to height
        let cur_depth = self.depth;
        self.height += cur_depth.absolute();

        let mut last_depth = Length::zero();
        for n in nodes_to_add {
            let h = n.height();
            let d = n.depth().absolute();
            let is_vb = n.is_vbox();
            if is_vb {
                last_depth = n.depth();
            }
            self.height = self.height + h + d;
            self.nodes.push(n);
        }
        self.height -= last_depth;
        self.depth = last_depth;
    }

    /// Returns the text debug representation.
    pub fn to_text(&self) -> String {
        let inner: String = self.nodes.iter().map(|n| n.to_text()).collect();
        format!("VB[{inner}]")
    }
}

impl std::fmt::Display for VBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VB<{}|{}v{})", self.height, self.to_text(), self.depth)
    }
}

// ─── Migrating ───────────────────────────────────────────────────────────────

/// A node that can move between frames (e.g. footnote material).
#[derive(Debug, Clone, Default)]
pub struct Migrating {
    pub width: Length,
    pub height: Length,
    pub depth: Length,
    pub material: Vec<Node>,
    pub nodes: Vec<Node>,
}

/// Material bound for another frame (a footnote, say), held in the vertical
/// list after the line it came from until the page builder places it.
#[derive(Debug, Clone, Default)]
pub struct Insertion {
    pub class: String,
    pub nodes: Vec<Node>,
    pub content_height: f64,
    pub content_depth: f64,
    /// Already accounted for on the page being built.
    pub seen: bool,
}

// ─── Constructor helpers (mirrors SILE.types.node.xxx({...})) ────────────────

impl Node {
    /// Build an `hbox` from explicit dimensions (in points).
    pub fn hbox(width: f64, height: f64, depth: f64) -> Self {
        Node::HBox(HBox::new(Length::pt(width), Length::pt(height), Length::pt(depth)))
    }

    /// Build an `hbox` from `Length` values.
    pub fn hbox_lengths(width: Length, height: Length, depth: Length) -> Self {
        Node::HBox(HBox::new(width, height, depth))
    }

    /// Build a `zerohbox`.
    pub fn zerohbox() -> Self {
        Node::ZeroHBox(HBox::default())
    }

    /// Build a `glue` with the given width `Length`.
    pub fn glue(width: Length) -> Self {
        Node::Glue(Glue::new(width))
    }

    /// Build a `kern` with the given width `Length`.
    pub fn kern(width: Length) -> Self {
        Node::Kern(Kern::new(width))
    }

    /// Build an `hfillglue` (infinite horizontal stretch).
    pub fn hfillglue(natural: Length) -> Self {
        let width = Length::new(
            natural.length,
            Measurement::pt(INFINITY),
            natural.shrink,
        );
        Node::HFillGlue(Glue { width, ..Default::default() })
    }

    /// Build an `hssglue` (infinite horizontal stretch and shrink).
    pub fn hssglue(natural: Length) -> Self {
        let width = Length::new(
            natural.length,
            Measurement::pt(INFINITY),
            Measurement::pt(INFINITY),
        );
        Node::HssGlue(Glue { width, ..Default::default() })
    }

    /// Build a `vglue` with the given height `Length`.
    pub fn vglue(height: Length) -> Self {
        Node::VGlue(VGlue::new(height))
    }

    /// Build a `vkern` with the given height `Length`.
    pub fn vkern(height: Length) -> Self {
        Node::VKern(VKern::new(height))
    }

    /// Build a `vfillglue` (infinite vertical stretch).
    pub fn vfillglue(natural: Length) -> Self {
        let height = Length::new(
            natural.length,
            Measurement::pt(INFINITY),
            natural.shrink,
        );
        Node::VFillGlue(VGlue { height, ..Default::default() })
    }

    /// Build a `vssglue` (infinite vertical stretch and shrink).
    pub fn vssglue(natural: Length) -> Self {
        let height = Length::new(
            natural.length,
            Measurement::pt(INFINITY),
            Measurement::pt(INFINITY),
        );
        Node::VssGlue(VGlue { height, ..Default::default() })
    }

    /// Build a `zerovglue`.
    pub fn zerovglue() -> Self {
        Node::ZeroVGlue(VGlue::default())
    }

    /// Build a `penalty`.
    pub fn penalty(value: i32) -> Self {
        Node::Penalty(Penalty::new(value))
    }

    /// Build an `nnode`.
    pub fn nnode(
        text: impl Into<String>,
        nodes: Vec<Node>,
        width: Option<Length>,
        height: Option<Length>,
        depth: Option<Length>,
    ) -> Self {
        Node::NNode(NNode::new(text, nodes, width, height, depth))
    }

    /// Build an `unshaped` node.
    pub fn unshaped(text: impl Into<String>) -> Self {
        Node::Unshaped(Unshaped::new(text))
    }

    /// Build a `discretionary`.
    pub fn discretionary(
        prebreak: Vec<Node>,
        postbreak: Vec<Node>,
        replacement: Vec<Node>,
    ) -> Self {
        Node::Discretionary(Discretionary::new(prebreak, postbreak, replacement))
    }

    /// Build a `vbox` from a list of nodes.
    pub fn vbox(nodes: Vec<Node>) -> Self {
        Node::VBox(VBox::new(nodes, Length::zero()))
    }

    /// Build an `nnode` with glyph data for PDF rendering.
    pub fn nnode_with_glyphs(
        text: impl Into<String>,
        glyphs: Vec<GlyphData>,
        font_key: impl Into<String>,
        font_size: f64,
        width: f64,
        height: f64,
        depth: f64,
    ) -> Self {
        Node::NNode(NNode::with_glyphs(text, glyphs, font_key, font_size, width, height, depth))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── hbox ──────────────────────────────────────────────────────────────────

    #[test]
    fn hbox_dimensions() {
        let h = Node::hbox(20.0, 30.0, 3.0);
        assert_eq!(h.width().to_pt_abs(), 20.0);
        assert_eq!(h.height().to_pt_abs(), 30.0);
        assert_eq!(h.depth().to_pt_abs(), 3.0);
    }

    #[test]
    fn hbox_type() {
        let h = Node::hbox(20.0, 30.0, 3.0);
        assert_eq!(h.node_type(), "hbox");
    }

    #[test]
    fn hbox_is_box_not_glue() {
        let h = Node::hbox(20.0, 30.0, 3.0);
        assert!(h.is_box());
        assert!(!h.is_glue());
    }

    #[test]
    fn hbox_not_discardable() {
        let h = Node::hbox(20.0, 30.0, 3.0);
        assert!(!h.is_discardable());
    }

    #[test]
    fn hbox_display() {
        let h = Node::hbox(20.0, 30.0, 3.0);
        assert_eq!(h.to_string(), "H<20pt>^30pt-3ptv");
    }

    // ── vbox ──────────────────────────────────────────────────────────────────

    #[test]
    fn vbox_height_from_nodes() {
        let h1 = Node::hbox(10.0, 5.0, 2.0);
        let h2 = Node::hbox(11.0, 6.0, 3.0);
        let vb = Node::vbox(vec![h1, h2]);
        // height = max(5, 6) = 6
        assert_eq!(vb.height().to_pt_abs(), 6.0);
        // depth = max(2, 3) = 3
        assert_eq!(vb.depth().to_pt_abs(), 3.0);
    }

    #[test]
    fn vbox_node_count() {
        let h1 = Node::hbox(10.0, 5.0, 2.0);
        let h2 = Node::hbox(11.0, 6.0, 3.0);
        let vb = match Node::vbox(vec![h1, h2]) {
            Node::VBox(v) => v,
            _ => panic!("expected VBox"),
        };
        assert_eq!(vb.nodes.len(), 2);
    }

    #[test]
    fn vbox_type() {
        let vb = Node::vbox(vec![]);
        assert_eq!(vb.node_type(), "vbox");
    }

    #[test]
    fn vbox_is_box_not_glue() {
        let vb = Node::vbox(vec![]);
        assert!(vb.is_box());
        assert!(!vb.is_glue());
    }

    #[test]
    fn vbox_not_discardable() {
        let vb = Node::vbox(vec![]);
        assert!(!vb.is_discardable());
    }

    #[test]
    fn vbox_display() {
        let h1 = Node::hbox(10.0, 5.0, 2.0);
        let h2 = Node::hbox(11.0, 6.0, 3.0);
        let vb = Node::vbox(vec![h1, h2]);
        assert_eq!(vb.to_string(), "VB<6pt|VB[hboxhbox]v3pt)");
    }

    // ── nnode ─────────────────────────────────────────────────────────────────

    #[test]
    fn nnode_dimensions_from_nodes() {
        let h1 = Node::hbox(10.0, 5.0, 3.0);
        let h2 = Node::hbox(20.0, 10.0, 5.0);
        let n = Node::nnode("test", vec![h1, h2], None, None, None);
        assert_eq!(n.width().to_pt_abs(), 30.0);
        assert_eq!(n.height().to_pt_abs(), 10.0);
        assert_eq!(n.depth().to_pt_abs(), 5.0);
    }

    #[test]
    fn nnode_type() {
        let n = Node::nnode("x", vec![], None, None, None);
        assert_eq!(n.node_type(), "nnode");
    }

    #[test]
    fn nnode_to_text() {
        let n = Node::nnode("test", vec![], None, None, None);
        assert_eq!(n.to_text(), "test");
    }

    #[test]
    fn nnode_display() {
        let h1 = Node::hbox(10.0, 5.0, 3.0);
        let h2 = Node::hbox(20.0, 10.0, 5.0);
        let n = Node::nnode("test", vec![h1, h2], None, None, None);
        assert_eq!(n.to_string(), "N<30pt>^10pt-5ptv(test)");
    }

    // ── discretionary ─────────────────────────────────────────────────────────

    #[test]
    fn discretionary_display() {
        let nnode1 = Node::nnode("pre", vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(3.0)));
        let nnode2 = Node::nnode("break", vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(3.0)));
        let nnode3 = Node::nnode("post", vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(3.0)));
        let disc = Node::discretionary(
            vec![nnode1, nnode2.clone()],
            vec![nnode3, nnode2],
            vec![],
        );
        assert_eq!(
            disc.to_string(),
            "D(N<20pt>^30pt-3ptv(pre)N<20pt>^30pt-3ptv(break)|N<20pt>^30pt-3ptv(post)N<20pt>^30pt-3ptv(break)|)"
        );
    }

    // ── glue ──────────────────────────────────────────────────────────────────

    #[test]
    fn glue_width() {
        let g = Node::glue(Length::new(
            Measurement::pt(3.0),
            Measurement::pt(2.0),
            Measurement::pt(2.0),
        ));
        assert_eq!(g.width().to_pt_abs(), 3.0);
    }

    #[test]
    fn glue_discardable() {
        let g = Node::glue(Length::pt(3.0));
        assert!(g.is_discardable());
    }

    #[test]
    fn glue_display() {
        let g = Node::glue(Length::new(
            Measurement::pt(3.0),
            Measurement::pt(2.0),
            Measurement::pt(2.0),
        ));
        assert_eq!(g.to_string(), "G<3pt plus 2pt minus 2pt>");
    }

    // ── vbox toText ───────────────────────────────────────────────────────────

    /// Mirrors the Lua `node_spec` "should go to text" test:
    /// `{ nnode1, glue, nnode2, glue, nnode3 }` (glue used twice → two spaces).
    #[test]
    fn vbox_to_text_with_glue() {
        let glue_width = Length::new(Measurement::pt(3.0), Measurement::pt(2.0), Measurement::pt(2.0));
        let n1 = Node::nnode("one",   vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(3.0)));
        let g1 = Node::glue(glue_width);
        let n2 = Node::nnode("two",   vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(7.0)));
        let g2 = Node::glue(glue_width);
        let n3 = Node::nnode("three", vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(2.0)));
        let vb = Node::vbox(vec![n1, g1, n2, g2, n3]);
        let text = match &vb {
            Node::VBox(v) => v.to_text(),
            _ => panic!(),
        };
        assert_eq!(text, "VB[one two three]");
    }

    #[test]
    fn vbox_depth_from_nodes_with_glue() {
        let glue_width = Length::new(Measurement::pt(3.0), Measurement::pt(2.0), Measurement::pt(2.0));
        let n1 = Node::nnode("one",   vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(3.0)));
        let g1 = Node::glue(glue_width);
        let n2 = Node::nnode("two",   vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(7.0)));
        let g2 = Node::glue(glue_width);
        let n3 = Node::nnode("three", vec![], Some(Length::pt(20.0)), Some(Length::pt(30.0)), Some(Length::pt(2.0)));
        let vb = Node::vbox(vec![n1, g1, n2, g2, n3]);
        // depth = max of all depths = 7
        assert_eq!(vb.depth().to_pt_abs(), 7.0);
    }
}
