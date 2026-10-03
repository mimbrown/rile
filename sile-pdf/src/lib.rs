use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, TableHeaderScope};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use sile_core::builder::Layout;
use sile_core::color::Color;
use sile_core::font::FontFace;
use sile_core::metadata::{Bookmark, Metadata};
use sile_core::pagebuilder::Page;
use sile_core::structure::{Role, StructKid, StructTree};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum PdfError {
    Font(String),
    Image(String),
    Io(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Font(msg) => write!(f, "PDF font error: {msg}"),
            Self::Image(msg) => write!(f, "PDF image error: {msg}"),
            Self::Io(msg) => write!(f, "PDF I/O error: {msg}"),
        }
    }
}

impl std::error::Error for PdfError {}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// How a PDF is written, as opposed to what it says (`Metadata`).
#[derive(Debug, Clone)]
pub struct PdfOptions {
    pub creator: String,
    pub compress: bool,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self { creator: "sile-rust".to_string(), compress: true }
    }
}

/// Write `layout` as a PDF.
pub fn render(layout: &Layout, options: PdfOptions) -> Result<Vec<u8>, PdfError> {
    let mut pdf = PdfOutputter::new(layout.metadata().clone(), options);
    for (name, face, variations) in layout.fonts() {
        pdf.register_font(name, Arc::clone(face), variations);
    }
    for bookmark in layout.bookmarks() {
        pdf.add_bookmark(bookmark.clone());
    }
    if let Some(structure) = layout.structure() {
        pdf.set_structure(structure.clone());
    }
    pdf.render_pages(&layout.pages);
    pdf.finish()
}

// ---------------------------------------------------------------------------
// Link types
// ---------------------------------------------------------------------------

pub use sile_core::node::LinkDest;

#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    pub rect: [f64; 4],
    pub dest: LinkDest,
    /// The structure tag of the link's text.
    pub tag: Option<u32>,
}


// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct RefAlloc(i32);

impl RefAlloc {
    fn new() -> Self {
        Self(1)
    }

    fn bump(&mut self) -> Ref {
        let r = Ref::new(self.0);
        self.0 += 1;
        r
    }
}

struct FontEntry {
    face: Arc<FontFace>,
    /// Axis positions to instance a variable font at.
    variations: Vec<([u8; 4], f32)>,
    /// Glyphs in the order pages first use them; content streams address
    /// each by its index here, which is its glyph id in the subset.
    remapper: subsetter::GlyphRemapper,
    /// Text for ToUnicode, by subset glyph id.
    cid_to_unicode: BTreeMap<u16, String>,
    pdf_name: String,
}

impl FontEntry {
    fn cid(&mut self, gid: u16) -> u16 {
        self.remapper.remap(gid)
    }
}

struct ImageEntry {
    /// For JPEG: raw JPEG bytes. For PNG: compressed pixel data.
    data: Vec<u8>,
    width: u32,
    height: u32,
    channels: u8,
    is_jpeg: bool,
    alpha: Option<Vec<u8>>,
}

struct BuiltPage {
    width: f64,
    height: f64,
    /// Where the page's origin is on its sheet.
    offset: (f64, f64),
    content: Vec<u8>,
    annotations: Vec<LinkAnnotation>,
    /// The tag of each marked content sequence, by MCID.
    marks: Vec<u32>,
}

struct CurrentPage {
    width: f64,
    height: f64,
    content: Content,
    annotations: Vec<LinkAnnotation>,
    current_font: Option<(String, f64)>,
    current_color: Option<Color>,
    /// The tag of what is drawn next, and of the marked content sequence
    /// open in the content stream, if one is.
    tag: Option<u32>,
    open_mark: Option<Option<u32>>,
    marks: Vec<u32>,
}

/// Where content with a tag ended up.
#[derive(Debug, Clone, Copy)]
enum Tagged {
    Mark { page: usize, mcid: usize },
    Link { page: usize, index: usize },
}

// ---------------------------------------------------------------------------
// PdfOutputter
// ---------------------------------------------------------------------------

pub struct PdfOutputter {
    metadata: Metadata,
    options: PdfOptions,
    fonts: BTreeMap<String, FontEntry>,
    font_counter: usize,
    images: Vec<ImageEntry>,
    bookmarks: Vec<Bookmark>,
    /// Page index and top-left position of each named destination.
    destinations: HashMap<String, (usize, f64, f64)>,
    /// Indexes of images already added, by their data's address.
    placed_images: HashMap<usize, usize>,
    pages: Vec<BuiltPage>,
    current: Option<CurrentPage>,
    structure: Option<StructTree>,
    tagged: Vec<Vec<Tagged>>,
}

impl PdfOutputter {
    pub fn new(metadata: Metadata, options: PdfOptions) -> Self {
        Self {
            metadata,
            options,
            fonts: BTreeMap::new(),
            font_counter: 0,
            images: Vec::new(),
            bookmarks: Vec::new(),
            destinations: HashMap::new(),
            placed_images: HashMap::new(),
            pages: Vec::new(),
            current: None,
            structure: None,
            tagged: Vec::new(),
        }
    }

    /// Tag the PDF with `structure`, whose tags the drawn content carries.
    pub fn set_structure(&mut self, structure: StructTree) {
        self.structure = Some(structure);
    }

    fn record(&mut self, tag: u32, item: Tagged) {
        let tag = tag as usize;
        if self.tagged.len() <= tag {
            self.tagged.resize(tag + 1, Vec::new());
        }
        self.tagged[tag].push(item);
    }

    /// Open the marked content sequence for what is drawn next, if it
    /// isn't open already.
    fn mark(&mut self) {
        let Some(structure) = &self.structure else { return };
        let page = self.current.as_mut().expect("no current page");
        if page.open_mark == Some(page.tag) {
            return;
        }
        if page.open_mark.take().is_some() {
            page.content.end_marked_content();
        }
        page.open_mark = Some(page.tag);
        let Some(tag) = page.tag else {
            page.content.begin_marked_content(Name(b"Artifact"));
            return;
        };
        let role = structure.elements[structure.owner(tag)].role.name();
        let mcid = page.marks.len();
        page.marks.push(tag);
        page.content.begin_marked_content_with_properties(Name(role.as_bytes())).properties().identify(mcid as i32);
        let page = self.pages.len();
        self.record(tag, Tagged::Mark { page, mcid });
    }

    /// Close the open marked content sequence, which can't straddle a
    /// change of graphics state.
    fn end_mark(&mut self) {
        let page = self.current.as_mut().expect("no current page");
        if page.open_mark.take().is_some() {
            page.content.end_marked_content();
        }
    }

    // -- Font management ---------------------------------------------------

    pub fn register_font(&mut self, key: &str, face: Arc<FontFace>, variations: Vec<([u8; 4], f32)>) {
        if self.fonts.contains_key(key) {
            return;
        }
        let pdf_name = format!("F{}", self.font_counter);
        self.font_counter += 1;
        self.fonts.insert(
            key.to_string(),
            FontEntry {
                face,
                variations,
                remapper: subsetter::GlyphRemapper::new(),
                cid_to_unicode: BTreeMap::new(),
                pdf_name,
            },
        );
    }

    /// The code that shows `gid` in the content stream.
    fn track_glyph(&mut self, font_key: &str, gid: u16, text: &str) -> u16 {
        let Some(entry) = self.fonts.get_mut(font_key) else { return gid };
        let cid = entry.cid(gid);
        if !text.is_empty() {
            entry.cid_to_unicode.entry(cid).or_insert_with(|| text.to_string());
        }
        cid
    }

    // -- Image management --------------------------------------------------

    #[cfg(feature = "images")]
    pub fn add_image_jpeg(&mut self, data: Vec<u8>) -> Result<usize, PdfError> {
        let reader = image::ImageReader::new(std::io::Cursor::new(&data))
            .with_guessed_format()
            .map_err(|e| PdfError::Image(e.to_string()))?;
        let dims = reader.into_dimensions().map_err(|e| PdfError::Image(e.to_string()))?;
        Ok(self.push_jpeg(data, dims))
    }

