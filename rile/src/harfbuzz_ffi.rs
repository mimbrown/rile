use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::Arc;

use harfbuzz_sys as hb;

// ---------------------------------------------------------------------------
// HbBlob
// ---------------------------------------------------------------------------

pub(crate) struct HbBlob(*mut hb::hb_blob_t);

impl HbBlob {
    /// A blob keeping `data` alive for as long as HarfBuzz uses it.
    pub fn from_arc(data: Arc<Vec<u8>>) -> Self {
        unsafe extern "C" fn release(user_data: *mut c_void) {
            unsafe { drop(Arc::from_raw(user_data as *const Vec<u8>)) }
        }
        let (ptr, len) = (data.as_ptr(), data.len());
        unsafe {
            Self(hb::hb_blob_create(ptr as *const c_char, len as u32, hb::HB_MEMORY_MODE_READONLY, Arc::into_raw(data) as *mut c_void, Some(release)))
        }
    }
}

impl Drop for HbBlob {
    fn drop(&mut self) {
        unsafe { hb::hb_blob_destroy(self.0) }
    }
}

// ---------------------------------------------------------------------------
// HbFace
// ---------------------------------------------------------------------------

pub(crate) struct HbFace(*mut hb::hb_face_t);

impl HbFace {
    pub fn new(blob: &HbBlob, index: u32) -> Self {
        unsafe { Self(hb::hb_face_create(blob.0, index)) }
    }
}

impl Drop for HbFace {
    fn drop(&mut self) {
        unsafe { hb::hb_face_destroy(self.0) }
    }
}

// ---------------------------------------------------------------------------
// HbFont
// ---------------------------------------------------------------------------

pub(crate) struct HbFont(*mut hb::hb_font_t);

impl HbFont {
    pub fn new(face: &HbFace) -> Self {
        unsafe { Self(hb::hb_font_create(face.0)) }
    }

    pub fn set_variations(&mut self, variations: &[([u8; 4], f32)]) {
        if variations.is_empty() {
            return;
        }
        let variations: Vec<hb::hb_variation_t> = variations
            .iter()
            .map(|(tag, value)| hb::hb_variation_t { tag: u32::from_be_bytes(*tag), value: *value })
            .collect();
        unsafe { hb::hb_font_set_variations(self.0, variations.as_ptr(), variations.len() as u32) }
    }

    /// The glyph's own advance, in font units.
    pub fn glyph_h_advance(&self, gid: u32) -> i32 {
        unsafe { hb::hb_font_get_glyph_h_advance(self.0, gid) }
    }

    pub fn as_ptr(&self) -> *mut hb::hb_font_t {
        self.0
    }
}

/// A font ready to shape with, at fixed variations.
pub(crate) struct HbShapingFont {
    font: HbFont,
}

// SAFETY: the face and font are made immutable before the font is handed
// out, and HarfBuzz allows immutable objects to be used from any thread.
unsafe impl Send for HbShapingFont {}
unsafe impl Sync for HbShapingFont {}

impl HbShapingFont {
    pub fn new(data: Arc<Vec<u8>>, index: u32, variations: &[([u8; 4], f32)]) -> Self {
        let blob = HbBlob::from_arc(data);
        let face = HbFace::new(&blob, index);
        unsafe { hb::hb_face_make_immutable(face.0) };
        let mut font = HbFont::new(&face);
        font.set_variations(variations);
        unsafe { hb::hb_font_make_immutable(font.0) };
        Self { font }
    }

    pub fn font(&self) -> &HbFont {
        &self.font
    }
}

impl Drop for HbFont {
    fn drop(&mut self) {
        unsafe { hb::hb_font_destroy(self.0) }
    }
}

// ---------------------------------------------------------------------------
// HbBuffer
// ---------------------------------------------------------------------------

pub(crate) struct HbBuffer(*mut hb::hb_buffer_t);

impl HbBuffer {
    pub fn new() -> Self {
        unsafe { Self(hb::hb_buffer_create()) }
    }

    pub fn add_str(&mut self, text: &str) {
        unsafe {
            hb::hb_buffer_add_utf8(
                self.0,
                text.as_ptr() as *const c_char,
                text.len() as i32,
                0,
                text.len() as i32,
            );
            hb::hb_buffer_set_content_type(self.0, hb::HB_BUFFER_CONTENT_TYPE_UNICODE);
        }
    }

