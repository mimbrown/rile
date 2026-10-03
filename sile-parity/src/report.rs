use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::compare::Status;
use crate::fonts::Fonts;
use crate::known::{self, Settled};
use crate::svg::{self, GlyphDefs, escape};
use crate::{Outcome, TestResult};

#[derive(Default)]
struct Tally {
    matched: usize,
    close: usize,
    differs: usize,
    unsupported: usize,
    errors: usize,
    explained: usize,
    skipped: usize,
}

fn tally(results: &[TestResult]) -> Tally {
    let mut t = Tally::default();
    for r in results {
        match (&r.outcome, r.settled) {
            (_, Some(Settled::Explained(_))) => t.explained += 1,
            (_, Some(Settled::Skipped(_))) => t.skipped += 1,
            (Outcome::Compared { comparison, .. }, None) => match comparison.status {
                Status::Match => t.matched += 1,
                Status::Close => t.close += 1,
                Status::Differs => t.differs += 1,
            },
            (Outcome::Unsupported(_), None) => t.unsupported += 1,
            (Outcome::Error(_), None) => t.errors += 1,
        }
    }
    t
}

/// The status a test is reported under: settled tests leave the open buckets.
fn bucket(r: &TestResult) -> &'static str {
    match (&r.outcome, r.settled) {
        (_, Some(s)) => s.label(),
        (Outcome::Compared { comparison, .. }, None) => comparison.status.label(),
        (Outcome::Error(_), None) => "error",
        (Outcome::Unsupported(_), None) => "unsupported",
    }
}

/// Features ranked by how many tests they block on their own or with others.
fn blockers(results: &[TestResult]) -> Vec<(String, usize, usize)> {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for r in results.iter().filter(|r| r.settled.is_none()) {
        if let Outcome::Unsupported(missing) = &r.outcome {
            for m in missing {
                let e = counts.entry(m.as_str()).or_default();
                e.0 += 1;
                if missing.len() == 1 {
                    e.1 += 1;
                }
            }
        }
    }
    let mut v: Vec<_> = counts
        .into_iter()
        .map(|(k, (n, only))| (k.to_string(), n, only))
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
    v
}

