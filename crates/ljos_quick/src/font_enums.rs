//! `Font` (QtQuick): the names of what a font's members hold, `Font.Bold`,
//! `Font.AllUppercase` (doc.qt.io/qt-6/qml-font.html).
//!
//! The `font` a `Text` has is a value, not an object of this type: nothing
//! is ever made of it. The numbers are what Qt 6.11.2 answers for the keys.

use ljos_engine::{Engine, QmlObject};

use crate::enums::EnumTable;

/// `Font` (QtQuick): only its enums are used.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Font", uncreatable, enums = font_enums)]
pub struct FontEnums {}

fn font_enums() -> EnumTable {
    vec![
        (
            "FontWeight",
            vec![
                ("Thin", 100),
                ("ExtraLight", 200),
                ("Light", 300),
                ("Normal", 400),
                ("Medium", 500),
                ("DemiBold", 600),
                ("Bold", 700),
                ("ExtraBold", 800),
                ("Black", 900),
            ],
        ),
        (
            "Capitalization",
            vec![
                ("MixedCase", 0),
                ("AllUppercase", 1),
                ("AllLowercase", 2),
                ("SmallCaps", 3),
                ("Capitalize", 4),
            ],
        ),
        (
            "HintingPreference",
            vec![
                ("PreferDefaultHinting", 0),
                ("PreferNoHinting", 1),
                ("PreferVerticalHinting", 2),
                ("PreferFullHinting", 3),
            ],
        ),
        (
            "Style",
            vec![("StyleNormal", 0), ("StyleItalic", 1), ("StyleOblique", 2)],
        ),
    ]
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [FontEnums]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weights_are_the_numbers_a_font_holds() {
        let engine = Engine::new();
        crate::register(&engine);
        let font = engine.lookup_type("QtQuick", "Font").expect("registered");
        assert_eq!(font.enum_key("Bold"), Some(700));
        assert_eq!(font.enum_key("Normal"), Some(400));
        assert_eq!(font.enum_key("SmallCaps"), Some(3));
        assert_eq!(font.enum_key("Nowhere"), None);
        // The value a font starts with is the key's number.
        assert_eq!(i64::from(ljos_engine::Font::default().weight), 400);
    }
}
