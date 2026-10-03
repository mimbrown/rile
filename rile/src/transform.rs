//! Boxes drawn rotated or scaled, and tables of boxes in columns.
// SILE: `rotate` and `scalebox` packages; `simpletable`.

use crate::builder::{Arranger, BuilderError, Typesetter};
use crate::builder::smallskip;
use crate::length::Length;
use crate::node::{HBox, Ink};

/// How a box's content is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    /// Turned clockwise by `angle` radians about the middle of the box,
    /// which was `width` wide, `height` high and `depth` deep before.
    // SILE turns it about a point half the difference between the two
    // widths further on, which moves it off the space it takes up.
    Rotate { angle: f64, width: f64, height: f64, depth: f64 },
    /// Stretched by these ratios from where the box starts on the baseline;
    /// a negative ratio mirrors it.
    // SILE mirrors from the scaled width back rather than the box's own,
    // which only fits a ratio of -1.
    Scale { x: f64, y: f64 },
}

/// An affine map `[a, b, c, d, e, f]` of page coordinates (y down), taking
/// (x, y) to (a x + c y + e, b x + d y + f).
pub type Matrix = [f64; 6];

impl Transform {
    /// The map that draws content starting at `(x, y)` on the baseline.
    pub fn matrix(&self, x: f64, y: f64) -> Matrix {
        match *self {
            Transform::Rotate { angle, width, height, depth } => {
                let (ox, oy) = (x + width / 2.0, y - (height - depth) / 2.0);
                let (sin, cos) = angle.sin_cos();
                [cos, sin, -sin, cos, ox - cos * ox + sin * oy, oy - sin * ox - cos * oy]
            }
            Transform::Scale { x: sx, y: sy } => [sx, 0.0, 0.0, sy, x - sx * x, y - sy * y],
        }
    }
}

/// `hbox` turned clockwise by `degrees`, measured as the space it then
/// covers.
pub fn rotated(hbox: HBox, degrees: f64) -> HBox {
    let (w, height, depth) = (pt(&hbox.width), pt(&hbox.height), pt(&hbox.depth));
    let h = height + depth;
    let theta = -degrees.to_radians();
    let (st, ct) = theta.sin_cos();
    let (width, up, down) = match (st <= 0.0, ct <= 0.0) {
        (true, true) => (-w * ct - h * st, 0.5 * (h - h * ct - w * st), 0.5 * (h + h * ct + w * st)),
        (true, false) => (w * ct - h * st, 0.5 * (h + h * ct - w * st), 0.5 * (h - h * ct + w * st)),
        (false, true) => (-w * ct + h * st, 0.5 * (h - h * ct + w * st), 0.5 * (h + h * ct - w * st)),
        (false, false) => (w * ct + h * st, 0.5 * (h + h * ct + w * st), 0.5 * (h - h * ct - w * st)),
    };
    let transform = Transform::Rotate { angle: degrees.to_radians(), width: w, height, depth };
    let shift = (w - width) / 2.0;
    HBox {
        ink: Some(Ink::Transform(transform, shift)),
        nodes: hbox.nodes,
        ..HBox::new(Length::pt(width), Length::pt(up), Length::pt((-down).max(0.0)))
    }
}

impl Typesetter {
    /// Add `hbox` turned clockwise by `degrees`, taking up the space it then
    /// covers.
    // SILE: `\rotate`.
    pub fn add_rotated(&mut self, hbox: HBox, degrees: f64) -> &mut Self {
        self.add_box(rotated(hbox, degrees))
    }

    /// Add `hbox` scaled by `x` across and `y` up; negative ratios mirror
    /// it.
    // SILE: `\scalebox`.
    pub fn add_scaled(&mut self, hbox: HBox, x: f64, y: f64) -> Result<&mut Self, BuilderError> {
        if x == 0.0 || y == 0.0 {
            return Err(BuilderError::Layout("scaling ratio cannot be zero".into()));
        }
        let width = hbox.width * x.abs();
        let (height, depth) = if y > 0.0 { (pt(&hbox.height) * y, pt(&hbox.depth) * y) } else { (pt(&hbox.depth) * -y, pt(&hbox.height) * -y) };
        let scaled = HBox {
            ink: Some(Ink::Transform(Transform::Scale { x, y }, 0.0)),
            nodes: hbox.nodes,
            ..HBox::new(width, Length::pt(height), Length::pt(depth))
        };
        Ok(self.add_box(scaled))
    }
}

