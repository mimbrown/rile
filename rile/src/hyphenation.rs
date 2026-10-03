//! Liang hyphenation, with patterns for many languages built in.
// SILE: `hyphenator-liang`.

use std::collections::HashMap;

use crate::language_data;

#[derive(Default)]
struct TrieNode {
    children: HashMap<char, usize>,
    points: Option<Vec<u8>>,
}

struct Patterns {
    trie: Vec<TrieNode>,
    exceptions: HashMap<String, Vec<bool>>,
    left_min: usize,
    right_min: usize,
}

impl Patterns {
    fn empty() -> Self {
        Self { trie: vec![TrieNode::default()], exceptions: HashMap::new(), left_min: 2, right_min: 2 }
    }

    fn load(lang: &str) -> Option<Self> {
        let source = language_data::hyphenation(lang)?;
        let mut patterns = Self::empty();
        let mut section = "";
        for line in source.lines() {
            match line.split_once(' ') {
                Some(("from", base)) => patterns = Self::load(base)?,
                Some(("hyphenmins", mins)) => {
                    let mut mins = mins.split(' ').filter_map(|m| m.parse().ok());
                    patterns.left_min = mins.next().unwrap_or(2);
                    patterns.right_min = mins.next().unwrap_or(2);
                }
                _ if line == "patterns" || line == "exceptions" => section = line,
                _ if section == "patterns" => patterns.add_pattern(line),
                _ if section == "exceptions" => patterns.add_exception(line),
                _ => {}
            }
        }
        Some(patterns)
    }

    fn add_pattern(&mut self, pattern: &str) {
        let mut node = 0;
        for c in pattern.chars().filter(|c| !c.is_ascii_digit()) {
            node = match self.trie[node].children.get(&c) {
                Some(&next) => next,
                None => {
                    self.trie.push(TrieNode::default());
                    let next = self.trie.len() - 1;
                    self.trie[node].children.insert(c, next);
                    next
                }
            };
        }
        let mut points = Vec::new();
        let mut last_was_digit = false;
        for c in pattern.chars() {
            if let Some(d) = c.to_digit(10) {
                last_was_digit = true;
                points.push(d as u8);
            } else if last_was_digit {
                last_was_digit = false;
            } else {
                points.push(0);
            }
        }
        self.trie[node].points = Some(points);
    }

    fn add_exception(&mut self, exception: &str) {
        let mut breaks = Vec::new();
        for c in exception.chars() {
            if c == '-' {
                if let Some(last) = breaks.last_mut() {
                    *last = true;
                }
            } else {
                breaks.push(false);
            }
        }
        self.exceptions.insert(lowercase(&exception.replace('-', "")), breaks);
    }

