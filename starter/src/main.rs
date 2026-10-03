//! A short book typeset from Rust. `cargo run -p rile-starter -- [fonts dir]`
//! writes `starter.pdf`, and `points.svg` with the same list set on its own.

use std::error::Error;

use rile::builder::{Arranger, BaselineSkip, BuilderError, Galley, Typesetter};
use rile::font::FontSpec;
use rile::frame::PaperSize;
use rile::length::Length;
use rile::lists::{ListKind, ListOptions};
use rile_pages::class::{Book, Heading};
use rile_pages::toc::{DefaultTocStyle, TableOfContents};
use rile_pages::{DocumentBuilder, lay_out_until_settled};

const FAMILY: &str = "Gentium Plus";

const POINTS: &[&str] = &[
    "Text goes in through a typesetter, which knows fonts and settings but not pages.",
    "An arranger decides where lines go: a galley keeps them, a document builder pages them.",
    "Outputs draw the finished layout, so the same document becomes PDF or SVG.",
];

/// Content written once against `Arranger` sets the same way into a galley
/// or onto pages.
fn key_points<A: Arranger>(a: &mut A) -> Result<(), BuilderError> {
    a.begin_list(ListKind::Itemize, &ListOptions::default())?;
    for point in POINTS {
        a.begin_item(None)?;
        a.add_text(*point);
        a.new_paragraph()?;
        a.end_item()?;
    }
    a.end_list()?;
    Ok(())
}

fn set_up(ts: &mut Typesetter, fonts: Option<&str>) -> Result<(), BuilderError> {
    ts.load_system_fonts();
    if let Some(dir) = fonts {
        ts.load_fonts_dir(dir);
    }
    ts.set_language("en").set_tagged(true);
    ts.set_font_spec(FontSpec { family: Some(FAMILY.into()), size: 11.0, ..Default::default() })?;
    ts.set_baseline_skip(Some(BaselineSkip { skip: Length::pt(14.0), lineskip: 1.0 }));
    Ok(())
}

fn book(fonts: Option<&str>) -> Result<rile::builder::Layout, BuilderError> {
    lay_out_until_settled(5, |references| -> Result<DocumentBuilder, BuilderError> {
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        doc.set_class(Book::new()).set_references(references);
        doc.set_title("A rile starter").set_author("You");
        set_up(&mut doc, fonts)?;
        doc.mark_toplevel();

        TableOfContents::default().typeset(&mut doc, &DefaultTocStyle)?;
        Book::chapter(&mut doc, Heading::default(), |doc: &mut DocumentBuilder| -> Result<(), BuilderError> {
            doc.add_text("Getting started");
            Ok(())
        })?;
        doc.add_text("This book was typeset by a few dozen lines of Rust.");
        DocumentBuilder::footnote(&mut doc, |doc: &mut DocumentBuilder| -> Result<(), BuilderError> {
            doc.add_text("See starter/src/main.rs.");
            Ok(())
        })?;
        doc.add_text(" Change them and run the starter again.");
        doc.new_paragraph()?;

        Book::section(&mut doc, Heading::default(), |doc: &mut DocumentBuilder| -> Result<(), BuilderError> {
            doc.add_text("Three ideas");
            Ok(())
        })?;
        key_points(&mut doc)?;
        Ok(doc)
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let fonts = std::env::args().nth(1);
    let layout = book(fonts.as_deref())?;
    std::fs::write("starter.pdf", rile_pdf::render(&layout, rile_pdf::PdfOptions::default())?)?;

    let mut galley = Galley::new(Some(280.0));
    set_up(&mut galley, fonts.as_deref())?;
    key_points(&mut galley)?;
    std::fs::write("points.svg", rile_svg::render(&galley.lay_out()?).concat())?;
    eprintln!("Wrote starter.pdf and points.svg");
    Ok(())
}
