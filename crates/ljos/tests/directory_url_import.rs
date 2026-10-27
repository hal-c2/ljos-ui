//! A directory imported by its `file:` URL (#158), in a file compiled
//! from disk and in a text made with `Qt.createQmlObject`: its types are
//! found and shadow an earlier import's, as a relative directory
//! import's do. A character the URL escapes is part of the name.

use std::fs;
use std::path::Path;

use ljos::prelude::*;

/// A directory's own `Text`, red, as Qt's Text is black.
const TEXT: &str = "import QtQuick as Q\nQ.Text { color: \"red\" }\n";

/// A directory whose name a URL has to escape.
const ODD: &str = "odd #1 %25 ?x";

const MAIN: &str = r#"
import QtQuick
Item {
    id: root
    property string made: ""
    function make(text, place) {
        try {
            made = "" + Qt.createQmlObject(text, root, place).color
        } catch (e) {
            made = "" + e
        }
    }
    AbsUrl { objectName: "absUrl" }
    AlonePlugin { objectName: "alone" }
    Odd { objectName: "odd" }
    Relative { objectName: "relative" }
}
"#;

/// `cases/` as the issue lays it out, with the URLs written into the
/// files that import by them.
fn cases(dir: &Path) -> std::path::PathBuf {
    let cases = dir.join("cases");
    let plugin = file_url(&cases.join("plugin"));
    let odd = file_url(&cases.join(ODD));
    let files = [
        ("plugin/Text.qml", TEXT.to_owned()),
        (&format!("{ODD}/Text.qml"), TEXT.to_owned()),
        (
            "AbsUrl.qml",
            format!("import QtQuick\nimport \"{plugin}\"\nText {{ text: \"y\" }}\n"),
        ),
        (
            "AlonePlugin.qml",
            format!("import \"{plugin}/\"\nText {{ text: \"y\" }}\n"),
        ),
        (
            "Odd.qml",
            format!("import QtQuick\nimport \"{odd}\"\nText {{ text: \"y\" }}\n"),
        ),
        (
            "Relative.qml",
            "import QtQuick\nimport \"plugin\"\nText { text: \"y\" }\n".to_owned(),
        ),
        ("Main.qml", MAIN.to_owned()),
    ];
    for (name, text) in files {
        let path = cases.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    cases
}

fn colour_of(engine: &Engine, root: ObjectId, name: &str) -> String {
    let found = engine
        .children(root)
        .into_iter()
        .find(|&c| engine.read_by_name(c, "objectName").to_js_string().as_ref() == name)
        .unwrap_or_else(|| panic!("no {name}"));
    engine
        .read_by_name(found, "color")
        .to_js_string()
        .to_string()
}

fn made(engine: &Engine, root: ObjectId, text: &str, place: &str) -> String {
    engine
        .invoke_by_name(root, "make", &[Value::from(text), Value::from(place)])
        .unwrap();
    engine.read_by_name(root, "made").to_js_string().to_string()
}

fn started(dir: &Path) -> (Engine, ObjectId) {
    let cases = cases(dir);
    let engine = ljos::engine();
    let compiled = ljos::dev::compile_quiet(&[&cases], Some(&engine)).unwrap();
    let errors: Vec<String> = compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == ljos::dev::Severity::Error)
        .map(ToString::to_string)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    engine.set_component_source(compiled.source);
    ljos::dev::install_text_compiler(&engine, &[&cases]);
    let root = engine.create_from_url("Main", None, Vec::new()).unwrap();
    (engine, root)
}

#[test]
fn a_compiled_file_imports_a_directory_by_its_url() {
    let tmp = tempfile::tempdir().unwrap();
    let (engine, root) = started(tmp.path());

    assert_eq!(colour_of(&engine, root, "relative"), "#ff0000");
    assert_eq!(colour_of(&engine, root, "absUrl"), "#ff0000");
    assert_eq!(colour_of(&engine, root, "alone"), "#ff0000");
    assert_eq!(colour_of(&engine, root, "odd"), "#ff0000");
}

#[test]
fn a_text_placed_elsewhere_imports_a_directory_by_its_url() {
    let tmp = tempfile::tempdir().unwrap();
    let (engine, root) = started(tmp.path());
    let cases = tmp.path().join("cases");
    let plugin = file_url(&cases.join("plugin"));
    let odd = file_url(&cases.join(ODD));
    let elsewhere = "file:///elsewhere/a.qml";

    let two_lines = format!("import QtQuick\nimport \"{plugin}\"\nText {{}}");
    assert_eq!(made(&engine, root, &two_lines, elsewhere), "#ff0000");
    let one_line = format!("import QtQuick; import \"{plugin}\"; Text {{}}");
    assert_eq!(made(&engine, root, &one_line, elsewhere), "#ff0000");
    let alone = format!("import \"{plugin}\"\nText {{}}");
    assert_eq!(made(&engine, root, &alone, elsewhere), "#ff0000");
    let escaped = format!("import QtQuick\nimport \"{odd}\"\nText {{}}");
    assert_eq!(made(&engine, root, &escaped, elsewhere), "#ff0000");
    let beside = "import \"plugin\"\nText {}";
    let place = file_url(&cases.join("b.qml"));
    assert_eq!(made(&engine, root, beside, &place), "#ff0000");
}
