//! A `ShellLoader` that tries its source again from its fallback holds that
//! fallback until the new shell's first read is over, as that read is where
//! a shell that was made can still fail: a fault by then shows the same
//! fallback again, and each other end destroys it.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{PointerEvent, QuickScene};
use ljos_runtime::{Fault, Time, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Rectangle {
    id: root
    width: 300; height: 200
    property bool ready: false
    property bool heavy: false
    property bool watching: false
    property bool choose: false
    property var chooser
    property bool greedy: false
    property string chosenWhenLetGo
    property bool reloadWhenLetGo: false
    property int shells: 0
    property string sawFirst
    property var looked: []
    property string source: "Shell"
    property string fallback: "Fallback"
    property int pressed: 0
    property int others: 0
    color: {
        if (watching && !looked.length) {
            looked.push(true)
            sawFirst = holder.children[0].error
        }
        return "white"
    }
    Item { objectName: "elsewhere"; width: 10; height: 10 }
    Item {
        id: holder
        objectName: "holder"
        ShellLoader {
            objectName: "loader"
            width: 100; height: 100
            source: root.source
            fallback: root.fallback
        }
    }
}
"#;

const SHELL: &str = r#"
import QtQuick
Rectangle {
    id: shell
    objectName: "shell"
    color: root.heavy && new ArrayBuffer(268435456).byteLength ? "red" : "blue"
    Component.onCompleted: {
        root.shells++
        if (root.choose) root.chooser = shell
        if (root.greedy) new ArrayBuffer(268435456)
        if (!root.ready) throw new Error("not ready")
        if (root.choose) root.source = "Other"
    }
    Component.onDestruction: if (root.choose) root.source = "Other"
}
"#;

const FALLBACK: &str = r#"
import QtQuick
Rectangle {
    objectName: "fallback"
    width: 100; height: 100
    color: "green"
    property int touched: 0
    Accessible.role: Accessible.Button
    Accessible.name: "The fallback"
    MouseArea { anchors.fill: parent; onPressed: root.pressed++ }
    Item { objectName: "field"; focus: true }
    Component.onDestruction: {
        if (root.chosenWhenLetGo) root.source = root.chosenWhenLetGo
        if (root.reloadWhenLetGo) holder.children[0].reload()
    }
}
"#;

/// A fallback that asks for another shell as it completes.
const CHOOSER: &str = r#"
import QtQuick
Rectangle {
    id: chooser
    objectName: "chooser"
    Component.onCompleted: {
        root.chooser = chooser
        root.source = "Other"
    }
}
"#;

const OTHER: &str = r#"
import QtQuick
Rectangle {
    objectName: "other"
    Component.onCompleted: root.others++
}
"#;

/// A loader showing its fallback, with the user's state in it, whose
/// source would work now.
struct Held {
    e: Engine,
    /// What the engine's timers read: a frame runs them up to its time.
    clock: VirtualClock,
    root: ObjectId,
    loader: ObjectId,
    fallback: ObjectId,
    errors: Rc<RefCell<Vec<String>>>,
}

impl Held {
    fn new() -> Self {
        let e = engine();
        let clock = VirtualClock::new();
        e.set_clock(Rc::new(clock.clone()));
        // As an application that works a binding out when it is read.
        e.set_eager_bindings(false);
        compile(
            &e,
            &[
                ("Main", MAIN),
                ("Shell", SHELL),
                ("Fallback", FALLBACK),
                ("Chooser", CHOOSER),
                ("Other", OTHER),
            ],
        );
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        let loader = find(&e, root, "loader");
        e.deliver_faults();
        e.process_deferred_deletes();
        assert_eq!(num(&e, loader, "status"), 2.0);
        let fallback = get(&e, loader, "item").as_object().expect("the fallback");
        set(&e, fallback, "touched", Value::from(7.0));
        set(&e, root, "ready", Value::Bool(true));
        // So that the heavy shell's binding is refused, not allocated.
        assert!(e.set_script_heap_limit(Some(64 << 20)));
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        e.connect_by_name(loader, "errorChanged", move |e, _| {
            seen.borrow_mut()
                .push(get(e, loader, "error").to_js_string().to_string());
        })
        .unwrap();
        Held {
            e,
            clock,
            root,
            loader,
            fallback,
            errors,
        }
    }

    /// The loader tries its source again, and the shell is made.
    fn retry(&self) {
        self.e.invoke_by_name(self.loader, "reload", &[]).unwrap();
        self.assert_held();
    }

    fn assert_held(&self) {
        let Held { e, loader, .. } = self;
        assert!(!e.is_going(self.fallback), "the fallback is held");
        assert_eq!(num(e, *loader, "status"), 1.0, "Ready meanwhile");
        assert_ne!(self.error(), "", "and the reason is said");
        assert!(self.errors.borrow().is_empty(), "with no signal");
    }

    fn assert_released(&self) {
        assert!(self.e.is_going(self.fallback), "the fallback goes");
        assert_eq!(num(&self.e, self.loader, "status"), 1.0);
        assert_eq!(self.error(), "");
        assert_eq!(*self.errors.borrow(), [""], "cleared with one signal");
    }

    fn assert_shown_again(&self) {
        assert_eq!(self.item(), Some(self.fallback), "the same fallback");
        assert!(!self.e.is_going(self.fallback));
        assert_eq!(num(&self.e, self.fallback, "touched"), 7.0);
        assert_eq!(num(&self.e, self.loader, "status"), 2.0);
        assert!(self.error().contains("out of memory"), "{}", self.error());
        assert!(self.errors.borrow().iter().all(|said| !said.is_empty()));
    }

    fn error(&self) -> String {
        get(&self.e, self.loader, "error")
            .to_js_string()
            .to_string()
    }

    fn item(&self) -> Option<ObjectId> {
        get(&self.e, self.loader, "item").as_object()
    }

    fn fault_in_shell(&self) {
        self.e.report_fault(self.item(), Fault::JsOutOfMemory);
    }

    fn scene(&self) -> QuickScene {
        let mut s = scene(&self.e, self.root, 300.0, 200.0);
        frame(&mut s, &self.e);
        s
    }

    /// The loader leaves every scene's trees.
    fn detach(&self) {
        let holder = find(&self.e, self.root, "holder");
        ljos_quick::set_parent_item(&self.e, holder, None).unwrap();
    }
}

/// Sets the source to `Other` once the loader shows nothing.
fn choose_other_when_nothing_is_shown(h: &Held) {
    let (root, loader) = (h.root, h.loader);
    h.e.connect_by_name(loader, "itemChanged", move |e, _| {
        if get(e, loader, "item").as_object().is_none() {
            set(e, root, "source", Value::from("Other"));
        }
    })
    .unwrap();
}

/// `Other` is shown as the source's, and is the only one that was made.
fn assert_other_shown(e: &Engine, root: ObjectId, loader: ObjectId, case: &str) {
    assert_eq!(num(e, loader, "status"), 1.0, "{case}: Ready");
    assert_eq!(
        get(e, loader, "error").to_js_string().as_str(),
        "",
        "{case}"
    );
    let item = get(e, loader, "item").as_object().expect("the one chosen");
    assert!(!e.is_going(item), "{case}");
    let name = get(e, item, "objectName").to_js_string();
    assert_eq!(name.as_str(), "other", "{case}");
    assert_eq!(num(e, root, "others"), 1.0, "{case}: one shell");
}

/// `Other` stays shown once the faults are delivered, and what asked for it
/// goes.
fn assert_other_replaced_the_chooser(h: &Held, case: &str) {
    assert_other_shown(&h.e, h.root, h.loader, case);
    let chooser = get(&h.e, h.root, "chooser")
        .as_object()
        .expect("the one that chose");
    assert!(h.e.is_going(chooser), "{case}: it is not kept");
    h.e.deliver_faults();
    assert_other_shown(&h.e, h.root, h.loader, case);
}

/// Sets the source to `Other` as `property` first says a shell failed.
fn choose_other_as_it_fails(h: &Held, property: &'static str) {
    let (root, loader) = (h.root, h.loader);
    h.e.connect_by_name(loader, &format!("{property}Changed"), move |e, _| {
        let failed = match property {
            "status" => num(e, loader, property) > 1.0,
            _ => get(e, loader, property)
                .to_js_string()
                .to_string()
                .contains("out of memory"),
        };
        if failed {
            set(e, root, "source", Value::from("Other"));
        }
    })
    .unwrap();
}

/// `Other` is shown, and nothing waits to try another source again.
fn assert_other_shown_and_no_wait(h: &Held, case: &str) {
    assert_other_shown(&h.e, h.root, h.loader, case);
    assert!(h.e.next_timer_deadline().is_none(), "{case}: no backoff");
}

/// Sets the source to `Other` as the log first says `said`.
fn choose_other_as_it_is_logged(e: &Engine, root: ObjectId, said: &'static str) {
    let (engine, mut chosen) = (e.clone(), false);
    e.add_log_listener(move |entry| {
        if !chosen && entry.message.contains(said) {
            chosen = true;
            set(&engine, root, "source", Value::from("Other"));
        }
    });
}

fn frame_when_due(s: &mut QuickScene, h: &Held) {
    let due = h.e.next_timer_deadline().expect("a timer");
    h.clock.set(due);
    s.frame(&h.e, due);
}

/// The items `itemChanged` told of, with a `reload()` from the first.
fn reload_when_the_item_changes(h: &Held) -> Rc<RefCell<Vec<Option<ObjectId>>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (items, loader) = (seen.clone(), h.loader);
    h.e.connect_by_name(loader, "itemChanged", move |e, _| {
        let first = items.borrow().is_empty();
        items.borrow_mut().push(get(e, loader, "item").as_object());
        if first {
            e.invoke_by_name(loader, "reload", &[]).unwrap();
        }
    })
    .unwrap();
    seen
}

