//! `QmlFrontEnd` against fixture QML files and throwaway projects. They hold
//! of a build with a table of Qt's types and of one with Ljós's own
//! description of its types alone; the few that are about one of the two say
//! which.

use std::path::{Path, PathBuf};

use ljos_mcp_author::{Diagnostic, FrontEnd, QmlFrontEnd, Severity};
use ljos_mcp_core::{ErrorCode, SurfaceError};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn run(root: &Path, files: &[&str], lint: bool) -> Vec<Diagnostic> {
    let front = QmlFrontEnd::new(root);
    let files: Vec<PathBuf> = files.iter().map(|f| root.join(f)).collect();
    let result = if lint {
        front.lint(&files)
    } else {
        front.check(&files)
    };
    result.expect("the analysis itself does not fail")
}

fn check(file: &str) -> Vec<Diagnostic> {
    run(&fixtures(), &[file], false)
}

fn lint(file: &str) -> Vec<Diagnostic> {
    run(&fixtures(), &[file], true)
}

/// `line:column code` of each diagnostic, for compact assertions.
fn brief(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| format!("{}:{} {}", d.line, d.column, d.code))
        .collect()
}

fn with_code<'d>(diagnostics: &'d [Diagnostic], code: &str) -> Vec<&'d Diagnostic> {
    diagnostics.iter().filter(|d| d.code == code).collect()
}

/// A project of `files` (path, text) in a temp dir.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

// --- check ---------------------------------------------------------------

#[test]
fn clean_file_has_no_diagnostics_in_check_and_lint() {
    assert_eq!(brief(&check("clean.qml")), Vec::<String>::new());
    assert_eq!(brief(&lint("clean.qml")), Vec::<String>::new());
}

#[test]
fn syntax_error_is_reported_with_position_and_stops_the_analysis() {
    let diagnostics = check("syntax_error.qml");
    assert_eq!(diagnostics.len(), 1);
    let syntax = with_code(&diagnostics, "syntax");
    assert_eq!(syntax.len(), 1, "{diagnostics:?}");
    assert_eq!((syntax[0].line, syntax[0].column), (9, 1));
    assert_eq!(syntax[0].severity, Severity::Error);
    assert!(syntax[0].file.ends_with("syntax_error.qml"));
}

#[test]
fn unknown_type_is_an_error_with_a_suggestion() {
    let diagnostics = check("unknown_type.qml");
    assert_eq!(brief(&diagnostics), ["4:5 unknown-type"]);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(
        diagnostics[0].message.contains("`Rectangle`"),
        "{:?}",
        diagnostics[0]
    );
}

#[test]
fn unknown_properties_signals_and_attached_members_are_found() {
    let diagnostics = check("unknown_property.qml");
    assert_eq!(
        brief(&diagnostics),
        [
            "5:5 unknown-property",
            "7:5 unknown-property",
            "8:5 unknown-signal",
            "9:5 unknown-property",
            "11:9 unknown-property",
        ]
    );
    assert!(diagnostics[0].message.contains("did you mean `color`"));
    assert!(diagnostics[1].message.contains("did you mean `fill`"));
    assert!(diagnostics[3].message.contains("Layout"));
    // Errors where Qt's table describes the types, as Qt fails to load
    // the file; read against Ljós's own table, which may lack what Qt
    // has, warnings, as the compiler's are.
    let severity = if qt_types::has_qt_table() {
        Severity::Error
    } else {
        Severity::Warning
    };
    assert!(diagnostics.iter().all(|d| d.severity == severity));
}

#[test]
fn check_leaves_lint_rules_to_lint() {
    assert_eq!(brief(&check("a11y.qml")), Vec::<String>::new());
    assert_eq!(brief(&check("i18n.qml")), Vec::<String>::new());
}

#[test]
fn siblings_are_types_and_undeclared_names_are_warnings() {
    let root = fixtures().join("app");
    let diagnostics = run(&root, &["Main.qml"], false);
    assert_eq!(
        brief(&diagnostics),
        [
            "14:9 unknown-property",
            "15:9 unknown-signal",
            "18:15 unresolved-name"
        ],
        "{diagnostics:?}"
    );
    let unresolved = with_code(&diagnostics, "unresolved-name")[0];
    assert_eq!(unresolved.severity, Severity::Warning);
    assert!(unresolved.message.contains("`missingName`"));
}

