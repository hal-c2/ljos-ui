//! A desktop's interface font as the desktop names it, made into the font
//! Qt makes the application font of it: GNOME's Pango name
//! (`org.gnome.desktop.interface font-name`) and KDE's `QFont::toString`
//! string (`font` in the `[General]` group of `kdeglobals`). Measured on
//! Qt 6.11 (qml6, from outside).

use ljos_engine::{Atom, Font};

/// The words of a Pango font name that say a style, not the family.
const PANGO_STYLE_WORDS: [&str; 44] = [
    "normal",
    "roman",
    "oblique",
    "italic",
    "small-caps",
    "all-small-caps",
    "petite-caps",
    "all-petite-caps",
    "unicase",
    "title-caps",
    "ultra-condensed",
    "extra-condensed",
    "condensed",
    "semi-condensed",
    "semi-expanded",
    "expanded",
    "extra-expanded",
    "ultra-expanded",
    "thin",
    "ultra-light",
    "extra-light",
    "light",
    "semi-light",
    "demi-light",
    "book",
    "regular",
    "medium",
    "semi-bold",
    "demi-bold",
    "bold",
    "ultra-bold",
    "extra-bold",
    "heavy",
    "black",
    "ultra-black",
    "extra-black",
    "ultra-heavy",
    "not-rotated",
    "south",
    "upside-down",
    "north",
    "rotated-left",
    "east",
    "rotated-right",
];

/// A Pango font name (`Family [style words] [size]`) as Qt makes it the
/// application font: the family at the size's whole points, or 10 pt.
pub fn from_pango(name: &str) -> Option<Font> {
    // Variations (`@wght=500`) and families after the first are not the
    // family.
    let name = name.split('@').next()?;
    let mut words: Vec<&str> = name.split_whitespace().collect();
    let mut points = 10.0;
    if let Some(&last) = words.last()
        && let Ok(size) = last.trim_end_matches("px").parse::<f64>()
    {
        words.pop();
        if !last.ends_with("px") && size >= 1.0 {
            points = size.trunc();
        }
    }
    while words.len() > 1
        && words
            .last()
            .is_some_and(|w| PANGO_STYLE_WORDS.contains(&w.to_ascii_lowercase().as_str()))
    {
        words.pop();
    }
    let family = words.join(" ");
    let family = family.split(',').next()?.trim();
    (!family.is_empty()).then(|| Font {
        family: Atom::from(family),
        point_size: points,
        pixel_size: -1,
        ..Font::fallback()
    })
}

/// Weights of Qt 5's scale (0 to 99) and what they are on Qt 6's (CSS).
const QT5_WEIGHTS: [(i32, i32); 9] = [
    (0, 100),
    (12, 200),
    (25, 300),
    (50, 400),
    (57, 500),
    (63, 600),
    (75, 700),
    (81, 800),
    (87, 900),
];

/// A font as `QFont::toString` writes it (`Family,pt,px,hint,weight,
/// style,underline,strikeout,…`), Qt 5's ten or eleven fields or Qt 6's
/// sixteen or more.
pub fn from_qt_string(s: &str) -> Option<Font> {
    let fields: Vec<&str> = s.split(',').map(str::trim).collect();
    if fields.len() < 10 {
        return None;
    }
    let family = fields[0];
    let num = |i: usize| fields.get(i).and_then(|f| f.parse::<f64>().ok());
    let point_size = num(1)?;
    let pixel_size = num(2)? as i32;
    let qt6 = fields.len() >= 16;
    let weight = num(4)? as i32;
    let weight = if qt6 {
        weight.clamp(1, 1000)
    } else {
        // The nearest weight Qt 5 names.
        QT5_WEIGHTS
            .iter()
            .min_by_key(|(w, _)| (w - weight).abs())
            .map_or(400, |(_, css)| *css)
    };
    let style_name = fields.get(if qt6 { 16 } else { 10 }).copied().unwrap_or("");
    let (point_size, pixel_size) = if pixel_size > 0 {
        (-1.0, pixel_size)
    } else if point_size > 0.0 {
        (point_size, -1)
    } else {
        return None;
    };
    (!family.is_empty()).then(|| Font {
        family: Atom::from(family),
        point_size,
        pixel_size,
        weight,
        italic: num(5).is_some_and(|v| v != 0.0),
        style_name: Atom::from(style_name),
        ..Font::fallback()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(f: Option<Font>) -> Option<(String, f64, i32, i32, bool, String)> {
        f.map(|f| {
            (
                f.family.as_str().to_owned(),
                f.point_size,
                f.pixel_size,
                f.weight,
                f.italic,
                f.style_name.as_str().to_owned(),
            )
        })
    }

    #[test]
    fn a_pango_name_is_its_family_at_whole_points() {
        let f = |s: &str| summary(from_pango(s)).map(|(f, p, ..)| (f, p));
        assert_eq!(f("Adwaita Sans 11"), Some(("Adwaita Sans".into(), 11.0)));
        // Measured on Qt 6.11: the style words go, the size is cut.
        assert_eq!(
            f("Noto Serif Bold Italic 13"),
            Some(("Noto Serif".into(), 13.0))
        );
        assert_eq!(f("Noto Sans 10.5"), Some(("Noto Sans".into(), 10.0)));
        assert_eq!(f("Cantarell"), Some(("Cantarell".into(), 10.0)));
        assert_eq!(
            f("DejaVu Sans Condensed Semi-Bold 12"),
            Some(("DejaVu Sans".into(), 12.0))
        );
        assert_eq!(f("Noto Sans, 12"), Some(("Noto Sans".into(), 12.0)));
        assert_eq!(f(""), None);
        assert_eq!(summary(from_pango("Bold 12")).map(|s| s.3), Some(400));
    }

    #[test]
    fn a_kde_font_is_a_qfont_string() {
        // What Qt 6.11 made of each, read back.
        assert_eq!(
            summary(from_qt_string("Noto Sans,10,-1,5,50,0,0,0,0,0")),
            Some(("Noto Sans".into(), 10.0, -1, 400, false, String::new()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Sans,10,-1,5,75,1,0,0,0,0")),
            Some(("Noto Sans".into(), 10.0, -1, 700, true, String::new()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Serif,13,-1,5,400,0,0,0,0,0")),
            Some(("Noto Serif".into(), 13.0, -1, 900, false, String::new()))
        );
        assert_eq!(
            summary(from_qt_string(
                "Hack,11,-1,5,700,0,0,0,0,0,0,0,0,0,0,1,Bold"
            )),
            Some(("Hack".into(), 11.0, -1, 700, false, "Bold".into()))
        );
        assert_eq!(
            summary(from_qt_string("Noto Sans,10.5,-1,5,50,0,0,0,0,0")).map(|s| s.1),
            Some(10.5)
        );
        assert_eq!(summary(from_qt_string("Noto Sans,10")), None);
    }
}
