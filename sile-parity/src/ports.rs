//! Rust in place of the Lua in SILE's tests. Each port does what the test's
//! Lua does, written against sile-core the way a Rust document would, so
//! the test runs without a Lua interpreter.

use sile_core::builder::{BuilderError, DocumentBuilder};
use sile_core::color::Color;
use sile_core::class::{bigskip, smallskip, with_font};
use sile_core::font::{FontStyle, FontWeight};
use sile_core::framespec::{FrameDirection, FrameSpec};
use sile_core::linebreak::ParShape;
use sile_core::node::{LinerPainter, LinerStyle};
use sile_core::textcase;

use crate::driver::Driver;
use crate::sil::Command;

type Chunk = fn(&mut Driver) -> Result<(), String>;
type CommandFn = fn(&mut Driver, &Command) -> Result<(), String>;

/// A test's Lua, ported.
pub struct Port {
    /// One per Lua chunk (`\begin{lua}` or `\lua{}`), in document order.
    pub chunks: &'static [Chunk],
    /// Commands the Lua registers.
    pub commands: &'static [(&'static str, CommandFn)],
}

impl Port {
    pub fn command(&self, name: &str) -> Option<CommandFn> {
        self.commands.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
    }
}

pub fn port(test: &str) -> Option<Port> {
    let (chunks, commands): (&'static [Chunk], &'static [(&'static str, CommandFn)]) = match test {
        "bug-1003" => (&[bug_1003], &[]),
        "bug-1069" => (&[nothing], &[("a", bold), ("b", process)]),
        "bug-117" => (&[times_table], &[]),
        "bug-1611" => (&[|d| d.add_text("foo")], &[]),
        "bug-1674" => (&[bug_1674], &[]),
        "bug-200" => (&[case_mapping], &[]),
        "bug-892" => (&[nothing], &[("bug892", bug_892)]),
        "hangafter" => (&[nothing], &[("testhang", testhang)]),
        "hyph-except" => (&[tr_rabbin, tr_rabbin, tr_rabbin_typographic, af_volksemosie], &[]),
        "inline-lua" => (&[nothing, modulus, modulus, braces, escapes, escapes], &[]),
        "parshaping-simple" => (&[parshaping], &[]),
        "settings" => (&[cormorant, reset_family, cormorant_by_default, reset_family], &[]),
        "footnote-skip" => (&[footnote_skips], &[]),
        "bug-1317" => (&[bug_1317], &[]),
        "bug-1321" => (&[bug_1321], &[]),
        "sura-2" => (&[nothing], &[]),
        "bug-926" => (&[nothing], &[]),
        "bug-1280" => (&[utf16_round_trips], &[]),
        "bug-255" => (&[nothing], &[("donothing", |d, cmd| skipped(d, cmd, "0pt", None))]),
        "bug-255b" => (&[nothing], &[("donothing", |d, cmd| skipped(d, cmd, "20pt", Some("20pt")))]),
        "feat-unicode-softhyphen" => (&[nothing], &[]),
        "bug-liner-width" => (&[nothing], &[("advance-box-width", advance_box_width)]),
        "negative-spaces-in-line" => (&[|d| d.add_text(&"کی خواہش ".repeat(8))], &[]),
        _ => return None,
    };
    Some(Port { chunks, commands })
}

fn err(e: BuilderError) -> String {
    e.to_string()
}

/// Typeset with the builder directly, after the driver's settings.
fn with_doc(d: &mut Driver, f: impl FnOnce(&mut DocumentBuilder) -> Result<(), BuilderError>) -> Result<(), String> {
    d.sync()?;
    f(&mut d.doc).map_err(err)
}

fn nothing(_: &mut Driver) -> Result<(), String> {
    Ok(())
}

fn process(d: &mut Driver, cmd: &Command) -> Result<(), String> {
    d.process(cmd.content.as_deref().unwrap_or(&[]))
}

fn bug_1003(d: &mut Driver) -> Result<(), String> {
    d.add_text(r"\begin{bar}\end{bar}")
}

fn bold(d: &mut Driver, cmd: &Command) -> Result<(), String> {
    d.scoped(|d| {
        d.update_font(|f| f.weight = FontWeight(800))?;
        process(d, cmd)
    })
}

fn times_table(d: &mut Driver) -> Result<(), String> {
    for i in 1..=10 {
        d.add_text(&format!("{i} x {i} = {}. ", i * i))?;
        with_doc(d, |doc| doc.add_explicit_vskip(smallskip()).map(|_| ()))?;
    }
    Ok(())
}

fn bug_1674(d: &mut Driver) -> Result<(), String> {
    let italic = |f: &mut sile_core::font::FontSpec| f.style = FontStyle::Italic;
    with_doc(d, |doc| {
        doc.add_text("Foo ");
        with_font(doc, italic, |doc| Ok::<_, BuilderError>(doc.add_text("bar")).map(|_| ()))?;
        doc.add_text(" ");
        with_font(doc, italic, |doc| Ok::<_, BuilderError>(doc.add_text("baz")).map(|_| ()))
    })
}

fn case_mapping(d: &mut Driver) -> Result<(), String> {
    let samples = [("teSTIng string", "en"), ("ibraNİLER denemesi ılIk çeşme", "tr")];
    let cases: [fn(&str, &str) -> String; 3] = [textcase::uppercase, textcase::lowercase, textcase::titlecase];
    for case in cases {
        for (text, lang) in samples {
            d.add_text(&case(text, lang))?;
            with_doc(d, |doc| Ok(doc.add_penalty(-10_000)).map(|_| ()))?;
        }
    }
    Ok(())
}

/// Exercises how Lua passes functions and commands as content. In Rust
/// that is just closures; what reaches the page is a dozen empty boxes and
/// the content of the two commands that process theirs.
fn bug_892(d: &mut Driver, cmd: &Command) -> Result<(), String> {
    with_doc(d, |doc| {
        for _ in 0..10 {
            doc.start_hbox().end_hbox();
        }
        doc.add_text("bar").add_text("bar");
        for _ in 0..2 {
            doc.start_hbox().end_hbox();
        }
        Ok(())
    })?;
    process(d, cmd)
}

fn testhang(d: &mut Driver, cmd: &Command) -> Result<(), String> {
    let after: i32 = cmd.option("after").unwrap_or("0").parse().map_err(|_| "bad after")?;
    let indent = cmd.option("indent").unwrap_or("0");
    let indent_pt = d.dimen(indent)?;
    d.doc.set_current_indent(Some(0.0)).set_hanging(after, indent_pt);
    d.add_text(&format!("With hang parameters ({after}, {indent}): "))?;
    d.lorem(64)?;
    d.par()
}

fn show_hyphenation_points(d: &mut Driver, word: &str, lang: &str) -> Result<(), String> {
    let points = d.doc.hyphenation_mut().hyphenate_word(word, lang).join("-");
    d.add_text(&points)
}

fn tr_rabbin(d: &mut Driver) -> Result<(), String> {
    show_hyphenation_points(d, "rab'bin", "tr")
}

fn tr_rabbin_typographic(d: &mut Driver) -> Result<(), String> {
    show_hyphenation_points(d, "rab’bin", "tr")
}

fn af_volksemosie(d: &mut Driver) -> Result<(), String> {
    show_hyphenation_points(d, "volksemosie", "af")
}

fn modulus(d: &mut Driver) -> Result<(), String> {
    d.add_text(&(3 * 3 % 5).to_string())
}

fn braces(d: &mut Driver) -> Result<(), String> {
    d.add_text("Matching brace {} inception is")
}

fn escapes(d: &mut Driver) -> Result<(), String> {
    d.add_text("\\backslash & \ttab")
}

fn cormorant(d: &mut Driver) -> Result<(), String> {
    d.set("font.family", "Cormorant Infant")
}

fn reset_family(d: &mut Driver) -> Result<(), String> {
    d.reset_setting("font.family")
}

fn cormorant_by_default(d: &mut Driver) -> Result<(), String> {
    d.set_default("font.family", "Cormorant Infant")
}

/// The Lua also sets `topSkip`, which SILE ignores when the class has a
/// `topBox`, as footnotes do.
fn footnote_skips(d: &mut Driver) -> Result<(), String> {
    d.footnote_class()?;
    d.doc.insertion_class_mut("footnote").ok_or("no footnote class")?.inter_skip = 24.0;
    Ok(())
}

fn parshaping(d: &mut Driver) -> Result<(), String> {
    const SHAPE: [(Option<f64>, Option<f64>, Option<f64>); 6] = [
        (Some(50.0), Some(30.0), None),
        (Some(50.0), Some(60.0), None),
        (None, Some(120.0), None),
        (None, Some(30.0), Some(40.0)),
        (None, None, None),
        (Some(50.0), None, Some(50.0)),
    ];
    let settings = d.doc.linebreak_settings_mut();
    settings.tolerance = 2000;
    settings.par_shape = Some(ParShape::new(|line| SHAPE.get(line - 1).copied().unwrap_or_default()));
    d.set_parindent("0pt");
    d.lorem(20)?;
    d.par()?;
    d.lorem(30)
}

fn arabic_frame(id: &str, top: &str, bottom: &str, direction: Option<FrameDirection>) -> FrameSpec {
    FrameSpec { direction, ..FrameSpec::new(id).left("left(content)").right("right(content)").top(top).bottom(bottom) }
}

fn bug_1317(d: &mut Driver) -> Result<(), String> {
    with_doc(d, |doc| {
        doc.declare_page_frames(&[arabic_frame("other", "top(content) + 50%ph", "bottom(content)", Some(FrameDirection::RTL))])?;
        doc.typeset_into("other", |doc| Ok(doc.add_text("عَرَبي pass")).map(|_| ()))?;
        doc.add_text("عَرَبي pass");
        doc.add_explicit_vskip(bigskip())?;
        doc.set_bidi(false).add_text("عَرَبي fail");
        Ok(())
    })
}

fn bug_1321(d: &mut Driver) -> Result<(), String> {
    with_doc(d, |doc| {
        doc.declare_page_frames(&[
            arabic_frame("inherit", "top(content) + 20%ph", "top(content) + 30%ph", None),
            arabic_frame("setleft", "top(content) + 40%ph", "top(content) + 50%ph", Some(FrameDirection::LTR)),
        ])?;
        for (frame, text) in [("folio", "عَرَبي"), ("setleft", "foo"), ("inherit", "عَرَبي")] {
            doc.typeset_into(frame, |doc| Ok(doc.add_text(text)).map(|_| ()))?;
        }
        Ok(())
    })
}

/// A liner that only shades a band the height of the line around its
/// content, then moves on by its width.
fn advance_box_width(d: &mut Driver, cmd: &Command) -> Result<(), String> {
    let bs = d.dimen("0.9bs")?;
    let ratio = 0.3;
    let gray = Color::parse("gray")?;
    let band = LinerPainter::new(move |pen| {
        let width = pen.width();
        let height = pen.height().max((1.0 - ratio) * bs);
        let depth = pen.depth().max(ratio * bs);
        let (x, y) = (pen.x(), pen.y());
        let canvas = pen.canvas();
        canvas.push_color(gray);
        canvas.rule(x, y - height, width, height + depth);
        canvas.pop_color();
        pen.advance_writing(width);
    });
    d.sync()?;
    d.doc.start_liner(LinerStyle::Custom(band));
    process(d, cmd)?;
    d.doc.end_hbox();
    Ok(())
}

/// A paragraph with its own margins.
fn skipped(d: &mut Driver, cmd: &Command, lskip: &str, rskip: Option<&str>) -> Result<(), String> {
    d.scoped(|d| {
        d.set("document.lskip", lskip)?;
        if let Some(rskip) = rskip {
            d.set("document.rskip", rskip)?;
        }
        process(d, cmd)?;
        d.par()
    })
}

/// Round-trips text through hex-encoded UTF-16 in both byte orders, with
/// and without a byte order mark, ticking each that comes back intact.
fn utf16_round_trips(d: &mut Driver) -> Result<(), String> {
    fn hex(text: &str, be: bool) -> String {
        let bom = if be { "feff" } else { "fffe" };
        let units = text.encode_utf16().map(|u| if be { u.to_be_bytes() } else { u.to_le_bytes() });
        bom.to_string() + &units.map(|[a, b]| format!("{a:02x}{b:02x}")).collect::<String>()
    }
    fn decode(hex: &str, be: bool) -> String {
        let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
        let units: Vec<u16> =
            bytes.chunks_exact(2).map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) }).collect();
        let text = String::from_utf16_lossy(&units);
        text.strip_prefix('\u{FEFF}').map(str::to_string).unwrap_or(text)
    }
    let mut checks = 0;
    let mut check = |d: &mut Driver, ok: bool| -> Result<(), String> {
        if ok {
            checks += 1;
            d.add_text("✓")?;
        }
        Ok(())
    };
    for text in ["Schrödinger", "猫𠂤~"] {
        d.par()?;
        d.add_text(&format!("{text} ⇒"))?;
        for (be, bom) in [(true, "feff"), (false, "fffe")] {
            let encoded = hex(text, be);
            d.par()?;
            d.add_text(&format!("{encoded} → "))?;
            check(d, encoded.starts_with(bom))?;
            d.add_text(&decode(&encoded, be))?;
            d.par()?;
            let bare = &encoded[4..];
            d.add_text(&format!("{bare} → "))?;
            check(d, decode(bare, be) == text)?;
            d.add_text(&decode(bare, be))?;
        }
    }
    d.par()?;
    d.add_text(if checks == 8 { "OK" } else { "NG" })?;
    for _ in 0..checks {
        d.add_text("✓")?;
    }
    Ok(())
}
