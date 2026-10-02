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

/// Why a test that doesn't match SILE needs no change in sile-rust.
pub fn divergence(test: &str) -> Option<&'static str> {
    Some(match test {
        "alignment" => {
            "SILE's test fonts include Libertinus Serif Regular but not the Italic, so the expected italics come from an older Libertinus installed where it was recorded (other advances, other ligature glyph ids). Roman text matches."
        }
        "content-detection-sil" | "content-detection-xml" => {
            "The expected output still uses Gentium Plus, SILE's default font before 0.15.14; current SILE and sile-rust use Gentium Book. Glyphs and breaks match."
        }
        _ => return None,
    })
}

/// Why a test that can't run needs no work, or that it is out of scope.
pub fn unsupported(missing: &[String]) -> Option<Settled> {
    if missing.iter().any(|m| m.contains("KNOWNBAD")) {
        Some(Settled::Explained("SILE marks this test KNOWNBAD: its expected output records a bug that is still open upstream."))
    } else if missing.iter().any(|m| m == "Lua input") {
        Some(Settled::Explained("The document is a Lua table, and sile-rust has no Lua."))
    } else if missing.iter().any(|m| m.contains("math")) {
        Some(Settled::Skipped("Math is out of scope for now."))
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
