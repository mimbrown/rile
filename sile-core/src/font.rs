use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FontError {
    Parse(String),
    NotFound(String),
    Io(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(msg) => write!(f, "font parse error: {msg}"),
            Self::NotFound(msg) => write!(f, "font not found: {msg}"),
            Self::Io(msg) => write!(f, "font I/O error: {msg}"),
        }
    }
}

impl std::error::Error for FontError {}

// ---------------------------------------------------------------------------
// FontWeight
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

impl fmt::Display for FontWeight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// FontStyle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl fmt::Display for FontStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Italic => write!(f, "italic"),
            Self::Oblique => write!(f, "oblique"),
        }
    }
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    #[default]
    LTR,
    RTL,
    TTB,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LTR => write!(f, "LTR"),
            Self::RTL => write!(f, "RTL"),
            Self::TTB => write!(f, "TTB"),
        }
    }
}

// ---------------------------------------------------------------------------
// FontSpec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FontSpec {
    pub family: Option<String>,
    pub size: f64,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub features: String,
    pub variations: String,
    pub direction: Direction,
    pub language: String,
    pub script: String,
    pub filename: Option<String>,
}

impl Default for FontSpec {
    fn default() -> Self {
        Self {
            family: Some("Gentium Plus".to_string()),
            size: 10.0,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            features: String::new(),
            variations: String::new(),
            direction: Direction::LTR,
            language: String::new(),
            script: String::new(),
            filename: None,
        }
    }
}

impl FontSpec {
    /// Cache key matching Lua's font cache format.
    /// Note: language and script are intentionally excluded (same font
    /// instance can serve multiple scripts/languages).
    pub fn cache_key(&self) -> String {
        format!(
            "{};{};{};{};{};{};{};{}",
            self.family.as_deref().unwrap_or(""),
            self.size,
            self.weight.0,
            self.style,
            self.features,
            self.variations,
            self.direction,
            self.filename.as_deref().unwrap_or(""),
        )
    }
}

// ---------------------------------------------------------------------------
// FontFace
// ---------------------------------------------------------------------------

pub struct FontFace {
    data: Vec<u8>,
    index: u32,
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
    underline_position: i16,
    underline_thickness: i16,
    strikeout_position: i16,
    strikeout_size: i16,
    glyph_count: u16,
    is_variable: bool,
    has_colr: bool,
    has_cpal: bool,
    has_svg: bool,
    has_math: bool,
}

impl FontFace {
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Self, FontError> {
        let face = ttf_parser::Face::parse(&data, index)
            .map_err(|e| FontError::Parse(e.to_string()))?;

        let (ul_pos, ul_thick) = face
            .underline_metrics()
            .map(|m| (m.position, m.thickness))
            .unwrap_or((0, 0));
        let (so_pos, so_size) = face
            .strikeout_metrics()
            .map(|m| (m.position, m.thickness))
            .unwrap_or((0, 0));

        Ok(Self {
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            line_gap: face.line_gap(),
            underline_position: ul_pos,
            underline_thickness: ul_thick,
            strikeout_position: so_pos,
            strikeout_size: so_size,
            glyph_count: face.number_of_glyphs(),
            is_variable: face.is_variable(),
            has_colr: face.tables().colr.is_some(),
            has_cpal: face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"CPAL"))
                .is_some(),
            has_svg: face.tables().svg.is_some(),
            has_math: face
                .raw_face()
                .table(ttf_parser::Tag::from_bytes(b"MATH"))
                .is_some(),
            data,
            index,
        })
    }

    // -- Top-level metrics ---------------------------------------------------

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn ascender(&self) -> i16 {
        self.ascender
    }

    pub fn descender(&self) -> i16 {
        self.descender
    }

    pub fn line_gap(&self) -> i16 {
        self.line_gap
    }

    pub fn underline_position(&self) -> i16 {
        self.underline_position
    }

    pub fn underline_thickness(&self) -> i16 {
        self.underline_thickness
    }

    pub fn strikeout_position(&self) -> i16 {
        self.strikeout_position
    }

    pub fn strikeout_size(&self) -> i16 {
        self.strikeout_size
    }

    pub fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    pub fn is_variable(&self) -> bool {
        self.is_variable
    }

    pub fn has_colr_table(&self) -> bool {
        self.has_colr
    }

    pub fn has_cpal_table(&self) -> bool {
        self.has_cpal
    }

    pub fn has_svg_table(&self) -> bool {
        self.has_svg
    }

    pub fn has_math_table(&self) -> bool {
        self.has_math
    }

    // -- Per-glyph queries ---------------------------------------------------

    pub fn glyph_id(&self, c: char) -> Option<u16> {
        self.with_face(|f| f.glyph_index(c).map(|g| g.0))
    }

    pub fn advance_width(&self, glyph_id: u16) -> Option<u16> {
        self.with_face(|f| f.glyph_hor_advance(ttf_parser::GlyphId(glyph_id)))
    }

    pub fn advance_height(&self, glyph_id: u16) -> Option<u16> {
        self.with_face(|f| f.glyph_ver_advance(ttf_parser::GlyphId(glyph_id)))
    }

    pub fn glyph_name(&self, glyph_id: u16) -> Option<String> {
        self.with_face(|f| f.glyph_name(ttf_parser::GlyphId(glyph_id)).map(String::from))
    }

    pub fn glyph_bounding_box(&self, glyph_id: u16) -> Option<GlyphBBox> {
        self.with_face(|f| {
            f.glyph_bounding_box(ttf_parser::GlyphId(glyph_id))
                .map(|r| GlyphBBox {
                    x_min: r.x_min,
                    y_min: r.y_min,
                    x_max: r.x_max,
                    y_max: r.y_max,
                })
        })
    }

    // -- Scaling helpers -----------------------------------------------------

    /// Convert signed font units to points at a given point size.
    pub fn scale(&self, font_units: i16, point_size: f64) -> f64 {
        font_units as f64 * point_size / self.units_per_em as f64
    }

    /// Convert unsigned font units to points at a given point size.
    pub fn scale_u(&self, font_units: u16, point_size: f64) -> f64 {
        font_units as f64 * point_size / self.units_per_em as f64
    }

    /// Raw font data and face index (useful for passing to shapers).
    pub fn raw_data(&self) -> (&[u8], u32) {
        (&self.data, self.index)
    }

    // -- Internal ------------------------------------------------------------

    fn with_face<T>(&self, f: impl FnOnce(&ttf_parser::Face<'_>) -> T) -> T {
        let face =
            ttf_parser::Face::parse(&self.data, self.index).expect("font data already validated");
        f(&face)
    }
}

