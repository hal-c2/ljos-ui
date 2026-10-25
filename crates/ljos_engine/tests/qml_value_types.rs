//! `point`, `size`, `rect`, the vectors, `quaternion` and `font` in JS:
//! values that compare by what they hold, print as Qt prints them, and
//! whose members can be written where the value sits in a property. The
//! expected values are what qml6 6.11.2 gives for the same QML.

mod support;

use ljos_engine::{Font, Value, Vector3d};
use support::engine;

#[test]
fn equal_values_are_the_same_value() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property rect area: Qt.rect(1, 2, 3, 4)
  property var kept: Qt.size(5, 6)
  property bool strict: area === Qt.rect(1, 2, 3, 4)
  property bool untyped: kept === Qt.size(5, 6)
  property bool other: area == Qt.rect(1, 2, 3, 5)
  property bool kinds: Qt.point(1, 2) == Qt.vector2d(1, 2)
  property int found: [Qt.point(0, 0), Qt.vector3d(1, 2, 3)].indexOf(Qt.vector3d(1, 2, 3))
  property bool asText: area == "QRectF(1, 2, 3, 4)"
  property bool fonts: Qt.font({ family: "Arial" }) === Qt.font({ family: "Arial" })
}
"##,
    );
    for (prop, want) in [
        ("strict", true),
        ("untyped", true),
        ("other", false),
        ("kinds", false),
        ("asText", true),
        ("fonts", true),
    ] {
        assert_eq!(h.get(root, prop), Value::Bool(want), "{prop}");
    }
    assert_eq!(h.num(root, "found"), 1.0);
    // A `var` keeps the value itself, not a JS object that happens to
    // look like one.
    assert_eq!(h.get(root, "kept"), Value::size(5.0, 6.0));
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn values_print_the_same_from_js_and_from_the_engine() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property point spot: Qt.point(1234567.891, 0.000001234)
  property rect area: Qt.rect(0.3, 1e-7, 100000.5, 1e6)
  property vector3d vector: Qt.vector3d(0.1, 1 / 3, 12345678)
  property quaternion turn: Qt.quaternion(1, 2, 3, 4)
  property font face: Qt.font({ family: "Arial", pointSize: 10, bold: true, letterSpacing: 2 })
  property var texts: ["" + spot, "" + area, "" + vector, "" + turn, "" + face]
  property string members: Object.keys(area).join()
}
"##,
    );
    let want = [
        ("spot", "QPointF(1.23457e+06, 1.234e-06)"),
        ("area", "QRectF(0.3, 1e-07, 100001, 1e+06)"),
        ("vector", "QVector3D(0.1, 0.333333, 1.23457e+07)"),
        ("turn", "QQuaternion(1, 2, 3, 4)"),
        (
            "face",
            "QFont(Arial,10,-1,5,700,0,0,0,0,0,0,1,2,0,0,1,,0,0)",
        ),
    ];
    let Value::List(texts) = h.get(root, "texts") else {
        panic!("texts is a list");
    };
    for (i, (prop, text)) in want.iter().enumerate() {
        assert_eq!(h.get(root, prop).to_js_string().as_str(), *text, "{prop}");
        assert_eq!(texts[i].to_js_string().as_str(), *text, "{prop} in JS");
    }
    assert_eq!(
        h.get(root, "members").to_js_string().as_str(),
        "x,y,width,height,left,right,top,bottom"
    );
}

