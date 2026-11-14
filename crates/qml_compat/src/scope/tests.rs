use qml_syntax::Allocator;

use super::*;

/// A project with the directory `parts` and the module `Gauges`, each of
/// whose types is built on `Item`, and the file `Swatch.qml` beside it.
struct Project;

fn built_on_item() -> Root {
    Root {
        type_name: "Item".to_owned(),
        bindings: Vec::new(),
        properties: Vec::new(),
        declares: vec!["level".to_owned()],
        kept: false,
    }
}

impl Locals for Project {
    fn root(&self, name: &str) -> Option<Root> {
        (name == "Swatch").then(built_on_item)
    }

    fn module(&self, uri: &str) -> Option<ProjectModule> {
        (uri == "Gauges").then_some(ProjectModule::Files)
    }

    fn imported(&self, _from: Imported<'_>, name: &str) -> Option<Root> {
        (name != "Item").then(built_on_item)
    }

    fn controls_style(&self) -> Option<&str> {
        Some(CONTROLS_STYLE)
    }
}

fn scope_of(allocator: &Allocator, imports: &str) -> Scope<'static> {
    let source = allocator.alloc_str(&format!("{imports}\nItem {{ }}\n"));
    let parsed = qml_syntax::parse(allocator, source);
    assert!(parsed.is_ok(), "{source}");
    let modules = Modules::new().expect("Qt's and Ljós's modules are described");
    let mut scope = Scope::new(modules, &Project);
    scope.read(&parsed.document.imports).expect("imports read");
    scope
}

/// What `name` means in a file with `imports`, said short: the module of
/// the Qt type it is, `on T` for a file of the project built on `T`, or
/// the cause the file is kept for.
fn meaning(imports: &str, name: &str) -> String {
    let allocator = Allocator::default();
    match scope_of(&allocator, imports).ty(name, 0, 0) {
        Ok(Ty::Exact(t)) => format!("{}'s", t.module),
        Ok(Ty::Derived(t)) => format!("on {}", t.name),
        Ok(other) => format!("{other:?}"),
        Err(kept) => format!("{:?}", kept.cause),
    }
}

/// Of a Qt module's import and one of the project's that both have a name,
/// the last is the type, as the loader takes the last; and the lowered
/// file, whose Ljós import is on a line of its own, finds the same type or
/// is kept.
#[test]
fn the_last_import_with_a_name_is_what_it_means() {
    let parts = "import \"parts\"";
    // Qt's table has `Qt` among the builtins every file has, which it calls
    // `QML`; Ljós's own table, without them, files it under `QtQml`.
    let qt_global = if qt_types::has_qt_table() {
        "QML's"
    } else {
        "QtQml's"
    };
    for (imports, name, means) in [
        ("import QtQuick".to_owned(), "Rectangle", "QtQuick's"),
        (format!("import QtQuick\n{parts}"), "Rectangle", "on Item"),
        (format!("{parts}\nimport QtQuick"), "Rectangle", "QtQuick's"),
        // A module of the project is looked up as a directory is.
        (
            "import QtQuick\nimport QtQuick.Controls\nimport Gauges".to_owned(),
            "Button",
            "on Item",
        ),
        // QtQml's types are brought by QtQuick too.
        (
            format!("import QtQuick\n{parts}\nimport QtQml"),
            "Timer",
            "QtQml's",
        ),
        // Of a module imported twice, the last import is the one.
        (
            format!("import QtQuick\nimport QtQuick.Layouts\n{parts}\nimport QtQuick.Layouts"),
            "RowLayout",
            "QtQuick.Layouts's",
        ),
        (
            format!("import QtQuick.Layouts\n{parts}\nimport QtQuick"),
            "RowLayout",
            "on Item",
        ),
        // A file beside it, and a name nothing has.
        ("import QtQuick".to_owned(), "Swatch", "on Item"),
        ("import QtQuick".to_owned(), "Nothing", "Unknown"),
        // `Qt` is every file's.
        ("import QtQuick".to_owned(), "Qt", qt_global),
    ] {
        assert_eq!(meaning(&imports, name), means, "{name} in {imports:?}");
    }
}

/// Where the lowered file would find another type than Qt's file does, the
/// file is kept, and says which.
#[test]
fn a_name_the_lowered_file_finds_otherwise_keeps_the_file() {
    let parts = "import \"parts\"";
    for (imports, name, means) in [
        // QtQml's line becomes Ljos.Core's, before the project's import,
        // which QtQuick's import of the same type came after.
        (
            format!("import QtQml\n{parts}\nimport QtQuick"),
            "Timer",
            r#"ShadowedInLowered { name: "Timer", by: Project, module: "Ljos.Core" }"#,
        ),
        // QtQuick.Shapes' line is emptied, and the project's import comes
        // before QtQuick's, which Ljos.Ui takes.
        (
            format!("import QtQuick.Shapes\n{parts}\nimport QtQuick"),
            "Shape",
            r#"FoundFirst { name: "Shape", is: Some(Project), module: "Ljos.Ui" }"#,
        ),
        // A name no Qt module the file imports has, that Ljos.Ui has.
        (
            "import QtQuick".to_owned(),
            "Shape",
            r#"FoundFirst { name: "Shape", is: None, module: "Ljos.Ui" }"#,
        ),
        (
            "import QtQuick\nimport QtQuick.Shapes".to_owned(),
            "Shape",
            "QtQuick.Shapes's",
        ),
    ] {
        assert_eq!(meaning(&imports, name), means, "{name} in {imports:?}");
    }
}

/// Ljós's name for a Qt type is given where the type is Qt's, and not
/// where a type of the project has its name.
#[test]
fn a_type_is_renamed_where_it_is_qts() {
    let allocator = Allocator::default();
    let renamed = |imports: &str| scope_of(&allocator, imports).renamed_type("MouseArea");
    assert_eq!(renamed("import QtQuick"), Some("PointerArea"));
    assert_eq!(
        renamed("import \"parts\"\nimport QtQuick"),
        Some("PointerArea")
    );
    assert_eq!(renamed("import QtQuick\nimport \"parts\""), None);
}