pub(crate) fn add_simple_table<A: Arranger + ?Sized>(a: &mut A, rows: Vec<Vec<HBox>>) -> Result<(), BuilderError> {
    a.leave_hmode(false)?;
    let mut widths: Vec<Length> = Vec::new();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            match widths.get_mut(i) {
                Some(w) if pt(&cell.width) > pt(w) => *w = cell.width,
                Some(_) => {}
                None => widths.push(cell.width),
            }
        }
    }
    let indent = a.paragraph_indent();
    a.set_paragraph_indent(0.0);
    let result = rows.into_iter().try_for_each(|row| -> Result<(), BuilderError> {
        for (i, mut cell) in row.into_iter().enumerate() {
            cell.width = widths[i];
            a.add_box(cell);
        }
        a.leave_hmode(false)?;
        a.add_explicit_vskip(smallskip())?;
        Ok(())
    });
    a.set_paragraph_indent(indent);
    result?;
    a.leave_hmode(false)?;
    Ok(())
}

fn pt(length: &Length) -> f64 {
    length.length.to_pt().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::Galley;
    use crate::test_support::galley;

    fn word(d: &mut Galley, text: &str) -> HBox {
        d.start_hbox().add_text(text);
        d.make_hbox().unwrap()
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height() {
        let mut d = galley();
        let hbox = word(&mut d, "Turn");
        let (w, h) = (pt(&hbox.width), pt(&hbox.height) + pt(&hbox.depth));
        d.add_rotated(hbox, 90.0);
        d.add_text("after");
        let layout = d.lay_out().unwrap();
        assert!(layout.render_debug().contains("(Turn)"));
        let line = layout.pages[0].content.iter().flat_map(|(_, n)| n).find_map(|n| match n {
            crate::node::Node::VBox(v) => Some(v),
            _ => None,
        });
        let rotated = line.unwrap().nodes.iter().find_map(|n| match n {
            crate::node::Node::HBox(b) if matches!(b.ink, Some(Ink::Transform(..))) => Some(b),
            _ => None,
        });
        let rotated = rotated.unwrap();
        assert!((pt(&rotated.width) - h).abs() < 1e-9);
        assert!((pt(&rotated.height) - (h + w) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn scaling_mirrors_and_rejects_zero() {
        let mut d = galley();
        let hbox = word(&mut d, "Big");
        assert!(d.add_scaled(hbox.clone(), 0.0, 1.0).is_err());
        d.add_scaled(hbox, -2.0, 1.0).unwrap();
        assert_eq!(Transform::Scale { x: -2.0, y: 1.0 }.matrix(10.0, 20.0), [-2.0, 0.0, 0.0, 1.0, 30.0, 0.0]);
    }

    #[test]
    fn table_columns_take_the_widest_cell() {
        let mut d = galley();
        let rows = vec![vec![word(&mut d, "a"), word(&mut d, "b")], vec![word(&mut d, "wide cell"), word(&mut d, "c")]];
        let wide = pt(&rows[1][0].width);
        d.add_simple_table(rows).unwrap();
        let trace = d.lay_out().unwrap().render_debug();
        let x_of = |text: &str| -> f64 {
            let lines: Vec<&str> = trace.lines().collect();
            let at = lines.iter().position(|l| l.ends_with(&format!("({text})"))).unwrap();
            lines[..at].iter().rev().find_map(|l| l.strip_prefix("Mx \t")).unwrap().parse().unwrap()
        };
        assert!((x_of("b") - x_of("c")).abs() < 1e-3);
        assert!(x_of("c") - x_of("wide") >= wide);
    }
}
