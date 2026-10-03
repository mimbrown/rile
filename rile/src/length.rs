use crate::measurement::Measurement;

/// A length: a natural dimension with optional stretch and shrink.
///
/// Maps to TeX's "glue": a space that can grow by `stretch` or compress by `shrink`
/// around its natural `length`. All three components are [`Measurement`]s.
#[derive(Debug, Clone, Copy, Default)]
pub struct Length {
    pub length: Measurement,
    pub stretch: Measurement,
    pub shrink: Measurement,
}

impl Length {
    pub fn new(length: Measurement, stretch: Measurement, shrink: Measurement) -> Self {
        Self { length, stretch, shrink }
    }

    /// A fixed-width length (no stretch or shrink).
    pub fn from_measurement(m: Measurement) -> Self {
        Self { length: m, ..Default::default() }
    }

    /// Convenience constructor for a plain point length.
    pub fn pt(amount: f64) -> Self {
        Self::from_measurement(Measurement::pt(amount))
    }

    pub fn zero() -> Self {
        Self::default()
    }

    /// Convert the natural length to points. Returns `None` for relative units.
    pub fn to_pt(&self) -> Option<f64> {
        self.length.to_pt()
    }

    /// Convert the natural length to points. Panics for relative units.
    pub fn to_pt_abs(&self) -> f64 {
        self.length.to_pt_abs()
    }

    /// Returns a new Length with all components resolved to absolute pt values.
    pub fn absolute(&self) -> Self {
        Self::new(
            Measurement::pt(self.length.to_pt_abs()),
            Measurement::pt(self.stretch.to_pt_abs()),
            Measurement::pt(self.shrink.to_pt_abs()),
        )
    }
}

impl From<Measurement> for Length {
    fn from(m: Measurement) -> Self {
        Self::from_measurement(m)
    }
}

impl From<f64> for Length {
    fn from(n: f64) -> Self {
        Self::pt(n)
    }
}

impl std::fmt::Display for Length {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.length)?;
        if self.stretch.amount != 0.0 {
            write!(f, " plus {}", self.stretch)?;
        }
        if self.shrink.amount != 0.0 {
            write!(f, " minus {}", self.shrink)?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Length {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Plain number → pt length
        if let Ok(n) = s.parse::<f64>() {
            return Ok(Length::pt(n));
        }

        // Format: "<length> [plus <stretch>] [minus <shrink>]"
        // " plus " is 6 bytes, " minus " is 7 bytes
        let plus_pos = find_keyword(s, " plus ");
        let minus_pos = find_keyword(s, " minus ");

        let (length_str, stretch_str, shrink_str) = match (plus_pos, minus_pos) {
            (Some(p), Some(m)) if p < m => {
                (&s[..p], Some(&s[p + 6..m]), Some(&s[m + 7..]))
            }
            (Some(p), _) => (&s[..p], Some(&s[p + 6..]), None),
            (None, Some(m)) => (&s[..m], None, Some(&s[m + 7..])),
            (None, None) => (s, None, None),
        };

        let length = length_str.trim().parse::<Measurement>()?;
        let stretch = match stretch_str {
            Some(st) => st.trim().parse::<Measurement>()?,
            None => Measurement::default(),
        };
        let shrink = match shrink_str {
            Some(sh) => sh.trim().parse::<Measurement>()?,
            None => Measurement::default(),
        };

        Ok(Length::new(length, stretch, shrink))
    }
}

fn find_keyword(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w == needle.as_bytes())
}

// ─── Arithmetic ──────────────────────────────────────────────────────────────

impl std::ops::Neg for Length {
    type Output = Self;
    fn neg(self) -> Self {
        Length::new(-self.length, self.stretch, self.shrink)
    }
}

impl std::ops::Add for Length {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Length::new(
            self.length + other.length,
            self.stretch + other.stretch,
            self.shrink + other.shrink,
        )
    }
}

