//! What a file compiles to, as a reviewed snapshot: the IR, then the module.

mod oracle;

use ljos_compiler::{Project, compile_file, dump};
use oracle::Runtime;

/// Compiles the first of `files` in a project of all of them.
fn compile(files: &[(&str, &str)]) -> String {
    let mut project = Project::new(Runtime::new());
    for (key, source) in files {
        project.add(key, *source);
    }
    match compile_file(&project, files[0].0) {
        Ok(compiled) => {
            let mut out = dump(&compiled.file);
            if !compiled.diagnostics.is_empty() {
                out.push_str("diagnostics\n");
                for diagnostic in &compiled.diagnostics {
                    out.push_str(&format!("  {diagnostic}\n"));
                }
            }
            out
        }
        Err(diagnostics) => diagnostics
            .iter()
            .map(|d| format!("failed: {d}\n"))
            .collect(),
    }
}

#[test]
fn literals() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Rectangle {
    objectName: "main"
    width: 100
    height: -2.5
    opacity: (0.5)
    visible: true
    color: "#ff0000"
    state: `idle`
    property var nothing: null
    property var missing: undefined
    property var list: [1, "two", false]
    property var record: ({ name: "x", size: 3, nested: [ { a: 1 } ] })
    property color tint: "steelblue"
    property int sum: 1 + 2
    property var shorthand: ({ width })
}
"##,
    )]));
}

#[test]
fn ids_parent_scope_and_root() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    property int size: 10
    width: size * 2
    Rectangle {
        id: box
        width: parent.width / 2
        height: width + root.size
        x: label.width
        Text { id: label; text: box.width + "px"; x: size }
        Rectangle { opacity: x > 0 ? 1 : 0.5 }
        Rectangle { }
    }
}
"##,
    )]));
}

#[test]
fn attached() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

ListView {
    id: view
    Component.onCompleted: console.log("ready", view.currentIndex)
    Component.onDestruction: { currentIndex = -1 }
    Keys.onPressed: (event) => { if (event.key) event.accepted = true }
    Keys.onReleased: event.accepted = false
    Keys.enabled: true
    delegate: Rectangle {
        width: ListView.isCurrentItem ? 10 : 5
        height: ListView.view.width
    }
}
"##,
    )]));
}

#[test]
fn attached_in_expressions() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick
import QtQuick as Q

Item {
    id: root
    Item { id: other }
    property bool on: Keys.enabled && other.Keys.enabled
    property bool qualified: Q.Keys.enabled
    property int both: Keys.priority + Keys.BeforeItem
    property var plain: ({ Keys: 1 })
    property int untouched: plain?.Keys + Text.AlignLeft
    function set(v) { Keys.enabled = v; parent.Keys.enabled = v; other.children[0].Keys.priority++ }
}
"##,
    )]));
}

#[test]
fn assignments_to_names_from_outside() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

ListView {
    id: view
    property int created: 0
    property int seen: 0
    delegate: Rectangle {
        property int own: 0
        Component.onCompleted: { created++; own = created; cost = 2 }
        width: created + seen
    }
}
"##,
    )]));
}

#[test]
fn grouped() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    Rectangle {
        anchors.left: parent.left
        anchors.margins: 4
        border.color: "black"
        border { width: 2 }
    }
    Text {
        anchors { left: root.left; right: root.right }
        font.pixelSize: 12 * 2
        font.bold: true
        color: "#808080"
    }
}
"##,
    )]));
}

#[test]
fn handlers_with_params() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    signal picked(int index, string name)
    property int count: 0
    onPicked: (i, n) => console.log(i, n)
    onCountChanged: console.log(count)
    onWidthChanged: { count++ }
    MouseArea {
        onClicked: root.count = mouse.x
        onPressed: function(m) { root.picked(m.x, "pressed") }
        onDoubleClicked: console.log(arguments.length)
    }
    Text {
        onLinkActivated: Qt.openUrlExternally(link)
    }
    Connections {
        target: root
        function onPicked(index, name) { console.log(index, name) }
        onCountChanged: console.log(root.count)
    }
}
"##,
    )]));
}

#[test]
fn inline_components() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    component Badge: Rectangle {
        id: badge
        property string label
        property bool big: false
        signal tapped()
        width: big ? 40 : 20
        Text { text: badge.label }
    }
    Badge { label: "one" }
    Badge { label: "two"; big: true }
}
"##,
    )]));
}

#[test]
fn property_alias() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    property alias label: caption.text
    property alias captionItem: caption
    readonly property alias size: caption.font.pixelSize
    default property alias content: holder.data
    required property string name
    required width
    Text { id: caption; text: name }
    Item { id: holder }
}
"##,
    )]));
}

