//! A PDF page placed as an image ends up in the output as a form XObject
//! with what its resources refer to.
#![cfg(feature = "pdf-images")]

use std::sync::Arc;

use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use rile::builder::{Arranger, Galley};
use rile::image::{Image, ImageFormat};

/// A one-page PDF, 40 by 20 points off the origin, that fills a rectangle
/// using a graphics state from its resources.
fn sample() -> Vec<u8> {
    let (catalog, pages, page, content, state) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4), Ref::new(5));
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(pages);
    pdf.pages(pages).kids([page]).count(1).media_box(Rect::new(10.0, 10.0, 50.0, 30.0));
    let mut writer = pdf.page(page);
    writer.parent(pages).contents(content);
    writer.resources().ext_g_states().pair(Name(b"G0"), state);
    writer.finish();
    pdf.ext_graphics(state).non_stroking_alpha(0.5);
    let mut drawing = Content::new();
    drawing.set_parameters(Name(b"G0")).rect(10.0, 10.0, 40.0, 20.0).fill_nonzero();
    pdf.stream(content, &drawing.finish());
    pdf.finish()
}

#[test]
fn a_pdf_page_is_sized_by_its_inherited_box() {
    let image = Image::from_bytes("sample.pdf", sample()).unwrap();
    assert_eq!(image.format, ImageFormat::Pdf);
    assert_eq!(image.page_box, Some([10.0, 10.0, 50.0, 30.0]));
    assert_eq!(image.natural_size(), (40.0, 20.0));
    assert_eq!(image.scaled_size(Some(80.0), None), (80.0, 40.0));
}

#[test]
fn a_placed_pdf_page_becomes_a_form_with_its_resources() {
    let image = Arc::new(Image::from_bytes("sample.pdf", sample()).unwrap());
    let mut galley = Galley::new(Some(200.0));
    galley.add_image(image.clone(), Some(80.0), None);
    galley.add_image(image, None, None);
    galley.new_paragraph().unwrap();
    let layout = galley.lay_out().unwrap();
    let written = rile_pdf::render(&layout, rile_pdf::PdfOptions::default()).unwrap();

    let document = lopdf::Document::load_mem(&written).unwrap();
    let forms: Vec<&lopdf::Stream> = document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| stream.dict.get(b"Subtype").and_then(|s| s.as_name()).ok() == Some(b"Form".as_slice()))
        .collect();
    // Placed twice, written once.
    assert_eq!(forms.len(), 1);
    let form = forms[0];
    let bbox: Vec<f32> = form.dict.get(b"BBox").unwrap().as_array().unwrap().iter().map(|n| n.as_float().unwrap()).collect();
    assert_eq!(bbox, [10.0, 10.0, 50.0, 30.0]);
    assert!(String::from_utf8_lossy(&form.decompressed_content().unwrap()).contains("/G0 gs"));

    // The graphics state came along, as an object of the new file.
    let resources = form.dict.get(b"Resources").unwrap().as_dict().unwrap();
    let states = resources.get(b"ExtGState").unwrap().as_dict().unwrap();
    let state = document.get_dictionary(states.get(b"G0").unwrap().as_reference().unwrap()).unwrap();
    assert_eq!(state.get(b"ca").unwrap().as_float().unwrap(), 0.5);

    // Each placing draws the form, scaled from its box to the size asked for.
    let page = document.get_pages().into_values().next().unwrap();
    let content = String::from_utf8_lossy(&document.get_page_content(page)).into_owned();
    assert_eq!(content.matches("/Fm0 Do").count(), 2);
    assert!(content.contains("2 0 0 2 "), "the first is set at twice its size: {content}");
}
