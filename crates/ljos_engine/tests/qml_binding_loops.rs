//! Binding loops and their absence, in lazy and in eager mode. Messages,
//! reported properties and final values are what Qt 6.11.2 (`qml6`, run as
//! a black box) gives for the same source.

mod support;

use ljos_engine::Value;
use support::{Harness, engine};

fn harness(eager: bool) -> Harness {
    let h = engine();
    h.engine.set_eager_bindings(eager);
    h
}

/// Every order of `items`.
fn permutations(items: &[&str]) -> Vec<Vec<String>> {
    if items.len() <= 1 {
        return vec![items.iter().map(|s| s.to_string()).collect()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, first.to_string());
            out.push(tail);
        }
    }
    out
}

fn loop_messages(h: &Harness) -> Vec<String> {
    h.warnings()
        .into_iter()
        .filter(|m| m.contains("Binding loop"))
        .collect()
}

#[test]
fn an_acyclic_chain_has_no_warnings_in_any_declaration_order() {
    let decls = [
        "property int p: 1",
        "property int q: p + 1",
        "property int r: q + 1",
        "property int s: r + q",
    ];
    for eager in [false, true] {
        for order in permutations(&decls) {
            let h = harness(eager);
            let src = format!("import QtQuick\nItem {{\n{}\n}}\n", order.join("\n"));
            let root = h.load(&src);
            // Eager mode has evaluated everything by now; lazy mode has
            // evaluated nothing, so the reads below are what could loop.
            let created = h.warnings();
            assert!(created.is_empty(), "eager {eager}\n{src}\n{created:?}");
            let got = ["p", "q", "r", "s"].map(|n| h.num(root, n));
            assert_eq!(got, [1.0, 2.0, 3.0, 5.0], "eager {eager}\n{src}");
            let w = h.warnings();
            assert!(w.is_empty(), "eager {eager}\n{src}\n{w:?}");
        }
    }
}

#[test]
fn a_chain_through_native_properties_and_other_objects_has_no_warnings() {
    let decls = [
        "width: base * 2",
        "height: width + inner.depth",
        "property int base: 10",
        "property int total: height + inner.width",
        "Item { id: inner; property int depth: root.base + 1; width: root.width / 4 }",
    ];
    for eager in [false, true] {
        for order in permutations(&decls) {
            let h = harness(eager);
            let src = format!(
                "import QtQuick\nItem {{\nid: root\n{}\n}}\n",
                order.join("\n")
            );
            let root = h.load(&src);
            assert_eq!(h.num(root, "total"), 36.0, "eager {eager}\n{src}");
            assert_eq!(h.num(root, "height"), 31.0, "eager {eager}\n{src}");
            h.set(root, "base", Value::Int(20));
            assert_eq!(h.num(root, "total"), 71.0, "eager {eager}\n{src}");
            let w = h.warnings();
            assert!(w.is_empty(), "eager {eager}\n{src}\n{w:?}");
        }
    }
}

#[test]
fn a_binding_that_changes_under_its_readers_is_no_loop() {
    // Each write re-evaluates the chain; nothing reads its own result.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load(
            "import QtQuick\nItem { property int p: 1; property int q: p + 1; property int r: q + 1 }\n",
        );
        for v in 2..6 {
            h.set(root, "p", Value::Int(v));
            assert_eq!(h.num(root, "r"), f64::from(v) + 2.0);
        }
        let w = h.warnings();
        assert!(w.is_empty(), "eager {eager}: {w:?}");
    }
}

const PAIR: &str = r#"
import QtQuick
Item {
  property int p: q + 1
  property int q: p + 1
}
"#;

#[test]
fn a_two_property_loop_is_reported_once_for_the_first_property() {
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load(PAIR);
        // Qt: p 2, q 3.
        assert_eq!(h.num(root, "p"), 2.0, "eager {eager}");
        assert_eq!(h.num(root, "q"), 3.0, "eager {eager}");
        // Reading again does not move the values or warn again.
        assert_eq!(h.num(root, "p"), 2.0);
        assert_eq!(h.num(root, "q"), 3.0);
        let w = h.warnings();
        assert_eq!(
            w,
            vec![
                "file:///Main.qml:3:1: QML Main: Binding loop detected for property \"p\":\n\
                 file:///Main.qml:4:3"
            ],
            "eager {eager}"
        );
    }
}

