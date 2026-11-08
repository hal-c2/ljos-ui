//! On macOS, text in the system UI font is as wide as CoreText sets it:
//! San Francisco's tracking (`trak`) is applied once, at the text's size.
//! CoreText's advances carry the tracking already, so a second pass on
//! top of the shaper's (#253) would show here as a run that is narrower
//! or wider than CoreText's by the track at every glyph.

#![cfg(target_os = "macos")]

use std::ptr::{NonNull, null_mut};

use ljos_text::{LayoutParams, TextEngine, TextStyle};
use objc2_core_text::{CTFont, CTFontOrientation, CTFontUIFontType};

/// CoreText's advance of `c` in the system UI font at `size` points (a
/// point is a pixel on macOS).
fn core_text_advance(c: char, size: f64) -> f64 {
    // SAFETY: a UI font type CoreText defines, and no language.
    let font = unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, size, None) }
        .expect("macOS has a system UI font");
    let mut unit = [0u16; 1];
    c.encode_utf16(&mut unit);
    let mut glyph = [0u16; 1];
    // SAFETY: one character in, one glyph out; a null advances array asks
    // for the total alone.
    unsafe {
        assert!(font.glyphs_for_characters(
            NonNull::from(&mut unit[0]),
            NonNull::from(&mut glyph[0]),
            1,
        ));
        font.advances_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::from(&mut glyph[0]),
            null_mut(),
            1,
        )
    }
}

/// Twenty `n`s, which no kerning pair joins: the run is twenty advances.
#[test]
fn the_system_font_is_tracked_once_as_core_text_tracks_it() {
    let mut e = TextEngine::with_system_fonts();
    // San Francisco's normal track at these sizes, in units of 2048: +38
    // at 9, -12 at 13, -46 at 20. Twice would be off by 20 of them.
    for size in [9.0f32, 13.0, 20.0] {
        let style = TextStyle {
            family: ".AppleSystemUIFont".into(),
            pixel_size: size,
            ..TextStyle::default()
        };
        let ljos = e
            .layout(&"n".repeat(20), &style, &LayoutParams::default())
            .advance;
        let core_text = 20.0 * core_text_advance('n', f64::from(size));
        // Each advance is set in 64ths of a pixel.
        assert!(
            (ljos - core_text).abs() <= 20.0 / 64.0,
            "{size} px: Ljós {ljos}, CoreText {core_text}"
        );
    }
}
