//! Localized strings from Fluent files. Only plain messages with
//! `{ $variable }` placeables are understood.

use std::collections::BTreeMap;

use crate::language_data;

/// Message `id` in `lang` with its variables filled from `args`. Falls back
/// to the primary language subtag, then to English.
pub fn message(lang: &str, id: &str, args: &[(&str, &str)]) -> Option<String> {
    Messages::default().message(lang, id, args)
}

/// The built-in messages, with a document's own added over them.
// SILE: `\ftl`.
#[derive(Debug, Clone, Default)]
pub struct Messages {
    added: BTreeMap<String, Vec<String>>,
}

impl Messages {
    /// Add the messages in Fluent source `ftl` to `lang`, over any it has.
    pub fn add(&mut self, lang: &str, ftl: &str) {
        self.added.entry(normalize(lang)).or_default().push(ftl.to_string());
    }

    pub fn message(&self, lang: &str, id: &str, args: &[(&str, &str)]) -> Option<String> {
        let normalized = normalize(lang);
        let primary = normalized.split('-').next().unwrap_or_default();
        [normalized.as_str(), primary, "en"]
            .into_iter()
            .find_map(|l| {
                let added = self.added.get(l).into_iter().flatten().rev().find_map(|ftl| lookup(ftl, id));
                added.or_else(|| language_data::messages(l).and_then(|ftl| lookup(ftl, id)))
            })
            .map(|value| fill(&value, args))
    }
}

fn normalize(lang: &str) -> String {
    lang.to_lowercase().replace('_', "-")
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
    fn added_messages_win() {
        let mut messages = Messages::default();
        messages.add("tr", "hello = { $name }’ya Selam!");
        assert_eq!(messages.message("tr", "hello", &[("name", "Dünya")]).as_deref(), Some("Dünya’ya Selam!"));
        assert_eq!(messages.message("en", "hello", &[("name", "World")]).as_deref(), Some("Hello <em>World</em>!"));
    }

    #[test]
    fn unknown_messages_are_none() {
        assert_eq!(message("en", "no-such-message", &[]), None);
    }
}
