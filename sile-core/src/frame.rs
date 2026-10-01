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
