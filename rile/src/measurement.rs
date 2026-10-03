/// A unit of measurement.
///
/// Absolute units can be converted to points directly. Relative units (em, ex, %fw, etc.)
/// require layout context to resolve and return `None` from [`Unit::to_pt_factor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    // Absolute units
    Pt,
    Twip,
    Mm,
    Cm,
    M,
    Hm,
    In,
    Ft,
    Pc,
    Px,
    // Relative units (require context)
    Em,
    Ex,
    Spc,
    En,
    Zw,
    PercentPw,
    PercentPh,
    PercentPmin,
    PercentPmax,
    PercentFw,
    PercentFh,
    PercentFmin,
    PercentFmax,
    PercentLw,
    Ps,
    Bs,
}

impl Unit {
    /// Returns the factor to multiply `amount` by to get points, or `None` for relative units.
    pub fn to_pt_factor(self) -> Option<f64> {
        match self {
            Unit::Pt => Some(1.0),
            Unit::Twip => Some(0.05),
            Unit::Mm => Some(2.8346457),
            Unit::Cm => Some(28.346457),
            Unit::M => Some(2834.6457),
            Unit::Hm => Some(0.028346457),
            Unit::In => Some(72.0),
            Unit::Ft => Some(864.0),
            Unit::Pc => Some(12.0),
            Unit::Px => Some(0.75),
            _ => None,
        }
    }

    /// Returns `true` if this unit requires layout context to resolve.
    pub fn is_relative(self) -> bool {
        self.to_pt_factor().is_none()
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Unit::Pt => "pt",
            Unit::Twip => "twip",
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::Hm => "hm",
            Unit::In => "in",
            Unit::Ft => "ft",
            Unit::Pc => "pc",
            Unit::Px => "px",
            Unit::Em => "em",
            Unit::Ex => "ex",
            Unit::Spc => "spc",
            Unit::En => "en",
            Unit::Zw => "zw",
            Unit::PercentPw => "%pw",
            Unit::PercentPh => "%ph",
            Unit::PercentPmin => "%pmin",
            Unit::PercentPmax => "%pmax",
            Unit::PercentFw => "%fw",
            Unit::PercentFh => "%fh",
            Unit::PercentFmin => "%fmin",
            Unit::PercentFmax => "%fmax",
            Unit::PercentLw => "%lw",
            Unit::Ps => "ps",
            Unit::Bs => "bs",
        })
    }
}

impl std::str::FromStr for Unit {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pt" => Ok(Unit::Pt),
            "twip" => Ok(Unit::Twip),
            "mm" => Ok(Unit::Mm),
            "cm" => Ok(Unit::Cm),
            "m" => Ok(Unit::M),
            "hm" => Ok(Unit::Hm),
            "in" => Ok(Unit::In),
            "ft" => Ok(Unit::Ft),
            "pc" => Ok(Unit::Pc),
            "px" => Ok(Unit::Px),
            "em" => Ok(Unit::Em),
            "ex" => Ok(Unit::Ex),
            "spc" => Ok(Unit::Spc),
            "en" => Ok(Unit::En),
            "zw" => Ok(Unit::Zw),
            "%pw" => Ok(Unit::PercentPw),
            "%ph" => Ok(Unit::PercentPh),
            "%pmin" => Ok(Unit::PercentPmin),
            "%pmax" => Ok(Unit::PercentPmax),
            "%fw" => Ok(Unit::PercentFw),
            "%fh" => Ok(Unit::PercentFh),
            "%fmin" => Ok(Unit::PercentFmin),
            "%fmax" => Ok(Unit::PercentFmax),
            "%lw" => Ok(Unit::PercentLw),
            "ps" => Ok(Unit::Ps),
            "bs" => Ok(Unit::Bs),
            _ => Err(format!("Unknown unit: '{s}'")),
        }
    }
}

// ─── Measurement ─────────────────────────────────────────────────────────────

