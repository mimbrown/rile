//! A reader for the subset of SILE's SIL markup used by its regression tests.
//! It only builds a tree; `driver` decides what each command means.

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Command(Command),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub options: Vec<(String, String)>,
    /// `None` for a bare `\cmd`, `Some` for `\cmd{...}` and environments.
    pub content: Option<Vec<Content>>,
    /// Verbatim body of raw environments such as `lua` and `raw`.
    pub raw: Option<String>,
}

impl Command {
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Commands and environments whose body SILE hands to another language
/// (Lua, TeX-like math) instead of parsing it as SIL.
const RAW_ENVIRONMENTS: &[&str] = &[
    "lua",
    "raw",
    "script",
    "use",
    "sil",
    "xml",
    "verbatim:raw",
    "math",
    "ftl",
    "include",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

pub fn parse(src: &str) -> Result<Vec<Content>, ParseError> {
    let mut p = Parser {
        s: src.as_bytes(),
        src,
        i: 0,
    };
    let out = p.content(None)?;
    if p.i < p.s.len() {
        return Err(ParseError(format!("unexpected '}}' at byte {}", p.i)));
    }
    Ok(out)
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn content(&mut self, env: Option<&str>) -> Result<Vec<Content>, ParseError> {
        let mut out = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    if let Some(env) = env {
                        return Err(ParseError(format!("unterminated environment '{env}'")));
                    }
                    break;
                }
                Some(b'}') if env.is_none() => break,
                Some(b'}') => {
                    return Err(ParseError(format!(
                        "stray '}}' inside '{}'",
                        env.unwrap_or("")
                    )));
                }
                Some(b'%') => self.comment(),
                Some(b'\\') => {
                    let next = self.s.get(self.i + 1).copied();
                    if matches!(next, Some(b'\\' | b'{' | b'}' | b'%')) {
                        text.push(next.unwrap_or(b'\\') as char);
                        self.i += 2;
                        continue;
                    }
                    let start = self.i;
                    let cmd = self.command()?;
                    if cmd.name == "end" {
                        let name = cmd.content.as_deref().map(plain_text).unwrap_or_default();
                        if env == Some(name.as_str()) {
                            flush(&mut text, &mut out);
                            return Ok(out);
                        }
                        self.i = start;
                        return Err(ParseError(format!(
                            "\\end{{{name}}} without matching \\begin"
                        )));
                    }
                    flush(&mut text, &mut out);
                    out.push(Content::Command(cmd));
                }
                Some(_) => {
                    let start = self.i;
                    while let Some(c) = self.peek() {
                        if matches!(c, b'\\' | b'{' | b'}' | b'%') {
                            break;
                        }
                        self.i += 1;
                    }
                    if self.i == start {
                        // A bare `{` group: SILE treats it as plain content.
                        self.i += 1;
                        let inner = self.content(None)?;
                        self.expect(b'}')?;
                        flush(&mut text, &mut out);
                        out.extend(inner);
                    } else {
                        text.push_str(&self.src[start..self.i]);
                    }
                }
            }
        }
        flush(&mut text, &mut out);
        Ok(out)
    }

    fn comment(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == b'\n' {
                break;
            }
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), ParseError> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(ParseError(format!(
                "expected '{}' at byte {}",
                c as char, self.i
            )))
        }
    }

    fn command(&mut self) -> Result<Command, ParseError> {
        self.i += 1; // backslash
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric()
                || matches!(c, b':' | b'-' | b'_' | b'.' | b'@')
                || c >= 0x80
            {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start {
            // Single-character command such as `\,` or `\ `.
            let ch = self.src[self.i..].chars().next().unwrap_or(' ');
            self.i += ch.len_utf8();
            return Ok(Command {
                name: ch.to_string(),
                options: vec![],
                content: None,
                raw: None,
            });
        }
        let name = self.src[start..self.i].to_string();
        let mut options = if self.peek() == Some(b'[') {
            self.options()?
        } else {
            vec![]
        };

        if name == "begin" {
            if self.peek() == Some(b'[') && options.is_empty() {
                options = self.options()?;
            }
            self.expect(b'{')?;
            let env_start = self.i;
            while self.peek().is_some_and(|c| c != b'}') {
                self.i += 1;
            }
            let env = self.src[env_start..self.i].trim().to_string();
            self.expect(b'}')?;
            if self.peek() == Some(b'[') && options.is_empty() {
                options = self.options()?;
            }
            if RAW_ENVIRONMENTS.contains(&env.as_str()) {
                let end = format!("\\end{{{env}}}");
                let rest = &self.src[self.i..];
                let len = rest
                    .find(&end)
                    .ok_or_else(|| ParseError(format!("unterminated raw environment '{env}'")))?;
                let raw = rest[..len].to_string();
                self.i += len + end.len();
                return Ok(Command {
                    name: env,
                    options,
                    content: None,
                    raw: Some(raw),
                });
            }
            let content = self.content(Some(&env))?;
            return Ok(Command {
                name: env,
                options,
                content: Some(content),
                raw: None,
            });
        }

        if RAW_ENVIRONMENTS.contains(&name.as_str()) && self.peek() == Some(b'{') {
            let raw = self.balanced_raw()?;
            return Ok(Command {
                name,
                options,
                content: None,
                raw: Some(raw),
            });
        }

        let content = if self.peek() == Some(b'{') {
            self.i += 1;
            let c = self.content(None)?;
            self.expect(b'}')?;
            Some(c)
        } else {
            None
        };
        Ok(Command {
            name,
            options,
            content,
            raw: None,
        })
    }

    fn balanced_raw(&mut self) -> Result<String, ParseError> {
        self.expect(b'{')?;
        let start = self.i;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                b'\\' => self.i += 1,
                b'{' => depth += 1,
                b'}' if depth == 0 => {
                    let raw = self.src[start..self.i].to_string();
                    self.i += 1;
                    return Ok(raw);
                }
                b'}' => depth -= 1,
                _ => {}
            }
            self.i += 1;
        }
        Err(ParseError("unterminated raw argument".into()))
    }

    fn options(&mut self) -> Result<Vec<(String, String)>, ParseError> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        loop {
            while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.i += 1;
            }
            match self.peek() {
                Some(b']') => {
                    self.i += 1;
                    return Ok(out);
                }
                None => return Err(ParseError("unterminated options".into())),
                _ => {}
            }
            let key_start = self.i;
            while self
                .peek()
                .is_some_and(|c| !matches!(c, b'=' | b',' | b';' | b']'))
            {
                self.i += 1;
            }
            let key = self.src[key_start..self.i].trim().to_string();
            let mut value = String::new();
            if self.peek() == Some(b'=') {
                self.i += 1;
                while self.peek().is_some_and(|c| c == b' ') {
                    self.i += 1;
                }
                if self.peek() == Some(b'"') {
                    self.i += 1;
                    let v_start = self.i;
                    while self.peek().is_some_and(|c| c != b'"') {
                        self.i += 1;
                    }
                    value = self.src[v_start..self.i].to_string();
                    self.expect(b'"')?;
                } else {
                    let v_start = self.i;
                    let mut depth = 0;
                    while let Some(c) = self.peek() {
                        match c {
                            b'[' | b'{' => depth += 1,
                            b']' | b'}' if depth > 0 => depth -= 1,
                            b',' | b';' | b']' if depth == 0 => break,
                            _ => {}
                        }
                        self.i += 1;
                    }
                    value = self.src[v_start..self.i].trim().to_string();
                }
            }
            if matches!(self.peek(), Some(b',' | b';')) {
                self.i += 1;
            }
            out.push((key, value));
        }
    }
}