#[test]
fn a_name_another_component_provides_is_not_unresolved() {
    // `gadgetScale` is declared by Provider, which makes a Gizmo.
    let root = fixtures().join("app");
    assert_eq!(
        brief(&run(&root, &["Gizmo.qml"], false)),
        Vec::<String>::new()
    );
}

#[test]
fn duplicate_ids_and_dangling_aliases_are_errors() {
    let dir = project(&[(
        "Dup.qml",
        "import QtQuick\nItem {\n  id: a\n  Rectangle { id: a }\n  property alias x: missing.width\n}\n",
    )]);
    let diagnostics = run(dir.path(), &["Dup.qml"], false);
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"duplicate-id"), "{diagnostics:?}");
    assert!(codes.contains(&"unknown-alias-target"), "{diagnostics:?}");
}

#[test]
fn unknown_module_is_a_warning_and_holds_back_type_errors() {
    let dir = project(&[(
        "Main.qml",
        "import QtQuick\nimport Vendor.Widgets\nItem {\n  Gadget { sparkle: 1; onFizzed: x }\n}\n",
    )]);
    let diagnostics = run(dir.path(), &["Main.qml"], false);
    assert_eq!(
        brief(&diagnostics),
        ["2:1 unknown-module"],
        "{diagnostics:?}"
    );
    assert_eq!(diagnostics[0].severity, Severity::Warning);
}

#[test]
fn a_type_some_cpp_registers_is_not_unknown() {
    let dir = project(&[
        (
            "Main.qml",
            "import QtQuick\nItem { Backend { } Missing { } }\n",
        ),
        (
            "backend.h",
            "class Backend : public QObject { Q_OBJECT QML_ELEMENT };\n",
        ),
    ]);
    let diagnostics = run(dir.path(), &["Main.qml"], false);
    assert_eq!(
        brief(&diagnostics),
        ["2:20 unknown-type"],
        "{diagnostics:?}"
    );
    assert!(diagnostics[0].message.contains("`Missing`"));
}

#[test]
fn types_of_a_module_the_project_describes_resolve_through_its_qmldir() {
    let dir = project(&[
        ("lib/qmldir", "module Lib\nBadge 1.0 Badge.qml\n"),
        (
            "lib/Badge.qml",
            "import QtQuick\nRectangle { property int n: 0 }\n",
        ),
        (
            "Main.qml",
            "import QtQuick\nimport Lib\nItem { Badge { n: 2 } }\n",
        ),
    ]);
    assert_eq!(
        brief(&run(dir.path(), &["Main.qml"], false)),
        Vec::<String>::new()
    );
}

#[test]
fn unreadable_file_is_an_io_diagnostic() {
    let dir = project(&[]);
    let diagnostics = run(dir.path(), &["Nope.qml"], false);
    let io = with_code(&diagnostics, "io");
    assert_eq!(io.len(), 1, "{diagnostics:?}");
    assert_eq!(io[0].severity, Severity::Error);
}

#[test]
fn several_files_are_reported_in_the_order_given() {
    let diagnostics = run(
        &fixtures(),
        &["unknown_type.qml", "syntax_error.qml", "clean.qml"],
        false,
    );
    let files: Vec<&str> = diagnostics
        .iter()
        .map(|d| d.file.rsplit('/').next().unwrap())
        .collect();
    assert_eq!(files, ["unknown_type.qml", "syntax_error.qml"]);
}

/// A build without a table of Qt's types cannot tell a misspelt name from a
/// type of Qt's that Ljós lacks, and says so; a build with one can, and has
/// nothing to add.
#[test]
fn an_unknown_type_says_what_a_build_without_qt_s_table_cannot_know() {
    let dir = project(&[("Main.qml", "import QtQuick\nItem { Sparkle {} }\n")]);
    let diagnostics = run(dir.path(), &["Main.qml"], false);
    assert_eq!(brief(&diagnostics), ["2:8 unknown-type"]);
    assert_eq!(
        diagnostics[0].message.contains(qt_types::WITHOUT_QT_TABLE),
        !qt_types::has_qt_table(),
        "{:?}",
        diagnostics[0]
    );
}

