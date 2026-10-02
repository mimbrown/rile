//! Parser for SILE's debug outputter format (`tests/*.expected`), producing
//! absolute positions for every glyph run and rule.

#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub paper: (f64, f64),
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub runs: Vec<Run>,
    pub rules: Vec<Rule>,
    pub figures: Vec<Figure>,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub x: f64,
    /// Baseline, measured down from the top of the page.
    pub y: f64,
    pub family: String,
    pub size: f64,
    pub weight: u16,
    pub italic: bool,
    pub gids: Vec<u32>,
    /// Per-glyph advances when SILE printed them; otherwise derived from `width`.
    pub advances: Option<Vec<f64>>,
    pub width: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub depth: f64,
}

/// An SVG drawing: SILE's debug output gives its baseline, size and PDF
/// operators but not where it starts across the line.
#[derive(Debug, Clone)]
pub struct Figure {
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub ops: String,
}

pub fn parse(src: &str) -> Trace {
    let mut trace = Trace {
        pages: vec![Page::default()],
        ..Default::default()
    };
    let (mut x, mut y) = (0.0, 0.0);
    let (mut family, mut size, mut weight, mut italic) = (String::new(), 10.0, 400, false);
    // Mx values seen since the last glyph run: SILE prints absolute moves
    // before a run, but relative per-glyph moves before a "complex" run.
    let mut pending_x: Vec<f64> = Vec::new();

    for line in src.lines() {
        let mut fields = line.split('\t');
        let op = fields.next().unwrap_or("").trim_end();
        let rest: Vec<&str> = fields.collect();
        match op {
            "Set paper size" => {
                let w = num(rest.first());
                let h = num(rest.get(1));
                trace.paper = (w, h);
            }
            "New page" => {
                trace.pages.push(Page::default());
                pending_x.clear();
            }
            "Mx" => pending_x.push(num(rest.first())),
            "My" => {
                if let Some(px) = pending_x.pop() {
                    x = px;
                }
                pending_x.clear();
                y = num(rest.first());
            }
            "Set font" => {
                let key = rest.first().copied().unwrap_or("");
                let parts: Vec<&str> = key.split(';').collect();
                family = parts.first().copied().unwrap_or("").to_string();
                size = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(10.0);
                weight = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(400);
                italic = parts.iter().any(|p| p.eq_ignore_ascii_case("italic"));
            }
            "T" => {
                let glyphs = rest.first().copied().unwrap_or("");
                let text = rest
                    .get(1)
                    .map(|t| {
                        t.strip_prefix('(')
                            .and_then(|t| t.strip_suffix(')'))
                            .unwrap_or(t)
                    })
                    .unwrap_or("")
                    .to_string();
                let mut gids = Vec::new();
                let mut advances = Vec::new();
                let mut width = None;
                let mut complex = false;
                for tok in glyphs.split(' ').filter(|t| !t.is_empty()) {
                    if let Some(w) = tok.strip_prefix("w=") {
                        width = w.parse().ok();
                    } else if let Some(a) = tok.strip_prefix("a=") {
                        complex = true;
                        if let Some(last) = advances.last_mut() {
                            *last = a.parse().unwrap_or(0.0);
                        }
                    } else if tok.starts_with("x=") || tok.starts_with("y=") {
                        complex = true;
                    } else if let Ok(g) = tok.parse() {
                        gids.push(g);
                        advances.push(0.0);
                    }
                }
                let start = if complex {
                    let relative = advances
                        .iter()
                        .filter(|a| **a != 0.0)
                        .count()
                        .min(pending_x.len());
                    let absolute = pending_x.len() - relative;
                    let start = if absolute > 0 {
                        pending_x[absolute - 1]
                    } else {
                        x
                    };
                    let moved: f64 = pending_x[absolute..].iter().sum();
                    x = start + moved;
                    start
                } else {
                    if let Some(px) = pending_x.last() {
                        x = *px;
                    }
                    x
                };
                pending_x.clear();
                let width = width.unwrap_or_else(|| advances.iter().sum());
                trace.pages.last_mut().unwrap().runs.push(Run {
                    x: start,
                    y,
                    family: family.clone(),
                    size,
                    weight,
                    italic,
                    gids,
                    advances: complex.then_some(advances),
                    width,
                    text,
                });
            }
            "Draw line" => {
                let v: Vec<f64> = rest
                    .iter()
                    .map(|s| s.trim().parse().unwrap_or(0.0))
                    .collect();
                if v.len() >= 4 {
                    trace.pages.last_mut().unwrap().rules.push(Rule {
                        x: v[0],
                        y: v[1],
                        width: v[2],
                        depth: v[3],
                    });
                }
            }
            "Draw SVG" => {
                trace.pages.last_mut().unwrap().figures.push(Figure {
                    y: num(rest.first()),
                    width: num(rest.get(1)),
                    height: num(rest.get(2)),
                    ops: rest.get(4).map_or("", |s| s.trim()).to_string(),
                });
            }
            _ => {}
        }
    }
    if trace.pages.len() > 1
        && trace
            .pages
            .last()
            .is_some_and(|p| p.runs.is_empty() && p.rules.is_empty() && p.figures.is_empty())
    {
        trace.pages.pop();
    }
    trace
}

fn num(s: Option<&&str>) -> f64 {
    s.and_then(|s| s.trim().parse().ok()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &str = "Set paper size \t430.8661464\t649.1338653\nBegin page\nMx \t35.7619\nMy \t82.8679\nSet font \tGentium Plus;10;400;;normal;;;LTR\nT\t90 75 82 w=17.6074\t(who)\nMx \t57.5624\nT\t76 86 w=6.5723\t(is)\nEnd page\nFinish\n";

    #[test]
    fn simple_runs_get_absolute_positions() {
        let t = parse(SIMPLE);
        assert_eq!(t.paper, (430.8661464, 649.1338653));
        let runs = &t.pages[0].runs;
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].x, runs[0].y), (35.7619, 82.8679));
        assert_eq!(runs[0].gids, vec![90, 75, 82]);
        assert_eq!(runs[0].family, "Gentium Plus");
        assert_eq!((runs[1].x, runs[1].text.as_str()), (57.5624, "is"));
    }

    #[test]
    fn complex_runs_consume_relative_moves() {
        let src = "Begin page\nMx \t22.4882\nMy \t22.4502\nSet font \tGentium Book;10;400;;normal;;;LTR;\nMx \t5.6689\nMx \t4.6924\nT\t37 a=5.6689 72 a=4.6924\t(Be)\nMx \t52.0399\nSet font \tGentium Book;10;400;Italic;normal;;;LTR;\nT\t71 w=5.0\t(d)\nEnd page\n";
        let t = parse(src);
        let runs = &t.pages[0].runs;
        assert_eq!(runs[0].x, 22.4882);
        assert_eq!(runs[0].advances, Some(vec![5.6689, 4.6924]));
        assert_eq!(runs[1].x, 52.0399);
        assert!(runs[1].italic);
    }
}