#[test]
fn writing_a_member_writes_the_property() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  id: root
  property int wide: 10
  property rect area: Qt.rect(1, 2, wide, 4)
  property font face: Qt.font({ family: "Arial", pixelSize: 12 })
  property int changes: 0
  onAreaChanged: changes++
  property var seen: []
  function move() { changes = 0; area.x = 5; root.area.y += 10 }
  function stay() { area.x = 5; area.nothing = 1 }
  function count() { seen = [area.height++, ++area.height, (area.width = "7")] }
  function embolden() { face.bold = true; face.pixelSize = 30 }
  function local() {
    var kept = area
    kept.x = 100
    var list = [Qt.point(1, 2)]
    list[0].y = 9
    return kept + " " + list[0] + " " + area
  }
}
"##,
    );
    h.engine.invoke_by_name(root, "move", &[]).unwrap();
    assert_eq!(h.get(root, "area"), Value::rect(5.0, 12.0, 10.0, 4.0));
    assert_eq!(h.num(root, "changes"), 2.0);
    // The write took the binding off, as any write from JS does.
    h.set(root, "wide", Value::Int(20));
    assert_eq!(h.get(root, "area"), Value::rect(5.0, 12.0, 10.0, 4.0));
    h.engine.invoke_by_name(root, "stay", &[]).unwrap();
    assert_eq!(h.num(root, "changes"), 2.0);
    h.engine.invoke_by_name(root, "count", &[]).unwrap();
    assert_eq!(h.get(root, "area"), Value::rect(5.0, 12.0, 7.0, 6.0));
    // A list is equal only to itself, so its items are compared.
    let Value::List(seen) = h.get(root, "seen") else {
        panic!("seen is a list");
    };
    assert_eq!(
        seen.iter().map(Value::to_js_string).collect::<Vec<_>>(),
        ["4", "6", "7"]
    );
    h.engine.invoke_by_name(root, "embolden", &[]).unwrap();
    let Value::Font(face) = h.get(root, "face") else {
        panic!("face is a font");
    };
    assert_eq!((face.weight, face.pixel_size), (700, 30));
    // A variable read from a property keeps where it was read from, as in
    // Qt 6 (features/language/value-types-in-variables.feature): changing
    // it writes the property too.
    let out = h.engine.invoke_by_name(root, "local", &[]).unwrap();
    assert_eq!(
        out.to_js_string().as_str(),
        "QRectF(100, 12, 7, 6) QPointF(1, 9) QRectF(100, 12, 7, 6)"
    );
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn vectors_are_single_precision_and_do_arithmetic() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property vector3d v: Qt.vector3d(0.1, 2, 3)
  property real tenth: v.x
  property vector3d sum: Qt.vector3d(1, 2, 3).plus(Qt.vector3d(1, 1, 1)).times(2)
  property real length: Qt.vector3d(1, 2, 3).length()
  property vector3d cross: Qt.vector3d(1, 0, 0).crossProduct(Qt.vector3d(0, 1, 0))
  property vector2d cut: Qt.vector3d(1, 2, 3).toVector2d()
  property bool near: Qt.vector3d(1, 2, 3).fuzzyEquals(Qt.vector3d(1, 2, 3.00001))
  property bool far: Qt.vector3d(1, 2, 3).fuzzyEquals(Qt.vector3d(1, 2, 3.0001))
}
"##,
    );
    assert_eq!(h.num(root, "tenth"), f64::from(0.1_f32));
    assert_eq!(
        h.get(root, "sum"),
        Value::Vector3d(Vector3d {
            x: 4.0,
            y: 6.0,
            z: 8.0
        })
    );
    // One step below the nearest single to the square root of 14: what
    // qml6 gives.
    assert_eq!(h.num(root, "length"), 3.741_657_257_080_078);
    assert_eq!(h.get(root, "cross"), Value::vector3d(0.0, 0.0, 1.0));
    assert_eq!(h.get(root, "cut"), Value::vector2d(1.0, 2.0));
    assert_eq!(h.get(root, "near"), Value::Bool(true));
    assert_eq!(h.get(root, "far"), Value::Bool(false));
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn the_makers_count_their_arguments() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property var few: Qt.rect(1, 2, 3)
  property var many: Qt.point(1, 2, 3)
  property var face: Qt.font({ colour: 1 })
}
"##,
    );
    for prop in ["few", "many", "face"] {
        assert_eq!(h.get(root, prop), Value::Undefined, "{prop}");
    }
    let warnings = h.warnings().join("\n");
    assert!(warnings.contains("Insufficient arguments"), "{warnings}");
    assert!(warnings.contains("Too many arguments"), "{warnings}");
    assert!(
        warnings.contains("no valid font subproperties specified"),
        "{warnings}"
    );
}

/// What Qt 6.11.2 draws is in `features/text/font-capitalization.feature`.
#[test]
fn a_font_draws_the_letters_its_capitalization_asks_for() {
    let drawn = |capitalization: i32, text: &str| {
        let font = Font {
            capitalization,
            ..Font::default()
        };
        font.drawn(text).into_owned()
    };
    assert_eq!(drawn(Font::MIXED_CASE, "hello wORLD"), "hello wORLD");
    assert_eq!(drawn(Font::ALL_UPPERCASE, "hello wORLD"), "HELLO WORLD");
    assert_eq!(drawn(Font::ALL_LOWERCASE, "hello wORLD"), "hello world");
    assert_eq!(drawn(Font::CAPITALIZE, "hello wORLD"), "Hello WORLD");
    // A letter whose capital is two letters stays, so that a position in
    // the text is the same position in what is drawn.
    assert_eq!(drawn(Font::ALL_UPPERCASE, "straße"), "STRAßE");
    // Small capitals are stored and not drawn.
    assert_eq!(drawn(Font::SMALL_CAPS, "hello"), "hello");
    // Text that needs no change is not copied.
    assert!(matches!(
        Font::default().drawn("hello"),
        std::borrow::Cow::Borrowed(_)
    ));
}

/// A theme switch writes a member back and forth, and a value keeps the
/// answer it last gave. What each write leaves is still the value the
/// members say, whatever was written before it.
#[test]
fn a_member_written_back_and_forth_leaves_the_value_it_says() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property font face: Qt.font({ family: "Arial", pixelSize: 12 })
  property point spot: Qt.point(1, 2)
  property var seen: []
  Component.onCompleted: {
    var out = []
    for (var i = 0; i < 4; ++i) {
      face.bold = (i & 1) == 0
      out.push(face.bold + "/" + face.pixelSize)
      face.pixelSize = 12 + (i & 1)
      out.push(face.bold + "/" + face.pixelSize)
    }
    var first = spot
    spot.x = 5
    spot.x = 1
    out.push(spot === first, spot.x)
    seen = out
  }
}
"##,
    );
    let seen = h.get(root, "seen").to_js_string();
    assert_eq!(
        seen.as_str(),
        "true/12,true/12,false/12,false/13,true/13,true/12,false/12,false/13,true,1"
    );
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}
