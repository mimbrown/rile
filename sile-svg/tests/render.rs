use sile_core::builder::{Arranger, Galley, Typesetter};
use sile_pages::DocumentBuilder;
use sile_core::color::Color;
use sile_core::font::FontSpec;
use sile_core::frame::PaperSize;

const FONTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000");

fn with_font(ts: &mut Typesetter) {
    let data = std::fs::read(format!("{FONTS}/GentiumPlus-R.ttf")).expect("committed test font");
    let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
    ts.load_font_data("body", data, spec).unwrap();
    ts.set_font("body");
}

#[test]
fn a_galley_is_one_svg_as_tall_as_its_text() {
    let mut g = Galley::new(Some(150.0));
    with_font(&mut g);
    g.add_text("A galley has no pages, so this paragraph is set as one surface as tall as its lines.");
    g.new_paragraph().unwrap();
    g.set_color(Color::Rgb { r: 1.0, g: 0.0, b: 0.0 });
    g.add_text("Red.");
    let layout = g.lay_out().unwrap();
    let height = layout.pages[0].paper.height;
    let svgs = sile_svg::render(&layout);
    assert_eq!(svgs.len(), 1);
    let svg = &svgs[0];
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains(&format!("viewBox=\"0 0 150 {}\"", format!("{height:.3}").trim_end_matches('0').trim_end_matches('.'))), "{svg}");
    assert!(svg.contains("<path id=\"g0\" d=\"M"));
    assert!(svg.contains("<use xlink:href=\"#g0\""));
    assert!(svg.contains("fill=\"#ff0000\""));
}

#[test]
fn pages_become_one_svg_each() {
    let mut doc = DocumentBuilder::new(PaperSize::A5);
    with_font(&mut doc);
    doc.add_text("One.");
    doc.add_page_break().unwrap();
    doc.add_text("Two.");
    doc.add_rule(100.0, 1.0).unwrap();
    let svgs = sile_svg::render(&doc.lay_out().unwrap());
    assert_eq!(svgs.len(), 2);
    assert!(svgs[1].contains("<rect x="));
}
