//! Generic MathML-like elements, as the TeX-like syntax compiles to and as
//! MathML markup reads, and their conversion to the typed tree.

use super::{
    ColumnAlign, Dimen, Enclose, Fraction, MathLength, MathNode, Notation, Operator, Padded, Row, Space, Table, Token, UnderOver,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Element(Element),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    /// The MathML tag, as `mrow` or `mfrac`.
    pub command: String,
    pub options: Vec<(String, String)>,
    pub children: Vec<Content>,
    /// A row of paired delimiters and what they enclose.
    pub paired: bool,
    /// An under/over whose scripts move beside it outside display style.
    pub movable_limits: bool,
    /// Always stacks scripts given to it, as braces do.
    pub(crate) always_stacked: bool,
}

impl Element {
    pub fn new(command: impl Into<String>) -> Self {
        Self { command: command.into(), ..Default::default() }
    }

    pub fn option(&self, key: &str) -> Option<&str> {
        self.options.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub(crate) fn set_option(&mut self, key: &str, value: &str) {
        self.options.retain(|(k, _)| k != key);
        self.options.push((key.to_string(), value.to_string()));
    }

    fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| match c {
            Content::Element(e) => Some(e),
            Content::Text(_) => None,
        })
    }

    fn text(&self) -> Result<String, String> {
        match self.children.first() {
            Some(Content::Text(t)) => Ok(t.trim().to_string()),
            None => Ok(String::new()),
            Some(Content::Element(_)) => Err(format!("{} command contains content which is not text", self.command)),
        }
    }

    fn converted_children(&self) -> Result<Vec<MathNode>, String> {
        self.elements().map(Element::to_node).collect()
    }

    fn exactly<const N: usize>(&self) -> Result<[MathNode; N], String> {
        let children = self.converted_children()?;
        let n = children.len();
        children.try_into().map_err(|_| format!("Wrong number of children in {}: {n}", self.command))
    }

    fn boolean(&self, key: &str) -> Result<Option<bool>, String> {
        self.option(key).map(parse_bool).transpose()
    }

    fn length(&self, key: &str) -> Result<Option<MathLength>, String> {
        self.option(key).map(str::parse).transpose()
    }

    fn dimen(&self, key: &str) -> Result<Option<Dimen>, String> {
        self.option(key).map(str::parse).transpose()
    }

    fn parse_option<T: std::str::FromStr<Err = String>>(&self, key: &str) -> Result<Option<T>, String> {
        self.option(key).map(str::parse).transpose()
    }

    /// The formula this element stands for.
    pub fn to_node(&self) -> Result<MathNode, String> {
        let row = |children| Ok(MathNode::Row(Row { children, paired: false }));
        match self.command.as_str() {
            "math" | "mathml" | "mstyle" => row(self.converted_children()?),
            "mrow" => Ok(MathNode::Row(Row { children: self.converted_children()?, paired: self.paired })),
            "mtd" => row(self.converted_children()?),
            "maction" => row(self.elements().next().map(Element::to_node).transpose()?.into_iter().collect()),
            "mphantom" => Ok(MathNode::Phantom(self.converted_children()?)),
            "mi" | "mn" => {
                let token = Token { text: self.text()?, variant: self.parse_option("mathvariant")? };
                Ok(if self.command == "mi" { MathNode::Identifier(token) } else { MathNode::Number(token) })
            }
            "mo" => Ok(MathNode::Operator(Operator {
                text: self.text()?,
                form: self.parse_option("form")?,
                atom: self.parse_option("atom")?,
                stretchy: self.boolean("stretchy")?,
                largeop: self.boolean("largeop")?,
                movable_limits: self.boolean("movablelimits")?,
                variant: self.parse_option("mathvariant")?,
                lspace: self.length("lspace")?,
                rspace: self.length("rspace")?,
            })),
            "mtext" | "ms" => {
                if self.children.len() > 1 {
                    return Err(format!("Wrong number of children in {}: {}", self.command, self.children.len()));
                }
                Ok(MathNode::Text(self.text_raw()?))
            }
            "mspace" => Ok(MathNode::Space(Space {
                width: self.length("width")?.unwrap_or_default(),
                height: self.length("height")?.unwrap_or_default(),
                depth: self.length("depth")?.unwrap_or_default(),
            })),
            "msub" => {
                let [base, sub] = self.exactly()?;
                Ok(MathNode::subscript(base, sub))
            }
            "msup" => {
                let [base, sup] = self.exactly()?;
                Ok(MathNode::superscript(base, sup))
            }
            "msubsup" => {
                let [base, sub, sup] = self.exactly()?;
                Ok(MathNode::sub_superscript(base, sub, sup))
            }
            "munder" | "mover" | "munderover" => {
                let (base, under, over) = match self.command.as_str() {
                    "munder" => {
                        let [base, under] = self.exactly()?;
                        (base, Some(under), None)
                    }
                    "mover" => {
                        let [base, over] = self.exactly()?;
                        (base, None, Some(over))
                    }
                    _ => {
                        let [base, under, over] = self.exactly()?;
                        (base, Some(under), Some(over))
                    }
                };
                Ok(MathNode::UnderOver(UnderOver {
                    base: Box::new(base),
                    under: under.map(Box::new),
                    over: over.map(Box::new),
                    accent: self.boolean("accent")?.unwrap_or(false),
                    accent_under: self.boolean("accentunder")?.unwrap_or(false),
                    movable_limits: self.movable_limits && self.command != "munderover",
                }))
            }
            "mfrac" => {
                let [numerator, denominator] = self.exactly()?;
                Ok(MathNode::Fraction(Fraction {
                    numerator: Box::new(numerator),
                    denominator: Box::new(denominator),
                    line_thickness: self.dimen("linethickness")?,
                    bevelled: self.boolean("bevelled")?.unwrap_or(false),
                }))
            }
            "msqrt" => Ok(MathNode::Root { radicand: Box::new(MathNode::row(self.converted_children()?)), index: None }),
            "mroot" => {
                let [radicand, index] = self.exactly()?;
                Ok(MathNode::Root { radicand: Box::new(radicand), index: Some(Box::new(index)) })
            }
            "mtable" | "table" => {
                let rows = self.elements().map(|row| row.converted_children()).collect::<Result<_, _>>()?;
                let column_align = match self.option("columnalign") {
                    Some(spec) => spec.split_whitespace().map(str::parse::<ColumnAlign>).collect::<Result<_, _>>()?,
                    None => Vec::new(),
                };
                Ok(MathNode::Table(Table {
                    rows,
                    column_align,
                    row_spacing: self.length("rowspacing")?,
                    column_spacing: self.length("columnspacing")?,
                    display_style: self.option("displaystyle") != Some("false"),
                }))
            }
            "mpadded" => Ok(MathNode::Padded(Padded {
                content: Box::new(MathNode::row(self.converted_children()?)),
                width: self.length("width")?,
                height: self.length("height")?,
                depth: self.length("depth")?,
                lspace: self.length("lspace")?,
                voffset: self.length("voffset")?,
            })),
            "menclose" => Ok(MathNode::Enclose(Enclose {
                content: Box::new(MathNode::row(self.converted_children()?)),
                notations: self.option("notations").unwrap_or("").split_whitespace().map(str::parse::<Notation>).collect::<Result<_, _>>()?,
                line_thickness: self.dimen("linethickness")?,
            })),
            other => Err(format!("Unknown math command {other}")),
        }
    }

    fn text_raw(&self) -> Result<String, String> {
        match self.children.first() {
            Some(Content::Text(t)) => Ok(t.clone()),
            None => Ok(String::new()),
            Some(Content::Element(_)) => Err(format!("{} command contains content which is not text", self.command)),
        }
    }
}

fn parse_bool(v: &str) -> Result<bool, String> {
    match v {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(format!("expected a boolean, got {v}")),
    }
}