/// A measurement: an amount paired with a unit.
///
/// Absolute measurements can be converted to points with [`Measurement::to_pt`].
/// Relative measurements (e.g. `3em`, `50%fw`) require layout context and
/// return `None` from `to_pt`.
#[derive(Debug, Clone, Copy)]
pub struct Measurement {
    pub amount: f64,
    pub unit: Unit,
}

impl Measurement {
    pub fn new(amount: f64, unit: Unit) -> Self {
        Self { amount, unit }
    }

    /// Construct an absolute point measurement.
    pub fn pt(amount: f64) -> Self {
        Self { amount, unit: Unit::Pt }
    }

    pub fn is_relative(&self) -> bool {
        self.unit.is_relative()
    }

    /// Convert to points. Returns `None` for relative units.
    pub fn to_pt(&self) -> Option<f64> {
        self.unit.to_pt_factor().map(|f| self.amount * f)
    }

    /// Convert to points. Panics for relative units.
    pub fn to_pt_abs(&self) -> f64 {
        self.to_pt().unwrap_or_else(|| {
            panic!("Cannot convert relative measurement '{self}' to points without context")
        })
    }
}

impl Default for Measurement {
    fn default() -> Self {
        Self::pt(0.0)
    }
}

/// Format an f64 like Lua (LuaJIT): no trailing `.0` for whole numbers.
pub(crate) fn format_f64(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 {
        format!("{:.0}", v)
    } else {
        format!("{}", v)
    }
}

impl std::fmt::Display for Measurement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", format_f64(self.amount), self.unit)
    }
}

impl std::str::FromStr for Measurement {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        // Plain number → pt
        if let Ok(n) = s.parse::<f64>() {
            return Ok(Measurement::pt(n));
        }

        // Find where the numeric part ends
        let mut end = 0;
        let bytes = s.as_bytes();

        if bytes.first() == Some(&b'-') {
            end += 1;
        }
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end < bytes.len() && bytes[end] == b'.' {
            end += 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
        }

        if end == 0 || end == s.len() {
            return Err(format!("Could not parse measurement '{s}'"));
        }

        let amount: f64 = s[..end]
            .parse()
            .map_err(|_| format!("Invalid number in measurement '{s}'"))?;
        let unit: Unit = s[end..].parse()?;
        Ok(Measurement::new(amount, unit))
    }
}

// ─── Arithmetic ──────────────────────────────────────────────────────────────

impl std::ops::Neg for Measurement {
    type Output = Self;
    fn neg(self) -> Self {
        Measurement::new(-self.amount, self.unit)
    }
}

/// Add two measurements. Same unit → keep unit. Different absolute units → convert to pt.
impl std::ops::Add for Measurement {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        if self.unit == other.unit {
            Measurement::new(self.amount + other.amount, self.unit)
        } else {
            assert!(
                !self.is_relative() && !other.is_relative(),
                "Cannot do arithmetic on relative measurements without absolutizing"
            );
            Measurement::pt(self.to_pt_abs() + other.to_pt_abs())
        }
    }
}

impl std::ops::Sub for Measurement {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        if self.unit == other.unit {
            Measurement::new(self.amount - other.amount, self.unit)
        } else {
            assert!(
                !self.is_relative() && !other.is_relative(),
                "Cannot do arithmetic on relative measurements without absolutizing"
            );
            Measurement::pt(self.to_pt_abs() - other.to_pt_abs())
        }
    }
}

