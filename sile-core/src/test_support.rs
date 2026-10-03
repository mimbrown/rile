use crate::builder::Galley;
use crate::font::FontSpec;
use crate::node::Node;
use crate::pagebuilder::Page;

pub fn gentium() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000/GentiumPlus-R.ttf");
    std::fs::read(path).expect("committed test font")
}

/// A galley as wide as an A5 page's text, set in 10pt Gentium Plus.
pub fn galley() -> Galley {
    let mut galley = Galley::new(Some(300.0));
    let spec = FontSpec { family: Some("Gentium Plus".into()), size: 10.0, ..Default::default() };
    galley.load_font_data("body", gentium(), spec).unwrap();
    galley.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../sile-parity/fonts/gentium-plus-5.000"));
    galley.set_font("body");
    galley
}

pub fn text_in(page: &Page, frame: &str) -> String {
    fn collect(nodes: &[Node], out: &mut String) {
        for n in nodes {
            match n {
                Node::NNode(n) => out.push_str(&n.text),
                Node::VBox(b) => collect(&b.nodes, out),
                Node::HBox(b) => collect(&b.nodes, out),
                _ => {}
            }
        }
    }
    let mut out = String::new();
    for (_, nodes) in page.content.iter().filter(|(id, _)| id == frame) {
        collect(nodes, &mut out);
    }
    out
}