#[test]
fn eager_mode_reports_a_loop_nobody_reads() {
    let h = harness(true);
    h.load(PAIR);
    assert_eq!(loop_messages(&h).len(), 1);
    // Lazy mode evaluates on demand, so it has nothing to report yet.
    let h = harness(false);
    h.load(PAIR);
    assert!(h.warnings().is_empty());
}

#[test]
fn declaring_the_other_property_first_reports_that_one() {
    // Qt: warns for q; p 3, q 2.
    let h = harness(true);
    let root =
        h.load("import QtQuick\nItem {\n  property int q: p + 1\n  property int p: q + 1\n}\n");
    assert_eq!((h.num(root, "p"), h.num(root, "q")), (3.0, 2.0));
    let w = loop_messages(&h);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("property \"q\""), "{w:?}");
}

#[test]
fn a_three_property_loop_settles_like_qt() {
    for eager in [false, true] {
        // Qt: warns for a; a 3, b 5, c 4.
        let h = harness(eager);
        let root = h.load(
            "import QtQuick\nItem {\n  property int a: b + 1\n  property int b: c + 1\n  property int c: a + 1\n}\n",
        );
        let got = ["a", "b", "c"].map(|n| h.num(root, n));
        assert_eq!(got, [3.0, 5.0, 4.0], "eager {eager}");
        let w = loop_messages(&h);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("property \"a\""), "{w:?}");
        // Qt: warns for a; a 3, b 4, c 5.
        let h = harness(eager);
        let root = h.load(
            "import QtQuick\nItem {\n  property int a: c + 1\n  property int b: a + 1\n  property int c: b + 1\n}\n",
        );
        let got = ["a", "b", "c"].map(|n| h.num(root, n));
        assert_eq!(got, [3.0, 4.0, 5.0], "eager {eager}");
        let w = loop_messages(&h);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("property \"a\""), "{w:?}");
    }
}

#[test]
fn a_loop_whose_values_do_not_change_is_silent() {
    // Qt logs nothing for these.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load(
            "import QtQuick\nItem {\n  property int p: q * 0\n  property int q: p * 0\n  property int s: s * 1\n}\n",
        );
        let got = ["p", "q", "s"].map(|n| h.num(root, n));
        assert_eq!(got, [0.0, 0.0, 0.0]);
        let w = h.warnings();
        assert!(w.is_empty(), "eager {eager}: {w:?}");
    }
}

#[test]
fn a_property_bound_to_itself_is_reported() {
    // Qt: warns for s and leaves 1.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load("import QtQuick\nItem {\n  property int s: s + 1\n}\n");
        assert_eq!(h.num(root, "s"), 1.0);
        assert_eq!(h.num(root, "s"), 1.0);
        let w = loop_messages(&h);
        assert_eq!(w.len(), 1, "eager {eager}: {w:?}");
        assert!(w[0].contains("property \"s\""), "{w:?}");
    }
}

#[test]
fn every_change_that_goes_round_the_loop_is_reported_again() {
    // Qt: p 1, q 2; x1 = 5 gives p 7, q 8; x1 = 7 gives p 15, q 16; one
    // warning for p each time.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load(
            "import QtQuick\nItem {\n  property int x1: 0\n  property int p: q + x1\n  property int q: p + 1\n}\n",
        );
        // The log is drained by each check: one new warning per step.
        let one_for_p = |step: &str| {
            let w = h.warnings();
            assert_eq!(w.len(), 1, "eager {eager}, {step}: {w:?}");
            assert!(w[0].contains("detected for property \"p\""), "{w:?}");
        };
        assert_eq!((h.num(root, "p"), h.num(root, "q")), (1.0, 2.0));
        one_for_p("creation");
        h.set(root, "x1", Value::Int(5));
        assert_eq!((h.num(root, "p"), h.num(root, "q")), (7.0, 8.0));
        one_for_p("x1 = 5");
        h.set(root, "x1", Value::Int(7));
        assert_eq!((h.num(root, "p"), h.num(root, "q")), (15.0, 16.0));
        one_for_p("x1 = 7");
    }
}

