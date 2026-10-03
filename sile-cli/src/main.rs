mod markdown;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use sile_core::builder::{BaselineSkip, BuilderError, DocumentBuilder, FontFallback};
use sile_core::class::{Book, Plain};
use sile_core::font::FontSpec;
use sile_core::frame::PaperSize;
use sile_core::length::Length;
use sile_core::measurement::Measurement;
use sile_core::references::lay_out_until_settled;
use sile_core::toc::{DefaultTocStyle, TableOfContents};
use sile_pdf::PdfOptions;

use markdown::Markdown;

/// Typeset a Markdown (CommonMark) document to PDF.
#[derive(Parser)]
#[command(name = "sile", version)]
struct Args {
    /// The Markdown file to typeset.
    input: PathBuf,
    /// Where to write the PDF; the input with a .pdf extension by default.
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Class::Plain)]
    class: Class,
    /// A paper size such as a4, a5, letter or "15cm x 6cm".
    #[arg(long, default_value = "a4", value_parser = |s: &str| s.parse::<PaperSize>())]
    paper: PaperSize,
    /// The main font family; the first of Gentium Plus, Gentium Book Plus
    /// and a few common serif fonts that is installed by default.
    #[arg(long)]
    font: Option<String>,
    /// Font size in points.
    #[arg(long, default_value_t = 10.0)]
    size: f64,
    /// Font families to use, in order, for characters the main font lacks,
    /// such as a Thai font.
    #[arg(long = "fallback")]
    fallbacks: Vec<String>,
    /// The font family for code; the first of Hack, DejaVu Sans Mono and a
    /// few common monospaced fonts that is installed by default.
    #[arg(long)]
    mono: Option<String>,
    /// The document's language, for hyphenation and line breaking.
    #[arg(long, default_value = "en")]
    language: String,
    /// The font family for $math$; the first of Libertinus Math, STIX Two
    /// Math and a few other OpenType math fonts that is installed by
    /// default.
    #[arg(long = "math-font")]
    math_font: Option<String>,
    /// More directories to look for fonts in.
    #[arg(long = "fonts-dir")]
    fonts_dirs: Vec<PathBuf>,
    /// Start with a table of contents.
    #[arg(long)]
    toc: bool,
    /// The PDF's title.
    #[arg(long)]
    title: Option<String>,
    /// The PDF's author.
    #[arg(long)]
    author: Option<String>,
    /// Leave out the document structure that makes the PDF accessible to
    /// screen readers.
    #[arg(long)]
    untagged: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Class {
    Plain,
    Book,
}

const SERIF: &[&str] = &["Gentium Plus", "Gentium Book Plus", "Gentium Book", "Libertinus Serif", "Noto Serif", "DejaVu Serif", "Liberation Serif", "Times New Roman", "Georgia"];
const MATH: &[&str] = &["Libertinus Math", "STIX Two Math", "Latin Modern Math", "TeX Gyre Termes Math", "TeX Gyre Pagella Math", "Cambria Math"];
const MONO: &[&str] = &["Hack", "DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono", "Menlo", "Consolas", "Courier New"];

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(output) => {
            eprintln!("Wrote {}", output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sile: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<PathBuf, String> {
    let src = std::fs::read_to_string(&args.input).map_err(|e| format!("{}: {e}", args.input.display()))?;
    let base = args.input.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut fonts = fontdb::Database::new();
    fonts.load_system_fonts();
    for dir in &args.fonts_dirs {
        fonts.load_fonts_dir(dir);
    }
    let font = pick_family(&fonts, args.font.as_deref(), SERIF).ok_or("no serif font found; name one with --font")?;
    let mono = pick_family(&fonts, args.mono.as_deref(), MONO).unwrap_or_else(|| font.clone());
    let math = pick_family(&fonts, args.math_font.as_deref(), MATH);

    let mut warnings = Vec::new();
    let layout = lay_out_until_settled(5, |references| -> Result<DocumentBuilder, BuilderError> {
        let mut doc = DocumentBuilder::new(args.paper);
        doc.load_system_fonts();
        for dir in &args.fonts_dirs {
            doc.load_fonts_dir(dir);
        }
        match args.class {
            Class::Plain => doc.set_class(Plain::new()),
            Class::Book => doc.set_class(Book::new()),
        };
        doc.set_references(references);
        doc.set_language(args.language.clone()).set_tagged(!args.untagged);
        let stem = args.input.file_stem().map(|s| s.to_string_lossy().into_owned());
        if let Some(title) = args.title.clone().or(stem) {
            doc.set_title(title);
        }
        if let Some(author) = &args.author {
            doc.set_author(author.clone());
        }
        doc.set_font_spec(FontSpec { family: Some(font.clone()), size: args.size, ..Default::default() })?;
        for family in &args.fallbacks {
            doc.add_font_fallback(FontFallback { family: Some(family.clone()), ..Default::default() })?;
        }
        let skip = Length::new(Measurement::pt(1.2 * args.size), Measurement::pt(1.0), Measurement::pt(0.0));
        if let Some(math) = &math {
            doc.math_settings_mut().family = math.clone();
        }
        doc.set_baseline_skip(Some(BaselineSkip { skip, lineskip: 1.0 }));
        doc.set_paragraph_indent(1.2 * args.size);
        doc.set_paragraph_skip(Length::new(Measurement::pt(0.0), Measurement::pt(1.0), Measurement::pt(0.0)));
        doc.mark_toplevel();
        if args.toc {
            TableOfContents::default().typeset(&mut doc, &DefaultTocStyle)?;
        }
        let mut md = Markdown::new(doc, &base, &mono);
        md.typeset(&src)?;
        warnings = std::mem::take(&mut md.warnings);
        Ok(md.finish())
    })
    .map_err(|e| e.to_string())?;
    for warning in warnings {
        eprintln!("sile: warning: {warning}");
    }

    let output = args.output.clone().unwrap_or_else(|| args.input.with_extension("pdf"));
    let pdf = sile_pdf::render(&layout, PdfOptions::default()).map_err(|e| e.to_string())?;
    std::fs::write(&output, pdf).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok(output)
}

/// The family asked for when it is installed, or else the first of
/// `defaults` that is.
fn pick_family(fonts: &fontdb::Database, wanted: Option<&str>, defaults: &[&str]) -> Option<String> {
    let installed = |name: &str| fonts.faces().flat_map(|f| &f.families).any(|(family, _)| family.eq_ignore_ascii_case(name));
    match wanted {
        Some(name) => Some(name.to_string()),
        None => defaults.iter().find(|name| installed(name)).map(|name| name.to_string()),
    }
}