#[test]
fn a_reload_from_item_changed_as_a_hold_starts_makes_no_second_shell() {
    let h = Held::new();
    let seen = reload_when_the_item_changes(&h);
    h.retry();
    assert_eq!(*seen.borrow(), [h.item()], "one shell");
}

#[test]
fn a_reload_from_item_changed_where_nothing_was_shown_makes_no_second_shell() {
    let h = Held::new();
    set(&h.e, h.root, "ready", Value::Bool(false));
    set(&h.e, h.root, "fallback", Value::from(""));
    assert_eq!(num(&h.e, h.loader, "status"), 3.0, "nothing to show");
    set(&h.e, h.root, "ready", Value::Bool(true));
    let seen = reload_when_the_item_changes(&h);
    h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
    assert_eq!(num(&h.e, h.loader, "status"), 1.0);
    assert_eq!(*seen.borrow(), [h.item()], "one shell");
}

#[test]
fn a_source_chosen_as_a_failed_shell_is_dropped_is_not_replaced_by_the_fallback() {
    let h = Held::new();
    h.retry();
    h.e.deliver_faults();
    h.assert_released();
    choose_other_when_nothing_is_shown(&h);
    h.fault_in_shell();
    h.e.deliver_faults();
    assert_other_shown(&h.e, h.root, h.loader, "stop");
}

