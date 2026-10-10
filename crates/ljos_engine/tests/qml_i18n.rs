//! Localisation on the engine: catalogues, the UI language and what follows
//! it. Mirrors features/i18n/*.feature; what a `.feature` cannot see is here:
//! how many times a binding ran.

mod support;

use ljos_engine::{ObjectId, Value};
use ljos_i18n::{Catalog, Translation};
use support::{Harness, engine};

fn text(translation: &str) -> Translation {
    Translation::Text(translation.to_owned())
}

fn forms(forms: &[&str]) -> Translation {
    Translation::Numerus(forms.iter().map(|form| (*form).to_owned()).collect())
}

fn icelandic() -> Box<Catalog> {
    let mut catalog = Catalog::new("is_IS");
    catalog.insert("Main", "Open", "", text("Opna"));
    catalog.insert("Main", "Close", "", text("Loka"));
    catalog.insert("Main", "Close", "near", text("Nálægt"));
    catalog.insert("Main", "%n file(s)", "", forms(&["%n skrá", "%n skrár"]));
    catalog.insert("Dialog", "Cancel", "", text("Hætta við"));
    catalog.insert_id("app-title", text("Ljós"));
    Box::new(catalog)
}

fn german() -> Box<Catalog> {
    let mut catalog = Catalog::new("de");
    catalog.insert("Main", "Open", "", text("Öffnen"));
    catalog.insert("Main", "%n file(s)", "", forms(&["%n Datei", "%n Dateien"]));
    Box::new(catalog)
}

fn string(h: &Harness, obj: ObjectId, prop: &str) -> String {
    h.get(obj, prop).to_js_string().to_string()
}

/// Every binding writes its name to the log when it runs.
const COUNTED: &str = r#"
import QtQuick
Item {
  property int n: 3
  property string title: { console.log("title"); return qsTr("Open") }
  property string files: { console.log("files"); return qsTr("%n file(s)", "", n) }
  property string cancel: { console.log("cancel"); return qsTranslate("Dialog", "Cancel") }
  property string name: { console.log("name"); return qsTrId("app-title") }
  property string plain: { console.log("plain"); return "n is " + n }
  property string upper: { console.log("upper"); return title.toUpperCase() }
  property string marked: { console.log("marked"); return QT_TR_NOOP("Open") }
  property string literal: qsTr("Close")
}
"#;

const ALL: [&str; 8] = [
    "title", "files", "cancel", "name", "plain", "upper", "marked", "literal",
];

/// Reads every property, then says which bindings ran since the last call.
fn ran(h: &Harness, root: ObjectId) -> Vec<String> {
    for prop in ALL {
        h.get(root, prop);
    }
    let mut ran = h.log();
    ran.sort();
    ran
}

#[test]
fn a_language_change_re_runs_the_bindings_that_translated_and_no_others() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    let root = h.load(COUNTED);
    assert_eq!(
        ran(&h, root),
        [
            "cancel", "files", "marked", "name", "plain", "title", "upper"
        ]
    );
    assert_eq!(string(&h, root, "title"), "Open");

    h.engine.set_ui_language("is_IS");
    // `upper` reads `title`, which changed; `plain` and `marked` asked for
    // no translation and are left alone.
    assert_eq!(ran(&h, root), ["cancel", "files", "name", "title", "upper"]);
    assert_eq!(string(&h, root, "title"), "Opna");
    assert_eq!(string(&h, root, "files"), "3 skrár");
    assert_eq!(string(&h, root, "cancel"), "Hætta við");
    assert_eq!(string(&h, root, "name"), "Ljós");
    assert_eq!(string(&h, root, "upper"), "OPNA");
    assert_eq!(string(&h, root, "marked"), "Open");
    assert_eq!(string(&h, root, "literal"), "Loka");
    assert_eq!(string(&h, root, "plain"), "n is 3");
    h.log();

    // The language it already has: nothing to do.
    h.engine.set_ui_language("is_IS");
    assert_eq!(ran(&h, root), [] as [&str; 0]);
}

