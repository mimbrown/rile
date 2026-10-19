//! A book whose chapters are laid out on several threads and put together
//! into one tagged PDF, the page numbers filled in once every chapter's
//! length is known.
//!
//! ```sh
//! cargo run --release -p rile-pdf --example parallel_book -- [chapters] [threads] [out.pdf]
//! ```
//!
//! Each chapter is a book of its own, starting on an odd page, with
//! deferred folios. Chapters are rendered in order as they arrive and then
//! dropped; a chapter with an odd number of pages is followed by a blank
//! page.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use rile::builder::{Arranger, BuilderError, Fill, Galley, Layout, ParagraphStyle, TextAlign};
use rile::font::FontSpec;
use rile::frame::PaperSize;
use rile::metadata::Metadata;
use rile_pages::DocumentBuilder;
use rile_pages::class::{Book, Heading};
use rile_pdf::{Assembler, PdfOptions};

const FONTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sile-parity/fonts/gentium-plus-5.000"
);

const WORDS: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
];

fn font() -> FontSpec {
    FontSpec {
        family: Some("Gentium Plus".into()),
        size: 10.0,
        ..Default::default()
    }
}

/// A paragraph of made-up words, the same for the same seed.
fn paragraph(seed: u64) -> String {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let length = 40 + next() % 80;
    let mut text = String::new();
    for i in 0..length {
        let word = WORDS[(next() % WORDS.len() as u64) as usize];
        if i == 0 {
            text.push_str(&word[..1].to_uppercase());
            text.push_str(&word[1..]);
        } else {
            text.push(' ');
            text.push_str(word);
        }
        if i + 1 == length {
            text.push('.');
        } else if next() % 12 == 0 {
            text.push(',');
        }
    }
    text
}

fn chapter(number: usize, paragraphs: usize) -> Result<Layout, BuilderError> {
    let mut doc = DocumentBuilder::new(PaperSize::A5);
    let mut book = Book::new();
    book.folio.deferred = std::env::var("IMMEDIATE").is_err();
    // The chapter starts on the chunk's first page, which is odd already.
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
    for p in 0..paragraphs {
        doc.add_text(paragraph((number * 1000 + p) as u64));
        doc.new_paragraph()?;
    }
    doc.lay_out()
}

/// The folio of page `page` (from 1), at the outer edge of a box `width`
/// wide.
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let chapters: usize = args.next().map_or(Ok(40), |a| a.parse())?;
    let threads: usize = args.next().map_or(Ok(4), |a| a.parse())?;
    let out = args.next().unwrap_or_else(|| "parallel-book.pdf".into());
    let paragraphs = 30;

    let start = Instant::now();
    let metadata = Metadata {
        title: Some("A book laid out in parallel".into()),
        ..Default::default()
    };
    let mut book = Assembler::new(metadata, PdfOptions::default());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let (send, receive) = mpsc::channel();
        for _ in 0..threads.max(1) {
            let (send, next) = (send.clone(), &next);
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= chapters || send.send((i, chapter(i + 1, paragraphs))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut waiting = BTreeMap::new();
        let mut wanted = 0;
        for (i, layout) in receive {
            waiting.insert(i, layout?);
            while let Some(layout) = waiting.remove(&wanted) {
                book.add(layout);
                wanted += 1;
            }
        }
        Ok(())
    })?;
    let laid_out = start.elapsed();

    let mut galley = Galley::new(None);
    galley.load_fonts_dir(FONTS);
    galley.set_font_spec(font())?;
    for chunk in book.chunks().to_vec() {
        for deferred in &chunk.deferred {
            let fill = folio(&mut galley, deferred.page + 1, deferred.width)?;
            book.fill(chunk.index, &deferred.key, &fill);
        }
    }
    let pages = book.page_count();
    let pdf = book.finish()?;
    std::fs::write(&out, &pdf)?;
    eprintln!(
        "{chapters} chapters, {pages} pages on {threads} threads: laid out and rendered in {:.2}s, {:.2}s in all; wrote {out} ({} KB)",
        laid_out.as_secs_f64(),
        start.elapsed().as_secs_f64(),
        pdf.len() / 1024
    );
    Ok(())
}