// --- lint ----------------------------------------------------------------

#[test]
fn lint_includes_everything_check_reports() {
    let checked = check("unknown_property.qml");
    let linted = lint("unknown_property.qml");
    for d in &checked {
        assert!(linted.contains(d), "{d:?} missing from lint");
    }
}

#[test]
fn a11y_unnamed_interactive_covers_mouse_areas_and_tap_handlers() {
    let diagnostics = lint("a11y.qml");
    let hits = with_code(&diagnostics, "a11y-unnamed-interactive");
    // The MouseArea with a handler and no name; not the one under a named
    // parent, not the one without a handler; the TapHandler in a plain Item.
    assert_eq!(
        hits.iter().map(|d| (d.line, d.column)).collect::<Vec<_>>(),
        [(6, 5), (24, 9)]
    );
    assert!(hits.iter().all(|d| d.severity == Severity::Warning));
}

#[test]
fn a11y_unnamed_icon_button_needs_an_icon_and_no_label() {
    let diagnostics = lint("a11y.qml");
    let hits = with_code(&diagnostics, "a11y-unnamed-icon-button");
    // The ToolButton with only an icon; not the named one, not the one with text.
    assert_eq!(
        hits.iter().map(|d| (d.line, d.column)).collect::<Vec<_>>(),
        [(29, 5)]
    );
}

#[test]
fn a11y_unnamed_image_accepts_ignored_and_named() {
    let diagnostics = lint("a11y.qml");
    let hits = with_code(&diagnostics, "a11y-unnamed-image");
    assert_eq!(
        hits.iter().map(|d| (d.line, d.column)).collect::<Vec<_>>(),
        [(43, 5)]
    );
}

#[test]
fn i18n_untranslated_string_is_a_hint_on_visible_text_only() {
    let diagnostics = lint("i18n.qml");
    let hits = with_code(&diagnostics, "i18n-untranslated-string");
    // "Hello", the placeholder, and the three tooltip/accessibility texts;
    // not qsTr, not "123", not a TextField's typed text, not a ListElement.
    assert_eq!(
        hits.iter().map(|d| (d.line, d.column)).collect::<Vec<_>>(),
        [(8, 9), (21, 9), (27, 9), (28, 9), (29, 9)]
    );
    assert!(hits.iter().all(|d| d.severity == Severity::Hint));
}

#[test]
fn i18n_qstr_non_literal_is_a_warning() {
    let diagnostics = lint("i18n.qml");
    let hits = with_code(&diagnostics, "i18n-qstr-non-literal");
    assert_eq!(hits.len(), 1, "{diagnostics:?}");
    assert_eq!((hits[0].line, hits[0].column), (17, 15));
    assert_eq!(hits[0].severity, Severity::Warning);
}

#[test]
fn a_component_built_on_a_button_is_not_judged_by_its_own_use_site() {
    // `Fancy` may give its base a text; the use site cannot tell.
    let dir = project(&[
        (
            "Fancy.qml",
            "import QtQuick.Controls\nButton { text: qsTr(\"Go\") }\n",
        ),
        (
            "Main.qml",
            "import QtQuick\nItem { Fancy { icon.source: \"a.svg\" } }\n",
        ),
    ]);
    let diagnostics = run(dir.path(), &["Main.qml"], true);
    assert!(with_code(&diagnostics, "a11y-unnamed-icon-button").is_empty());
}

// --- type_lookup ---------------------------------------------------------

fn lookup(module: &str, name: &str) -> Result<ljos_mcp_author::TypeInfo, SurfaceError> {
    QmlFrontEnd::new(fixtures()).type_lookup(module, name)
}

#[test]
fn type_lookup_describes_a_type_with_its_chain() {
    let info = lookup("QtQuick", "Rectangle").unwrap();
    assert_eq!(info.name, "Rectangle");
    assert_eq!(info.base_chain.first().map(String::as_str), Some("Item"));
    assert!(
        info.properties.contains(&"color: color".to_owned()),
        "{:?}",
        info.properties
    );
    assert_eq!(info.default_property.as_deref(), Some("data"));

    let area = lookup("QtQuick", "MouseArea").unwrap();
    assert!(
        area.signals.iter().any(|s| s.starts_with("clicked(")),
        "{:?}",
        area.signals
    );

    let text = lookup("QtQuick", "Text").unwrap();
    assert!(
        text.enums.iter().any(|e| e.starts_with("HAlignment")),
        "{:?}",
        text.enums
    );
}

