//! Localized strings from SILE's Fluent files. Only plain messages with
//! `{ $variable }` placeables are understood.

use crate::language_data;

/// Message `id` in `lang` with its variables filled from `args`. Falls back
/// to the primary language subtag, then to English.
pub fn message(lang: &str, id: &str, args: &[(&str, &str)]) -> Option<String> {
    let normalized = lang.to_lowercase().replace('_', "-");
    let primary = normalized.split('-').next().unwrap_or_default();
    [normalized.as_str(), primary, "en"]
        .into_iter()
        .find_map(|l| language_data::messages(l).and_then(|ftl| lookup(ftl, id)))
        .map(|value| fill(&value, args))
}

fn lookup(ftl: &str, id: &str) -> Option<String> {
    let mut lines = ftl.lines();
    let first = lines.by_ref().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim_end() == id && !line.starts_with(char::is_whitespace)).then(|| value.trim().to_string())
    })?;
    let continuation = lines.take_while(|l| l.starts_with(' ') && !l.trim().is_empty()).map(str::trim);
    Some(std::iter::once(first.as_str()).chain(continuation).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n"))
}

fn fill(value: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else { break };
        let placeable = rest[open + 1..open + close].trim();
        match placeable.strip_prefix('$').and_then(|name| args.iter().find(|(k, _)| *k == name)) {
            Some((_, v)) => out.push_str(v),
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_titles_follow_the_language() {
        let title = |lang| message(lang, "book-chapter-title", &[("number", "3")]);
        assert_eq!(title("en").as_deref(), Some("Chapter 3"));
        assert_eq!(title("tr").as_deref(), Some("Bölüm 3"));
        assert_eq!(title("cs").as_deref(), Some("Kapitola 3"));
        assert_eq!(title("ja").as_deref(), Some("第3章"));
        assert_eq!(title("en-GB").as_deref(), Some("Chapter 3"));
        assert_eq!(title("xx").as_deref(), Some("Chapter 3"));
    }

    #[test]
    fn unknown_messages_are_none() {
        assert_eq!(message("en", "no-such-message", &[]), None);
    }
}
