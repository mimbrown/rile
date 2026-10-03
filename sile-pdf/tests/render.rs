use sile_core::builder::{BuilderError, DocumentBuilder, Layout};
use sile_core::class::{Book, DocumentClass, Heading, Plain};
use sile_core::color::Color;
use sile_core::cropmarks::Cropmarks;
use sile_core::font::{Direction, FontSpec, FontWeight};
use sile_core::frame::PaperSize;
use sile_core::lists::{ListKind, ListOptions};
use sile_core::node::LinkDest;
use sile_core::references::{CrossReferences, lay_out_until_settled};
use sile_core::toc::{DefaultTocStyle, TableOfContents};
use sile_pdf::PdfOptions;

const FONTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000");

fn gentium() -> Vec<u8> {
    std::fs::read(format!("{FONTS}/GentiumPlus-R.ttf")).expect("committed test font")
}

fn doc(class: impl DocumentClass + 'static) -> DocumentBuilder {
    let mut doc = DocumentBuilder::new(PaperSize::A5);
    let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
    doc.load_font_data("body", gentium(), spec).unwrap();
    doc.load_fonts_dir(FONTS);
    doc.set_font("body").set_class(class);
    doc
}

fn pdf(layout: &Layout) -> String {
    let bytes = sile_pdf::render(layout, PdfOptions { compress: false, ..Default::default() }).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    String::from_utf8_lossy(&bytes).into_owned()
}

fn render(doc: DocumentBuilder) -> String {
    pdf(&doc.lay_out().unwrap())
}

#[test]
fn documents_render() {
    let empty = DocumentBuilder::new(PaperSize::A4);
    render(empty);

    let mut d = doc(Plain::new());
    d.set_title("My Document").set_author("Test Author").set_subject("Testing");
    d.set_margins(36.0, 36.0, 36.0, 36.0).set_paragraph_indent(40.0).set_paragraph_skip(12.0);
    d.add_bookmark("Chapter 1", 0);
    d.set_color(Color::Rgb { r: 1.0, g: 0.0, b: 0.0 });
    d.add_text("Red text.");
    d.clear_color();
    d.add_text(" Normal text.");
    d.new_paragraph().unwrap();
    d.add_page_break().unwrap();
    d.add_text("Page two content.");
    let text = render(d);
    assert!(text.contains("/Title (My Document)"));
    assert!(text.contains("/Author (Test Author)"));
    assert!(text.contains("/Title (Chapter 1)"));
    assert_eq!(text.matches("/Type /Page\n").count(), 2);
}

#[test]
fn output_is_deterministic_with_many_fonts() {
    let render = || {
        let mut d = DocumentBuilder::new(PaperSize::A4);
        for name in ["a", "b", "c", "d", "e"] {
            let spec = FontSpec { family: Some("Gentium Plus".into()), size: 12.0, weight: FontWeight::NORMAL, ..Default::default() };
            d.load_font_data(name, gentium(), spec).unwrap();
            d.set_font(name);
            d.add_text("Hello world ");
        }
        sile_pdf::render(&d.lay_out().unwrap(), PdfOptions::default()).unwrap()
    };
    let first = render();
    for _ in 0..4 {
        assert_eq!(render(), first);
    }
}

#[test]
fn latin_in_vertical_frames_is_turned_on_its_side() {
    let mut d = doc(Plain::japanese(true));
    d.update_font(|f| f.direction = Direction::Frame).unwrap();
    d.add_text("tate");
    DocumentBuilder::add_latin_in_tate(&mut d, |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
        d.add_text("yoko");
        Ok(())
    })
    .unwrap();
    d.new_paragraph().unwrap();
    assert_eq!(render(d).matches("0 -1 1 0 ").count(), 1);
}

#[test]
fn pages_are_centred_on_their_sheets() {
    let mut d = doc(Plain::new());
    d.set_sheet_size(Some(PaperSize::A4));
    Cropmarks::default().install(&mut d);
    d.add_text("Text.");
    let text = render(d);
    assert!(text.contains("/MediaBox [0 0 595.2756 841.8898]"), "{}", &text[..text.len().min(2000)]);
}

fn tagged_book() -> String {
    let mut d = doc(Book::new());
    d.set_tagged(true).set_title("Tagged");
    Book::chapter(&mut d, Heading::default(), |d: &mut DocumentBuilder| -> Result<(), BuilderError> {
        d.add_text("Openings");
        Ok(())
    })
    .unwrap();
    d.add_text("A paragraph with ");
    d.start_link(LinkDest::Uri("https://sile-typesetter.org".into())).add_text("a link").end_hbox();
    d.add_text(" and ");
    d.set_language("fr");
    d.add_text("du français");
    d.set_language("en");
    d.add_text(" in it.");
    d.new_paragraph().unwrap();
    d.begin_list(ListKind::Itemize, &ListOptions::default()).unwrap();
    d.begin_item(None).unwrap();
    d.add_text("An item");
    d.end_item().unwrap();
    d.end_list().unwrap();
    d.add_text("After the list.");
    render(d)
}

#[test]
fn tagged_pdfs_carry_the_structure() {
    let text = tagged_book();
    if let Ok(path) = std::env::var("SILE_TAGGED_PDF") {
        std::fs::write(path, &text).unwrap();
    }
    for needle in [
        "/StructTreeRoot",
        "/Marked true",
        "/Lang (en)",
        "/DisplayDocTitle true",
        "/S /H1",
        "/S /L",
        "/S /LI",
        "/S /Lbl",
        "/S /LBody",
        "/S /Link",
        "/Type /OBJR",
        "/Lang (fr)",
        "/H1 <<\n  /MCID 0\n>> BDC",
        "/Artifact BMC",
        "/StructParents ",
        "/Tabs /S",
        "<pdfuaid:part>1</pdfuaid:part>",
    ] {
        assert!(text.contains(needle), "no {needle}");
    }
    assert_eq!(text.matches(" BDC").count() + text.matches(" BMC").count(), text.matches("EMC").count());
}

#[test]
fn untagged_pdfs_have_no_marked_content() {
    let mut d = doc(Plain::new());
    d.add_text("Plain");
    let text = render(d);
    assert!(!text.contains("StructTreeRoot") && !text.contains("BDC") && !text.contains("BMC"));
}

fn book(references: Option<CrossReferences>) -> Result<DocumentBuilder, BuilderError> {
    let mut d = doc(Book::new());
    d.set_references(references);
    TableOfContents::default().typeset(&mut d, &DefaultTocStyle)?;
    for chapter in ["One", "Two"] {
        Book::chapter(&mut d, Heading::default(), |d: &mut DocumentBuilder| {
            d.add_text(chapter);
            Ok::<_, BuilderError>(())
        })?;
        Book::section(&mut d, Heading::default(), |d: &mut DocumentBuilder| {
            d.add_text("Start");
            Ok::<_, BuilderError>(())
        })?;
        d.add_label(chapter, None).add_text("Text.");
    }
    Ok(d)
}

#[test]
fn toc_entries_link_to_their_headings_and_fill_the_outline() {
    let text = pdf(&lay_out_until_settled(3, book).unwrap());
    assert_eq!(text.matches("/S /GoTo").count(), 4);
    assert_eq!(text.matches("/Type /Outlines").count(), 1);
    assert_eq!(text.matches("/Title (Start)").count(), 2);
}
