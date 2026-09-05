use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::color::Color;
use crate::font::FontFace;
use crate::frame::PageLayout;
use crate::node::Node;
use crate::pagebuilder::Page;

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

#[derive(Debug, Clone)]
pub struct PdfConfig {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: String,
    pub compress: bool,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            title: None,
            author: None,
            subject: None,
            creator: "sile-rust".to_string(),
            compress: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Image types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
}

// ---------------------------------------------------------------------------
// Link / Bookmark types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum LinkDest {
    Uri(String),
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    pub rect: [f64; 4],
    pub dest: LinkDest,
}

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub title: String,
    pub page_index: usize,
    pub level: u32,
    pub y_position: f64,
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
    used_glyphs: BTreeSet<u16>,
    gid_to_unicode: HashMap<u16, char>,
    pdf_name: String,
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
    content: Vec<u8>,
    annotations: Vec<LinkAnnotation>,
}

struct CurrentPage {
    width: f64,
    height: f64,
    content: Content,
    annotations: Vec<LinkAnnotation>,
    current_font: Option<(String, f64)>,
    current_color: Option<Color>,
}

// ---------------------------------------------------------------------------
// PdfOutputter
// ---------------------------------------------------------------------------

pub struct PdfOutputter {
    config: PdfConfig,
    fonts: HashMap<String, FontEntry>,
    font_counter: usize,
    images: Vec<ImageEntry>,
    bookmarks: Vec<Bookmark>,
    pages: Vec<BuiltPage>,
    current: Option<CurrentPage>,
}

impl PdfOutputter {
    pub fn new(config: PdfConfig) -> Self {
        Self {
            config,
            fonts: HashMap::new(),
            font_counter: 0,
            images: Vec::new(),
            bookmarks: Vec::new(),
            pages: Vec::new(),
            current: None,
        }
    }

    // -- Font management ---------------------------------------------------

    pub fn register_font(&mut self, key: &str, face: Arc<FontFace>) {
        if self.fonts.contains_key(key) {
            return;
        }
        let pdf_name = format!("F{}", self.font_counter);
        self.font_counter += 1;
        self.fonts.insert(
            key.to_string(),
            FontEntry {
                face,
                used_glyphs: BTreeSet::new(),
                gid_to_unicode: HashMap::new(),
                pdf_name,
            },
        );
    }

    fn track_glyph(&mut self, font_key: &str, gid: u16, codepoint: Option<char>) {
        if let Some(entry) = self.fonts.get_mut(font_key) {
            entry.used_glyphs.insert(gid);
            if let Some(cp) = codepoint {
                entry.gid_to_unicode.entry(gid).or_insert(cp);
            }
        }
    }

    // -- Image management --------------------------------------------------