#[test]
fn type_lookup_reports_attached_types() {
    let layout = lookup("QtQuick.Layouts", "Layout").unwrap();
    assert!(layout.attached.is_some(), "{layout:?}");
}

#[test]
fn type_lookup_unknown_type_suggests_names() {
    let err = lookup("QtQuick", "Rectangel").unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let details = err.details.unwrap().to_string();
    assert!(details.contains("Rectangle"), "{details}");
}

#[test]
fn type_lookup_unknown_module_is_not_found() {
    let err = lookup("QtQuik", "Item").unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.details.unwrap().to_string().contains("QtQuick"));
}

/// The answer is "not found" in either build, never "unavailable": a build
/// always has a table. Without Qt's the message says what it cannot know.
#[test]
fn a_type_that_is_not_found_says_what_a_build_without_qt_s_table_cannot_know() {
    let err = lookup("QtQuick", "Sparkle").unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(
        err.message.contains(qt_types::WITHOUT_QT_TABLE),
        !qt_types::has_qt_table(),
        "{}",
        err.message
    );
}

#[test]
fn module_types_lists_the_types_of_a_module_sorted() {
    let front = QmlFrontEnd::new(fixtures());
    let info = front.module_types("QtQuick").unwrap();
    assert_eq!(info.module, "QtQuick");
    for name in ["Item", "Rectangle", "MouseArea"] {
        assert!(info.types.iter().any(|t| t == name), "{name}");
    }
    let mut sorted = info.types.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(info.types, sorted);
    // Every one of them can be looked up.
    assert!(front.type_lookup("QtQuick", "Item").is_ok());
}

#[test]
fn module_types_of_an_unknown_module_suggests_modules() {
    let err = QmlFrontEnd::new(fixtures())
        .module_types("QtQuik.Controls")
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let details = err.details.unwrap().to_string();
    assert!(details.contains("QtQuick.Controls"), "{details}");
    // The suggestions come from the table, not from a fixed list: a module
    // the old list lacked is found.
    let err = QmlFrontEnd::new(fixtures())
        .type_lookup("Qt.labs.sharedimag", "Item")
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn a_module_that_is_not_found_says_what_a_build_without_qt_s_table_cannot_know() {
    let err = QmlFrontEnd::new(fixtures())
        .module_types("QtSparkle")
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(
        err.message.contains(qt_types::WITHOUT_QT_TABLE),
        !qt_types::has_qt_table(),
        "{}",
        err.message
    );
}

/// The modules Ljós implements are in the table of either build. (Not
/// `QtQuick.Controls`: to Qt its types are the style's, which come with it.)
#[test]
fn the_modules_ljos_implements_are_listed_in_either_build() {
    let front = QmlFrontEnd::new(fixtures());
    for module in ["QtQml", "QtQuick", "QtQuick.Layouts", "QtTest", "Ljos"] {
        let info = front.module_types(module).unwrap();
        assert!(!info.types.is_empty(), "{module}");
    }
}

// --- what is definitely missing and what is not --------------------------

#[test]
fn a_name_missing_from_a_component_that_did_not_parse_whole_is_not_reported() {
    let dir = project(&[
        // Whole: a typo on it is an error.
        ("Whole.qml", "import QtQuick\nItem { property int size }\n"),
        // Not whole: the part that did not parse may declare `more`.
        (
            "Part.qml",
            "import QtQuick\nItem {\n    property int size\n    property int = \n    property int more\n}\n",
        ),
        (
            "Main.qml",
            "import QtQuick\nItem {\n    Whole { sise: 1; onNope: 1 }\n    Part { more: 1; sise: 1; onNope: 1 }\n}\n",
        ),
    ]);
    let diagnostics = run(dir.path(), &["Main.qml"], false);
    let lines: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("unknown-"))
        .map(|d| d.line)
        .collect();
    // Line 3 (Whole): both the property and the handler. Line 4 (Part): none.
    assert_eq!(lines, [3, 3], "{diagnostics:#?}");
    // The file that did not parse still says why, in its own check.
    let own = run(dir.path(), &["Part.qml"], false);
    assert!(own.iter().any(|d| d.code == "syntax"), "{own:#?}");
}

