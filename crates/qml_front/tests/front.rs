//! What a project's files mean: which type a name is, what an object of it
//! has, and what a free name in a script is bound to.

mod oracle;

use oracle::Runtime;
use qml_front::{
    Error,
    names::{Global, Names, Target},
    project::{Project, Source, join, relative},
    scope::{Declared, Member, Own, Tree, classify, handled},
    types::{Kind, Member as TypeMember, Origin, Types},
};
use qml_syntax::Allocator;

fn project(files: &[(&str, &str)]) -> Project {
    let mut project = Project::new();
    for (file, source) in files {
        project
            .add(file, source)
            .unwrap_or_else(|errors| panic!("{file}: {errors:?}"));
    }
    project
}

/// Reads `name` of the project made of `files` and hands its tree to `check`.
fn analyzed<R>(
    files: &[(&str, &str)],
    name: &str,
    check: impl FnOnce(&Tree<'_, usize>, Types<'_, Runtime>, &[Error]) -> R,
) -> R {
    let project = project(files);
    let runtime = Runtime::new();
    let types = Types::new(&project, &runtime, name);
    let source = files
        .iter()
        .find(|(file, _)| *file == name)
        .expect("a file of the project")
        .1;
    let allocator = Allocator::default();
    let parsed = qml_syntax::parse(&allocator, source);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let mut errors = Vec::new();
    let tree = Tree::analyze(&parsed.document, types, &mut errors);
    check(&tree, types, &errors)
}

/// The object's id, or the type it is written as.
fn label(tree: &Tree<'_, usize>, object: usize) -> String {
    let object = &tree.objects[object];
    object
        .id
        .map_or_else(|| object.name.clone(), str::to_string)
}

/// What each `(name, written)` is: the free name `name` at the first place
/// `written` is in the file `Sample`, said in a few words.
fn lookups(source: &str, queries: &[(&str, &str)]) -> Vec<String> {
    analyzed(&[("Sample", source)], "Sample", |tree, types, _| {
        let names = Names { tree, types };
        queries
            .iter()
            .map(|(name, written)| {
                let offset = source
                    .find(written)
                    .unwrap_or_else(|| panic!("`{written}` is not in the file"))
                    as u32;
                let Some(resolution) = names.lookup(name, offset) else {
                    return "undefined".to_string();
                };
                let mut said = String::new();
                if let Some(owner) = resolution.aimed {
                    said.push_str(&format!("target of {}? ", label(tree, owner)));
                }
                for context in &resolution.roles {
                    said.push_str(&format!(
                        "role of {}? ",
                        label(tree, tree.contexts[*context].root)
                    ));
                }
                said.push_str(&match resolution.target {
                    None => "nothing".to_string(),
                    Some(Target::Id(object)) => format!("id {}", label(tree, object)),
                    Some(Target::Property { object, name }) => {
                        format!("{}.{name}", label(tree, object))
                    }
                    Some(Target::Context { context, name }) => {
                        format!(
                            "given {name} of {}",
                            label(tree, tree.contexts[context].root)
                        )
                    }
                    Some(Target::Type(found)) => format!("type {:?}", found.origin),
                    Some(Target::Namespace(source)) => format!("namespace {source:?}"),
                    Some(Target::Global(Global::Qml)) => "QML global".to_string(),
                    Some(Target::Global(Global::Js)) => "JS global".to_string(),
                });
                said
            })
            .collect()
    })
}

