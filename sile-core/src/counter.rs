//! Counters such as section numbers (SILE's `counters` package).

/// A counter with one value per level, shown as `1.2.3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultilevelCounter {
    pub values: Vec<i64>,
}

impl Default for MultilevelCounter {
    fn default() -> Self {
        Self { values: vec![0] }
    }
}

impl MultilevelCounter {
    /// Step the counter at `level` (counting from 1; the deepest level when
    /// `None`). Going deeper opens the new levels at 0 and the stepped one
    /// at 1; going shallower drops the deeper levels when `reset`.
    pub fn increment(&mut self, level: Option<usize>, reset: bool) {
        let level = level.unwrap_or(self.values.len()).max(1);
        if level > self.values.len() {
            self.values.resize(level - 1, 0);
            self.values.push(1);
        } else {
            self.values[level - 1] += 1;
            if reset {
                self.values.truncate(level);
            }
        }
    }

    /// Set the value at `level`, opening or dropping levels as `increment`
    /// does.
    pub fn set(&mut self, level: usize, value: i64) {
        let level = level.max(1);
        self.values.resize(level, 0);
        self.values[level - 1] = value;
    }

    /// The levels up to `level` (all of them when `None`), joined by dots.
    pub fn format(&self, level: Option<usize>) -> String {
        let max = level.unwrap_or(self.values.len()).min(self.values.len());
        self.values[..max].iter().map(i64::to_string).collect::<Vec<_>>().join(".")
    }
}

/// A page's number and the numbering system it is shown in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageNumber {
    pub value: i64,
    pub display: String,
}

impl PageNumber {
    pub fn arabic(value: i64) -> Self {
        Self { value, display: "arabic".into() }
    }
}

impl std::fmt::Display for PageNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match format_number(self.value, &self.display) {
            Some(s) => f.write_str(&s),
            None => write!(f, "{}", self.value),
        }
    }
}

/// `n` written in the numbering system `display` (SILE's
/// `SU.formatNumber`): `arabic`, `alpha`/`ALPHA`, `roman`/`ROMAN`, Greek
/// (`greklow`, `grek`), Japanese (`jpan`), or a decimal system with its own
/// digits such as `arabext` or `deva`.
pub fn format_number(n: i64, display: &str) -> Option<String> {
    Some(match display {
        "arabic" | "" => n.to_string(),
        "alpha" => alpha(n),
        "Alpha" | "ALPHA" => alpha(n).to_uppercase(),
        "roman" => roman(n).to_lowercase(),
        "Roman" | "ROMAN" => roman(n),
        "greklow" => greek(n, false),
        "grek" => greek(n, true),
        "jpan" => japanese(n),
        other => {
            let zero = decimal_zero(other)?;
            n.to_string()
                .chars()
                .map(|c| c.to_digit(10).and_then(|d| char::from_u32(zero + d)).unwrap_or(c))
                .collect()
        }
    })
}

fn decimal_zero(system: &str) -> Option<u32> {
    Some(match system {
        "arab" => 0x0660,
        "arabext" => 0x06F0,
        "deva" => 0x0966,
        "beng" => 0x09E6,
        "guru" => 0x0A66,
        "gujr" => 0x0AE6,
        "orya" => 0x0B66,
        "tamldec" => 0x0BE6,
        "telu" => 0x0C66,
        "knda" => 0x0CE6,
        "mlym" => 0x0D66,
        "thai" => 0x0E50,
        "laoo" => 0x0ED0,
        "tibt" => 0x0F20,
        "mymr" => 0x1040,
        "khmr" => 0x17E0,
        "mong" => 0x1810,
        "fullwide" => 0xFF10,
        _ => return None,
    })
}

/// Bijective base 26: a…z, aa…
fn alpha(mut n: i64) -> String {
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    out.iter().rev().collect()
}

fn roman(mut n: i64) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"),
        (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

/// Alphabetic Greek numerals, thousands marked with ͵ and the number closed
/// with a keraia, as ICU writes them.
fn greek(n: i64, upper: bool) -> String {
    const UNITS: [&str; 9] = ["α", "β", "γ", "δ", "ε", "ϛ", "ζ", "η", "θ"];
    const TENS: [&str; 9] = ["ι", "κ", "λ", "μ", "ν", "ξ", "ο", "π", "ϟ"];
    const HUNDREDS: [&str; 9] = ["ρ", "σ", "τ", "υ", "φ", "χ", "ψ", "ω", "ϡ"];
    let digit = |table: &[&'static str; 9], d: i64| if d > 0 { table[d as usize - 1] } else { "" };
    let mut out = String::new();
    if n >= 1000 {
        out.push('͵');
        out.push_str(digit(&UNITS, n / 1000 % 10));
    }
    out.push_str(digit(&HUNDREDS, n / 100 % 10));
    out.push_str(digit(&TENS, n / 10 % 10));
    out.push_str(digit(&UNITS, n % 10));
    if upper {
        out = out.to_uppercase();
    }
    out.push('´');
    out
}

/// Japanese numerals with 十, 百, 千, 万 and 億, leaving out 一 before the
/// first three.
fn japanese(n: i64) -> String {
    const DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    if n == 0 {
        return "〇".to_string();
    }
    let below_10000 = |n: i64| {
        let mut out = String::new();
        for (unit, mark) in [(1000, Some('千')), (100, Some('百')), (10, Some('十')), (1, None)] {
            let d = n / unit % 10;
            if d == 0 {
                continue;
            }
            if d > 1 || mark.is_none() {
                out.push(DIGITS[d as usize]);
            }
            out.extend(mark);
        }
        out
    };
    let mut out = String::new();
    for (unit, mark) in [(100_000_000, Some('億')), (10_000, Some('万')), (1, None)] {
        let part = n / unit % 10_000;
        if part > 0 {
            out.push_str(&below_10000(part));
            out.extend(mark);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_systems_follow_sile() {
        let f = |display| format_number(1234, display).unwrap();
        assert_eq!(f("arabic"), "1234");
        assert_eq!(f("alpha"), "aul");
        assert_eq!(f("ROMAN"), "MCCXXXIV");
        assert_eq!(f("roman"), "mccxxxiv");
        assert_eq!(f("greklow"), "͵ασλδ´");
        assert_eq!(f("jpan"), "千二百三十四");
        assert_eq!(f("arabext"), "۱۲۳۴");
        assert_eq!(format_number(26, "alpha").unwrap(), "z");
        assert_eq!(format_number(27, "alpha").unwrap(), "aa");
        assert_eq!(format_number(10_010, "jpan").unwrap(), "一万十");
        assert_eq!(format_number(1, "nonesuch"), None);
    }

    #[test]
    fn levels_open_and_close() {
        let mut c = MultilevelCounter::default();
        assert_eq!(c.format(None), "0");
        c.increment(Some(3), true);
        assert_eq!(c.format(Some(3)), "0.0.1");
        c.increment(Some(3), true);
        assert_eq!(c.format(None), "0.0.2");
        c.increment(Some(2), true);
        assert_eq!(c.format(None), "0.1");
        c.increment(Some(3), false);
        c.increment(Some(1), false);
        assert_eq!(c.format(None), "1.1.1");
    }
}
