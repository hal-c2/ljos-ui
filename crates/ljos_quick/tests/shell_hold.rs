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
    property string sawFirst
    property var looked: []
    property string source: "Shell"
    property string fallback: "Fallback"
    property int pressed: 0
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
    objectName: "shell"
    color: root.heavy && new ArrayBuffer(268435456).byteLength ? "red" : "blue"
    Component.onCompleted: if (!root.ready) throw new Error("not ready")
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
}
"#;

const OTHER: &str = r#"
import QtQuick
Rectangle { objectName: "other" }
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
