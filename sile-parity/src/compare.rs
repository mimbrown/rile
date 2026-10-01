//! Fuzzy, tiered comparison of two traces. Each tier scores 0..=1 so a test
//! can be "close" without being identical.

use crate::trace::{Run, Trace};

/// Positions closer than this are considered the same.
pub const TOLERANCE_PT: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Match,
    Close,
    Differs,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Match => "match",
            Status::Close => "close",
            Status::Differs => "differs",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Comparison {
    /// Same glyphs in the same order.
    pub content: f64,
    /// Same words ending each line, and same lines on each page.
    pub breaks: f64,
    /// Share of matched runs placed within `TOLERANCE_PT`.
    pub geometry: f64,
    pub max_offset: f64,
    pub pages: (usize, usize),
    pub status: Status,
}

impl Comparison {
    pub fn score(&self) -> f64 {
        (self.content + self.breaks + self.geometry) / 3.0
    }
}

pub fn compare(expected: &Trace, actual: &Trace) -> Comparison {
    let pages = (expected.pages.len(), actual.pages.len());
    let (mut exp_total, mut act_total, mut common, mut within) = (0, 0, 0, 0);
    let mut max_offset: f64 = 0.0;
    for p in 0..pages.0.max(pages.1) {
        let e = expected.pages.get(p).map(glyphs).unwrap_or_default();
        let a = actual.pages.get(p).map(glyphs).unwrap_or_default();
        exp_total += e.len();
        act_total += a.len();
        let ek: Vec<u32> = e.iter().map(|g| g.gid).collect();
        let ak: Vec<u32> = a.iter().map(|g| g.gid).collect();
        let pairs = lcs_pairs(&ek, &ak);
        common += pairs.len();
        for (i, j) in pairs {
            let offset = (e[i].x - a[j].x).abs().max((e[i].y - a[j].y).abs());
            if offset <= TOLERANCE_PT {
                within += 1;
            }
            max_offset = max_offset.max(offset);
        }
    }
    let content = ratio(common, exp_total, act_total);
    let geometry = if exp_total + act_total == 0 {
        1.0
    } else {
        within as f64 / exp_total.max(act_total) as f64
    };

    let exp_lines = line_keys(expected);
    let act_lines = line_keys(actual);
    let breaks = ratio(
        lcs_pairs(&exp_lines, &act_lines).len(),
        exp_lines.len(),
        act_lines.len(),
    );

    let status = if content == 1.0 && breaks == 1.0 && geometry >= 0.99 && pages.0 == pages.1 {
        Status::Match
    } else if content >= 0.95 && breaks >= 0.8 {
        Status::Close
    } else {
        Status::Differs
    };
    Comparison {
        content,
        breaks,
        geometry,
        max_offset,
        pages,
        status,
    }
}

struct Glyph {
    gid: u32,
    x: f64,
    y: f64,
}

/// Every glyph on the page with its pen position. Runs without per-glyph
/// advances spread their width evenly, which is exact for the first glyph and
/// close enough for the rest at the tolerance used.
fn glyphs(page: &crate::trace::Page) -> Vec<Glyph> {
    let mut out = Vec::new();
    for run in &page.runs {
        let n = run.gids.len().max(1) as f64;
        let mut x = run.x;
        for (i, gid) in run.gids.iter().enumerate() {
            out.push(Glyph {
                gid: *gid,
                x,
                y: run.y,
            });
            x += match &run.advances {
                Some(a) => a.get(i).copied().unwrap_or(0.0),
                None => run.width / n,
            };
        }
    }
    out
}

/// One key per line: its page and its glyphs, so line and page breaks count
/// regardless of how either side splits words into runs.
fn line_keys(trace: &Trace) -> Vec<String> {
    let mut out = Vec::new();
    for (p, page) in trace.pages.iter().enumerate() {
        let mut lines: Vec<(f64, Vec<&Run>)> = Vec::new();
        for run in &page.runs {
            match lines.iter_mut().find(|(y, _)| (y - run.y).abs() < 0.01) {
                Some((_, runs)) => runs.push(run),
                None => lines.push((run.y, vec![run])),
            }
        }
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, mut runs) in lines {
            runs.sort_by(|a, b| a.x.total_cmp(&b.x));
            let gids: Vec<String> = runs
                .iter()
                .flat_map(|r| r.gids.iter().map(u32::to_string))
                .collect();
            out.push(format!("{p}:{}", gids.join(" ")));
        }
    }
    out
}

fn ratio(common: usize, a: usize, b: usize) -> f64 {
    if a + b == 0 {
        1.0
    } else {
        2.0 * common as f64 / (a + b) as f64
    }
}

/// Indices of a longest common subsequence of `a` and `b`.
pub fn lcs_pairs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let idx = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[idx(i, j)] = if a[i] == b[j] {
                table[idx(i + 1, j + 1)] + 1
            } else {
                table[idx(i + 1, j)].max(table[idx(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table[idx(i + 1, j)] >= table[idx(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::parse;

    const A: &str = "Begin page\nMx \t10\nMy \t20\nSet font \tX;10;400;;normal;;;LTR\nT\t1 2 w=5\t(ab)\nMx \t20\nT\t3 w=5\t(c)\nMx \t10\nMy \t32\nT\t4 w=5\t(d)\nEnd page\n";

    #[test]
    fn identical_traces_match() {
        let c = compare(&parse(A), &parse(A));
        assert_eq!(c.status, Status::Match);
        assert_eq!(c.score(), 1.0);
    }

    #[test]
    fn small_shift_is_still_a_match_but_a_rebreak_is_not() {
        let shifted = A.replace("Mx \t20", "Mx \t20.3");
        assert_eq!(compare(&parse(A), &parse(&shifted)).status, Status::Match);

        let rebroken = A.replace("Mx \t20\nT\t3", "Mx \t10\nMy \t26\nT\t3");
        let c = compare(&parse(A), &parse(&rebroken));
        assert_eq!(c.content, 1.0);
        assert!(c.breaks < 1.0);
        assert_ne!(c.status, Status::Match);
    }

    #[test]
    fn lcs_finds_common_subsequence() {
        assert_eq!(lcs_pairs(&[1, 2, 3, 4], &[2, 4, 5]), vec![(1, 0), (3, 1)]);
    }
}
