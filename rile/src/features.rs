//! OpenType features by fontspec-style names, such as `Letters=SmallCaps`
//! for `+smcp`.
// SILE: `features` package.

use std::collections::BTreeMap;

/// A set of features turned on or off, written `+smcp;-liga`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OtFeatures(BTreeMap<String, bool>);

impl OtFeatures {
    /// Read a feature string such as `+smcp;-liga` or `+dlig,+hlig`.
    pub fn parse(features: &str) -> Self {
        let mut out = Self::default();
        for tag in features.split([',', ';', ':']).map(str::trim).filter(|t| !t.is_empty()) {
            let (on, name) = match tag.split_at(1) {
                ("-", name) => (false, name),
                ("+", name) => (true, name),
                _ => (true, tag),
            };
            let name = name.split('=').next().unwrap_or(name);
            out.0.insert(name.to_string(), on);
        }
        out
    }

    /// Turn on (or off, if `invert`) the features `name=value` stands for.
    /// `value` may list several, as in `{Historic, Discretionary}`, and a
    /// value starting `No` turns its feature off.
    pub fn load_option(&mut self, name: &str, value: &str, invert: bool) -> Result<(), String> {
        let values: Vec<&str> = match value.trim().strip_prefix('{').and_then(|v| v.strip_suffix('}')) {
            Some(list) => list.split(',').map(str::trim).collect(),
            None => vec![value.trim()],
        };
        let mut on = !invert;
        for value in values {
            let value = match value.strip_prefix("No") {
                Some(rest) => {
                    on = false;
                    rest
                }
                None => value,
            };
            let tag = feature_tag(name, value).ok_or_else(|| format!("bad OpenType feature {name}={value}"))?;
            self.0.insert(tag, on);
        }
        Ok(())
    }
}

impl std::fmt::Display for OtFeatures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tags: Vec<String> = self.0.iter().map(|(tag, on)| format!("{}{tag}", if *on { '+' } else { '-' })).collect();
        f.write_str(&tags.join(";"))
    }
}

fn feature_tag(name: &str, value: &str) -> Option<String> {
    let numbered = |prefix: &str| value.parse::<u32>().ok().map(|n| format!("{prefix}{n:02}"));
    let tag = match (name, value) {
        ("StylisticSet", _) => return numbered("ss"),
        ("CharacterVariant", _) => return numbered("cv"),
        ("Ligatures", "Required") => "rlig",
        ("Ligatures", "Common") => "liga",
        ("Ligatures", "Contextual") => "clig",
        ("Ligatures", "Rare" | "Discretionary") => "dlig",
        ("Ligatures", "Historic") => "hlig",
        ("Fractions", "On") => "frac",
        ("Fractions", "Alternate") => "afrc",
        ("Letters", "Uppercase") => "case",
        ("Letters", "SmallCaps") => "smcp",
        ("Letters", "PetiteCaps") => "pcap",
        ("Letters", "UppercaseSmallCaps") => "c2sc",
        ("Letters", "UppercasePetiteCaps") => "c2pc",
        ("Letters", "Unicase") => "unic",
        ("Numbers", "Uppercase" | "Lining") => "lnum",
        ("Numbers", "LowerCase" | "OldStyle") => "onum",
        ("Numbers", "Proportional") => "pnum",
        ("Numbers", "monospaced") => "tnum",
        ("Numbers", "SlashedZero") => "zero",
        ("Numbers", "Arabic") => "anum",
        ("Contextuals", "Swash") => "cswh",
        ("Contextuals", "Alternate") => "calt",
        ("Contextuals", "WordInitial") => "init",
        ("Contextuals", "WordFinal") => "fina",
        ("Contextuals", "LineFinal") => "fault",
        ("Contextuals", "Inner") => "medi",
        ("VerticalPosition", "Superior") => "sups",
        ("VerticalPosition", "Inferior") => "subs",
        ("VerticalPosition", "Numerator") => "numr",
        ("VerticalPosition", "Denominator") => "dnom",
        ("VerticalPosition", "ScientificInferior") => "sinf",
        ("VerticalPosition", "Ordinal") => "ordn",
        ("Style", "Alternate") => "salt",
        ("Style", "Italic") => "ital",
        ("Style", "Ruby") => "ruby",
        ("Style", "Swash") => "swsh",
        ("Style", "Historic") => "hist",
        ("Style", "TitlingCaps") => "titl",
        ("Style", "HorizontalKana") => "hkna",
        ("Style", "VerticalKana") => "vkna",
        ("Diacritics", "MarkToBase") => "mark",
        ("Diacritics", "MarkToMark") => "mkmk",
        ("Diacritics", "AboveBase") => "abvm",
        ("Diacritics", "BelowBase") => "blwm",
        ("Kerning", "Uppercase") => "cpsp",
        ("Kerning", "On") => "kern",
        ("CJKShape", "Traditional") => "trad",
        ("CJKShape", "Simplified") => "smpl",
        ("CJKShape", "JIS1978") => "jp78",
        ("CJKShape", "JIS1983") => "jp83",
        ("CJKShape", "JIS1990") => "jp90",
        ("CJKShape", "Expert") => "expt",
        ("CJKShape", "NLC") => "nlck",
        ("CharacterWidth", "Proportional") => "pwid",
        ("CharacterWidth", "Full") => "fwid",
        ("CharacterWidth", "Half") => "hwid",
        ("CharacterWidth", "Third") => "twid",
        ("CharacterWidth", "Quarter") => "qwid",
        ("CharacterWidth", "AlternateProportional") => "palt",
        ("CharacterWidth", "AlternateHalf") => "halt",
        _ => return None,
    };
    Some(tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_tags_and_no_turns_off() {
        let mut f = OtFeatures::parse("-smcp");
        f.load_option("Ligatures", "{ Historic, Discretionary }", false).unwrap();
        assert_eq!(f.to_string(), "+dlig;+hlig;-smcp");
        let mut f = OtFeatures::default();
        f.load_option("CharacterVariant", "{No75, 81}", false).unwrap();
        assert_eq!(f.to_string(), "-cv75;-cv81");
    }
}
