//! Tests whose outcome is settled rather than open work: differences that
//! come from SILE's expected output, tests SILE itself marks bad, and the
//! features deliberately left out.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    /// Nothing to do on our side; the reason says why.
    Explained(&'static str),
    /// Out of scope by decision.
    Skipped(&'static str),
}

impl Settled {
    pub fn label(self) -> &'static str {
        match self {
            Settled::Explained(_) => "explained",
            Settled::Skipped(_) => "skipped",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Settled::Explained(r) | Settled::Skipped(r) => r,
        }
    }
}

/// Why a test that doesn't match SILE needs no change in rile.
pub fn divergence(test: &str) -> Option<&'static str> {
    Some(match test {
        "alignment" => {
            "SILE's test fonts include Libertinus Serif Regular but not the Italic, so the expected italics come from an older Libertinus installed where it was recorded (other advances, other ligature glyph ids). Roman text matches."
        }
        "content-detection-sil" | "content-detection-xml" => {
            "The expected output still uses Gentium Plus, SILE's default font before 0.15.14; current SILE and rile use Gentium Book. Glyphs and breaks match."
        }
        "math-bigops" | "math-left-right-tex" | "math-stretchy" | "math-unary-binary-minus" => {
            "SILE measures size variants of big operators and stretchy delimiters at the math font size cut to whole points (an unsigned int cast in its HarfBuzz glue), so at the default 10.5851pt they come out about 6% narrower and shorter. rile uses the true size; with SILE's truncation these tests match."
        }
        "feat-math-display-unnumbered" => {
            "SILE resolves the ex in math.displayskip against the font in force when the page is output, here the 11pt document font rather than the 10.45pt blockquote font around the formulas. rile uses the font around the formula; with SILE's font the test matches."
        }
        _ => return None,
    })
}

/// Why a test that can't run needs no work, or that it is out of scope.
pub fn unsupported(missing: &[String]) -> Option<Settled> {
    if missing.iter().any(|m| m.contains("KNOWNBAD")) {
        Some(Settled::Explained("SILE marks this test KNOWNBAD: its expected output records a bug that is still open upstream."))
    } else if missing.iter().any(|m| m == "Lua input") {
        Some(Settled::Explained("The document is a Lua table, and rile has no Lua."))
    } else {
        None
    }
}

/// The source file for an expectation whose name SILE's repository misspells.
pub fn source_name(test: &str) -> &str {
    match test {
        "turkish-hypenation-options" => "turkish-hyphenation-options",
        t => t,
    }
}
