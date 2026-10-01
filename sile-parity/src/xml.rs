//! SILE's XML input format, read into the same tree as SIL: every element is
//! a command whose attributes are its options.

use crate::sil::{Command, Content, ParseError};

pub fn parse(src: &str) -> Result<Vec<Content>, ParseError> {
    let mut p = Parser { src, pos: 0 };
    let mut root = p.children(None)?;
    for c in &mut root {
        if let Content::Command(cmd) = c
            && cmd.name == "sile"
        {
            cmd.name = "document".into();
        }
    }
    Ok(root)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn err<T>(&self, msg: &str) -> Result<T, ParseError> {
        Err(ParseError(format!("{msg} at byte {}", self.pos)))
    }

    fn skip_past(&mut self, end: &str) -> Result<(), ParseError> {
        match self.rest().find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => self.err(&format!("unterminated, expected {end}")),
        }
    }

    /// Content up to the closing tag of `parent` (or the end of input).
    fn children(&mut self, parent: Option<&str>) -> Result<Vec<Content>, ParseError> {
        let mut out = Vec::new();
        loop {
            let rest = self.rest();
            if rest.is_empty() {
                return match parent {
                    Some(p) => self.err(&format!("unclosed <{p}>")),
                    None => Ok(out),
                };
            }
            if rest.starts_with("<!--") {
                self.skip_past("-->")?;
            } else if rest.starts_with("<?") || rest.starts_with("<!") {
                self.skip_past(">")?;
            } else if let Some(close) = rest.strip_prefix("</") {
                let name_len = close.find('>').unwrap_or(close.len());
                let name = close[..name_len].trim();
                if Some(name) != parent {
                    return self.err(&format!("unexpected </{name}>"));
                }
                self.pos += 2 + name_len + 1;
                return Ok(out);
            } else if rest.starts_with('<') {
                out.push(Content::Command(self.element()?));
            } else {
                let end = rest.find('<').unwrap_or(rest.len());
                out.push(Content::Text(unescape(&rest[..end])));
                self.pos += end;
            }
        }
    }

    fn element(&mut self) -> Result<Command, ParseError> {
        self.pos += 1;
        let name = self.name();
        let mut options = Vec::new();
        loop {
            self.skip_ws();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.pos += 2;
                return Ok(Command {
                    name,
                    options,
                    content: None,
                    raw: None,
                });
            }
            if rest.starts_with('>') {
                self.pos += 1;
                let content = self.children(Some(&name))?;
                return Ok(Command {
                    name,
                    options,
                    content: Some(content),
                    raw: None,
                });
            }
            let key = self.name();
            if key.is_empty() {
                return self.err("bad attribute");
            }
            self.skip_ws();
            if !self.rest().starts_with('=') {
                return self.err("expected =");
            }
            self.pos += 1;
            self.skip_ws();
            let Some(quote) = self
                .rest()
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
            else {
                return self.err("expected quoted value");
            };
            self.pos += 1;
            let len = self
                .rest()
                .find(quote)
                .ok_or(ParseError("unterminated value".into()))?;
            options.push((key, unescape(&self.rest()[..len])));
            self.pos += len + 1;
        }
    }

    fn name(&mut self) -> String {
        let rest = self.rest();
        let len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '>' | '/' | '='))
            .unwrap_or(rest.len());
        let name = rest[..len].to_string();
        self.pos += len;
        name
    }

    fn skip_ws(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start().len();
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e => e
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| e.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_become_commands() {
        let tree =
            parse("<!-- c --><sile papersize=\"a6\"><font size='12pt'/>a &amp; b</sile>").unwrap();
        let [Content::Command(doc)] = tree.as_slice() else {
            panic!("{tree:?}")
        };
        assert_eq!(doc.name, "document");
        assert_eq!(doc.option("papersize"), Some("a6"));
        let content = doc.content.as_ref().unwrap();
        let Content::Command(font) = &content[0] else {
            panic!()
        };
        assert_eq!((font.name.as_str(), font.content.is_none()), ("font", true));
        assert_eq!(content[1], Content::Text("a & b".into()));
    }

    #[test]
    fn mismatched_tags_are_errors() {
        assert!(parse("<a><b></a>").is_err());
    }
}
