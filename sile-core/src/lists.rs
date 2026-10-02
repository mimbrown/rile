//! Numbered and bulleted lists (SILE's `lists` package).

use crate::builder::{BuilderError, DocumentBuilder, LineSkips};
use crate::counter::format_number;
use crate::length::Length;
use crate::structure::Role;
use crate::measurement::{Measurement, Unit};
use crate::node::Node;

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

impl DocumentBuilder {
    pub fn list_settings_mut(&mut self) -> &mut ListSettings {
        &mut self.lists.settings
    }

    /// Start a list; items follow with `begin_item` / `end_item`.
    pub fn begin_list(&mut self, kind: ListKind, options: &ListOptions) -> Result<&mut Self, BuilderError> {
        let depth = self.lists.levels.iter().filter(|l| l.kind == kind).count() + 1;
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
        let base_indent = if depth == 1 { self.paragraph_indent() } else { 0.0 };
        let margin = match kind {
            ListKind::Enumerate => self.lists.settings.enumerate_margin,
            ListKind::Itemize => self.lists.settings.itemize_margin,
        };
        let indent = self.resolve(margin);
        self.list_spacing(true, true, 0)?;
        self.begin_structure(Role::L);
        let skips = self.line_skips();
        self.lists.levels.push(ListLevel {
            kind,
            label,
            counter: options.start.map_or(0, |s| s - 1),
            indent,
            saved: (skips, self.paragraph_indent()),
        });
        let left = skips.left + Length::pt(base_indent + indent);
        self.set_current_indent(Some(0.0)).set_paragraph_indent(0.0).set_line_skips(LineSkips { left, ..skips });
        Ok(self)
    }

    pub fn end_list(&mut self) -> Result<&mut Self, BuilderError> {
        if let Some(level) = self.lists.levels.pop() {
            let (skips, indent) = level.saved;
            self.set_line_skips(skips).set_paragraph_indent(indent);
        }
        self.list_spacing(true, false, 0)?;
        self.end_structure();
        Ok(self)
    }

    /// Start an item of the innermost list with its label hung in the
    /// margin; `bullet` overrides the list's bullet for this item.
    pub fn begin_item(&mut self, bullet: Option<&str>) -> Result<&mut Self, BuilderError> {
        let Some(level) = self.lists.levels.last_mut() else {
            return Ok(self);
        };
        level.counter += 1;
        let (counter, label, indent) = (level.counter, level.label.clone(), level.indent);
        self.list_spacing(false, true, counter)?;
        self.begin_structure(Role::LI).begin_structure(Role::Lbl);
        let text = match &label {
            ListLabel::Number { display, before, after } => {
                format!("{before}{}{after}", format_number(counter, display).unwrap_or_else(|| counter.to_string()))
            }
            ListLabel::Bullet(b) => bullet.unwrap_or(b).to_string(),
        };
        self.start_hbox().add_text(text);
        let mut mark = self.make_hbox()?;
        let stepback = match label {
            ListLabel::Number { .. } => indent - self.resolve(self.lists.settings.enumerate_label_indent),
            ListLabel::Bullet(_) => indent / 2.0 + mark.width.to_pt_abs() / 2.0,
        };
        self.add_kern(Length::pt(-stepback));
        mark.width = Length::pt(stepback);
        self.add_box(mark);
        self.end_structure().begin_structure(Role::LBody);
        Ok(self)
    }

    pub fn end_item(&mut self) -> Result<&mut Self, BuilderError> {
        self.set_current_indent(Some(0.0));
        let counter = self.lists.levels.last().map_or(0, |l| l.counter);
        self.list_spacing(false, false, counter)?;
        self.end_structure().end_structure();
        Ok(self)
    }

    /// SILE's `maybeAddListSpacing`.
    fn list_spacing(&mut self, list: bool, entering: bool, counter: i64) -> Result<(), BuilderError> {
        let depth = self.lists.levels.len() + usize::from(list);
        self.leave_hmode(false)?;
        if entering && !list && (counter != 1 || depth >= 2) {
            self.push_list_spacing();
        }
        if !entering && list {
            self.push_list_spacing();
            if depth == 1
                && let Some(i) = self.lists.spacing
            {
                self.remove_vertical(i - 1, Node::is_vglue);
            }
        }
        Ok(())
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
