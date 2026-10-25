//! What a font says of single glyphs, read once.
//!
//! Every layout asks for the font's own advance of each glyph ([`snap`]) and
//! for the ink of each line's last glyph ([`overhang`]), and reading either
//! parses the font's tables, the second an outline too. A list that makes a
//! delegate per row lays out "Row 1", "Row 2", ... with the same few glyphs,
//! so they are kept here, for every engine on the thread: font data is
//! shared between engines, and a font's blob id is never reused.
//!
//! [`snap`]: crate::snap
//! [`overhang`]: crate::engine

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use ljos_render::kurbo::Rect;
use ljos_render::{GlyphRun, PositionedGlyph};
use parley::FontData;
use rustc_hash::{FxHashMap, FxHasher};

/// Glyphs held, over every face. Past it everything is dropped and read
/// again: a text in many scripts and sizes is rare, and a few thousand
/// glyphs cover the ones an interface uses.
const MAX_GLYPHS: usize = 16 * 1024;

thread_local! {
    static FACES: RefCell<Faces> = RefCell::default();
}

#[derive(Default)]
struct Faces {
    /// By blob id, face index and the hash of the variation coordinates;
    /// the coordinates themselves are checked on a hit.
    map: FxHashMap<(u64, u32, u64), Face>,
    glyphs: usize,
    measured: u64,
}

struct Face {
    coords: Box<[i16]>,
    /// `None` for data that is not a font.
    units_per_em: Option<u16>,
    advances: FxHashMap<u32, f32>,
    /// Right edge of the ink by glyph, size, synthetic bold and slant; `None`
    /// for a glyph without ink.
    ink: FxHashMap<(u32, u32, bool, u32), Option<f64>>,
}

impl Faces {
    fn face(&mut self, font: &FontData, coords: &[i16]) -> &mut Face {
        if self.glyphs > MAX_GLYPHS {
            self.map.clear();
            self.glyphs = 0;
        }
        let mut h = FxHasher::default();
        coords.hash(&mut h);
        let key = (font.data.id(), font.index, h.finish());
        let face = self.map.entry(key).or_insert_with(|| Face::new(coords));
        if *face.coords != *coords {
            // Two instances with one hash: the newer one wins.
            *face = Face::new(coords);
        }
        face
    }
}

impl Face {
    fn new(coords: &[i16]) -> Self {
        Self {
            coords: coords.into(),
            units_per_em: None,
            advances: FxHashMap::default(),
            ink: FxHashMap::default(),
        }
    }
}

/// [`ljos_render::glyph_advances`], read from the font only for glyphs not
/// seen before.
pub(crate) fn advances(
    font: &FontData,
    normalized_coords: &[i16],
    glyphs: impl Iterator<Item = u32> + Clone,
) -> Option<(u16, Vec<f32>)> {
    FACES.with_borrow_mut(|faces| {
        let face = faces.face(font, normalized_coords);
        let mut read = 0;
        // Nearly always every glyph has been seen: no list of none missing.
        if glyphs.clone().any(|id| !face.advances.contains_key(&id)) || face.units_per_em.is_none()
        {
            let missing: Vec<u32> = glyphs
                .clone()
                .filter(|id| !face.advances.contains_key(id))
                .collect();
            let (units_per_em, units) =
                ljos_render::glyph_advances(font, normalized_coords, &missing)?;
            face.units_per_em = Some(units_per_em);
            read = missing.len();
            face.advances.extend(missing.into_iter().zip(units));
        }
        let units_per_em = face.units_per_em?;
        let out = glyphs.map(|id| face.advances[&id]).collect();
        faces.glyphs += read;
        faces.measured += read as u64;
        Some((units_per_em, out))
    })
}

/// The right edge of `glyph`'s ink with the run's synthetic bold and slant,
/// from its origin, as [`ljos_render::glyph_ink_bounds`] has it. `None` for
/// a glyph without ink.
pub(crate) fn ink_right(
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    embolden: bool,
    skew: f32,
    glyph: u32,
) -> Option<f64> {
    FACES.with_borrow_mut(|faces| {
        let face = faces.face(font, coords);
        let key = (glyph, font_size.to_bits(), embolden, skew.to_bits());
        if let Some(right) = face.ink.get(&key) {
            return *right;
        }
        let run = GlyphRun {
            font: font.clone(),
            font_size,
            glyphs: vec![PositionedGlyph {
                id: glyph,
                x: 0.0,
                y: 0.0,
            }],
            baseline: 0.0,
            bounds: Rect::ZERO,
            normalized_coords: coords.to_vec(),
            embolden,
            skew,
        };
        let right = ljos_render::glyph_ink_bounds(&run)
            .first()
            .copied()
            .flatten()
            .map(|ink| ink.x1);
        face.ink.insert(key, right);
        faces.glyphs += 1;
        faces.measured += 1;
        right
    })
}

/// The glyphs whose advance or ink this thread keeps, and about the bytes
/// of the tables they are kept in.
pub(crate) fn kept() -> (usize, usize) {
    FACES.with_borrow(|faces| {
        let bytes = faces
            .map
            .values()
            .map(|f| {
                std::mem::size_of::<Face>()
                    + f.coords.len() * 2
                    + f.advances.capacity() * 8
                    + f.ink.capacity() * (16 + std::mem::size_of::<Option<f64>>())
            })
            .sum();
        (faces.glyphs, bytes)
    })
}

/// Glyphs read from a font so far on this thread.
pub(crate) fn measured() -> u64 {
    FACES.with_borrow(|faces| faces.measured)
}
