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
fn a_dialog_one_engine_closes_leaves_another_engines_shown() {
    // One system for the whole process, as a phone's picker is: a ticket
    // names one engine's dialog among every engine's.
    let dir = tempfile::tempdir().expect("temp dir");
    let fake = FakeDialogs::default();
    let open = |title: &str| {
        let e = engine(dir.path());
        Platform::of(&e).set_dialogs(Rc::new(fake.clone()));
        let d = make::<FileDialog>(&e);
        e.write_by_name(d, "title", Value::from(title))
            .expect("titled");
        e.invoke_by_name(d, "open", &[]).expect("opened");
        (e, d)
    };
    let (_first, _) = open("First");
    let (second, d) = open("Second");
    second.invoke_by_name(d, "close", &[]).expect("closed");
    assert_eq!(
        fake.shown(),
        vec![(DialogKind::OpenFile, "First".to_owned())]
    );
}

#[test]
fn a_document_to_save_is_typed_by_the_selected_name_filter() {
    let dir = tempfile::tempdir().expect("temp dir");
    // The types the Java side hands Android's picker: a save creates one
    // document, of its selected filter's type; an open offers every filter's.
    let offered = |mode: i32, index: i32| {
        let e = engine(dir.path());
        let fake = FakeDialogs::default();
        Platform::of(&e).set_dialogs(Rc::new(fake.clone()));
        let d = make::<FileDialog>(&e);
        let filters = Value::list(vec![
            Value::from("PNG (*.png)"),
            Value::from("JPEG (*.jpg)"),
        ]);
        e.write_by_name(d, "nameFilters", filters)
            .expect("filtered");
        e.write_by_name(d, "fileMode", Value::Int(mode))
            .expect("moded");
        e.write_by_name(d, "selectedNameFilter.index", Value::Int(index))
            .expect("selected");
        e.invoke_by_name(d, "open", &[]).expect("opened");
        let request = fake.request().expect("shown");
        crate::android_documents::type_extensions(&request)
    };
    let (open, save) = (0, 2);
    assert_eq!(offered(save, 1), ["jpg"]);
    assert_eq!(offered(save, 0), ["png"]);
    // None chosen, or one beyond the list, is the first.
    assert_eq!(offered(save, -1), ["png"]);
    assert_eq!(offered(save, 7), ["png"]);
    // An open creates nothing: every filter's, in the filters' order.
    assert_eq!(offered(open, 1), ["png", "jpg"]);
}

#[test]
fn a_second_save_suggests_the_name_the_first_did() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    e.write_by_name(d, "fileMode", Value::Int(2)).expect("save");
    let suggested = |file: Option<&str>| {
        if let Some(file) = file {
            e.write_by_name(d, "currentFile", Value::from(file))
                .expect("named");
        }
        e.invoke_by_name(d, "open", &[]).expect("opened");
        let name = fake.request().expect("shown").file_name;
        // A provider's document says nothing of its name.
        assert!(fake.choose_documents(vec!["content://docs/document/31".to_owned()]));
        platform.finish(&e);
        name
    };
    let report = Some("report.txt".to_owned());
    assert_eq!(suggested(Some("file:///report.txt")), report);
    assert_eq!(suggested(None), report);
    assert_eq!(suggested(None), report);
    let notes = Some("notes.md".to_owned());
    assert_eq!(suggested(Some("file:///notes.md")), notes);
    assert_eq!(suggested(None), notes);
}

#[test]
fn a_document_the_system_hands_out_is_selected_by_its_url() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    e.write_by_name(d, "fileMode", Value::Int(2)).expect("save");
    let accepted = counter(&e, d, "accepted");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    let url = "content://com.android.providers.downloads.documents/document/msf%3A31";
    assert!(fake.choose_documents(vec![url.to_owned()]));
    platform.finish(&e);
    assert_eq!(accepted.get(), 1, "{:?}", e.take_log());
    assert_eq!(
        e.read_by_name(d, "selectedFile").to_js_string().as_str(),
        url
    );
    assert_eq!(
        e.read_by_name(d, "currentFile").to_js_string().as_str(),
        url
    );
    assert!(fake.unheard().is_empty(), "a heard answer went back");
}

#[test]
fn documents_that_arrive_after_the_dialog_closed_go_back_to_the_system() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    let accepted = counter(&e, d, "accepted");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    let urls = vec!["file:///cache/documents/1/0/notes.txt".to_owned()];
    // The system's answer is on its way when QML closes the dialog.
    assert!(fake.choose_documents(urls.clone()));
    e.invoke_by_name(d, "close", &[]).expect("closed");
    platform.finish(&e);
    assert_eq!(accepted.get(), 0);
    assert_eq!(
        fake.unheard(),
        vec![crate::dialogs::DialogReply::Documents(urls)]
    );
}

#[test]
fn documents_that_arrive_after_the_dialog_was_destroyed_go_back_to_the_system() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    e.invoke_by_name(d, "open", &[]).expect("opened");
    let urls = vec!["file:///cache/documents/2/0/notes.txt".to_owned()];
    assert!(fake.choose_documents(urls.clone()));
    assert!(e.destroy_now(d));
    platform.finish(&e);
    assert_eq!(
        fake.unheard(),
        vec![crate::dialogs::DialogReply::Documents(urls)]
    );
}

