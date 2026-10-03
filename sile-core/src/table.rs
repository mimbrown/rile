//! Tables whose cells wrap to fit the frame, ruled above, below and under
//! the header rows.

use crate::class::medskip;
use crate::builder::{BuilderError, DocumentBuilder, LineSkips, Material, Typesetter};
use crate::length::Length;
use crate::measurement::Measurement;
use crate::node::{HBox, Node, VBox};
use crate::structure::Role;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CellAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Default)]
pub(crate) struct TableState {
    columns: Vec<CellAlign>,
    rows: Vec<(bool, Vec<Material>)>,
}

impl Typesetter {
    /// Start a row; header rows' cells are column headers.
    pub fn begin_table_row(&mut self, header: bool) -> &mut Self {
        if let Some(table) = self.tables.last_mut() {
            table.rows.push((header, Vec::new()));
        }
        self.begin_structure(Role::TR)
    }

    /// What is added until `end_table_cell` is the row's next cell.
    pub fn begin_table_cell(&mut self) -> &mut Self {
        let header = self.tables.last().and_then(|t| t.rows.last()).is_some_and(|r| r.0);
        self.begin_structure(if header { Role::TH } else { Role::TD }).begin_capture()
    }

    pub fn end_table_cell(&mut self) -> &mut Self {
        let cell = self.end_capture();
        if let Some((_, cells)) = self.tables.last_mut().and_then(|t| t.rows.last_mut()) {
            cells.push(cell);
        }
        self.end_structure()
    }

    pub fn end_table_row(&mut self) -> &mut Self {
        self.end_structure()
    }
}

impl DocumentBuilder {
    /// Start a table with these columns; rows are added with
    /// `begin_table_row` and laid out at `end_table`.
    pub fn begin_table(&mut self, columns: &[CellAlign]) -> Result<&mut Self, BuilderError> {
        self.leave_hmode(false)?;
        self.begin_structure(Role::Table);
        self.tables.push(TableState { columns: columns.to_vec(), rows: Vec::new() });
        Ok(self)
    }

    /// Set the table: columns at their natural widths when they fit,
    /// otherwise narrowed (widest first) with their cells wrapped ragged.
    pub fn end_table(&mut self) -> Result<&mut Self, BuilderError> {
        let Some(table) = self.tables.pop() else {
            return Ok(self);
        };
        let result = self.set_table(table);
        self.end_structure();
        result.map(|_| self)
    }

    fn set_table(&mut self, table: TableState) -> Result<(), BuilderError> {
        let em = self.font_spec().map_or(10.0, |f| f.size);
        let ex = self.x_height();
        let gap = em;
        let columns = table.rows.iter().map(|r| r.1.len()).chain([table.columns.len()]).max().unwrap_or(0);
        if columns == 0 {
            return Ok(());
        }
        let mut natural = vec![0.0_f64; columns];
        for (_, cells) in &table.rows {
            for (i, cell) in cells.iter().enumerate() {
                natural[i] = natural[i].max(self.natural_width(cell)?);
            }
        }
        let frame = self.frame_size()?.0;
        let skips = self.line_skips();
        let indent = skips.left.to_pt_abs();
        let available = frame - indent - skips.right.to_pt_abs() - gap * (columns - 1) as f64;
        let widths = column_widths(&natural, available);
        let total = widths.iter().sum::<f64>() + gap * (columns - 1) as f64;

        self.add_explicit_vskip(medskip())?;
        self.add_rule_line(indent, total, 0.08 * em, [0.0, 0.65 * ex]);
        let mut previous_header = None;
        for (header, cells) in &table.rows {
            if previous_header == Some(true) && !header {
                self.add_rule_line(indent, total, 0.05 * em, [0.4 * ex, 0.65 * ex]);
            } else if previous_header.is_some() {
                self.push_vglue(Length::pt(0.3 * em));
            }
            previous_header = Some(*header);
            let mut set = Vec::new();
            let mut migrating = Vec::new();
            for (i, cell) in cells.iter().enumerate() {
                let (lines, moving) = self.cell_lines(cell, widths[i], frame)?;
                set.push(lines);
                migrating.extend(moving);
            }
            let depth = set.iter().map(Vec::len).max().unwrap_or(0);
            let mut lines = Vec::new();
            for line in 0..depth {
                let mut nodes = vec![Node::kern(Length::pt(indent))];
                for (i, width) in widths.iter().enumerate() {
                    if i > 0 {
                        nodes.push(Node::kern(Length::pt(gap)));
                    }
                    let content = set.get(i).and_then(|c| c.get(line)).cloned().unwrap_or_default();
                    let align = table.columns.get(i).copied().unwrap_or_default();
                    nodes.push(Node::HBox(cell_box(content, *width, align)));
                }
                lines.push((VBox::new(nodes, Length::pt(indent + total)), false, Vec::new()));
            }
            if let Some(last) = lines.last_mut() {
                last.2 = migrating;
            }
            self.add_lines(lines);
        }
        self.add_rule_line(indent, total, 0.08 * em, [0.4 * ex, ex]);
        self.add_explicit_vskip(medskip())?;
        self.leave_hmode(false)
    }

