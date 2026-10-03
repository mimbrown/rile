//! Numbered and bulleted lists (SILE's `lists` package).

use crate::builder::{Arranger, BuilderError, LineSkips, Typesetter};
use crate::counter::format_number;
use crate::length::Length;
use crate::structure::Role;
use crate::measurement::{Measurement, Unit};
use crate::node::{HBox, Ink, Node};
use crate::svg_image::{SvgFigure, SvgImage};
use std::sync::Arc;

const UNCHECKED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="black" stroke-width="0.8"/></svg>"#;
const CHECKED: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="black" stroke-width="0.8"/><path d="M2.3 5.3 L4.3 7.3 L7.8 2.9" fill="none" stroke="black" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Enumerate,
    Itemize,
}

/// How an item is marked: a formatted number or a bullet.
#[derive(Debug, Clone, PartialEq)]
pub enum ListLabel {
    Number { display: String, before: String, after: String },
    Bullet(String),
}

impl ListLabel {
    /// SILE's default for a list of `kind` nested `depth` deep in lists of
    /// that kind.
    pub fn default_for(kind: ListKind, depth: usize) -> Self {
        let level = (depth.max(1) - 1) % 6;
        match kind {
            ListKind::Enumerate => ListLabel::Number {
                display: ["arabic", "roman", "alpha"][level % 3].to_string(),
                before: String::new(),
                after: if level < 3 { "." } else { ")" }.to_string(),
            },
            ListKind::Itemize => ListLabel::Bullet(["•", "◦", "–"][level % 3].to_string()),
        }
    }
}

/// Overrides for a list's label and first number.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub start: Option<i64>,
    pub display: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub bullet: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListSettings {
    pub enumerate_margin: Measurement,
    pub enumerate_label_indent: Measurement,
    pub itemize_margin: Measurement,
    /// Space between items (SILE's `lists.parskip`).
    pub parskip: Length,
}

