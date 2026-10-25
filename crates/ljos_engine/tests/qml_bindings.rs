//! QML compiled with `ljos_compiler` and run on the engine: bindings,
//! geometry and property semantics. Mirrors features/bindings/*.feature
//! and features/items/geometry.feature.

mod support;

use ljos_engine::{Color, Value};
use support::engine;

const BINDINGS: &str = r#"
import QtQuick
Item {
  objectName: "root"
  property int a: 1
  property int b: a + 1
}
"#;

#[test]
fn a_binding_is_evaluated_once_at_creation() {
    let h = engine();
    let root = h.load(BINDINGS);
    assert_eq!(h.num(root, "b"), 2.0);
}

#[test]
fn a_binding_follows_its_dependencies() {
    let h = engine();
    let root = h.load(BINDINGS);
    h.set(root, "a", Value::Int(5));
    assert_eq!(h.num(root, "b"), 6.0);
}

#[test]
fn assigning_a_value_breaks_the_binding() {
    let h = engine();
    let root = h.load(BINDINGS);
    h.set(root, "b", Value::Int(100));
    h.set(root, "a", Value::Int(8));
    assert_eq!(h.num(root, "b"), 100.0);
}

#[test]
fn qt_binding_puts_a_binding_back() {
    let h = engine();
    h.files(&[(
        "Main",
        r#"
import QtQuick
Item {
  property int a: 1
  property int b: a + 1
  function rebind() { b = Qt.binding(function() { return a + 1 }) }
  function rebindTimesTen() { b = Qt.binding(() => a * 10) }
  function assign(v) { b = v }
}
"#,
    )]);
    let root = h.engine.create_from_url("Main", None, vec![]).unwrap();
    h.engine
        .invoke_by_name(root, "assign", &[Value::Int(100)])
        .unwrap();
    h.set(root, "a", Value::Int(8));
    assert_eq!(h.num(root, "b"), 100.0);
    h.engine.invoke_by_name(root, "rebind", &[]).unwrap();
    h.set(root, "a", Value::Int(9));
    assert_eq!(h.num(root, "b"), 10.0);
    h.engine
        .invoke_by_name(root, "rebindTimesTen", &[])
        .unwrap();
    h.set(root, "a", Value::Int(3));
    assert_eq!(h.num(root, "b"), 30.0);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_js_assignment_in_a_handler_breaks_a_binding() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int a: 1
  property int b: a * 2
  signal poke()
  onPoke: b = 7
}
"#,
    );
    h.engine.emit_by_name(root, "poke", &[]).unwrap();
    h.set(root, "a", Value::Int(4));
    assert_eq!(h.num(root, "b"), 7.0);
}

const GEOMETRY: &str = r##"
import QtQuick
Item {
  objectName: "root"
  width: 200; height: 100
  Rectangle {
    objectName: "box"
    x: 10; y: 20
    width: parent.width / 2
    height: 30
    color: "#ff0000"
  }
}
"##;

#[test]
fn a_childs_width_is_bound_to_its_parent() {
    let h = engine();
    let root = h.load(GEOMETRY);
    let box_ = h.find(root, "box");
    assert_eq!(h.num(box_, "width"), 100.0);
    assert_eq!(h.num(box_, "x"), 10.0);
    assert_eq!(h.engine.visual_parent(box_), Some(root));
}

#[test]
fn a_child_follows_its_parent_when_the_parent_resizes() {
    let h = engine();
    let root = h.load(GEOMETRY);
    let box_ = h.find(root, "box");
    h.set(root, "width", Value::Int(400));
    assert_eq!(h.num(box_, "width"), 200.0);
}

#[test]
fn rectangle_reports_its_colour_as_hex() {
    let h = engine();
    let root = h.load(GEOMETRY);
    let box_ = h.find(root, "box");
    assert_eq!(
        h.get(box_, "color"),
        Value::Color(Color::from_rgba8(255, 0, 0, 255))
    );
    h.set(box_, "color", Value::from("#0000FF"));
    assert_eq!(h.get(box_, "color").to_js_string().as_str(), "#0000ff");
}

#[test]
fn setting_a_property_that_does_not_exist_is_rejected() {
    let h = engine();
    let root = h.load(GEOMETRY);
    let box_ = h.find(root, "box");
    assert!(
        h.engine
            .write_by_name(box_, "nothing", Value::Int(1))
            .is_err()
    );
}