#[test]
fn a_type_whose_members_qt_does_not_list_gets_no_unknown_property() {
    let dir = project(&[(
        "Main.qml",
        "import QtQuick\nItem {\n    Loader { anything: 1 }\n    ListModel { ListElement { anything: 1 } }\n}\n",
    )]);
    let diagnostics = run(dir.path(), &["Main.qml"], false);
    // `Loader` is an ordinary type, so its typo is reported; what a row of
    // a model has is the row's to say.
    let lines: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == "unknown-property")
        .map(|d| d.line)
        .collect();
    assert_eq!(lines, [3], "{diagnostics:#?}");
}

// --- the whole corpus ----------------------------------------------------

fn qml_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            qml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "qml") {
            out.push(path);
        }
    }
}

/// `cargo test --release -p ljos_mcp_author --test front -- --ignored --nocapture`
///
/// Qt's own documentation and examples are valid QML, so almost everything
/// reported is a false positive or a project that cannot be seen whole.
#[test]
#[ignore = "reads corpus/qtdoc; run in release for the timing"]
fn check_over_the_qtdoc_corpus_is_quick_and_quiet() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/qtdoc")
        .canonicalize()
        .expect("corpus/qtdoc (git submodule update --init)");
    let mut files = Vec::new();
    qml_files(&root, &mut files);
    files.sort();
    let started = std::time::Instant::now();
    let diagnostics = QmlFrontEnd::new(&root).check(&files).unwrap();
    let elapsed = started.elapsed();
    let mut by_code = std::collections::BTreeMap::<&str, usize>::new();
    for d in &diagnostics {
        *by_code.entry(d.code.as_str()).or_default() += 1;
    }
    eprintln!("{} files in {elapsed:?}: {by_code:?}", files.len());
    assert!(files.len() > 400);
    assert!(elapsed.as_secs() < 10, "{elapsed:?}");
    // What Qt ships must not be called wrong: no property, signal or name error.
    for code in [
        "unknown-property",
        "unknown-signal",
        "unresolved-name",
        "syntax",
    ] {
        assert_eq!(by_code.get(code), None, "{code}: {diagnostics:#?}");
    }
}

// --- through the protocol ------------------------------------------------

#[tokio::test]
async fn tools_and_type_resource_are_served_by_the_real_front_end() {
    use std::sync::Arc;

    use ljos_mcp_author::{AuthorServer, Project};
    use ljos_mcp_core::{CapSet, glue::RmcpServer};
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ReadResourceRequestParams, ResourceContents},
    };
    use serde_json::{Value, json};

    let dir = project(&[("Main.qml", "import QtQuick\nItem { Rectngle {} }\n")]);
    let root = dir.path().canonicalize().unwrap();
    let front = QmlFrontEnd::new(&root);
    let server = AuthorServer::new(Project::open(&root).unwrap(), Arc::new(front));
    let (client_io, server_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        let _ = RmcpServer::new(Arc::new(server), CapSet::EMPTY)
            .serve(server_io)
            .await;
    });
    let client = ().serve(client_io).await.expect("handshake");

    let args = json!({ "paths": ["Main.qml"] })
        .as_object()
        .cloned()
        .unwrap();
    let result = client
        .call_tool(CallToolRequestParams::new("qml_check").with_arguments(args))
        .await
        .unwrap();
    let body: Value = result.structured_content.expect("structured content");
    let first = &body["diagnostics"][0];
    assert_eq!(first["code"], "unknown-type", "{body}");
    assert_eq!(first["file"], "Main.qml");
    assert_eq!(
        (first["line"].as_u64(), first["column"].as_u64()),
        (Some(2), Some(8))
    );

    let res = client
        .read_resource(ReadResourceRequestParams::new(
            "ljos://types/QtQuick/Rectangle",
        ))
        .await
        .unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &res.contents[0] else {
        panic!("expected text");
    };
    let info: Value = serde_json::from_str(text).unwrap();
    assert_eq!(info["name"], "Rectangle");
}