#[test]
fn a_type_is_found_where_qml_looks_for_it() {
    let mut project = project(&[
        (
            "app/Main",
            r#"import QtQuick
import QtQuick as Q
import "widgets"
Item {
    component Badge: Rectangle {}
}"#,
        ),
        ("app/Button", "import QtQuick\nItem {}"),
        ("app/Label", "import QtQuick\nItem {}"),
        ("app/widgets/Button", "import QtQuick\nItem {}"),
        ("app/Screen.ui", "import QtQuick\nItem {}"),
        ("lib/Knob", "import QtQuick\nItem {}"),
    ]);
    project.add_type("Controls", "Main", "app/Main");
    project.add_type("Controls", "Knob", "lib/Knob");
    let runtime = Runtime::new();
    let types = Types::new(&project, &runtime, "app/Main");
    let origin = |name: &str| {
        types
            .find(&name.split('.').collect::<Vec<_>>())
            .map(|found| found.origin)
    };

    // An inline component first.
    assert_eq!(origin("Badge"), Some(Origin::Inline));
    // Then the imports, from the last: the directory the file imports
    // before the directory it is in.
    assert_eq!(
        origin("Button"),
        Some(Origin::File("app/widgets/Button".to_string()))
    );
    assert_eq!(
        origin("Rectangle"),
        Some(Origin::Module("QtQuick".to_string()))
    );
    // What QtQuick comes with.
    assert_eq!(origin("Timer"), Some(Origin::Module("QtQuick".to_string())));
    assert_eq!(origin("Q.Text"), Some(Origin::Namespace));
    // Then its own directory, where Qt Design Studio's files count too.
    assert_eq!(origin("Label"), Some(Origin::File("app/Label".to_string())));
    assert_eq!(
        origin("Screen"),
        Some(Origin::File("app/Screen.ui".to_string()))
    );
    // Then the modules the file is a type of, wherever their files are.
    assert_eq!(origin("Knob"), Some(Origin::File("lib/Knob".to_string())));
    assert_eq!(origin("Nothing"), None);
    assert_eq!(origin("Q.Button"), None);
    assert_eq!(
        types.namespace("Q"),
        Some(&Source::Module("QtQuick".to_string()))
    );
}

#[test]
fn a_component_has_what_the_type_of_its_root_has() {
    let project = project(&[
        (
            "Card",
            r#"import QtQuick
Rectangle {
    required property string title
    property url icon
    signal picked(int index, string why)
    function open() {}
    Text { id: label }
    property alias caption: label.text
    enum Theme { Light, Dark }
}"#,
        ),
        (
            "Panel",
            "import QtQuick\nCard { required property int depth }",
        ),
        ("Loop", "import QtQuick\nLoop {}"),
    ]);
    let runtime = Runtime::new();
    let types = Types::new(&project, &runtime, "Panel");
    let panel = types.find(&["Panel"]).unwrap().kind;
    let card = types.find(&["Card"]).unwrap().kind;
    let member = |kind: &Kind<usize>, name: &str| types.member(kind, name);

    // Its own, and its root type's, however many components down.
    assert!(
        matches!(member(&panel, "title"), Some(TypeMember::Property(property)) if property.is_text)
    );
    assert!(
        matches!(member(&panel, "icon"), Some(TypeMember::Property(property)) if property.is_url)
    );
    assert!(matches!(
        member(&panel, "color"),
        Some(TypeMember::Property(_))
    ));
    assert!(matches!(
        member(&panel, "width"),
        Some(TypeMember::Property(_))
    ));
    assert_eq!(member(&panel, "picked"), Some(TypeMember::Signal));
    assert_eq!(member(&panel, "titleChanged"), Some(TypeMember::Signal));
    assert_eq!(member(&panel, "open"), Some(TypeMember::Method));
    assert_eq!(member(&panel, "forceActiveFocus"), Some(TypeMember::Method));
    assert_eq!(member(&panel, "nothing"), None);
    assert_eq!(
        types.signal(&panel, "picked"),
        Some(vec!["index".to_string(), "why".to_string()])
    );
    assert_eq!(
        types.base(&panel),
        types
            .find(&["Rectangle"])
            .and_then(|found| types.base(&found.kind))
    );
    assert_eq!(types.required(&panel), ["depth", "title"]);

    // An alias is the property it names: `label.text` is text.
    assert!(
        matches!(member(&card, "caption"), Some(TypeMember::Property(property)) if property.is_text)
    );
    // A path given to a property a component declares is taken from its file.
    assert_eq!(types.home(&panel, "icon"), Some("Card".to_string()));
    assert_eq!(types.home(&panel, "color"), None);

    // Its enums, and the ones of its root's type.
    assert_eq!(types.declared_enum(&panel, "Theme"), Some(true));
    assert_eq!(types.declared_enum(&panel, "Dark"), Some(false));
    assert!(types.is_key(&panel, Some("Theme"), "Dark"));
    assert!(types.is_key(&panel, None, "TopLeft"));
    assert!(!types.is_key(&panel, Some("Theme"), "TopLeft"));

    // A change signal of Qt's carries its value only where Qt says it does.
    let text = types.find(&["Text"]).unwrap().kind;
    assert_eq!(types.carried(&text, "textChanged"), Some("text"));
    assert_eq!(types.carried(&text, "widthChanged"), None);
    assert_eq!(types.carried(&card, "titleChanged"), None);

    // A component built on itself has nothing, and does not loop.
    let looped = Types::new(&project, &runtime, "Loop");
    assert_eq!(
        looped.member(&looped.find(&["Loop"]).unwrap().kind, "width"),
        None
    );
}