    pub fn add_image_jpeg(&mut self, data: Vec<u8>) -> Result<usize, PdfError> {
        let reader = image::ImageReader::new(std::io::Cursor::new(&data))
            .with_guessed_format()
            .map_err(|e| PdfError::Image(e.to_string()))?;
        let dims = reader.into_dimensions().map_err(|e| PdfError::Image(e.to_string()))?;

        let idx = self.images.len();
        self.images.push(ImageEntry {
            data,
            width: dims.0,
            height: dims.1,
            channels: 3,
            is_jpeg: true,
            alpha: None,
        });
        Ok(idx)
    }

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
        });
    }

    pub fn end_page(&mut self) {
        let page = self.current.take().expect("begin_page() not called");
        self.pages.push(BuiltPage {
            width: page.width,
            height: page.height,
            content: page.content.finish(),
            annotations: page.annotations,
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

        // Track glyph usage for font subsetting
        for &(gid, _, _, _, _) in glyphs {
            self.track_glyph_on_font(font_key, gid);
        }

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
        for &(gid, x_advance, y_advance, x_offset, y_offset) in glyphs {
            let px = cur_x + x_offset;
            let py = page_height - (cur_y - y_offset);
            page.content.set_text_matrix([1.0, 0.0, 0.0, 1.0, px as f32, py as f32]);
            page.content.show(Str(&gid.to_be_bytes()));
            cur_x += x_advance;
            cur_y += y_advance;
        }

        page.content.end_text();
    }

    fn track_glyph_on_font(&mut self, font_key: &str, gid: u16) {
        if let Some(entry) = self.fonts.get_mut(font_key) {
            entry.used_glyphs.insert(gid);
        }
    }

    pub fn draw_rule(&mut self, x: f64, y: f64, width: f64, height: f64) {
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
        let page = self.current.as_mut().expect("no current page");
        page.content.save_state();
    }

    pub fn pop_state(&mut self) {
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
        page.annotations.push(LinkAnnotation { rect, dest });
    }

    // -- High-level: render from Page objects ---

    pub fn render_pages(&mut self, pages: &[Page], layout: &PageLayout) {
        for page in pages {
            self.begin_page(layout.paper.width, layout.paper.height);
            self.render_page_content(page, layout);
            self.end_page();
        }
    }

    fn render_page_content(&mut self, page: &Page, layout: &PageLayout) {
        for (frame_id, nodes) in &page.frames {
            let frame = layout.frame(*frame_id);
            let frame_x = frame.left;
            let frame_top = frame.top;
            let mut cursor_y = frame_top;

            for node in nodes {
                match node {
                    Node::VBox(vbox) => {
                        let line_height = vbox.height.to_pt().unwrap_or(0.0);
                        let line_depth = vbox.depth.to_pt().unwrap_or(0.0);
                        let baseline_y = cursor_y + line_height;
                        let mut cursor_x = frame_x;

                        for hnode in &vbox.nodes {
                            match hnode {
                                Node::NNode(nnode) => {
                                    self.render_nnode(nnode, cursor_x, baseline_y);
                                    cursor_x += nnode.width.to_pt().unwrap_or(0.0);
                                }
                                Node::Glue(g) | Node::HFillGlue(g) | Node::HssGlue(g) => {
                                    let natural = g.width.length.to_pt().unwrap_or(0.0);
                                    let scaled = if vbox.ratio > 0.0 {
                                        natural + g.width.stretch.to_pt().unwrap_or(0.0) * vbox.ratio
                                    } else if vbox.ratio < 0.0 {
                                        natural + g.width.shrink.to_pt().unwrap_or(0.0) * vbox.ratio
                                    } else {
                                        natural
                                    };
                                    cursor_x += scaled.max(0.0);
                                }
                                Node::Kern(k) => {
                                    cursor_x += k.width.to_pt().unwrap_or(0.0);
                                }
                                Node::HBox(hbox) => {
                                    cursor_x += hbox.width.to_pt().unwrap_or(0.0);
                                }
                                _ => {}
                            }
                        }

                        cursor_y += line_height + line_depth;
                    }
                    Node::VGlue(g)
                    | Node::VFillGlue(g)
                    | Node::VssGlue(g)
                    | Node::ZeroVGlue(g) => {
                        cursor_y += g.height.to_pt().unwrap_or(0.0);
                    }
                    Node::VKern(k) => {
                        cursor_y += k.height.to_pt().unwrap_or(0.0);
                    }
                    _ => {}
                }
            }
        }
    }

    fn render_nnode(&mut self, nnode: &crate::node::NNode, x: f64, baseline_y: f64) {
        if nnode.glyphs.is_empty() || nnode.font_key.is_empty() {
            return;
        }

        // An uncoloured node is black; without the reset, a coloured node
        // earlier on the page would bleed into everything after it.
        self.set_color(nnode.color.unwrap_or(Color::Grayscale { l: 0.0 }));

        // Track glyph usage
        for glyph in &nnode.glyphs {
            self.track_glyph(&nnode.font_key, glyph.gid, None);
        }
        // Track unicode mappings from the text
        let chars: Vec<char> = nnode.text.chars().collect();
        for (i, glyph) in nnode.glyphs.iter().enumerate() {
            if i < chars.len() {
                self.track_glyph(&nnode.font_key, glyph.gid, Some(chars[i]));
            }
        }

        let page = self.current.as_mut().expect("no current page");
        let page_height = page.height;

        let pdf_name = self
            .fonts
            .get(&nnode.font_key)
            .map(|e| e.pdf_name.clone())
            .unwrap_or_else(|| "F0".to_string());

        page.content.begin_text();
        page.content.set_font(Name(pdf_name.as_bytes()), nnode.font_size as f32);

        let mut cur_x = x;
        for glyph in &nnode.glyphs {
            let px = cur_x + glyph.x_offset;
            let py = page_height - (baseline_y - glyph.y_offset);
            page.content
                .set_text_matrix([1.0, 0.0, 0.0, 1.0, px as f32, py as f32]);
            page.content.show(Str(&glyph.gid.to_be_bytes()));
            cur_x += glyph.x_advance;
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
        let font_refs: HashMap<String, FontRefs> = self
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
                        cid_to_gid_map: alloc.bump(),
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

        // -- Write catalog --
        let mut catalog = pdf.catalog(catalog_ref);
        catalog.pages(page_tree_ref);
        if let Some(outline_ref) = outline_ref {
            catalog.outlines(outline_ref);
        }
        catalog.finish();

        // -- Write document info --
        if self.config.title.is_some()
            || self.config.author.is_some()
            || self.config.subject.is_some()
        {
            let info_ref = alloc.bump();
            let mut info = pdf.document_info(info_ref);
            if let Some(ref title) = self.config.title {
                info.title(TextStr(title));
            }
            if let Some(ref author) = self.config.author {
                info.author(TextStr(author));
            }
            if let Some(ref subject) = self.config.subject {
                info.subject(TextStr(subject));
            }
            info.creator(TextStr(&self.config.creator));
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

            let content_bytes = if self.config.compress {
                compress_data(&built_page.content)
            } else {
                built_page.content.clone()
            };

            let mut pg = pdf.page(page_ref);
            pg.media_box(Rect::new(
                0.0,
                0.0,
                built_page.width as f32,
                built_page.height as f32,
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

            pg.finish();

            // Content stream
            let mut stream = pdf.stream(content_ref, &content_bytes);
            if self.config.compress {
                stream.filter(Filter::FlateDecode);
            }
            stream.finish();
        }

        // -- Write fonts --
        for (key, entry) in &self.fonts {
            if let Some(frefs) = font_refs.get(key) {
                write_font(&mut pdf, entry, frefs, self.config.compress)?;
            }
        }

        // -- Write images --
        for (i, img) in self.images.iter().enumerate() {
            let (img_ref, smask_ref) = image_data[i];
            write_image(&mut pdf, img, img_ref, smask_ref, self.config.compress);
        }

        // -- Write annotations --
        for (i, built_page) in self.pages.iter().enumerate() {
            let page_height = built_page.height;
            for (j, annot) in built_page.annotations.iter().enumerate() {
                let annot_ref = annot_refs[i][j];
                write_annotation(&mut pdf, annot, annot_ref, page_height);
            }
        }

        // -- Write bookmarks --
        if let Some(outline_ref) = outline_ref {
            write_outlines(
                &mut pdf,
                &self.bookmarks,
                &bookmark_refs,
                outline_ref,
                &page_ref_list,
                &self.pages,
            );
        }

        Ok(pdf.finish())
    }
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
    cid_to_gid_map: Ref,
}

// ---------------------------------------------------------------------------
// Font embedding
// ---------------------------------------------------------------------------

fn write_font(
    pdf: &mut Pdf,
    entry: &FontEntry,
    refs: &FontRefs,
    compress: bool,
) -> Result<(), PdfError> {
    let (raw_data, face_index) = entry.face.raw_data();
    let base_name = format!("SILE+Font{}", entry.pdf_name);

    // Try subsetting — get both the subsetted data and the GID remapping
    // A registered font that no glyph ended up using still gets referenced
    // by every page's resources; subset it to .notdef alone rather than
    // embedding the whole file.
    let mut gids: Vec<u16> = entry.used_glyphs.iter().copied().collect();
    if gids.is_empty() {
        gids.push(0);
    }
    let subset_result = try_subset(raw_data, face_index, &gids);
    let (font_data, gid_map) = match &subset_result {
        Some(result) => (result.data.as_slice(), Some(&result.gid_map)),
        None => (raw_data, None),
    };

    let font_bytes = if compress {
        compress_data(font_data)
    } else {
        font_data.to_vec()
    };

    // Type0 font (composite)
    let mut type0 = pdf.type0_font(refs.type0);
    type0.base_font(Name(base_name.as_bytes()));
    type0.encoding_predefined(Name(b"Identity-H"));
    type0.descendant_font(refs.cid_font);
    type0.to_unicode(refs.tounicode);
    type0.finish();

    // CIDFont
    let mut cid = pdf.cid_font(refs.cid_font);
    cid.subtype(CidFontType::Type2);
    cid.base_font(Name(base_name.as_bytes()));
    cid.system_info(SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    });
    cid.font_descriptor(refs.descriptor);

    // When we subset, GIDs in the font change. The content stream still uses
    // original GIDs as character codes. We write a CIDToGIDMap stream that
    // translates original GIDs (used as CIDs) → new GIDs in the subset font.
    // Without subsetting, Identity works (CID = GID).
    if gid_map.is_some() {
        cid.cid_to_gid_map_stream(refs.cid_to_gid_map);
    } else {
        cid.cid_to_gid_map_predefined(Name(b"Identity"));
    }
    cid.default_width(1000.0);

    // W (width) array — widths are indexed by CID (= original GID) since
    // that's what the content stream uses as character codes
    if !entry.used_glyphs.is_empty() {
        let units_per_em = entry.face.units_per_em();
        let mut widths = cid.widths();
        for &gid in &entry.used_glyphs {
            let advance = entry.face.advance_width(gid).unwrap_or(0);
            let w = advance as f32 * 1000.0 / units_per_em as f32;
            widths.same(gid, gid, w);
        }
        widths.finish();
    }
    cid.finish();

    // FontDescriptor
    let units_per_em = entry.face.units_per_em();
    let ascent = entry.face.ascender() as f32 * 1000.0 / units_per_em as f32;
    let descent = entry.face.descender() as f32 * 1000.0 / units_per_em as f32;

    let mut desc = pdf.font_descriptor(refs.descriptor);
    desc.name(Name(base_name.as_bytes()));
    desc.flags(FontFlags::SYMBOLIC | FontFlags::NON_SYMBOLIC);
    desc.bbox(Rect::new(0.0, descent, 1000.0, ascent));
    desc.italic_angle(0.0);
    desc.ascent(ascent);
    desc.descent(descent);
    desc.cap_height(ascent * 0.7);
    desc.stem_v(80.0);
    desc.font_file2(refs.font_file);
    desc.finish();

    // Embedded font data
    let mut stream = pdf.stream(refs.font_file, &font_bytes);
    if compress {
        stream.filter(Filter::FlateDecode);
    }
    stream.pair(Name(b"Length1"), font_data.len() as i32);
    stream.finish();

    // CIDToGIDMap stream (only when subsetted)
    if let Some(map) = gid_map {
        let max_cid = entry.used_glyphs.iter().copied().max().unwrap_or(0) as usize;
        // Binary array: 2 bytes per CID, big-endian, from CID 0 to max_cid
        let mut cid_to_gid_data = vec![0u8; (max_cid + 1) * 2];
        for (&old_gid, &new_gid) in map {
            let idx = old_gid as usize * 2;
            if idx + 1 < cid_to_gid_data.len() {
                cid_to_gid_data[idx] = (new_gid >> 8) as u8;
                cid_to_gid_data[idx + 1] = (new_gid & 0xFF) as u8;
            }
        }
        let map_bytes = if compress {
            compress_data(&cid_to_gid_data)
        } else {
            cid_to_gid_data
        };
        let mut map_stream = pdf.stream(refs.cid_to_gid_map, &map_bytes);
        if compress {
            map_stream.filter(Filter::FlateDecode);
        }
        map_stream.finish();
    }

    // ToUnicode CMap
    let cmap_data = build_tounicode_cmap(&entry.gid_to_unicode);
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

struct SubsetResult {
    data: Vec<u8>,
    gid_map: HashMap<u16, u16>, // old GID → new GID
}

fn try_subset(data: &[u8], _face_index: u32, gids: &[u16]) -> Option<SubsetResult> {
    let mapper = subsetter::GlyphRemapper::new_from_glyphs(gids);
    let subset_data = subsetter::subset(data, 0, &mapper).ok()?;
    let mut gid_map = HashMap::new();
    for &old_gid in gids {
        if let Some(new_gid) = mapper.get(old_gid) {
            gid_map.insert(old_gid, new_gid);
        }
    }
    Some(SubsetResult {
        data: subset_data,
        gid_map,
    })
}

// ---------------------------------------------------------------------------
// ToUnicode CMap
// ---------------------------------------------------------------------------

fn build_tounicode_cmap(gid_to_unicode: &HashMap<u16, char>) -> Vec<u8> {
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
        let mut entries: Vec<(u16, char)> = gid_to_unicode.iter().map(|(&g, &c)| (g, c)).collect();
        entries.sort_by_key(|&(g, _)| g);

        // Write in batches of 100 (PDF limit)
        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for &(gid, cp) in chunk {
                let cp_val = cp as u32;
                if cp_val <= 0xFFFF {
                    cmap.push_str(&format!("<{gid:04X}> <{cp_val:04X}>\n"));
                } else {
                    // Surrogate pair for supplementary planes
                    let hi = ((cp_val - 0x10000) >> 10) + 0xD800;
                    let lo = ((cp_val - 0x10000) & 0x3FF) + 0xDC00;
                    cmap.push_str(&format!("<{gid:04X}> <{hi:04X}{lo:04X}>\n"));
                }
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

fn write_annotation(pdf: &mut Pdf, annot: &LinkAnnotation, annot_ref: Ref, page_height: f64) {
    let [x1, y1, x2, y2] = annot.rect;
    let pdf_y1 = page_height - y2;
    let pdf_y2 = page_height - y1;

    let mut writer = pdf.annotation(annot_ref);
    writer.subtype(pdf_writer::types::AnnotationType::Link);
    writer.rect(Rect::new(x1 as f32, pdf_y1 as f32, x2 as f32, pdf_y2 as f32));
    writer.border(0.0, 0.0, 0.0, None);

    match &annot.dest {
        LinkDest::Uri(uri) => {
            writer
                .action()
                .action_type(pdf_writer::types::ActionType::Uri)
                .uri(Str(uri.as_bytes()));
        }
        LinkDest::Internal(_name) => {
            // Internal links require named destinations (future enhancement)
        }
    }
    writer.finish();
}

// ---------------------------------------------------------------------------
// Outline (bookmarks) writing
// ---------------------------------------------------------------------------

fn write_outlines(
    pdf: &mut Pdf,
    bookmarks: &[Bookmark],
    bookmark_refs: &[Ref],
    outline_ref: Ref,
    page_refs: &[Ref],
    pages: &[BuiltPage],
) {
    if bookmarks.is_empty() {
        return;
    }

    // Simple flat outline (all at level 0)
    let first = bookmark_refs[0];
    let last = *bookmark_refs.last().unwrap();

    let mut outline = pdf.outline(outline_ref);
    outline.first(first);
    outline.last(last);
    outline.count(bookmarks.len() as i32);
    outline.finish();

    for (i, bm) in bookmarks.iter().enumerate() {
        let bm_ref = bookmark_refs[i];
        let page_idx = bm.page_index.min(page_refs.len().saturating_sub(1));
        let page_ref = page_refs[page_idx];
        let page_height = pages.get(page_idx).map(|p| p.height).unwrap_or(842.0);
        let pdf_y = page_height - bm.y_position;

        let mut item = pdf.outline_item(bm_ref);
        item.title(TextStr(&bm.title));
        item.parent(outline_ref);

        if i > 0 {
            item.prev(bookmark_refs[i - 1]);
        }
        if i + 1 < bookmarks.len() {
            item.next(bookmark_refs[i + 1]);
        }

        item.dest().page(page_ref).xyz(0.0, pdf_y as f32, None);
        item.finish();
    }
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
    use crate::frame::PaperSize;
    use crate::length::Length;
    use crate::node::{GlyphData, NNode, VBox};

    #[test]
    fn empty_document() {
        let out = PdfOutputter::new(PdfConfig::default());
        let bytes = out.finish().unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn single_empty_page() {
        let mut out = PdfOutputter::new(PdfConfig::default());
        out.begin_page(595.0, 842.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 100);
    }

    #[test]
    fn document_with_metadata() {
        let config = PdfConfig {
            title: Some("Test Document".to_string()),
            author: Some("Test Author".to_string()),
            subject: Some("Testing".to_string()),
            ..Default::default()
        };
        let mut out = PdfOutputter::new(config);
        out.begin_page(595.0, 842.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn multiple_pages() {
        let mut out = PdfOutputter::new(PdfConfig::default());
        for _ in 0..5 {
            out.begin_page(595.0, 842.0);
            out.end_page();
        }
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn draw_rules() {
        let mut out = PdfOutputter::new(PdfConfig::default());
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
        let mut out = PdfOutputter::new(PdfConfig::default());
        out.begin_page(595.0, 842.0);
        out.end_page();
        out.begin_page(595.0, 842.0);
        out.end_page();

        out.add_bookmark(Bookmark {
            title: "Chapter 1".to_string(),
            page_index: 0,
            level: 0,
            y_position: 72.0,
        });
        out.add_bookmark(Bookmark {
            title: "Chapter 2".to_string(),
            page_index: 1,
            level: 0,
            y_position: 72.0,
        });

        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn link_annotation() {
        let mut out = PdfOutputter::new(PdfConfig::default());
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
        let mut out = PdfOutputter::new(PdfConfig::default());
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
        let config = PdfConfig {
            compress: false,
            ..Default::default()
        };
        let mut out = PdfOutputter::new(config);
        out.begin_page(595.0, 842.0);
        out.draw_rule(72.0, 72.0, 100.0, 1.0);
        out.end_page();
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn render_pages_from_page_builder() {
        let layout = crate::frame::PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();

        // Create a simple page with VBox content
        let mut page = crate::pagebuilder::Page::new(1);
        let vbox = VBox {
            width: Length::pt(300.0),
            height: Length::pt(12.0),
            depth: Length::pt(3.0),
            nodes: vec![Node::hbox(300.0, 12.0, 3.0)],
            ratio: 0.0,
            misfit: false,
            explicit: false,
        };
        page.add_frame_content(frame_id, vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(PdfConfig::default());
        out.render_pages(&[page], &layout);
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn render_nnode_with_glyphs() {
        let layout = crate::frame::PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();

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
                x_advance: w_h,
                ..Default::default()
            },
            GlyphData {
                gid: gid_i,
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
        };

        let mut page = crate::pagebuilder::Page::new(1);
        page.add_frame_content(frame_id, vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(PdfConfig::default());
        out.register_font("body", face);
        out.render_pages(&[page], &layout);
        let bytes = out.finish().unwrap();

        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 500, "PDF with embedded font should be substantial");
    }

    #[test]
    fn render_colored_text() {
        let layout = crate::frame::PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();

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
        };

        let mut page = crate::pagebuilder::Page::new(1);
        page.add_frame_content(frame_id, vec![Node::VBox(vbox)]);

        let mut out = PdfOutputter::new(PdfConfig::default());
        out.register_font("body", face);
        out.render_pages(&[page], &layout);
        let bytes = out.finish().unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn tounicode_cmap_generation() {
        let mut map = HashMap::new();
        map.insert(72u16, 'H');
        map.insert(105u16, 'i');

        let cmap = build_tounicode_cmap(&map);
        let cmap_str = String::from_utf8(cmap).unwrap();

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
        let layout = crate::frame::PageLayout::plain(PaperSize::A4, 72.0);
        let frame_id = layout.content_frame_id().unwrap();

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
            };
            let mut page = crate::pagebuilder::Page::new(i + 1);
            page.add_frame_content(frame_id, vec![Node::VBox(vbox)]);
            pages.push(page);
        }

        let mut out = PdfOutputter::new(PdfConfig {
            title: Some("Multi-page Test".to_string()),
            ..Default::default()
        });
        out.register_font("body", face);
        out.render_pages(&pages, &layout);

        out.add_bookmark(Bookmark {
            title: "Page 1".to_string(),
            page_index: 0,
            level: 0,
            y_position: 72.0,
        });
        out.add_bookmark(Bookmark {
            title: "Page 2".to_string(),
            page_index: 1,
            level: 0,
            y_position: 72.0,
        });

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
}
