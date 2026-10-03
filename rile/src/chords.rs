//! Chord names set above lyrics.
// SILE: `chordmode` package.

use crate::builder::{Arranger, BuilderError, Context, Typesetter};
use crate::length::Length;

/// How far above the lyric's baseline chords sit by default: two ex.
pub fn default_offset(doc: &Typesetter) -> f64 {
    2.0 * doc.x_height()
}

/// Set the chord typeset by `chord` `offset` above the lyric typeset by
/// `lyric`. The chord takes no room on its line; the lyric is widened to
/// the chord's width and half an em when narrower, and made tall enough
/// for the chord.
// SILE: `\ch`.
pub fn add_chord<C, E>(
    ctx: &mut C,
    offset: f64,
    chord: impl FnOnce(&mut C) -> Result<(), E>,
    lyric: impl FnOnce(&mut C) -> Result<(), E>,
) -> Result<(), E>
where
    C: Context,
    E: From<BuilderError>,
{
    ctx.arranger().start_hbox();
    chord(ctx)?;
    let doc = ctx.arranger();
    let mut chord_box = doc.make_hbox()?;
    let chord_width = pt(&chord_box.width);
    chord_box.width = Length::zero();
    let chord_line = pt(&chord_box.height) + offset;
    doc.add_baseline_shift(offset).add_box(chord_box).add_baseline_shift(-offset);
    doc.start_hbox();
    lyric(ctx)?;
    let doc = ctx.arranger();
    let mut lyric_box = doc.make_hbox()?;
    if pt(&lyric_box.width) < chord_width {
        let em = doc.font_spec().map_or(10.0, |f| f.size);
        lyric_box.width = Length::pt(chord_width + 0.5 * em);
    }
    if chord_line > pt(&lyric_box.height) {
        lyric_box.height = Length::pt(chord_line);
    }
    doc.add_box(lyric_box);
    Ok(())
}

/// Typeset lyrics with chords written inline, `I’ve be<G>en a wild
/// rover`, each chord above the text up to the next chord or the end of
/// the line.
// SILE: `chordmode`.
pub fn add_chord_lyrics<A: Arranger>(doc: &mut A, lyrics: &str, offset: f64) -> Result<(), BuilderError> {
    for (chord, text) in parse(lyrics) {
        match chord {
            Some(chord) => add_chord(
                doc,
                offset,
                |d: &mut A| {
                    d.add_text(chord);
                    Ok::<_, BuilderError>(())
                },
                |d| {
                    d.add_text(text);
                    Ok(())
                },
            )?,
            None => {
                doc.add_text(text);
            }
        }
    }
    Ok(())
}

/// Split lyrics into runs of text, each with the chord written before it.
pub fn parse(lyrics: &str) -> Vec<(Option<String>, String)> {
    enum State {
        Text,
        Name,
        Chord(String),
    }
    let mut out = Vec::new();
    let mut state = State::Text;
    let mut current = String::new();
    for c in lyrics.chars() {
        state = match (state, c) {
            (State::Text, '<') => {
                if !current.is_empty() {
                    out.push((None, std::mem::take(&mut current)));
                }
                State::Name
            }
            (State::Name, '>') => State::Chord(std::mem::take(&mut current)),
            (State::Chord(name), '<') => {
                out.push((Some(name), std::mem::take(&mut current)));
                State::Name
            }
            (State::Chord(name), '\n') => {
                out.push((Some(name), std::mem::take(&mut current)));
                current.push('\n');
                State::Text
            }
            (state, c) => {
                current.push(c);
                state
            }
        };
    }
    match state {
        State::Chord(name) => out.push((Some(name), current)),
        _ if !current.is_empty() => out.push((None, current)),
        _ => {}
    }
    out
}

fn pt(length: &Length) -> f64 {
    length.length.to_pt().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chord: Option<&str>, text: &str) -> (Option<String>, String) {
        (chord.map(str::to_string), text.to_string())
    }

    #[test]
    fn chords_attach_to_the_text_after_them_up_to_the_line_end() {
        assert_eq!(
            parse("I’ve be<G>en a wild rover for many’s a <C>year\nAnd <D>I"),
            [run(None, "I’ve be"), run(Some("G"), "en a wild rover for many’s a "), run(Some("C"), "year"), run(None, "\nAnd "), run(Some("D"), "I")]
        );
        assert_eq!(parse("<Am><F>"), [run(Some("Am"), ""), run(Some("F"), "")]);
    }

    #[test]
    fn chords_take_no_room_and_widen_their_lyric() {
        use crate::node::Node;
        let mut doc = crate::test_support::galley();
        add_chord_lyrics(&mut doc, "<Cmaj7>a", 10.0).unwrap();
        let pages = doc.lay_out().unwrap().pages;
        let (_, nodes) = pages[0].content.iter().find(|(id, _)| id == "content").unwrap();
        let Some(Node::VBox(line)) = nodes.iter().find(|n| matches!(n, Node::VBox(_))) else { panic!("no line") };
        let boxes: Vec<_> = line.nodes.iter().filter_map(|n| match n {
            Node::HBox(b) if !b.nodes.is_empty() => Some(b),
            _ => None,
        }).collect();
        assert_eq!(boxes.len(), 2);
        assert_eq!(pt(&boxes[0].width), 0.0);
        assert!(pt(&boxes[1].width) > 25.0, "{}", pt(&boxes[1].width));
        assert!(pt(&boxes[1].height) > 15.0);
    }
}