    /// Whether to break after each character of `word`.
    fn breaks(&self, word: &[char]) -> Vec<bool> {
        if let Some(breaks) = self.exceptions.get(&word.iter().copied().map(lower).collect::<String>()) {
            return breaks.clone();
        }
        let mut work = Vec::with_capacity(word.len() + 2);
        work.push('.');
        work.extend(word.iter().copied().map(lower));
        work.push('.');
        // points[m] is the value before character m (1-based), points[n + 1] after the last.
        let mut points = vec![0u8; word.len() + 1];
        for i in 0..work.len() {
            let mut node = 0;
            for &c in &work[i..] {
                let Some(&next) = self.trie[node].children.get(&c) else { break };
                node = next;
                if let Some(p) = &self.trie[node].points {
                    for (k, &value) in p.iter().enumerate() {
                        if let Some(slot) = (i + k).checked_sub(1).and_then(|idx| points.get_mut(idx))
                            && *slot < value
                        {
                            *slot = value;
                        }
                    }
                }
            }
        }
        let len = points.len();
        points[..self.left_min.min(len)].fill(0);
        points[len.saturating_sub(self.right_min)..].fill(0);
        (1..=word.len()).map(|i| points.get(i).is_some_and(|p| p % 2 == 1)).collect()
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn lowercase(s: &str) -> String {
    s.chars().map(lower).collect()
}

pub struct HyphenationDictionary {
    languages: HashMap<String, Patterns>,
    /// Words shorter than this, in characters, are never hyphenated.
    pub min_word: usize,
}

impl Default for HyphenationDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl HyphenationDictionary {
    pub fn new() -> Self {
        Self { languages: HashMap::new(), min_word: 5 }
    }

    /// Load the patterns for `lang`, falling back to its primary subtag
    /// (`en-US` uses `en`). Returns whether any were found.
    pub fn load_language(&mut self, lang: &str) -> bool {
        self.patterns(lang);
        self.languages.get(lang).is_some_and(|p| p.trie.len() > 1 || !p.exceptions.is_empty())
    }

    fn patterns(&mut self, lang: &str) -> &mut Patterns {
        self.languages.entry(lang.to_string()).or_insert_with(|| {
            let normalized = lang.to_lowercase().replace('_', "-");
            Patterns::load(&normalized)
                .or_else(|| Patterns::load(normalized.split('-').next().unwrap_or_default()))
                .unwrap_or_else(Patterns::empty)
        })
    }

    /// Add words with their hyphenation points marked by `-`.
    // SILE: `hyphenator:add-exceptions`.
    pub fn add_exceptions<'a>(&mut self, lang: &str, words: impl IntoIterator<Item = &'a str>) {
        let patterns = self.patterns(lang);
        for word in words {
            patterns.add_exception(word);
        }
    }

    /// Split `word` at its hyphenation points.
    pub fn hyphenate_word(&mut self, word: &str, lang: &str) -> Vec<String> {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < self.min_word {
            return vec![word.to_string()];
        }
        let breaks = self.patterns(lang).breaks(&chars);
        let mut pieces = vec![String::new()];
        for (i, &c) in chars.iter().enumerate() {
            pieces.last_mut().unwrap().push(c);
            if breaks.get(i).copied().unwrap_or(false) && i + 1 < chars.len() {
                pieces.push(String::new());
            }
        }
        pieces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(word: &str, lang: &str) -> String {
        HyphenationDictionary::new().hyphenate_word(word, lang).join("-")
    }

    #[test]
    fn english_follows_sile() {
        assert_eq!(show("hyphenation", "en"), "hy-phen-ation");
        assert_eq!(show("labore", "en"), "la-bore");
        assert_eq!(show("sadipscing", "en"), "sadip-sc-ing");
        assert_eq!(show("table", "en"), "ta-ble");
        assert_eq!(show("Table", "en"), "Ta-ble");
        assert_eq!(show("the", "en"), "the");
    }

    #[test]
    fn minima_come_from_the_pattern_file() {
        let mut dict = HyphenationDictionary::new();
        let mins = |p: &Patterns| (p.left_min, p.right_min);
        assert_eq!(mins(dict.patterns("en")), (2, 3));
        assert_eq!(mins(dict.patterns("de")), (2, 2));
        assert_eq!(show("Donaudampfschifffahrt", "de"), "Do-nau-dampf-schiff-fahrt");
    }

    #[test]
    fn region_subtags_fall_back_to_the_language() {
        assert_eq!(show("hyphenation", "en-US"), "hy-phen-ation");
        assert_eq!(show("something", "xx-unknown"), "something");
    }

    #[test]
    fn norwegian_variants_share_patterns() {
        let mut dict = HyphenationDictionary::new();
        assert!(dict.load_language("nb"));
        assert!(dict.load_language("nn"));
        assert_eq!(dict.hyphenate_word("attende", "nn").join("-"), "att-en-de");
    }

    #[test]
    fn french_from_sile_spec() {
        assert_eq!(show("série", "fr"), "sé-rie");
        assert_eq!(show("Légèrement", "fr"), "Lé-gè-re-ment");
        let mut dict = HyphenationDictionary::new();
        dict.add_exceptions("fr", ["légè-rement"]);
        assert_eq!(dict.hyphenate_word("Légèrement", "fr").join("-"), "Légè-rement");
    }

    #[test]
    fn exceptions_override_patterns() {
        let mut dict = HyphenationDictionary::new();
        dict.add_exceptions("en", ["hy-phenation"]);
        assert_eq!(dict.hyphenate_word("Hyphenation", "en").join("-"), "Hy-phenation");
    }

    #[test]
    fn segments_rejoin_to_the_word() {
        let mut dict = HyphenationDictionary::new();
        for word in ["international", "extraordinary", "communication", "responsibility"] {
            assert_eq!(dict.hyphenate_word(word, "en").concat(), word);
        }
    }
}
