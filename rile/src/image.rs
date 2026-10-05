//! Images placed in the text or behind pages: rasters, and the first page
//! of a PDF.
// SILE: `image` and `background` packages.

use std::path::Path;
use std::sync::Arc;

use crate::builder::{BuilderError, Typesetter};
use crate::color::Color;
use crate::node::{HBox, Ink};
use crate::length::Length;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    /// The first page of a PDF, drawn as it is.
    Pdf,
}

/// An image file's data and size.
pub struct Image {
    /// What the image is called in debug output, such as its path.
    pub src: String,
    pub data: Arc<Vec<u8>>,
    pub format: ImageFormat,
    pub pixels: (u32, u32),
    /// Resolution in dots per inch; 72 when the file doesn't say.
    pub dpi: (f64, f64),
    /// A PDF page's visible box (left, bottom, right, top), in points.
    pub page_box: Option<[f64; 4]>,
}

impl Image {
    pub fn load(path: impl AsRef<Path>, src: impl Into<String>) -> Result<Self, BuilderError> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| BuilderError::Layout(format!("image {}: {e}", path.display())))?;
        Self::from_bytes(src, data)
    }

    pub fn from_bytes(src: impl Into<String>, data: Vec<u8>) -> Result<Self, BuilderError> {
        let src = src.into();
        let bad = |what: &str| BuilderError::Layout(format!("image {src}: {what}"));
        if data.starts_with(b"%PDF-") {
            let page_box = pdf_page_box(&data).map_err(|what| bad(&what))?;
            let pixels = ((page_box[2] - page_box[0]).ceil() as u32, (page_box[3] - page_box[1]).ceil() as u32);
            return Ok(Self {
                src,
                data: Arc::new(data),
                format: ImageFormat::Pdf,
                pixels,
                dpi: (72.0, 72.0),
                page_box: Some(page_box),
            });
        }
        let (format, pixels, dpi) = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            let (pixels, dpi) = png_header(&data).ok_or_else(|| bad("bad PNG"))?;
            (ImageFormat::Png, pixels, dpi)
        } else if data.starts_with(&[0xff, 0xd8]) {
            let (pixels, dpi) = jpeg_header(&data).ok_or_else(|| bad("bad JPEG"))?;
            (ImageFormat::Jpeg, pixels, dpi)
        } else {
            return Err(bad("only PNG, JPEG and PDF images are supported"));
        };
        Ok(Self { src, data: Arc::new(data), format, pixels, dpi, page_box: None })
    }

    /// Its size in points: a raster's at its resolution, a PDF page's own.
    pub fn natural_size(&self) -> (f64, f64) {
        if let Some([left, bottom, right, top]) = self.page_box {
            return (right - left, top - bottom);
        }
        (self.pixels.0 as f64 * 72.0 / self.dpi.0, self.pixels.1 as f64 * 72.0 / self.dpi.1)
    }

    /// The size it is set at for a `width` and `height` in points, either
    /// of which may be left out to keep its proportions.
    // SILE: `\img`.
    pub fn scaled_size(&self, width: Option<f64>, height: Option<f64>) -> (f64, f64) {
        let (w, h) = self.natural_size();
        match (width, height) {
            (Some(width), Some(height)) => (width, height),
            (Some(width), None) => (width, h * width / w),
            (None, Some(height)) => (w * height / h, height),
            (None, None) => (w, h),
        }
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image").field("src", &self.src).field("pixels", &self.pixels).finish()
    }
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }
}

/// The crop box of a PDF's first page, or its media box.
#[cfg(feature = "pdf-images")]
fn pdf_page_box(data: &[u8]) -> Result<[f64; 4], String> {
    let document = lopdf::Document::load_mem(data).map_err(|e| format!("bad PDF: {e}"))?;
    let page = document.get_pages().into_values().next().ok_or("the PDF has no pages")?;
    let number = |object: &lopdf::Object| match document.dereference(object).ok()?.1 {
        lopdf::Object::Integer(n) => Some(*n as f64),
        lopdf::Object::Real(n) => Some(f64::from(*n)),
        _ => None,
    };
    // A page inherits its boxes from the page tree above it.
    let inherited = |key: &[u8]| {
        let mut node = document.get_dictionary(page).ok()?;
        for _ in 0..64 {
            if let Ok(found) = node.get(key) {
                let corners = document.dereference(found).ok()?.1.as_array().ok()?;
                let corners: Vec<f64> = corners.iter().filter_map(number).collect();
                return <[f64; 4]>::try_from(corners).ok();
            }
            node = document.get_dictionary(node.get(b"Parent").ok()?.as_reference().ok()?).ok()?;
        }
        None
    };
    let [x0, y0, x1, y1] = inherited(b"CropBox").or_else(|| inherited(b"MediaBox")).ok_or("the PDF's page has no size")?;
    Ok([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
}

#[cfg(not(feature = "pdf-images"))]
fn pdf_page_box(_: &[u8]) -> Result<[f64; 4], String> {
    Err("PDF images need the pdf-images feature".into())
}

fn png_header(data: &[u8]) -> Option<((u32, u32), (f64, f64))> {
    let be = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    let pixels = (be(16)?, be(20)?);
    let mut dpi = (72.0, 72.0);
    let mut at = 8;
    while at + 8 <= data.len() {
        let len = be(at)? as usize;
        match &data[at + 4..at + 8] {
            b"pHYs" if data.get(at + 16) == Some(&1) => {
                dpi = (be(at + 8)? as f64 * 0.0254, be(at + 12)? as f64 * 0.0254);
                break;
            }
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    Some((pixels, dpi))
}

fn jpeg_header(data: &[u8]) -> Option<((u32, u32), (f64, f64))> {
    let be = |at: usize| Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let mut dpi = (72.0, 72.0);
    let mut at = 2;
    while at + 4 <= data.len() {
        if data[at] != 0xff {
            return None;
        }
        let marker = data[at + 1];
        let len = be(at + 2)? as usize;
        match marker {
            0xe0 if data.get(at + 4..at + 9) == Some(b"JFIF\0") => {
                let (x, y) = (be(at + 12)? as f64, be(at + 14)? as f64);
                match data.get(at + 11) {
                    Some(1) if x > 0.0 && y > 0.0 => dpi = (x, y),
                    Some(2) if x > 0.0 && y > 0.0 => dpi = (x * 2.54, y * 2.54),
                    _ => {}
                }
            }
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some(((be(at + 7)? as u32, be(at + 5)? as u32), dpi));
            }
            _ => {}
        }
        at += 2 + len;
    }
    None
}

/// What fills the page behind its content.
#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundFill {
    Color(Color),
    Image(Arc<Image>),
}

/// A page background, for the current page and, if `all_pages`, every one
/// after it.
// SILE: `\background`.
#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    pub fill: BackgroundFill,
    pub all_pages: bool,
}

impl Typesetter {
    /// Set `image` in the text at `width` by `height`, either of which may
    /// be left out to keep its proportions.
    // SILE: `\img`.
    pub fn add_image(&mut self, image: Arc<Image>, width: Option<f64>, height: Option<f64>) -> &mut Self {
        let (w, h) = image.scaled_size(width, height);
        let hbox = HBox { ink: Some(Ink::Image(image)), ..HBox::new(Length::pt(w), Length::pt(h), Length::zero()) };
        self.add_figure(hbox)
    }
}
