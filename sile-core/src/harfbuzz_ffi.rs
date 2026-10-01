use std::ffi::c_char;
use std::ptr;

use harfbuzz_sys as hb;

// ---------------------------------------------------------------------------
// HbBlob
// ---------------------------------------------------------------------------

pub(crate) struct HbBlob(*mut hb::hb_blob_t);

impl HbBlob {
    pub fn from_bytes(data: &[u8]) -> Self {
        unsafe {
            let blob = hb::hb_blob_create(
                data.as_ptr() as *const c_char,
                data.len() as u32,
                hb::HB_MEMORY_MODE_READONLY,
                ptr::null_mut(),
                None,
            );
            Self(blob)
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

    pub fn as_ptr(&self) -> *mut hb::hb_font_t {
        self.0
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

pub(crate) fn script_from_string(s: &str) -> hb::hb_script_t {
    unsafe { hb::hb_script_from_string(s.as_ptr() as *const c_char, s.len() as i32) }
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

pub(crate) fn shape(font: &HbFont, buffer: &mut HbBuffer, features: &[hb::hb_feature_t]) {
    unsafe {
        let features_ptr = if features.is_empty() {
            ptr::null()
        } else {
            features.as_ptr()
        };
        hb::hb_shape(font.as_ptr(), buffer.as_ptr(), features_ptr, features.len() as u32);
    }
}