#[test]
fn a_singleton_is_one_object() {
    let project = project(&[
        (
            "Style",
            "pragma Singleton\nimport QtQuick\nQtObject { property color accent }",
        ),
        ("Main", "import QtQuick\nimport Styles\nItem {}"),
    ]);
    let runtime = Runtime::new();
    let types = Types::new(&project, &runtime, "Main");
    assert!(types.is_singleton(&types.find(&["Style"]).unwrap().kind));
    assert!(types.is_singleton(&types.find(&["Theme"]).unwrap().kind));
    assert!(!types.is_singleton(&types.find(&["Item"]).unwrap().kind));
    assert!(project.summary("Style").unwrap().is_singleton);
}

#[test]
fn a_name_is_an_id_before_it_is_a_property() {
    let source = r#"import QtQuick
Rectangle {
    id: root
    property int count: 0
    Text {
        id: label
        property int caption: 1
        text: count + width + caption + label + root + radius
    }
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                // What the root declares, and what its type has.
                ("count", "count +"),
                ("radius", "radius"),
                // The object the binding is written on comes before the root.
                ("width", "width +"),
                ("caption", "caption +"),
                // An id comes before any property.
                ("label", "label +"),
                ("root", "root +"),
                ("nothing", "count +"),
            ]
        ),
        [
            "root.count",
            "root.radius",
            "label.width",
            "label.caption",
            "id label",
            "id root",
            "undefined"
        ]
    );
}

#[test]
fn a_delegate_is_given_what_its_model_has() {
    let source = r#"import QtQuick
Rectangle {
    id: root
    property int count: 0
    Repeater {
        model: 3
        Rectangle {
            id: cell
            property int label: 0
            width: index + modelData + name + count + label + color
        }
    }
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                ("index", "index +"),
                ("modelData", "modelData +"),
                // A role, if the model has it: only the running model knows.
                ("name", "name +"),
                ("count", "count +"),
                ("label", "label +"),
                ("color", "color"),
            ]
        ),
        [
            "given index of cell",
            "given modelData of cell",
            "given name of cell",
            "role of cell? root.count",
            "cell.label",
            "cell.color",
        ]
    );
}

#[test]
fn a_bound_component_sees_ids_first_and_is_given_nothing() {
    let source = r#"pragma ComponentBehavior: Bound
import QtQuick
Rectangle {
    id: root
    property int count: 0
    Item { id: label }
    ListView {
        delegate: Rectangle {
            required property int index
            property int label: 0
            width: index + name + count + label
        }
    }
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                ("index", "index +"),
                ("name", "name +"),
                ("count", "count +"),
                ("label", "label\n")
            ]
        ),
        // The ids of the components around come before the delegate's own
        // properties, and what the model has is a required property.
        ["Rectangle.index", "undefined", "root.count", "id label"]
    );
    // Unbound, the delegate's own property comes before an id further out.
    let unbound = source.replace("pragma ComponentBehavior: Bound\n", "");
    assert_eq!(
        lookups(&unbound, &[("label", "label\n")]),
        ["Rectangle.label"]
    );
}

#[test]
fn an_inline_component_sees_nothing_around_it() {
    let source = r#"import QtQuick
Item {
    id: root
    property int count: 0
    component Badge: Rectangle {
        width: count + root + radius
    }
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                ("count", "count +"),
                ("root", "root +"),
                ("radius", "radius")
            ]
        ),
        ["undefined", "undefined", "Rectangle.radius"]
    );
}

#[test]
fn a_capitalised_name_is_a_type_a_namespace_or_a_global() {
    let source = r#"import QtQuick
import "lib.js" as Lib
Item {
    width: ListView.Horizontal + Lib.size + Math.max(1) + Qt.rgba(1) + qsTr("a") + undefined + Nothing
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                ("ListView", "ListView"),
                ("Lib", "Lib"),
                ("Math", "Math"),
                ("Qt", "Qt"),
                ("qsTr", "qsTr"),
                ("undefined", "undefined"),
                ("Nothing", "Nothing"),
            ]
        ),
        [
            "type Module(\"QtQuick\")",
            "namespace Path(\"lib.js\")",
            "JS global",
            "QML global",
            "QML global",
            "JS global",
            "undefined",
        ]
    );
}