    /// A JPEG embedded as it is, `pixels` in size.
    fn push_jpeg(&mut self, data: Vec<u8>, pixels: (u32, u32)) -> usize {
        let idx = self.images.len();
        self.images.push(ImageEntry {
            data,
            width: pixels.0,
            height: pixels.1,
            channels: 3,
            is_jpeg: true,
            alpha: None,
        });
        idx
    }

    #[cfg(feature = "images")]
    pub fn add_image_png(&mut self, data: &[u8]) -> Result<usize, PdfError> {
        let img = image::load_from_memory_with_format(data, image::ImageFormat::Png)
            .map_err(|e| PdfError::Image(e.to_string()))?;
        let width = img.width();
        let height = img.height();

        let (pixels, alpha, channels) = if img.color().has_alpha() {
            let rgba = img.to_rgba8();
            let raw = rgba.into_raw();
            let mut rgb = Vec::with_capacity((width * height * 3) as usize);
            let mut a = Vec::with_capacity((width * height) as usize);
            for chunk in raw.chunks(4) {
                rgb.extend_from_slice(&chunk[..3]);
                a.push(chunk[3]);
            }
            (rgb, Some(a), 3u8)
        } else {
            let rgb = img.to_rgb8();
            (rgb.into_raw(), None, 3u8)
        };

        let idx = self.images.len();
        self.images.push(ImageEntry {
            data: pixels,
            width,
            height,
            channels,
            is_jpeg: false,
            alpha,
        });
        Ok(idx)
    }

    // -- Bookmark management -----------------------------------------------

    pub fn add_bookmark(&mut self, bookmark: Bookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Name the point `(x, y)` on the current page; the first of a name wins.
    pub fn add_destination(&mut self, name: &str, x: f64, y: f64) {
        self.destinations.entry(name.to_string()).or_insert((self.pages.len(), x, y));
    }

    // -- Imperative page API -----------------------------------------------

    pub fn begin_page(&mut self, width: f64, height: f64) {
        assert!(self.current.is_none(), "end_page() not called before begin_page()");
        self.current = Some(CurrentPage {
            width,
            height,
            content: Content::new(),
            annotations: Vec::new(),
            current_font: None,
            current_color: None,
            tag: None,
            open_mark: None,
            marks: Vec::new(),
        });
    }

    pub fn end_page(&mut self) {
        self.end_mark();
        let page = self.current.take().expect("begin_page() not called");
        let offset = self.metadata.sheet.map_or((0.0, 0.0), |s| ((s.width - page.width) / 2.0, (s.height - page.height) / 2.0));
        let mut content = Content::new();
        if offset != (0.0, 0.0) {
            content.transform([1.0, 0.0, 0.0, 1.0, offset.0 as f32, offset.1 as f32]);
        }
        let mut content = content.finish();
        if !content.is_empty() {
            content.push(b'\n');
        }
        content.extend(page.content.finish());
        self.pages.push(BuiltPage {
            width: page.width,
            height: page.height,
            offset,
            content,
            annotations: page.annotations,
            marks: page.marks,
        });
    }

    pub fn set_font(&mut self, font_key: &str, size: f64) {
        let page = self.current.as_mut().expect("no current page");
        if page.current_font.as_ref().is_some_and(|(k, s)| k == font_key && *s == size) {
            return;
        }
        let pdf_name = self
            .fonts
            .get(font_key)
            .map(|e| e.pdf_name.clone())
            .unwrap_or_else(|| "F0".to_string());
        page.content.set_font(Name(pdf_name.as_bytes()), size as f32);
        page.current_font = Some((font_key.to_string(), size));
    }

    pub fn set_color(&mut self, color: Color) {
        let page = self.current.as_mut().expect("no current page");
        if page.current_color == Some(color) {
            return;
        }
        match color {
            Color::Rgb { r, g, b } => {
                page.content.set_fill_rgb(r as f32, g as f32, b as f32);
            }
            Color::Cmyk { c, m, y, k } => {
                page.content
                    .set_fill_cmyk(c as f32, m as f32, y as f32, k as f32);
            }
            Color::Grayscale { l } => {
                page.content.set_fill_gray(l as f32);
            }
        }
        page.current_color = Some(color);
    }

    /// Output positioned glyphs at (x, y) in SILE coordinates (origin top-left).
    /// Converts to PDF coordinates internally.
    pub fn show_glyphs(&mut self, x: f64, y: f64, font_key: &str, font_size: f64, glyphs: &[(u16, f64, f64, f64, f64)]) {
        // glyphs: (gid, x_advance, y_advance, x_offset, y_offset)
        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;

        let cids: Vec<u16> = glyphs.iter().map(|g| self.track_glyph(font_key, g.0, "")).collect();
        self.mark();
        let page = self.current.as_mut().expect("no current page");
        page.content.begin_text();

        let pdf_name = self
            .fonts
            .get(font_key)
            .map(|e| e.pdf_name.clone())
            .unwrap_or_else(|| "F0".to_string());
        page.content.set_font(Name(pdf_name.as_bytes()), font_size as f32);

        let mut cur_x = x;
        let mut cur_y = y;
        for (&(_, x_advance, y_advance, x_offset, y_offset), cid) in glyphs.iter().zip(cids) {
            let px = cur_x + x_offset;
            let py = page_height - (cur_y - y_offset);
            page.content.set_text_matrix([1.0, 0.0, 0.0, 1.0, px as f32, py as f32]);
            page.content.show(Str(&cid.to_be_bytes()));
            cur_x += x_advance;
            cur_y += y_advance;
        }

        page.content.end_text();
    }

    pub fn draw_rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.mark();
        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;
        let pdf_y = page_height - y - height;
        page.content.save_state();
        page.content
            .rect(x as f32, pdf_y as f32, width as f32, height as f32);
        page.content.fill_nonzero();
        page.content.restore_state();
    }

    pub fn draw_image(
        &mut self,
        image_idx: usize,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) {
        self.mark();
        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;
        let pdf_y = page_height - y - height;

        page.content.save_state();
        page.content.transform([
            width as f32,
            0.0,
            0.0,
            height as f32,
            x as f32,
            pdf_y as f32,
        ]);
        let img_name = format!("Im{image_idx}");
        page.content.x_object(Name(img_name.as_bytes()));
        page.content.restore_state();
    }

    pub fn push_state(&mut self) {
        self.end_mark();
        let page = self.current.as_mut().expect("no current page");
        page.content.save_state();
    }

    pub fn pop_state(&mut self) {
        self.end_mark();
        let page = self.current.as_mut().expect("no current page");
        page.content.restore_state();
    }

    pub fn rotate(&mut self, angle_deg: f64, cx: f64, cy: f64) {
        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;
        let pdf_cy = page_height - cy;
        let rad = angle_deg.to_radians();
        let cos = rad.cos() as f32;
        let sin = rad.sin() as f32;
        let tx = (cx * (1.0 - rad.cos()) + cy * rad.sin()) as f32;
        let ty = (pdf_cy * (1.0 - rad.cos()) - cx * rad.sin()) as f32;
        page.content.transform([cos, sin, -sin, cos, tx, ty]);
    }

    pub fn add_link(&mut self, rect: [f64; 4], dest: LinkDest) {
        let page = self.current.as_mut().expect("no current page");
        let tag = page.tag.filter(|_| self.structure.is_some());
        let index = page.annotations.len();
        page.annotations.push(LinkAnnotation { rect, dest, tag });
        if let Some(tag) = tag {
            let page = self.pages.len();
            self.record(tag, Tagged::Link { page, index });
        }
    }

    // -- High-level: render from Page objects ---

