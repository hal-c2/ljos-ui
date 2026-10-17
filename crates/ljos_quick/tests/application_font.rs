//! A host's application font (`Font::set_application_font`), which a
//! desktop shell sets to the desktop's. A binary of its own: the font is the
//! process's.

mod common;

use common::*;
use ljos_engine::{Atom, Engine, Font, ObjectId};

const TEXT: &str = r#"import QtQuick
Item {
    property font made
    readonly property string family: t.font.family
    readonly property real points: t.font.pointSize
    readonly property int pixels: t.font.pixelSize
    readonly property real height: t.implicitHeight
    readonly property string madeFamily: made.family
    Text { id: t; text: "Hello" }
}"#;

/// The root's implicit height of `t`, once a frame has laid it out.
fn height(e: &Engine, root: ObjectId) -> f64 {
    let mut s = scene(e, root, 200.0, 100.0);
    frame(&mut s, e);
    num(e, root, "height")
}

fn text(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

#[test]
fn the_application_font_is_every_default_font() {
    let e = engine();
    let root = load(&e, TEXT);
    assert_eq!(text(&e, root, "family"), "Sans Serif");
    assert_eq!(num(&e, root, "points"), 9.0);
    let small = height(&e, root);

    Font::set_application_font(Some(&Font {
        family: Atom::from("Liberation Sans"),
        point_size: 11.0,
        ..Font::fallback()
    }));
    let e = engine();
    let root = load(&e, TEXT);
    assert_eq!(text(&e, root, "family"), "Liberation Sans");
    assert_eq!(num(&e, root, "points"), 11.0);
    // 11 pt reads 14 px, as in Qt, and is set at 15.
    assert_eq!(num(&e, root, "pixels"), 14.0);
    assert!(height(&e, root) > small);
    assert_eq!(text(&e, root, "madeFamily"), "Liberation Sans");

    Font::set_application_font(None);
    assert_eq!(Font::default().family.as_str(), "Sans Serif");
    assert_eq!(Font::default().point_size, Font::DEFAULT_POINT_SIZE);
}
