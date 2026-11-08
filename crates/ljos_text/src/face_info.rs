//! What a face says of itself: what it is called, and whether it is
//! fixed-pitch (`QFontInfo`).

use std::cell::RefCell;
use std::sync::Arc;

use parley::FontData;
use rustc_hash::FxHashMap;
use skrifa::instance::{LocationRef, Size};
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// The face a style's text is set in, as Qt's `FontInfo` tells it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaceInfo {
    /// The face's family, as a `font.family` names it.
    pub family: Arc<str>,
    /// The face's style within its family: "Regular", "Bold Italic".
    pub style_name: Arc<str>,
    /// Whether every character the face maps that moves the pen moves it
    /// as far as the others.
    pub fixed_pitch: bool,
}

thread_local! {
    /// By blob id and face index: a face is read once.
    static FACES: RefCell<FxHashMap<(u64, u32), FaceInfo>> = RefCell::default();
}

/// What `font` says of itself; nothing for data that is not a font.
pub(crate) fn of(font: &FontData) -> FaceInfo {
    FACES.with_borrow_mut(|faces| {
        faces
            .entry((font.data.id(), font.index))
            .or_insert_with(|| read(font))
            .clone()
    })
}

fn read(font: &FontData) -> FaceInfo {
    let Ok(face) = FontRef::from_index(font.data.as_ref(), font.index) else {
        return FaceInfo::default();
    };
    // The typographic names first: they keep the family whole where the
    // older ones split it by weight ("Noto Sans" against "Noto Sans Light").
    let name = |ids: [StringId; 2]| -> Arc<str> {
        ids.into_iter()
            .find_map(|id| face.localized_strings(id).english_or_first())
            .map_or_else(|| Arc::from(""), |s| Arc::from(s.to_string()))
    };
    FaceInfo {
        family: name([StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]),
        style_name: name([
            StringId::TYPOGRAPHIC_SUBFAMILY_NAME,
            StringId::SUBFAMILY_NAME,
        ]),
        fixed_pitch: fixed_pitch(&face),
    }
}

/// Judged by the widths of the characters, as fontconfig judges a face's
/// spacing, which is what Qt reports on Linux: the flag in the `post`
/// table is unset in many monospace fonts (JetBrains Mono's Nerd Font among
/// them), and set in some whose characters are of two widths. A character
/// that does not move the pen, a combining mark, does not count.
fn fixed_pitch(face: &FontRef<'_>) -> bool {
    let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut width = None;
    for (_, glyph) in face.charmap().mappings() {
        if glyph == GlyphId::NOTDEF {
            continue;
        }
        let Some(advance) = metrics.advance_width(glyph).filter(|a| *a > 0.0) else {
            continue;
        };
        match width {
            None => width = Some(advance),
            Some(w) if w == advance => {}
            Some(_) => return false,
        }
    }
    width.is_some()
}