    pub fn set_direction(&mut self, dir: hb::hb_direction_t) {
        unsafe { hb::hb_buffer_set_direction(self.0, dir) }
    }

    pub fn set_script(&mut self, script: hb::hb_script_t) {
        unsafe { hb::hb_buffer_set_script(self.0, script) }
    }

    pub fn set_language(&mut self, lang: &str) {
        unsafe {
            let hb_lang = hb::hb_language_from_string(
                lang.as_ptr() as *const c_char,
                lang.len() as i32,
            );
            hb::hb_buffer_set_language(self.0, hb_lang);
        }
    }

    /// Fill in script, direction and language left unset from the text.
    pub fn guess_segment_properties(&mut self) {
        unsafe { hb::hb_buffer_guess_segment_properties(self.0) }
    }

    pub fn as_ptr(&mut self) -> *mut hb::hb_buffer_t {
        self.0
    }

    pub fn glyph_infos(&self) -> &[hb::hb_glyph_info_t] {
        unsafe {
            let mut len = 0u32;
            let ptr = hb::hb_buffer_get_glyph_infos(self.0, &mut len);
            if ptr.is_null() || len == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(ptr, len as usize)
            }
        }
    }

    pub fn glyph_positions(&self) -> &[hb::hb_glyph_position_t] {
        unsafe {
            let mut len = 0u32;
            let ptr = hb::hb_buffer_get_glyph_positions(self.0, &mut len);
            if ptr.is_null() || len == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(ptr, len as usize)
            }
        }
    }
}

impl Drop for HbBuffer {
    fn drop(&mut self) {
        unsafe { hb::hb_buffer_destroy(self.0) }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The script a font option names, in whatever case it is written.
#[cfg(not(feature = "sile-quirks"))]
pub(crate) fn script_from_string(s: &str) -> hb::hb_script_t {
    unsafe { hb::hb_script_from_string(s.as_ptr() as *const c_char, s.len() as i32) }
}

/// The script a font option names, as SILE hands it to HarfBuzz: the tag as
/// written. Only a tag in ISO 15924's own case (`Hebr`) is the script
/// HarfBuzz knows; another spelling (`hebr`) still selects the font's
/// features for that tag, but as a script of no known direction, so
/// right-to-left text is shaped left to right and turned round.
#[cfg(feature = "sile-quirks")]
pub(crate) fn script_from_string(s: &str) -> hb::hb_script_t {
    unsafe { hb::hb_tag_from_string(s.as_ptr() as *const c_char, s.len() as i32) as hb::hb_script_t }
}

#[cfg(test)]
mod tests {
    use super::script_from_string;

    #[test]
    fn a_script_is_known_in_its_own_case() {
        assert_eq!(script_from_string("Hebr"), harfbuzz_sys::HB_SCRIPT_HEBREW);
    }

    #[test]
    #[cfg(not(feature = "sile-quirks"))]
    fn a_script_is_known_in_any_case() {
        assert_eq!(script_from_string("hebr"), harfbuzz_sys::HB_SCRIPT_HEBREW);
    }

    #[test]
    #[cfg(feature = "sile-quirks")]
    fn sile_takes_a_script_in_another_case_for_another_script() {
        assert_ne!(script_from_string("hebr"), harfbuzz_sys::HB_SCRIPT_HEBREW);
    }
}

pub(crate) fn parse_feature(s: &str) -> Option<hb::hb_feature_t> {
    unsafe {
        let mut feature = std::mem::zeroed::<hb::hb_feature_t>();
        let ok = hb::hb_feature_from_string(
            s.as_ptr() as *const c_char,
            s.len() as i32,
            &mut feature,
        );
        if ok != 0 { Some(feature) } else { None }
    }
}

pub(crate) fn shape(font: &HbFont, buffer: &mut HbBuffer, features: &[hb::hb_feature_t], shapers: &[std::ffi::CString]) {
    unsafe {
        let features_ptr = if features.is_empty() {
            ptr::null()
        } else {
            features.as_ptr()
        };
        if shapers.is_empty() {
            hb::hb_shape(font.as_ptr(), buffer.as_ptr(), features_ptr, features.len() as u32);
        } else {
            let mut list: Vec<*const c_char> = shapers.iter().map(|s| s.as_ptr()).collect();
            list.push(ptr::null());
            hb::hb_shape_full(font.as_ptr(), buffer.as_ptr(), features_ptr, features.len() as u32, list.as_ptr());
        }
    }
}
