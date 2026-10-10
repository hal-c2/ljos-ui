//! `FontMetrics` and `TextMetrics` with the bundled font (Liberation Sans).
//!
//! The expected numbers were read from Qt 6.11 with the same font: at 14 px
//! and, for the font-wide ones, at a few other sizes.

mod common;

use common::*;
use ljos_engine::{Atom, Engine, ObjectId, Value};
use ljos_quick::{FontMetrics, TextMetrics, text_engine};

const WORDS: &str = "Hello world, this is a long line of text";

fn string(s: &str) -> Value {
    Value::String(Atom::from(s))
}

fn rect_of(v: &Value) -> (f64, f64, f64, f64) {
    match v {
        Value::Rect(r) => (r.x, r.y, r.width, r.height),
        other => panic!("not a rect: {other:?}"),
    }
}

fn text_of(e: &Engine, o: ObjectId, name: &str) -> String {
    get(e, o, name).to_js_string().to_string()
}

#[test]
fn font_metrics_are_qts() {
    let e = engine();
    let fm = make::<FontMetrics>(&e, None);
    for (name, want) in [
        ("ascent", 12.671875),
        ("descent", 2.953125),
        ("height", 15.625),
        ("leading", 0.0),
        ("lineSpacing", 15.625),
        ("strikeOutPosition", 12.671875 / 3.0),
        ("overlinePosition", 13.671875),
        ("underlinePosition", 0.453125),
        ("lineWidth", 1.03125),
        ("xHeight", 7.390625),
        ("averageCharacterWidth", 8.109375),
        ("maximumCharacterWidth", 19.0),
        ("minimumLeftBearing", -7.615234375),
        ("minimumRightBearing", -9.666015625),
    ] {
        assert_eq!(num(&e, fm, name), want, "{name}");
    }
}

#[test]
fn font_metrics_follow_the_font() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    FontMetrics { objectName: "small"; font.pixelSize: 8 }
    FontMetrics { objectName: "large"; font.pixelSize: 48 }
    FontMetrics { objectName: "points"; font.pointSize: 12 }
}"#,
    );
    let small = find(&e, root, "small");
    assert_eq!(num(&e, small, "ascent"), 7.234375);
    assert_eq!(num(&e, small, "height"), 8.921875);
    assert_eq!(
        num(&e, small, "lineWidth"),
        1.0,
        "never thinner than a pixel"
    );
    assert_eq!(num(&e, small, "maximumCharacterWidth"), 11.0);
    let large = find(&e, root, "large");
    assert_eq!(num(&e, large, "ascent"), 43.453125);
    assert_eq!(num(&e, large, "descent"), 10.171875);
    assert_eq!(num(&e, large, "xHeight"), 25.359375);
    assert_eq!(num(&e, large, "averageCharacterWidth"), 27.828125);
    assert_eq!(num(&e, large, "underlinePosition"), 1.578125);
    // 12 pt is 16 px.
    assert_eq!(num(&e, find(&e, root, "points"), "ascent"), 14.484375);

    let font = get(&e, small, "font")
        .with_member(&ljos_engine::TypeTag::Font, "pixelSize", &Value::Int(48))
        .expect("font.pixelSize");
    set(&e, small, "font", font);
    assert_eq!(num(&e, small, "ascent"), 43.453125);
    assert_eq!(num(&e, small, "strikeOutPosition"), 43.453125 / 3.0);
}

#[test]
fn a_point_size_is_set_at_a_whole_pixel_size() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    property real up: eleven.advanceWidth("Hello world")
    property real down: ten.advanceWidth("Hello world")
    FontMetrics { id: eleven; font.pointSize: 11 }
    FontMetrics { id: ten; font.pointSize: 10 }
    FontMetrics { objectName: "even"; font.pointSize: 10.5 }
    Text { objectName: "text"; text: "Hello world"; font.pointSize: 11 }
}"#,
    );
    // 14.67 px is set at 15 px and 13.33 px at 13 px, as Qt does.
    assert_eq!(num(&e, root, "up"), 74.09375);
    assert_eq!(num(&e, root, "down"), 64.1875);
    assert_eq!(num(&e, find(&e, root, "even"), "ascent"), 12.671875);
    // A Text gets its numbers from the scene's layout pass.
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let text = find(&e, root, "text");
    assert_eq!(num(&e, text, "width"), 74.09375);
    assert_eq!(num(&e, text, "height"), 17.0);
}

#[test]
fn font_metrics_measure_text() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    property real advance: fm.advanceWidth("Hello world")
    property real twoLines: fm.advanceWidth("Hello\nworld")
    property real number: fm.advanceWidth(42)
    property rect box: fm.boundingRect("Hello world")
    property rect tight: fm.tightBoundingRect("Hello world")
    property rect dash: fm.tightBoundingRect("-")
    property rect nothing: fm.boundingRect("")
    property rect spaced: wide.boundingRect("Hello")
    property real spacedAdvance: wide.advanceWidth("Hello")
    FontMetrics { id: fm }
    FontMetrics { id: wide; font.letterSpacing: 2 }
}"#,
    );
    assert_eq!(num(&e, root, "advance"), 69.203125);
    assert_eq!(
        num(&e, root, "twoLines"),
        69.203125 - 3.875,
        "a line break takes no room"
    );
    assert_eq!(num(&e, root, "number"), 2.0 * 7.78125);
    assert_eq!(
        rect_of(&get(&e, root, "box")),
        (1.0, -12.671875, 68.0, 15.625)
    );
    let (x, y, w, h) = rect_of(&get(&e, root, "tight"));
    assert_eq!((x, w), (1.0, 68.0));
    // Qt: (1, -11, 68, 11) from hinted outlines; ours are unhinted.
    assert!((y + 11.0).abs() <= 1.0 && (y + h).abs() <= 1.0, "{y} {h}");
    assert_eq!(rect_of(&get(&e, root, "dash")), (0.0, -5.0, 5.0, 5.0));
    assert_eq!(rect_of(&get(&e, root, "nothing")), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(num(&e, root, "spacedAdvance"), 41.890625);
    let (x, _, w, _) = rect_of(&get(&e, root, "spaced"));
    assert_eq!((x, w), (1.0, 40.0));
}

