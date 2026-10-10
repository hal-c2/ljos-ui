//! Member-level error recovery: what survives a typo, and where the
//! diagnostics point. See the crate docs for the contract.

use qml_syntax::{
    Allocator,
    ast::{QmlBindingValue, QmlMember, QmlObject},
};

/// The tree as flat lines, nested objects indented by two spaces per level:
/// `binding width`, `property count`, `object Text`, ...
fn outline_of(object: &QmlObject<'_>, depth: usize, out: &mut Vec<String>) {
    let pad = " ".repeat(depth * 2);
    for member in &object.members {
        match member {
            QmlMember::Object(inner) => {
                out.push(format!("{pad}object {}", inner.type_name));
                outline_of(inner, depth + 1, out);
            }
            QmlMember::Binding(binding) => {
                out.push(format!("{pad}binding {}", binding.name));
                match &binding.value {
                    QmlBindingValue::Object(inner) => {
                        out.push(format!("{pad}  object {}", inner.type_name));
                        outline_of(inner, depth + 2, out);
                    }
                    QmlBindingValue::Objects(list) => {
                        for inner in list {
                            out.push(format!("{pad}  object {}", inner.type_name));
                            outline_of(inner, depth + 2, out);
                        }
                    }
                    _ => {}
                }
            }
            QmlMember::Property(property) => {
                out.push(format!("{pad}property {}", property.name.name))
            }
            QmlMember::Signal(signal) => out.push(format!("{pad}signal {}", signal.name.name)),
            QmlMember::Function(function) => out.push(format!(
                "{pad}function {}",
                function.id.as_ref().map_or("", |id| id.name.as_str())
            )),
            QmlMember::InlineComponent(inline) => {
                out.push(format!("{pad}component {}", inline.name.name));
                outline_of(&inline.object, depth + 1, out);
            }
            QmlMember::Enum(declaration) => {
                out.push(format!("{pad}enum {}", declaration.name.name))
            }
        }
    }
}

struct Recovered {
    root: String,
    imports: Vec<String>,
    pragmas: Vec<String>,
    outline: Vec<String>,
    /// `(line, column)` of each diagnostic, in the order reported.
    positions: Vec<(usize, usize)>,
    messages: Vec<String>,
    panicked: bool,
}

fn recover(source: &str) -> Recovered {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    let mut outline = Vec::new();
    outline_of(&parsed.document.root, 0, &mut outline);
    Recovered {
        root: parsed.document.root.type_name.to_string(),
        imports: parsed
            .document
            .imports
            .iter()
            .map(|import| qml_syntax::text(source, import.span).to_string())
            .collect(),
        pragmas: parsed
            .document
            .pragmas
            .iter()
            .map(|p| p.name.to_string())
            .collect(),
        outline,
        positions: parsed
            .diagnostics
            .iter()
            .map(|d| d.position(source))
            .collect(),
        messages: parsed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect(),
        panicked: parsed.panicked,
    }
}

