//! Splits shaped text into words, spaces and break penalties, following
//! SILE's unicode node maker: UAX #14 line boundaries and UAX #29 word
//! boundaries decide where tokens end and where lines may break.

use std::ops::Range;

use unicode_linebreak::{BreakClass, BreakOpportunity, break_property, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

/// One shaped glyph in logical order: the text of its cluster (empty for all
/// but the last glyph of a cluster) and that cluster's byte offset.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    pub text: &'a str,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A run of items forming one nnode.
    Word(Range<usize>),
    /// Inter-word glue shaped from this item.
    Space(usize),
    /// U+00A0: a kern the width of a space.
    NonBreakingSpace,
    Penalty(i32),
    /// Discretionary that repeats the hyphen at the start of the next line.
    RepeatedHyphen,
    /// `document.letterspaceglue` between characters.
    LetterSpace,
    /// French space before or after punctuation and guillemets.
    PunctSpace(PunctSpace),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunctSpace {
    /// Before `; ! ?`: half a space, fixed.
    Thin,
    /// Before `:`: a full, flexible space.
    Colon,
    /// Inside guillemets.
    Guillemet,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NodeMakerOptions {
    /// Repeat an explicit hyphen at the start of the next line (Czech,
    /// Polish, Portuguese, ...).
    pub repeated_hyphen: bool,
    /// End tokens after apostrophes and quotes (French, Catalan).
    pub split_quotes: bool,
    /// French spacing around punctuation and guillemets.
    pub french: bool,
    /// Treat U+00A0 as an ordinary character instead of a space-wide kern.
    pub fixed_nbsp: bool,
    pub obey_spaces: bool,
    pub letterspace: bool,
    /// Ethiopic word separators break and stretch (Amharic).
    pub ethiopic: bool,
    /// Space on both sides of Ethiopic separators, not just after.
    pub ethiopic_centered: bool,
}

impl NodeMakerOptions {
    pub fn for_language(lang: &str) -> Self {
        let base = lang.split(['-', '_']).next().unwrap_or(lang);
        Self {
            repeated_hyphen: matches!(base, "cs" | "es" | "gl" | "hr" | "pl" | "pt" | "sk"),
            split_quotes: matches!(base, "fr" | "ca"),
            french: base == "fr",
            ethiopic: base == "am",
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Boundary {
    Word,
    Line { hard: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Last {
    Nothing,
    Word,
    Glue,
    Penalty,
    Discretionary,
}

pub fn tokenize(items: &[Item], options: NodeMakerOptions) -> Vec<Token> {
    // French drops typed spaces where it sets its own.
    let (mut kept, mut clean, mut text, mut removed) = (Vec::new(), Vec::new(), String::new(), 0);
    for (i, item) in items.iter().enumerate() {
        if options.french && french::must_remove(items, i) {
            removed += item.text.len();
            continue;
        }
        kept.push(i);
        text.push_str(item.text);
        clean.push(Item { text: item.text, index: item.index - removed });
    }

    let mut maker = Maker {
        options,
        items: &clean,
        out: Vec::new(),
        token: None,
        last: Last::Nothing,
        last_class: None,
    };
    if options.ethiopic {
        maker.ethiopic();
        return maker.out;
    }
    let boundaries = boundaries(&text);
    let mut next = 0;
    let first = clean.iter().take_while(|i| is_space(i)).count();
    for i in 0..first {
        maker.glue(i);
    }
    for (i, item) in clean.iter().enumerate().skip(first) {
        match boundaries.get(next) {
            Some(&(at, kind)) if item.index >= at => {
                while boundaries.get(next).is_some_and(|&(at, _)| item.index >= at) {
                    next += 1;
                }
                match kind {
                    Boundary::Word => maker.word_break(i, item),
                    Boundary::Line { hard } => maker.line_break(i, item, hard),
                }
            }
            _ => maker.deal_with(i, item),
        }
    }
    maker.flush();
    maker
        .out
        .into_iter()
        .map(|t| match t {
            Token::Word(r) => Token::Word(kept[r.start]..kept[r.end - 1] + 1),
            Token::Space(i) => Token::Space(kept[i]),
            other => other,
        })
        .collect()
}

/// Every boundary after the start of `text`, line boundaries winning over
/// word boundaries at the same offset.
fn boundaries(text: &str) -> Vec<(usize, Boundary)> {
    let mut out: Vec<(usize, Boundary)> = text
        .split_word_bound_indices()
        .map(|(i, _)| (i, Boundary::Word))
        .chain(std::iter::once((text.len(), Boundary::Word)))
        .filter(|(i, _)| *i > 0)
        .collect();
    out.extend(
        linebreaks(text).map(|(i, op)| (i, Boundary::Line { hard: op == BreakOpportunity::Mandatory })),
    );
    out.sort_by_key(|&(i, b)| (i, b == Boundary::Word));
    out.dedup_by_key(|(i, _)| *i);
    out
}

fn class(item: &Item) -> Option<BreakClass> {
    item.text.chars().next().map(|c| break_property(c as u32))
}

fn is_space(item: &Item) -> bool {
    class(item) == Some(BreakClass::Space)
}

fn is_nbsp(item: &Item) -> bool {
    item.text.starts_with('\u{00A0}')
}

mod french {
    use super::{Item, is_nbsp, is_space};

    const HIGH: &[&str] = &[";", "!", "?", "!!", "?!", "!?"];
    const COLON: &[&str] = &[":"];
    const OPENING: &[&str] = &["«", "‹"];
    const CLOSING: &[&str] = &["»", "›"];
    const NO_SPACE_AFTER: &[&str] = &["!", "?", ":", ".", "…", "(", "[", "{", "<", "«", "‹", "“", "‘", "?!", "!!", "!?"];

    pub fn is_high(t: &str) -> bool {
        HIGH.contains(&t)
    }
    pub fn is_colon(t: &str) -> bool {
        COLON.contains(&t)
    }
    pub fn is_opening(t: &str) -> bool {
        OPENING.contains(&t)
    }
    pub fn is_closing(t: &str) -> bool {
        CLOSING.contains(&t)
    }
    pub fn is_space_exception(t: &str) -> bool {
        NO_SPACE_AFTER.contains(&t)
    }

    pub fn must_remove(items: &[Item], i: usize) -> bool {
        let curr = &items[i];
        if !(is_space(curr) || is_nbsp(curr)) {
            return false;
        }
        let next_sets_space = items.get(i + 1).is_some_and(|n| {
            is_space(n) || is_nbsp(n) || is_high(n.text) || is_colon(n.text) || is_closing(n.text)
        });
        next_sets_space || (i > 0 && is_opening(items[i - 1].text))
    }
}

struct Maker<'a> {
    options: NodeMakerOptions,
    items: &'a [Item<'a>],
    out: Vec<Token>,
    token: Option<Range<usize>>,
    last: Last,
    last_class: Option<BreakClass>,
}

impl Maker<'_> {
    fn is_active_nbsp(&self, item: &Item) -> bool {
        is_nbsp(item) && !self.options.fixed_nbsp
    }

    fn add(&mut self, i: usize) {
        match &mut self.token {
            Some(r) => r.end = i + 1,
            None => self.token = Some(i..i + 1),
        }
    }

    fn flush(&mut self) {
        if let Some(r) = self.token.take() {
            self.out.push(Token::Word(r));
            self.last = Last::Word;
        }
    }

    fn glue(&mut self, i: usize) {
        if self.options.obey_spaces || self.last != Last::Glue {
            self.out.push(Token::Space(i));
        }
        self.last = Last::Glue;
        self.last_class = Some(BreakClass::Space);
    }

    fn penalty(&mut self, p: i32) {
        if !matches!(self.last, Last::Penalty | Last::Glue) {
            self.out.push(Token::Penalty(p));
        }
        self.last = Last::Penalty;
    }

    fn nbsp(&mut self) {
        self.out.push(Token::NonBreakingSpace);
        self.last = Last::Glue;
        self.last_class = Some(BreakClass::Space);
    }

    fn punct_space(&mut self, space: PunctSpace) {
        self.flush();
        self.last = Last::Glue;
        self.out.push(Token::PunctSpace(space));
    }

    fn letterspace(&mut self) {
        if !self.options.letterspace {
            return;
        }
        self.flush();
        if !matches!(self.last, Last::Nothing | Last::Glue) {
            self.out.push(Token::LetterSpace);
            self.last = Last::Glue;
            self.last_class = Some(BreakClass::Space);
        }
    }

    /// French spacing before high punctuation, colons and closing
    /// guillemets, and after opening guillemets.
    fn french_spacing(&mut self, i: usize, item: &Item) -> bool {
        if !self.options.french {
            return false;
        }
        let after_exception = i > 0 && french::is_space_exception(self.items[i - 1].text);
        let before = if french::is_high(item.text) && !after_exception {
            Some(PunctSpace::Thin)
        } else if french::is_colon(item.text) && !after_exception {
            Some(PunctSpace::Colon)
        } else if french::is_closing(item.text) {
            Some(PunctSpace::Guillemet)
        } else {
            None
        };
        if let Some(space) = before {
            self.punct_space(space);
            self.add(i);
            return true;
        }
        if french::is_opening(item.text) {
            self.add(i);
            self.punct_space(PunctSpace::Guillemet);
            return true;
        }
        false
    }

    fn deal_with(&mut self, i: usize, item: &Item) {
        if self.french_spacing(i, item) {
            return;
        }
        let this = class(item);
        if this == Some(BreakClass::Space) {
            self.flush();
            self.glue(i);
        } else if self.is_active_nbsp(item) {
            self.flush();
            self.nbsp();
        } else if matches!(this, Some(BreakClass::After | BreakClass::ZeroWidthSpace)) {
            self.add(i);
            self.flush();
            self.penalty(0);
        } else if self.options.split_quotes && this == Some(BreakClass::Quotation) {
            self.add(i);
            self.flush();
        } else if self.last_class.is_some() && this.is_some() && this != self.last_class {
            self.add(i);
        } else {
            self.letterspace();
            self.add(i);
        }
        self.last_class = this;
    }

    /// SILE's Amharic node maker: word space and full stop end a word and
    /// are followed by space, the full stop by a break as well.
    fn ethiopic(&mut self) {
        let items = self.items;
        for (i, item) in items.iter().enumerate() {
            let separator = match item.text.chars().next() {
                Some('\u{1361}') => Some(false),
                Some('\u{1362}') => Some(true),
                _ => None,
            };
            let Some(full_stop) = separator else {
                self.deal_with(i, item);
                continue;
            };
            if self.options.ethiopic_centered {
                self.flush();
                self.glue(i);
            }
            self.add(i);
            self.flush();
            self.glue(i);
            if full_stop {
                self.penalty(0);
                self.glue(i);
            }
        }
        self.flush();
    }

    fn word_break(&mut self, i: usize, item: &Item) {
        if self.french_spacing(i, item) {
            return;
        }
        if self.options.repeated_hyphen && item.text == "-" {
            self.add(i);
            self.flush();
            if self.last != Last::Discretionary {
                self.out.push(Token::RepeatedHyphen);
                self.last = Last::Discretionary;
            }
        } else {
            self.flush();
            if is_space(item) {
                self.glue(i);
            } else if self.is_active_nbsp(item) {
                self.nbsp();
            } else {
                self.add(i);
            }
        }
    }

    fn line_break(&mut self, i: usize, item: &Item, hard: bool) {
        if self.options.french && is_space(item) {
            return self.word_break(i, item);
        }
        if self.french_spacing(i, item) {
            return;
        }
        if self.last == Last::Discretionary {
            self.deal_with(i, item);
        } else if is_space(item) || self.is_active_nbsp(item) {
            self.word_break(i, item);
        } else {
            self.flush();
            self.penalty(if hard { -1000 } else { 0 });
            self.add(i);
            self.last_class = class(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(text: &str) -> Vec<Item<'_>> {
        text.char_indices()
            .map(|(i, c)| Item { text: &text[i..i + c.len_utf8()], index: i })
            .collect()
    }

    fn render(text: &str, options: NodeMakerOptions) -> String {
        let items = items(text);
        tokenize(&items, options)
            .iter()
            .map(|t| match t {
                Token::Word(r) => format!("[{}]", items[r.clone()].iter().map(|i| i.text).collect::<String>()),
                Token::Space(_) => "_".into(),
                Token::NonBreakingSpace => "~".into(),
                Token::Penalty(p) => format!("P{p}"),
                Token::RepeatedHyphen => "D".into(),
                Token::LetterSpace => "k".into(),
                Token::PunctSpace(PunctSpace::Thin) => "t".into(),
                Token::PunctSpace(PunctSpace::Colon) => "c".into(),
                Token::PunctSpace(PunctSpace::Guillemet) => "g".into(),
            })
            .collect()
    }

    #[test]
    fn ethiopic_separators_end_words() {
        let am = NodeMakerOptions::for_language("am");
        assert_eq!(render("ሰው፡ልጅ።ሁሉ", am), "[ሰው፡]_[ልጅ።]__[ሁሉ]");
        let centered = NodeMakerOptions { ethiopic_centered: true, ..am };
        assert_eq!(render("ሰው፡ልጅ", centered), "[ሰው]_[፡]_[ልጅ]");
    }

    #[test]
    fn punctuation_is_its_own_token() {
        assert_eq!(render("Hello, world.", Default::default()), "[Hello][,]_[world][.]");
    }

    #[test]
    fn breaks_after_hyphens_and_dashes() {
        assert_eq!(render("biało-czerwony", Default::default()), "[biało][-]P0[czerwony]");
        assert_eq!(render("a—b", Default::default()), "[a]P0[—]P0[b]");
    }

    #[test]
    fn repeated_hyphen_languages_get_a_discretionary() {
        let pl = NodeMakerOptions::for_language("pl");
        assert_eq!(render("biało-czerwony", pl), "[biało-]D[czerwony]");
    }

    #[test]
    fn nbsp_is_a_kern_unless_fixed() {
        assert_eq!(render("a\u{a0}b", Default::default()), "[a]~[b]");
        let fixed = NodeMakerOptions { fixed_nbsp: true, ..Default::default() };
        assert_eq!(render("a\u{a0}b", fixed), "[a][\u{a0}][b]");
    }

    #[test]
    fn spaces_collapse_unless_obeyed() {
        assert_eq!(render("  a  b", Default::default()), "_[a]_[b]");
        let obey = NodeMakerOptions { obey_spaces: true, ..Default::default() };
        assert_eq!(render("a  b", obey), "[a]__[b]");
    }

    #[test]
    fn contractions_stay_whole() {
        assert_eq!(render("don't 3.14", Default::default()), "[don't]_[3.14]");
    }

    #[test]
    fn french_sets_its_own_punctuation_spaces() {
        let fr = NodeMakerOptions::for_language("fr");
        assert_eq!(render("Français ? « Oui » d’hommes", fr), "[Français]t[?]_[«]g[Oui]g[»]_[d’][hommes]");
        assert_eq!(render("Hein ?!", fr), "[Hein]t[?][!]");
    }
}
