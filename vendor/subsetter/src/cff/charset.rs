use crate::write::Writer;
use crate::GlyphRemapper;
use crate::Result;

/// Rewrite the charset of the font. We do not preserve the CIDs from the original font. Instead,
/// we assign each glyph its original glyph ID as the CID. This makes it easier to reference them
/// from the PDF, since we know the CID a glyph will have before it's subsetted.
pub fn rewrite_charset(gid_mapper: &GlyphRemapper, w: &mut Writer) -> Result<()> {
    // Format 0: the CID of every glyph after .notdef, in subset order.
    w.write::<u8>(0);
    for old in gid_mapper.remapped_gids().skip(1) {
        w.write::<u16>(old);
    }

    Ok(())
}
