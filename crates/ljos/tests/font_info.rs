//! Whether a family is fixed-pitch, and which is the system's fixed one,
//! asked from Rust: an application picks a terminal's face, or gives a
//! monospace interface face other leading, from the font's characters
//! rather than its name.

mod common;

use ljos::item::Font;
use ljos::prelude::*;

fn family(name: &str) -> Font {
    Font {
        family: Atom::from(name),
        ..Font::default()
    }
}

#[test]
fn the_system_s_fixed_family_is_fixed_pitch_and_a_proportional_one_is_not() {
    let _engine = ljos::engine();
    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    let fixed = ljos::fixed_font_family();
    assert!(!fixed.is_empty() && fixed != "monospace", "{fixed:?}");
    let info = ljos::font_info(&family(&fixed));
    assert_eq!(&*info.family, fixed);
    assert!(info.fixed_pitch, "{info:?}");
    // fonts-liberation, which CI installs.
    let sans = ljos::font_info(&family("Liberation Sans"));
    assert_eq!(&*sans.family, "Liberation Sans");
    assert!(!sans.fixed_pitch, "{sans:?}");
    assert!(ljos::font_info(&family("Liberation Mono")).fixed_pitch);
}

/// A FontInfo that has told its face goes on telling the face of the text
/// engine there is now: a host that swaps engines (the bundled fonts for
/// the system's, and back) is not left with the old engine's answer, and
/// knows it at once, before any frame.
#[test]
fn a_font_info_follows_the_text_engine_it_is_given() {
    let engine = ljos::engine();
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    let root = common::create_main(
        &engine,
        "import QtQuick\nItem {\n\
         FontInfo { id: mono; font.family: \"Liberation Mono\" }\n\
         property string family: mono.family\n\
         property bool fixed: mono.fixedPitch\n}\n",
    );
    let family = || {
        engine
            .to_js_string(&engine.read_by_name(root, "family"))
            .to_string()
    };
    let fixed = || engine.read_by_name(root, "fixed").to_boolean();
    let bundled = family();
    assert_ne!(
        bundled, "Liberation Mono",
        "only the bundled fonts at first"
    );

    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    // fonts-liberation, which CI installs.
    assert_eq!(family(), "Liberation Mono");
    assert!(fixed());

    // Another fresh engine: one that has changed no more than the last.
    ljos_quick::set_text_engine(ljos_quick::TextEngine::new());
    assert_eq!(family(), bundled);
}
