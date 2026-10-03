//! Where a font lets text be shaped a word at a time, so shaped words can
//! be reused. Words are shaped with the spaces around them, so a rule may
//! look at a space as long as it looks no further. Rules that pair a space
//! with particular glyphs (kerning against a space, cursive or mark
//! attachment to one) make the characters behind those glyphs sticky: text
//! isn't split at a space beside them. Anything else that can see past a
//! space or change one rules the font out.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use ttf_parser::gdef::GlyphClass;
use ttf_parser::gpos::{PairAdjustment, PositioningSubtable, ValueRecord};
use ttf_parser::gsub::{SingleSubstitution, SubstitutionSubtable};
use ttf_parser::opentype_layout::{
    ChainedContextLookup, ClassDefinition, ContextLookup, Coverage, LayoutTable, Lookup, LookupFlags, SequenceLookupRecord,
};
use ttf_parser::{Face, GlyphId, Tag};
use unicode_linebreak::{BreakClass, break_property};

use crate::font::{Direction, FontFace, FontSpec};
use crate::shaper::{GlyphItem, Shaper};

/// `text` shaped as `shaper` would shape it whole, from the words cached
/// for `face` where the font allows.
pub(crate) fn shape(shaper: &dyn Shaper, text: &str, face: &FontFace, spec: &FontSpec) -> Vec<GlyphItem> {
    let shaped = shape_by_words(shaper, text, face, spec);
    #[cfg(debug_assertions)]
    if let Some(shaped) = &shaped {
        let whole = shaper.shape(text, face, spec);
        let key = |g: &GlyphItem| (g.gid, g.cluster, g.text.clone(), g.width, g.x_offset, g.y_offset, g.x_advance, g.y_advance);
        debug_assert!(
            shaped.iter().map(key).eq(whole.iter().map(key)),
            "{text:?} shaped by words differs from shaping it whole:\n{shaped:?}\n{whole:?}"
        );
    }
    shaped.unwrap_or_else(|| shaper.shape(text, face, spec))
}

enum Piece {
    Word { range: Range<usize>, space_before: bool, space_after: bool },
    Space(usize),
}

fn shape_by_words(shaper: &dyn Shaper, text: &str, face: &FontFace, spec: &FontSpec) -> Option<Vec<GlyphItem>> {
    if spec.direction == Direction::TTB {
        return None;
    }
    let sticky = face.sticky_chars(&spec.features, shaper.uses_graphite())?;
    let script = if spec.script.is_empty() { guess_script(text) } else { Cow::Borrowed("") };
    let mut pieces = split(text, &sticky, |c| face.glyph_id(c).is_some());
    if spec.direction == Direction::RTL {
        pieces.reverse();
    }
    let mut scripted: Option<FontSpec> = None;
    let mut padded = String::new();
    let mut glyphs = Vec::with_capacity(text.len());
    face.with_words(spec, &script, |words| {
        for piece in pieces {
            let (start, range, space_before, space_after) = match piece {
                Piece::Space(at) => (at, at..at + 1, false, false),
                Piece::Word { range, space_before, space_after } => (range.start, range, space_before, space_after),
            };
            padded.clear();
            padded.push(char::from(b'0' + u8::from(space_before) + 2 * u8::from(space_after)));
            padded.extend([(space_before, " "), (true, &text[range.clone()]), (space_after, " ")].into_iter().filter(|p| p.0).map(|p| p.1));
            let word = match words.get(&padded) {
                Some(word) => Arc::clone(word),
                None => {
                    let spec = scripted.get_or_insert_with(|| FontSpec { script: if script.is_empty() { spec.script.clone() } else { script.to_string() }, ..spec.clone() });
                    let word = Arc::new(unpad(shaper.shape(&padded[1..], face, spec), range.len(), space_before, space_after)?);
                    words.insert(padded.clone(), Arc::clone(&word));
                    word
                }
            };
            glyphs.extend(word.iter().map(|g| GlyphItem { cluster: g.cluster + start as u32, ..g.clone() }));
        }
        Some(glyphs)
    })
}

