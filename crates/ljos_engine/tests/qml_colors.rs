//! A `color` in JS: an object with channels that compares by value. The
//! expected values are what qml6 6.11.2 gives for the same QML.

mod support;

use ljos_engine::{Color, Value};
use support::engine;

fn color(v: Value) -> [u16; 4] {
    match v {
        Value::Color(Color { r, g, b, a, .. }) => [r, g, b, a],
        other => panic!("not a colour: {other:?}"),
    }
}

#[test]
fn the_channels_of_a_colour_can_be_read() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Rectangle {
  color: "#204080"
  property real red: color.r
  property real green: color.g
  property real blue: color.b
  property real alpha: color.a
  property real hue: color.hsvHue
  property real lightness: color.hslLightness
  property string kind: typeof color
  property bool valid: color.valid
}
"##,
    );
    let near = |prop: &str, want: f64| {
        let got = h.num(root, prop);
        assert!((got - want).abs() < 1e-7, "{prop}: {got}");
    };
    near("red", 32.0 / 255.0);
    near("green", 64.0 / 255.0);
    near("blue", 128.0 / 255.0);
    near("alpha", 1.0);
    near("hue", 220.0 / 360.0);
    near("lightness", 80.0 / 255.0);
    assert_eq!(h.get(root, "kind").to_js_string().as_str(), "object");
    assert_eq!(h.get(root, "valid"), Value::Bool(true));
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn colours_compare_by_their_channels() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Rectangle {
  id: root
  color: "red"
  Rectangle { id: other; color: "#ff0000" }
  property bool loosely: root.color == other.color
  property bool strictly: root.color === other.color
  property bool made: root.color === Qt.rgba(1, 0, 0, 1)
  property bool differs: root.color != Qt.rgba(0, 0, 1, 1)
  property bool close: Qt.rgba(0.5, 0, 0, 1) == Qt.rgba(0.501, 0, 0, 1)
  property bool closer: Qt.rgba(0.5, 0, 0, 1) == Qt.rgba(0.500001, 0, 0, 1)
  property bool alpha: Qt.rgba(1, 0, 0, 0.999) == root.color
  property int at: [Qt.rgba(0, 0, 1, 1), other.color].indexOf(root.color)
  property bool hex: root.color == "#ff0000"
  property bool strictHex: root.color === "#ff0000"
  property bool name: root.color == "red"
}
"##,
    );
    for (prop, want) in [
        ("loosely", true),
        ("strictly", true),
        ("made", true),
        ("differs", true),
        ("close", false),
        ("closer", true),
        ("alpha", false),
        ("hex", true),
        ("strictHex", false),
        ("name", false),
    ] {
        assert_eq!(h.get(root, prop), Value::Bool(want), "{prop}");
    }
    assert_eq!(h.num(root, "at"), 1.0);
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn a_colour_turns_into_its_hex_text() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Rectangle {
  color: "red"
  property string text: color
  property string joined: "is " + color
  property string translucent: Qt.rgba(0.5, 0.25, 0, 0.5)
  property string nearlyOpaque: Qt.rgba(1, 0, 0, 0.999)
  property string notQuite: Qt.rgba(1, 0, 0, 0.997)
  Component.onCompleted: console.log(color, Qt.rgba(0, 0, 1, 0.5))
}
"##,
    );
    let text = |prop: &str| h.get(root, prop).to_js_string().to_string();
    assert_eq!(text("text"), "#ff0000");
    assert_eq!(text("joined"), "is #ff0000");
    assert_eq!(text("translucent"), "#80804000");
    assert_eq!(text("nearlyOpaque"), "#ff0000");
    assert_eq!(text("notQuite"), "#feff0000");
    assert_eq!(h.log(), ["#ff0000 #800000ff"]);
}

#[test]
fn a_var_property_keeps_a_colour() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property var kept: Qt.rgba(0, 0, 1, 1)
  property real blue: kept.b
  property bool same: kept === Qt.rgba(0, 0, 1, 1)
  property var list: [Qt.rgba(1, 0, 0, 1), "#ff0000"]
}
"##,
    );
    assert_eq!(color(h.get(root, "kept")), [0, 0, 65535, 65535]);
    assert_eq!(h.num(root, "blue"), 1.0);
    assert_eq!(h.get(root, "same"), Value::Bool(true));
    // A colour inside a list is still one; text stays text.
    let Value::List(items) = h.get(root, "list") else {
        panic!("not a list");
    };
    assert_eq!(color(items[0].clone()), [65535, 0, 0, 65535]);
    assert_eq!(items[1], Value::from("#ff0000"));
}

