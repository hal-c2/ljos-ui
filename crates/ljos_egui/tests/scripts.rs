//! A view prepares the scripts its scene may run on input in the pass
//! after its first frame: that frame is not kept waiting for the JS host,
//! and the first click runs its handler and not the start of the host too.

use std::rc::Rc;

use egui::Vec2;
use egui_kittest::Harness;
use ljos_egui::QmlView;
use ljos_engine::{Engine, ObjectId};

const HANDLER: &str = r#"
import QtQuick
Rectangle {
    width: 100; height: 100
    property int n: 0
    MouseArea { anchors.fill: parent; onClicked: n++ }
}
"#;

fn load(source: &str) -> (Rc<Engine>, ObjectId) {
    use ljos_compiler::{Project, compile_file};
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let mut project = Project::new(qml_front::QtTypeOracle);
    project.add("Main", source);
    let c = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    assert!(!c.has_errors(), "{:?}", c.diagnostics);
    let mut files = ljos_engine::MapSource::new();
    files.insert("Main", c.file);
    e.set_component_source(files);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    (e, root)
}

struct State {
    view: QmlView,
    engine: Rc<Engine>,
    /// Whether the JS host had started when each pass ended.
    started: Vec<bool>,
}

#[test]
fn the_scripts_are_prepared_in_the_pass_after_the_first_frame_once() {
    let (e, root) = load(HANDLER);
    let view = QmlView::new(Rc::clone(&e), root).unwrap();
    let state = State {
        view,
        engine: Rc::clone(&e),
        started: Vec::new(),
    };
    // Building the harness runs passes until egui asks for no more.
    let mut h = Harness::builder()
        .with_size(Vec2::new(100.0, 100.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.view.show(ui);
                st.started.push(st.engine.js_started());
            },
            state,
        );
    let started = &h.state().started;
    assert!(!started[0], "the first frame waited for the JS host");
    assert!(started[1], "{started:?}");

    // Then the view is idle again: preparing asked for one pass only.
    let settled = h.state().view.stats();
    h.step();
    h.step();
    assert_eq!(
        h.state().view.stats().repaint_requests,
        settled.repaint_requests
    );
}