#[test]
fn a_loop_on_native_properties_settles_too() {
    // Qt: warns for width; width 2, height 3.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load("import QtQuick\nItem {\n  width: height + 1\n  height: width + 1\n}\n");
        assert_eq!((h.num(root, "width"), h.num(root, "height")), (2.0, 3.0));
        let w = loop_messages(&h);
        assert_eq!(w.len(), 1, "eager {eager}: {w:?}");
        assert!(w[0].contains("property \"width\""), "{w:?}");
    }
}

#[test]
fn a_loop_closed_by_qt_binding_reports_the_new_binding() {
    // Qt: after the first assignment p 1, q 0; the second warns for q and
    // leaves p 3, q 2.
    for eager in [false, true] {
        let h = harness(eager);
        let root = h.load(
            r#"
import QtQuick
Item {
  property int p: 0
  property int q: 0
  function first() { p = Qt.binding(() => q + 1) }
  function second() { q = Qt.binding(() => p + 1) }
}
"#,
        );
        h.engine.invoke_by_name(root, "first", &[]).unwrap();
        assert_eq!((h.num(root, "p"), h.num(root, "q")), (1.0, 0.0));
        assert!(h.warnings().is_empty());
        h.engine.invoke_by_name(root, "second", &[]).unwrap();
        assert_eq!((h.num(root, "q"), h.num(root, "p")), (2.0, 3.0));
        let w = loop_messages(&h);
        assert_eq!(w.len(), 1, "eager {eager}: {w:?}");
        assert!(w[0].contains("property \"q\""), "{w:?}");
    }
}

/// The loop messages of `main`, loaded eagerly with `files` beside it.
fn messages_of(main: &str, files: &[(&str, &str)]) -> Vec<String> {
    let h = harness(true);
    let mut all = vec![("Main", main)];
    all.extend_from_slice(files);
    h.files(&all);
    h.engine
        .create_from_url("Main", None, Vec::new())
        .expect("Main.qml creates");
    loop_messages(&h)
}

const BASE: &str =
    "import QtQuick\nItem {\n    property int m:   k + 1\n    property int k: m + 1\n}\n";
const PLAIN: &str = "import QtQuick\nItem {\n    property int n: 1\n    property int o: n\n}\n";

// The places below are the ones Qt 6.11.2 prints for the same files.

#[test]
fn a_loop_on_a_qt_bindable_property_is_one_line_at_the_property_name() {
    assert_eq!(
        messages_of(
            "import QtQuick\nItem {\n     width:   height + 1\n  height: width + 1\n}\n",
            &[]
        ),
        ["file:///Main.qml:3:6: QML Main: Binding loop detected for property \"width\""]
    );
    assert_eq!(
        messages_of(
            "import QtQuick\nItem {\n     objectName: tag + \"x\"\n    \
             property string tag: objectName + \"y\"\n}\n",
            &[]
        ),
        ["file:///Main.qml:3:6: QML Main: Binding loop detected for property \"objectName\""]
    );
}

#[test]
fn a_loop_on_a_group_member_names_the_group_type_and_the_first_name() {
    for main in [
        "import QtQuick\nRectangle {\n    radius: border.width + 1\n      \
         border.width: radius + 1\n}\n",
        "import QtQuick\nRectangle {\n    radius: border.width + 1\n      \
         border {\n        width: radius + 1 }\n}\n",
    ] {
        let w = messages_of(main, &[]);
        assert_eq!(w.len(), 1, "{w:?}");
        let inner = main.contains("border {");
        let place = if inner { "5:9" } else { "4:7" };
        assert_eq!(
            w[0],
            format!(
                "file:///Main.qml:2:1: QML Main (parent or ancestor of QQuickPen): \
                 Binding loop detected for property \"width\":\nfile:///Main.qml:{place}"
            )
        );
    }
}

#[test]
fn a_loop_on_a_native_property_that_is_not_bindable_in_qt_has_two_lines() {
    assert_eq!(
        messages_of(
            "import QtQuick\nRectangle {\n   radius: extra + 1\n  \
             property real extra: radius + 1\n}\n",
            &[]
        ),
        [
            "file:///Main.qml:2:1: QML Main: Binding loop detected for property \"radius\":\n\
             file:///Main.qml:3:4"
        ]
    );
}