#[test]
fn a_binding_whose_text_stays_the_same_does_not_wake_what_reads_it() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    let root = h.load(
        r#"
import QtQuick
Item {
  property string same: { console.log("same"); return qsTr("Not in the catalogue") }
  property string reader: { console.log("reader"); return same + "!" }
}
"#,
    );
    let read = |h: &Harness| {
        h.get(root, "same");
        h.get(root, "reader");
        h.log()
    };
    assert_eq!(read(&h).len(), 2);
    h.engine.set_ui_language("is_IS");
    assert_eq!(read(&h), ["same"]);
}

#[test]
fn a_change_of_another_dependency_does_not_re_run_other_translations() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    h.engine.set_ui_language("is_IS");
    let root = h.load(COUNTED);
    ran(&h, root);

    h.set(root, "n", Value::Int(1));
    assert_eq!(ran(&h, root), ["files", "plain"]);
    assert_eq!(string(&h, root, "files"), "1 skrá");
}

#[test]
fn qt_ui_language_is_a_property_bindings_can_follow_and_methods_can_set() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    h.engine.install_catalog(german());
    let root = h.load(
        r#"
import QtQuick
Item {
  property string language: Qt.uiLanguage
  property string title: qsTr("Open")
  property int direction: Qt.application.layoutDirection
  property bool mirrored: Qt.application.layoutDirection === Qt.RightToLeft
  function speak(language) { Qt.uiLanguage = language }
}
"#,
    );
    assert_eq!(string(&h, root, "language"), "en");
    assert_eq!(h.num(root, "direction"), 0.0);

    h.engine
        .invoke_by_name(root, "speak", &[Value::from("de_AT")])
        .unwrap();
    assert_eq!(h.engine.ui_language(), "de_AT");
    assert_eq!(string(&h, root, "language"), "de_AT");
    // No Austrian catalogue: the German one is the nearest relative.
    assert_eq!(string(&h, root, "title"), "Öffnen");

    h.engine.set_ui_language("ar");
    assert!(h.engine.is_right_to_left());
    assert_eq!(h.num(root, "direction"), 1.0);
    assert_eq!(h.get(root, "mirrored"), Value::Bool(true));
    // No Arabic catalogue: the text as written.
    assert_eq!(string(&h, root, "title"), "Open");

    h.engine.set_ui_language("is_IS");
    assert_eq!(h.num(root, "direction"), 0.0);
    assert_eq!(string(&h, root, "title"), "Opna");
}

#[test]
fn a_catalogue_can_come_and_go_while_the_program_runs() {
    let h = engine();
    h.engine.set_ui_language("is_IS");
    let root = h.load(
        r#"
import QtQuick
Item {
  property string title: qsTr("Open")
  property string literal: qsTr("Close", "near")
}
"#,
    );
    assert_eq!(string(&h, root, "title"), "Open");

    h.engine.install_catalog(icelandic());
    assert_eq!(string(&h, root, "title"), "Opna");
    assert_eq!(string(&h, root, "literal"), "Nálægt");

    assert!(h.engine.remove_catalog("is_IS"));
    assert_eq!(string(&h, root, "title"), "Open");
    assert_eq!(string(&h, root, "literal"), "Close");
    assert!(!h.engine.remove_catalog("is_IS"));
}

#[test]
fn the_host_can_translate_without_qml() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    h.engine.set_ui_language("is");
    assert_eq!(h.engine.translate("Main", "Open", "", None), "Opna");
    assert_eq!(h.engine.translate("Main", "Close", "near", None), "Nálægt");
    assert_eq!(
        h.engine.translate("Main", "%n file(s)", "", Some(21)),
        "21 skrá"
    );
    assert_eq!(
        h.engine.translate("Main", "%n file(s)", "", Some(11)),
        "11 skrár"
    );
    assert_eq!(h.engine.translate("Main", "Missing", "", None), "Missing");
    assert_eq!(h.engine.translate_id("app-title", None), "Ljós");
    assert_eq!(h.engine.translate_id("no-such-id", None), "no-such-id");
}