    pub fn render_pages(&mut self, pages: &[Page]) {
        sile_core::render::draw_pages(pages, self);
    }

    fn render_nnode(&mut self, nnode: &sile_core::node::NNode, x: f64, baseline_y: f64) {
        if nnode.glyphs.is_empty() || nnode.font_key.is_empty() {
            return;
        }

        // An uncoloured node is black; without the reset, a coloured node
        // earlier on the page would bleed into everything after it.
        self.set_color(nnode.color.unwrap_or(Color::Grayscale { l: 0.0 }));

        let cids: Vec<u16> = nnode.glyphs.iter().map(|g| self.track_glyph(&nnode.font_key, g.gid, &g.text)).collect();
        let space = (self.structure.is_some() && nnode.space_after && !nnode.vertical && nnode.bidi_level.unwrap_or(0).is_multiple_of(2))
            .then(|| self.fonts.get(&*nnode.font_key)?.face.glyph_id(' '))
            .flatten()
            .filter(|&gid| gid != 0)
            .map(|gid| self.track_glyph(&nnode.font_key, gid, " "));

        // Where the font's own advances are those of the embedded font,
        // glyphs follow each other in one TJ array, adjusted where the
        // shaper moved them; otherwise each is placed on its own.
        let entry = &self.fonts[&*nnode.font_key];
        let advance = |gid: u16| entry.face.advance_width(gid).unwrap_or(0) as f64 * nnode.font_size / entry.face.units_per_em() as f64;
        let chained = !nnode.vertical && !entry.face.is_variable();
        let mut placed: Vec<(f64, f64, u16, Option<f64>)> = Vec::with_capacity(cids.len() + 1);
        let (mut cur_x, mut cur_y) = (x, baseline_y);
        for (glyph, cid) in nnode.glyphs.iter().zip(&cids) {
            let follows = (chained && glyph.y_offset == 0.0).then(|| advance(glyph.gid));
            placed.push((cur_x + glyph.x_offset, cur_y - glyph.y_offset, *cid, follows));
            if nnode.vertical {
                cur_y += glyph.width;
            } else {
                cur_x += glyph.width;
            }
        }
        if let Some(cid) = space {
            placed.push((cur_x, cur_y, cid, None));
        }

        self.mark();
        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;

        let pdf_name = self.fonts.get(&*nnode.font_key).map_or("F0", |e| e.pdf_name.as_str());

        page.content.begin_text();
        page.content.set_font(Name(pdf_name.as_bytes()), nnode.font_size as f32);
        let mut bytes: Vec<u8> = Vec::with_capacity(2 * placed.len());
        let mut segments: Vec<(f32, usize)> = Vec::new();
        let mut i = 0;
        while i < placed.len() {
            let (x, y, _, _) = placed[i];
            page.content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x as f32, (page_height - y) as f32]);
            bytes.clear();
            segments.clear();
            segments.push((0.0, 0));
            let mut pen = x;
            loop {
                let (gx, _, cid, follows) = placed[i];
                let adjust = ((pen - gx) * 1000.0 / nnode.font_size * 100.0).round() as f32 / 100.0;
                if adjust != 0.0 {
                    segments.push((adjust, bytes.len()));
                }
                bytes.extend(cid.to_be_bytes());
                i += 1;
                match follows {
                    Some(width) if placed.get(i).is_some_and(|next| next.1 == y) => pen = gx + width,
                    _ => break,
                }
            }
            if segments.len() == 1 {
                page.content.show(Str(&bytes));
            } else {
                let mut shown = page.content.show_positioned();
                let mut items = shown.items();
                for (k, &(adjust, start)) in segments.iter().enumerate() {
                    let end = segments.get(k + 1).map_or(bytes.len(), |s| s.1);
                    if adjust != 0.0 {
                        items.adjust(adjust);
                    }
                    if end > start {
                        items.show(Str(&bytes[start..end]));
                    }
                }
            }
        }

        page.content.end_text();
    }

    // -- PDF assembly ------------------------------------------------------

    pub fn finish(self) -> Result<Vec<u8>, PdfError> {
        let mut alloc = RefAlloc::new();
        let mut pdf = Pdf::new();

        let catalog_ref = alloc.bump();
        let page_tree_ref = alloc.bump();

        // Allocate page refs
        let page_data: Vec<(Ref, Ref)> = self
            .pages
            .iter()
            .map(|_| (alloc.bump(), alloc.bump()))
            .collect();

        // Allocate font refs
        let font_refs: BTreeMap<String, FontRefs> = self
            .fonts
            .keys()
            .map(|key| {
                (
                    key.clone(),
                    FontRefs {
                        type0: alloc.bump(),
                        cid_font: alloc.bump(),
                        descriptor: alloc.bump(),
                        font_file: alloc.bump(),
                        tounicode: alloc.bump(),
                    },
                )
            })
            .collect();

        // Allocate image refs
        let image_data: Vec<(Ref, Option<Ref>)> = self
            .images
            .iter()
            .map(|img| {
                let main = alloc.bump();
                let smask = if img.alpha.is_some() {
                    Some(alloc.bump())
                } else {
                    None
                };
                (main, smask)
            })
            .collect();

        // Allocate bookmark refs
        let outline_ref = if !self.bookmarks.is_empty() {
            Some(alloc.bump())
        } else {
            None
        };
        let bookmark_refs: Vec<Ref> = self.bookmarks.iter().map(|_| alloc.bump()).collect();

        // Allocate annotation refs (per page)
        let annot_refs: Vec<Vec<Ref>> = self
            .pages
            .iter()
            .map(|p| p.annotations.iter().map(|_| alloc.bump()).collect())
            .collect();

        let structure = self.structure.as_ref().map(|tree| StructRefs::new(tree, &self.tagged, &mut alloc));
        let mut annot_parents = self.pages.len() as i32..;
        let annot_keys: Vec<Vec<Option<i32>>> = self
            .pages
            .iter()
            .map(|p| p.annotations.iter().map(|a| a.tag.filter(|_| structure.is_some()).map(|_| annot_parents.next().expect("keys"))).collect())
            .collect();

        // -- Write catalog --
        let mut catalog = pdf.catalog(catalog_ref);
        catalog.pages(page_tree_ref);
        if let Some(outline_ref) = outline_ref {
            catalog.outlines(outline_ref);
        }
        let metadata = structure.as_ref().map(|_| (alloc.bump(), self.xmp()));
        if let Some((metadata_ref, _)) = &metadata {
            catalog.metadata(*metadata_ref);
        }
        if let (Some(refs), Some(tree)) = (&structure, &self.structure) {
            catalog.pair(Name(b"StructTreeRoot"), refs.root);
            catalog.mark_info().marked(true);
            if !tree.lang().is_empty() {
                catalog.lang(TextStr(tree.lang()));
            }
            if self.metadata.title.is_some() {
                catalog.viewer_preferences().display_doc_title(true);
            }
        }
        catalog.finish();

        // -- Write document info --
        if self.metadata.title.is_some()
            || self.metadata.author.is_some()
            || self.metadata.subject.is_some()
            || !self.metadata.info.is_empty()
        {
            let info_ref = alloc.bump();
            let mut info = pdf.document_info(info_ref);
            if let Some(ref title) = self.metadata.title {
                info.title(TextStr(title));
            }
            if let Some(ref author) = self.metadata.author {
                info.author(TextStr(author));
            }
            if let Some(ref subject) = self.metadata.subject {
                info.subject(TextStr(subject));
            }
            for (key, value) in &self.metadata.info {
                info.pair(Name(key.as_bytes()), TextStr(value));
            }
            info.creator(TextStr(&self.options.creator));
            info.finish();
        }

        // -- Write page tree --
        let page_ref_list: Vec<Ref> = page_data.iter().map(|(pr, _)| *pr).collect();
        pdf.pages(page_tree_ref)
            .kids(page_ref_list.clone())
            .count(self.pages.len() as i32);

        // -- Write pages --
        for (i, built_page) in self.pages.iter().enumerate() {
            let (page_ref, content_ref) = page_data[i];

            let content_bytes = if self.options.compress {
                compress_data(&built_page.content)
            } else {
                built_page.content.clone()
            };

            let mut pg = pdf.page(page_ref);
            let (dx, dy) = built_page.offset;
            pg.media_box(Rect::new(
                0.0,
                0.0,
                (built_page.width + 2.0 * dx) as f32,
                (built_page.height + 2.0 * dy) as f32,
            ));
            pg.parent(page_tree_ref);
            pg.contents(content_ref);

            // Resources
            let mut resources = pg.resources();
            if !self.fonts.is_empty() {
                let mut font_dict = resources.fonts();
                for (key, entry) in &self.fonts {
                    if let Some(frefs) = font_refs.get(key) {
                        font_dict.pair(Name(entry.pdf_name.as_bytes()), frefs.type0);
                    }
                }
                font_dict.finish();
            }

            if !self.images.is_empty() {
                let mut xobjects = resources.x_objects();
                for (j, (img_ref, _)) in image_data.iter().enumerate() {
                    let name = format!("Im{j}");
                    xobjects.pair(Name(name.as_bytes()), *img_ref);
                }
                xobjects.finish();
            }
            resources.finish();

            // Annotations
            if !built_page.annotations.is_empty() {
                pg.annotations(annot_refs[i].iter().copied());
            }
            if structure.is_some() {
                pg.tab_order(pdf_writer::types::TabOrder::StructureOrder);
                if !built_page.marks.is_empty() {
                    pg.struct_parents(i as i32);
                }
            }

            pg.finish();

            // Content stream
            let mut stream = pdf.stream(content_ref, &content_bytes);
            if self.options.compress {
                stream.filter(Filter::FlateDecode);
            }
            stream.finish();
        }

        // -- Write fonts --
        for (key, entry) in &self.fonts {
            if let Some(frefs) = font_refs.get(key) {
                write_font(&mut pdf, entry, frefs, self.options.compress)?;
            }
        }

        // -- Write images --
        for (i, img) in self.images.iter().enumerate() {
            let (img_ref, smask_ref) = image_data[i];
            write_image(&mut pdf, img, img_ref, smask_ref, self.options.compress);
        }

        // -- Write annotations --
        for (i, built_page) in self.pages.iter().enumerate() {
            for (j, annot) in built_page.annotations.iter().enumerate() {
                let annot_ref = annot_refs[i][j];
                let dest = match &annot.dest {
                    LinkDest::Internal(name) => self.destination(name, &page_ref_list),
                    LinkDest::Uri(_) => None,
                };
                write_annotation(&mut pdf, annot, annot_ref, built_page, dest, annot_keys[i][j]);
            }
        }

        // -- Write bookmarks --
        if let Some(outline_ref) = outline_ref {
            let dests: Vec<_> = self.bookmarks.iter().map(|b| self.destination(&b.dest, &page_ref_list)).collect();
            write_outlines(&mut pdf, &self.bookmarks, &bookmark_refs, outline_ref, &dests);
        }

        if let Some((metadata_ref, xmp)) = &metadata {
            pdf.metadata(*metadata_ref, xmp.as_bytes());
        }
        if let (Some(refs), Some(tree)) = (&structure, &self.structure) {
            let page_refs: Vec<Ref> = page_data.iter().map(|(r, _)| *r).collect();
            refs.write(&mut pdf, tree, &self.tagged, &self.pages, &page_refs, &annot_refs, &annot_keys);
        }

        Ok(pdf.finish())
    }

    /// The document's XMP metadata. A tagged document with a title, whose
    /// figures and formulas all have descriptions, claims PDF/UA.
    fn xmp(&self) -> String {
        fn escape(s: &str) -> String {
            s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        }
        let mut props = String::new();
        if let Some(title) = &self.metadata.title {
            props += &format!("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>", escape(title));
        }
        if let Some(author) = &self.metadata.author {
            props += &format!("<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>", escape(author));
        }
        if let Some(subject) = &self.metadata.subject {
            props += &format!("<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>", escape(subject));
        }
        props += &format!("<xmp:CreatorTool>{}</xmp:CreatorTool>", escape(&self.options.creator));
        let described = self.structure.as_ref().is_some_and(|tree| {
            use sile_core::structure::Role;
            tree.elements.iter().all(|e| !matches!(e.role, Role::Figure | Role::Formula) || e.alt.is_some())
        });
        if self.metadata.title.is_some() && described {
            props += "<pdfuaid:part>1</pdfuaid:part>";
        }
        format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
             <x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
             <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" \
             xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\">{props}</rdf:Description></rdf:RDF></x:xmpmeta>\
             <?xpacket end=\"w\"?>"
        )
    }

    /// Where `name` is in PDF terms: its page and position from the bottom.
    fn destination(&self, name: &str, page_refs: &[Ref]) -> Option<Dest> {
        let &(page, x, y) = self.destinations.get(name)?;
        let built = self.pages.get(page)?;
        let (dx, dy) = built.offset;
        Some(Dest { page: *page_refs.get(page)?, x: (x + dx) as f32, y: (built.height - y + dy) as f32 })
    }
}

