//! A character no font of the style has is set in a font that has it.

use ljos_text::{LayoutParams, TextEngine, TextStyle};

/// A font of two private-use characters no other font has: U+F8F0 at 600
/// units of 1000 and U+10FFF0 at 700 (features/text/fonts).
fn symbols() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../features/text/fonts/LjosSymbolsTest-Regular.ttf"
    ))
    .unwrap()
}

fn style() -> TextStyle {
    TextStyle {
        pixel_size: 20.0,
        ..TextStyle::default()
    }
}

fn advance(e: &mut TextEngine, text: &str) -> f64 {
    e.layout(text, &style(), &LayoutParams::default()).advance
}

// The widths are Qt's for the same font named outright: 600 and 700 units
// at 20 pixels, in 64ths.
#[test]
fn a_private_use_character_is_set_in_the_font_that_has_it() {
    let mut e = TextEngine::new();
    e.register_font(symbols()).unwrap();
    assert_eq!(advance(&mut e, "\u{F8F0}"), 11.984375);
    assert_eq!(advance(&mut e, "\u{10FFF0}"), 13.984375);
    let word = advance(&mut e, "main");
    assert_eq!(advance(&mut e, "main\u{F8F0}") - word, 11.984375);
}

#[test]
fn a_box_laid_out_before_the_font_was_added_is_set_in_it_after() {
    let mut e = TextEngine::new();
    let boxed = advance(&mut e, "\u{F8F0}");
    assert_ne!(boxed, 11.984375);
    e.register_font(symbols()).unwrap();
    assert_eq!(advance(&mut e, "\u{F8F0}"), 11.984375);
}

#[test]
fn the_fonts_are_indexed_once_and_only_for_a_box() {
    let mut e = TextEngine::new();
    e.register_font(symbols()).unwrap();
    advance(&mut e, "Hello");
    advance(&mut e, "Grüße, αβγ");
    assert_eq!(e.stats().coverage_indexes, 0, "no character was missing");
    advance(&mut e, "\u{F8F0}");
    advance(&mut e, "a\u{F8F0}");
    advance(&mut e, "\u{10FFF0} b");
    // A character no font has is looked up once, not on every layout.
    advance(&mut e, "\u{F8F1}");
    e.clear_caches();
    advance(&mut e, "\u{F8F1}");
    assert_eq!(e.stats().coverage_indexes, 1);
}

// Beside the platform's fonts, which some platforms are asked about rather
// than indexed, the fonts added are still looked through.
#[test]
fn an_added_font_is_found_beside_the_system_fonts() {
    let mut e = TextEngine::with_system_fonts();
    e.register_font(symbols()).unwrap();
    assert_eq!(advance(&mut e, "\u{F8F0}"), 11.984375);
    assert_eq!(advance(&mut e, "\u{10FFF0}"), 13.984375);
}
