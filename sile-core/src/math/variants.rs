//! Mathematical Alphanumeric Symbols for `mathvariant`, and spacing
//! equivalents of combining accents (SILE's `unicode-mathvariants.lua` and
//! `unicode-accents.lua`).

use super::MathVariant;
use MathVariant::*;

fn latin_upper(v: MathVariant, c: u32) -> Option<u32> {
    let offset = |base: u32| Some(c + base - 0x41);
    match v {
        Normal => Some(c),
        BoldScript => offset(0x1D4D0),
        BoldItalic => offset(0x1D468),
        Bold => offset(0x1D400),
        DoubleStruck => match c {
            0x43 => Some(0x2102),
            0x48 => Some(0x210D),
            0x4E => Some(0x2115),
            0x50 => Some(0x2119),
            0x51 => Some(0x211A),
            0x52 => Some(0x211D),
            0x5A => Some(0x2124),
            _ => offset(0x1D538),
        },
        Fraktur => match c {
            0x43 => Some(0x212D),
            0x48 => Some(0x210C),
            0x49 => Some(0x2111),
            0x52 => Some(0x211C),
            0x5A => Some(0x2128),
            _ => offset(0x1D504),
        },
        Script => match c {
            0x42 => Some(0x212C),
            0x45 => Some(0x2130),
            0x46 => Some(0x2131),
            0x48 => Some(0x210B),
            0x49 => Some(0x2110),
            0x4C => Some(0x2112),
            0x4D => Some(0x2133),
            0x52 => Some(0x211D),
            _ => offset(0x1D49C),
        },
        Monospace => offset(0x1D670),
        SansSerif => offset(0x1D5A0),
        Italic => offset(0x1D434),
        BoldFraktur => offset(0x1D56C),
        SansSerifBoldItalic => offset(0x1D63C),
        SansSerifItalic => offset(0x1D608),
        BoldSansSerif => offset(0x1D5D4),
    }
}

fn latin_lower(v: MathVariant, c: u32) -> Option<u32> {
    let offset = |base: u32| Some(c + base - 0x61);
    match v {
        Normal => Some(c),
        BoldScript => offset(0x1D4EA),
        BoldItalic => offset(0x1D482),
        Bold => offset(0x1D41A),
        DoubleStruck => offset(0x1D552),
        Fraktur => offset(0x1D51E),
        Script => match c {
            0x65 => Some(0x212F),
            0x67 => Some(0x210A),
            0x6F => Some(0x2134),
            _ => offset(0x1D4B6),
        },
        Monospace => offset(0x1D68A),
        SansSerif => offset(0x1D5BA),
        Italic if c == 0x68 => Some(0x210E),
        Italic => offset(0x1D44E),
        BoldFraktur => offset(0x1D586),
        SansSerifBoldItalic => offset(0x1D656),
        SansSerifItalic => offset(0x1D622),
        BoldSansSerif => offset(0x1D5EE),
    }
}

fn digit(v: MathVariant, c: u32) -> Option<u32> {
    let offset = |base: u32| Some(c + base - 0x30);
    match v {
        Normal => Some(c),
        Bold => offset(0x1D7CE),
        Monospace => offset(0x1D7F6),
        SansSerif => offset(0x1D7E2),
        DoubleStruck => offset(0x1D7D8),
        BoldSansSerif => offset(0x1D7EC),
        _ => None,
    }
}

fn greek_upper(v: MathVariant, c: u32) -> Option<u32> {
    let (theta, nabla, base) = match v {
        Normal => return Some(c),
        BoldItalic => (0x1D72D, 0x1D735, 0x1D71C),
        Bold => (0x1D6B9, 0x1D6C1, 0x1D6A8),
        Italic => (0x1D6F3, 0x1D6FB, 0x1D6E2),
        SansSerifBoldItalic => (0x1D7A1, 0x1D7A9, 0x1D790),
        BoldSansSerif => (0x1D767, 0x1D76F, 0x1D756),
        _ => return None,
    };
    Some(match c {
        0x3F4 => theta,
        0x2207 => nabla,
        _ => c + base - 0x391,
    })
}