#[test]
fn a_system_that_hands_out_no_document_rejects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = FakeDialogs::default();
    let platform = Platform::of(&e);
    platform.set_dialogs(Rc::new(fake.clone()));
    let d = make::<FileDialog>(&e);
    let rejected = counter(&e, d, "rejected");
    e.invoke_by_name(d, "open", &[]).expect("opened");
    assert!(fake.choose_documents(Vec::new()));
    platform.finish(&e);
    assert_eq!(rejected.get(), 1, "{:?}", e.take_log());
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
fn the_notification_permission_is_asked_for_on_its_own() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = crate::FakePermissions::default();
    let platform = Platform::of(&e);
    platform.set_permissions(Rc::new(fake.clone()));
    let p = make::<crate::NotificationPermission>(&e);
    let camera = make::<crate::CameraPermission>(&e);
    assert_eq!(e.read_by_name(p, "status").to_number(), 0.0);
    e.invoke_by_name(p, "request", &[]).expect("requested");
    assert_eq!(fake.asked(), vec!["notifications"]);
    assert!(fake.answer("notifications", true));
    platform.finish(&e);
    assert_eq!(e.read_by_name(p, "status").to_number(), 1.0);
    assert_eq!(e.read_by_name(camera, "status").to_number(), 0.0);
}

/// The badge's count, set before the user let the application show
/// notifications, shown again once they do (on Android the badge is a
/// notification's number); a denial shows nothing more.
#[test]
fn the_badge_is_shown_again_once_notifications_are_granted() {
    for (granted, shown) in [(true, vec![3, 3]), (false, vec![3])] {
        let dir = tempfile::tempdir().expect("temp dir");
        let e = engine(dir.path());
        let fake = crate::FakePermissions::default();
        let platform = Platform::of(&e);
        platform.set_permissions(Rc::new(fake.clone()));
        let badge = crate::RecordedBadge::default();
        platform.set_badge(Rc::new(badge.clone()));
        platform.set_badge_number(3);
        let p = make::<crate::NotificationPermission>(&e);
        e.invoke_by_name(p, "request", &[]).expect("requested");
        assert!(fake.answer("notifications", granted));
        platform.finish(&e);
        assert_eq!(badge.counts(), shown, "granted: {granted}");
    }
}

/// The badge is the application's, not the asking object's: a grant that
/// comes after the object went still shows the count again.
#[test]
fn the_badge_is_shown_again_when_the_asking_object_is_gone() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let fake = crate::FakePermissions::default();
    let platform = Platform::of(&e);
    platform.set_permissions(Rc::new(fake.clone()));
    let badge = crate::RecordedBadge::default();
    platform.set_badge(Rc::new(badge.clone()));
    platform.set_badge_number(3);
    let p = make::<crate::NotificationPermission>(&e);
    e.invoke_by_name(p, "request", &[]).expect("requested");
    assert!(e.destroy(p));
    e.process_deferred_deletes();
    assert!(!e.is_alive(p), "the object is gone before the answer");
    assert!(fake.answer("notifications", true));
    platform.finish(&e);
    assert_eq!(badge.counts(), vec![3, 3]);
}

/// Only a permission newly granted shows the badge again: one granted
/// from the start, asked for again, adds nothing.
#[test]
fn a_granted_notification_permission_asked_again_shows_the_badge_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let platform = Platform::of(&e);
    let badge = crate::RecordedBadge::default();
    platform.set_badge(Rc::new(badge.clone()));
    platform.set_badge_number(2);
    let p = make::<crate::NotificationPermission>(&e);
    e.invoke_by_name(p, "request", &[]).expect("requested");
    platform.finish(&e);
    assert_eq!(badge.counts(), vec![2]);
}

/// A system whose answers the test sends when it likes, and that grants
/// nothing it is asked of.
#[derive(Default)]
struct Holding(std::cell::RefCell<Vec<crate::PermissionAnswer>>);

impl crate::PermissionService for Holding {
    fn status(&self, _: crate::Permission) -> crate::PermissionStatus {
        crate::PermissionStatus::Undetermined
    }

    fn request(&self, _: crate::Permission, answer: crate::PermissionAnswer) {
        self.0.borrow_mut().push(answer);
    }
}

#[test]
fn an_answer_to_what_a_permission_asked_before_it_changed_is_not_its_status() {
    let dir = tempfile::tempdir().expect("temp dir");
    let e = engine(dir.path());
    let system = Rc::new(Holding::default());
    let platform = Platform::of(&e);
    platform.set_permissions(system.clone());
    let p = make::<crate::LocationPermission>(&e);
    e.invoke_by_name(p, "request", &[]).expect("requested");
    // Asked in use, it asks for always before the user answers.
    e.write_by_name(p, "availability", Value::Int(1))
        .expect("always");
    let answer = system.0.borrow_mut().pop().expect("asked");
    answer.send(crate::PermissionStatus::Granted);
    platform.finish(&e);
    assert_eq!(
        e.read_by_name(p, "status").to_number(),
        0.0,
        "what the system says of always"
    );
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

#[test]
fn a_window_shows_its_own_icon_or_else_the_applications() {
    let e = Engine::new();
    register(&e);
    let (a, b) = (make::<FileDialog>(&e), make::<FileDialog>(&e));
    let platform = Platform::of(&e);
    let pixel = |v: u8| crate::Icon::from_rgba(1, 1, vec![v, v, v, 255]).unwrap();
    let seen = platform.icons_changed();
    assert_eq!(platform.window_icon(Some(a)), None);
    platform.set_application_icon(Some(pixel(1)));
    platform.set_window_icon(a, Some(pixel(2)));
    assert_eq!(platform.window_icon(Some(a)), Some(pixel(2)));
    assert_eq!(platform.window_icon(Some(b)), Some(pixel(1)));
    assert_eq!(platform.window_icon(None), Some(pixel(1)));
    platform.set_window_icon(a, None);
    assert_eq!(platform.window_icon(Some(a)), Some(pixel(1)));
    platform.set_application_icon(None);
    assert_eq!(platform.window_icon(Some(a)), None);
    assert_eq!(platform.icons_changed(), seen + 4);
}