#[derive(Debug, Clone, Copy)]
struct Dest {
    page: Ref,
    x: f32,
    y: f32,
}

// ---------------------------------------------------------------------------
// Font refs bundle
// ---------------------------------------------------------------------------

struct FontRefs {
    type0: Ref,
    cid_font: Ref,
    descriptor: Ref,
    font_file: Ref,
    tounicode: Ref,
}

// ---------------------------------------------------------------------------
// Font embedding
// ---------------------------------------------------------------------------

impl sile_core::render::Canvas for PdfOutputter {
    fn begin_page(&mut self, width: f64, height: f64) {
        PdfOutputter::begin_page(self, width, height);
    }

    fn end_page(&mut self) {
        PdfOutputter::end_page(self);
    }

    fn glyphs(&mut self, nnode: &sile_core::node::NNode, x: f64, baseline_y: f64) {
        self.render_nnode(nnode, x, baseline_y);
    }

    fn rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.draw_rule(x, y, width, height);
    }

    fn push_color(&mut self, color: Color) {
        self.end_mark();
        let page = self.current.as_mut().expect("no current page");
        page.content.save_state();
        match color {
            Color::Rgb { r, g, b } => page.content.set_fill_rgb(r as f32, g as f32, b as f32),
            Color::Cmyk { c, m, y, k } => page.content.set_fill_cmyk(c as f32, m as f32, y as f32, k as f32),
            Color::Grayscale { l } => page.content.set_fill_gray(l as f32),
        };
    }

    fn pop_color(&mut self) {
        self.end_mark();
        self.current.as_mut().expect("no current page").content.restore_state();
    }

    fn destination(&mut self, name: &str, x: f64, y: f64) {
        self.add_destination(name, x, y);
    }

    fn image(&mut self, image: &sile_core::image::Image, x: f64, y: f64, width: f64, height: f64) {
        let key = Arc::as_ptr(&image.data) as usize;
        let index = match self.placed_images.get(&key) {
            Some(&index) => index,
            None => {
                let index = match image.format {
                    #[cfg(feature = "images")]
                    sile_core::image::ImageFormat::Png => match self.add_image_png(&image.data) {
                        Ok(index) => index,
                        Err(_) => return,
                    },
                    #[cfg(not(feature = "images"))]
                    sile_core::image::ImageFormat::Png => return,
                    sile_core::image::ImageFormat::Jpeg => self.push_jpeg(image.data.to_vec(), image.pixels),
                };
                self.placed_images.insert(key, index);
                index
            }
        };
        self.draw_image(index, x, y, width, height);
    }

    fn svg(&mut self, figure: &sile_core::svg_image::SvgFigure, x: f64, y: f64, _baseline: f64, _width: f64, _height: f64) {
        use sile_core::svg_image::SvgOp;
        self.mark();
        let page = self.current.as_mut().expect("no current page");
        let s = figure.scale as f32;
        let c = &mut page.content;
        c.save_state();
        c.transform([s, 0.0, 0.0, -s, x as f32, (page.height - y) as f32]);
        for op in &figure.image.ops {
            match *op {
                SvgOp::Move(x, y) => {
                    c.move_to(x, y);
                }
                SvgOp::Line(x, y) => {
                    c.line_to(x, y);
                }
                SvgOp::Curve(p) => {
                    c.cubic_to(p[0], p[1], p[2], p[3], p[4], p[5]);
                }
                SvgOp::LineWidth(w) => {
                    c.set_line_width(w);
                }
                SvgOp::LineJoin(j) => {
                    c.set_line_join(match j {
                        0 => pdf_writer::types::LineJoinStyle::MiterJoin,
                        1 => pdf_writer::types::LineJoinStyle::RoundJoin,
                        _ => pdf_writer::types::LineJoinStyle::BevelJoin,
                    });
                }
                SvgOp::LineCap(j) => {
                    c.set_line_cap(match j {
                        0 => pdf_writer::types::LineCapStyle::ButtCap,
                        1 => pdf_writer::types::LineCapStyle::RoundCap,
                        _ => pdf_writer::types::LineCapStyle::ProjectingSquareCap,
                    });
                }
                SvgOp::StrokeRgb(r, g, b) => {
                    c.set_stroke_rgb(r as f32, g as f32, b as f32);
                }
                SvgOp::FillRgb(r, g, b) => {
                    c.set_fill_rgb(r as f32, g as f32, b as f32);
                }
                SvgOp::Close => {
                    c.close_path();
                }
                SvgOp::Stroke => {
                    c.stroke();
                }
                SvgOp::CloseStroke => {
                    c.close_and_stroke();
                }
                SvgOp::Fill => {
                    c.fill_nonzero();
                }
                SvgOp::FillEvenOdd => {
                    c.fill_even_odd();
                }
                SvgOp::FillStroke => {
                    c.fill_nonzero_and_stroke();
                }
            }
        }
        c.restore_state();
    }

    fn push_transform(&mut self, [a, b, c, d, e, f]: sile_core::transform::Matrix) {
        self.end_mark();
        let page = self.current.as_mut().expect("no current page");
        let h = page.height;
        page.content.save_state();
        page.content.transform([a, -b, -c, d, c * h + e, h * (1.0 - d) - f].map(|v| v as f32));
    }

    fn pop_transform(&mut self) {
        self.end_mark();
        self.current.as_mut().expect("no current page").content.restore_state();
    }

    fn link(&mut self, rect: [f64; 4], dest: &LinkDest) {
        self.add_link(rect, dest.clone());
    }

    fn set_tag(&mut self, tag: Option<u32>) {
        self.current.as_mut().expect("no current page").tag = tag;
    }

    fn frame_outline(&mut self, frame: &sile_core::frame::FrameGeometry) {
        self.mark();
        let page = self.current.as_mut().expect("no current page");
        let y = page.height - frame.bottom;
        page.content.save_state();
        page.content.set_stroke_rgb(0.8, 0.0, 0.0);
        page.content.set_line_width(0.5);
        page.content.rect(frame.left as f32, y as f32, frame.width() as f32, frame.height() as f32);
        page.content.stroke();
        page.content.restore_state();
    }
}