pub fn print_summary(results: &[TestResult]) {
    println!(
        "{:<40} {:<12} {:>8} {:>8} {:>8} {:>7}",
        "test", "status", "content", "breaks", "geometry", "pages"
    );
    for r in results {
        match &r.outcome {
            Outcome::Compared { comparison: c, .. } => println!(
                "{:<40} {:<12} {:>7.0}% {:>7.0}% {:>7.0}% {:>3}/{:<3}",
                r.name,
                bucket(r),
                c.content * 100.0,
                c.breaks * 100.0,
                c.geometry * 100.0,
                c.pages.0,
                c.pages.1
            ),
            Outcome::Error(e) => println!("{:<40} {:<12} {}", r.name, "error", first_line(e)),
            Outcome::Unsupported(_) => {}
        }
    }
    for r in results {
        if let Outcome::Compared { comparison, .. } = &r.outcome
            && comparison.status == Status::Match
            && known::divergence(&r.name).is_some()
        {
            println!("note: {} matches now; drop it from known::divergence", r.name);
        }
    }
    let t = tally(results);
    println!(
        "\n{} tests: {} match, {} close, {} differ, {} error, {} unsupported, {} explained, {} skipped",
        results.len(),
        t.matched,
        t.close,
        t.differs,
        t.errors,
        t.unsupported,
        t.explained,
        t.skipped
    );
    println!("\ntop blockers (tests blocked only by this / tests needing it):");
    for (feature, n, only) in blockers(results).iter().take(15) {
        println!("  {only:>3} / {n:<3} {feature}");
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

pub fn write_html(results: &[TestResult], fonts: &Fonts, out: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(out.join("tests"))?;
    std::fs::write(out.join("style.css"), STYLE)?;
    for r in results {
        if let Outcome::Compared { .. } = r.outcome {
            std::fs::write(
                out.join("tests").join(format!("{}.html", r.name)),
                detail(r, fonts),
            )?;
        }
    }
    let index = out.join("index.html");
    std::fs::write(&index, index_page(results))?;
    Ok(index)
}

fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

fn index_page(results: &[TestResult]) -> String {
    let t = tally(results);
    let total = results.len().max(1);
    let mut h = String::new();
    let _ = write!(
        h,
        "<title>SILE Parity</title>\n<link rel='stylesheet' href='https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600&family=Source+Serif+4:opsz,wght@8..60,500;8..60,600&display=swap'>\n<link rel='stylesheet' href='style.css'>\n<main class='wrap'>\n<header class='masthead'><p class='eyebrow'>rile · regression corpus</p><h1>SILE Parity</h1>\
<p class='lede'>Each of SILE's regression tests is typeset by rile and compared with SILE's expected output in three tiers: the same glyphs, the same line and page breaks, and glyph positions within {tol}pt.</p></header>\n",
        tol = crate::compare::TOLERANCE_PT
    );

    let seg = |n: usize, class: &str, label: &str| {
        if n == 0 {
            String::new()
        } else {
            format!("<span class='seg {class}' style='flex:{n}' title='{n} {label}'></span>")
        }
    };
    let _ = write!(
        h,
        "<section class='summary'><div class='bar'>{}{}{}{}{}{}{}</div><dl class='counts'>\
<div><dt><i class='dot match'></i>Match</dt><dd>{}</dd></div>\
<div><dt><i class='dot close'></i>Close</dt><dd>{}</dd></div>\
<div><dt><i class='dot differs'></i>Differs</dt><dd>{}</dd></div>\
<div><dt><i class='dot error'></i>Error</dt><dd>{}</dd></div>\
<div><dt><i class='dot unsupported'></i>Not yet runnable</dt><dd>{}</dd></div>\
<div><dt><i class='dot explained'></i>Explained</dt><dd>{}</dd></div>\
<div><dt><i class='dot skipped'></i>Skipped</dt><dd>{}</dd></div>\
<div><dt>Total</dt><dd>{}</dd></div></dl></section>",
        seg(t.matched, "match", "match"),
        seg(t.close, "close", "close"),
        seg(t.differs, "differs", "differ"),
        seg(t.errors, "error", "errors"),
        seg(t.unsupported, "unsupported", "not runnable"),
        seg(t.explained, "explained", "explained"),
        seg(t.skipped, "skipped", "skipped"),
        t.matched,
        t.close,
        t.differs,
        t.errors,
        t.unsupported,
        t.explained,
        t.skipped,
        total
    );
    h.push('\n');

    h.push_str("<section><h2>What blocks the rest</h2><p class='note'>Features the test reader or engine does not support yet. <b>Only blocker</b> counts tests that would run once this one feature lands.</p><div class='scroll'><table class='blockers'><thead><tr><th>Feature</th><th class='num'>Only blocker</th><th class='num'>Tests needing it</th></tr></thead><tbody>");
    for (feature, n, only) in blockers(results).iter().take(20) {
        let _ = write!(
            h,
            "<tr><td><code>{}</code></td><td class='num'>{only}</td><td class='num'>{n}</td></tr>",
            escape(feature)
        );
    }
    h.push_str("</tbody></table></div></section>\n");

    h.push_str("<section><h2>Tests</h2><div class='filters' role='group' aria-label='Filter by status'>\
<button type='button' class='chip on' data-f='all'>All</button><button type='button' class='chip' data-f='match'>Match</button>\
<button type='button' class='chip' data-f='close'>Close</button><button type='button' class='chip' data-f='differs'>Differs</button>\
<button type='button' class='chip' data-f='error'>Error</button><button type='button' class='chip' data-f='unsupported'>Not yet runnable</button>\
<button type='button' class='chip' data-f='explained'>Explained</button><button type='button' class='chip' data-f='skipped'>Skipped</button></div>\
<div class='scroll'><table class='tests'><thead><tr><th>Test</th><th>Status</th><th class='num'>Glyphs</th><th class='num'>Breaks</th><th class='num'>Positions</th><th class='num'>Pages SILE/ours</th><th>Notes</th></tr></thead><tbody>");
    let mut rows: Vec<&TestResult> = results.iter().collect();
    rows.sort_by_key(|r| (order(r), r.name.clone()));
    for r in rows {
        let s = bucket(r);
        match (&r.outcome, r.settled) {
            (Outcome::Compared { comparison: c, .. }, settled) => {
                let note = settled.map_or_else(|| format!("max offset {:.1}pt", c.max_offset), |k| escape(k.reason()));
                let _ = write!(
                    h,
                    "<tr data-s='{s}'><td><a href='tests/{n}.html'>{n}</a></td><td><span class='pill {s}'>{s}</span></td><td class='num'>{}</td><td class='num'>{}</td><td class='num'>{}</td><td class='num'>{}/{}</td><td class='muted'>{note}</td></tr>",
                    pct(c.content),
                    pct(c.breaks),
                    pct(c.geometry),
                    c.pages.0,
                    c.pages.1,
                    n = escape(&r.name)
                );
            }
            (Outcome::Unsupported(_), Some(k)) => {
                let _ = write!(
                    h,
                    "<tr data-s='{s}'><td>{}</td><td><span class='pill {s}'>{s}</span></td><td colspan='4'></td><td class='muted'>{}</td></tr>",
                    escape(&r.name),
                    escape(k.reason())
                );
            }
            (Outcome::Error(e), _) => {
                let _ = write!(
                    h,
                    "<tr data-s='error'><td>{}</td><td><span class='pill error'>error</span></td><td colspan='4'></td><td class='muted'>{}</td></tr>",
                    escape(&r.name),
                    escape(first_line(e))
                );
            }
            (Outcome::Unsupported(m), None) => {
                let _ = write!(
                    h,
                    "<tr data-s='unsupported'><td>{}</td><td><span class='pill unsupported'>not yet</span></td><td colspan='4'></td><td class='muted'>needs {}</td></tr>",
                    escape(&r.name),
                    escape(&m.join(", "))
                );
            }
        }
    }
    h.push_str("</tbody></table></div></section></main>\n<script>\nconst chips=document.querySelectorAll('.chip');chips.forEach(c=>c.addEventListener('click',()=>{chips.forEach(x=>x.classList.toggle('on',x===c));const f=c.dataset.f;document.querySelectorAll('tr[data-s]').forEach(r=>{r.hidden=!(f==='all'||r.dataset.s===f);});}));\n</script>\n");
    h
}

fn order(r: &TestResult) -> u8 {
    match bucket(r) {
        "differs" => 0,
        "close" => 1,
        "match" => 2,
        "error" => 3,
        "unsupported" => 4,
        "explained" => 5,
        _ => 6,
    }
}

fn detail(r: &TestResult, fonts: &Fonts) -> String {
    let Outcome::Compared {
        ours,
        comparison: c,
    } = &r.outcome
    else {
        return String::new();
    };
    let mut defs = GlyphDefs::new(fonts);
    let mut body = String::new();
    let pages = r.expected.pages.len().max(ours.pages.len());
    for i in 0..pages {
        let e = r.expected.pages.get(i);
        let o = ours.pages.get(i);
        let paper = if e.is_some() { &r.expected } else { ours };
        let vb = svg::view_box(paper, &[e, o]);
        let missing = || "<p class='missing'>No page</p>".to_string();
        let _ = write!(
            body,
            "<section class='spread'><h2>Page {}</h2><div class='panes'><figure><figcaption>SILE</figcaption>{}</figure><figure><figcaption>rile</figcaption>{}</figure></div>\
<figure class='wide'><figcaption><i class='key sile'></i>SILE <i class='key ours'></i>rile, overlaid</figcaption>{}</figure>",
            i + 1,
            e.map(|p| svg::page(p, vb, &mut defs))
                .unwrap_or_else(missing),
            o.map(|p| svg::page(p, vb, &mut defs))
                .unwrap_or_else(missing),
            svg::overlay(e, o, vb, &mut defs)
        );
        body.push_str("</section>");
    }
    format!(
        "<!doctype html>\n<html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1, viewport-fit=cover'>\
<title>{n} · SILE Parity</title>\
<link rel='stylesheet' href='https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600&family=Source+Serif+4:opsz,wght@8..60,500;8..60,600&display=swap'>\
<link rel='stylesheet' href='../style.css'></head><body>{defs}<main class='wrap'>\
<p class='back'><a href='../index.html'>← All tests</a></p>\
<header class='masthead'><p class='eyebrow'>SILE regression test</p><h1>{n}</h1>{why}\
<dl class='scores'><div><dt>Status</dt><dd><span class='pill {s}'>{s}</span></dd></div><div><dt>Glyphs</dt><dd>{}</dd></div><div><dt>Breaks</dt><dd>{}</dd></div><div><dt>Positions within {tol}pt</dt><dd>{}</dd></div><div><dt>Max offset</dt><dd>{:.2}pt</dd></div><div><dt>Pages SILE/ours</dt><dd>{}/{}</dd></div></dl></header>\
{body}</main></body></html>\n",
        pct(c.content),
        pct(c.breaks),
        pct(c.geometry),
        c.max_offset,
        c.pages.0,
        c.pages.1,
        n = escape(&r.name),
        s = bucket(r),
        why = r.settled.map_or_else(String::new, |k| format!("<p class='lede'>{}</p>", escape(k.reason()))),
        tol = crate::compare::TOLERANCE_PT,
        defs = defs.defs(),
    )
}

const STYLE: &str = r#"/* Layout: one reading column; page proofs sit three abreast like a light table. */
:root {
  --bg: #f3f4f6; --surface: #ffffff; --ink: #1d2128; --muted: #5d6573; --line: #d9dde3;
  --sile: #2a5bd7; --ours: #d2342b; --paper: #ffffff; --paper-edge: #c9ced6;
  --match: #2f8f5b; --close: #b7862a; --differs: #d2342b; --error: #7a3fb0; --unsupported: #9aa1ad; --explained: #3c7f8c; --skipped: #b5bac3;
  --display: "Source Serif 4", Georgia, serif; --body: "IBM Plex Sans", system-ui, sans-serif; --mono: "IBM Plex Mono", ui-monospace, monospace;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #15181d; --surface: #1d2128; --ink: #e6e8ec; --muted: #9aa2af; --line: #2e333c;
  --sile: #6f97ff; --ours: #ff6b5f; --paper: #f4f5f7; --paper-edge: #3a404a;
  --match: #5cc48a; --close: #e0b354; --differs: #ff6b5f; --error: #b98cf0; --unsupported: #6c7380; --explained: #6cc0cc; --skipped: #4a505a; color-scheme: dark } }
:root[data-theme="dark"] {
  --bg: #15181d; --surface: #1d2128; --ink: #e6e8ec; --muted: #9aa2af; --line: #2e333c;
  --sile: #6f97ff; --ours: #ff6b5f; --paper: #f4f5f7; --paper-edge: #3a404a;
  --match: #5cc48a; --close: #e0b354; --differs: #ff6b5f; --error: #b98cf0; --unsupported: #6c7380; --explained: #6cc0cc; --skipped: #4a505a; color-scheme: dark }
* { box-sizing: border-box }
body { margin: 0; background: var(--bg); color: var(--ink); font: 15px/1.55 var(--body) }
.wrap { max-width: 1180px; margin: 0 auto; padding-inline: 16px; padding-block: 32px 64px; display: grid; gap: 40px }
a { color: var(--sile) } a:focus-visible, button:focus-visible { outline: 2px solid var(--sile); outline-offset: 2px }
h1, h2 { font-family: var(--display); font-weight: 600; text-wrap: balance; margin: 0 }
h1 { font-size: 2.4rem; line-height: 1.1 } h2 { font-size: 1.35rem; margin-bottom: 8px }
.eyebrow { font: 500 12px/1 var(--mono); letter-spacing: .08em; text-transform: uppercase; color: var(--muted); margin: 0 0 10px }
.lede, .note { color: var(--muted); max-width: 65ch; margin: 10px 0 0 }
.masthead { display: grid; gap: 4px }
.summary { display: grid; gap: 16px }
.bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; background: var(--line) }
.seg.match { background: var(--match) } .seg.close { background: var(--close) } .seg.differs { background: var(--differs) }
.seg.error { background: var(--error) } .seg.unsupported { background: var(--unsupported) }
.seg.explained { background: var(--explained) } .seg.skipped { background: var(--skipped) }
.counts, .scores { display: flex; flex-wrap: wrap; gap: 12px 32px; margin: 0 }
.counts div, .scores div { display: grid; gap: 2px }
dt { font-size: 12px; color: var(--muted); display: flex; align-items: center; gap: 6px }
dd { margin: 0; font: 500 1.4rem/1.2 var(--mono); font-variant-numeric: tabular-nums }
.scores dd { font-size: 1.1rem }
.dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block }
.dot.match { background: var(--match) } .dot.close { background: var(--close) } .dot.differs { background: var(--differs) }
.dot.error { background: var(--error) } .dot.unsupported { background: var(--unsupported) }
.dot.explained { background: var(--explained) } .dot.skipped { background: var(--skipped) }
.scroll { overflow-x: auto; background: var(--surface); border: 1px solid var(--line); border-radius: 6px }
table { border-collapse: collapse; width: 100%; font-size: 14px }
th, td { text-align: left; padding: 7px 12px; border-bottom: 1px solid var(--line); white-space: nowrap }
th { font: 500 12px/1.3 var(--mono); color: var(--muted); letter-spacing: .04em; position: sticky; top: 0; background: var(--surface) }
tbody tr:last-child td { border-bottom: 0 }
td.muted { color: var(--muted); white-space: normal; min-width: 16rem }
.num { text-align: right; font-family: var(--mono); font-variant-numeric: tabular-nums }
code { font: 13px var(--mono) }
.pill { font: 500 12px/1 var(--mono); padding: 4px 8px; border-radius: 4px; border: 1px solid currentColor }
.pill.match { color: var(--match) } .pill.close { color: var(--close) } .pill.differs { color: var(--differs) }
.pill.error { color: var(--error) } .pill.unsupported { color: var(--muted) }
.pill.explained { color: var(--explained) } .pill.skipped { color: var(--muted) }
.filters { display: flex; flex-wrap: wrap; gap: 8px; margin: 4px 0 12px }
.chip { font: 500 13px var(--body); color: var(--ink); background: var(--surface); border: 1px solid var(--line); border-radius: 999px; padding: 5px 12px; cursor: pointer }
.chip.on { background: var(--ink); color: var(--bg); border-color: var(--ink) }
.back { margin: 0 } .back a { text-decoration: none; font-weight: 500 }
.spread { display: grid; gap: 10px }
.panes { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px }
.spread h2 + .panes { margin-top: 2px } .wide { margin-top: 8px }
@media (max-width: 760px) { .panes { grid-template-columns: minmax(0, 1fr) } }
figure { margin: 0; display: grid; gap: 6px; min-width: 0 }
figcaption { font: 500 12px var(--mono); color: var(--muted); display: flex; align-items: center; gap: 6px }
.key { width: 10px; height: 10px; display: inline-block; border-radius: 2px } .key.sile { background: var(--sile) } .key.ours { background: var(--ours); margin-left: 8px }
.page { width: 100%; height: auto; display: block; box-shadow: 0 0 0 1px var(--paper-edge) }
.page .paper { fill: var(--paper) }
.page .ink { fill: #1d2128 } .page .ink text { fill: #1d2128 }
.overlay .sile { fill: #2a5bd7; opacity: .75 } .overlay .ours { fill: #d2342b; opacity: .6 }
.overlay .sile text, .overlay .ours text { fill: inherit }
.rule { fill: inherit }
.missing { color: var(--muted); font-style: italic; margin: 0; padding: 24px; border: 1px dashed var(--line) }
@media (prefers-reduced-motion: reduce) { * { transition: none !important } }
"#;
