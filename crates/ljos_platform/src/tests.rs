//! What the engine sees of the services, without QML: the scenarios in
//! `features/platform/` cover the rest through the BDD runner.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId, Value};

use crate::{
    DialogKind, FakeDialogs, FileDialog, MessageDialog, PathEnv, Platform, Settings, flush_writes,
    register,
};

fn engine(home: &std::path::Path) -> Engine {
    let e = Engine::new();
    register(&e);
    let platform = Platform::of(&e);
    platform.set_env(PathEnv::sandbox("/home/user", home));
    platform.set_application("Example", "Notes");
    e.take_log();
    e
}

fn make<T: ljos_engine::QmlType + Default>(e: &Engine) -> ObjectId {
    let o = e
        .create_native_incomplete(T::default(), None)
        .expect("made");
    e.complete(o).expect("completed");
    o
}

fn counter(e: &Engine, o: ObjectId, signal: &str) -> Rc<Cell<u32>> {
    let n = Rc::new(Cell::new(0));
    let seen = Rc::clone(&n);
    e.connect_by_name(o, signal, move |_, _| seen.set(seen.get() + 1))
        .expect("connected");
    n
}

#[test]
fn qt_platform_names_the_system_and_the_window_system() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    Platform::of(&e).set_plugin_name("wayland");
    let os = e.eval_js("Qt.platform.os").expect("evaluated");
    assert_eq!(os.to_js_string().as_str(), crate::os());
    let plugin = e.eval_js("Qt.platform.pluginName").expect("evaluated");
    assert_eq!(plugin.to_js_string().as_str(), "wayland");
}

#[test]
fn a_stored_value_is_in_the_file_and_read_by_the_next_run() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let e = engine(dir.path());
        let s = make::<Settings>(&e);
        e.invoke_by_name(s, "setValue", &[Value::from("width"), Value::Int(640)])
            .expect("set");
        e.invoke_by_name(s, "sync", &[]).expect("synced");
        Platform::of(&e).finish(&e);
    }
    flush_writes();
    let file = dir.path().join("home/user/.config/Example/Notes.conf");
    let written = std::fs::read_to_string(&file).expect("the settings file");
    assert!(written.contains("[General]"), "{written}");
    assert!(written.contains("width=640"), "{written}");

    let e = engine(dir.path());
    let s = make::<Settings>(&e);
    let v = e
        .invoke_by_name(s, "value", &[Value::from("width"), Value::Int(0)])
        .expect("read");
    assert_eq!(v.to_number(), 640.0);
}

#[test]
fn a_file_dialog_asks_the_system_and_the_answer_arrives_as_a_message() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    e.write_by_name(d, "title", Value::from("Open"))
        .expect("titled");
    let accepted = counter(&e, d, "accepted");
    e.invoke_by_name(d, "open", &[]).expect("opened");

    assert_eq!(
        fake.shown(),
        vec![(DialogKind::OpenFile, "Open".to_owned())]
    );
    assert!(e.read_by_name(d, "visible").to_boolean());
    assert!(fake.choose(vec!["/home/user/notes.txt".into()]));
    // The answer waits for the Engine thread's next frame.
    assert_eq!(accepted.get(), 0);
    platform.finish(&e);
    assert_eq!(accepted.get(), 1, "{:?}", e.take_log());
    assert_eq!(
        e.read_by_name(d, "selectedFile").to_js_string().as_str(),
        "file:///home/user/notes.txt"
    );
    assert!(!e.read_by_name(d, "visible").to_boolean());
}

#[test]
fn a_dismissed_system_dialog_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    let rejected = counter(&e, d, "rejected");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    assert!(fake.dismiss());
    platform.finish(&e);
    assert_eq!(rejected.get(), 1);
    assert_eq!(platform.pending(), 0);
}