#[test]
fn a_source_chosen_as_a_fallback_is_dropped_for_another_is_made_once() {
    let h = Held::new();
    choose_other_when_nothing_is_shown(&h);
    set(&h.e, h.root, "fallback", Value::from("Fallback2"));
    assert_other_shown(&h.e, h.root, h.loader, "load");
}

#[test]
fn a_source_chosen_as_a_shell_judged_at_creation_is_dropped_is_the_one_shown() {
    for (source, fallback) in [("Broken", "Fallback"), ("", "Broken")] {
        let e = engine();
        let main = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property string source: "SOURCE"
    property int others: 0
    property int pressed: 0
    ShellLoader {
        id: shells
        objectName: "loader"
        source: root.source
        fallback: "FALLBACK"
        onItemChanged: if (!shells.item) root.source = "Other"
    }
}
"#
        .replace("SOURCE", source)
        .replace("FALLBACK", fallback);
        let broken = r#"
import QtQuick
Rectangle {
    id: shell
    property var size
    width: shell.size.width
}
"#;
        compile(
            &e,
            &[
                ("Main", &main),
                ("Broken", broken),
                ("Fallback", FALLBACK),
                ("Other", OTHER),
            ],
        );
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        let loader = find(&e, root, "loader");
        assert_other_shown(&e, root, loader, &format!("judged, from {source:?}"));
    }
}

#[test]
fn a_source_chosen_as_a_retried_shell_completes_is_not_replaced_by_that_shell() {
    let h = Held::new();
    set(&h.e, h.root, "choose", Value::Bool(true));
    h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
    assert_other_replaced_the_chooser(&h, "completed");
    assert!(h.e.is_going(h.fallback), "nothing holds the fallback");
}

