//! The QML mode of the parser: the tree it builds and what it rejects.
//! Ported from qml-solid's `tests/parse.rs`.

use qml_syntax::{
    Allocator, GetSpan, Span,
    ast::{QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject},
    js::{Expression, Statement},
};

fn parse<'a>(allocator: &'a Allocator, source: &'a str) -> QmlDocument<'a> {
    let parsed = qml_syntax::parse(allocator, source);
    let messages: Vec<_> = parsed
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect();
    assert!(messages.is_empty(), "{messages:?}");
    assert!(!parsed.panicked);
    parsed.document
}

fn diagnostics(source: &str) -> Vec<String> {
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    parsed.diagnostics.into_iter().map(|d| d.message).collect()
}

fn text(source: &str, span: Span) -> &str {
    qml_syntax::text(source, span)
}

/// The members of `object` as `kind name` lines.
fn members(object: &QmlObject<'_>) -> Vec<String> {
    object
        .members
        .iter()
        .map(|member| match member {
            QmlMember::Object(object) => format!("object {}", object.type_name),
            QmlMember::Binding(binding) => format!("binding {}", binding.name),
            QmlMember::Property(property) => format!("property {}", property.name.name),
            QmlMember::Signal(signal) => format!("signal {}", signal.name.name),
            QmlMember::Function(function) => {
                format!(
                    "function {}",
                    function.id.as_ref().map_or("", |id| id.name.as_str())
                )
            }
            QmlMember::InlineComponent(inline) => format!("component {}", inline.name.name),
            QmlMember::Enum(declaration) => {
                let keys: Vec<_> = declaration
                    .members
                    .iter()
                    .map(|member| format!("{}={}", member.name.name, member.value))
                    .collect();
                format!("enum {} {}", declaration.name.name, keys.join(" "))
            }
        })
        .collect()
}

#[test]
fn header() {
    let allocator = Allocator::default();
    let document = parse(
        &allocator,
        "pragma Singleton\npragma ComponentBehavior: Bound\nimport QtQuick 2.15\nimport QtQuick.Controls as Controls\nimport \"../parts\"\nItem {}\n",
    );
    let pragmas: Vec<_> = document.pragmas.iter().map(|p| (p.name, p.value)).collect();
    assert_eq!(
        pragmas,
        [("Singleton", None), ("ComponentBehavior", Some("Bound"))]
    );

    let imports: Vec<_> = document
        .imports
        .iter()
        .map(|import| {
            let source = match &import.source {
                QmlImportSource::Module(name) => name.to_string(),
                QmlImportSource::Path(path) => format!("path {path}"),
            };
            (
                source,
                import.version,
                import.alias.as_ref().map(|alias| alias.name.as_str()),
            )
        })
        .collect();
    assert_eq!(
        imports,
        [
            ("QtQuick".to_string(), Some("2.15"), None),
            ("QtQuick.Controls".to_string(), None, Some("Controls")),
            ("path ../parts".to_string(), None, None),
        ]
    );
    assert_eq!(document.root.type_name.as_simple(), Some("Item"));
}

#[test]
fn members_of_an_object() {
    let allocator = Allocator::default();
    let source = r#"Item {
    id: root
    property int count: 0
    readonly property string label: "n"
    required property var entry
    default property alias content: body.data
    property list<Item> rows
    signal picked(int index, name: string)
    function bump(by) { count += by }
    component Badge: Text { color: "red" }
    anchors.fill: parent
    font { bold: true }
    Text { text: label }
    Behavior on width { }
}"#;
    let document = parse(&allocator, source);
    assert_eq!(
        members(&document.root),
        [
            "binding id",
            "property count",
            "property label",
            "property entry",
            "property content",
            "property rows",
            "signal picked",
            "function bump",
            "component Badge",
            "binding anchors.fill",
            "object font",
            "object Text",
            "object Behavior",
        ]
    );

    let property = |name: &str| {
        document
            .root
            .members
            .iter()
            .find_map(|member| match member {
                QmlMember::Property(property) if property.name.name == name => Some(property),
                _ => None,
            })
            .unwrap()
    };
    assert!(property("label").is_readonly);
    assert!(property("entry").is_required && property("entry").value.is_none());
    assert!(property("content").is_default);
    let rows = property("rows").type_name.as_ref().unwrap();
    assert!(rows.is_list && rows.name.as_simple() == Some("Item"));

    let QmlMember::Signal(signal) = &document.root.members[6] else {
        panic!()
    };
    let parameters: Vec<_> = signal.params.iter().map(|p| p.name.name.as_str()).collect();
    assert_eq!(parameters, ["index", "name"]);

    let QmlMember::Object(behavior) = &document.root.members[12] else {
        panic!()
    };
    assert_eq!(behavior.on.as_ref().unwrap().to_string(), "width");
}

#[test]
fn an_enum_counts_from_where_it_is_told() {
    let allocator = Allocator::default();
    let document = parse(
        &allocator,
        "Item {\n    enum Theme { Light, Dark }\n    enum Flag {\n        None = -1,\n        Bold = 0x10,\n        Italic\n    }\n    property int enum: 1\n}\n",
    );
    assert_eq!(
        members(&document.root),
        [
            "enum Theme Light=0 Dark=1",
            "enum Flag None=-1 Bold=16 Italic=17",
            "property enum"
        ]
    );
    assert_eq!(
        diagnostics("Item { enum Theme { Light = 1.5 } }"),
        ["An enum's value is a whole number"]
    );
}