#[test]
fn a_parent_path_binding_reads_through_tier_zero_and_js_alike() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  id: root
  width: 50
  Item { objectName: "a"; width: root.width }
  Item { objectName: "b"; width: root.width + 0 }
  Item { objectName: "c"; width: parent.width * 2 }
}
"#,
    );
    let (a, b, c) = (h.find(root, "a"), h.find(root, "b"), h.find(root, "c"));
    h.set(root, "width", Value::Int(70));
    assert_eq!(h.num(a, "width"), 70.0);
    assert_eq!(h.num(b, "width"), 70.0);
    assert_eq!(h.num(c, "width"), 140.0);
}

#[test]
fn grouped_properties_set_and_bind() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Rectangle {
  property int w: 3
  border.width: w
  border { color: "blue" }
}
"#,
    );
    assert_eq!(h.num(root, "border.width"), 3.0);
    h.set(root, "w", Value::Int(5));
    assert_eq!(h.num(root, "border.width"), 5.0);
    assert_eq!(
        h.get(root, "border.color").to_js_string().as_str(),
        "#0000ff"
    );
}

#[test]
fn change_handlers_run_for_initial_bindings_before_on_completed() {
    let h = engine();
    h.load(
        r#"
import QtQuick
Item {
  property int a: 1
  property int b: a + 1
  property int c: 5
  onBChanged: console.log("b", b)
  onCChanged: console.log("c", c)
  Component.onCompleted: console.log("done")
}
"#,
    );
    assert_eq!(h.log(), vec!["b 2", "done"]);
}

#[test]
fn aliases_read_and_write_their_target() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property alias kidWidth: kid.width
  property alias kidItem: kid
  Item { id: kid; objectName: "kid"; width: 10 }
}
"#,
    );
    let kid = h.find(root, "kid");
    assert_eq!(h.num(root, "kidWidth"), 10.0);
    h.set(root, "kidWidth", Value::Int(33));
    assert_eq!(h.num(kid, "width"), 33.0);
    assert_eq!(h.get(root, "kidItem"), Value::Object(kid));
}

#[test]
fn enums_and_object_literals() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  enum Size { Small, Big = 5 }
  property int s: Main.Big
  property var v: ({ a: 1, b: "x" })
  property var lit: { "k": 2 }
  property int fromJs: v.a + lit.k
}
"#,
    );
    assert_eq!(h.num(root, "s"), 5.0);
    assert_eq!(h.num(root, "fromJs"), 3.0);
}

#[test]
fn a_type_error_in_a_binding_is_logged_with_its_location() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property Item none
  property int w: none.width
}
"#,
    );
    assert_eq!(h.num(root, "w"), 0.0);
    let w = h.warnings();
    assert!(
        w.iter().any(|m| m.contains("file:///Main.qml:5")
            && m.contains("TypeError: Cannot read property 'width' of null")),
        "{w:?}"
    );
}

#[test]
fn script_functions_are_callable_from_js_and_rust() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int a: 4
  function twice(x) { return x * 2 }
  property int b: twice(a)
}
"#,
    );
    assert_eq!(h.num(root, "b"), 8.0);
    let r = h
        .engine
        .invoke_by_name(root, "twice", &[Value::Int(21)])
        .unwrap();
    assert_eq!(r.to_number(), 42.0);
}

#[test]
fn signals_with_arguments_reach_handlers() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  signal ping(int n, string s)
  onPing: (n, s) => console.log("ping", n, s)
  property int last
  Item { id: kid; signal pong(int n) }
  Component.onCompleted: { kid.pong.connect(v => last = v); kid.pong(9) }
}
"#,
    );
    h.engine
        .emit_by_name(root, "ping", &[Value::Int(3), Value::from("x")])
        .unwrap();
    assert_eq!(h.log(), vec!["ping 3 x"]);
    assert_eq!(h.num(root, "last"), 9.0);
}

#[test]
fn console_messages_go_to_the_log_sink_with_category() {
    let h = engine();
    h.load(
        r#"
import QtQuick
Item {
  Component.onCompleted: {
    console.log("a", 1)
    console.warn("w")
    console.error("e")
    console.info("i")
  }
}
"#,
    );
    let entries = h.log_entries();
    let got: Vec<(String, String)> = entries
        .iter()
        .map(|e| (format!("{:?}", e.level), e.message.clone()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("Debug".into(), "a 1".into()),
            ("Warning".into(), "w".into()),
            ("Critical".into(), "e".into()),
            ("Info".into(), "i".into()),
        ]
    );
    assert!(entries.iter().all(|e| e.category == "qml"));
}