/// `text` cut at the spaces it can be split at: those with nothing sticky,
/// no mark or joiner and nothing the font lacks beside them.
fn split(text: &str, sticky: &BTreeSet<char>, in_font: impl Fn(char) -> bool) -> Vec<Piece> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let loose = |c: char| {
        c == ' '
            || (!sticky.contains(&c)
                && in_font(c)
                && !matches!(break_property(c as u32), BreakClass::CombiningMark | BreakClass::ZeroWidthJoiner | BreakClass::EmojiModifier))
    };
    let attached = |i: usize| chars.get(i).is_some_and(|&(_, c)| !loose(c));
    let splits: Vec<usize> = (0..chars.len())
        .filter(|&i| chars[i].1 == ' ' && !(i > 0 && attached(i - 1)) && !attached(i + 1) && !(attached(i + 2) && chars[i + 1].1 != ' '))
        .collect();
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut space_before = false;
    for &i in &splits {
        let at = chars[i].0;
        if at > start {
            pieces.push(Piece::Word { range: start..at, space_before, space_after: true });
        }
        pieces.push(Piece::Space(at));
        start = at + 1;
        space_before = true;
    }
    if start < text.len() {
        pieces.push(Piece::Word { range: start..text.len(), space_before, space_after: false });
    }
    pieces
}

/// A padded word's glyphs without the spaces around it, clustered from the
/// word's start; `None` unless each space came out as a glyph of its own.
fn unpad(mut glyphs: Vec<GlyphItem>, len: usize, space_before: bool, space_after: bool) -> Option<Vec<GlyphItem>> {
    let offset = u32::from(space_before);
    let end = offset + len as u32;
    let pads = glyphs.iter().filter(|g| g.cluster < offset || g.cluster >= end).count();
    if pads != usize::from(space_before) + usize::from(space_after) {
        return None;
    }
    glyphs.retain(|g| g.cluster >= offset && g.cluster < end);
    for g in &mut glyphs {
        g.cluster -= offset;
    }
    Some(glyphs)
}

/// The script HarfBuzz would pick for `text`, or none when the text has
/// no characters of a script of their own, nor would any part of it.
fn guess_script(text: &str) -> Cow<'static, str> {
    match text.chars().find(|c| !c.is_ascii() || c.is_ascii_alphabetic()) {
        None => return Cow::Borrowed(""),
        Some(c) if c.is_ascii() => return Cow::Borrowed("Latn"),
        _ => {}
    }
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let tag = buffer.script().tag().to_string();
    if matches!(tag.as_str(), "Zyyy" | "Zinh" | "Zzzz") { Cow::Borrowed("") } else { Cow::Owned(tag) }
}

/// Features HarfBuzz leaves off unless asked for.
const OFF_BY_DEFAULT: &[&str] = &[
    "aalt", "afrc", "c2pc", "c2sc", "case", "cpsp", "cswh", "dlig", "expt", "fwid", "halt", "hist", "hkna", "hlig", "hwid",
    "ital", "jp04", "jp78", "jp83", "jp90", "lnum", "nalt", "nlck", "onum", "ordn", "ornm", "palt", "pcap", "pkna", "pnum",
    "pwid", "qwid", "ruby", "salt", "sinf", "size", "smcp", "smpl", "subs", "sups", "swsh", "titl", "tnam", "tnum", "trad",
    "twid", "unic", "vhal", "vkna", "vpal", "zero",
];

/// The characters text mustn't be split beside, when `face` with
/// `features` (as in `FontSpec::features`) on top of the shaper's defaults
/// can be shaped a word at a time at all. `graphite` says whether the
/// shaper would use the font's Graphite tables.
pub(crate) fn sticky_chars(face: &Face, features: &str, graphite: bool) -> Option<BTreeSet<char>> {
    let partners = space_partners(face, features, graphite).ok()?;
    let mut sticky = BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for subtable in cmap.subtables {
            if !subtable.is_unicode() {
                continue;
            }
            subtable.codepoints(|cp| {
                if let (Some(c), Some(g)) = (char::from_u32(cp), subtable.glyph_index(cp))
                    && partners.contains(&g.0)
                {
                    sticky.insert(c);
                }
            });
        }
    }
    Some(sticky)
}

