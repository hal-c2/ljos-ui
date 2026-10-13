//! Live regions as a scene has them: a status, a notification and an alert
//! written in the Basic style are live nodes, what they say next reaches
//! the AccessKit node, and the accessibility audit finds nothing wrong with
//! them in a light or a dark colour scheme.

use ljos_a11y::accesskit::{Live, Role as AkRole};
use ljos_a11y::{ColorScheme, NodeKey, Politeness, SystemPrefs};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;
use ljos_test::a11y::{Options, audit};

const SCHEMES: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];

const BODY: &str = r##"
    property string message: "Saving"
    Column {
        x: 8; y: 8; spacing: 8
        Label {
            text: message
            Accessible.role: Accessible.StatusBar
            Accessible.name: text
        }
        Label {
            text: "3 new"
            Accessible.role: Accessible.Notification
            Accessible.name: text
        }
        Label {
            text: "Disk full"
            Accessible.role: Accessible.AlertMessage
            Accessible.name: text
        }
        Button { text: "Save" }
    }"##;

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

/// Compiles `source` as `Main.qml` and creates it.
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

/// The body in a window that is as dark as the scheme, with an assistive
/// technology listening.
fn mount(scheme: ColorScheme) -> (Engine, QuickScene, ObjectId) {
    let source = format!(
        "import QtQuick\nimport QtQuick.Controls.Basic\nApplicationWindow {{\n    \
         width: 640; height: 480; visible: true\n{BODY}\n}}\n"
    );
    let e = engine(scheme);
    let root = load(&e, &source);
    let mut s = QuickScene::new(kurbo::Size::new(640.0, 480.0));
    s.set_root(&e, root).unwrap();
    s.set_accessibility_active(&e, true);
    s.frame(&e, Time::ZERO);
    (e, s, root)
}

fn find(s: &QuickScene, role: AkRole, name: &str) -> NodeKey {
    s.accessibility_tree()
        .find(role, name)
        .unwrap_or_else(|| panic!("no {role:?} named {name:?}"))
}

#[test]
fn live_texts_are_clean_in_both_schemes() {
    for scheme in SCHEMES {
        let (e, mut s, _) = mount(scheme);
        let report = audit(&e, &mut s, Time::ZERO, &Options::desktop());
        assert!(
            report.is_clean(),
            "{scheme:?}: expected nothing, found:\n{report}"
        );
        let warnings = e.take_warnings();
        assert!(warnings.is_empty(), "{scheme:?}: {warnings:?}");
    }
}

#[test]
fn each_live_text_is_a_live_accesskit_node() {
    for scheme in SCHEMES {
        let (_e, s, _) = mount(scheme);
        let tree = s.accessibility_tree();
        for (role, name, live) in [
            (AkRole::Status, "Saving", Live::Polite),
            (AkRole::Status, "3 new", Live::Polite),
            (AkRole::Alert, "Disk full", Live::Assertive),
        ] {
            let node = tree.accesskit_node(find(&s, role, name)).unwrap();
            assert_eq!(node.live(), Some(live), "{scheme:?}: {name}");
            assert!(node.is_live_atomic(), "{scheme:?}: {name}");
        }
        let save = tree
            .accesskit_node(find(&s, AkRole::Button, "Save"))
            .unwrap();
        assert_eq!(save.live(), None);
    }
}

#[test]
fn what_a_status_says_next_is_a_change_of_its_node() {
    let (e, mut s, root) = mount(ColorScheme::Light);
    let key = find(&s, AkRole::Status, "Saving");
    let _ = s.accessibility_tree_mut().take_changes();
    e.write_by_name(root, "message", Value::from("Saved"))
        .expect("message");
    s.frame(&e, Time::from_millis(16));
    let changes = s.accessibility_tree_mut().take_changes();
    assert!(changes.changed.contains(&key), "{changes:?}");
    let node = s.accessibility_tree().accesskit_node(key).unwrap();
    assert_eq!(node.label(), Some("Saved"));
    assert_eq!(node.live(), Some(Live::Polite));
}

#[test]
fn an_announcement_in_the_scene_is_live_as_asked() {
    let (_e, mut s, _) = mount(ColorScheme::Dark);
    let tree = s.accessibility_tree_mut();
    let polite = tree.announce("Copied", Politeness::Polite);
    let assertive = tree.announce("Offline", Politeness::Assertive);
    let live = |k| tree.accesskit_node(k).and_then(|n| n.live());
    assert_eq!(live(polite), Some(Live::Polite));
    assert_eq!(live(assertive), Some(Live::Assertive));
}
