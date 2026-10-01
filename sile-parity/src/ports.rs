//! Rust in place of the Lua in SILE's tests. Each port does what the test's
//! Lua does, written against sile-core the way a Rust document would, so
//! the test runs without a Lua interpreter.

use sile_core::builder::{BuilderError, DocumentBuilder};
use sile_core::class::{bigskip, smallskip, with_font};
use sile_core::font::{Direction, FontStyle, FontWeight};
use sile_core::framespec::FrameSpec;
use sile_core::linebreak::ParShape;
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

fn arabic_frame(id: &str, top: &str, bottom: &str, direction: Option<Direction>) -> FrameSpec {
    FrameSpec { direction, ..FrameSpec::new(id).left("left(content)").right("right(content)").top(top).bottom(bottom) }
}

fn bug_1317(d: &mut Driver) -> Result<(), String> {
    with_doc(d, |doc| {
        doc.declare_page_frames(&[arabic_frame("other", "top(content) + 50%ph", "bottom(content)", Some(Direction::RTL))])?;
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
            arabic_frame("setleft", "top(content) + 40%ph", "top(content) + 50%ph", Some(Direction::LTR)),
        ])?;
        for (frame, text) in [("folio", "عَرَبي"), ("setleft", "foo"), ("inherit", "عَرَبي")] {
            doc.typeset_into(frame, |doc| Ok(doc.add_text(text)).map(|_| ()))?;
        }
        Ok(())
    })
}