#[test]
fn font_metrics_elide() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    property string atEnd: fm.elidedText("Hello world", Qt.ElideRight, 40)
    property string atStart: fm.elidedText("Hello world", Qt.ElideLeft, 40)
    property string inMiddle: fm.elidedText("Hello world", Qt.ElideMiddle, 40)
    property string asIs: fm.elidedText("Hello world", Qt.ElideNone, 40)
    property string fits: fm.elidedText("Hello world", Qt.ElideRight, 70)
    property string flagged: fm.elidedText("Hello world", Qt.ElideRight, 40, 0)
    property string noRoom: fm.elidedText("Hello world", Qt.ElideRight, 0)
    FontMetrics { id: fm }
}"#,
    );
    assert_eq!(text_of(&e, root, "atEnd"), "Hell…");
    assert_eq!(text_of(&e, root, "atStart"), "…orld");
    assert_eq!(text_of(&e, root, "inMiddle"), "H…d");
    assert_eq!(text_of(&e, root, "asIs"), "Hello world");
    assert_eq!(text_of(&e, root, "fits"), "Hello world");
    assert_eq!(text_of(&e, root, "flagged"), "Hell…");
    assert_eq!(text_of(&e, root, "noRoom"), "");
}

#[test]
fn text_metrics_are_qts() {
    let e = engine();
    let tm = make::<TextMetrics>(&e, None);
    assert_eq!(num(&e, tm, "elide"), 3.0, "Qt.ElideNone");
    assert_eq!(num(&e, tm, "width"), 0.0);
    assert_eq!(num(&e, tm, "height"), 0.0);
    assert_eq!(rect_of(&get(&e, tm, "boundingRect")), (0.0, 0.0, 0.0, 0.0));

    set(&e, tm, "text", string("Hello world"));
    assert_eq!(num(&e, tm, "advanceWidth"), 69.203125);
    assert_eq!(num(&e, tm, "width"), 68.0);
    assert_eq!(num(&e, tm, "height"), 15.625);
    assert_eq!(
        rect_of(&get(&e, tm, "boundingRect")),
        (1.0, -12.671875, 68.0, 15.625)
    );
    let (x, _, w, h) = rect_of(&get(&e, tm, "tightBoundingRect"));
    assert_eq!((x, w), (1.0, 68.0));
    assert!((h - 11.0).abs() <= 1.0, "{h}");
    assert_eq!(text_of(&e, tm, "elidedText"), "Hello world");
}

#[test]
fn text_metrics_elide() {
    let e = engine();
    let tm = make::<TextMetrics>(&e, None);
    set(&e, tm, "text", string(WORDS));
    set(&e, tm, "elide", Value::Int(1));
    assert_eq!(text_of(&e, tm, "elidedText"), "", "an elide width of 0");
    set(&e, tm, "elideWidth", Value::Real(100.0));
    let right = text_of(&e, tm, "elidedText");
    assert!(
        right.starts_with("Hello world") && right.ends_with('…'),
        "{right}"
    );
    set(&e, tm, "elide", Value::Int(2));
    assert_eq!(text_of(&e, tm, "elidedText"), "Hello w…of text");
    set(&e, tm, "elide", Value::Int(0));
    let left = text_of(&e, tm, "elidedText");
    assert!(
        left.starts_with('…') && left.ends_with("line of text"),
        "{left}"
    );
    set(&e, tm, "elide", Value::Int(3));
    assert_eq!(text_of(&e, tm, "elidedText"), WORDS);
    // The measured size is the whole text's whatever is elided.
    assert!(num(&e, tm, "width") > 200.0);
}

#[test]
fn one_change_measures_once() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    property real sum: tm.advanceWidth + tm.width + tm.height
        + tm.boundingRect.x + tm.tightBoundingRect.y
    TextMetrics { id: tm; objectName: "tm"; text: "Hello world" }
}"#,
    );
    let tm = find(&e, root, "tm");
    let first = num(&e, root, "sum");
    assert!(first > 100.0);
    let before = text_engine().borrow().stats();
    set(&e, tm, "text", string("Hello world, and more"));
    assert!(num(&e, root, "sum") > first);
    let after = text_engine().borrow().stats();
    assert_eq!(
        after.layout_misses - before.layout_misses,
        1,
        "five properties, one layout"
    );
    // Reading again asks the text engine nothing.
    let _ = num(&e, root, "sum");
    let again = text_engine().borrow().stats();
    assert_eq!(again.layout_misses, after.layout_misses);
    assert_eq!(again.layout_hits, after.layout_hits);
}

#[test]
fn unread_metrics_shape_nothing() {
    let e = engine();
    let before = text_engine().borrow().stats();
    let _ = load(
        &e,
        r#"import QtQuick
Item {
    FontMetrics { font.pixelSize: 31 }
    TextMetrics { text: "Nobody asks"; font.pixelSize: 31 }
}"#,
    );
    let after = text_engine().borrow().stats();
    assert_eq!(after.layout_misses, before.layout_misses);
    assert_eq!(after.shapes_built, before.shapes_built);
}
