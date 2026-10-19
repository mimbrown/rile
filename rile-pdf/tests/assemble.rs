use rile::builder::{Arranger, BuilderError, Fill, Galley, Layout, ParagraphStyle, TextAlign};
use rile::font::{FontFace, FontSpec};
use rile::frame::PaperSize;
use rile::metadata::Metadata;
use rile_pages::DocumentBuilder;
use rile_pages::class::{Book, Heading};
use rile_pdf::{Assembler, Chunk, PdfOptions, RenderedLayout};

const FONTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sile-parity/fonts/gentium-plus-5.000"
);

fn font() -> FontSpec {
    FontSpec {
        family: Some("Gentium Plus".into()),
        size: 10.0,
        ..Default::default()
    }
}

const TEXT: &str = "Each chapter is laid out on its own, as a book whose first page is odd, \
    and its folios are kept as deferred boxes until the chapters are put together and every \
    page's number is known. Nothing else on the page moves when they are filled.";

fn chapter(number: usize, paragraphs: usize) -> Result<Layout, BuilderError> {
    let mut doc = DocumentBuilder::new(PaperSize::A5);
    let mut book = Book::new();
    book.folio.deferred = true;
    book.chapters_open_spread = false;
    doc.set_class(book);
    doc.load_fonts_dir(FONTS);
    doc.set_language("en").set_tagged(true);
    doc.set_font_spec(font())?;
    doc.mark_toplevel();
    let heading = Heading {
        numbering: false,
        toc: false,
    };
    Book::chapter(
        &mut doc,
        heading,
        |doc: &mut DocumentBuilder| -> Result<(), BuilderError> {
            doc.add_text(format!("Chapter {number}"));
            Ok(())
        },
    )?;
    for _ in 0..paragraphs {
        doc.add_text(TEXT);
        doc.new_paragraph()?;
    }
    doc.lay_out()
}

fn folio(galley: &mut Galley, page: usize, width: f64) -> Result<Fill, BuilderError> {
    let align = if page % 2 == 1 {
        TextAlign::Right
    } else {
        TextAlign::Left
    };
    galley.set_measure(Some(width));
    galley.paragraph(
        &ParagraphStyle {
            align: Some(align),
            first_line_indent: Some(0.0),
            ..Default::default()
        },
        |g| {
            g.add_text(page.to_string());
            Ok::<_, BuilderError>(())
        },
    )?;
    galley.take_fill()
}

/// The digits each page's fills show, read back from the PDF by glyph id.
fn folios(pdf: &[u8]) -> Vec<String> {
    let face = FontFace::from_bytes(
        std::fs::read(format!("{FONTS}/GentiumPlus-R.ttf")).unwrap(),
        0,
    )
    .unwrap();
    let digits: Vec<(u16, char)> = ('0'..='9')
        .map(|c| (face.glyph_id(c).unwrap(), c))
        .collect();
    let doc = lopdf::Document::load_mem(pdf).unwrap();
    doc.get_pages()
        .values()
        .map(|&page| {
            let streams = doc.get_page_contents(page);
            let mut text = String::new();
            // A filled page draws its own content between q and Q, then
            // the fills.
            for &id in streams.iter().skip(3) {
                let stream = doc.get_object(id).unwrap().as_stream().unwrap();
                let content =
                    lopdf::content::Content::decode(&stream.decompressed_content().unwrap())
                        .unwrap();
                assert_eq!(
                    content
                        .operations
                        .iter()
                        .find(|op| op.operator == "BMC")
                        .map(|op| op.operands[0].as_name().unwrap()),
                    Some(&b"Artifact"[..])
                );
                for op in content
                    .operations
                    .iter()
                    .filter(|op| op.operator == "Tj" || op.operator == "TJ")
                {
                    let strings = match op.operator.as_str() {
                        "Tj" => op.operands.clone(),
                        _ => op.operands[0].as_array().unwrap().clone(),
                    };
                    for bytes in strings.iter().filter_map(|s| s.as_str().ok()) {
                        for gid in bytes.chunks(2).map(|b| u16::from_be_bytes([b[0], b[1]])) {
                            text.push(digits.iter().find(|d| d.0 == gid).map_or_else(
                                || panic!("glyph {gid} is not a digit in {digits:?}"),
                                |d| d.1,
                            ));
                        }
                    }
                }
            }
            text
        })
        .collect()
}