impl std::ops::Sub for Length {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Length::new(
            self.length - other.length,
            self.stretch - other.stretch,
            self.shrink - other.shrink,
        )
    }
}

impl std::ops::AddAssign for Length {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl std::ops::SubAssign for Length {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl std::ops::Mul<f64> for Length {
    type Output = Self;
    fn mul(self, scalar: f64) -> Self {
        Length::new(self.length * scalar, self.stretch * scalar, self.shrink * scalar)
    }
}

impl std::ops::Mul<Length> for f64 {
    type Output = Length;
    fn mul(self, l: Length) -> Length {
        l * self
    }
}

impl std::ops::Div<f64> for Length {
    type Output = Self;
    fn div(self, scalar: f64) -> Self {
        Length::new(self.length / scalar, self.stretch / scalar, self.shrink / scalar)
    }
}

impl PartialEq for Length {
    fn eq(&self, other: &Self) -> bool {
        self.length == other.length
            && self.stretch == other.stretch
            && self.shrink == other.shrink
    }
}

impl PartialOrd for Length {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.length.partial_cmp(&other.length)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::Unit;

    #[test]
    fn length_zero() {
        let l = Length::zero();
        assert_eq!(l.to_pt(), Some(0.0));
    }

    #[test]
    fn length_pt() {
        let l = Length::pt(6.0);
        assert_eq!(l.to_pt(), Some(6.0));
        assert_eq!(l.to_string(), "6pt");
    }

    #[test]
    fn length_with_stretch_shrink() {
        let l = Length::new(
            Measurement::pt(3.0),
            Measurement::pt(2.0),
            Measurement::pt(2.0),
        );
        assert_eq!(l.to_string(), "3pt plus 2pt minus 2pt");
    }

    #[test]
    fn length_display_no_shrink() {
        let l = Length::new(
            Measurement::new(6.0, Unit::Em),
            Measurement::pt(4.0),
            Measurement::default(),
        );
        assert_eq!(l.to_string(), "6em plus 4pt");
    }

    #[test]
    fn length_add() {
        let a = Length::new(
            Measurement::pt(3.0),
            Measurement::pt(1.0),
            Measurement::pt(1.0),
        );
        let b = Length::new(
            Measurement::pt(2.0),
            Measurement::pt(0.5),
            Measurement::pt(0.5),
        );
        let c = a + b;
        assert_eq!(c.length.amount, 5.0);
        assert_eq!(c.stretch.amount, 1.5);
        assert_eq!(c.shrink.amount, 1.5);
    }

    #[test]
    fn length_mul_scalar() {
        let l = Length::new(
            Measurement::pt(6.0),
            Measurement::pt(2.0),
            Measurement::pt(1.0),
        );
        let r = l * 3.0;
        assert_eq!(r.length.amount, 18.0);
        assert_eq!(r.stretch.amount, 6.0);
        assert_eq!(r.shrink.amount, 3.0);
    }

    #[test]
    fn length_parse_plain() {
        let l: Length = "10".parse().unwrap();
        assert_eq!(l.to_pt(), Some(10.0));
    }

    #[test]
    fn length_parse_with_stretch_and_shrink() {
        let l: Length = "6pt plus 4pt minus 2pt".parse().unwrap();
        assert_eq!(l.length.amount, 6.0);
        assert_eq!(l.stretch.amount, 4.0);
        assert_eq!(l.shrink.amount, 2.0);
    }

    #[test]
    fn length_parse_only_minus() {
        let l: Length = "6pt minus 2pt".parse().unwrap();
        assert_eq!(l.length.amount, 6.0);
        assert_eq!(l.stretch.amount, 0.0);
        assert_eq!(l.shrink.amount, 2.0);
    }

    #[test]
    fn length_from_measurement() {
        let m = Measurement::new(3.0, Unit::Em);
        let l = Length::from(m);
        assert_eq!(l.length.unit, Unit::Em);
        assert_eq!(l.length.amount, 3.0);
    }
}