#[test]
fn a_source_chosen_as_a_failed_try_is_destroyed_keeps_no_error_of_it() {
    let h = Held::new();
    set(&h.e, h.root, "ready", Value::Bool(false));
    set(&h.e, h.root, "choose", Value::Bool(true));
    h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
    assert_other_replaced_the_chooser(&h, "destroyed");
}

#[test]
fn a_source_chosen_as_a_fallback_completes_is_shown_and_guarded() {
    let h = Held::new();
    set(&h.e, h.root, "ready", Value::Bool(false));
    set(&h.e, h.root, "fallback", Value::from("Chooser"));
    assert_other_replaced_the_chooser(&h, "fallback");
    h.fault_in_shell();
    h.e.deliver_faults();
    assert_eq!(
        num(&h.e, h.loader, "status"),
        2.0,
        "its fault is the loader's"
    );
}

#[test]
fn a_source_chosen_as_no_fallback_is_said_is_the_one_shown() {
    for property in ["error", "status"] {
        let h = Held::new();
        set(&h.e, h.root, "fallback", Value::from(""));
        choose_other_as_it_fails(&h, property);
        h.fault_in_shell();
        h.e.deliver_faults();
        assert_other_shown_and_no_wait(&h, property);
    }
}

#[test]
fn a_source_chosen_as_a_failed_fallback_is_said_is_the_one_shown() {
    for property in ["error", "status"] {
        let h = Held::new();
        set(&h.e, h.root, "fallback", Value::from("Missing"));
        choose_other_as_it_fails(&h, property);
        h.fault_in_shell();
        h.e.deliver_faults();
        assert_other_shown_and_no_wait(&h, property);
    }
}

#[test]
fn a_source_chosen_as_a_failed_retry_is_said_starts_no_backoff() {
    let h = Held::new();
    set(&h.e, h.root, "greedy", Value::Bool(true));
    choose_other_as_it_fails(&h, "error");
    h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
    assert_other_shown_and_no_wait(&h, "retry");
}

#[test]
fn a_source_chosen_as_the_held_fallback_is_said_is_the_one_shown() {
    for property in ["error", "status"] {
        let h = Held::new();
        h.retry();
        choose_other_as_it_fails(&h, property);
        h.fault_in_shell();
        h.e.deliver_faults();
        assert_other_shown_and_no_wait(&h, property);
    }
}

#[test]
fn a_source_chosen_as_a_fallback_judged_at_creation_is_said_is_the_one_shown() {
    let e = engine();
    let main = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property string source
    property int others: 0
    ShellLoader {
        id: shells
        objectName: "loader"
        source: root.source
        fallback: "Broken"
        onErrorChanged: if (shells.error) root.source = "Other"
    }
}
"#;
    let broken = r#"