#[test]
fn globals_qt_helpers_and_string_arg() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property color c: Qt.rgba(1, 0, 0, 1)
  property string s: "%1 of %2".arg(3).arg("five")
  property string t: qsTr("Hello")
  property real px: Qt.point(3, 4).y
}
"#,
    );
    assert_eq!(h.get(root, "c").to_js_string().as_str(), "#ff0000");
    assert_eq!(h.get(root, "s").to_js_string().as_str(), "3 of five");
    assert_eq!(h.get(root, "t").to_js_string().as_str(), "Hello");
    assert_eq!(h.num(root, "px"), 4.0);
}

#[test]
fn list_properties_take_objects_in_order() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property list<Item> things: [ Item { objectName: "one" }, Item { objectName: "two" } ]
  property int n: things.length
  property string second: things[1].objectName
}
"#,
    );
    assert_eq!(h.num(root, "n"), 2.0);
    assert_eq!(h.get(root, "second").to_js_string().as_str(), "two");
    // Not visual children: only `data` routes items to `children`.
    assert!(h.engine.visual_children(root).is_empty());
}

#[test]
fn a_destroyed_object_leaves_the_native_lists_of_its_parent() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  id: root
  property list<QtObject> picked: [visual, plain]
  property int kids: children.length
  Item { id: visual; objectName: "visual" }
  Item { objectName: "stays" }
  QtObject { id: plain; objectName: "plain" }
}
"#,
    );
    let (visual, stays, plain) = (
        h.find(root, "visual"),
        h.find(root, "stays"),
        h.find(root, "plain"),
    );
    let raw = |name: &str| match h.get(root, name) {
        Value::List(l) => (*l).clone(),
        other => panic!("{name} is {other:?}"),
    };
    assert_eq!(h.num(root, "kids"), 2.0);
    h.engine.destroy_now(visual);
    assert_eq!(raw("children"), vec![Value::Object(stays)]);
    assert_eq!(
        raw("data"),
        vec![Value::Object(stays), Value::Object(plain)]
    );
    // A binding on the list hears of it.
    assert_eq!(h.num(root, "kids"), 1.0);
    h.engine.destroy_now(plain);
    assert_eq!(raw("resources"), vec![]);
    assert_eq!(raw("data"), vec![Value::Object(stays)]);
    // A list the author declared keeps its length, as in Qt.
    assert_eq!(raw("picked").len(), 2);
}

#[test]
fn non_items_in_the_default_property_become_resources() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  Item { objectName: "visual" }
  QtObject { objectName: "plain" }
  property int kids: children.length
  property int res: resources.length
  property int all: data.length
}
"#,
    );
    assert_eq!(h.num(root, "kids"), 1.0);
    assert_eq!(h.num(root, "res"), 1.0);
    assert_eq!(h.num(root, "all"), 2.0);
}

#[test]
fn a_change_handler_sees_the_new_value_after_a_js_assignment() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int a: 1
  onAChanged: console.log("a is", a)
  function set(v) { a = v }
}
"#,
    );
    h.engine
        .invoke_by_name(root, "set", &[Value::Int(4)])
        .unwrap();
    h.engine
        .invoke_by_name(root, "set", &[Value::Int(4)])
        .unwrap();
    assert_eq!(h.log(), vec!["a is 4"]);
}

#[test]
fn translated_strings_re_evaluate_on_a_language_change() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property string plain: qsTr("Open")
  property string bound: qsTr("Close") + "!"
}
"#,
    );
    assert_eq!(h.get(root, "plain").to_js_string().as_str(), "Open");
    assert_eq!(h.get(root, "bound").to_js_string().as_str(), "Close!");
    // No translator: retranslating keeps the source text.
    h.engine.retranslate();
    assert_eq!(h.get(root, "bound").to_js_string().as_str(), "Close!");
}

#[test]
fn eval_js_sees_globals() {
    let h = engine();
    h.load("import QtQuick\nItem {}\n");
    let v = h
        .engine
        .eval_js("Qt.rgba(0, 1, 0, 1) + ' ' + '%1-%2'.arg('a').arg('b')")
        .unwrap();
    assert_eq!(v.to_js_string().as_str(), "#00ff00 a-b");
}

#[test]
fn value_type_members_are_assigned_and_bound() {
    let h = engine();
    let root = h.load(
        r#"
        import QtQuick
        Item {
            property font f
            property point p
            property int n: 2
            f.pixelSize: 24
            f.bold: true
            p.x: n * 3
            p.y: 4
        }
        "#,
    );
    let Value::Font(f) = h.get(root, "f") else {
        panic!("font: {:?}", h.get(root, "f"))
    };
    assert_eq!((f.pixel_size, f.bold()), (24, true));
    assert_eq!(h.get(root, "p"), Value::point(6.0, 4.0));
    h.set(root, "n", Value::Int(5));
    assert_eq!(h.get(root, "p"), Value::point(15.0, 4.0));
    assert!(h.warnings().is_empty());
}

