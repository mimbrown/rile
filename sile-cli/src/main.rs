use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use sile_core::builder::{BaselineSkip, BuilderError, FontFallback, Galley, Typesetter};
use sile_core::font::FontSpec;
use sile_core::frame::PaperSize;
use sile_core::length::Length;
use sile_core::measurement::Measurement;
use sile_core::metadata::Metadata;
use sile_markdown::Markdown;
use sile_pages::DocumentBuilder;
use sile_pages::class::{Book, Plain};
use sile_pages::lay_out_until_settled;
use sile_pages::toc::{DefaultTocStyle, TableOfContents};
use sile_pdf::PdfOptions;

/// Typeset a Markdown (CommonMark) document to PDF.
#[derive(Parser)]
#[command(name = "sile", version)]
struct Args {
    /// The Markdown file to typeset.
    input: PathBuf,
    /// Where to write the PDF; the input with a .pdf extension by default.
    /// A .svg name writes SVG instead, numbered per page when there are
    /// several.
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Class::Plain)]
    class: Class,
    /// A paper size such as a4, a5, letter or "15cm x 6cm".
    #[arg(long, default_value = "a4", value_parser = |s: &str| s.parse::<PaperSize>())]
    paper: PaperSize,
    /// Set the text this many points wide on one page as tall as it is,
    /// instead of on pages of the paper size.
    #[arg(long, conflicts_with_all = ["class", "paper", "toc"])]
    width: Option<f64>,
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
        Ok(outputs) => {
            for output in outputs {
                eprintln!("Wrote {}", output.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sile: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<Vec<PathBuf>, String> {
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

    let stem = args.input.file_stem().map(|s| s.to_string_lossy().into_owned());
    let metadata = Metadata { title: args.title.clone().or(stem), author: args.author.clone(), ..Default::default() };
    let setup = |ts: &mut Typesetter| -> Result<(), BuilderError> {
        ts.set_language(args.language.clone()).set_tagged(!args.untagged);
        ts.set_font_spec(FontSpec { family: Some(font.clone()), size: args.size, ..Default::default() })?;
        for family in &args.fallbacks {
            ts.add_font_fallback(FontFallback { family: Some(family.clone()), ..Default::default() })?;
        }
        let skip = Length::new(Measurement::pt(1.2 * args.size), Measurement::pt(1.0), Measurement::pt(0.0));
        if let Some(math) = &math {
            ts.math_settings_mut().family = math.clone();
        }
        ts.set_baseline_skip(Some(BaselineSkip { skip, lineskip: 1.0 }));
        ts.set_paragraph_indent(1.2 * args.size);
        ts.set_paragraph_skip(Length::new(Measurement::pt(0.0), Measurement::pt(1.0), Measurement::pt(0.0)));
        ts.mark_toplevel();
        Ok(())
    };
    let load_fonts = |ts: &mut Typesetter| {
        ts.load_system_fonts();
        for dir in &args.fonts_dirs {
            ts.load_fonts_dir(dir);
        }
    };

    let mut warnings = Vec::new();
    let layout = match args.width {
        Some(width) => (|| -> Result<_, BuilderError> {
            let mut galley = Galley::new(Some(width));
            load_fonts(&mut galley);
            setup(&mut galley)?;
            let mut md = Markdown::new(galley, &base, &mono);
            md.typeset(&src)?;
            warnings = std::mem::take(&mut md.warnings);
            let mut layout = md.finish().lay_out()?;
            layout.set_metadata(metadata);
            Ok(layout)
        })(),
        None => lay_out_until_settled(5, |references| -> Result<DocumentBuilder, BuilderError> {
            let mut doc = DocumentBuilder::new(args.paper);
            load_fonts(&mut doc);
            match args.class {
                Class::Plain => doc.set_class(Plain::new()),
                Class::Book => doc.set_class(Book::new()),
            };
            doc.set_references(references);
            if let Some(title) = &metadata.title {
                doc.set_title(title.clone());
            }
            if let Some(author) = &metadata.author {
                doc.set_author(author.clone());
            }
            setup(&mut doc)?;
            if args.toc {
                TableOfContents::default().typeset(&mut doc, &DefaultTocStyle)?;
            }
            let mut md = Markdown::new(doc, &base, &mono);
            md.typeset(&src)?;
            warnings = std::mem::take(&mut md.warnings);
            Ok(md.finish())
        }),
    }
    .map_err(|e| e.to_string())?;
    for warning in warnings {
        eprintln!("sile: warning: {warning}");
    }

    let output = args.output.clone().unwrap_or_else(|| args.input.with_extension("pdf"));
    if output.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        let pages = sile_svg::render(&layout);
        let stem = output.file_stem().unwrap_or_default().to_string_lossy();
        let paths: Vec<PathBuf> = (1..=pages.len())
            .map(|n| if pages.len() == 1 { output.clone() } else { output.with_file_name(format!("{stem}-{n}.svg")) })
            .collect();
        for (path, page) in paths.iter().zip(&pages) {
            write(path, page.as_bytes())?;
        }
        return Ok(paths);
    }
    write(&output, &sile_pdf::render(&layout, PdfOptions::default()).map_err(|e| e.to_string())?)?;
    Ok(vec![output])
}

fn write(path: &PathBuf, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
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
