//! Paper sizes. Frames are declared with `framespec`.

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