fn write_font(
    pdf: &mut Pdf,
    entry: &FontEntry,
    refs: &FontRefs,
    compress: bool,
) -> Result<(), PdfError> {
    let (raw_data, face_index) = entry.face.raw_data();
    let variations: Vec<(subsetter::Tag, f32)> =
        entry.variations.iter().map(|(tag, v)| (subsetter::Tag::new(tag), *v)).collect();
    // Content streams address glyphs by their id in the subset, so the
    // embedded font is always the subset and CIDs are its glyph ids.
    let font_data = subsetter::subset_with_variations(raw_data, face_index, &variations, &entry.remapper)
        .map_err(|e| PdfError::Font(format!("{}: {e:?}", entry.pdf_name)))?;
    let subset = ttf_parser::Face::parse(&font_data, 0).map_err(|e| PdfError::Font(format!("{}: {e}", entry.pdf_name)))?;
    let cff = subset.tables().cff.is_some();
    let scale = 1000.0 / subset.units_per_em() as f32;
    let face = ttf_parser::Face::parse(raw_data, face_index).map_err(|e| PdfError::Font(format!("{}: {e}", entry.pdf_name)))?;
    let base_name = subset_name(&face, &entry.pdf_name, &font_data);

    let font_bytes = if compress {
        compress_data(&font_data)
    } else {
        font_data.clone()
    };

    let mut type0 = pdf.type0_font(refs.type0);
    type0.base_font(Name(base_name.as_bytes()));
    type0.encoding_predefined(Name(b"Identity-H"));
    type0.descendant_font(refs.cid_font);
    type0.to_unicode(refs.tounicode);
    type0.finish();

    let mut cid = pdf.cid_font(refs.cid_font);
    cid.subtype(if cff { CidFontType::Type0 } else { CidFontType::Type2 });
    cid.base_font(Name(base_name.as_bytes()));
    cid.system_info(SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    });
    cid.font_descriptor(refs.descriptor);
    if !cff {
        cid.cid_to_gid_map_predefined(Name(b"Identity"));
    }
    cid.default_width(0.0);
    let mut widths = cid.widths();
    for g in 0..subset.number_of_glyphs() {
        let advance = subset.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0);
        widths.same(g, g, advance as f32 * scale);
    }
    widths.finish();
    cid.finish();

    let bbox = face.global_bounding_box();
    let italic_angle = face.italic_angle();
    let mut flags = FontFlags::SYMBOLIC;
    flags.set(FontFlags::ITALIC, italic_angle != 0.0);
    flags.set(FontFlags::FIXED_PITCH, face.is_monospaced());
    let ascent = face.ascender() as f32 * scale;
    let descent = face.descender() as f32 * scale;
    let cap_height = face.capital_height().filter(|h| *h > 0).map_or(ascent, |h| h as f32 * scale);
    let weight = face.tables().os2.map_or(400, |os2| os2.weight().to_number()) as f32;

    let mut desc = pdf.font_descriptor(refs.descriptor);
    desc.name(Name(base_name.as_bytes()));
    desc.flags(flags);
    desc.bbox(Rect::new(
        bbox.x_min as f32 * scale,
        bbox.y_min as f32 * scale,
        bbox.x_max as f32 * scale,
        bbox.y_max as f32 * scale,
    ));
    desc.italic_angle(italic_angle);
    desc.ascent(ascent);
    desc.descent(descent);
    desc.cap_height(cap_height);
    // The usual estimate from the weight class, as in Typst and pdfTeX.
    desc.stem_v(10.0 + 0.244 * (weight - 50.0));
    if cff {
        desc.font_file3(refs.font_file);
    } else {
        desc.font_file2(refs.font_file);
    }
    desc.finish();

    let mut stream = pdf.stream(refs.font_file, &font_bytes);
    if compress {
        stream.filter(Filter::FlateDecode);
    }
    if cff {
        stream.pair(Name(b"Subtype"), Name(b"OpenType"));
    } else {
        stream.pair(Name(b"Length1"), font_data.len() as i32);
    }
    stream.finish();

    // ToUnicode CMap
    let cmap_data = build_tounicode_cmap(&entry.cid_to_unicode);
    let cmap_bytes = if compress {
        compress_data(&cmap_data)
    } else {
        cmap_data
    };
    let mut cmap_stream = pdf.stream(refs.tounicode, &cmap_bytes);
    if compress {
        cmap_stream.filter(Filter::FlateDecode);
    }
    cmap_stream.finish();

    Ok(())
}