enum Part {
    Layout(Box<Layout>),
    Rendered(RenderedLayout),
}

/// Chapters laid out on threads and put together, each rendered either on
/// its thread or by the assembler, with their folios filled.
fn assemble(rendered_apart: bool) -> (Vec<Chunk>, usize, Vec<u8>) {
    let lengths = [6, 14, 9, 7];
    let parts: Vec<Part> = std::thread::scope(|scope| {
        let handles: Vec<_> = lengths
            .iter()
            .enumerate()
            .map(|(i, &n)| {
                scope.spawn(move || {
                    let layout = chapter(i + 1, n).unwrap();
                    match rendered_apart {
                        true => Part::Rendered(RenderedLayout::new(&layout, PdfOptions::default())),
                        false => Part::Layout(Box::new(layout)),
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let metadata = Metadata {
        title: Some("Chapters".into()),
        ..Default::default()
    };
    let mut book = Assembler::new(metadata, PdfOptions::default());
    for part in parts {
        match part {
            Part::Layout(layout) => book.add(*layout),
            Part::Rendered(rendered) => book.add_rendered(rendered),
        };
    }
    let chunks = book.chunks().to_vec();
    let mut galley = Galley::new(None);
    galley.load_fonts_dir(FONTS);
    galley.set_font_spec(font()).unwrap();
    for chunk in &chunks {
        for deferred in &chunk.deferred {
            let fill = folio(&mut galley, deferred.page + 1, deferred.width).unwrap();
            assert_eq!(book.fill(chunk.index, &deferred.key, &fill), 1);
        }
    }
    let pages = book.page_count();
    (chunks, pages, book.finish().unwrap())
}

#[test]
fn chapters_laid_out_apart_are_numbered_once_put_together() {
    let (chunks, pages, pdf) = assemble(false);
    let blanks = chunks[..chunks.len() - 1]
        .iter()
        .filter(|c| c.pages % 2 == 1)
        .count();
    assert!(
        blanks > 0 && chunks.iter().any(|c| c.pages % 2 == 0),
        "{chunks:?}"
    );
    assert_eq!(
        pages,
        chunks.iter().map(|c| c.pages).sum::<usize>() + blanks
    );
    assert!(
        chunks.iter().all(|c| c.first_page % 2 == 0),
        "each chapter starts on an odd page"
    );

    let expected: Vec<String> = (0..pages)
        .map(|page| {
            let opener = chunks.iter().any(|c| c.first_page == page);
            let blank = !chunks
                .iter()
                .any(|c| (c.first_page..c.first_page + c.pages).contains(&page));
            if opener || blank {
                String::new()
            } else {
                (page + 1).to_string()
            }
        })
        .collect();
    assert_eq!(folios(&pdf), expected);
    assert!(String::from_utf8_lossy(&pdf).contains("/StructTreeRoot"));

    let (apart, apart_pages, apart_pdf) = assemble(true);
    assert_eq!((apart, apart_pages), (chunks, pages));
    assert_eq!(folios(&apart_pdf), expected);
}

#[test]
fn a_fill_wider_than_its_box_is_refused() {
    let mut galley = Galley::new(Some(5.0));
    galley.load_fonts_dir(FONTS);
    galley.set_font_spec(font()).unwrap();
    galley.add_text("12345");
    assert!(galley.take_fill().is_err());
    galley.set_measure(Some(100.0));
    galley.add_text("12345");
    assert!(galley.take_fill().is_ok());
}
