//! SILE's TeX-like math syntax (`texlike.lua`): a parser, and a compiler
//! from the parse tree to MathML-like elements with macros.

use std::collections::HashMap;
use std::fmt;

use super::mathml::{Content, Element};
use super::operators::{self, dictionary};
use super::{Atom, MathNode};

#[derive(Debug, Clone, PartialEq)]
pub struct TexError(pub String);

impl fmt::Display for TexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TexError {}

type Options = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
enum Ast {
    Atom(String),
    Command { name: String, options: Options, args: Vec<Ast> },
    List { items: Vec<Ast>, paired: bool },
    Sup(Box<Ast>, Box<Ast>),
    Sub(Box<Ast>, Box<Ast>),
    SubSup(Box<Ast>, Box<Ast>, Box<Ast>),
    Text { options: Options, text: String },
    Sqrt { radicand: Box<Ast>, degree: Option<Box<Ast>> },
    Def { name: String, body: Box<Ast> },
    Argument(usize),
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

const DELCODE: &str = "([</|)]>.";

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.rest().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if !c.is_ascii_whitespace() {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn error<T>(&self, msg: &str) -> Result<T, TexError> {
        Err(TexError(format!("{msg} at position {} in math: {}", self.pos, self.src)))
    }

    fn comment(&mut self) -> bool {
        if !self.eat("%") {
            return false;
        }
        while let Some(c) = self.peek() {
            self.pos += c.len_utf8();
            if c == '\r' || c == '\n' {
                break;
            }
        }
        true
    }

    fn mathlist(&mut self, no_bracket: bool) -> Result<Vec<Ast>, TexError> {
        let mut items = Vec::new();
        loop {
            if self.comment() {
                continue;
            }
            if self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.ws();
                continue;
            }
            if no_bracket && self.peek() == Some(']') {
                break;
            }
            match self.element()? {
                Some(e) => items.push(e),
                None => break,
            }
        }
        Ok(items)
    }

    fn element(&mut self) -> Result<Option<Ast>, TexError> {
        let Some(base) = self.element_no_infix()? else { return Ok(None) };
        let after_base = self.pos;
        self.ws();
        let b = Box::new(base);
        if let Some(sup) = self.primes_sup()? {
            let after = self.pos;
            if let Some(sub) = self.script("_")? {
                return Ok(Some(Ast::SubSup(b, Box::new(sub), Box::new(sup))));
            }
            self.pos = after;
            return Ok(Some(Ast::Sup(b, Box::new(sup))));
        }
        let at_script = self.pos;
        if self.eat("^") {
            self.ws();
            if let Some(sup) = self.element_no_infix()? {
                let after = self.pos;
                if let Some(sub) = self.script("_")? {
                    return Ok(Some(Ast::SubSup(b, Box::new(sub), Box::new(sup))));
                }
                self.pos = after;
                return Ok(Some(Ast::Sup(b, Box::new(sup))));
            }
            self.pos = after_base;
            return Ok(Some(*b));
        }
        self.pos = at_script;
        if self.eat("_") {
            self.ws();
            if let Some(sub) = self.element_no_infix()? {
                if let Some(sup) = self.primes_sup()? {
                    return Ok(Some(Ast::SubSup(b, Box::new(sub), Box::new(sup))));
                }
                let after = self.pos;
                if let Some(sup) = self.script("^")? {
                    return Ok(Some(Ast::SubSup(b, Box::new(sub), Box::new(sup))));
                }
                self.pos = after;
                return Ok(Some(Ast::Sub(b, Box::new(sub))));
            }
        }
        self.pos = after_base;
        Ok(Some(*b))
    }