fn flush(text: &mut String, out: &mut Vec<Content>) {
    if !text.is_empty() {
        out.push(Content::Text(std::mem::take(text)));
    }
}

pub fn plain_text(content: &[Content]) -> String {
    content
        .iter()
        .map(|c| match c {
            Content::Text(t) => t.clone(),
            Content::Command(c) => c.content.as_deref().map(plain_text).unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(c: &Content) -> &Command {
        match c {
            Content::Command(c) => c,
            other => panic!("expected command, got {other:?}"),
        }
    }

    #[test]
    fn document_with_options_and_comment() {
        let tree =
            parse("\\begin[papersize=a7]{document}\n% note\nHello \\em{world}.\n\\end{document}\n")
                .unwrap();
        let doc = cmd(&tree[0]);
        assert_eq!(doc.name, "document");
        assert_eq!(doc.option("papersize"), Some("a7"));
        let body = doc.content.as_ref().unwrap();
        assert_eq!(body[0], Content::Text("\nHello ".into()));
        assert_eq!(cmd(&body[1]).name, "em");
        assert_eq!(body[2], Content::Text(".\n".into()));
    }

    #[test]
    fn options_with_quotes_and_spaces() {
        let tree = parse("\\font[family=\"Gentium Plus\", size=12pt]{x}").unwrap();
        let font = cmd(&tree[0]);
        assert_eq!(font.option("family"), Some("Gentium Plus"));
        assert_eq!(font.option("size"), Some("12pt"));
    }

    #[test]
    fn raw_environment_is_verbatim() {
        let tree = parse("\\begin{lua}\nfor i=1,2 do x{} end\n\\end{lua}").unwrap();
        let lua = cmd(&tree[0]);
        assert_eq!(lua.name, "lua");
        assert!(lua.raw.as_ref().unwrap().contains("x{}"));
    }

    #[test]
    fn raw_command_keeps_percent_and_braces() {
        let tree = parse("\\lua{x = { s = \"{}\" } % 5} after").unwrap();
        assert_eq!(cmd(&tree[0]).raw.as_deref(), Some("x = { s = \"{}\" } % 5"));
        assert_eq!(tree[1], Content::Text(" after".into()));
    }

    #[test]
    fn escapes_and_single_char_commands() {
        let tree = parse("a\\%b\\,c").unwrap();
        assert_eq!(tree[0], Content::Text("a%b".into()));
        assert_eq!(cmd(&tree[1]).name, ",");
    }
}