/// The glyphs whose shaping depends on a space beside them, or why the
/// font can't be shaped a word at a time.
fn space_partners(face: &Face, features: &str, graphite: bool) -> Result<BTreeSet<u16>, &'static str> {
    let has = |t: &str| face.raw_face().table(Tag::from_bytes_lossy(t.as_bytes())).is_some();
    if graphite && has("Silf") {
        return Err("Graphite");
    }
    if ["morx", "mort", "kerx"].into_iter().any(has) {
        return Err("AAT");
    }
    let space = face.glyph_index(' ').ok_or("no space glyph")?;
    let on: BTreeSet<Tag> = features
        .split([',', ';'])
        .filter_map(|f| f.trim().parse::<rustybuzz::Feature>().ok())
        .filter(|f| f.value != 0)
        .map(|f| Tag(f.tag.0))
        .collect();
    let mut scan = Scan { face, space, partners: BTreeSet::new() };
    scan.kern()?;
    let tables = face.tables();
    let gsub_applied = tables.gsub.map(|gsub| {
        applied_lookups(&gsub, &on, |l| l.subtables.into_iter::<SubstitutionSubtable>().flat_map(|s| substitution_calls(&s)).collect())
    });
    if let (Some(gsub), Some(applied)) = (tables.gsub, &gsub_applied) {
        for lookup in applied.iter().filter_map(|&i| gsub.lookups.get(i)) {
            if lookup.subtables.into_iter::<SubstitutionSubtable>().any(|s| !matches!(s, SubstitutionSubtable::Single(_) | SubstitutionSubtable::Multiple(_) | SubstitutionSubtable::Alternate(_))) {
                scan.check_flags(&lookup)?;
            }
            for subtable in lookup.subtables.into_iter::<SubstitutionSubtable>() {
                scan.substitution(&subtable, lookup.flags)?;
            }
        }
    }
    if let Some(gpos) = tables.gpos {
        let calls = |l: &Lookup| l.subtables.into_iter::<PositioningSubtable>().flat_map(|s| positioning_calls(&s)).collect();
        for lookup in applied_lookups(&gpos, &on, calls).into_iter().filter_map(|i| gpos.lookups.get(i)) {
            if lookup.subtables.into_iter::<PositioningSubtable>().any(|s| !matches!(s, PositioningSubtable::Single(_) | PositioningSubtable::MarkToBase(_) | PositioningSubtable::MarkToLigature(_) | PositioningSubtable::MarkToMark(_))) {
                scan.check_flags(&lookup)?;
            }
            for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
                scan.positioning(&subtable, lookup.flags)?;
            }
        }
    }
    if let (Some(gsub), Some(applied)) = (tables.gsub, &gsub_applied) {
        scan.substituted_into(&gsub, applied);
    }
    Ok(scan.partners)
}

/// The lookups of the features the shaper may apply, with those their
/// contextual rules call; all of them when the font swaps feature tables by
/// its variation axes.
fn applied_lookups(table: &LayoutTable, on: &BTreeSet<Tag>, calls: impl Fn(&Lookup) -> Vec<u16>) -> BTreeSet<u16> {
    let mut applied: BTreeSet<u16> = if table.variations.is_some() {
        (0..table.lookups.len()).collect()
    } else {
        table
            .features
            .into_iter()
            .filter(|f| on.contains(&f.tag) || !OFF_BY_DEFAULT.iter().any(|t| f.tag == Tag::from_bytes_lossy(t.as_bytes())))
            .flat_map(|f| f.lookup_indices)
            .collect()
    };
    let mut pending: Vec<u16> = applied.iter().copied().collect();
    while let Some(index) = pending.pop() {
        for called in table.lookups.get(index).map(|l| calls(&l)).unwrap_or_default() {
            if applied.insert(called) {
                pending.push(called);
            }
        }
    }
    applied
}

fn substitution_calls(subtable: &SubstitutionSubtable) -> Vec<u16> {
    match subtable {
        SubstitutionSubtable::Context(c) => context_calls(c),
        SubstitutionSubtable::ChainContext(c) => chained_calls(c),
        _ => Vec::new(),
    }
}

fn positioning_calls(subtable: &PositioningSubtable) -> Vec<u16> {
    match subtable {
        PositioningSubtable::Context(c) => context_calls(c),
        PositioningSubtable::ChainContext(c) => chained_calls(c),
        _ => Vec::new(),
    }
}

fn record_lookups(records: impl IntoIterator<Item = SequenceLookupRecord>) -> impl Iterator<Item = u16> {
    records.into_iter().map(|r| r.lookup_list_index)
}

fn context_calls(lookup: &ContextLookup) -> Vec<u16> {
    match lookup {
        ContextLookup::Format1 { sets, .. } | ContextLookup::Format2 { sets, .. } => {
            sets.into_iter().flatten().flat_map(|rule| record_lookups(rule.lookups)).collect()
        }
        ContextLookup::Format3 { lookups, .. } => record_lookups(*lookups).collect(),
    }
}