import QtQuick
Rectangle {
    id: shell
    property var size
    width: shell.size.width
}
"#;
    compile(&e, &[("Main", main), ("Broken", broken), ("Other", OTHER)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    assert_other_shown(&e, root, loader, "judged");
}

#[test]
fn a_source_chosen_as_the_held_fallback_goes_keeps_its_own_error() {
    let h = Held::new();
    h.retry();
    set(&h.e, h.root, "chosenWhenLetGo", Value::from("Missing"));
    h.e.deliver_faults();
    assert_eq!(num(&h.e, h.loader, "status"), 2.0, "the one chosen failed");
    assert_ne!(h.error(), "", "and that is said");
}

#[test]
fn a_source_chosen_as_a_change_lets_go_of_the_held_one_is_made_once() {
    let h = Held::new();
    h.retry();
    set(&h.e, h.root, "chosenWhenLetGo", Value::from("Other"));
    set(&h.e, h.root, "fallback", Value::from("Missing"));
    assert_other_shown(&h.e, h.root, h.loader, "let go");
}

#[test]
fn a_reload_as_the_fallback_is_dropped_is_not_loaded_over() {
    for ready in [true, false] {
        let h = Held::new();
        set(&h.e, h.root, "ready", Value::Bool(ready));
        set(&h.e, h.root, "shells", Value::from(0.0));
        set(&h.e, h.root, "reloadWhenLetGo", Value::Bool(true));
        set(&h.e, h.root, "fallback", Value::from(""));
        assert_eq!(num(&h.e, h.root, "shells"), 1.0, "{ready}: one shell");
        let status = num(&h.e, h.loader, "status");
        assert_eq!(status, if ready { 1.0 } else { 3.0 }, "{ready}");
        assert_eq!(h.item().is_some(), ready, "{ready}");
    }
}

#[test]
fn a_source_chosen_as_a_judged_fallback_is_logged_is_not_dropped() {
    let e = engine();
    let main = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property string source
    property int others: 0
    function show() { shells.createObject(root) }
    Component {
        id: shells
        ShellLoader {
            objectName: "loader"
            source: root.source
            fallback: "Broken"
        }
    }
}
"#;
    let broken = r#"
import QtQuick
Rectangle {
    id: shell
    property var size
    width: shell.size.width
}
"#;
    compile(&e, &[("Main", main), ("Broken", broken), ("Other", OTHER)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    choose_other_as_it_is_logged(&e, root, "ShellLoader: fallback:");
    e.invoke_by_name(root, "show", &[]).unwrap();
    let loader = find(&e, root, "loader");
    assert_other_shown(&e, root, loader, "logged");
}

#[test]
fn a_source_chosen_as_a_backoff_is_logged_cancels_it() {
    let h = Held::new();
    set(&h.e, h.root, "fallback", Value::from(""));
    choose_other_as_it_is_logged(&h.e, h.root, "trying the source again");
    h.fault_in_shell();
    h.e.deliver_faults();
    assert_other_shown_and_no_wait(&h, "logged");
}

#[test]
fn a_shell_no_scene_shows_releases_the_fallback_at_the_next_delivery() {
    let h = Held::new();
    h.retry();
    h.e.deliver_faults();
    h.assert_released();
}

#[test]
fn a_fault_before_the_next_delivery_shows_the_same_fallback_again() {
    let h = Held::new();
    h.retry();
    h.fault_in_shell();
    h.e.deliver_faults();
    h.assert_shown_again();
}

#[test]
fn a_shell_a_scene_shows_waits_for_that_scenes_frame_not_for_a_delivery() {
    let h = Held::new();
    let mut s = h.scene();
    h.retry();
    h.e.deliver_faults();
    h.assert_held();
    frame(&mut s, &h.e);
    h.assert_released();
}

#[test]
fn another_scenes_frame_does_not_release_the_fallback_of_a_shell_a_scene_shows() {
    let h = Held::new();
    let mut s = h.scene();
    let beside = make::<ljos_quick::Rectangle>(&h.e, None);
    let mut other = scene(&h.e, beside, 20.0, 20.0);
    h.retry();
    frame(&mut other, &h.e);
    h.assert_held();
    frame(&mut s, &h.e);
    h.assert_released();
}

#[test]
fn a_fault_in_the_first_frames_read_shows_the_same_fallback_once_it_is_delivered() {
    let h = Held::new();
    let mut s = h.scene();
    set(&h.e, h.root, "heavy", Value::Bool(true));
    h.retry();
    let out = frame(&mut s, &h.e);
    assert!(h.e.has_pending_faults(), "the read faulted");
    h.assert_held();
    assert!(out.next_wake.is_some(), "a turn to deliver it in");
    h.e.deliver_faults();
    h.assert_shown_again();
}

#[test]
fn a_shell_a_timer_made_inside_a_frame_is_held_through_that_frames_read() {
    let h = Held::new();
    let mut s = h.scene();
    set(&h.e, h.root, "heavy", Value::Bool(true));
    let loader = h.loader;
    h.e.call_at(h.e.now(), move |e| {
        e.invoke_by_name(loader, "reload", &[]).unwrap();
    });
    frame_when_due(&mut s, &h);
    h.assert_held();
    h.e.deliver_faults();
    h.assert_shown_again();
}

#[test]
fn a_shell_no_scene_reaches_made_inside_a_frame_is_held_until_that_frame_ends() {
    let h = Held::new();
    let mut s = h.scene();
    h.detach();
    frame(&mut s, &h.e);
    let (loader, root) = (h.loader, h.root);
    h.e.call_at(h.e.now(), move |e| {
        e.invoke_by_name(loader, "reload", &[]).unwrap();
        // Read as the frame syncs, after the deliveries in it.
        e.write_by_name(root, "watching", Value::Bool(true))
            .unwrap();
    });
    frame_when_due(&mut s, &h);
    assert_ne!(
        get(&h.e, h.root, "sawFirst").to_js_string().to_string(),
        "",
        "held while the frame read"
    );
    h.assert_released();
}

#[test]
fn changing_the_source_or_the_fallback_destroys_the_held_one() {
    for property in ["source", "fallback"] {
        let h = Held::new();
        h.retry();
        set(&h.e, h.root, property, Value::from("Other"));
        assert!(h.e.is_going(h.fallback), "{property}");
        assert_eq!(h.error(), "", "{property}");
        assert_eq!(*h.errors.borrow(), [""], "{property}");
    }
}

#[test]
fn no_source_discards_the_held_one_for_a_new_fallback() {
    let h = Held::new();
    h.retry();
    set(&h.e, h.root, "source", Value::from(""));
    assert!(h.e.is_going(h.fallback));
    assert_eq!(num(&h.e, h.loader, "status"), 2.0);
    assert_ne!(h.item(), Some(h.fallback));
    assert_eq!(h.error(), "");
}

#[test]
fn destroying_the_loader_destroys_the_held_one() {
    let h = Held::new();
    h.retry();
    h.e.destroy(h.loader);
    assert!(h.e.is_going(h.fallback));
    h.e.process_deferred_deletes();
    h.e.deliver_faults();
    assert!(!h.e.is_alive(h.fallback));
}

/// The limit: only the first read is covered.
#[test]
fn a_fault_after_the_first_read_gets_a_new_fallback() {
    let h = Held::new();
    h.retry();
    h.e.deliver_faults();
    h.assert_released();
    h.fault_in_shell();
    h.e.deliver_faults();
    assert_eq!(num(&h.e, h.loader, "status"), 2.0);
    assert_ne!(h.item(), Some(h.fallback));
    assert!(h.error().contains("out of memory"), "{}", h.error());
}

#[test]
fn a_second_reload_keeps_the_shell_and_the_one_held_fallback() {
    let h = Held::new();
    h.retry();
    let shell = h.item();
    h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
    assert_eq!(h.item(), shell);
    h.assert_held();
    h.e.deliver_faults();
    h.assert_released();
}

#[test]
fn a_fault_in_the_held_fallback_is_not_the_loaders() {
    let h = Held::new();
    h.retry();
    let shell = h.item();
    h.e.report_fault(Some(h.fallback), Fault::JsOutOfMemory);
    h.e.deliver_faults();
    assert_eq!(h.item(), shell, "the source's shell stays");
    assert_eq!(num(&h.e, h.loader, "status"), 1.0);
}

#[test]
fn the_held_fallback_takes_no_pointer_and_keeps_its_visible() {
    let h = Held::new();
    let mut s = h.scene();
    set(&h.e, h.root, "heavy", Value::Bool(true));
    h.retry();
    frame(&mut s, &h.e);
    h.assert_held();
    s.pointer_event(&h.e, &PointerEvent::press(50.0, 50.0, Time::ZERO));
    assert_eq!(num(&h.e, h.root, "pressed"), 0.0, "no pointer");
    assert!(get(&h.e, h.fallback, "visible").to_boolean());
}

#[test]
fn the_held_fallback_is_not_described_to_assistive_technology() {
    let h = Held::new();
    let mut s = h.scene();
    s.set_accessibility_active(&h.e, true);
    frame(&mut s, &h.e);
    let described = |s: &QuickScene| {
        let mut found = false;
        s.accessibility_tree()
            .walk(|_, _, data| found |= data.name == "The fallback");
        found
    };
    assert!(described(&s), "described while it is shown");
    set(&h.e, h.root, "heavy", Value::Bool(true));
    h.retry();
    frame(&mut s, &h.e);
    h.assert_held();
    assert!(!described(&s), "not while it is held");
}

#[test]
fn focus_goes_back_to_the_fallback_unless_something_outside_both_took_it() {
    for taken in [false, true] {
        let h = Held::new();
        let mut s = h.scene();
        let field = find(&h.e, h.fallback, "field");
        h.e.invoke_by_name(field, "forceActiveFocus", &[]).unwrap();
        frame(&mut s, &h.e);
        h.retry();
        let shell = h.item().expect("the shell");
        h.e.invoke_by_name(shell, "forceActiveFocus", &[]).unwrap();
        let elsewhere = find(&h.e, h.root, "elsewhere");
        if taken {
            h.e.invoke_by_name(elsewhere, "forceActiveFocus", &[])
                .unwrap();
        }
        h.fault_in_shell();
        h.e.deliver_faults();
        h.assert_shown_again();
        let focused = if taken { elsewhere } else { field };
        assert!(get(&h.e, focused, "activeFocus").to_boolean());
    }
}