#[test]
fn qt_namespace_enum_keys_read_the_same_as_literals_and_in_javascript() {
    let h = engine();
    let root = h.load(
        r#"
        import QtQuick
        Item {
            property bool flip: true
            property int literal: Qt.RightToLeft
            property int computed: flip ? Qt.RightToLeft : Qt.LeftToRight
            property int align: Qt.AlignHCenter
            property int flags: Qt.AlignRight | Qt.AlignVCenter
            property int button: Qt.BackButton
        }
        "#,
    );
    assert_eq!(h.get(root, "literal"), Value::Int(1));
    assert_eq!(h.get(root, "computed"), Value::Int(1));
    h.set(root, "flip", Value::Bool(false));
    assert_eq!(h.get(root, "computed"), Value::Int(0));
    assert_eq!(h.get(root, "align"), Value::Int(4));
    assert_eq!(h.get(root, "flags"), Value::Int(0x82));
    assert_eq!(h.get(root, "button"), Value::Int(8));
    assert!(h.warnings().is_empty());
}

#[test]
fn a_script_s_plain_write_goes_straight_to_the_property() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int n: 1
  property real r
  property bool b
  property string s: "a"
  property int bound: n + 1
  readonly property int fixed: 3
  property int changes: 0
  onNChanged: changes++
  function plain() { n = 2; r = 2.5; b = true; s = "b"; bound = 7; n = 2 }
  function others() { s = ""; n = 4.5; try { fixed = 4 } catch (e) {} }
}
"#,
    );
    let e = &h.engine;
    let before = e.plain_script_writes();
    e.invoke_by_name(root, "plain", &[]).unwrap();
    assert_eq!(e.plain_script_writes() - before, 6);
    assert_eq!(h.num(root, "n"), 2.0);
    assert_eq!(h.num(root, "r"), 2.5);
    assert_eq!(h.get(root, "b"), Value::Bool(true));
    assert_eq!(h.get(root, "s"), Value::from("b"));
    // The write ends the binding, and the same value written again tells
    // nobody.
    h.set(root, "n", Value::Int(2));
    assert_eq!(h.num(root, "bound"), 7.0);
    assert_eq!(h.num(root, "changes"), 1.0);

    // A text that may stand for none, a number an int has to round and a
    // read-only property take the long way.
    let before = e.plain_script_writes();
    e.invoke_by_name(root, "others", &[]).unwrap();
    assert_eq!(e.plain_script_writes() - before, 1);
    assert_eq!(h.get(root, "s"), Value::from(""));
    assert_eq!(h.num(root, "n"), 4.0);
    assert_eq!(h.num(root, "fixed"), 3.0);
}

#[test]
fn a_script_s_read_of_an_object_goes_the_quick_way() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  id: root
  objectName: "root"
  width: 3
  property Item other: kid
  property real seen
  Item { id: kid; width: 4 }
  function read() { seen = kid.parent.width * 10 + root.other.width }
}
"#,
    );
    let e = &h.engine;
    let before = e.quick_script_reads();
    e.invoke_by_name(root, "read", &[]).unwrap();
    // `other` holds the object; `parent` is the item's to work out, so it
    // takes the long way.
    assert_eq!(e.quick_script_reads() - before, 1);
    assert_eq!(h.num(root, "seen"), 34.0);
}

#[test]
fn which_properties_an_author_gave_is_told_in_one_answer() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int given: 1
  property int bound: given + 1
  property int unset
  property alias kidWidth: kid.width
  property alias kidHeight: kid.height
  property int later
  Item { id: kid; width: 10 }
}
"#,
    );
    h.set(root, "later", Value::Int(2));
    let e = &h.engine;
    let props: Vec<_> = ["given", "bound", "unset", "kidWidth", "kidHeight", "later"]
        .iter()
        .map(|n| e.property_index(root, n).unwrap())
        .collect();
    // An alias answers for its target; a bit not asked for, or past the
    // end of the properties, is not set.
    assert_eq!(e.authored(root, &props, u128::MAX), 0b101011);
    assert_eq!(e.authored(root, &props, 0b000110), 0b000010);
    assert_eq!(e.authored(root, &props[..1], 0b11), 0b1);
}
