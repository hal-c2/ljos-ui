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

/// A module whose singleton says where its directory of types is, by a
/// URL made from its own (#305).
fn module(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("modular");
    let files = [
        (
            "Kit/Parts/qmldir",
            "module Kit.Parts\nsingleton Registry 1.0 Registry.qml\n",
        ),
        (
            "Kit/Parts/Registry.qml",
            "pragma Singleton\nimport QtQuick\n\
             QtObject { readonly property string types: Qt.resolvedUrl(\"plugin\") }\n",
        ),
        ("Kit/Parts/plugin/Text.qml", TEXT),
        (
            "Main.qml",
            r#"import QtQuick
import Kit.Parts
Item {
    id: root
    property string made: ""
    property string types: Registry.types
    function make(text, place) {
        try {
            made = "" + Qt.createQmlObject(text, root, place).color
        } catch (e) {
            made = "" + e
        }
    }
}
"#,
        ),
    ];
    for (name, text) in files {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    root
}

/// The text the singleton's URL is imported into, after a module, as a
/// plugin's file would be.
fn made_with_module_types(engine: &Engine, root: ObjectId) -> String {
    let types = engine.read_by_name(root, "types").to_js_string();
    let text = format!("import QtQuick; import Kit.Parts; import \"{types}\"\nText {{}}\n");
    made(engine, root, &text, "file:///elsewhere/a.qml")
}

#[test]
fn a_module_says_its_directory_by_a_url_of_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let root = module(tmp.path());
    let engine = ljos::engine();
    let compiled = ljos::dev::compile_quiet(&[&root], Some(&engine)).unwrap();
    engine.set_component_source(compiled.source);
    ljos::dev::install_text_compiler(&engine, &[&root]);
    let main = engine.create_from_url("Main", None, Vec::new()).unwrap();

    assert_eq!(made_with_module_types(&engine, main), "#ff0000");
}

/// A source read from a pack keeps no file's URL, so the singleton's
/// is the one the engine gives a file it has no URL for
/// (`file:///Kit/Parts/plugin`), and the texts' compiler finds the
/// directory there too.
#[test]
fn a_module_loaded_from_a_pack_says_its_directory_by_a_url_of_its_own() {
    let tmp = tempfile::tempdir().unwrap();
    let root = module(tmp.path());
    let engine = ljos::engine();
    let compiled = ljos::dev::compile_quiet(&[&root], Some(&engine)).unwrap();
    let pack = compiled.source.to_pack().unwrap();
    engine.set_component_source(ljos::MapSource::from_pack(&pack).unwrap());
    ljos::dev::install_text_compiler(&engine, &[&root]);
    let main = engine.create_from_url("Main", None, Vec::new()).unwrap();

    assert_eq!(
        engine.read_by_name(main, "types").to_js_string().as_ref(),
        "file:///Kit/Parts/plugin"
    );
    assert_eq!(made_with_module_types(&engine, main), "#ff0000");
}