#[test]
fn enums() {
    insta::assert_snapshot!(compile(&[
        (
            "Main",
            r##"
import QtQuick

Item {
    enum Mode { Off, On = 5, Auto }
    property int mode: Main.On
    property int other: Main.Mode.Auto
    Text {
        horizontalAlignment: Text.AlignHCenter
        property int scoped: Text.HAlignment.AlignRight
        property int qt: Qt.AlignLeft
        property int picked: parent.width > 10 ? Text.AlignLeft : Text.AlignRight
        property int theirs: Panel.Wide
    }
}
"##,
        ),
        (
            "Panel",
            r##"
import QtQuick
Item {
    enum Layout { Narrow, Wide }
}
"##,
        ),
    ]));
}

#[test]
fn translations() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    property int count: 2
    Text { text: qsTr("Hello") }
    Text { text: qsTr("Open", "menu entry") }
    Text { text: qsTr("%n file(s)", "", count) }
    Text { text: qsTr("%n item(s)", "", 3) }
    Text { text: qsTranslate("Dialog", "Cancel") }
    Text { text: qsTrId("app-title") }
    Text { text: QT_TR_NOOP("Later") }
    Text { text: "Total: " + qsTr("Sum") }
}
"##,
    )]));
}

#[test]
fn js_functions() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    property int total: add(1, 2)
    function add(a: int, b: int): int {
        const sum = a + b
        return sum + width
    }
    function reset() {
        for (let i = 0; i < 3; i++) root.x = i
        total = 0
    }
}
"##,
    )]));
}

// A name nothing in sight has is read through `__ljos_named`, which throws
// where the running program has none either; `typeof` asks without
// throwing, and what `new` makes one of keeps its parentheses.
#[test]
fn names_nothing_declares() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    property string kind: typeof nowhere
    property var made: new Maker.Kind(1)
    property var both: ({ short })
    function later() {
        absent()
        other.x = 1
        written = 2
        return written
    }
}
"##,
    )]));
}

#[test]
fn delegates_and_singletons() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick
import Styles as S

ListView {
    id: view
    model: 3
    delegate: Rectangle {
        required property int index
        color: S.Theme.accent
        width: view.width
        height: modelData + index
        Text { text: name }
    }
    Timer { onTriggered: mystery.go() }
}
"##,
    )]));
}

/// A write to a member of a value type goes back to where the value was
/// read from: a property, a member of another object, an element, or a
/// variable the script can assign to.
#[test]
fn writes_to_members_of_values() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    property rect area
    property var list: []
    Item { id: other; property point spot }
    signal moved(point to)
    onMoved: to.x = 1
    function change(given) {
        area.x = 1
        root.area.width += 2
        other.spot.y++
        list[0].x = 3
        other.Layout.row = 4
        var kept = area
        kept.x = 5
        given.x = 6
        const fixed = area
        fixed.x = 7
        this.area.height = 8
    }
}
"##
    )]));
}

#[test]
fn a_file_with_errors_compiles_in_part() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    width: 10
    height: (
    Rectangle { color: "red" }
    Unheard { }
}
"##,
    )]));
}

/// A key declared in another file has its number there, also when the file
/// that names it is built on the one that declares it.
#[test]
fn enums_of_other_files_have_numbers() {
    insta::assert_snapshot!(compile(&[
        (
            "Main",
            r##"
import QtQuick

Item {
    property int a: Base.Large
    property int b: Child.Base.Large
    property int c: Child.Medium
    property int d: Child.Small
}
"##,
        ),
        (
            "Base",
            "import QtQuick\nItem { enum Base { Small = 3, Medium, Large = 10 } }\n",
        ),
        ("Child", "import QtQuick\nBase { enum Mine { Small } }\n"),
    ]));
}

/// A string written for a property that holds an enum is the key it names;
/// what a script gives is for the runtime to check.
#[test]
fn plain_values_for_enum_properties() {
    insta::assert_snapshot!(compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    transformOrigin: "Center"
    Text { horizontalAlignment: "AlignRight"; transformOrigin: "TopLeft" }
    MouseArea { acceptedButtons: "RightButton" }
    MouseArea { acceptedButtons: "LeftButton | MiddleButton" }
    Item { transformOrigin: +1 }
    Item { transformOrigin: (1) }
    Item { transformOrigin: Text.AlignRight }
    Item { transformOrigin: `Center` }
}
"##,
    )]));
}

