//! Line breaks in URLs (SILE's `url` package).

use crate::builder::Typesetter;

/// A stretch of a URL, or a break point within it.
#[derive(Debug, Clone, PartialEq)]
pub enum UrlPiece<'a> {
    Text(&'a str),
    Penalty(i32),
}

/// Penalties for breaking at preferred break points (after path and query
/// separators) and tolerable ones (before them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UrlPenalties {
    pub primary: i32,
    pub secondary: i32,
}

impl Default for UrlPenalties {
    fn default() -> Self {
        Self { primary: 100, secondary: 200 }
    }
}

const BREAK_BEFORE: &str = "%#";
const BREAK_AFTER: &str = ":/.;?&=!_-";

/// Split `url` at the places it may break: after the scheme's colon, before
/// escapes and fragments, and after other separators (or, worse, before).
pub fn url_pieces(url: &str, penalties: UrlPenalties) -> Vec<UrlPiece<'_>> {
    let UrlPenalties { primary, secondary } = penalties;
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in url.char_indices() {
        if !BREAK_BEFORE.contains(c) && !BREAK_AFTER.contains(c) {
            continue;
        }
        if start < i {
            out.push(UrlPiece::Text(&url[start..i]));
        }
        let separator = &url[i..i + c.len_utf8()];
        if BREAK_BEFORE.contains(c) {
            out.extend([UrlPiece::Penalty(primary), UrlPiece::Text(separator), UrlPiece::Penalty(primary + secondary)]);
        } else if c == ':' {
            out.extend([UrlPiece::Text(separator), UrlPiece::Penalty(primary)]);
        } else {
            out.extend([UrlPiece::Penalty(secondary), UrlPiece::Text(separator), UrlPiece::Penalty(primary)]);
        }
        start = i + c.len_utf8();
    }
    if start < url.len() {
        out.push(UrlPiece::Text(&url[start..]));
    }
    out
}

impl Typesetter {
    /// Add `url` with break points, unhyphenated, in the current font.
    pub fn add_url(&mut self, url: &str, penalties: UrlPenalties) -> &mut Self {
        let language = self.language().to_string();
        self.set_language("und");
        for piece in url_pieces(url, penalties) {
            match piece {
                UrlPiece::Text(text) => self.add_text(text),
                UrlPiece::Penalty(p) => self.add_penalty(p),
            };
        }
        self.set_language(language);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UrlPiece::*;

    #[test]
    fn breaks_after_the_scheme_and_around_separators() {
        let pieces = url_pieces("a://b.c#d", UrlPenalties::default());
        assert_eq!(
            pieces,
            [
                Text("a"), Text(":"), Penalty(100),
                Penalty(200), Text("/"), Penalty(100),
                Penalty(200), Text("/"), Penalty(100),
                Text("b"), Penalty(200), Text("."), Penalty(100),
                Text("c"), Penalty(100), Text("#"), Penalty(300),
                Text("d"),
            ]
        );
    }
}
