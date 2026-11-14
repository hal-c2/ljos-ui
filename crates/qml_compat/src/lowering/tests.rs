use std::collections::BTreeMap;

use super::*;

/// A project with no modules, whose files are the lowering's.
#[derive(Default)]
struct Files {
    scripts: BTreeMap<String, String>,
}

struct Beside<'a> {
    key: &'a str,
    held: &'a dyn Held,
}

impl Locals for Beside<'_> {
    fn root(&self, name: &str) -> Option<Root> {
        let dir = self.key.rsplit_once('/').map_or("", |(dir, _)| dir);
        self.held.root(&format!("{dir}/{name}")).flatten()
    }
}

impl Project for Files {
    fn locals<'a>(&'a self, key: &'a str, held: &'a dyn Held) -> Box<dyn Locals + 'a> {
        Box::new(Beside { key, held })
    }

    fn module_files(&self, _: &str, _: &str) -> Vec<String> {
        Vec::new()
    }

    fn script(&self, _: &str, path: &str) -> Option<ProjectScript<'_>> {
        self.scripts
            .get_key_value(path)
            .map(|(path, text)| ProjectScript {
                path: path.clone(),
                text,
            })
    }
}

const TABLE: &str = "import QtQuick\nItem {\n    ListModel {\n        id: fruitModel\n        ListElement { name: \"Apple\" }\n    }\n    Repeater { model: fruitModel; Text { text: model.name } }\n}\n";

fn text(lowering: &Lowering, key: &str) -> String {
    match lowering.lowering(key) {
        Some(Outcome::Lowered(lowered)) => lowered.text.clone(),
        other => panic!("{key} is not lowered: {other:?}"),
    }
}

fn cause(lowering: &Lowering, key: &str) -> String {
    match lowering.lowering(key) {
        Some(Outcome::Kept(kept)) => kept.to_string(),
        other => panic!("{key} is not kept: {other:?}"),
    }
}

#[test]
fn a_file_is_lowered_again_when_a_file_it_is_built_on_changes() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(text(&lowering, "app/Main").contains("fill: \"red\""));

    lowering.put("app/Swatch", "import QtQuick\nText { }\n".to_string());
    lowering.settle(&project);
    assert!(
        text(&lowering, "app/Main").contains("color: \"red\""),
        "{}",
        text(&lowering, "app/Main")
    );
}

#[test]
fn the_rows_module_a_file_imports_is_written_beside_it_and_goes_with_it() {
    let mut project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put("app/Main", TABLE.to_string());
    let scripts = lowering.settle(&project);
    let [ScriptEdit::Put { path, text }] = scripts.as_slice() else {
        panic!("{scripts:?}");
    };
    assert_eq!(path, "app/Main.rows.ts");
    project.scripts.insert(path.clone(), text.clone());

    // The module is the pass's: the file lowered again keeps importing it.
    assert!(lowering.changed(Changed::Script("app/Main.rows.ts")));
    assert_eq!(lowering.settle(&project), []);
    assert!(text_of(&lowering).contains("Main.rows.ts"));

    assert_eq!(
        lowering.forget("app/Main"),
        Some(vec!["app/Main.rows.ts".to_string()])
    );
}

fn text_of(lowering: &Lowering) -> String {
    text(lowering, "app/Main")
}

#[test]
fn a_script_of_the_projects_where_the_rows_module_goes_keeps_the_file() {
    let project = Files {
        scripts: BTreeMap::from([(
            "app/Main.rows.ts".to_string(),
            "export const mine = 1\n".to_string(),
        )]),
    };
    let mut lowering = Lowering::new();
    lowering.put("app/Main", TABLE.to_string());
    assert_eq!(lowering.settle(&project), []);
    assert!(
        cause(&lowering, "app/Main").contains("`app/Main.rows.ts` is a script of the project's"),
        "{}",
        cause(&lowering, "app/Main")
    );
}

/// A file kept is Qt's: one that uses it is lowered binding Qt's names on
/// it, as a Ljós file reads them on a kept type.
#[test]
fn a_file_that_uses_one_kept_keeps_qts_names_on_it() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\"; border.color: \"blue\" } }\n".to_string(),
    );
    lowering.put_read_only("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(cause(&lowering, "app/Swatch").contains("it is not named"));
    let main = text(&lowering, "app/Main");
    assert!(
        main.contains("Swatch { color: \"red\"; border.color: \"blue\" }"),
        "{main}"
    );
}

