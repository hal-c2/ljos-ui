//! Whether a family is fixed-pitch, and which is the system's fixed one,
//! asked from Rust: an application picks a terminal's face, or gives a
//! monospace interface face other leading, from the font's characters
//! rather than its name.

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
