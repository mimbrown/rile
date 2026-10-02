//! The operator dictionary (`math/operators.txt`, from SILE's
//! `mathml-entities.lua`) and the TeX-like symbol names.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::{Atom, Form};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OperatorForm {
    pub form: Form,
    /// In mu.
    pub lspace: f64,
    pub rspace: f64,
    pub stretchy: bool,
    pub largeop: bool,
    pub movable_limits: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OperatorEntry {
    pub atom: Atom,
    pub forms: Vec<OperatorForm>,
}

impl OperatorEntry {
    /// The properties for `form`, or else for the infix, prefix or postfix
    /// form, in that order.
    pub fn form(&self, form: Form) -> Option<&OperatorForm> {
        let find = |f: Form| self.forms.iter().find(|o| o.form == f);
        find(form).or_else(|| find(Form::Infix)).or_else(|| find(Form::Prefix)).or_else(|| find(Form::Postfix))
    }
}

pub(crate) struct Dictionary {
    pub symbols: HashMap<String, String>,
    pub operators: HashMap<String, OperatorEntry>,
}

const ALIASES: &[(&str, &str)] = &[
    ("%", "mathpercent"),
    ("dots", "unicodeellipsis"),
    ("ldots", "unicodeellipsis"),
    ("cdots", "unicodecdots"),
    ("implies", "Longrightarrow"),
    ("iff", "Longleftrightarrow"),
    ("vec", "overrightarrow"),
    ("le", "leq"),
    ("ge", "geq"),
    ("neq", "ne"),
    ("triangle", "bigtriangleup"),
    ("bigcirc", "mdlgwhtcircle"),
    ("circ", "vysmwhtcircle"),
    ("bullet", "smblkcircle"),
    ("yen", "mathyen"),
    ("sterling", "mathsterling"),
    ("diamond", "smwhtdiamond"),
    ("emptyset", "varnothing"),
    ("hbar", "hslash"),
    ("land", "wedge"),
    ("lor", "vee"),
    ("owns", "ni"),
    ("gets", "leftarrow"),
    ("mathring", "ocirc"),
    ("lnot", "neg"),
    ("colon", "mathcolon"),
    ("eth", "matheth"),
    ("AA", "Angstrom"),
    ("bbsum", "Bbbsum"),
    ("blacksquare", "mdlgblksquare"),
    ("square", "mdlgwhtsquare"),
    ("lozenge", "mdlgwhtlozenge"),
    ("circlearrowleft", "acwcirclearrow"),
    ("circlearrowright", "cwcirclearrow"),
    ("blacklozenge", "mdlgblklozenge"),
    ("overline", "overbar"),
    ("underline", "mathunderbar"),
    ("underbar", "mathunderbar"),
    ("overrightharpoon", "rightharpoonaccent"),
    ("overleftharpoon", "leftharpoonaccent"),
    ("utilde", "wideutilde"),
    ("widecheck", "check"),
    ("widehat", "hat"),
    ("widetilde", "tilde"),
];

const GREEK: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ϵ"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("omicron", "ο"),
    ("pi", "π"),
    ("varpi", "ϖ"),
    ("rho", "ρ"),
    ("varrho", "ϱ"),
    ("sigma", "σ"),
    ("varsigma", "ς"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "ϕ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Alpha", "Α"),
    ("Beta", "Β"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Epsilon", "Ε"),
    ("Zeta", "Ζ"),
    ("Eta", "Η"),
    ("Theta", "Θ"),
    ("Iota", "Ι"),
    ("Kappa", "Κ"),
    ("Lambda", "Λ"),
    ("Mu", "Μ"),
    ("Nu", "Ν"),
    ("Xi", "Ξ"),
    ("Omicron", "Ο"),
    ("Pi", "Π"),
    ("Rho", "Ρ"),
    ("Sigma", "Σ"),
    ("Tau", "Τ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Chi", "Χ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("digamma", "ϝ"),
    ("Digamma", "Ϝ"),
];

pub(crate) fn dictionary() -> &'static Dictionary {
    static DICT: OnceLock<Dictionary> = OnceLock::new();
    DICT.get_or_init(|| {
        let mut symbols = HashMap::new();
        let mut operators = HashMap::new();
        for line in include_str!("../../math/operators.txt").lines() {
            let mut fields = line.split('\t');
            let (Some(cps), Some(atom), Some(name)) = (fields.next(), fields.next(), fields.next()) else { continue };
            let text: String = cps
                .split(' ')
                .filter_map(|c| u32::from_str_radix(c, 16).ok().and_then(char::from_u32))
                .collect();
            if name != "-" {
                symbols.insert(name.to_string(), text.clone());
            }
            let forms = fields
                .filter_map(|f| {
                    let mut parts = f.split(' ');
                    let form = parts.next()?.parse().ok()?;
                    let lspace = parts.next()?.parse().ok()?;
                    let rspace = parts.next()?.parse().ok()?;
                    let flags = parts.next().unwrap_or("");
                    Some(OperatorForm {
                        form,
                        lspace,
                        rspace,
                        stretchy: flags.contains('s'),
                        largeop: flags.contains('l'),
                        movable_limits: flags.contains('m'),
                    })
                })
                .collect();
            operators.insert(text, OperatorEntry { atom: atom.parse().unwrap_or(Atom::Ord), forms });
        }
        for (alias, name) in ALIASES {
            if let Some(text) = symbols.get(*name).cloned() {
                symbols.insert(alias.to_string(), text);
            }
        }
        for (name, text) in GREEK {
            symbols.insert(name.to_string(), text.to_string());
        }
        if let Some(minus) = operators.get("−").cloned() {
            operators.insert("-".into(), minus);
        }
        if let Some(entry) = symbols.get("underleftrightarrow").and_then(|t| operators.get_mut(t)) {
            entry.atom = Atom::BotAccent;
        }
        Dictionary { symbols, operators }
    })
}

/// The character a TeX-like name stands for, as `\alpha` or `\sum`.
pub fn symbol(name: &str) -> Option<&'static str> {
    dictionary().symbols.get(name).map(String::as_str)
}

pub(crate) fn operator(text: &str) -> Option<&'static OperatorEntry> {
    dictionary().operators.get(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_has_operators_and_names() {
        assert_eq!(symbol("sum"), Some("∑"));
        assert_eq!(symbol("le"), Some("≤"));
        assert_eq!(symbol("alpha"), Some("α"));
        let sum = operator("∑").unwrap();
        assert_eq!(sum.atom, Atom::Op);
        let prefix = sum.form(Form::Infix).unwrap();
        assert!(prefix.largeop && prefix.movable_limits);
        assert_eq!(operator("-").unwrap().atom, Atom::Bin);
        assert!(operator("(").unwrap().form(Form::Prefix).unwrap().stretchy);
    }
}
