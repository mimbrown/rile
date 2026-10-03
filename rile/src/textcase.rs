//! Language-aware case mapping.
// SILE: `textcase` package, which uses ICU.

fn turkic(lang: &str) -> bool {
    matches!(lang.split(['-', '_']).next(), Some("tr" | "az"))
}

pub fn uppercase(text: &str, lang: &str) -> String {
    if !turkic(lang) {
        return text.to_uppercase();
    }
    text.chars().map(|c| if c == 'i' { "İ".to_string() } else { c.to_uppercase().collect() }).collect()
}

pub fn lowercase(text: &str, lang: &str) -> String {
    if !turkic(lang) {
        return text.to_lowercase();
    }
    let dotless = text.replace('I', "ı").replace('İ', "i");
    dotless.to_lowercase()
}

/// Each word's first letter in title case, the rest in lower case.
pub fn titlecase(text: &str, lang: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push_str(&uppercase(&first.to_string(), lang));
            out.push_str(&lowercase(chars.as_str(), lang));
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TR: &str = "ibraNİLER denemesi ılIk çeşme";

    #[test]
    fn english_uses_the_default_mappings() {
        assert_eq!(uppercase("teSTIng string", "en"), "TESTING STRING");
        assert_eq!(lowercase("teSTIng string", "en"), "testing string");
        assert_eq!(titlecase("teSTIng string", "en"), "Testing String");
    }

    #[test]
    fn turkish_keeps_dotted_and_dotless_i_apart() {
        assert_eq!(uppercase(TR, "tr"), "İBRANİLER DENEMESİ ILIK ÇEŞME");
        assert_eq!(lowercase(TR, "tr"), "ibraniler denemesi ılık çeşme");
        assert_eq!(titlecase(TR, "tr"), "İbraniler Denemesi Ilık Çeşme");
    }
}