/// A name a script reads unqualified in a kept type's object is that
/// object's if its type has it, else the root's: what the kept type has is
/// not known, so a name the root renames keeps the file.
#[test]
fn a_name_read_in_a_kept_types_object_that_the_root_renames_keeps_the_file() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nRectangle { Swatch { Component.onCompleted: console.log(color, opacity) } }\n"
            .to_string(),
    );
    lowering.put_read_only("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    let main = cause(&lowering, "app/Main");
    assert!(main.contains("`color`"), "{main}");
}

/// A file built on one built on a kept file binds the kept file's names
/// through it: it is lowered again when that file turns, though the file
/// between turns neither way.
#[test]
fn a_file_is_lowered_again_when_one_it_uses_through_another_turns() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Wrapper { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Wrapper", "import QtQuick\nSwatch { }\n".to_string());
    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(text(&lowering, "app/Main").contains("fill: \"red\""));

    let kept = "import QtQuick\nRectangle { function f() { Qt.createQmlObject(\"\", this) } }\n";
    lowering.put("app/Swatch", kept.to_string());
    lowering.settle(&project);
    cause(&lowering, "app/Swatch");
    text(&lowering, "app/Wrapper");
    let main = text(&lowering, "app/Main");
    assert!(main.contains("color: \"red\""), "{main}");

    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    let main = text(&lowering, "app/Main");
    assert!(main.contains("fill: \"red\""), "{main}");
}

#[test]
fn a_file_read_and_not_named_keeps_the_files_it_uses() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put_read_only(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert_eq!(
        cause(&lowering, "app/Swatch"),
        "it is used by `Main.qml`, which is kept: it is not named: name it too"
    );
}

/// A file that turns kept is Qt's to the files that use it, which are
/// lowered again, and turned back to lowered, Ljós's again.
#[test]
fn a_file_is_lowered_again_when_one_it_uses_is_kept_or_lowered() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(text_of(&lowering).contains("fill: \"red\""));

    let kept = "import QtQuick\nRectangle { function f() { Qt.createQmlObject(\"\", this) } }\n";
    lowering.put("app/Swatch", kept.to_string());
    lowering.settle(&project);
    cause(&lowering, "app/Swatch");
    assert!(
        text_of(&lowering).contains("color: \"red\""),
        "{}",
        text_of(&lowering)
    );

    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(
        text_of(&lowering).contains("fill: \"red\""),
        "{}",
        text_of(&lowering)
    );
}

#[test]
fn two_files_that_would_turn_each_other_over_are_both_kept() {
    // Against a lowered `Swatch`, an `Item`, Main's `color` may be one
    // Ljós renames, and Main is kept; that keeps Swatch, against which
    // Main is lowered with Qt's names, which leaves Swatch lowered again.
    // Neither is lowered while the other is, so both are kept.
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Swatch", "import QtQuick\nItem { }\n".to_string());
    lowering.settle(&project);
    assert!(
        cause(&lowering, "app/Main").contains("`color`"),
        "{}",
        cause(&lowering, "app/Main")
    );
    assert!(
        cause(&lowering, "app/Swatch").contains("`Main.qml`, which is kept"),
        "{}",
        cause(&lowering, "app/Swatch")
    );
    // Settled again with nothing changed, it says the same.
    lowering.settle(&project);
    assert!(cause(&lowering, "app/Main").contains("`color`"));
    assert!(cause(&lowering, "app/Swatch").contains("`Main.qml`, which is kept"));
}

#[test]
fn forgetting_a_file_lowers_the_files_that_named_it_again() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(text_of(&lowering).contains("fill: \"red\""));
    assert_eq!(lowering.forget("app/Swatch"), Some(Vec::new()));
    lowering.settle(&project);
    // Without the file, `Swatch` is a type nothing declares.
    assert!(
        cause(&lowering, "app/Main").contains("`color`"),
        "{}",
        cause(&lowering, "app/Main")
    );
    assert_eq!(lowering.forget("app/Swatch"), None);
}

/// A file lowered to `modules` beside it.
fn lowered(modules: &[(&str, &str)]) -> Outcome {
    Outcome::Lowered(crate::Lowered {
        text: String::new(),
        changes: Vec::new(),
        modules: modules
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect(),
    })
}

// No two files' rows modules go to one path (`rows_module` is named by the
// file's own name), so these give each file a module of one name.

fn project(scripts: &[(&str, &str)]) -> Files {
    Files {
        scripts: scripts
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect(),
    }
}

/// A lowering that wrote the module of each of `owners` before.
fn written_by(owners: &[&str]) -> Lowering {
    let mut lowering = Lowering::new();
    for owner in owners {
        lowering
            .generated
            .insert(owner.to_string(), vec![M.to_string()]);
    }
    lowering
}