fn chained_calls(lookup: &ChainedContextLookup) -> Vec<u16> {
    match lookup {
        ChainedContextLookup::Format1 { sets, .. } | ChainedContextLookup::Format2 { sets, .. } => {
            sets.into_iter().flatten().flat_map(|rule| record_lookups(rule.lookups)).collect()
        }
        ChainedContextLookup::Format3 { lookups, .. } => record_lookups(*lookups).collect(),
    }
}

/// A position in a chained rule's sequence: the glyphs that can match it.
#[derive(Clone, Copy)]
enum Slot<'a> {
    Glyph(u16),
    Class(ClassDefinition<'a>, u16),
    Coverage(Coverage<'a>),
}

struct Scan<'f, 'a> {
    face: &'f Face<'a>,
    space: GlyphId,
    partners: BTreeSet<u16>,
}

impl Scan<'_, '_> {
    fn glyphs(&self) -> impl Iterator<Item = GlyphId> + use<> {
        (0..self.face.number_of_glyphs()).map(GlyphId)
    }

    fn kern(&mut self) -> Result<(), &'static str> {
        let Some(kern) = self.face.tables().kern else { return Ok(()) };
        for subtable in kern.subtables {
            if subtable.has_state_machine {
                return Err("kern state machine");
            }
            for g in self.glyphs() {
                let kerns = |l, r| subtable.glyphs_kerning(l, r).is_some_and(|k| k != 0);
                if kerns(g, self.space) || kerns(self.space, g) {
                    self.partners.insert(g.0);
                }
            }
        }
        Ok(())
    }

    /// A lookup matching a sequence while skipping glyphs of the space's own
    /// class could match across it unseen. (HarfBuzz doesn't skip by class
    /// when finding what a mark attaches to.)
    fn check_flags(&self, lookup: &Lookup) -> Result<(), &'static str> {
        let skipped = match self.face.tables().gdef.and_then(|g| g.glyph_class(self.space)) {
            Some(GlyphClass::Base) => lookup.flags.ignore_base_glyphs(),
            Some(GlyphClass::Ligature) => lookup.flags.ignore_ligatures(),
            Some(GlyphClass::Mark) => true,
            _ => false,
        };
        if skipped { Err("lookup skips the space") } else { Ok(()) }
    }

    fn substitution(&mut self, subtable: &SubstitutionSubtable, flags: LookupFlags) -> Result<(), &'static str> {
        let space = self.space;
        let changes_space = match subtable {
            SubstitutionSubtable::Single(s) => s.coverage().contains(space),
            SubstitutionSubtable::Multiple(s) => s.coverage.contains(space),
            SubstitutionSubtable::Alternate(s) => s.coverage.contains(space),
            SubstitutionSubtable::Ligature(s) => {
                s.coverage.contains(space)
                    || s.ligature_sets.into_iter().flatten().any(|l| l.components.into_iter().any(|g| g == space))
            }
            SubstitutionSubtable::Context(c) => return self.context(c),
            SubstitutionSubtable::ChainContext(c) => return self.chain(c, flags),
            SubstitutionSubtable::ReverseChainSingle(s) => {
                let mut sequence: Vec<Slot> = s.backtrack_coverages.into_iter().map(Slot::Coverage).collect();
                sequence.reverse();
                let input = sequence.len();
                sequence.push(Slot::Coverage(s.coverage));
                sequence.extend(s.lookahead_coverages.into_iter().map(Slot::Coverage));
                return self.sequence(&sequence, input..input + 1, flags);
            }
        };
        if changes_space { Err("GSUB changes the space") } else { Ok(()) }
    }

    fn positioning(&mut self, subtable: &PositioningSubtable, flags: LookupFlags) -> Result<(), &'static str> {
        let space = self.space;
        match subtable {
            PositioningSubtable::Single(s) => {
                if s.coverage().contains(space) {
                    return Err("GPOS moves the space");
                }
            }
            PositioningSubtable::Pair(PairAdjustment::Format1 { coverage, sets }) => {
                if let Some(set) = coverage.get(space).and_then(|i| sets.get(i)) {
                    let partners: Vec<u16> =
                        self.glyphs().filter(|g| set.get(*g).is_some_and(|(a, b)| moves(&a) || moves(&b))).map(|g| g.0).collect();
                    self.partners.extend(partners);
                }
                for (i, first) in coverage_glyphs(*coverage).into_iter().enumerate() {
                    if sets.get(i as u16).and_then(|set| set.get(space)).is_some_and(|(a, b)| moves(&a) || moves(&b)) {
                        self.partners.insert(first.0);
                    }
                }
            }
            PositioningSubtable::Pair(PairAdjustment::Format2 { coverage, classes, matrix }) => {
                if coverage.contains(space) {
                    let row = classes.0.get(space);
                    for second in (0..).take_while(|&c| matrix.get((row, c)).is_some()) {
                        if matrix.get((row, second)).is_some_and(|(a, b)| moves(&a) || moves(&b)) {
                            let partners: Vec<u16> = self.glyphs().filter(|g| classes.1.get(*g) == second).map(|g| g.0).collect();
                            self.partners.extend(partners);
                        }
                    }
                }
                let column = classes.1.get(space);
                let firsts = coverage_glyphs(*coverage);
                for first in (0..).take_while(|&c| matrix.get((c, column)).is_some()) {
                    if matrix.get((first, column)).is_some_and(|(a, b)| moves(&a) || moves(&b)) {
                        self.partners.extend(firsts.iter().filter(|g| classes.0.get(**g) == first).map(|g| g.0));
                    }
                }
            }
            PositioningSubtable::Cursive(s) => {
                if let Some(i) = s.coverage.get(space) {
                    let (entry, exit) = (s.sets.entry(i).is_some(), s.sets.exit(i).is_some());
                    for (j, g) in coverage_glyphs(s.coverage).into_iter().enumerate() {
                        if (entry && s.sets.exit(j as u16).is_some()) || (exit && s.sets.entry(j as u16).is_some()) {
                            self.partners.insert(g.0);
                        }
                    }
                }
            }
            PositioningSubtable::MarkToBase(s) => {
                if s.mark_coverage.contains(space) {
                    return Err("the space is a mark");
                }
                if s.base_coverage.contains(space) {
                    self.partners.extend(coverage_glyphs(s.mark_coverage).into_iter().map(|g| g.0));
                }
            }
            PositioningSubtable::MarkToLigature(s) => {
                if s.mark_coverage.contains(space) {
                    return Err("the space is a mark");
                }
                if s.ligature_coverage.contains(space) {
                    self.partners.extend(coverage_glyphs(s.mark_coverage).into_iter().map(|g| g.0));
                }
            }
            PositioningSubtable::MarkToMark(s) => {
                if s.mark1_coverage.contains(space) || s.mark2_coverage.contains(space) {
                    return Err("the space is a mark");
                }
            }
            PositioningSubtable::Context(c) => self.context(c)?,
            PositioningSubtable::ChainContext(c) => self.chain(c, flags)?,
        }
        Ok(())
    }

    /// Plain contextual rules act on every glyph they match.
    fn context(&self, lookup: &ContextLookup) -> Result<(), &'static str> {
        let space = self.space;
        let matches = match lookup {
            ContextLookup::Format1 { coverage, sets } => {
                coverage.contains(space) || sets.into_iter().flatten().any(|rule| rule.input.into_iter().any(|g| g == space.0))
            }
            ContextLookup::Format2 { coverage, classes, sets } => {
                let class = classes.get(space);
                coverage.contains(space) || sets.into_iter().flatten().any(|rule| rule.input.into_iter().any(|c| c == class))
            }
            ContextLookup::Format3 { coverage, coverages, .. } => coverage.contains(space) || coverages.into_iter().any(|c| c.contains(space)),
        };
        if matches { Err("contextual rule matches the space") } else { Ok(()) }
    }

    fn chain(&mut self, lookup: &ChainedContextLookup, flags: LookupFlags) -> Result<(), &'static str> {
        match lookup {
            ChainedContextLookup::Format1 { coverage, sets } => {
                for rule in sets.into_iter().flatten() {
                    let mut sequence: Vec<Slot> = rule.backtrack.into_iter().map(Slot::Glyph).collect();
                    sequence.reverse();
                    let input = sequence.len()..sequence.len() + 1 + rule.input.len() as usize;
                    sequence.push(Slot::Coverage(*coverage));
                    sequence.extend(rule.input.into_iter().map(Slot::Glyph));
                    sequence.extend(rule.lookahead.into_iter().map(Slot::Glyph));
                    self.sequence(&sequence, input, flags)?;
                }
            }
            ChainedContextLookup::Format2 { coverage, backtrack_classes, input_classes, lookahead_classes, sets } => {
                for rule in sets.into_iter().flatten() {
                    let mut sequence: Vec<Slot> = rule.backtrack.into_iter().map(|c| Slot::Class(*backtrack_classes, c)).collect();
                    sequence.reverse();
                    let input = sequence.len()..sequence.len() + 1 + rule.input.len() as usize;
                    sequence.push(Slot::Coverage(*coverage));
                    sequence.extend(rule.input.into_iter().map(|c| Slot::Class(*input_classes, c)));
                    sequence.extend(rule.lookahead.into_iter().map(|c| Slot::Class(*lookahead_classes, c)));
                    self.sequence(&sequence, input, flags)?;
                }
            }
            ChainedContextLookup::Format3 { coverage, backtrack_coverages, input_coverages, lookahead_coverages, .. } => {
                let mut sequence: Vec<Slot> = backtrack_coverages.into_iter().map(Slot::Coverage).collect();
                sequence.reverse();
                let input = sequence.len()..sequence.len() + 1 + input_coverages.len() as usize;
                sequence.push(Slot::Coverage(*coverage));
                sequence.extend(input_coverages.into_iter().map(Slot::Coverage));
                sequence.extend(lookahead_coverages.into_iter().map(Slot::Coverage));
                self.sequence(&sequence, input, flags)?;
            }
        }
        Ok(())
    }

    /// A chained rule, in text order, acting on the glyphs at `input`. A
    /// space is fine as its outermost context; elsewhere in the context the
    /// glyphs either side of it become partners, along with any marks the
    /// lookup skips; in the input it rules the font out.
    fn sequence(&mut self, sequence: &[Slot], input: std::ops::Range<usize>, flags: LookupFlags) -> Result<(), &'static str> {
        let last = sequence.len() - 1;
        let spaces: Vec<usize> = (0..sequence.len()).filter(|&i| self.matches(sequence[i], self.space)).collect();
        for i in spaces {
            if input.contains(&i) {
                return Err("chained rule acts on the space");
            }
            if i == 0 || i == last {
                continue;
            }
            if flags.ignore_base_glyphs() || flags.ignore_ligatures() {
                return Err("chained rule skips glyphs beside the space");
            }
            for slot in [sequence[i - 1], sequence[i + 1]] {
                let partners: Vec<u16> = self.glyphs().filter(|g| self.matches(slot, *g)).map(|g| g.0).collect();
                self.partners.extend(partners);
            }
            if flags.ignore_marks() || flags.use_mark_filtering_set() || flags.mark_attachment_type() != 0 {
                let gdef = self.face.tables().gdef;
                let marks: Vec<u16> =
                    self.glyphs().filter(|g| gdef.and_then(|d| d.glyph_class(*g)) == Some(GlyphClass::Mark)).map(|g| g.0).collect();
                self.partners.extend(marks);
            }
        }
        Ok(())
    }

    fn matches(&self, slot: Slot, glyph: GlyphId) -> bool {
        match slot {
            Slot::Glyph(g) => g == glyph.0,
            Slot::Class(classes, class) => classes.get(glyph) == class,
            Slot::Coverage(c) => c.contains(glyph),
        }
    }

    /// Add the glyphs that applied substitutions can turn into partners.
    fn substituted_into(&mut self, gsub: &LayoutTable, applied: &BTreeSet<u16>) {
        loop {
            let before = self.partners.len();
            for lookup in applied.iter().filter_map(|&i| gsub.lookups.get(i)) {
                for subtable in lookup.subtables.into_iter::<SubstitutionSubtable>() {
                    self.substitution_sources(&subtable);
                }
            }
            if self.partners.len() == before {
                return;
            }
        }
    }

    fn substitution_sources(&mut self, subtable: &SubstitutionSubtable) {
        let partners = &mut self.partners;
        let mut add = |from: GlyphId, to: Vec<GlyphId>| {
            if to.iter().any(|g| partners.contains(&g.0)) {
                partners.insert(from.0);
            }
        };
        match subtable {
            SubstitutionSubtable::Single(SingleSubstitution::Format1 { coverage, delta }) => {
                for g in coverage_glyphs(*coverage) {
                    add(g, vec![GlyphId((i32::from(g.0) + i32::from(*delta)) as u16)]);
                }
            }
            SubstitutionSubtable::Single(SingleSubstitution::Format2 { coverage, substitutes }) => {
                for (i, g) in coverage_glyphs(*coverage).into_iter().enumerate() {
                    add(g, substitutes.get(i as u16).into_iter().collect());
                }
            }
            SubstitutionSubtable::Multiple(s) => {
                for (i, g) in coverage_glyphs(s.coverage).into_iter().enumerate() {
                    add(g, s.sequences.get(i as u16).into_iter().flat_map(|q| q.substitutes).collect());
                }
            }
            SubstitutionSubtable::Alternate(s) => {
                for (i, g) in coverage_glyphs(s.coverage).into_iter().enumerate() {
                    add(g, s.alternate_sets.get(i as u16).into_iter().flat_map(|a| a.alternates).collect());
                }
            }
            SubstitutionSubtable::Ligature(s) => {
                for (i, g) in coverage_glyphs(s.coverage).into_iter().enumerate() {
                    for ligature in s.ligature_sets.get(i as u16).into_iter().flatten() {
                        if partners.contains(&ligature.glyph.0) {
                            partners.insert(g.0);
                            partners.extend(ligature.components.into_iter().map(|c| c.0));
                        }
                    }
                }
            }
            SubstitutionSubtable::ReverseChainSingle(s) => {
                for (i, g) in coverage_glyphs(s.coverage).into_iter().enumerate() {
                    add(g, s.substitutes.get(i as u16).into_iter().collect());
                }
            }
            SubstitutionSubtable::Context(_) | SubstitutionSubtable::ChainContext(_) => {}
        }
    }
}