#[test]
fn qt_rgba_keeps_sixteen_bits_and_bounds_what_it_is_given() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property color half: Qt.rgba(0.5, 0.25, 0.125, 0.75)
  property color tenths: Qt.rgba(0.1, 0.2, 0.3)
  property color bounded: Qt.rgba(-1, 2, 0.5, 7)
  property color text: Qt.rgba("1", "x", NaN, undefined)
  property color noAlpha: Qt.rgba(1, 0, 0, null)
  property color single: Qt.rgba(0.9271, 0.3141, 0.7, 0.123456)
}
"##,
    );
    assert_eq!(color(h.get(root, "half")), [32768, 16384, 8192, 49151]);
    assert_eq!(color(h.get(root, "tenths")), [6554, 13107, 19661, 65535]);
    assert_eq!(color(h.get(root, "bounded")), [0, 65535, 32768, 65535]);
    assert_eq!(color(h.get(root, "text")), [65535, 0, 0, 65535]);
    assert_eq!(color(h.get(root, "noAlpha")), [65535, 0, 0, 0]);
    // 0.9271 * 65535 is 60757.4985: Qt rounds it in single precision, up.
    assert_eq!(color(h.get(root, "single")), [60758, 20585, 45875, 8091]);
    assert_eq!(
        Color::from_rgba_f(0.9271, 0.3141, 0.7, 0.123456),
        Color::rgba16(60758, 20585, 45875, 8091)
    );
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn qt_rgba_needs_three_or_four_arguments() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property string few
  property string many
  Component.onCompleted: {
    try { Qt.rgba(1, 0) } catch (e) { few = e.message }
    try { Qt.hsla(1, 0, 0, 1, 1) } catch (e) { many = e.message }
  }
}
"##,
    );
    assert_eq!(
        h.get(root, "few").to_js_string().as_str(),
        "Unable to determine callable overload.  Candidates are:\n    rgba(double,double,double)\n    rgba(double,double,double,double)"
    );
    assert_eq!(
        h.get(root, "many").to_js_string().as_str(),
        "Too many arguments"
    );
}

#[test]
fn qt_hsla_and_hsva_give_the_channels_qt_gives() {
    let h = engine();
    let cases: [(&str, [u16; 4]); 22] = [
        (
            "Qt.hsla(0.45, 0.33, 0.77, 0.2)",
            [45488, 55436, 52452, 13107],
        ),
        ("Qt.hsva(0.13, 0.57, 0.91)", [59637, 52159, 25644, 65535]),
        ("Qt.hsla(0.6, 0.7, 0.5, 1)", [9831, 28181, 55705, 65535]),
        ("Qt.hsva(0.9, 1, 1, 1)", [65535, 0, 39321, 65535]),
        ("Qt.hsla(0.05, 0.95, 0.05, 1)", [6390, 2032, 164, 65535]),
        ("Qt.hsva(0.77, 0.01, 0.99, 1)", [64634, 64232, 64880, 65535]),
        ("Qt.hsla(0.1, 0.4, 0.3, 0.6)", [27525, 21234, 11797, 39321]),
        ("Qt.hsva(0.7, 0.4, 0.3, 1)", [13369, 11797, 19661, 65535]),
        ("Qt.hsva(0, 0, 0.5, 1)", [32768, 32768, 32768, 65535]),
        // Out of range is bounded, and a full turn is red again.
        ("Qt.hsla(1.5, 2, -1, 1)", [0, 0, 0, 65535]),
        ("Qt.hsla(-0.5, 1, 0.5, 1)", [65535, 0, 0, 65535]),
        ("Qt.hsla(1, 1, 0.5, 1)", [65535, 0, 0, 65535]),
        ("Qt.hsla(0.5, 1, 0.5)", [0, 65535, 65535, 65535]),
        // Fully saturated at half lightness is a pure colour.
        ("Qt.hsla(0, 1, 0.5, 1)", [65535, 0, 0, 65535]),
        ("Qt.hsla(0.1, 1, 0.5, 1)", [65535, 39321, 0, 65535]),
        ("Qt.hsla(0, 1, 0.75, 1)", [65535, 32767, 32767, 65535]),
        ("Qt.hsla(0, 0.5, 0.5, 1)", [49152, 16384, 16384, 65535]),
        ("Qt.hsla(0, 0.5, 2 / 65535, 1)", [3, 0, 0, 65535]),
        ("Qt.hsla(0, 0.5, 4 / 65535, 1)", [6, 2, 2, 65535]),
        ("Qt.hsva(0, 65534 / 65535, 1, 1)", [65535, 1, 1, 65535]),
        // Numbers Qt rounds in single precision.
        (
            "Qt.hsla(0.9422, 0.3306, 0.9271, 1)",
            [62337, 59179, 60274, 65535],
        ),
        (
            "Qt.hsva(0.9422, 0.3306, 0.9271, 1)",
            [60758, 40671, 47638, 65535],
        ),
    ];
    let props: String = cases
        .iter()
        .enumerate()
        .map(|(i, (expr, _))| format!("  property color c{i}: {expr}\n"))
        .collect();
    let root = h.load(&format!("import QtQuick\nItem {{\n{props}}}\n"));
    for (i, (expr, want)) in cases.iter().enumerate() {
        assert_eq!(color(h.get(root, &format!("c{i}"))), *want, "{expr}");
    }
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

#[test]
fn a_binding_on_a_channel_follows_the_colour() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Rectangle {
  color: "red"
  property real blue: color.b
  function paint() { color = Qt.rgba(0, 0, 1, 0.5) }
}
"##,
    );
    assert_eq!(h.num(root, "blue"), 0.0);
    h.set(root, "color", Value::from("#0000ff"));
    assert_eq!(h.num(root, "blue"), 1.0);
    h.set(root, "color", Value::from("black"));
    h.engine.invoke_by_name(root, "paint", &[]).unwrap();
    assert_eq!(color(h.get(root, "color")), [0, 0, 65535, 32768]);
    assert_eq!(h.num(root, "blue"), 1.0);
}

