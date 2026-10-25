//! What the scenarios in `features/controls/` cannot see of the value
//! controls: that the animations of the Basic style stop when there is
//! nothing to show, so an idle window costs nothing.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{PointerEvent, QuickScene};
use ljos_render::kurbo;
use ljos_runtime::Time;

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e.take_log();
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

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

/// A scene of `body` in an item, with its first frame drawn.
fn start(body: &str) -> (Engine, QuickScene, ObjectId) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            "import QtQuick\nimport QtQuick.Controls.Basic\nItem {{\n    \
             width: 400; height: 400\n{body}\n}}\n"
        ),
    );
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    s.set_root(&e, root).unwrap();
    s.frame(&e, Time::ZERO);
    (e, s, root)
}

/// Frames from `from` to `to`, one every 16 ms, as a display would ask
/// for them while something moves.
fn run(s: &mut QuickScene, e: &Engine, from: u64, to: u64) {
    let mut t = from;
    while t <= to {
        s.frame(e, Time::from_millis(t));
        t += 16;
    }
}

/// Nothing moves and nothing will: no animation, no frame, no wake.
#[track_caller]
fn assert_idle(s: &mut QuickScene, e: &Engine, now: u64, when: &str) {
    let out = s.frame(e, Time::from_millis(now));
    assert_eq!(ljos_quick::running_animation_count(e), 0, "{when}");
    assert!(!s.needs_frame(), "a frame {when}");
    assert_eq!(out.next_wake, None, "a wake {when}");
}

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v).unwrap();
}

#[test]
fn a_busy_indicator_moves_only_while_it_runs_and_is_shown() {
    let (e, mut s, root) = start(r#"BusyIndicator { objectName: "busy" }"#);
    let busy = find(&e, root, "busy");
    run(&mut s, &e, 0, 100);
    assert!(ljos_quick::running_animation_count(&e) > 0, "running");

    // It fades out, then stops.
    set(&e, busy, "running", Value::Bool(false));
    run(&mut s, &e, 116, 600);
    assert_idle(&mut s, &e, 616, "after it stopped running");

    set(&e, busy, "running", Value::Bool(true));
    run(&mut s, &e, 632, 1000);
    assert!(ljos_quick::running_animation_count(&e) > 0, "running again");

    set(&e, busy, "visible", Value::Bool(false));
    run(&mut s, &e, 1016, 1500);
    assert_idle(&mut s, &e, 1516, "while hidden");
}

#[test]
fn an_indeterminate_progress_bar_moves_only_while_it_is_shown() {
    let (e, mut s, root) = start(
        r#"ProgressBar { objectName: "bar"; indeterminate: true }
           ProgressBar { y: 40; value: 0.5 }"#,
    );
    let bar = find(&e, root, "bar");
    run(&mut s, &e, 0, 100);
    assert!(ljos_quick::running_animation_count(&e) > 0, "indeterminate");

    set(&e, bar, "indeterminate", Value::Bool(false));
    run(&mut s, &e, 116, 200);
    assert_idle(&mut s, &e, 216, "with a value");

    set(&e, bar, "indeterminate", Value::Bool(true));
    run(&mut s, &e, 232, 300);
    assert!(
        ljos_quick::running_animation_count(&e) > 0,
        "indeterminate again"
    );

    set(&e, bar, "visible", Value::Bool(false));
    run(&mut s, &e, 316, 400);
    assert_idle(&mut s, &e, 416, "while hidden");
}

#[test]
fn a_delay_button_is_still_once_its_progress_is_full() {
    let (e, mut s, root) = start(r#"DelayButton { objectName: "b"; delay: 200 }"#);
    let b = find(&e, root, "b");
    s.pointer_event(&e, &PointerEvent::press(20.0, 20.0, Time::ZERO));
    run(&mut s, &e, 0, 300);
    assert_eq!(e.read_by_name(b, "progress").to_number(), 1.0);
    assert_idle(&mut s, &e, 316, "while held at full");

    s.pointer_event(
        &e,
        &PointerEvent::release(20.0, 20.0, Time::from_millis(320)),
    );
    assert!(e.read_by_name(b, "checked").to_boolean());
    assert_idle(&mut s, &e, 336, "once checked");
}

#[test]
fn a_page_indicator_is_still_once_its_dots_have_faded() {
    let (e, mut s, root) = start(r#"PageIndicator { objectName: "p"; count: 4 }"#);
    let p = find(&e, root, "p");
    assert_idle(&mut s, &e, 0, "at first");
    set(&e, p, "currentIndex", Value::Int(2));
    run(&mut s, &e, 16, 300);
    assert_idle(&mut s, &e, 316, "after the change");
}