    /// `_ marker _ element`, or nothing (restoring the position).
    fn script(&mut self, marker: &str) -> Result<Option<Ast>, TexError> {
        let start = self.pos;
        self.ws();
        if self.eat(marker) {
            self.ws();
            if let Some(e) = self.element_no_infix()? {
                return Ok(Some(e));
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn primes(&mut self) -> Option<Ast> {
        for (s, p) in [("''''", "⁗"), ("'''", "‴"), ("''", "″"), ("'", "′")] {
            if self.eat(s) {
                return Some(Ast::Atom(p.into()));
            }
        }
        None
    }

    /// Primes, possibly followed by a superscript they join.
    fn primes_sup(&mut self) -> Result<Option<Ast>, TexError> {
        let Some(p) = self.primes() else { return Ok(None) };
        let after = self.pos;
        if let Some(e) = self.script("^")? {
            return Ok(Some(match e {
                Ast::List { mut items, paired } => {
                    items.insert(0, p);
                    Ast::List { items, paired }
                }
                e => Ast::List { items: vec![p, e], paired: false },
            }));
        }
        self.pos = after;
        Ok(Some(p))
    }

    fn element_no_infix(&mut self) -> Result<Option<Ast>, TexError> {
        let start = self.pos;
        for f in [Self::left_right, Self::def, Self::text, Self::sqrt, Self::command, Self::group] {
            if let Some(e) = f(self)? {
                return Ok(Some(e));
            }
            self.pos = start;
        }
        if self.eat("#") {
            let digits: String = self.rest().chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() && !digits.starts_with('0') {
                self.pos += digits.len();
                return Ok(Some(Ast::Argument(digits.parse().unwrap_or(0))));
            }
            self.pos = start;
        }
        Ok(self.atom())
    }

    fn atom(&mut self) -> Option<Ast> {
        let rest = self.rest();
        let digits = |s: &str| s.chars().take_while(char::is_ascii_digit).count();
        let int = digits(rest);
        let decimal = rest[int..].strip_prefix('.').map(digits).filter(|d| *d > 0).map(|d| int + 1 + d);
        if let Some(len) = decimal.or((int > 0).then_some(int)) {
            self.pos += len;
            return Some(Ast::Atom(rest[..len].to_string()));
        }
        if self.eat("*") {
            return Some(Ast::Atom("\u{2217}".into()));
        }
        if self.eat("\\{") {
            return Some(Ast::Atom("{".into()));
        }
        if self.eat("\\}") {
            return Some(Ast::Atom("}".into()));
        }
        let c = self.peek()?;
        if "\\{}%^_&'".contains(c) {
            return None;
        }
        self.pos += c.len_utf8();
        Some(Ast::Atom(c.to_string()))
    }

    fn left_right_excluded(&self) -> bool {
        let rest = self.rest();
        ["left", "right"].iter().any(|w| {
            rest.strip_prefix(w).and_then(|r| r.chars().next()).is_some_and(|c| DELCODE.contains(c) || c == '\\')
        })
    }

    fn control_sequence_name(&mut self) -> Option<String> {
        if self.left_right_excluded() {
            return None;
        }
        let word: String = self.rest().chars().take_while(char::is_ascii_alphabetic).collect();
        if !word.is_empty() {
            self.pos += word.len();
            return Some(word);
        }
        let c = self.peek().filter(|c| !"{}\\".contains(*c))?;
        self.pos += c.len_utf8();
        Some(c.to_string())
    }

    fn parameters(&mut self) -> Options {
        let start = self.pos;
        if !self.eat("[") {
            return Vec::new();
        }
        let mut options = Vec::new();
        loop {
            let key: String = self.rest().chars().take_while(|c| c.is_alphanumeric() || "_-:.".contains(*c)).collect();
            let key = if key.is_empty() {
                match self.peek() {
                    Some(c) if c != ']' => c.to_string(),
                    _ => break,
                }
            } else {
                key
            };
            let before_pair = self.pos;
            self.pos += key.len();
            self.ws();
            if !self.eat("=") {
                self.pos = before_pair;
                break;
            }
            self.ws();
            let value = if self.eat("\"") {
                let v: String = self.rest().chars().take_while(|c| *c != '"').collect();
                if v.is_empty() || !self.rest()[v.len()..].starts_with('"') {
                    self.pos = before_pair;
                    break;
                }
                self.pos += v.len() + 1;
                v
            } else {
                let v: String = self.rest().chars().take_while(|c| !",;]".contains(*c)).collect();
                if v.is_empty() {
                    self.pos = before_pair;
                    break;
                }
                self.pos += v.len();
                v
            };
            options.push((key, value));
            if self.peek().is_some_and(|c| c == ',' || c == ';') {
                self.pos += 1;
                self.ws();
            }
        }
        if self.eat("]") {
            options
        } else {
            self.pos = start;
            Vec::new()
        }
    }

    fn group(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("{") {
            return Ok(None);
        }
        let items = self.mathlist(false)?;
        if !self.eat("}") {
            return self.error("`}` expected");
        }
        Ok(Some(Ast::List { items, paired: false }))
    }

    fn delimiter(&mut self) -> Option<Option<Ast>> {
        if let Some(c) = self.peek().filter(|c| DELCODE.contains(*c)) {
            self.pos += 1;
            return Some((c != '.').then(|| Ast::Atom(c.to_string())));
        }
        let start = self.pos;
        if self.eat("\\") {
            if let Some(c) = self.peek().filter(|c| *c == '{' || *c == '}') {
                self.pos += 1;
                return Some(Some(Ast::Atom(c.to_string())));
            }
            if let Some(name) = self.control_sequence_name() {
                return Some(Some(Ast::Command { name, options: Vec::new(), args: Vec::new() }));
            }
        }
        self.pos = start;
        None
    }

    fn left_right(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("\\left") {
            return Ok(None);
        }
        let Some(left) = self.delimiter() else { return Ok(None) };
        let inner = Ast::List { items: self.mathlist(false)?, paired: false };
        if !self.eat("\\right") {
            return Ok(None);
        }
        let Some(right) = self.delimiter() else { return Ok(None) };
        if left.is_none() && right.is_none() {
            return Ok(Some(inner));
        }
        let mut items: Vec<Ast> = left.into_iter().collect();
        items.push(inner);
        items.extend(right);
        Ok(Some(Ast::List { items, paired: true }))
    }

    fn def(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("\\def") {
            return Ok(None);
        }
        self.ws();
        if !self.eat("{") {
            return Ok(None);
        }
        let Some(name) = self.control_sequence_name() else { return Ok(None) };
        if !self.eat("}") {
            return Ok(None);
        }
        self.ws();
        if !self.eat("{") {
            return Ok(None);
        }
        let body = self.mathlist(false)?;
        if !self.eat("}") {
            return Ok(None);
        }
        Ok(Some(Ast::Def { name, body: Box::new(Ast::List { items: body, paired: false }) }))
    }

    fn text(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("\\text") {
            return Ok(None);
        }
        let options = self.parameters();
        if !self.eat("{") {
            return Ok(None);
        }
        let text: String = self.rest().chars().take_while(|c| *c != '}').collect();
        if text.is_empty() {
            return Ok(None);
        }
        self.pos += text.len();
        if !self.eat("}") {
            return self.error("`}` expected");
        }
        Ok(Some(Ast::Text { options, text }))
    }

    fn sqrt(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("\\sqrt") {
            return Ok(None);
        }
        let start = self.pos;
        let mut degree = None;
        if self.eat("[") {
            let items = self.mathlist(true)?;
            if self.eat("]") {
                degree = Some(Box::new(Ast::List { items, paired: false }));
            } else {
                self.pos = start;
            }
        }
        if !self.eat("{") {
            return Ok(None);
        }
        let items = self.mathlist(false)?;
        if !self.eat("}") {
            return Ok(None);
        }
        Ok(Some(Ast::Sqrt { radicand: Box::new(Ast::List { items, paired: false }), degree }))
    }

    fn command(&mut self) -> Result<Option<Ast>, TexError> {
        if !self.eat("\\") {
            return Ok(None);
        }
        let Some(name) = self.control_sequence_name() else { return Ok(None) };
        let options = self.parameters();
        let start = self.pos;
        if let Some(rows) = self.table_argument()? {
            return Ok(Some(Ast::Command { name, options, args: rows }));
        }
        self.pos = start;
        let mut args = Vec::new();
        while let Some(g) = self.group()? {
            args.push(g);
        }
        Ok(Some(Ast::Command { name, options, args }))
    }

    /// `{a & b \\ c & d}`: rows of cells.
    fn table_argument(&mut self) -> Result<Option<Vec<Ast>>, TexError> {
        if !self.eat("{") {
            return Ok(None);
        }
        let mut rows = vec![self.table_row()?];
        while self.eat("\\\\") {
            rows.push(self.table_row()?);
        }
        if rows.len() < 2 {
            return Ok(None);
        }
        if !self.eat("}") {
            return self.error("`}` expected");
        }
        if let Some(Ast::List { items, .. }) = rows.last()
            && (items.is_empty() || matches!(items.as_slice(), [Ast::List { items, .. }] if items.is_empty()))
        {
            rows.pop();
        }
        Ok(Some(rows))
    }

    fn table_row(&mut self) -> Result<Ast, TexError> {
        let mut cells = vec![Ast::List { items: self.mathlist(false)?, paired: false }];
        while self.eat("&") {
            cells.push(Ast::List { items: self.mathlist(false)?, paired: false });
        }
        Ok(Ast::List { items: cells, paired: false })
    }
}

fn parse(src: &str) -> Result<Ast, TexError> {
    let mut p = Parser { src, pos: 0 };
    let items = p.mathlist(false)?;
    if p.pos < src.len() {
        return p.error("Unexpected character at end of math code");
    }
    Ok(Ast::List { items, paired: false })
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ArgType {
    Tree,
    Str,
    Unknown,
}

#[derive(Debug, Clone)]
enum Value {
    Str(String),
    Tree(Element),
    None,
}

#[derive(Debug, Clone)]
enum Macro {
    Body(Vec<ArgType>, Ast),
    /// `\mi`, `\mo` and `\mn`: a token from a string.
    Token,
    VarLimits(&'static str, char),
    BraceLike(&'static str, char),
}

impl Macro {
    fn arity(&self) -> usize {
        match self {
            Macro::Body(types, _) => types.len(),
            Macro::Token | Macro::BraceLike(..) => 1,
            Macro::VarLimits(..) => 0,
        }
    }

    fn arg_type(&self, i: usize) -> ArgType {
        match self {
            Macro::Body(types, _) => types.get(i).copied().unwrap_or(ArgType::Tree),
            Macro::Token => ArgType::Str,
            _ => ArgType::Tree,
        }
    }
}

/// What compiling a piece of the parse tree gives.
enum Out {
    Element(Element),
    /// Children for the enclosing element.
    Splice(Vec<Content>),
    Text(String),
    Nothing,
}

const PREDEFINED: &str = r"
  \def{frac}{\mfrac{#1}{#2}}
  \def{sqrt}{\msqrt{#1}}
  \def{bi}{\mi[mathvariant=bold-italic]{#1}}
  \def{dsi}{\mi[mathvariant=double-struck]{#1}}
  \def{to}{\mo[atom=bin]{→}}
  \def{lim}{\mo[atom=op, movablelimits=true]{lim}}
  \def{gcd}{\mo[atom=op, movablelimits=true]{gcd}}
  \def{sup}{\mo[atom=op, movablelimits=true]{sup}}
  \def{inf}{\mo[atom=op, movablelimits=true]{inf}}
  \def{max}{\mo[atom=op, movablelimits=true]{max}}
  \def{min}{\mo[atom=op, movablelimits=true]{min}}
  \def{limsup}{\mo[atom=op, movablelimits=true]{lim sup}}
  \def{liminf}{\mo[atom=op, movablelimits=true]{lim inf}}
  \def{projlim}{\mo[atom=op, movablelimits=true]{proj lim}}
  \def{injlim}{\mo[atom=op, movablelimits=true]{inj lim}}
  \def{arccos}{\mo[atom=op]{arccos}}
  \def{arcsin}{\mo[atom=op]{arcsin}}
  \def{arctan}{\mo[atom=op]{arctan}}
  \def{arg}{\mo[atom=op]{arg}}
  \def{cos}{\mo[atom=op]{cos}}
  \def{cosh}{\mo[atom=op]{cosh}}
  \def{cot}{\mo[atom=op]{cot}}
  \def{coth}{\mo[atom=op]{coth}}
  \def{csc}{\mo[atom=op]{csc}}
  \def{deg}{\mo[atom=op]{deg}}
  \def{det}{\mo[atom=op]{det}}
  \def{dim}{\mo[atom=op]{dim}}
  \def{exp}{\mo[atom=op]{exp}}
  \def{hom}{\mo[atom=op]{hom}}
  \def{ker}{\mo[atom=op]{ker}}
  \def{lg}{\mo[atom=op]{lg}}
  \def{ln}{\mo[atom=op]{ln}}
  \def{log}{\mo[atom=op]{log}}
  \def{Pr}{\mo[atom=op]{Pr}}
  \def{sec}{\mo[atom=op]{sec}}
  \def{sin}{\mo[atom=op]{sin}}
  \def{sinh}{\mo[atom=op]{sinh}}
  \def{tan}{\mo[atom=op]{tan}}
  \def{tanh}{\mo[atom=op]{tanh}}
  \def{thinspace}{\mspace[width=thin]}
  \def{negthinspace}{\mspace[width=-thin]}
  \def{,}{\thinspace}
  \def{!}{\negthinspace}
  \def{medspace}{\mspace[width=med]}
  \def{negmedspace}{\mspace[width=-med]}
  \def{>}{\medspace}
  \def{thickspace}{\mspace[width=thick]}
  \def{negthickspace}{\mspace[width=-thick]}
  \def{;}{\thickspace}
  \def{enspace}{\mspace[width=1en]}
  \def{enskip}{\enspace}
  \def{quad}{\mspace[width=1em]}
  \def{qquad}{\mspace[width=2em]}
  \def{Gamma}{\mi[mathvariant=normal]{Γ}}
  \def{Delta}{\mi[mathvariant=normal]{Δ}}
  \def{Theta}{\mi[mathvariant=normal]{Θ}}
  \def{Lambda}{\mi[mathvariant=normal]{Λ}}
  \def{Xi}{\mi[mathvariant=normal]{Ξ}}
  \def{Pi}{\mi[mathvariant=normal]{Π}}
  \def{Sigma}{\mi[mathvariant=normal]{Σ}}
  \def{Upsilon}{\mi[mathvariant=normal]{Υ}}
  \def{Phi}{\mi[mathvariant=normal]{Φ}}
  \def{Psi}{\mi[mathvariant=normal]{Ψ}}
  \def{Omega}{\mi[mathvariant=normal]{Ω}}
  \def{mathcal}{\mi[mathvariant=script]{#1}}
  \def{mathfrak}{\mi[mathvariant=fraktur]{#1}}
  \def{mathbb}{\mi[mathvariant=double-struck]{#1}}
  \def{mathrm}{\mi[mathvariant=normal]{#1}}
  \def{mathbf}{\mi[mathvariant=bold]{#1}}
  \def{mathit}{\mi[mathvariant=italic]{#1}}
  \def{mathsf}{\mi[mathvariant=sans-serif]{#1}}
  \def{mathtt}{\mi[mathvariant=monospace]{#1}}
  \def{bmod}{\mo[atom=bin]{mod}}
  \def{pmod}{\quad(\mo[atom=ord]{mod}\>#1)}
  \def{mod}{\quad \mo[atom=ord]{mod}\>#1}
  \def{pod}{\quad(#1)}
  \def{phantom}{\mphantom{#1}}
  \def{hphantom}{\mpadded[height=0, depth=0]{\mphantom{#1}}}
  \def{vphantom}{\mpadded[width=0]{\mphantom{#1}}}
  \def{stackrel}{\mover{#2}{#1}}
  \def{stackbin}{\mover{#2}{#1}}
  \def{overset}{\mover{#2}{#1}}
  \def{underset}{\munder{#2}{#1}}
  \def{boxed}{\menclose[notations=box]{#1}}
  \def{cancel}{\menclose[notations=updiagonalstrike]{#1}}
  \def{bcancel}{\menclose[notations=downdiagonalstrike]{#1}}
  \def{xcancel}{\menclose[notations=updiagonalstrike downdiagonalstrike]{#1}}
  \def{cancelto}{\menclose[notations=northeastarrow]{#2}^{#1}}
";

/// SILE's TeX-like math syntax: a layer of shorthands over MathML, with
/// macros (`\def{name}{body}`) that last as long as this value does.
#[derive(Debug, Clone)]
pub struct TexMath {
    macros: HashMap<String, Macro>,
}

impl Default for TexMath {
    fn default() -> Self {
        Self::new()
    }
}

impl TexMath {
    pub fn new() -> Self {
        let mut macros = HashMap::new();
        macros.insert("mi".into(), Macro::Token);
        macros.insert("mo".into(), Macro::Token);
        macros.insert("mn".into(), Macro::Token);
        for (name, cmd, sym) in [
            ("varlimsup", "mover", '\u{203E}'),
            ("varliminf", "munder", '\u{203E}'),
            ("varprojlim", "munder", '\u{2190}'),
            ("varinjlim", "munder", '\u{2192}'),
        ] {
            macros.insert(name.into(), Macro::VarLimits(cmd, sym));
        }
        for (name, cmd, sym) in [
            ("overbrace", "mover", '\u{23DE}'),
            ("underbrace", "munder", '\u{23DF}'),
            ("overparen", "mover", '\u{23DC}'),
            ("underparen", "munder", '\u{23DD}'),
            ("overbracket", "mover", '\u{23B4}'),
            ("underbracket", "munder", '\u{23B5}'),
        ] {
            macros.insert(name.into(), Macro::BraceLike(cmd, sym));
        }
        let mut tex = Self { macros };
        tex.compile_source(PREDEFINED).expect("predefined math macros compile");
        tex
    }

    /// `src` as elements, ready for `Element::to_node`.
    pub fn compile_source(&mut self, src: &str) -> Result<Element, TexError> {
        let ast = parse(src)?;
        let list = match self.compile(&ast, &[])? {
            Out::Element(e) => e,
            Out::Splice(children) => Element { command: "mrow".into(), children, ..Default::default() },
            _ => Element::new("mrow"),
        };
        if list.elements_all(|c| c.command == "mrow") {
            let mut list = list;
            list.command = "math".into();
            return Ok(list);
        }
        Ok(Element { command: "math".into(), children: vec![Content::Element(list)], ..Default::default() })
    }

    /// Parse and compile `src`.
    pub fn parse(&mut self, src: &str) -> Result<MathNode, TexError> {
        self.compile_source(src)?.to_node().map_err(TexError)
    }

    fn compile_children(&mut self, items: &[Ast], env: &[Value]) -> Result<Vec<Content>, TexError> {
        let mut out = Vec::new();
        for item in items {
            match self.compile(item, env)? {
                Out::Element(e) => out.push(Content::Element(e)),
                Out::Splice(children) => out.extend(children),
                Out::Text(t) => out.push(Content::Text(t)),
                Out::Nothing => {}
            }
        }
        Ok(out)
    }

    fn compile_element(&mut self, ast: &Ast, env: &[Value]) -> Result<Element, TexError> {
        Ok(match self.compile(ast, env)? {
            Out::Element(e) => e,
            Out::Splice(children) => Element { command: "mrow".into(), children, ..Default::default() },
            Out::Text(t) => Element { command: "mrow".into(), children: vec![Content::Text(t)], ..Default::default() },
            Out::Nothing => Element::new("mrow"),
        })
    }

    fn compile(&mut self, ast: &Ast, env: &[Value]) -> Result<Out, TexError> {
        Ok(match ast {
            Ast::Atom(s) => Out::Element(atom(s)),
            Ast::List { items, paired } => {
                let children = self.compile_children(items, env)?;
                if let [Content::Element(only)] = children.as_slice() {
                    if only.command == "mtr" || only.command == "mtd" {
                        return Ok(Out::Element(only.clone()));
                    }
                    return Ok(Out::Element(Element { command: "mrow".into(), children, ..Default::default() }));
                }
                if *paired {
                    return Ok(Out::Element(Element { command: "mrow".into(), children, paired: true, ..Default::default() }));
                }
                Out::Element(Element { command: "mrow".into(), children: pair_delimiters(children), ..Default::default() })
            }
            Ast::Sup(base, sup) => self.scripts(&[base, sup], env, "msup", "mover")?,
            Ast::Sub(base, sub) => self.scripts(&[base, sub], env, "msub", "munder")?,
            Ast::SubSup(base, sub, sup) => self.scripts(&[base, sub, sup], env, "msubsup", "munderover")?,
            Ast::Def { name, body } => {
                let mut types = Vec::new();
                self.infer(body, ArgType::Tree, &mut types)?;
                self.macros.insert(name.clone(), Macro::Body(types, (**body).clone()));
                Out::Nothing
            }
            Ast::Text { options, text } => {
                Out::Element(Element { command: "mtext".into(), options: options.clone(), children: vec![Content::Text(text.clone())], ..Default::default() })
            }
            Ast::Sqrt { radicand, degree } => {
                let mut children = vec![Content::Element(self.compile_element(radicand, env)?)];
                let command = match degree {
                    Some(d) => {
                        children.push(Content::Element(self.compile_element(d, env)?));
                        "mroot"
                    }
                    None => "msqrt",
                };
                Out::Element(Element { command: command.into(), children, ..Default::default() })
            }
            Ast::Argument(i) => match env.get(i - 1) {
                Some(Value::Tree(e)) => Out::Element(e.clone()),
                Some(Value::Str(s)) => Out::Text(s.clone()),
                _ => return Err(TexError(format!("Argument #{i} has escaped its scope (probably not fully applied command)."))),
            },
            Ast::Command { name, options, args } => {
                if let Some(m) = self.macros.get(name).cloned() {
                    return self.apply(name, &m, options, args, env);
                }
                if let Some(sym) = operators::symbol(name) {
                    let entry = operators::operator(sym);
                    let is = |a: Atom| entry.is_some_and(|e| e.atom == a);
                    if !args.is_empty() && (is(Atom::Accent) || is(Atom::BotAccent)) {
                        let (command, flag) = if is(Atom::Accent) { ("mover", "accent") } else { ("munder", "accentunder") };
                        let base = self.compile_element(&args[0], env)?;
                        let mut e = Element::new(command);
                        e.set_option(flag, "true");
                        e.children = vec![Content::Element(base), Content::Element(token("mo", sym))];
                        return Ok(Out::Element(e));
                    }
                    let symbol = atom(sym);
                    if args.is_empty() {
                        return Ok(Out::Element(symbol));
                    }
                    let mut children = vec![Content::Element(symbol)];
                    children.extend(self.compile_children(args, env)?);
                    return Ok(Out::Splice(children));
                }
                let children = self.compile_children(args, env)?;
                Out::Element(Element { command: name.clone(), options: options.clone(), children, ..Default::default() })
            }
        })
    }

    fn scripts(&mut self, parts: &[&Ast], env: &[Value], scripted: &str, stacked: &str) -> Result<Out, TexError> {
        let mut children = Vec::new();
        for p in parts {
            children.extend(self.compile_children(std::slice::from_ref(*p), env)?);
        }
        let stack = matches!(children.first(), Some(Content::Element(b)) if movable_or_stacked(b));
        let command = if stack { stacked } else { scripted };
        Ok(Out::Element(Element { command: command.into(), children, ..Default::default() }))
    }

    fn apply(&mut self, name: &str, m: &Macro, options: &Options, args: &[Ast], env: &[Value]) -> Result<Out, TexError> {
        if args.len() != m.arity() {
            return Err(TexError(format!("Wrong number of arguments ({}) for command {name} (should be {})", args.len(), m.arity())));
        }
        let mut values = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            values.push(match m.arg_type(i) {
                ArgType::Tree => Value::Tree(self.compile_element(arg, env)?),
                _ => self.compile_str(arg, env)?,
            });
        }
        Ok(match m {
            Macro::Token => {
                let text = match values.pop() {
                    Some(Value::Str(s)) => Content::Text(s),
                    Some(Value::Tree(e)) => Content::Element(e),
                    _ => Content::Text(String::new()),
                };
                Out::Element(Element { command: name.into(), options: options.clone(), children: vec![text], ..Default::default() })
            }
            Macro::Body(_, body) => match self.compile(body, &values)? {
                Out::Element(e) if e.command == "mrow" => Out::Splice(e.children),
                other => other,
            },
            Macro::VarLimits(command, sym) => {
                let mut e = Element::new(*command);
                e.set_option(if *command == "mover" { "accent" } else { "accentunder" }, "true");
                e.movable_limits = true;
                let mut lim = token("mo", "lim");
                lim.set_option("atom", "op");
                lim.set_option("movablelimits", "false");
                let mut accent = token("mo", &sym.to_string());
                accent.set_option("accentunder", "true");
                e.children = vec![Content::Element(lim), Content::Element(accent)];
                Out::Element(e)
            }
            Macro::BraceLike(command, sym) => {
                let mut e = Element::new(*command);
                e.set_option(if *command == "mover" { "accent" } else { "accentunder" }, "true");
                e.always_stacked = true;
                let base = match values.pop() {
                    Some(Value::Tree(t)) => Content::Element(t),
                    _ => Content::Element(Element::new("mrow")),
                };
                let mut brace = token("mo", &sym.to_string());
                brace.set_option("stretchy", "true");
                e.children = vec![base, Content::Element(brace)];
                Out::Element(e)
            }
        })
    }

    fn compile_str(&mut self, arg: &Ast, env: &[Value]) -> Result<Value, TexError> {
        let Ast::List { items, .. } = arg else {
            return Ok(match arg {
                Ast::Argument(i) => env.get(i - 1).cloned().unwrap_or(Value::None),
                Ast::Atom(s) => Value::Str(s.clone()),
                _ => Value::None,
            });
        };
        if let [Ast::Argument(i)] = items.as_slice() {
            return Ok(env.get(i - 1).cloned().unwrap_or(Value::None));
        }
        let mut s = String::new();
        for item in items {
            match item {
                Ast::Atom(a) => s.push_str(a),
                Ast::Command { name, .. } if operators::symbol(name).is_some() => s.push_str(operators::symbol(name).unwrap()),
                _ => return Err(TexError("Encountered non-character token in command that takes a string".into())),
            }
        }
        Ok(Value::Str(s))
    }

    fn infer(&self, body: &Ast, required: ArgType, types: &mut Vec<ArgType>) -> Result<(), TexError> {
        match body {
            Ast::Argument(i) => {
                if types.len() < *i {
                    types.resize(*i, ArgType::Unknown);
                }
                types[i - 1] = required;
            }
            Ast::Command { name, args, .. } => match self.macros.get(name) {
                Some(m) => {
                    if m.arity() != args.len() {
                        return Err(TexError(format!(
                            "Wrong number of arguments ({}) for command {name} (should be {})",
                            args.len(),
                            m.arity()
                        )));
                    }
                    for (i, a) in args.iter().enumerate() {
                        self.infer(a, m.arg_type(i), types)?;
                    }
                }
                None => {
                    for a in args {
                        self.infer(a, ArgType::Tree, types)?;
                    }
                }
            },
            Ast::Atom(_) | Ast::Text { .. } => {}
            Ast::List { items, .. } => {
                for i in items {
                    self.infer(i, required, types)?;
                }
            }
            Ast::Sup(a, b) | Ast::Sub(a, b) => {
                self.infer(a, required, types)?;
                self.infer(b, required, types)?;
            }
            Ast::SubSup(a, b, c) => {
                for x in [a, b, c] {
                    self.infer(x, required, types)?;
                }
            }
            Ast::Sqrt { radicand, degree } => {
                self.infer(radicand, required, types)?;
                if let Some(d) = degree {
                    self.infer(d, required, types)?;
                }
            }
            Ast::Def { body, .. } => self.infer(body, required, types)?,
        }
        Ok(())
    }
}

impl Element {
    fn elements_all(&self, f: impl Fn(&Element) -> bool) -> bool {
        self.children.iter().all(|c| matches!(c, Content::Element(e) if f(e)))
    }
}

fn token(command: &str, text: &str) -> Element {
    Element { command: command.into(), children: vec![Content::Text(text.into())], ..Default::default() }
}

/// A character as `mi` (a single Latin or Greek letter), `mn` (digits) or
/// `mo` (anything else).
fn atom(s: &str) -> Element {
    let mut chars = s.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    let letter = single.is_some_and(|c| {
        c.is_ascii_alphabetic() || ('Α'..='Ω').contains(&c) || ('α'..='ω').contains(&c) || "ϑϕϰϱϖϵ".contains(c)
    });
    let command = if letter {
        "mi"
    } else if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        "mn"
    } else {
        "mo"
    };
    token(command, s)
}

fn operator_atom(e: &Element) -> Option<Atom> {
    if e.command != "mo" {
        return None;
    }
    if let Some(a) = e.option("atom") {
        return a.parse().ok();
    }
    match e.children.first() {
        Some(Content::Text(t)) => operators::operator(t).map(|o| o.atom),
        _ => None,
    }
}

fn movable_or_stacked(e: &Element) -> bool {
    if e.always_stacked {
        return true;
    }
    if e.command != "mo" {
        return e.movable_limits || matches!(e.children.first(), Some(Content::Element(first)) if movable_or_stacked(first));
    }
    if matches!(e.option("movablelimits"), Some("true" | "yes" | "1")) {
        return true;
    }
    match e.children.first() {
        Some(Content::Text(t)) => dictionary().operators.get(t).is_some_and(|o| o.forms.iter().any(|f| f.movable_limits)),
        _ => false,
    }
}

/// Wrap each opening delimiter and what follows up to its closing one in a
/// row of their own, so the delimiters stretch to what they enclose.
fn pair_delimiters(items: Vec<Content>) -> Vec<Content> {
    let mut stack: Vec<Vec<Content>> = Vec::new();
    let mut children: Vec<Content> = Vec::new();
    let is = |c: &Content, a: Atom| matches!(c, Content::Element(e) if operator_atom(e) == Some(a));
    for child in items {
        if is(&child, Atom::Open) {
            stack.push(std::mem::replace(&mut children, vec![child]));
        } else if is(&child, Atom::Close) {
            children.push(child);
            if let Some(parent) = stack.pop() {
                let row = Element { command: "mrow".into(), children: std::mem::replace(&mut children, parent), paired: true, ..Default::default() };
                children.push(Content::Element(row));
            }
        } else if let Content::Element(e) = &child
            && matches!(e.command.as_str(), "msubsup" | "msub" | "msup")
            && matches!(e.children.first(), Some(c) if is(c, Atom::Close))
            && !stack.is_empty()
        {
            let Content::Element(mut e) = child else { unreachable!() };
            let close = e.children.remove(0);
            children.push(close);
            let parent = stack.pop().unwrap();
            let row = Element { command: "mrow".into(), children: std::mem::replace(&mut children, parent), paired: true, ..Default::default() };
            e.children.insert(0, Content::Element(row));
            children.push(Content::Element(e));
        } else {
            children.push(child);
        }
    }
    while let Some(parent) = stack.pop() {
        let row = Element { command: "mrow".into(), children: std::mem::replace(&mut children, parent), paired: true, ..Default::default() };
        children.push(Content::Element(row));
    }
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::mathml::{Content, Element};

    fn show(e: &Element) -> String {
        let children: Vec<String> = e
            .children
            .iter()
            .map(|c| match c {
                Content::Text(t) => t.clone(),
                Content::Element(e) => show(e),
            })
            .collect();
        format!("({} {})", e.command, children.join(" "))
    }

    fn compile(src: &str) -> String {
        show(&TexMath::new().compile_source(src).unwrap())
    }

    #[test]
    fn scripts_and_operators() {
        assert_eq!(compile(r"e^{i\pi} = -1"), "(math (mrow (msup (mi e) (mrow (mi i) (mi π))) (mo =) (mo -) (mn 1)))");
        assert_eq!(
            compile(r"\sum_{i=0}^n x_i'"),
            "(math (mrow (munderover (mo ∑) (mrow (mi i) (mo =) (mn 0)) (mi n)) (msubsup (mi x) (mi i) (mo ′))))"
        );
    }

    #[test]
    fn macros_fractions_delimiters_and_tables() {
        assert_eq!(
            compile(r"\def{f}{#1^2} \f{x} + \frac{a}{b}"),
            "(math (mrow (msup (mrow (mi x)) (mn 2)) (mo +) (mfrac (mrow (mrow (mi a))) (mrow (mrow (mi b))))))"
        );
        assert_eq!(compile(r"\left( x \right)"), "(math (mrow (mo () (mrow (mi x)) (mo ))))");
        assert_eq!(
            compile(r"\table{a & b \\ c & d \\}"),
            "(math (mrow (table (mrow (mrow (mi a)) (mrow (mi b))) (mrow (mrow (mi c)) (mrow (mi d))))))"
        );
    }

    #[test]
    fn errors_are_reported() {
        assert!(TexMath::new().compile_source("{x").is_err());
        assert!(TexMath::new().parse(r"\frac{a}").is_err());
    }
}
