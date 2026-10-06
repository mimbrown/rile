//! What a paragraph keeps of a space its text ends with.

use rile::builder::{Arranger, Galley};
use rile::font::FontSpec;
use rile::node::Node;

/// The glue between the last word of the paragraph's last line and the glue
/// that fills the line out.
fn spaces_after_the_last_word(text: &str) -> usize {
    let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sile-parity/fonts/gentium-plus-5.000");
    let mut galley = Galley::new(Some(200.0));
    galley.load_fonts_dir(fonts);
    galley.set_font_spec(FontSpec { family: Some("Gentium Plus".into()), ..FontSpec::default() }).unwrap();
    galley.add_text(text);
    galley.new_paragraph().unwrap();
    let Some(Node::VBox(line)) = galley.vertical_list().iter().rev().find(|node| node.is_vbox()) else {
        panic!("no line was set");
    };
    let word = line.nodes.iter().rposition(|node| matches!(node, Node::NNode(_))).unwrap();
    // After the word come the line-filling glue and the glue of the margin.
    line.nodes[word + 1..].iter().filter(|node| node.is_glue()).count() - 2
}

#[test]
fn a_paragraph_without_a_final_space_ends_at_its_last_word() {
    assert_eq!(spaces_after_the_last_word("one two"), 0);
}

#[test]
#[cfg(not(feature = "sile-quirks"))]
fn a_final_space_is_dropped() {
    assert_eq!(spaces_after_the_last_word("one two "), 0);
}

#[test]
#[cfg(feature = "sile-quirks")]
fn sile_keeps_a_final_space() {
    assert_eq!(spaces_after_the_last_word("one two "), 1);
}
