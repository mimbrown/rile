//! A face is embedded once, however many sizes and directions it is set in.

use rile::builder::{Arranger, Galley};
use rile::font::{Direction, FontSpec};

fn fonts() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sile-parity/fonts/gentium-plus-5.000")
}

#[test]
fn a_face_set_at_several_sizes_is_embedded_once() {
    let mut galley = Galley::new(Some(300.0));
    galley.load_fonts_dir(fonts());
    let spec = |size: f64, direction: Direction| FontSpec {
        family: Some("Gentium Plus".into()),
        size,
        direction,
        ..FontSpec::default()
    };
    for (size, direction) in [(10.0, Direction::LTR), (14.0, Direction::LTR), (10.0, Direction::RTL)] {
        galley.set_font_spec(spec(size, direction)).unwrap();
        galley.add_text("abc def");
        galley.new_paragraph().unwrap();
    }
    galley.set_font_spec(FontSpec { style: rile::font::FontStyle::Italic, ..spec(10.0, Direction::LTR) }).unwrap();
    galley.add_text("ghi");
    galley.new_paragraph().unwrap();

    let layout = galley.lay_out().unwrap();
    assert!(layout.fonts().count() >= 4, "the layout names a font for each size and direction");
    let written = rile_pdf::render(&layout, rile_pdf::PdfOptions { compress: false, ..Default::default() }).unwrap();
    let text = String::from_utf8_lossy(&written);

    // The upright face and the italic, and no more.
    assert_eq!(text.matches("/FontFile2").count(), 2);
    // Text at both sizes is set in the one upright font.
    let sizes = |name: &str| -> Vec<&str> {
        text.match_indices(&format!("/{name} ")).filter_map(|(at, m)| text[at + m.len()..].split(' ').next()).collect()
    };
    let both = ["F0", "F1"].iter().any(|name| sizes(name).contains(&"10") && sizes(name).contains(&"14"));
    assert!(both, "sizes set in F0: {:?}, in F1: {:?}", sizes("F0"), sizes("F1"));
}