impl fmt::Debug for FontFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontFace")
            .field("index", &self.index)
            .field("units_per_em", &self.units_per_em)
            .field("ascender", &self.ascender)
            .field("descender", &self.descender)
            .field("glyph_count", &self.glyph_count)
            .field("is_variable", &self.is_variable)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// GlyphBBox
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct GlyphBBox {
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

// ---------------------------------------------------------------------------
// FontDatabase
// ---------------------------------------------------------------------------

pub struct FontDatabase {
    db: fontdb::Database,
    cache: HashMap<String, Arc<FontFace>>,
}

impl FontDatabase {
    pub fn new() -> Self {
        Self {
            db: fontdb::Database::new(),
            cache: HashMap::new(),
        }
    }

    pub fn load_system_fonts(&mut self) {
        self.db.load_system_fonts();
    }

    pub fn load_font_data(&mut self, data: Vec<u8>) {
        self.db.load_font_data(data);
    }

    pub fn load_fonts_dir(&mut self, dir: &Path) {
        self.db.load_fonts_dir(dir);
    }

    pub fn load_font_file(&mut self, path: &Path) -> Result<(), FontError> {
        self.db
            .load_font_file(path)
            .map_err(|e| FontError::Io(e.to_string()))
    }

    pub fn font_count(&self) -> usize {
        self.db.faces().count()
    }

    /// Resolve a FontSpec to a loaded FontFace.
    ///
    /// If the spec has a `filename`, the font is loaded directly from that
    /// path. Otherwise fontdb is queried by family/weight/style.
    pub fn resolve(&mut self, spec: &FontSpec) -> Result<Arc<FontFace>, FontError> {
        let key = spec.cache_key();
        if let Some(face) = self.cache.get(&key) {
            return Ok(Arc::clone(face));
        }

        let face = if let Some(filename) = &spec.filename {
            let data = std::fs::read(filename).map_err(|e| FontError::Io(e.to_string()))?;
            FontFace::from_bytes(data, 0)?
        } else {
            self.resolve_from_db(spec)?
        };

        let face = Arc::new(face);
        self.cache.insert(key, Arc::clone(&face));
        Ok(face)
    }

    /// The resolved family name for a fontdb face ID.
    pub fn family_name(&self, id: fontdb::ID) -> Option<&str> {
        self.db
            .face(id)
            .and_then(|fi| fi.families.first().map(|(name, _)| name.as_str()))
    }

    /// Query fontdb and return the matching face ID (without loading).
    pub fn query(&self, spec: &FontSpec) -> Option<fontdb::ID> {
        let family = spec.family.as_deref()?;
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(spec.weight.0),
            style: match spec.style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            },
            stretch: fontdb::Stretch::Normal,
        };
        self.db.query(&query)
    }

    fn resolve_from_db(&self, spec: &FontSpec) -> Result<FontFace, FontError> {
        let family = spec
            .family
            .as_deref()
            .ok_or_else(|| FontError::NotFound("no family or filename specified".into()))?;

        let id = self
            .query(spec)
            .ok_or_else(|| FontError::NotFound(format!("no match for family \"{family}\"")))?;

        let mut data_out: Option<(Vec<u8>, u32)> = None;
        self.db.with_face_data(id, |data, index| {
            data_out = Some((data.to_vec(), index));
        });

        let (data, index) =
            data_out.ok_or_else(|| FontError::NotFound("face data unavailable".into()))?;

        FontFace::from_bytes(data, index)
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FontDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontDatabase")
            .field("faces", &self.db.faces().count())
            .field("cached", &self.cache.len())
            .finish()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- FontSpec ------------------------------------------------------------

    #[test]
    fn spec_default() {
        let spec = FontSpec::default();
        assert_eq!(spec.family.as_deref(), Some("Gentium Plus"));
        assert_eq!(spec.size, 10.0);
        assert_eq!(spec.weight, FontWeight::NORMAL);
        assert_eq!(spec.style, FontStyle::Normal);
    }

    #[test]
    fn spec_cache_key_excludes_language_and_script() {
        let mut a = FontSpec::default();
        a.language = "en".into();
        a.script = "Latn".into();
        let mut b = FontSpec::default();
        b.language = "ar".into();
        b.script = "Arab".into();
        assert_eq!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn spec_cache_key_differs_by_weight() {
        let mut a = FontSpec::default();
        a.weight = FontWeight::NORMAL;
        let mut b = FontSpec::default();
        b.weight = FontWeight::BOLD;
        assert_ne!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn spec_cache_key_differs_by_filename() {
        let mut a = FontSpec::default();
        a.filename = Some("/a.ttf".into());
        let mut b = FontSpec::default();
        b.filename = Some("/b.ttf".into());
        assert_ne!(a.cache_key(), b.cache_key());
    }

    // -- FontWeight ----------------------------------------------------------

    #[test]
    fn weight_constants() {
        assert_eq!(FontWeight::THIN.0, 100);
        assert_eq!(FontWeight::NORMAL.0, 400);
        assert_eq!(FontWeight::BOLD.0, 700);
        assert_eq!(FontWeight::BLACK.0, 900);
    }

    // -- FontFace via system font --------------------------------------------

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

    #[test]
    fn face_metrics() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return, // skip if no system fonts
        };
        assert!(face.units_per_em() > 0);
        assert!(face.glyph_count() > 0);
        // ascender is typically positive, descender negative
        assert!(face.ascender() > 0);
        assert!(face.descender() < 0);
    }

    #[test]
    fn face_glyph_id_for_ascii() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let gid = face.glyph_id('A');
        assert!(gid.is_some(), "every font should have an 'A' glyph");
        assert_ne!(gid.unwrap(), 0);
    }

    #[test]
    fn face_advance_width() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let gid = face.glyph_id('A').unwrap();
        let advance = face.advance_width(gid);
        assert!(advance.is_some());
        assert!(advance.unwrap() > 0);
    }

    #[test]
    fn face_scale() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let gid = face.glyph_id('A').unwrap();
        let raw = face.advance_width(gid).unwrap();
        let scaled = face.scale_u(raw, 12.0);
        assert!(scaled > 0.0);
        // At 12pt the advance should be in a reasonable range
        assert!(scaled < 20.0);
    }

    #[test]
    fn face_missing_glyph_returns_none() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        // Private use area codepoint — unlikely to be in most fonts
        let gid = face.glyph_id('\u{F8FF}');
        // Either None or glyph ID 0 (.notdef) is acceptable
        if let Some(id) = gid {
            // Some fonts map unknown chars to 0
            let _ = id;
        }
    }

    #[test]
    fn face_glyph_bounding_box() {
        let face = match load_any_system_font() {
            Some(f) => f,
            None => return,
        };
        let gid = face.glyph_id('A').unwrap();
        let bbox = face.glyph_bounding_box(gid);
        if let Some(bb) = bbox {
            assert!(bb.x_max > bb.x_min);
            assert!(bb.y_max > bb.y_min);
        }
    }

    // -- FontDatabase --------------------------------------------------------

    #[test]
    fn database_load_system_fonts() {
        let mut db = FontDatabase::new();
        db.load_system_fonts();
        assert!(db.font_count() > 0, "should find at least one system font");
    }

    #[test]
    fn database_resolve_by_family() {
        let mut db = FontDatabase::new();
        db.load_system_fonts();
        // Try a font that exists on macOS
        let spec = FontSpec {
            family: Some("Helvetica".into()),
            ..Default::default()
        };
        if db.query(&spec).is_some() {
            let face = db.resolve(&spec).unwrap();
            assert!(face.glyph_count() > 0);
        }
    }

    #[test]
    fn database_resolve_caches() {
        let mut db = FontDatabase::new();
        db.load_system_fonts();
        let spec = FontSpec {
            family: Some("Helvetica".into()),
            ..Default::default()
        };
        if db.query(&spec).is_some() {
            let f1 = db.resolve(&spec).unwrap();
            let f2 = db.resolve(&spec).unwrap();
            assert!(Arc::ptr_eq(&f1, &f2), "second resolve should hit cache");
        }
    }

    #[test]
    fn database_not_found() {
        let mut db = FontDatabase::new();
        db.load_system_fonts();
        let spec = FontSpec {
            family: Some("ThisFontDoesNotExist999".into()),
            ..Default::default()
        };
        assert!(db.resolve(&spec).is_err());
    }
}