fn greek_lower(v: MathVariant, c: u32) -> Option<u32> {
    // theta, phi, pi, kappa, rho and epsilon symbols, then the base
    let (variants, base): ([u32; 6], u32) = match v {
        Normal => return Some(c),
        BoldItalic => ([0x1D751, 0x1D753, 0x1D755, 0x1D752, 0x1D754, 0x1D750], 0x1D736),
        Bold => ([0x1D6DD, 0x1D6DF, 0x1D6E1, 0x1D6DE, 0x1D6E0, 0x1D6DC], 0x1D6C2),
        Italic => ([0x1D717, 0x1D719, 0x1D71B, 0x1D718, 0x1D71A, 0x1D716], 0x1D6FC),
        SansSerifBoldItalic => ([0x1D7C5, 0x1D7C7, 0x1D7C9, 0x1D7C6, 0x1D7C8, 0x1D7C4], 0x1D7AA),
        BoldSansSerif => ([0x1D78B, 0x1D78D, 0x1D78F, 0x1D78C, 0x1D78E, 0x1D78A], 0x1D770),
        _ => return None,
    };
    Some(match c {
        0x3D1 => variants[0],
        0x3D5 => variants[1],
        0x3D6 => variants[2],
        0x3F0 => variants[3],
        0x3F1 => variants[4],
        0x3F5 => variants[5],
        _ => c + base - 0x3B1,
    })
}

/// `text` with its letters and digits in `variant`.
pub(crate) fn convert(text: &str, variant: MathVariant) -> String {
    text.chars()
        .map(|ch| {
            let c = ch as u32;
            let converted = match c {
                0x41..=0x5A => latin_upper(variant, c),
                0x61..=0x7A => latin_lower(variant, c),
                0x30..=0x39 => digit(variant, c),
                0x391..=0x3A9 if c != 0x3A2 => greek_upper(variant, c),
                0x3F4 | 0x2207 => greek_upper(variant, c),
                0x3B1..=0x3C9 | 0x3D1 | 0x3D5 | 0x3D6 | 0x3F0 | 0x3F1 | 0x3F5 => greek_lower(variant, c),
                _ => None,
            };
            converted.and_then(char::from_u32).unwrap_or(ch)
        })
        .collect()
}

fn is_combining(c: u32) -> bool {
    matches!(c, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0x2DE0..=0x2DFF | 0xFE20..=0xFE2F)
}

const NON_COMBINING: &[(u32, u32)] = &[
    (0x0300, 0x0060),
    (0x0301, 0x00B4),
    (0x0302, 0x02C6),
    (0x0303, 0x007E),
    (0x0304, 0x00AF),
    (0x0305, 0x203E),
    (0x0306, 0x02D8),
    (0x0307, 0x02D9),
    (0x0308, 0x00A8),
    (0x030A, 0x02DA),
    (0x030B, 0x02DD),
    (0x030C, 0x02C7),
    (0x0312, 0x00B8),
    (0x0316, 0x0060),
    (0x0317, 0x00B4),
    (0x031F, 0x002B),
    (0x0320, 0x002D),
    (0x0323, 0x002E),
    (0x0324, 0x00A8),
    (0x0327, 0x00B8),
    (0x0328, 0x02DB),
    (0x032C, 0x02C7),
    (0x032D, 0x005E),
    (0x032E, 0x02D8),
    (0x0330, 0x007E),
    (0x0331, 0x00AF),
    (0x0332, 0x203E),
    (0x0333, 0x2017),
    (0x0338, 0x002F),
    (0x034D, 0x2194),
    (0x20D0, 0x21BC),
    (0x20D1, 0x21C0),
    (0x20D6, 0x2190),
    (0x20D7, 0x2192),
    (0x20DB, 0x22EF),
    (0x20E1, 0x2194),
    (0x20E8, 0x22EF),
    (0x20EC, 0x21C1),
    (0x20ED, 0x21BD),
    (0x20EE, 0x2190),
    (0x20EF, 0x2192),
];

/// A lone combining accent as its spacing equivalent, which fonts have
/// stretchy variants for.
pub(crate) fn make_non_combining(text: &str) -> String {
    let mut chars = text.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else { return text.to_string() };
    let c = ch as u32;
    if is_combining(c)
        && let Some((_, nc)) = NON_COMBINING.iter().find(|(from, _)| *from == c)
    {
        return char::from_u32(*nc).map_or_else(|| text.to_string(), String::from);
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_map_to_alphanumeric_symbols() {
        assert_eq!(convert("x", Italic), "𝑥");
        assert_eq!(convert("h", Italic), "ℎ");
        assert_eq!(convert("R", DoubleStruck), "ℝ");
        assert_eq!(convert("1+", Bold), "𝟏+");
        assert_eq!(convert("1", Italic), "1");
        assert_eq!(convert("α", Italic), "𝛼");
        assert_eq!(make_non_combining("\u{20D7}"), "→");
    }
}