impl std::ops::AddAssign for Measurement {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl std::ops::SubAssign for Measurement {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl std::ops::Mul<f64> for Measurement {
    type Output = Self;
    fn mul(self, scalar: f64) -> Self {
        Measurement::new(self.amount * scalar, self.unit)
    }
}

impl std::ops::Mul<Measurement> for f64 {
    type Output = Measurement;
    fn mul(self, m: Measurement) -> Measurement {
        Measurement::new(self * m.amount, m.unit)
    }
}

impl std::ops::Div<f64> for Measurement {
    type Output = Self;
    fn div(self, scalar: f64) -> Self {
        Measurement::new(self.amount / scalar, self.unit)
    }
}

impl std::ops::Rem<f64> for Measurement {
    type Output = Self;
    fn rem(self, scalar: f64) -> Self {
        Measurement::new(self.amount % scalar, self.unit)
    }
}

impl PartialEq for Measurement {
    fn eq(&self, other: &Self) -> bool {
        match (self.to_pt(), other.to_pt()) {
            (Some(a), Some(b)) => a == b,
            _ => self.unit == other.unit && self.amount == other.amount,
        }
    }
}

impl PartialOrd for Measurement {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self.to_pt(), other.to_pt()) {
            (Some(a), Some(b)) => a.partial_cmp(&b),
            _ => {
                if self.unit == other.unit {
                    self.amount.partial_cmp(&other.amount)
                } else {
                    None
                }
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn measurement_pt_explicit() {
        let m = Measurement::new(20.0, Unit::Pt);
        assert_eq!(m.to_pt(), Some(20.0));
        assert_eq!(m.to_string(), "20pt");
    }

    #[test]
    fn measurement_pt_implicit() {
        let m = Measurement::from_str("20pt").unwrap();
        assert_eq!(m.to_pt(), Some(20.0));
        assert_eq!(m.amount, 20.0);
        assert_eq!(m.unit, Unit::Pt);
    }

    #[test]
    fn measurement_inch() {
        let m = Measurement::from_str("0.2in").unwrap();
        // 0.2 * 72 = 14.4
        let pts = m.to_pt().unwrap();
        assert!((pts - 14.4).abs() < 1e-9, "expected 14.4 got {pts}");
    }

    #[test]
    fn measurement_display_decimal() {
        let m = Measurement::new(1.5, Unit::Mm);
        assert_eq!(m.to_string(), "1.5mm");
    }

    #[test]
    fn measurement_display_integer() {
        let m = Measurement::new(10.0, Unit::Em);
        assert_eq!(m.to_string(), "10em");
    }

    #[test]
    fn measurement_add_same_unit() {
        let a = Measurement::new(3.0, Unit::Pt);
        let b = Measurement::new(2.0, Unit::Pt);
        let c = a + b;
        assert_eq!(c.unit, Unit::Pt);
        assert_eq!(c.amount, 5.0);
    }

    #[test]
    fn measurement_add_different_units() {
        // 1in = 72pt, 1pt = 1pt → sum = 73pt
        let a = Measurement::new(1.0, Unit::In);
        let b = Measurement::new(1.0, Unit::Pt);
        let c = a + b;
        assert_eq!(c.unit, Unit::Pt);
        assert!((c.amount - 73.0).abs() < 1e-9);
    }

    #[test]
    fn measurement_neg() {
        let m = Measurement::new(5.0, Unit::Mm);
        let n = -m;
        assert_eq!(n.amount, -5.0);
        assert_eq!(n.unit, Unit::Mm);
    }

    #[test]
    fn measurement_mul_scalar() {
        let m = Measurement::new(4.0, Unit::Em);
        let r = m * 2.5;
        assert_eq!(r.amount, 10.0);
        assert_eq!(r.unit, Unit::Em);
    }

    #[test]
    fn measurement_eq_cross_unit() {
        // 72pt == 1in
        let a = Measurement::new(72.0, Unit::Pt);
        let b = Measurement::new(1.0, Unit::In);
        assert_eq!(a, b);
    }

    #[test]
    fn unit_is_relative() {
        assert!(!Unit::Pt.is_relative());
        assert!(!Unit::Mm.is_relative());
        assert!(Unit::Em.is_relative());
        assert!(Unit::PercentFw.is_relative());
    }

    #[test]
    fn measurement_parse_plain_number() {
        let m = Measurement::from_str("20").unwrap();
        assert_eq!(m.unit, Unit::Pt);
        assert_eq!(m.amount, 20.0);
    }
}