/// Qt 6.11.2 does not load a file with any of these: "Invalid property
/// assignment: unknown enumeration", at the value.
#[test]
fn a_plain_value_that_is_no_key_is_an_error() {
    for (object, column) in [
        ("Item { transformOrigin: \"Nowhere\" }", 29),
        ("Item { transformOrigin: \"center\" }", 29),
        ("Item { transformOrigin: \"Item.Center\" }", 29),
        ("Item { transformOrigin: \"AlignRight\" }", 29),
        ("Item { transformOrigin: \"\" }", 29),
        ("Item { transformOrigin: \"TopLeft|Nowhere\" }", 29),
        // One key is what the property holds; only flags take several.
        ("Item { transformOrigin: \"TopLeft|Center\" }", 29),
        ("Text { horizontalAlignment: \"AlignRight|AlignLeft\" }", 33),
        (
            "Text { horizontalAlignment: \"AlignRight | AlignLeft\" }",
            33,
        ),
        ("Item { transformOrigin: 4 }", 29),
        ("Item { transformOrigin: -1 }", 29),
        ("Item { transformOrigin: 2.5 }", 29),
        ("Item { transformOrigin: true }", 29),
        ("Item { transformOrigin: null }", 29),
        ("Text { horizontalAlignment: 2 }", 33),
        ("MouseArea { acceptedButtons: 1 }", 34),
        ("MouseArea { acceptedButtons: \"LeftButton|\" }", 34),
        ("MouseArea { acceptedButtons: \"AlignLeft\" }", 34),
    ] {
        let source = format!("import QtQuick\nItem {{\n    {object}\n}}\n");
        let out = compile(&[("Main", &source)]);
        assert!(
            out.contains(&format!(
                "3:{column}: error: Invalid property assignment: unknown enumeration"
            )),
            "{object}\n{out}"
        );
    }
}

/// What a PropertyChanges is given for its target is checked when the state
/// is entered, and a property a file declares holds what its type says.
#[test]
fn numbers_that_are_not_for_an_enum_property_are_taken() {
    let out = compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    id: root
    property int transformOrigins: 3
    property var origin: 2
    states: State {
        PropertyChanges { target: root; transformOrigin: 1 }
    }
}
"##,
    )]);
    assert!(!out.contains("diagnostics"), "{out}");
}

#[test]
fn a_binding_knows_where_its_name_is_written() {
    // Qt's binding-loop message points at these places (Qt 6.11.2, by
    // running qml6).
    let source = "import QtQuick\n\
Rectangle {\n\
\x20   id: root\n\
\x20      width:   height + 1\n\
\x20   border.width:  root.width\n\
\x20   border {\n\
\x20       color: root.color\n\
\x20   }\n\
\x20     readonly property int p: width + 1\n\
}\n";
    let mut project = Project::new(Runtime::new());
    project.add("Main", source);
    let compiled = compile_file(&project, "Main").expect("compiles");
    assert!(
        compiled.diagnostics.is_empty(),
        "{:?}",
        compiled.diagnostics
    );
    let root = &compiled.file.components[0].objects[0];
    let place = |members: &ljos_ir::Members, name: &str| {
        let b = members
            .bindings
            .iter()
            .find(|b| b.name == name)
            .unwrap_or_else(|| panic!("no binding {name}"));
        let s = b.name_span;
        (s.line, s.column, &source[s.start as usize..s.end as usize])
    };
    assert_eq!(place(&root.members, "width"), (4, 8, "width"));
    // A value given in a declaration is placed at the declaration.
    let (line, column, text) = place(&root.members, "p");
    assert_eq!((line, column), (9, 7));
    assert!(text.starts_with("readonly property int p"), "{text:?}");
    let border = &root
        .grouped
        .iter()
        .find(|g| g.path == ["border"])
        .expect("border")
        .members;
    // Dotted, the first part; in braces, the name itself.
    assert_eq!(place(border, "width"), (5, 5, "border.width"));
    assert_eq!(place(border, "color"), (7, 9, "color"));
}

/// A method that needs nothing of a function of its own is the exported
/// function itself: a call makes no closure and applies nothing. One that
/// reads `arguments`, names itself, or has a default or a rest parameter
/// keeps its own function inside the export.
#[test]
fn a_plain_method_makes_no_function_per_call() {
    let out = compile(&[(
        "Main",
        r##"
import QtQuick

Item {
    function plain(a, b) { return a + b + width }
    function counted() { return arguments.length }
    function again(n) { return n > 0 ? again(n - 1) : 0 }
    function given(a = 1) { return a }
    function rest(...a) { return a.length }
}
"##,
    )]);
    assert_eq!(out.matches(".apply(this, $args)").count(), 4, "{out}");
    assert!(out.contains("export function f0($scope, a, b) {"), "{out}");
}
