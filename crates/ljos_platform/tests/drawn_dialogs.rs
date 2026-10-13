//! The dialogs the application draws when the system shows none, held to
//! the accessibility audit in a light and a dark colour scheme, as the
//! style's own popups are (`ljos_controls/tests/audit.rs`). What a screen
//! reader finds in them (a dialog named by its title, the keyboard inside,
//! Escape closing it) is in `features/platform/dialogs.feature`.

use ljos_a11y::{ColorScheme, SystemPrefs};
use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;
use ljos_test::a11y::{Options, Report, audit};

fn engine(scheme: ColorScheme) -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    ljos_quick::set_system_prefs(
        &e,
        SystemPrefs {
            color_scheme: scheme,
            ..SystemPrefs::default()
        },
    );
    e
}

fn load(e: &Engine, source: &str) -> ObjectId {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

/// The dialog, opened, in a window as dark as the scheme.
fn run(scheme: ColorScheme, (dialog, more): (&str, &str)) -> Report {
    let source = format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nimport QtQuick.Dialogs\n\
         ApplicationWindow {{\n    width: 640; height: 480; visible: true\n    \
         {dialog} {{ id: d; title: \"Choose\"; {more} Component.onCompleted: d.open() }}\n}}\n"
    );
    let e = engine(scheme);
    let root = load(&e, &source);
    let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
    s.set_root(&e, root).unwrap();
    s.frame(&e, Time::ZERO);
    let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
    let warnings = e.take_warnings();
    assert!(warnings.is_empty(), "{dialog}: {warnings:?}");
    report
}

/// Each kind, and what it is given besides a title.
const DIALOGS: [(&str, &str); 5] = [
    ("ColorDialog", "selectedColor: \"#336699\";"),
    ("FontDialog", ""),
    ("FileDialog", ""),
    ("FolderDialog", ""),
    // Buttons no system dialog has, so it is drawn on every system.
    (
        "MessageDialog",
        "text: \"Save the changes?\"; \
         buttons: MessageDialog.Save | MessageDialog.Discard | MessageDialog.Cancel;",
    ),
];

#[test]
fn the_drawn_dialogs_are_clean_in_both_schemes() {
    for dialog in DIALOGS {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            let report = run(scheme, dialog);
            assert!(
                report.is_clean(),
                "{}, {scheme:?}: expected nothing, found:\n{report}",
                dialog.0
            );
        }
    }
}