/// The font's PostScript name behind the six-letter tag that marks a
/// subset, the tag differing between subsets of one font.
fn subset_name(face: &ttf_parser::Face, pdf_name: &str, subset: &[u8]) -> String {
    let postscript = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .find_map(|n| n.to_string())
        .map(|n| n.chars().filter(|c| c.is_ascii_graphic() && !"[](){}<>/%".contains(*c)).collect::<String>())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| pdf_name.to_string());
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in pdf_name.as_bytes().iter().chain(subset) {
        hash = (hash ^ b as u64).wrapping_mul(0x0100_0000_01b3);
    }
    let tag: String = (0..6).map(|i| (b'A' + ((hash >> (i * 5)) % 26) as u8) as char).collect();
    format!("{tag}+{postscript}")
}

// ---------------------------------------------------------------------------
// ToUnicode CMap
// ---------------------------------------------------------------------------

fn build_tounicode_cmap(gid_to_unicode: &BTreeMap<u16, String>) -> Vec<u8> {
    let mut cmap = String::new();
    cmap.push_str("/CIDInit /ProcSet findresource begin\n");
    cmap.push_str("12 dict begin\n");
    cmap.push_str("begincmap\n");
    cmap.push_str("/CIDSystemInfo <<\n");
    cmap.push_str("  /Registry (Adobe)\n");
    cmap.push_str("  /Ordering (UCS)\n");
    cmap.push_str("  /Supplement 0\n");
    cmap.push_str(">> def\n");
    cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
    cmap.push_str("/CMapType 2 def\n");
    cmap.push_str("1 begincodespacerange\n");
    cmap.push_str("<0000> <FFFF>\n");
    cmap.push_str("endcodespacerange\n");

    if !gid_to_unicode.is_empty() {
        let entries: Vec<(&u16, &String)> = gid_to_unicode.iter().collect();

        // Write in batches of 100 (PDF limit)
        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for &(gid, text) in chunk {
                let utf16: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
                cmap.push_str(&format!("<{gid:04X}> <{utf16}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
    }

    cmap.push_str("endcmap\n");
    cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
    cmap.push_str("end\n");
    cmap.push_str("end\n");
    cmap.into_bytes()
}

// ---------------------------------------------------------------------------
// Image embedding
// ---------------------------------------------------------------------------

fn write_image(
    pdf: &mut Pdf,
    img: &ImageEntry,
    img_ref: Ref,
    smask_ref: Option<Ref>,
    compress: bool,
) {
    if img.is_jpeg {
        // JPEG: embed raw bytes with DCTDecode
        let mut xobj = pdf.image_xobject(img_ref, &img.data);
        xobj.filter(Filter::DctDecode);
        xobj.width(img.width as i32);
        xobj.height(img.height as i32);
        xobj.color_space().device_rgb();
        xobj.bits_per_component(8);
        xobj.finish();
    } else {
        // PNG (decoded pixels): embed with FlateDecode
        let pixel_data = if compress {
            compress_data(&img.data)
        } else {
            img.data.clone()
        };

        let mut xobj = pdf.image_xobject(img_ref, &pixel_data);
        if compress {
            xobj.filter(Filter::FlateDecode);
        }
        xobj.width(img.width as i32);
        xobj.height(img.height as i32);
        if img.channels == 1 {
            xobj.color_space().device_gray();
        } else {
            xobj.color_space().device_rgb();
        }
        xobj.bits_per_component(8);

        if let Some(smask_ref) = smask_ref {
            xobj.s_mask(smask_ref);
        }
        xobj.finish();

        // Write alpha mask if present
        if let (Some(alpha), Some(smask_ref)) = (&img.alpha, smask_ref) {
            let alpha_data = if compress {
                compress_data(alpha)
            } else {
                alpha.clone()
            };
            let mut smask = pdf.image_xobject(smask_ref, &alpha_data);
            if compress {
                smask.filter(Filter::FlateDecode);
            }
            smask.width(img.width as i32);
            smask.height(img.height as i32);
            smask.color_space().device_gray();
            smask.bits_per_component(8);
            smask.finish();
        }
    }
}

// ---------------------------------------------------------------------------
// Annotation writing
// ---------------------------------------------------------------------------

fn write_annotation(pdf: &mut Pdf, annot: &LinkAnnotation, annot_ref: Ref, page: &BuiltPage, dest: Option<Dest>, struct_parent: Option<i32>) {
    let (dx, dy) = page.offset;
    let [x1, y1, x2, y2] = annot.rect;
    let (x1, x2) = (x1 + dx, x2 + dx);
    let pdf_y1 = page.height - y2 + dy;
    let pdf_y2 = page.height - y1 + dy;

    let mut writer = pdf.annotation(annot_ref);
    writer.subtype(pdf_writer::types::AnnotationType::Link);
    writer.rect(Rect::new(x1 as f32, pdf_y1 as f32, x2 as f32, pdf_y2 as f32));
    writer.border(0.0, 0.0, 0.0, None);
    if let Some(key) = struct_parent {
        writer.struct_parent(key);
        writer.contents(TextStr(match &annot.dest {
            LinkDest::Uri(uri) => uri,
            LinkDest::Internal(name) => name,
        }));
    }

    match &annot.dest {
        LinkDest::Uri(uri) => {
            writer
                .action()
                .action_type(pdf_writer::types::ActionType::Uri)
                .uri(Str(uri.as_bytes()));
        }
        LinkDest::Internal(_) => {
            if let Some(d) = dest {
                writer
                    .action()
                    .action_type(pdf_writer::types::ActionType::GoTo)
                    .destination()
                    .page(d.page)
                    .xyz(d.x, d.y, None);
            }
        }
    }
    writer.finish();
}

// ---------------------------------------------------------------------------
// Structure tree writing
// ---------------------------------------------------------------------------

/// References for the structure tree's objects. Elements nothing was
/// drawn for are left out.
struct StructRefs {
    root: Ref,
    elements: Vec<Option<Ref>>,
    /// Each page's array of the elements its marked content belongs to.
    page_parents: Vec<Option<Ref>>,
}

impl StructRefs {
    fn new(tree: &StructTree, tagged: &[Vec<Tagged>], alloc: &mut RefAlloc) -> Self {
        let drawn = |tag: u32| tagged.get(tag as usize).is_some_and(|t| !t.is_empty());
        let mut keep = vec![false; tree.elements.len()];
        for (i, element) in tree.elements.iter().enumerate().rev() {
            keep[i] |= i == 0
                || element.kids.iter().any(|kid| match *kid {
                    StructKid::Content(tag) => drawn(tag),
                    StructKid::Element(e) => keep[e],
                });
        }
        let root = alloc.bump();
        let elements = keep.iter().map(|&k| k.then(|| alloc.bump())).collect();
        let mut pages = std::collections::BTreeSet::new();
        for item in tagged.iter().flatten() {
            if let Tagged::Mark { page, .. } = item {
                pages.insert(*page);
            }
        }
        let page_count = pages.last().map_or(0, |p| p + 1);
        let page_parents = (0..page_count).map(|p| pages.contains(&p).then(|| alloc.bump())).collect();
        Self { root, elements, page_parents }
    }

    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        pdf: &mut Pdf,
        tree: &StructTree,
        tagged: &[Vec<Tagged>],
        pages: &[BuiltPage],
        page_refs: &[Ref],
        annot_refs: &[Vec<Ref>],
        annot_keys: &[Vec<Option<i32>>],
    ) {
        let owner = |tag: u32| self.elements[tree.owner(tag)].expect("drawn content's element is kept");
        let mut root = pdf.indirect(self.root).start::<pdf_writer::writers::StructTreeRoot>();
        root.child(self.elements[0].expect("document"));
        let mut next_key = pages.len() as i32;
        {
            let mut parent_tree = root.parent_tree();
            let mut nums = parent_tree.nums();
            for (i, parents) in self.page_parents.iter().enumerate() {
                if let Some(r) = parents {
                    nums.insert(i as i32, *r);
                }
            }
            for (page, keys) in pages.iter().zip(annot_keys) {
                for (annot, key) in page.annotations.iter().zip(keys) {
                    if let (Some(tag), Some(key)) = (annot.tag, key) {
                        nums.insert(*key, owner(tag));
                        next_key = next_key.max(key + 1);
                    }
                }
            }
        }
        root.parent_tree_next_key(next_key);
        root.finish();

        for (page, parents) in pages.iter().zip(&self.page_parents) {
            if let Some(r) = parents {
                pdf.indirect(*r).array().items(page.marks.iter().map(|&tag| owner(tag)));
            }
        }

        for (i, element) in tree.elements.iter().enumerate() {
            let Some(r) = self.elements[i] else { continue };
            let mut writer = pdf.struct_element(r);
            writer.custom_kind(Name(element.role.name().as_bytes()));
            writer.parent(element.parent.and_then(|p| self.elements[p]).unwrap_or(self.root));
            if let Some(lang) = &element.lang {
                writer.lang(TextStr(lang));
            }
            if let Some(alt) = &element.alt {
                writer.alt(TextStr(alt));
            }
            if let Some(text) = &element.actual_text {
                writer.actual_text(TextStr(text));
            }
            match element.role {
                Role::TH => {
                    writer.attributes().push().table().scope(TableHeaderScope::Column);
                }
                Role::Note => {
                    writer.id(Str(format!("note{i}").as_bytes()));
                }
                _ => {}
            }
            let mut kids = writer.children();
            for kid in &element.kids {
                match *kid {
                    StructKid::Element(e) => {
                        if let Some(r) = self.elements[e] {
                            kids.struct_element(r);
                        }
                    }
                    StructKid::Content(tag) => {
                        for item in tagged.get(tag as usize).into_iter().flatten() {
                            match *item {
                                Tagged::Mark { page, mcid } => {
                                    kids.marked_content_ref().page(page_refs[page]).marked_content_id(mcid as i32);
                                }
                                Tagged::Link { page, index } => {
                                    kids.object_ref().page(page_refs[page]).object(annot_refs[page][index]);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Outline (bookmarks) writing
// ---------------------------------------------------------------------------

fn write_outlines(pdf: &mut Pdf, bookmarks: &[Bookmark], refs: &[Ref], outline_ref: Ref, dests: &[Option<Dest>]) {
    let parents = outline_parents(bookmarks);
    let children = |parent: Option<usize>| -> Vec<usize> { (0..bookmarks.len()).filter(|&i| parents[i] == parent).collect() };
    let descendants = |i: usize| {
        let mut n = 0;
        let mut j = i + 1;
        while j < bookmarks.len() && is_descendant(&parents, j, i) {
            n += 1;
            j += 1;
        }
        n
    };

    let top = children(None);
    let mut outline = pdf.outline(outline_ref);
    outline.first(refs[top[0]]);
    outline.last(refs[*top.last().unwrap()]);
    outline.count(bookmarks.len() as i32);
    outline.finish();

    for (i, bm) in bookmarks.iter().enumerate() {
        let siblings = children(parents[i]);
        let pos = siblings.iter().position(|&s| s == i).unwrap();
        let mut item = pdf.outline_item(refs[i]);
        item.title(TextStr(&bm.title));
        item.parent(parents[i].map_or(outline_ref, |p| refs[p]));
        if pos > 0 {
            item.prev(refs[siblings[pos - 1]]);
        }
        if let Some(&next) = siblings.get(pos + 1) {
            item.next(refs[next]);
        }
        let kids = children(Some(i));
        if let (Some(&first), Some(&last)) = (kids.first(), kids.last()) {
            item.first(refs[first]);
            item.last(refs[last]);
            item.count(descendants(i));
        }
        if let Some(d) = dests[i] {
            item.dest().page(d.page).xyz(d.x, d.y, None);
        }
        item.finish();
    }
}

/// Each bookmark's parent: the closest earlier one with a lower level.
fn outline_parents(bookmarks: &[Bookmark]) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    bookmarks
        .iter()
        .enumerate()
        .map(|(i, bm)| {
            while stack.last().is_some_and(|&p| bookmarks[p].level >= bm.level) {
                stack.pop();
            }
            let parent = stack.last().copied();
            stack.push(i);
            parent
        })
        .collect()
}

fn is_descendant(parents: &[Option<usize>], mut i: usize, ancestor: usize) -> bool {
    while let Some(p) = parents[i] {
        if p == ancestor {
            return true;
        }
        i = p;
    }
    false
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

fn compress_data(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sile_core::builder::Arranger;
    use sile_core::frame::PaperSize;
    use sile_core::length::Length;
    use sile_core::node::Node;
    use sile_core::node::{GlyphData, NNode, VBox};


    fn test_page(number: usize) -> sile_core::pagebuilder::Page {
        let content = sile_core::frame::FrameGeometry {
            id: "content".into(),
            left: 72.0,
            top: 72.0,
            right: PaperSize::A4.width - 72.0,
            bottom: PaperSize::A4.height - 72.0,
            next: None,
            direction: None,
            tate: false,
            balanced: false,
        };
        sile_core::pagebuilder::Page::new(number, PaperSize::A4, vec![content])
    }
    #[test]
    fn empty_document() {
        let out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        let bytes = out.finish().unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn single_empty_page() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.begin_page(595.0, 842.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 100);
    }

    #[test]
    fn document_with_metadata() {
        let metadata = Metadata {
            title: Some("Test Document".to_string()),
            author: Some("Test Author".to_string()),
            subject: Some("Testing".to_string()),
            ..Default::default()
        };
        let mut out = PdfOutputter::new(metadata, PdfOptions::default());
        out.begin_page(595.0, 842.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn multiple_pages() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        for _ in 0..5 {
            out.begin_page(595.0, 842.0);
            out.end_page();
        }
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn draw_rules() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.begin_page(595.0, 842.0);
        out.set_color(Color::Rgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        });
        out.draw_rule(72.0, 72.0, 200.0, 2.0);
        out.set_color(Color::Rgb {
            r: 0.0,
            g: 0.0,
            b: 1.0,
        });
        out.draw_rule(72.0, 80.0, 200.0, 2.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn bookmarks() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        for name in ["one", "two"] {
            out.begin_page(595.0, 842.0);
            out.add_destination(name, 72.0, 72.0);
            out.end_page();
        }
        for (title, level, dest) in [("Chapter 1", 1, "one"), ("Section 1.1", 2, "one"), ("Chapter 2", 1, "two")] {
            out.add_bookmark(Bookmark { title: title.into(), level, dest: dest.into() });
        }

        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn outline_nests_under_lower_levels() {
        let bm = |level| Bookmark { title: String::new(), level, dest: String::new() };
        let parents = outline_parents(&[bm(1), bm(2), bm(3), bm(2), bm(1), bm(3)]);
        assert_eq!(parents, [None, Some(0), Some(1), Some(0), None, Some(4)]);
    }

    #[test]
    fn link_annotation() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.begin_page(595.0, 842.0);
        out.add_link(
            [72.0, 72.0, 200.0, 84.0],
            LinkDest::Uri("https://example.com".to_string()),
        );
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn rotation() {
        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.begin_page(595.0, 842.0);
        out.push_state();
        out.rotate(45.0, 297.5, 421.0);
        out.draw_rule(200.0, 400.0, 195.0, 2.0);
        out.pop_state();
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn uncompressed_output() {
        let options = PdfOptions { compress: false, ..Default::default() };
        let mut out = PdfOutputter::new(Metadata::default(), options);
        out.begin_page(595.0, 842.0);
        out.draw_rule(72.0, 72.0, 100.0, 1.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn render_pages_from_page_builder() {

        // Create a simple page with VBox content
        let mut page = test_page(1);
        let vbox = VBox {
            width: Length::pt(300.0),
            height: Length::pt(12.0),
            depth: Length::pt(3.0),
            nodes: vec![Node::hbox(300.0, 12.0, 3.0)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
            reversed: false,
        };
        page.add_frame_content("content", vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.render_pages(&[page]);
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn render_nnode_with_glyphs() {

        // Load a system font for the test
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let face = Arc::new(face);

        // Create glyph data for "Hi"
        let gid_h = face.glyph_id('H').unwrap_or(0);
        let gid_i = face.glyph_id('i').unwrap_or(0);
        let units_per_em = face.units_per_em() as f64;
        let font_size = 12.0;
        let scale = font_size / units_per_em;

        let w_h = face.advance_width(gid_h).unwrap_or(600) as f64 * scale;
        let w_i = face.advance_width(gid_i).unwrap_or(300) as f64 * scale;

        let glyphs = vec![
            GlyphData {
                gid: gid_h,
                width: w_h,
                x_advance: w_h,
                ..Default::default()
            },
            GlyphData {
                gid: gid_i,
                width: w_i,
                x_advance: w_i,
                ..Default::default()
            },
        ];

        let nnode = NNode::with_glyphs("Hi", glyphs, "body", font_size, w_h + w_i, 10.0, 3.0);
        let vbox = VBox {
            width: Length::pt(451.0),
            height: Length::pt(12.0),
            depth: Length::pt(3.0),
            nodes: vec![Node::NNode(nnode)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
            reversed: false,
        };

        let mut page = test_page(1);
        page.add_frame_content("content", vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.register_font("body", face, Vec::new());
        out.render_pages(&[page]);
        let bytes = out.finish().unwrap();

        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 500, "PDF with embedded font should be substantial");
    }

    #[test]
    fn render_colored_text() {

        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let face = Arc::new(face);
        let gid = face.glyph_id('A').unwrap_or(0);
        let units_per_em = face.units_per_em() as f64;
        let font_size = 12.0;
        let scale = font_size / units_per_em;
        let w = face.advance_width(gid).unwrap_or(600) as f64 * scale;

        let mut nnode = NNode::with_glyphs(
            "A",
            vec![GlyphData {
                gid,
                width: w,
                    x_advance: w,
                ..Default::default()
            }],
            "body",
            font_size,
            w,
            10.0,
            3.0,
        );
        nnode.color = Some(Color::Rgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        });

        let vbox = VBox {
            width: Length::pt(451.0),
            height: Length::pt(12.0),
            depth: Length::pt(3.0),
            nodes: vec![Node::NNode(nnode)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
            reversed: false,
        };

        let mut page = test_page(1);
        page.add_frame_content("content", vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(Metadata::default(), PdfOptions::default());
        out.register_font("body", face, Vec::new());
        out.render_pages(&[page]);
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn tounicode_cmap_generation() {
        let mut map = BTreeMap::new();
        map.insert(72u16, "H".to_string());
        map.insert(105u16, "i".to_string());
        map.insert(300u16, "fi".to_string());
        map.insert(301u16, "\u{1D400}".to_string());

        let cmap = build_tounicode_cmap(&map);
        let cmap_str = String::from_utf8(cmap).unwrap();

        assert!(cmap_str.contains("<012C> <00660069>"));
        assert!(cmap_str.contains("<012D> <D835DC00>"));
        assert!(cmap_str.contains("beginbfchar"));
        assert!(cmap_str.contains("endbfchar"));
        assert!(cmap_str.contains("endcmap"));
    }

    #[test]
    fn multi_page_with_fonts_and_bookmarks() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let face = Arc::new(face);

        let gid = face.glyph_id('X').unwrap_or(0);
        let units_per_em = face.units_per_em() as f64;
        let font_size = 14.0;
        let scale = font_size / units_per_em;
        let w = face.advance_width(gid).unwrap_or(600) as f64 * scale;

        let mut pages = Vec::new();
        for i in 0..3 {
            let nnode = NNode::with_glyphs(
                "X",
                vec![GlyphData {
                    gid,
                    width: w,
                    x_advance: w,
                    ..Default::default()
                }],
                "body",
                font_size,
                w,
                12.0,
                3.0,
            );
            let vbox = VBox {
                width: Length::pt(451.0),
                height: Length::pt(14.0),
                depth: Length::pt(3.0),
                nodes: vec![Node::NNode(nnode)],
                ratio: 0.0,
                misfit: false,
                explicit: false,
                reversed: false,
            };
            let mut page = test_page(i + 1);
            page.add_frame_content("content", vec![Node::VBox(vbox)]);
            pages.push(page);
        }

        let metadata = Metadata { title: Some("Multi-page Test".to_string()), ..Default::default() };
        let mut out = PdfOutputter::new(metadata, PdfOptions::default());
        out.register_font("body", face, Vec::new());
        out.render_pages(&pages);

        out.add_bookmark(Bookmark { title: "Page 1".into(), level: 0, dest: "nowhere".into() });

        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1000);
    }

    fn load_any_system_font() -> Option<FontFace> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let id = db.faces().next()?.id;
        let mut data_out: Option<(Vec<u8>, u32)> = None;
        db.with_face_data(id, |data, index| {
            data_out = Some((data.to_vec(), index));
        });
        let (data, index) = data_out?;
        FontFace::from_bytes(data, index).ok()
    }

    fn render_with_test_fonts(fonts: &[(&str, &str)]) -> Vec<u8> {
        use sile_pages::DocumentBuilder;
        use sile_core::font::FontSpec;
        use sile_core::frame::PaperSize;
        let mut doc = DocumentBuilder::new(PaperSize::A5);
        doc.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts"));
        for (family, variations) in fonts {
            let spec = FontSpec { family: Some(family.to_string()), variations: variations.to_string(), ..Default::default() };
            doc.set_font_spec(spec).unwrap();
            doc.add_text("Hi");
            doc.new_paragraph().unwrap();
        }
        render(&doc.lay_out().unwrap(), PdfOptions { compress: false, ..Default::default() }).unwrap()
    }

    /// The embedded font programs, found by their `/Length1`.
    fn truetype_programs(pdf: &[u8]) -> Vec<&[u8]> {
        let find = |from: usize, what: &[u8]| pdf[from..].windows(what.len()).position(|w| w == what).map(|p| p + from);
        let mut programs = Vec::new();
        let mut from = 0;
        while let Some(at) = find(from, b"/Length1 ") {
            let digits: String = pdf[at + 9..].iter().take_while(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
            let start = find(at, b"stream\n").unwrap() + 7;
            programs.push(&pdf[start..start + digits.parse::<usize>().unwrap()]);
            from = start;
        }
        programs
    }

    #[test]
    fn cff_fonts_embed_as_opentype() {
        let pdf = render_with_test_fonts(&[("Libertinus Mono", "")]);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/CIDFontType0"));
        assert!(text.contains("/FontFile3"));
        assert!(text.contains("/Subtype /OpenType"));
        assert!(!text.contains("/CIDToGIDMap"));
        assert!(text.contains("+LibertinusMono-Regular"));
    }

    #[test]
    fn variable_fonts_embed_the_instance_used() {
        let pdf = render_with_test_fonts(&[("Tourney", "wght=100"), ("Tourney", "wght=900")]);
        let outlines: Vec<Vec<u8>> = truetype_programs(&pdf)
            .iter()
            .map(|data| {
                let face = ttf_parser::Face::parse(data, 0).unwrap();
                face.raw_face().table(ttf_parser::Tag::from_bytes(b"glyf")).unwrap().to_vec()
            })
            .collect();
        assert_eq!(outlines.len(), 2);
        assert_ne!(outlines[0], outlines[1]);
    }
}

