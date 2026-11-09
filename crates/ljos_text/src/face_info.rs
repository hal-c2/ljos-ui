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
    one_width(
        face.charmap()
            .mappings()
            .filter(|(_, glyph)| *glyph != GlyphId::NOTDEF)
            .filter_map(|(_, glyph)| metrics.advance_width(glyph))
            .filter(|advance| *advance > 0.0),
    )
}

/// Whether advances, in the order of their characters, are all of one
/// width. Hand-tuned and patched monospace fonts have a character or two a
/// unit off, so as in fontconfig, which is what Qt reports, an advance is
/// that width while it is within a 33rd of the larger of the two. The
/// width is the first character's, not the narrowest: a 1030 then 1000
/// then 1060 is one width, 1000 then 1060 then 1030 is not (`fc-query` and
/// qml6 agree on both).
fn one_width(mut advances: impl Iterator<Item = f32>) -> bool {
    let Some(width) = advances.next() else {
        return false;
    };
    advances.all(|advance| (advance - width).abs() <= advance.max(width) / 33.0)
}

#[cfg(test)]
mod tests {
    use super::one_width;

    fn one(advances: &[f32]) -> bool {
        one_width(advances.iter().copied())
    }

    // Each answer is fontconfig's spacing and Qt's fixedPitch for a face
    // made with these advances, in units of 1000 an em.
    #[test]
    fn an_advance_within_a_33rd_of_the_larger_is_the_same_width() {
        assert!(one(&[600.0, 601.0]));
        assert!(one(&[600.0, 618.0]));
        assert!(!one(&[600.0, 619.0]));
        assert!(one(&[1000.0, 1031.0]));
        assert!(!one(&[1000.0, 1032.0]));
        assert!(one(&[1000.0, 970.0]));
        assert!(!one(&[1000.0, 969.0]));
    }

    #[test]
    fn the_width_is_the_first_characters() {
        assert!(one(&[1030.0, 1000.0, 1060.0]));
        assert!(!one(&[1000.0, 1060.0, 1030.0]));
        assert!(!one(&[1000.0, 1030.0, 1060.0]));
    }

    #[test]
    fn two_widths_one_twice_the_other_are_not_fixed_pitch() {
        assert!(!one(&[500.0, 1000.0]));
        assert!(!one(&[600.0, 700.0]));
    }

    #[test]
    fn a_face_with_nothing_that_moves_the_pen_is_not_fixed_pitch() {
        assert!(!one(&[]));
    }
}
