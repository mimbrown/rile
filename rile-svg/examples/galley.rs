//! Sets a few paragraphs in a 300pt galley, with no pages, and prints the
//! SVG: `cargo run -p rile-svg --example galley > galley.svg`.

use rile::builder::{Arranger, BaselineSkip, BuilderError, Galley};
use rile::font::FontSpec;
use rile::length::Length;

const FONTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000");

fn main() -> Result<(), BuilderError> {
    let mut galley = Galley::new(Some(300.0));
    let regular = FontSpec { family: Some("Gentium Plus".into()), size: 11.0, ..Default::default() };
    let heading = FontSpec { size: 16.0, ..regular.clone() };
    galley.load_font_file("body", format!("{FONTS}/GentiumPlus-R.ttf"), regular)?;
    galley.load_font_file("heading", format!("{FONTS}/GentiumPlus-R.ttf"), heading)?;
    galley.set_baseline_skip(Some(BaselineSkip { skip: Length::pt(14.0), lineskip: 1.0 }));
    galley.set_paragraph_skip(Length::pt(6.0));

    galley.set_paragraph_indent(0.0);
    galley.set_font("heading").add_text("A galley");
    galley.new_paragraph()?;
    galley.set_paragraph_indent(12.0);
    galley.set_font("body").add_text(
        "Text set in a galley is broken into lines to the measure and stacked as far down as it needs to go. \
         There are no pages, so nothing is spent choosing page breaks, and the surface is exactly as tall as its text.",
    );
    galley.new_paragraph()?;
    galley.add_text("Hyphenation, justification and the rest of the paragraph builder work as they do on a page.");

    for svg in rile_svg::render(&galley.lay_out()?) {
        print!("{svg}");
    }
    Ok(())
}