#[test]
fn a_loop_written_in_a_component_file_is_placed_in_that_file() {
    assert_eq!(
        messages_of(
            "import QtQuick\nItem {\n    Base {}\n}\n",
            &[("Base", BASE)]
        ),
        [
            "file:///Main.qml:3:5: QML Base: Binding loop detected for property \"m\":\n\
             file:///Base.qml:3:5"
        ]
    );
}

#[test]
fn a_loop_written_where_a_component_is_used_is_placed_there() {
    assert_eq!(
        messages_of(
            "import QtQuick\nItem {\n    Plain {\n          n: o + 1\n        o: n + 1\n    }\n}\n",
            &[("Plain", PLAIN)]
        ),
        [
            "file:///Main.qml:3:5: QML Plain: Binding loop detected for property \"n\":\n\
             file:///Main.qml:4:11"
        ]
    );
}

#[test]
fn a_binding_that_replaces_the_one_of_the_component_gives_its_own_place() {
    assert_eq!(
        messages_of(
            "import QtQuick\nItem {\n    Plain {\n       n: o + 1\n    }\n}\n",
            &[("Plain", PLAIN)]
        ),
        [
            "file:///Main.qml:3:5: QML Plain: Binding loop detected for property \"n\":\n\
             file:///Main.qml:4:8"
        ]
    );
}

/// Loads `main` eagerly with `files` beside it: the harness, the root and
/// what creating it logged.
fn created(main: &str, files: &[(&str, &str)]) -> (Harness, ljos_engine::ObjectId, Vec<String>) {
    let h = harness(true);
    let mut all = vec![("Main", main)];
    all.extend_from_slice(files);
    h.files(&all);
    let root = h
        .engine
        .create_from_url("Main", None, Vec::new())
        .expect("Main.qml creates");
    let log = h.log();
    (h, root, log)
}

// The order bindings first run in decides which member of a loop is
// reported and what each is left with, so eager mode follows Qt's.

#[test]
fn creation_runs_bindings_from_the_last_set_up_to_the_first() {
    let (_h, _root, log) = created(
        r#"import QtQuick
Item {
    id: root
    function l(s, v) { console.log(s); return v }
    property int a: l("a", 1)
    Item {
        visible: root.l("child.visible", true)
    }
    property int b: l("b", 1)
    Rectangle {
        border.width: root.l("rect.border.width", 1)
        radius: root.l("rect.radius", 1)
        visible: root.l("rect.visible", true)
        x: root.l("rect.x", 1)
        y: root.l("rect.y", 1)
    }
}
"#,
        &[],
    );
    assert_eq!(
        log,
        [
            "rect.border.width",
            "rect.radius",
            "rect.visible",
            "child.visible",
            "a",
            "b",
            // Qt's bindable properties come after everything else, in
            // the order they were set up.
            "rect.y",
            "rect.x",
        ]
    );
}

#[test]
fn a_binding_that_read_a_bindable_property_runs_again_once_that_is_bound() {
    let (h, root, log) = created(
        r#"import QtQuick
Item {
    function l(s, v) { console.log(s); return v }
    property int first: l("first", width + 1)
    width: l("width", 5)
    property int last: l("last", width + 1)
}
"#,
        &[],
    );
    assert_eq!(log[..3], ["first", "last", "width"], "{log:?}");
    let mut again = log[3..].to_vec();
    again.sort();
    assert_eq!(again, ["first", "last"], "{log:?}");
    assert_eq!(
        (
            h.num(root, "first"),
            h.num(root, "width"),
            h.num(root, "last")
        ),
        (6.0, 5.0, 6.0)
    );
}