fn strs(lines: &[&str]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

#[test]
fn a_broken_binding_loses_only_itself() {
    let source = "Item {\n    a: 1\n    b: : 2\n    c: 3\n}\n";
    let r = recover(source);
    assert_eq!(r.outline, strs(&["binding a", "binding c"]));
    assert_eq!(r.positions, vec![(3, 8)]);
    assert!(!r.panicked);
}

#[test]
fn a_broken_expression_stops_at_the_next_line_without_a_semicolon() {
    let source = "Item {\n    a: (1 +\n    b: 2\n    c: 3 +\n    d: 4\n}\n";
    let r = recover(source);
    // `a` hides `b` inside its open parenthesis (the error is reported where
    // the parser noticed it), but `c` and `d` are on lines of their own.
    assert!(
        r.outline.contains(&"binding d".to_string()),
        "{:?}",
        r.outline
    );
    assert!(!r.outline.contains(&"binding a".to_string()));
    assert!(!r.panicked);
    assert!(!r.positions.is_empty());
}

#[test]
fn a_semicolon_ends_a_broken_member_on_the_same_line() {
    let r = recover("Item { a: 1; b: ); c: 3; d: 4 }");
    assert_eq!(r.outline, strs(&["binding a", "binding c", "binding d"]));
    assert_eq!(r.positions, vec![(1, 17)]);
}

#[test]
fn a_broken_nested_object_is_contained() {
    let source = "\
Item {
    id: root
    Rectangle {
        width: 1
        height: +
        color: \"red\"
        Text { text: \"inner\" }
    }
    Text { text: \"after\"; x: ) ; y: 2 }
    Item
    z: 3
}
";
    let r = recover(source);
    // The error in `height` stays inside Rectangle, which keeps the rest.
    assert_eq!(
        r.outline,
        strs(&[
            "binding id",
            "object Rectangle",
            "  binding width",
            "  binding color",
            "  object Text",
            "    binding text",
            "object Text",
            "  binding text",
            "  binding y",
            // `Item` without a brace is dropped; `z` survives.
            "binding z",
        ])
    );
    let lines: Vec<_> = r.positions.iter().map(|p| p.0).collect();
    // `+` takes `color` as its operand; the parser notices at the colon.
    assert_eq!(lines, vec![6, 9, 11]);
}

#[test]
fn an_unbalanced_brace_in_a_handler_block_is_contained() {
    let source = "\
Item {
    a: 1
    onClicked: { foo( }
    b: 2
    onPressed: {
        if (x) {
            bar()
        }
    }
    c: 3
}
";
    let r = recover(source);
    assert_eq!(
        r.outline,
        strs(&["binding a", "binding b", "binding onPressed", "binding c"])
    );
    assert_eq!(r.positions.len(), 1);
    assert_eq!(r.positions[0].0, 3);
}

#[test]
fn a_handler_that_never_closes_gives_up_its_brackets_at_a_sibling() {
    let source = "\
Item {
    a: 1
    onClicked: {
        if (x) {
            foo()
    b: 2
    c: 3
}
";
    let r = recover(source);
    // Indentation tells the skip that `b` is a sibling of `onClicked`, not
    // its body, so the open braces do not eat the file.
    assert_eq!(r.outline, strs(&["binding a", "binding b", "binding c"]));
    assert_eq!(r.positions.len(), 1);
}

#[test]
fn template_literals_in_a_skipped_member_do_not_end_it() {
    let source = "\
Item {
    a: `x${ foo( }y`
    b: 1
}
";
    let r = recover(source);
    assert_eq!(r.outline, strs(&["binding b"]));
}

#[test]
fn a_bad_property_declaration_is_dropped() {
    let source = "\
Item {
    property int a: 1
    property int : 5
    property: 7
    required property string c
    property var d: [1, 2
    default property list<Item> e
    readonly property int f: 3
    property alias g: h.i
}
";
    let r = recover(source);
    assert_eq!(
        r.outline,
        strs(&[
            "property a",
            "binding property",
            "property c",
            "property e",
            "property f",
            "property g"
        ])
    );
    let lines: Vec<_> = r.positions.iter().map(|p| p.0).collect();
    assert_eq!(lines, vec![3, 7]);
}

#[test]
fn signals_functions_enums_and_inline_components_recover() {
    let source = "\
Item {
    signal ok(int a)
    signal bad(int
    function f() { return 1 }
    function g( { }
    enum E { A, B = x }
    enum F { A, B }
    component C: Rectangle { width: 1; height: ) }
    component D: +
    after: 1
}
";
    let r = recover(source);
    assert_eq!(
        r.outline,
        strs(&[
            "signal ok",
            "function f",
            "enum F",
            "component C",
            "  binding width",
            "binding after",
        ])
    );
    assert_eq!(r.positions.len(), 5);
}

#[test]
fn a_broken_import_loses_only_itself() {
    let source = "\
import QtQuick 2.15
import QtQuick.
import QtQuick.Controls as
import \"lib.js\" as Lib
pragma Singleton
Item { a: 1 }
";
    let r = recover(source);
    assert_eq!(
        r.imports,
        vec!["import QtQuick 2.15", "import \"lib.js\" as Lib"]
    );
    assert_eq!(r.pragmas, vec!["Singleton"]);
    assert_eq!(r.root, "Item");
    assert_eq!(r.outline, strs(&["binding a"]));
    let lines: Vec<_> = r.positions.iter().map(|p| p.0).collect();
    assert_eq!(lines, vec![3, 4]);
    assert!(!r.panicked);
}

#[test]
fn a_root_without_a_brace_keeps_its_members() {
    let r = recover("import QtQuick\nItem\n    a: 1\n    b: 2\n}\n");
    assert_eq!(r.root, "Item");
    assert_eq!(r.outline, strs(&["binding a", "binding b"]));
    assert_eq!(r.positions, vec![(3, 5)]);
}

#[test]
fn several_errors_in_one_file() {
    let source = "\
import QtQuick
import QtQuick.
Item {
    a: 1
    b: +
    Rectangle { c: 1; d: ); e: 3 }
    onX: { foo( }
    property int : 1
    f: 6
    g: = 3
    h: 7
}
";
    let r = recover(source);
    assert_eq!(r.imports, vec!["import QtQuick"]);
    assert_eq!(
        r.outline,
        strs(&[
            "binding a",
            "object Rectangle",
            "  binding c",
            "  binding e",
            "binding f",
            "binding h",
        ])
    );
    let lines: Vec<_> = r.positions.iter().map(|p| p.0).collect();
    assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sorted: {lines:?}");
    assert!(lines.len() >= 6, "{lines:?}");
    assert!(!r.panicked);
}

#[test]
fn an_error_at_the_end_of_the_file_keeps_everything_before_it() {
    let r = recover("Item {\n    a: 1\n    Rectangle {\n        b: 2\n        c: ");
    assert_eq!(
        r.outline,
        strs(&["binding a", "object Rectangle", "  binding b"])
    );
    // `c:` has no value; the two unclosed objects are one report, because
    // the message and the place are the same.
    assert_eq!(r.positions.len(), 2, "{:?}", r.messages);
    assert!(r.positions.iter().all(|p| p.0 == 5));
    assert!(!r.panicked);

    let r = recover("Item {\n    a: 1\n");
    assert_eq!(r.outline, strs(&["binding a"]));
    assert_eq!(r.positions.len(), 1);
}

#[test]
fn an_unterminated_string_costs_the_rest_of_the_file_but_not_what_came_before() {
    // Strings may span lines in QML mode, so there is no later line to resume at.
    let r = recover("Item {\n    a: 1\n    b: \"open\n    c: 3\n}\n");
    assert_eq!(r.outline, strs(&["binding a"]));
    assert!(r.positions.iter().all(|p| p.0 >= 3), "{:?}", r.positions);
    assert!(!r.panicked);
}

#[test]
fn a_second_root_object_is_reported_and_the_first_kept() {
    let r = recover("Item { a: 1 }\nItem { b: 2 }\n");
    assert_eq!(r.outline, strs(&["binding a"]));
    assert_eq!(r.positions, vec![(2, 1)]);
    assert!(!r.panicked);
}

#[test]
fn without_a_root_type_the_document_panics_but_keeps_the_header() {
    let r = recover("import QtQuick\n}\n");
    assert_eq!(r.imports, vec!["import QtQuick"]);
    assert!(r.panicked);
    assert!(r.outline.is_empty());
    assert_eq!(r.root, "");
}

#[test]
fn valid_files_are_untouched() {
    let r = recover("import QtQuick\nItem { a: 1; Rectangle { b: 2 } }\n");
    assert!(r.positions.is_empty());
    assert_eq!(
        r.outline,
        strs(&["binding a", "object Rectangle", "  binding b"])
    );
}
