//! Paper sizes, and frames as solved on a page.

use crate::font::Direction;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaperSize {
    pub width: f64,
    pub height: f64,
}

impl PaperSize {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    pub const A4: Self = Self::new(595.275597, 841.8897728999999);
    pub const A5: Self = Self::new(419.52756359999995, 595.275597);
    pub const A3: Self = Self::new(841.8897728999999, 1190.551194);
    pub const LETTER: Self = Self::new(612.0, 792.0);
    pub const LEGAL: Self = Self::new(612.0, 1008.0);
    pub const B5: Self = Self::new(498.89764319999995, 708.661425);

    pub fn landscape(self) -> Self {
        Self::new(self.height, self.width)
    }
}

/// A size SILE names, such as `a5` or `letter`, or `width x height` in
/// absolute units, such as `15cm x 6cm`.
impl std::str::FromStr for PaperSize {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, String> {
        let named = match name.trim().to_lowercase().as_str() {
            "a3" => Some(Self::A3),
            "a4" => Some(Self::A4),
            "a5" => Some(Self::A5),
            "a6" => Some(Self::new(297.6377985, 419.52756359999995)),
            "a7" => Some(Self::new(209.76378179999998, 297.6377985)),
            "a8" => Some(Self::new(147.40157639999998, 209.76378179999998)),
            "b5" => Some(Self::B5),
            "b6" => Some(Self::new(354.3307125, 498.89764319999995)),
            "letter" => Some(Self::LETTER),
            "legal" => Some(Self::LEGAL),
            "halfletter" => Some(Self::new(396.0, 612.0)),
            _ => None,
        };
        if let Some(size) = named {
            return Ok(size);
        }
        let bad = || format!("unknown paper size {name}");
        let (w, h) = name.split_once(" x ").ok_or_else(bad)?;
        let pt = |v: &str| {
            let v = v.trim();
            v.parse::<f64>().ok().or_else(|| v.parse::<crate::measurement::Measurement>().ok()?.to_pt()).ok_or_else(bad)
        };
        Ok(Self::new(pt(w)?, pt(h)?))
    }
}

/// One of the four ways text or lines can advance across a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    LTR,
    RTL,
    TTB,
    BTT,
}

impl Flow {
    pub fn reversed(self) -> Self {
        match self {
            Self::LTR => Self::RTL,
            Self::RTL => Self::LTR,
            Self::TTB => Self::BTT,
            Self::BTT => Self::TTB,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "LTR" => Some(Self::LTR),
            "RTL" => Some(Self::RTL),
            "TTB" => Some(Self::TTB),
            "BTT" => Some(Self::BTT),
            _ => None,
        }
    }
}

/// How a frame fills: the way text runs along a line, then the way lines
/// follow one another (SILE's frame `direction`, such as `RTL-TTB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDirection {
    pub writing: Flow,
    pub page: Flow,
}

impl FrameDirection {
    pub const LTR: Self = Self { writing: Flow::LTR, page: Flow::TTB };
    pub const RTL: Self = Self { writing: Flow::RTL, page: Flow::TTB };
    /// Vertical Japanese: columns down the page, from right to left.
    pub const TATE: Self = Self { writing: Flow::TTB, page: Flow::RTL };

    /// `WRITING-PAGE`, either part optional as in SILE.
    pub fn parse(s: &str) -> Option<Self> {
        let (writing, page) = match s.split_once('-') {
            Some((w, p)) => (Flow::parse(w)?, Flow::parse(p)?),
            None => (Flow::parse(s)?, Flow::TTB),
        };
        Some(Self { writing, page })
    }

    /// The direction text is shaped in.
    pub fn text(self) -> Direction {
        match self.writing {
            Flow::LTR => Direction::LTR,
            Flow::RTL => Direction::RTL,
            Flow::TTB | Flow::BTT => Direction::TTB,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self.writing, Flow::TTB | Flow::BTT)
    }
}

impl From<Direction> for FrameDirection {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::LTR | Direction::Frame => Self::LTR,
            Direction::RTL => Self::RTL,
            Direction::TTB => Self { writing: Flow::TTB, page: Flow::TTB },
        }
    }
}
/// A solved frame, in points from the top-left of the page.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameGeometry {
    pub id: String,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub next: Option<String>,
    pub direction: Option<FrameDirection>,
    /// Set as vertical Japanese: lines broken first-fit, one zenkaku tall
    /// (SILE's tate frames).
    pub tate: bool,
    pub balanced: bool,
}

impl FrameGeometry {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }

    fn flow(&self) -> FrameDirection {
        self.direction.unwrap_or(FrameDirection::LTR)
    }

    /// How long a line is: across the frame, or down it when the writing
    /// is vertical (SILE's `getLineWidth`).
    pub fn line_length(&self) -> f64 {
        if self.flow().is_vertical() { self.height() } else { self.width() }
    }

    /// How much room lines have, along the way they follow each other
    /// (SILE's `getTargetLength`).
    pub fn target_length(&self) -> f64 {
        match self.flow().page {
            Flow::TTB | Flow::BTT => self.height(),
            Flow::LTR | Flow::RTL => self.width(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.5
    }

    // -- PaperSize --

    #[test]
    fn paper_size_a4() {
        let a4 = PaperSize::A4;
        assert!(approx_eq(a4.width, 595.276));
        assert!(approx_eq(a4.height, 841.89));
    }

    #[test]
    fn paper_size_landscape() {
        let l = PaperSize::A4.landscape();
        assert!(approx_eq(l.width, 841.89));
        assert!(approx_eq(l.height, 595.276));
    }
}
