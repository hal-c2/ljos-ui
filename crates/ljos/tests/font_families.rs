//! The font families an application can choose a UI font from, asked
//! from Rust.

use ljos::prelude::*;

#[test]
fn the_families_are_what_qt_font_families_answers() {
    let engine = ljos::engine();
    let families = ljos::font_families();
    assert!(!families.is_empty());
    assert!(families.is_sorted());
    let from_qml = engine
        .eval_js("Qt.fontFamilies().join('\\n')")
        .expect("Qt.fontFamilies()");
    let Value::String(from_qml) = from_qml else {
        panic!("not a string: {from_qml:?}");
    };
    assert_eq!(from_qml.split('\n').collect::<Vec<_>>(), families);
}

#[test]
fn the_system_fonts_choice_is_applied() {
    let bundled = ljos::font_families();
    // What the desktop shell installs when `ShellConfig::system_fonts` is
    // on: the bundled families stay, the system's are added.
    ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    let with_system = ljos::font_families();
    assert!(bundled.iter().all(|f| with_system.contains(f)));
}