#[test]
fn the_context_of_a_script_is_its_own_file() {
    let h = engine();
    let mut catalog = Catalog::new("is_IS");
    catalog.insert("Main", "Open", "", text("Opna (Main)"));
    catalog.insert("Part", "Open", "", text("Opna (Part)"));
    h.engine.install_catalog(Box::new(catalog));
    h.engine.set_ui_language("is_IS");
    h.files(&[
        (
            "Main",
            r#"
import QtQuick
Item {
  property int one: 1
  property string here: qsTr("Open") + one
  Part { objectName: "part" }
}
"#,
        ),
        (
            "Part",
            r#"
import QtQuick
Item {
  property int two: 2
  property string there: qsTr("Open") + two
  property string literal: qsTr("Open")
}
"#,
        ),
    ]);
    let root = h
        .engine
        .create_from_url("Main", None, Vec::new())
        .expect("creates");
    let part = h.find(root, "part");
    assert_eq!(string(&h, root, "here"), "Opna (Main)1");
    assert_eq!(string(&h, part, "there"), "Opna (Part)2");
    assert_eq!(string(&h, part, "literal"), "Opna (Part)");
}

#[test]
fn numbers_are_written_by_the_locale_not_by_the_ui_language() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    let root = h.load(
        r#"
import QtQuick
Item {
  property string grouped: "%L1".arg(1234.5)
  property string plain: "%1".arg(1234.5)
  property string point: Qt.locale().decimalPoint
  property string name: Qt.locale().name
  property string files: qsTr("%Ln file(s)", "", 12345)
  property string icelandic: Qt.locale("is_IS").groupSeparator
}
"#,
    );
    assert_eq!(h.engine.locale_name(), "en_US");
    assert_eq!(string(&h, root, "grouped"), "1,234.5");
    assert_eq!(string(&h, root, "plain"), "1234.5");
    assert_eq!(string(&h, root, "icelandic"), ".");

    h.engine.set_ui_language("is_IS");
    assert_eq!(string(&h, root, "name"), "en_US");
    assert_eq!(string(&h, root, "grouped"), "1,234.5");

    h.engine.set_locale("de_DE");
    assert_eq!(string(&h, root, "name"), "de_DE");
    assert_eq!(string(&h, root, "point"), ",");
    assert_eq!(string(&h, root, "grouped"), "1.234,5");
    assert_eq!(string(&h, root, "plain"), "1234.5");
    assert_eq!(string(&h, root, "files"), "12.345 file(s)");
    assert!(h.warnings().is_empty());
}

#[test]
fn a_delegate_and_an_inline_component_translate_in_their_files_context() {
    let h = engine();
    h.engine.install_catalog(icelandic());
    let root = h.load(
        r#"
import QtQml
QtObject {
  id: root
  component Label: QtObject { property string title: qsTr("Open") }
  property Label label: Label {}
  property Instantiator made: Instantiator {
    model: 2
    QtObject {
      property string literal: qsTr("Close", "near")
      property string worked: qsTr("Close") + index
    }
  }
  function rows() {
    var s = label.title
    for (var i = 0; i < made.count; i++)
      s += "|" + made.objectAt(i).literal + "," + made.objectAt(i).worked
    return s
  }
}
"#,
    );
    let rows = |h: &Harness| {
        h.engine
            .invoke_by_name(root, "rows", &[])
            .unwrap()
            .to_js_string()
            .to_string()
    };
    assert_eq!(rows(&h), "Open|Close,Close0|Close,Close1");
    h.engine.set_ui_language("is_IS");
    assert_eq!(rows(&h), "Opna|Nálægt,Loka0|Nálægt,Loka1");
    assert!(h.warnings().is_empty(), "{:?}", h.warnings());
}