#[test]
fn a_message_the_system_shows_reports_the_button_pressed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<MessageDialog>(&e);
    e.write_by_name(d, "buttons", Value::Int(crate::OK | crate::CANCEL))
        .expect("buttons");
    let clicked = Rc::new(Cell::new(0));
    let seen = Rc::clone(&clicked);
    e.connect_by_name(d, "buttonClicked", move |_, args| {
        seen.set(args.first().map_or(0, |v| v.to_number() as i32));
    })
    .expect("connected");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    assert_eq!(
        fake.shown().first().map(|(k, _)| *k),
        Some(DialogKind::Message)
    );
    assert!(fake.answer("OK"));
    platform.finish(&e);
    assert_eq!(clicked.get(), crate::OK);
}

#[test]
fn closing_a_dialog_takes_it_off_the_screen_and_rejects_it_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    let rejected = counter(&e, d, "rejected");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    e.invoke_by_name(d, "close", &[]).expect("closed");
    assert!(fake.shown().is_empty());
    platform.finish(&e);
    assert_eq!(rejected.get(), 1);
    e.invoke_by_name(d, "close", &[]).expect("closed again");
    assert_eq!(rejected.get(), 1);
}

#[test]
fn a_dialog_the_system_cannot_take_away_is_closed_for_qml_and_its_answer_dropped() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    fake.cannot_close();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    let rejected = counter(&e, d, "rejected");
    let accepted = counter(&e, d, "accepted");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    e.invoke_by_name(d, "close", &[]).expect("closed");
    assert_eq!(rejected.get(), 1);
    assert!(!e.read_by_name(d, "visible").to_boolean());
    assert_eq!(fake.shown().len(), 1, "still on screen");
    assert!(fake.choose(vec!["/home/user/late.txt".into()]));
    platform.finish(&e);
    assert_eq!(accepted.get(), 0);
    assert_eq!(
        e.read_by_name(d, "selectedFile").to_js_string().as_str(),
        ""
    );
    assert_eq!(platform.pending(), 0);
}

#[test]
fn desktop_linux_grants_every_permission_and_asking_changes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let p = make::<crate::LocationPermission>(&e);
    assert_eq!(e.read_by_name(p, "status").to_number(), 1.0);
    let changed = counter(&e, p, "statusChanged");
    e.invoke_by_name(p, "request", &[]).expect("requested");
    Platform::of(&e).finish(&e);
    assert_eq!(changed.get(), 0);
}

#[test]
fn a_system_that_asks_tells_the_answer_later() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = crate::FakePermissions::default();
    let platform = Platform::of(&e);
    platform.set_permissions(Rc::new(fake.clone()));
    let p = make::<crate::CameraPermission>(&e);
    assert_eq!(e.read_by_name(p, "status").to_number(), 0.0);
    let changed = counter(&e, p, "statusChanged");
    e.invoke_by_name(p, "request", &[]).expect("requested");
    assert_eq!(fake.asked(), vec!["camera"]);
    assert!(fake.answer("camera", false));
    assert_eq!(changed.get(), 0, "the answer waits for a frame");
    platform.finish(&e);
    assert_eq!(changed.get(), 1);
    assert_eq!(e.read_by_name(p, "status").to_number(), 2.0);
    // The system remembers: a new object reads the answer at once.
    let again = make::<crate::CameraPermission>(&e);
    assert_eq!(e.read_by_name(again, "status").to_number(), 2.0);
}

#[test]
fn a_drop_target_files_cannot_reach_is_warned_of_once_in_either_order() {
    let warnings = |e: &Engine| {
        e.take_log()
            .iter()
            .filter(|r| r.message.contains("cannot reach a DropArea"))
            .count()
    };
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let p = Platform::of(&e);
    p.note_external_drop_target(&e);
    assert_eq!(warnings(&e), 0, "files may yet come");
    p.set_external_drops_unavailable(&e, "no drag-and-drop here");
    p.note_external_drop_target(&e);
    p.set_external_drops_unavailable(&e, "no drag-and-drop here");
    assert_eq!(warnings(&e), 1);

    let e = engine(dir.path());
    let p = Platform::of(&e);
    p.set_external_drops_unavailable(&e, "no drag-and-drop here");
    assert_eq!(warnings(&e), 0, "no target yet");
    p.note_external_drop_target(&e);
    p.note_external_drop_target(&e);
    assert_eq!(warnings(&e), 1);
}
