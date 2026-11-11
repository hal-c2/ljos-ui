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

#[test]
fn a_file_read_and_not_named_is_kept_with_the_files_that_name_it() {
    let project = Files::default();
    let mut lowering = Lowering::new();
    lowering.put(
        "app/Main",
        "import QtQuick\nItem { Swatch { color: \"red\" } }\n".to_string(),
    );
    lowering.put_read_only("app/Swatch", "import QtQuick\nRectangle { }\n".to_string());
    lowering.settle(&project);
    assert!(cause(&lowering, "app/Swatch").contains("it is not named"));
    assert!(
        cause(&lowering, "app/Main").contains("`Swatch.qml`, which is kept"),
        "{}",
        cause(&lowering, "app/Main")
    );
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