    /// `cell` broken into ragged lines `width` wide, as the content of each
    /// line, and the material migrating from it (footnotes).
    fn cell_lines(&mut self, cell: &Material, width: f64, frame: f64) -> Result<(Vec<Vec<Node>>, Vec<Node>), BuilderError> {
        self.push_typesetter(None)?;
        let skips = self.line_skips();
        let em = self.font_spec().map_or(10.0, |f| f.size);
        let ragged = Length::new(Measurement::pt(frame - width - 0.1 * em), Measurement::pt(width.min(3.0 * em)), Measurement::pt(0.0));
        self.set_line_skips(LineSkips { left: Length::zero(), right: ragged, ..skips });
        self.set_paragraph_indent(0.0).set_current_indent(Some(0.0));
        let added = self.add_material(cell).map(|_| ());
        let nodes = self.pop_typesetter()?;
        added?;
        let (mut lines, mut migrating) = (Vec::new(), Vec::new());
        for node in nodes {
            match node {
                Node::VBox(vbox) => lines.extend(Some(trim(vbox.nodes)).filter(|l| !l.is_empty())),
                Node::Insertion(_) | Node::Migrating(_) => migrating.push(node),
                _ => {}
            }
        }
        Ok((lines, migrating))
    }
}

/// Natural widths when they fit; otherwise columns no wider than an equal
/// share keep theirs and the rest share what is left by natural width.
fn column_widths(natural: &[f64], available: f64) -> Vec<f64> {
    if natural.iter().sum::<f64>() <= available {
        return natural.to_vec();
    }
    let mut fixed = vec![false; natural.len()];
    loop {
        let left = available - natural.iter().zip(&fixed).filter(|(_, f)| **f).map(|(w, _)| w).sum::<f64>();
        let flexible: f64 = natural.iter().zip(&fixed).filter(|(_, f)| !**f).map(|(w, _)| w).sum();
        let count = fixed.iter().filter(|f| !**f).count();
        let share = left / count.max(1) as f64;
        let mut changed = false;
        for (i, w) in natural.iter().enumerate() {
            if !fixed[i] && *w <= share {
                fixed[i] = true;
                changed = true;
            }
        }
        if !changed {
            return natural.iter().zip(&fixed).map(|(w, f)| if *f { *w } else { (left * w / flexible).max(0.0) }).collect();
        }
    }
}

/// A line's content without the margin glue, fill and markers around it.
fn trim(mut nodes: Vec<Node>) -> Vec<Node> {
    let edge = |n: &Node| matches!(n, Node::Glue(_) | Node::HFillGlue(_) | Node::HssGlue(_) | Node::ZeroHBox(_) | Node::Penalty(_));
    while nodes.last().is_some_and(edge) {
        nodes.pop();
    }
    let start = nodes.iter().position(|n| !edge(n)).unwrap_or(nodes.len());
    nodes.drain(..start);
    nodes
}

fn cell_box(content: Vec<Node>, width: f64, align: CellAlign) -> HBox {
    let natural: f64 = content.iter().map(|n| n.width().to_pt_abs()).sum();
    let shift = match align {
        CellAlign::Left => 0.0,
        CellAlign::Center => (width - natural) / 2.0,
        CellAlign::Right => width - natural,
    };
    let mut nodes = vec![Node::kern(Length::pt(shift))];
    nodes.extend(content);
    let vbox = VBox::new(nodes, Length::pt(width));
    HBox { nodes: vbox.nodes, ..HBox::new(Length::pt(width), vbox.height, vbox.depth) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests_support::*;
    use crate::class::Plain;

    fn table(d: &mut DocumentBuilder, rows: &[&[&str]]) {
        d.begin_table(&[CellAlign::Left, CellAlign::Right]).unwrap();
        for (i, row) in rows.iter().enumerate() {
            d.begin_table_row(i == 0);
            for cell in *row {
                d.begin_table_cell().add_text(*cell);
                d.end_table_cell();
            }
            d.end_table_row();
        }
        d.end_table().unwrap();
    }

    #[test]
    fn narrow_columns_keep_their_width_and_wide_ones_wrap() {
        assert_eq!(column_widths(&[10.0, 20.0], 100.0), [10.0, 20.0]);
        assert_eq!(column_widths(&[10.0, 200.0, 100.0], 100.0), [10.0, 60.0, 30.0]);
    }

    #[test]
    fn cells_line_up_in_columns_and_long_ones_wrap() {
        let mut d = doc(Plain::new());
        d.add_text("Before.");
        table(&mut d, &[&["Name", "Value"], &["one", "1"], &[&"long text ".repeat(30), "22"]]);
        d.add_text("After.");
        let trace = d.render_debug().unwrap();
        let lines: Vec<&str> = trace.lines().collect();
        let x_of = |word: &str| -> f64 {
            let at = lines.iter().position(|l| l.ends_with(&format!("({word})"))).unwrap();
            lines[..at].iter().rev().find_map(|l| l.strip_prefix("Mx \t")).unwrap().parse().unwrap()
        };
        assert!((x_of("Name") - x_of("one")).abs() < 1e-3);
        assert!(x_of("1") > x_of("Value"), "numbers are right aligned");
        assert!(x_of("22") > x_of("Value"));
        assert_eq!(lines.iter().filter(|l| l.ends_with("(text)")).count(), 30);
    }

    #[test]
    fn tagged_tables_have_rows_of_header_and_data_cells() {
        let mut d = doc(Plain::new());
        d.set_tagged(true);
        table(&mut d, &[&["A", "B"], &["1", "2"]]);
        let tree = d.structure().unwrap().clone();
        let roles: Vec<Role> = tree.elements.iter().map(|e| e.role).collect();
        assert_eq!(roles[1..], [Role::Table, Role::TR, Role::TH, Role::TH, Role::TR, Role::TD, Role::TD]);
    }
}