#[test]
fn what_a_state_changes_is_read_on_its_target() {
    let source = r#"import QtQuick
Item {
    id: root
    property int count: 0
    Rectangle { id: box }
    ListView { id: view }
    states: [
        State {
            PropertyChanges { target: box; color: radius > count ? "red" : "blue" }
        },
        State {
            PropertyChanges { target: view.currentItem; visible: shown }
        }
    ]
}"#;
    assert_eq!(
        lookups(
            source,
            &[
                ("radius", "radius >"),
                ("count", "count ?"),
                ("shown", "shown")
            ]
        ),
        // `radius` is the box's, and so is anything else it has; a target
        // that is not named by its id is known only as the program runs.
        [
            "box.radius",
            "root.count",
            "target of PropertyChanges? nothing"
        ]
    );
}

#[test]
fn a_template_is_a_component_of_its_own() {
    analyzed(
        &[(
            "Sample",
            r#"import QtQuick
Item {
    property Component card: Rectangle {}
    Component { id: maker; Text {} }
    ListView { delegate: Item {} }
    Rectangle {}
}"#,
        )],
        "Sample",
        |tree, _, errors| {
            assert!(errors.is_empty(), "{errors:?}");
            let contexts: Vec<(String, bool)> = tree
                .contexts
                .iter()
                .map(|context| (tree.objects[context.root].name.clone(), context.is_delegate))
                .collect();
            assert_eq!(
                contexts,
                [
                    ("Item".to_string(), false),
                    ("Rectangle".to_string(), true),
                    ("Text".to_string(), true),
                    ("Item".to_string(), true),
                ]
            );
            assert!(
                tree.objects
                    .iter()
                    .any(|object| object.is_template && object.id == Some("maker"))
            );
            // The last Rectangle is an object of the file's component.
            assert_eq!(tree.objects.last().unwrap().context, 0);
        },
    );
}

#[test]
fn a_declaration_says_how_it_holds_its_value() {
    analyzed(
        &[(
            "Sample",
            r#"import QtQuick
Item {
    id: root
    property int a: 1
    readonly property int b: 2
    property alias k: inner.k
    property alias lost: nobody.k
    signal picked(int index)
    function bump() { inner.d = 3; e++ }
    Item {
        id: inner
        property int k: 1
        property int l: 1
        property int m: l * 2
        property int d: 0
        property var e: 0
        readonly property int f: 0
        property Item g: Item {}
    }
}"#,
        )],
        "Sample",
        |tree, _, errors| {
            let member =
                |object: usize, name: &str| tree.objects[object].declared[name].member().clone();
            // Whoever makes the component may set what its root declares.
            assert_eq!(member(0, "a"), Member::Binding);
            assert_eq!(member(0, "b"), Member::Const);
            assert_eq!(member(0, "picked"), Member::Function);
            assert_eq!(member(0, "bump"), Member::Function);
            let inner = tree
                .objects
                .iter()
                .position(|object| object.id == Some("inner"))
                .unwrap();
            assert_eq!(member(inner, "l"), Member::Const);
            assert_eq!(member(inner, "m"), Member::Binding);
            // What a script assigns to is state.
            assert_eq!(member(inner, "d"), Member::Signal);
            assert_eq!(member(inner, "e"), Member::Signal);
            assert_eq!(member(inner, "f"), Member::Const);
            assert_eq!(member(inner, "g"), Member::Const);
            // An alias makes what it names settable from outside.
            assert_eq!(member(inner, "k"), Member::Binding);
            assert_eq!(member(0, "k"), Member::Binding);
            assert!(
                matches!(tree.objects[0].declared["picked"], Declared::Signal(ref names) if names == &["index"])
            );
            let messages: Vec<&str> = errors.iter().map(|error| error.message.as_str()).collect();
            assert_eq!(messages, ["`nobody` is not an id in this component"]);
        },
    );
}