/// The glyphs of `coverage`, in coverage index order.
fn coverage_glyphs(coverage: Coverage) -> Vec<GlyphId> {
    match coverage {
        Coverage::Format1 { glyphs } => glyphs.into_iter().collect(),
        Coverage::Format2 { records } => records.into_iter().flat_map(|r| (r.start.0..=r.end.0).map(GlyphId)).collect(),
    }
}

fn moves(value: &ValueRecord) -> bool {
    value.x_placement != 0
        || value.y_placement != 0
        || value.x_advance != 0
        || value.y_advance != 0
        || value.x_placement_device.is_some()
        || value.y_placement_device.is_some()
        || value.x_advance_device.is_some()
        || value.y_advance_device.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::gentium;

    fn pieces(text: &str, sticky: &str) -> Vec<String> {
        let sticky: BTreeSet<char> = sticky.chars().collect();
        split(text, &sticky, |_| true)
            .into_iter()
            .map(|p| match p {
                Piece::Word { range, space_before, space_after } => {
                    format!("{}{}{}", if space_before { "[" } else { "" }, &text[range], if space_after { "]" } else { "" })
                }
                Piece::Space(_) => "_".into(),
            })
            .collect()
    }

    #[test]
    fn text_is_split_at_spaces_with_nothing_attached() {
        assert_eq!(pieces("the cat sat", ""), ["the]", "_", "[cat]", "_", "[sat"]);
        assert_eq!(pieces("AT TAV", "AT"), ["AT TAV"]);
        assert_eq!(pieces("a \u{301}b c", ""), ["a \u{301}b]", "_", "[c"]);
        assert_eq!(pieces("a  b", ""), ["a]", "_", "_", "[b"]);
        assert_eq!(pieces(" a ", ""), ["_", "[a]", "_"]);
    }

    #[test]
    fn gentium_shapes_alike_by_words_and_whole_unless_by_graphite() {
        let face = FontFace::from_bytes(gentium(), 0).unwrap();
        let spec = FontSpec { size: 10.0, language: "en".into(), ..Default::default() };
        let shaper = crate::shaper::default_shaper();
        let text = "Office affinity: ǟ ˥˩ T\u{301}y, AVA “quoted” — 1⁄2 fi ffl.";
        if shaper.uses_graphite() {
            assert!(shape_by_words(&*shaper, text, &face, &spec).is_none(), "Graphite fonts are shaped whole");
            return;
        }
        let by_words = shape_by_words(&*shaper, text, &face, &spec).expect("Gentium shapes by words");
        let whole = shaper.shape(text, &face, &spec);
        let key = |g: &GlyphItem| (g.gid, g.cluster, g.text.clone(), g.width, g.x_offset, g.y_offset);
        assert!(by_words.iter().map(key).eq(whole.iter().map(key)));
        assert!(face.words.lock().unwrap().iter().map(|w| w.2.len()).sum::<usize>() > 5);
    }
}