/// One object stands for every use of a colour, so the channel is not
/// written on it: the write is compiled as one of the whole colour, changed,
/// to the property it was read from. That is the write-back Qt does.
#[test]
fn writing_a_channel_changes_the_property_it_was_read_from() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Rectangle {
  id: root
  color: "red"
  property int changes: 0
  onColorChanged: changes++
  function tint() { color.b = 1 }
  function same() { root.color.r = 1 }
  function fade() { root.color.a = 0.5; color.hsvHue = 0.5 }
}
"##,
    );
    h.engine.invoke_by_name(root, "tint", &[]).unwrap();
    assert_eq!(color(h.get(root, "color")), [65535, 0, 65535, 65535]);
    assert_eq!(h.num(root, "changes"), 1.0);
    h.engine.invoke_by_name(root, "same", &[]).unwrap();
    assert_eq!(h.num(root, "changes"), 1.0);
    h.engine.invoke_by_name(root, "fade", &[]).unwrap();
    assert_eq!(color(h.get(root, "color")), [0, 65535, 65535, 32768]);
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}

/// What Qt makes from a colour, in the channels qml6 gives.
#[test]
fn qt_makes_colours_from_colours() {
    let h = engine();
    let root = h.load(
        r##"import QtQuick
Item {
  property color lighter: Qt.lighter("#336699")
  property color darker: Qt.darker("#336699", 3)
  property color tinted: Qt.tint("#336699", "#80ff0000")
  property color faded: Qt.alpha("red", 0.5)
  property color named: Qt.color("steelblue")
  property var nothing: Qt.lighter("nope")
  property bool equal: Qt.colorEqual("red", Qt.rgba(1, 0, 0, 1))
  property string kind: typeof Qt.red
}
"##,
    );
    let hex = |prop: &str| h.get(root, prop).to_js_string().to_string();
    assert_eq!(hex("lighter"), "#4c99e5");
    assert_eq!(hex("darker"), "#112233");
    assert_eq!(hex("tinted"), "#99334c");
    assert_eq!(hex("faded"), "#80ff0000");
    assert_eq!(hex("named"), "#4682b4");
    assert_eq!(h.get(root, "nothing"), Value::Null);
    assert_eq!(h.get(root, "equal"), Value::Bool(true));
    assert_eq!(h.get(root, "kind").to_js_string().as_str(), "number");
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}
