use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

type FaceKey = (String, u16, bool);

pub struct EmMetrics {
    pub x_height: f64,
    pub space: f64,
}

/// The pinned test fonts, looked up the way SILE's tests name them.
pub struct Fonts {
    db: fontdb::Database,
    cache: std::sync::Mutex<HashMap<FaceKey, Option<Arc<Vec<u8>>>>>,
}

impl Fonts {
    pub fn load(dir: &Path) -> Self {
        let mut db = fontdb::Database::new();
        db.load_fonts_dir(dir);
        Self {
            db,
            cache: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }

    pub fn has_family(&self, family: &str) -> bool {
        self.db.faces().any(|f| {
            f.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(family))
        })
    }

    pub fn data(&self, family: &str, weight: u16, italic: bool) -> Option<Arc<Vec<u8>>> {
        let key = (family.to_lowercase(), weight, italic);
        if let Some(hit) = self.cache.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight),
            style: if italic {
                fontdb::Style::Italic
            } else {
                fontdb::Style::Normal
            },
            stretch: fontdb::Stretch::Normal,
        };
        let data = self
            .db
            .query(&query)
            .filter(|id| {
                self.db.face(*id).is_some_and(|f| {
                    f.families
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case(family))
                })
            })
            .and_then(|id| {
                self.db
                    .with_face_data(id, |data, _| Arc::new(data.to_vec()))
            });
        self.cache.lock().unwrap().insert(key, data.clone());
        data
    }

    /// x-height and space advance as fractions of the em, for SILE's `ex`
    /// and `spc` units.
    pub fn em_metrics(&self, family: &str, weight: u16, italic: bool) -> Option<EmMetrics> {
        let data = self.data(family, weight, italic)?;
        let face = ttf_parser::Face::parse(&data, 0).ok()?;
        let upem = face.units_per_em() as f64;
        let x_height = match face.x_height() {
            Some(h) if h > 0 => h as f64 / upem,
            _ => 0.5,
        };
        let space = face
            .glyph_index(' ')
            .and_then(|g| face.glyph_hor_advance(g))
            .map_or(0.25, |a| a as f64 / upem);
        Some(EmMetrics { x_height, space })
    }

    /// Fill in per-glyph advances for runs SILE printed only a width for.
    pub fn fill_advances(&self, trace: &mut crate::trace::Trace) {
        for page in &mut trace.pages {
            for run in &mut page.runs {
                if run.advances.is_some() {
                    continue;
                }
                let Some(data) = self.data(&run.family, run.weight, run.italic) else {
                    continue;
                };
                let Ok(face) = ttf_parser::Face::parse(&data, 0) else {
                    continue;
                };
                let scale = run.size / face.units_per_em() as f64;
                let advances = run
                    .gids
                    .iter()
                    .map(|g| {
                        face.glyph_hor_advance(ttf_parser::GlyphId(*g as u16))
                            .unwrap_or(0) as f64
                            * scale
                    })
                    .collect();
                run.advances = Some(advances);
            }
        }
    }
}