impl Default for ListSettings {
    fn default() -> Self {
        Self {
            enumerate_margin: Measurement::new(2.0, Unit::Em),
            enumerate_label_indent: Measurement::new(0.5, Unit::Em),
            itemize_margin: Measurement::new(1.5, Unit::Em),
            parskip: Length::new(Measurement::pt(0.0), Measurement::pt(1.0), Measurement::pt(0.0)),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ListLevel {
    kind: ListKind,
    label: ListLabel,
    counter: i64,
    indent: f64,
    saved: (LineSkips, f64),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Lists {
    pub(crate) levels: Vec<ListLevel>,
    /// Where the last item spacing went in the vertical list, so it is
    /// neither doubled nor left after the outermost list.
    spacing: Option<usize>,
    pub(crate) settings: ListSettings,
}

pub(crate) fn begin_list<A: Arranger + ?Sized>(a: &mut A, kind: ListKind, options: &ListOptions) -> Result<(), BuilderError> {
    let depth = a.lists.levels.iter().filter(|l| l.kind == kind).count() + 1;
    let mut label = ListLabel::default_for(kind, depth);
    match &mut label {
        ListLabel::Number { display, before, after } => {
            if options.before.is_some() || options.after.is_some() {
                *before = options.before.clone().unwrap_or_default();
                *after = options.after.clone().unwrap_or_default();
            }
            if let Some(d) = &options.display {
                *display = d.clone();
            }
        }
        ListLabel::Bullet(bullet) => {
            if let Some(b) = &options.bullet {
                *bullet = b.clone();
            }
        }
    }
    let base_indent = if depth == 1 { a.paragraph_indent() } else { 0.0 };
    let margin = match kind {
        ListKind::Enumerate => a.lists.settings.enumerate_margin,
        ListKind::Itemize => a.lists.settings.itemize_margin,
    };
    let indent = a.resolve(margin);
    list_spacing(a, true, true, 0)?;
    a.begin_structure(Role::L);
    let skips = a.line_skips();
    let saved = (skips, a.paragraph_indent());
    a.lists.levels.push(ListLevel { kind, label, counter: options.start.map_or(0, |s| s - 1), indent, saved });
    let left = skips.left + Length::pt(base_indent + indent);
    a.set_current_indent(Some(0.0)).set_paragraph_indent(0.0).set_line_skips(LineSkips { left, ..skips });
    Ok(())
}

pub(crate) fn end_list<A: Arranger + ?Sized>(a: &mut A) -> Result<(), BuilderError> {
    if let Some(level) = a.lists.levels.pop() {
        let (skips, indent) = level.saved;
        a.set_line_skips(skips).set_paragraph_indent(indent);
    }
    list_spacing(a, true, false, 0)?;
    a.end_structure();
    Ok(())
}

pub(crate) fn begin_item<A: Arranger + ?Sized>(a: &mut A, bullet: Option<&str>) -> Result<(), BuilderError> {
    begin_marked_item(a, bullet, None)
}

pub(crate) fn begin_task_item<A: Arranger + ?Sized>(a: &mut A, done: bool) -> Result<(), BuilderError> {
    begin_marked_item(a, None, Some(done))
}

fn begin_marked_item<A: Arranger + ?Sized>(a: &mut A, bullet: Option<&str>, task: Option<bool>) -> Result<(), BuilderError> {
    let Some(level) = a.lists.levels.last_mut() else {
        return Ok(());
    };
    level.counter += 1;
    let (counter, label, indent) = (level.counter, level.label.clone(), level.indent);
    list_spacing(a, false, true, counter)?;
    a.begin_structure(Role::LI).begin_structure(Role::Lbl);
    let text = match &label {
        ListLabel::Number { display, before, after } => {
            format!("{before}{}{after}", format_number(counter, display).unwrap_or_else(|| counter.to_string()))
        }
        ListLabel::Bullet(b) => bullet.unwrap_or(b).to_string(),
    };
    a.start_hbox();
    match task {
        Some(done) => {
            let size = 0.65 * a.font_spec().map_or(10.0, |f| f.size);
            let image = SvgImage::parse(if done { CHECKED } else { UNCHECKED }, 72.0);
            let figure = SvgFigure { scale: size / image.height, image: Arc::new(image), drop: false };
            a.add_box(HBox { ink: Some(Ink::Svg(figure)), ..HBox::new(Length::pt(size), Length::pt(size), Length::zero()) });
            a.set_actual_text(if done { "\u{2611}" } else { "\u{2610}" });
        }
        None => {
            a.add_text(text);
        }
    }
    let mut mark = a.make_hbox()?;
    let stepback = match label {
        ListLabel::Number { .. } => indent - a.resolve(a.lists.settings.enumerate_label_indent),
        ListLabel::Bullet(_) => indent / 2.0 + mark.width.to_pt_abs() / 2.0,
    };
    a.add_kern(Length::pt(-stepback));
    mark.width = Length::pt(stepback);
    a.add_box(mark);
    a.end_structure().begin_structure(Role::LBody);
    Ok(())
}

pub(crate) fn end_item<A: Arranger + ?Sized>(a: &mut A) -> Result<(), BuilderError> {
    a.set_current_indent(Some(0.0));
    let counter = a.lists.levels.last().map_or(0, |l| l.counter);
    list_spacing(a, false, false, counter)?;
    a.end_structure().end_structure();
    Ok(())
}

/// SILE's `maybeAddListSpacing`.
fn list_spacing<A: Arranger + ?Sized>(a: &mut A, list: bool, entering: bool, counter: i64) -> Result<(), BuilderError> {
    let depth = a.lists.levels.len() + usize::from(list);
    a.leave_hmode(false)?;
    if entering && !list && (counter != 1 || depth >= 2) {
        a.push_list_spacing();
    }
    if !entering && list {
        a.push_list_spacing();
        if depth == 1
            && let Some(i) = a.lists.spacing
        {
            a.remove_vertical(i - 1, Node::is_vglue);
        }
    }
    Ok(())
}

impl Typesetter {
    pub fn list_settings_mut(&mut self) -> &mut ListSettings {
        &mut self.lists.settings
    }

    fn push_list_spacing(&mut self) {
        if self.lists.spacing != Some(self.vertical_len()) {
            self.add_vertical(Node::vglue(self.lists.settings.parskip));
            self.lists.spacing = Some(self.vertical_len());
        }
    }

    fn resolve(&self, m: Measurement) -> f64 {
        let em = self.font_spec().map_or(10.0, |f| f.size);
        match m.unit {
            Unit::Em => m.amount * em,
            Unit::En => m.amount * em / 2.0,
            _ => m.to_pt_abs(),
        }
    }
}