const M: &str = "app/m.rows.ts";

fn put(text: &str) -> ScriptEdit {
    ScriptEdit::Put {
        path: M.to_string(),
        text: text.to_string(),
    }
}

fn owners(lowering: &Lowering) -> Vec<&str> {
    lowering
        .generated
        .iter()
        .filter(|(_, wrote)| wrote.iter().any(|path| path == M))
        .map(|(key, _)| key.as_str())
        .collect()
}

#[test]
fn a_file_settled_after_another_sees_the_module_that_one_writes() {
    let mut lowering = Lowering::new();
    let scripts = lowering.settle_modules(
        &project(&[]),
        &[
            ("app/a".to_string(), lowered(&[("m.rows.ts", "a")])),
            ("app/b".to_string(), lowered(&[("m.rows.ts", "b")])),
        ],
    );
    assert_eq!(scripts, [put("a")]);
    assert_eq!(owners(&lowering), ["app/a"]);
}

#[test]
fn a_file_settled_after_another_writes_where_that_one_dropped_its_module() {
    let mut lowering = written_by(&["app/a"]);
    let scripts = lowering.settle_modules(
        &project(&[(M, "a")]),
        &[
            ("app/a".to_string(), Outcome::Unchanged),
            ("app/b".to_string(), lowered(&[("m.rows.ts", "b")])),
        ],
    );
    assert_eq!(scripts, [ScriptEdit::Drop(M.to_string()), put("b")]);
    assert_eq!(owners(&lowering), ["app/b"]);
}

#[test]
fn a_file_settled_before_another_writes_where_that_one_drops_its_module() {
    let mut lowering = written_by(&["app/b"]);
    let scripts = lowering.settle_modules(
        &project(&[(M, "b")]),
        &[
            ("app/a".to_string(), lowered(&[("m.rows.ts", "a")])),
            ("app/b".to_string(), Outcome::Unchanged),
        ],
    );
    assert_eq!(scripts, [ScriptEdit::Drop(M.to_string()), put("a")]);
    assert_eq!(owners(&lowering), ["app/a"]);
}

#[test]
fn a_module_one_file_still_writes_is_not_dropped_by_another_that_wrote_it() {
    let mut lowering = written_by(&["app/a", "app/b"]);
    let scripts = lowering.settle_modules(
        &project(&[(M, "x")]),
        &[
            ("app/a".to_string(), lowered(&[("m.rows.ts", "a")])),
            ("app/b".to_string(), Outcome::Unchanged),
        ],
    );
    assert_eq!(scripts, [put("a")]);
    assert_eq!(owners(&lowering), ["app/a"]);
}

#[test]
fn a_module_a_file_settled_before_writes_is_not_written_over() {
    let mut lowering = written_by(&["app/a", "app/b"]);
    let scripts = lowering.settle_modules(
        &project(&[(M, "x")]),
        &[
            ("app/a".to_string(), lowered(&[("m.rows.ts", "a")])),
            ("app/b".to_string(), lowered(&[("m.rows.ts", "b")])),
        ],
    );
    assert_eq!(scripts, [put("a")]);
    assert_eq!(owners(&lowering), ["app/a"]);
}

#[test]
fn a_module_two_files_wrote_and_neither_writes_now_is_dropped_once() {
    let mut lowering = written_by(&["app/a", "app/b"]);
    let scripts = lowering.settle_modules(
        &project(&[(M, "a")]),
        &[
            ("app/a".to_string(), Outcome::Unchanged),
            ("app/b".to_string(), Outcome::Unchanged),
        ],
    );
    // The project is told once that the module is gone.
    assert_eq!(scripts, [ScriptEdit::Drop(M.to_string())]);
    assert!(owners(&lowering).is_empty());
}

/// What a directory a file imports holds may change its lowering: the
/// file is lowered again. A URL is the project's to resolve, against the
/// places it reads, so a file that imports one may import any directory.
#[test]
fn a_file_that_imports_a_directory_is_lowered_again_when_it_is_described() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nimport \"../site\"\nItem { }\n".to_string(),
    );
    lowering.put(
        "app/Url",
        "import QtQuick\nimport \"file:///home/me/app/site\"\nItem { }\n".to_string(),
    );
    lowering.put("app/Plain", "import QtQuick\nItem { }\n".to_string());
    lowering.settle(&project);

    assert!(lowering.changed(Changed::Dir("site")));
    assert_eq!(
        lowering
            .stale
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["app/Main", "app/Url"]
    );
}