#[test]
fn a_binding_the_user_of_a_component_replaces_never_runs() {
    let (h, root, log) = created(
        r#"import QtQuick
Item {
    Logs {
        objectName: "u"
        n: l("the user's", 2)
        width: l("the user's width", 2)
    }
}
"#,
        &[(
            "Logs",
            r#"import QtQuick
Item {
    function l(s, v) { console.log(s); return v }
    property int n: l("the component's", 1)
    width: l("the component's width", 1)
}
"#,
        )],
    );
    assert_eq!(log, ["the user's", "the user's width"]);
    let u = h.find(root, "u");
    assert_eq!((h.num(u, "n"), h.num(u, "width")), (2.0, 2.0));
}

#[test]
fn a_loop_over_bindable_properties_reports_the_last_one_set_up() {
    let (h, root, log) = created(
        r#"import QtQuick
Item {
    Item { id: a; objectName: "a"; width: c.width + 1 }
    Item { id: b; objectName: "b"; width: a.width + 1 }
    Item { id: c; objectName: "c"; width: b.width + 1 }
}
"#,
        &[],
    );
    assert_eq!(
        log,
        ["file:///Main.qml:5:36: QML Item: Binding loop detected for property \"width\""]
    );
    let w = ["a", "b", "c"].map(|n| h.num(h.find(root, n), "width"));
    assert_eq!(w, [4.0, 5.0, 3.0]);
}

#[test]
fn a_loop_over_both_kinds_reports_the_bindable_property_that_closes_it() {
    let (h, root, log) = created(
        r#"import QtQuick
Item {
    Item { id: a; objectName: "a"; width: n + 1; property int n: b.height + 1 }
    Item { id: b; objectName: "b"; height: a.width + 1; property int k: a.width + height }
}
"#,
        &[],
    );
    assert_eq!(
        log,
        ["file:///Main.qml:4:36: QML Item: Binding loop detected for property \"height\""]
    );
    let (a, b) = (h.find(root, "a"), h.find(root, "b"));
    assert_eq!(
        [
            h.num(a, "width"),
            h.num(a, "n"),
            h.num(b, "height"),
            h.num(b, "k")
        ],
        [5.0, 4.0, 3.0, 8.0]
    );
}

#[test]
fn a_loop_across_objects_reports_the_later_object() {
    let (h, root, log) = created(
        r#"import QtQuick
Item {
    Item { id: a; objectName: "a"; property int n: b.n + 1 }
    Item { id: b; objectName: "b"; property int n: a.n + 1 }
}
"#,
        &[],
    );
    assert_eq!(
        log,
        [
            "file:///Main.qml:4:5: QML Item: Binding loop detected for property \"n\":\n\
          file:///Main.qml:4:36"
        ]
    );
    let (a, b) = (h.find(root, "a"), h.find(root, "b"));
    assert_eq!((h.num(a, "n"), h.num(b, "n")), (3.0, 2.0));
}

#[test]
fn a_component_made_in_slices_runs_its_bindings_in_the_same_order() {
    let h = harness(true);
    let root = h.load(
        r#"import QtQuick
Item {
    id: root
    function l(s, v) { console.log(s); return v }
    property Component c: Component {
        Item {
            objectName: "made"
            property int first: root.l("first", width + 1)
            width: root.l("width", b.width + 1)
            Item { id: b; width: root.l("b.width", parent.width + 1) }
            Item {}
            Item {}
        }
    }
}
"#,
    );
    let comp = h.get(root, "c").as_object().unwrap();
    assert!(h.log().is_empty());
    let (mut job, handle) = ljos_engine::CreationJob::for_component(
        &h.engine,
        comp,
        Some(root),
        vec![],
        ljos_runtime::Lane::Visible,
        ljos_runtime::ProcId::new(0, 0),
    );
    let mut steps = 0;
    while matches!(job.step(&|| true), ljos_runtime::JobPoll::Yield) {
        steps += 1;
        assert!(steps < 100);
        assert!(h.log().is_empty(), "nothing runs before it is whole");
    }
    assert!(steps > 1, "it was made in slices");
    let made = handle.result().unwrap().unwrap();
    // qml6 for the same component made with createObject().
    assert_eq!(
        h.log(),
        [
            "first",
            "width",
            "first",
            "b.width",
            "width",
            "file:///Main.qml:10:27: QML Item: Binding loop detected for property \"width\"",
            "first",
        ]
    );
    assert_eq!((h.num(made, "width"), h.num(made, "first")), (3.0, 4.0));
}