#[test]
fn what_is_wrong_is_said_where_it_is_written() {
    let source = "import QtQuick\nItem {\n    Item { id: twice }\n    Nothing { id: twice }\n}";
    analyzed(&[("Sample", source)], "Sample", |tree, _, errors| {
        let messages: Vec<(&str, &str)> = errors
            .iter()
            .map(|error| (error.message.as_str(), qml_syntax::text(source, error.span)))
            .collect();
        assert_eq!(
            messages[0],
            (
                "`Nothing` is not a type of anything the file imports",
                "Nothing"
            )
        );
        assert_eq!(messages[1].0, "id `twice` is not unique");
        // An object whose type is unknown is still there, with no type.
        assert!(tree.objects[2].kind.is_none());
    });
    let mut project = Project::new();
    let errors = project.add("Broken", "import QtQuick\nItem {").unwrap_err();
    assert!(!errors.is_empty());
    assert!(!project.has("Broken"));
}

#[test]
fn a_handler_is_of_a_signal_or_of_a_change() {
    assert_eq!(handled("onClicked"), Some("clicked".to_string()));
    assert_eq!(handled("onX"), Some("x".to_string()));
    assert_eq!(handled("only"), None);
    assert_eq!(
        classify("Component.onCompleted", |_| false, |_| false),
        Some(Own::Completed)
    );
    assert_eq!(
        classify("onPicked", |name| name == "picked", |_| false),
        Some(Own::Signal("picked".to_string()))
    );
    assert_eq!(
        classify("onTitleChanged", |_| false, |name| name == "title"),
        Some(Own::Changed("title".to_string()))
    );
    assert_eq!(classify("onWidthChanged", |_| false, |_| false), None);

    analyzed(
        &[(
            "Sample",
            "import QtQuick\nMouseArea { signal picked(int index); property int count }",
        )],
        "Sample",
        |tree, types, _| {
            assert_eq!(
                tree.own(0, "onPicked"),
                Some(Own::Signal("picked".to_string()))
            );
            assert_eq!(
                tree.own(0, "onCountChanged"),
                Some(Own::Changed("count".to_string()))
            );
            assert_eq!(tree.own(0, "onClicked"), None);
            assert_eq!(
                tree.signal(types, 0, "clicked"),
                Some(vec!["mouse".to_string()])
            );
            assert_eq!(tree.signal(types, 0, "countChanged"), Some(vec![]));
            assert!(tree.property(types, 0, &["anchors", "fill"]).is_some());
            assert!(tree.property(types, 0, &["anchors", "nothing"]).is_none());
        },
    );
}

#[test]
fn the_files_a_project_imports_by_path_are_found() {
    let project = project(&[
        (
            "main",
            "import QtQuick\nimport \"content\"\nimport \"lib.js\" as Lib\nItem {}",
        ),
        (
            "content/Clock",
            "import QtQuick\nimport \"../shared\"\nItem {}",
        ),
    ]);
    assert_eq!(project.directories(), ["content", "shared"]);
    assert_eq!(join("content", "../shared/./x"), "shared/x");
    assert_eq!(join("", "../up"), "../up");
    assert_eq!(join("/abs", "x"), "/abs/x");
    assert_eq!(relative("content/Clock", "shared/Hand"), "../shared/Hand");
    assert_eq!(relative("Main", "content/Clock"), "./content/Clock");
}

#[test]
fn a_name_one_component_has_may_be_used_by_another_as_it_runs() {
    let project = project(&[
        (
            "Main",
            "import QtQuick\nItem { id: window; property int secret; property int width: 3; Loader {} }",
        ),
        (
            "Page",
            "import QtQuick\nItem { x: window.x + secret + width + own; property int own }",
        ),
    ]);
    let runtime = Runtime::new();
    let mut names: Vec<String> = project.dynamic_names(&runtime).into_iter().collect();
    names.sort();
    // `width` is what any Item has, and `own` is the file's own.
    assert_eq!(names, ["secret", "window"]);
    let mut mentions: Vec<&String> = project.mentions("Page").collect();
    mentions.sort();
    assert_eq!(mentions, ["secret", "width", "window"]);
}

#[test]
fn a_pragma_is_kept_in_the_summary() {
    let project = project(&[
        (
            "Bound",
            "pragma ComponentBehavior: Bound\nimport QtQuick\nItem {}",
        ),
        (
            "Unbound",
            "pragma ComponentBehavior: Unbound\nimport QtQuick\nItem {}",
        ),
    ]);
    assert!(project.summary("Bound").unwrap().is_bound);
    assert!(!project.summary("Unbound").unwrap().is_bound);
}