#[test]
fn binding_values() {
    let allocator = Allocator::default();
    let source = r#"Item {
    width: parent.width - 2 * margin
    text: `${count} items`
    cast: (entry as Entry).name
    onPressed: count += 1
    onReleased: { count = 0; done() }
    onMoved: if (active) move()
    onPicked: (index) => select(index)
    delegate: Text { text: modelData }
    states: [ State { name: "a" }, State { name: "b" } ]
    model: [1, 2, 3]
    range: ({ from: 0, to: 9 })
    limits: { "from": 0, to: 9 }
    onMoved: { count = 0 }
    height: { width }
    onChosen: { if (active) { move() } }
    onLeft: { while (busy) { step() } }
    tools: { pick(item) { return item } }
}"#;
    let document = parse(&allocator, source);
    let value = |index: usize| {
        let QmlMember::Binding(binding) = &document.root.members[index] else {
            panic!()
        };
        &binding.value
    };

    // Values are Oxc's own nodes, with spans into the QML file.
    let QmlBindingValue::Expression(width) = value(0) else {
        panic!()
    };
    assert!(matches!(width, Expression::BinaryExpression(_)));
    assert_eq!(text(source, width.span()), "parent.width - 2 * margin");
    let Expression::BinaryExpression(binary) = width else {
        panic!()
    };
    assert_eq!(text(source, binary.left.span()), "parent.width");

    assert!(matches!(
        value(1),
        QmlBindingValue::Expression(Expression::TemplateLiteral(_))
    ));
    assert!(matches!(
        value(2),
        QmlBindingValue::Expression(Expression::StaticMemberExpression(_))
    ));
    assert!(matches!(
        value(3),
        QmlBindingValue::Expression(Expression::AssignmentExpression(_))
    ));
    assert!(matches!(
        value(4),
        QmlBindingValue::Statement(Statement::BlockStatement(_))
    ));
    assert!(matches!(
        value(5),
        QmlBindingValue::Statement(Statement::IfStatement(_))
    ));
    assert!(matches!(
        value(6),
        QmlBindingValue::Expression(Expression::ArrowFunctionExpression(_))
    ));
    let QmlBindingValue::Object(delegate) = value(7) else {
        panic!()
    };
    assert_eq!(members(delegate), ["binding text"]);
    let QmlBindingValue::Objects(states) = value(8) else {
        panic!()
    };
    assert_eq!(states.len(), 2);
    assert!(matches!(
        value(9),
        QmlBindingValue::Expression(Expression::ArrayExpression(_))
    ));
    assert!(matches!(
        value(10),
        QmlBindingValue::Expression(Expression::ParenthesizedExpression(_))
    ));
    // Braces are an object when what is in them could be nothing else.
    assert!(matches!(
        value(11),
        QmlBindingValue::Expression(Expression::ObjectExpression(_))
    ));
    assert!(matches!(
        value(12),
        QmlBindingValue::Statement(Statement::BlockStatement(_))
    ));
    assert!(matches!(
        value(13),
        QmlBindingValue::Statement(Statement::BlockStatement(_))
    ));
    // A statement written as a call and a block is no method of that name.
    assert!(matches!(
        value(14),
        QmlBindingValue::Statement(Statement::BlockStatement(_))
    ));
    assert!(matches!(
        value(15),
        QmlBindingValue::Statement(Statement::BlockStatement(_))
    ));
    assert!(matches!(
        value(16),
        QmlBindingValue::Expression(Expression::ObjectExpression(_))
    ));
}

#[test]
fn separators_and_comments() {
    let allocator = Allocator::default();
    let source =
        "Item { width: 1; height: 2 // trailing\n /* block */ Text { text: \"a\"; color: \"b\" } }";
    let document = parse(&allocator, source);
    assert_eq!(
        members(&document.root),
        ["binding width", "binding height", "object Text"]
    );
    assert_eq!(text(source, document.root.span), source);
}

#[test]
fn syntax_errors() {
    assert!(!diagnostics("Item { width: }").is_empty());
    assert!(!diagnostics("Item { width: 1").is_empty());
    assert!(!diagnostics("import\nItem {}").is_empty());
    assert!(!diagnostics("Item {} Item {}").is_empty());
    assert!(!diagnostics("").is_empty());
}

#[test]
fn strings_may_span_lines() {
    let allocator = Allocator::default();
    let source = "Item { text: \"one\ntwo\"; other: 'a\\tb\nc' }";
    let document = parse(&allocator, source);
    let value = |index: usize| {
        let QmlMember::Binding(binding) = &document.root.members[index] else {
            panic!()
        };
        let QmlBindingValue::Expression(Expression::StringLiteral(string)) = &binding.value else {
            panic!()
        };
        string.value.as_str()
    };
    assert_eq!(value(0), "one\ntwo");
    assert_eq!(value(1), "a\tb\nc");
}

#[test]
fn diagnostics_point_into_the_source() {
    let source = "Item {\n    width: \n}";
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    assert!(!parsed.is_ok());
    let first = &parsed.diagnostics[0];
    assert_eq!(first.position(source).0, 3);
    assert_eq!(qml_syntax::position("ab\ncd", 4), (2, 2));
    // An offset inside a multi-byte character does not split it.
    assert_eq!(qml_syntax::position("é", 1), (1, 1));
}
