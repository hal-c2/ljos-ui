//! A `ShellLoader` whose source's shell ran out of script heap shows its
//! fallback meanwhile and tries the source again on its own, at a bounded
//! rate, until there is room (ljos-ui#667). A shell that fails for its own
//! reasons stays on the fallback until the source changes, as before.

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use ljos_engine::{Engine, ObjectId, TakenBy, Value};
use ljos_runtime::{Clock, VirtualClock};

const MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property bool on: false
    ShellLoader {
        objectName: "loader"
        source: root.on ? "SOURCE" : ""
        fallback: "FALLBACK"
    }
    function litter(n) {
        for (let i = 0; i < n; i++) { const a = { b: [i, i + 1, "x" + i] }; a.self = a }
    }
}
"#;

const HEAVY: &str = r#"
import QtQuick
Item {
    objectName: "heavy"
    property var lots: {
        const a = [];
        for (let i = 0; i < 20000; i++) a.push({ i: i, s: "row " + i });
        return a;
    }
}
"#;

/// A shell that fills the heap while it is made.
const HUNGRY: &str = r#"
import QtQuick
Item {
    objectName: "hungry"
    property var lots: []
    Component.onCompleted: {
        for (let i = 0; i < 20000; i++) lots.push({ i: i, s: "row " + i });
    }
}
"#;

const LIGHT: &str = r#"
import QtQuick
Item { objectName: "light" }
"#;

fn status(e: &Engine, loader: ObjectId) -> i32 {
    num(e, loader, "status") as i32
}

fn shown(e: &Engine, loader: ObjectId) -> String {
    get(e, loader, "item")
        .as_object()
        .map(|o| get(e, o, "objectName").to_js_string().to_string())
        .unwrap_or_default()
}

/// What a frame does: what is due runs, the shown shell is read (its
/// bindings run when read), and faults go to their boundaries.
fn settle(e: &Engine, clock: &VirtualClock, loader: ObjectId) {
    e.advance_timers(clock.now());
    e.run_pending_calls();
    if let Some(item) = get(e, loader, "item").as_object() {
        let _ = get(e, item, "lots");
    }
    e.deliver_faults();
    e.process_deferred_deletes();
}

fn timed(source: &str) -> (Engine, Arc<VirtualClock>, ObjectId, ObjectId) {
    timed_with(source, "Light")
}

fn timed_with(source: &str, fallback: &str) -> (Engine, Arc<VirtualClock>, ObjectId, ObjectId) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(
        &e,
        &[
            (
                "Main",
                &MAIN.replace("SOURCE", source).replace("FALLBACK", fallback),
            ),
            ("Heavy", HEAVY),
            ("Light", LIGHT),
        ],
    );
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    (e, clock, root, loader)
}

#[test]
fn a_shell_that_ran_out_of_heap_falls_back_and_is_tried_again_once_there_is_room() {
    let (e, clock, root, loader) = timed("Heavy");
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2, "the fallback stands in");
    assert_eq!(shown(&e, loader), "light");

    // While the heap stays full the source is tried again, at a bounded
    // rate, with the fallback shown in between.
    let mut tries = 0;
    let wait = loop {
        let due = e.next_timer_deadline().expect("the source is tried again");
        let wait = due.saturating_since(clock.now());
        // Waits under a second are a first try at once, a timer step on.
        if wait >= Duration::from_secs(1) {
            break wait;
        }
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 2);
        tries += 1;
        assert!(tries < 4, "tried at once only at first");
    };
    assert!(wait <= Duration::from_secs(60), "{wait:?}");
    let warned = e.take_warnings();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("ShellLoader: trying the source again in")),
        "{warned:?}"
    );

    // Room again: the source's shell comes back with nobody touching it.
    assert!(e.set_script_heap_limit(None));
    clock.set(clock.now() + wait);
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    assert_eq!(shown(&e, loader), "heavy");
    assert_eq!(get(&e, loader, "error").to_js_string().to_string(), "");
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

#[test]
fn a_fallback_that_also_ran_out_of_heap_charges_the_backoff_once_a_try() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let main = MAIN
        .replace("SOURCE", "Hungry")
        .replace("\"FALLBACK\"", "root.fallback")
        .replace(
            "property bool on: false",
            "property bool on: false\n    property string fallback",
        );
    compile(&e, &[("Main", &main), ("Hungry", HUNGRY)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    // Room for the application, not for a shell's rows.
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    // No source: the fallback alone fails, tried at once and again in the
    // frame (a second on).
    set(&e, root, "fallback", Value::from("Hungry"));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 3, "{:?}", get(&e, loader, "error"));
    // The source fails, and the fallback with it.
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 3, "{:?}", get(&e, loader, "error"));
    let _ = e.take_warnings();

    // Each try the source and the fallback both fail is one fault to the
    // backoff, not two: the wait doubles once a try (charged twice, it
    // would go up four times a try), one warning a try.
    let mut waits = Vec::new();
    for _ in 0..5 {
        let due = e.next_timer_deadline().expect("the source is tried again");
        waits.push(due.saturating_since(clock.now()));
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 3, "{:?}", get(&e, loader, "error"));
    }
    // Whole seconds: a wait runs a timer step over.
    let secs: Vec<u64> = waits.iter().map(Duration::as_secs).collect();
    assert_eq!(secs, [2, 4, 8, 16, 32], "{waits:?}");
    let warned = e.take_warnings();
    let tries = warned
        .iter()
        .filter(|w| w.contains("ShellLoader: trying the source again in"))
        .count();
    assert_eq!(tries, 5, "{warned:?}");
}

#[test]
fn a_shell_asked_to_load_again_that_runs_out_of_heap_keeps_the_fallback_through_its_tries() {
    let (e, clock, root, loader) = timed("Heavy");
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2, "the fallback stands in");
    // What the user did in the fallback: it is kept while the source is
    // tried, as long as the fallback is.
    let fallback = get(&e, loader, "item").as_object().expect("the fallback");
    set(&e, fallback, "objectName", Value::from("light-touched"));

    e.invoke_by_name(loader, "reload", &[]).unwrap();
    settle(&e, &clock, loader);
    assert_eq!(shown(&e, loader), "light-touched", "the fallback is kept");
    for _ in 0..3 {
        let due = e.next_timer_deadline().expect("the source is tried again");
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 2);
        assert_eq!(
            shown(&e, loader),
            "light-touched",
            "the fallback is kept through the tries"
        );
    }

    // Room again: the source's shell takes the fallback's place.
    assert!(e.set_script_heap_limit(None));
    let due = e.next_timer_deadline().expect("the source is tried again");
    clock.set(due);
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    assert_eq!(shown(&e, loader), "heavy");
}

/// The same of a shell that fills the heap while it is made, not when it
/// is first read.
#[test]
fn a_shell_asked_to_load_again_that_runs_out_of_heap_while_made_keeps_the_fallback_through_its_tries()
 {
    let (e, clock, root, loader) = timed("Hungry");
    compile(&e, &[("Hungry", HUNGRY), ("Light", LIGHT)]);
    e.invoke_by_name(root, "litter", &[Value::from(0.0)])
        .unwrap();
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2, "the fallback stands in");
    let fallback = get(&e, loader, "item").as_object().expect("the fallback");
    set(&e, fallback, "objectName", Value::from("light-touched"));

    e.invoke_by_name(loader, "reload", &[]).unwrap();
    settle(&e, &clock, loader);
    assert_eq!(shown(&e, loader), "light-touched", "the fallback is kept");
    for _ in 0..3 {
        let due = e.next_timer_deadline().expect("the source is tried again");
        clock.set(due);
        settle(&e, &clock, loader);
        assert_eq!(status(&e, loader), 2);
        assert_eq!(
            shown(&e, loader),
            "light-touched",
            "the fallback is kept through the tries"
        );
    }

    // Room again: the source's shell takes the fallback's place.
    assert!(e.set_script_heap_limit(None));
    let due = e.next_timer_deadline().expect("the source is tried again");
    clock.set(due);
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    assert_eq!(shown(&e, loader), "hungry");
}

#[test]
fn a_shell_that_cannot_be_made_for_its_own_reasons_stays_on_the_fallback() {
    let (e, clock, root, loader) = timed("Missing");
    set(&e, root, "on", Value::Bool(true));
    settle(&e, &clock, loader);
    assert_eq!(status(&e, loader), 2);
    assert_eq!(shown(&e, loader), "light");
    assert_eq!(
        e.next_timer_deadline(),
        None,
        "only a resource fault is tried again"
    );
}

const SLOW_MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property bool on: false
    property int made: 0
    readonly property var tries: ({ binding: 0, handler: 0 })
    // Runs over its time the first `n` times it is called for `what`.
    function slow(what, n) {
        if (tries[what]++ < n) { for (;;) {} }
        return 5
    }
    ShellLoader {
        objectName: "loader"
        source: root.on ? "Slow" : ""
        fallback: "Light"
    }
}
"#;

const SLOW: &str = r#"
import QtQuick
Item {
    objectName: "slow"
    property int size: root.slow("binding", 3)
    // Runs over its time while the shell is made.
    Component.onCompleted: { root.made++; root.slow("handler", 1) }
}
"#;

/// A script stopped for its time in a shell that was made is not a fault
/// that breaks the shell (ljos-ui#676): the shell is made once and stays
/// shown, the fallback never stands in, and the binding cut short, here
/// three times, gets its value when the engine runs it again. Making the
/// shell again would blank it, and a binding that always runs over would
/// have it made again for good.
#[test]
fn a_shell_whose_scripts_ran_over_their_time_is_kept_and_its_binding_runs_again() {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(&e, &[("Main", SLOW_MAIN), ("Slow", SLOW), ("Light", LIGHT)]);
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    let frame = |e: &Engine| {
        e.advance_timers(clock.now());
        e.run_pending_calls();
        if let Some(item) = get(e, loader, "item").as_object() {
            let _ = get(e, item, "size");
        }
        e.deliver_faults();
        e.process_deferred_deletes();
    };
    frame(&e);
    assert_eq!(shown(&e, loader), "light");
    e.set_js_dispatch_budget(Duration::from_millis(10), Duration::from_millis(50))
        .unwrap();

    set(&e, root, "on", Value::Bool(true));
    frame(&e);
    assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    let shell = get(&e, loader, "item").as_object().expect("shown");
    assert_eq!(shown(&e, loader), "slow");
    assert_eq!(num(&e, root, "made"), 1.0);
    assert_eq!(num(&e, shell, "size"), 0.0, "cut short");
    // On record as taken by the loader, which kept its shell.
    let kept = e
        .faults()
        .into_iter()
        .filter(|f| f.kind() == "JsInterrupted")
        .all(|f| matches!(f.taken_by, TakenBy::Kept { object, .. } if object == loader));
    assert!(kept, "{:?}", e.faults());

    // The binding runs again after a wait; the shell is not made again.
    let mut waits = Vec::new();
    while num(&e, shell, "size") != 5.0 {
        assert!(waits.len() < 8, "never ran again: {waits:?}");
        let due = e.next_timer_deadline().expect("the binding runs again");
        waits.push(due.saturating_since(clock.now()));
        clock.set(due);
        frame(&e);
        assert_eq!(get(&e, loader, "item").as_object(), Some(shell));
        assert_eq!(status(&e, loader), 1, "{:?}", get(&e, loader, "error"));
    }
    assert!(
        waits.iter().all(|w| *w <= Duration::from_secs(60)),
        "{waits:?}"
    );
    assert!(e.is_alive(shell));
    assert_eq!(num(&e, root, "made"), 1.0);
    assert_eq!(get(&e, loader, "error").to_js_string().to_string(), "");
    assert!(e.application_fault().is_none());
    frame(&e);
    assert_eq!(e.next_timer_deadline(), None, "nothing more to wait for");
}

const SNUG_MAIN: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    property bool ready: false
    property int made: 0
    property string source: "Snug"
    property string fallback: "Full"
    ShellLoader {
        objectName: "loader"
        source: root.source
        fallback: root.fallback
    }
}
"#;

/// A fallback whose state takes more room than [`SNUG`] needs.
const FULL: &str = r#"
import QtQuick
Item {
    objectName: "full"
    property int touched: 0
    property var held: []
    Component.onCompleted: {
        for (let i = 0; i < 40000; i++) held.push({ i: i, s: "held " + i });
    }
}
"#;

/// A shell that fits only without [`FULL`], which its first read finds out.
const SNUG: &str = r#"
import QtQuick
Item {
    objectName: "snug"
    property var lots: {
        const a = [];
        for (let i = 0; i < 20000; i++) a.push({ i: i, s: "row " + i });
        return a;
    }
    Component.onCompleted: {
        root.made++;
        if (!root.ready) throw new Error("not ready");
    }
}
"#;

/// What the loader said just before the shown shell was read.
struct BeforeRead {
    status: i32,
    error: String,
    fallback_alive: bool,
}

fn error(e: &Engine, loader: ObjectId) -> String {
    get(e, loader, "error").to_js_string().to_string()
}

fn out_of_memory(e: &Engine) -> usize {
    e.faults()
        .iter()
        .filter(|f| f.kind() == "JsOutOfMemory")
        .count()
}

/// A frame's order: what is due runs and what was destroyed goes, so its
/// room is there before the shown shell is read. Only then are the faults
/// delivered: a delivery before the read would be the end of the first read
/// to an engine with no scene.
fn turn(e: &Engine, clock: &VirtualClock, loader: ObjectId, fallback: ObjectId) -> BeforeRead {
    e.advance_timers(clock.now());
    e.run_pending_calls();
    e.process_deferred_deletes();
    e.collect_garbage();
    let before = BeforeRead {
        status: status(e, loader),
        error: error(e, loader),
        fallback_alive: e.is_alive(fallback),
    };
    if let Some(item) = get(e, loader, "item").as_object() {
        let _ = get(e, item, "lots");
    }
    e.deliver_faults();
    e.process_deferred_deletes();
    before
}

/// `Full` shown because `Snug` was not ready, and the heap as full as that
/// leaves it; `Snug` is ready now.
fn snug() -> (Engine, Arc<VirtualClock>, ObjectId, ObjectId, ObjectId) {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    compile(
        &e,
        &[
            ("Main", SNUG_MAIN),
            ("Full", FULL),
            ("Snug", SNUG),
            ("Other", SNUG),
            ("Light", LIGHT),
        ],
    );
    let root = e.create_from_url("Main", None, Vec::new()).unwrap();
    let loader = find(&e, root, "loader");
    e.deliver_faults();
    e.process_deferred_deletes();
    assert_eq!(shown(&e, loader), "full", "{}", error(&e, loader));
    let fallback = get(&e, loader, "item").as_object().expect("the fallback");
    set(&e, fallback, "touched", Value::from(7.0));
    set(&e, root, "ready", Value::Bool(true));
    assert!(e.collect_garbage());
    let held = e.script_memory().unwrap().bytes;
    assert!(e.set_script_heap_limit(Some(held + 64 * 1024)));
    let _ = e.take_warnings();
    (e, clock, root, loader, fallback)
}

/// Holding the fallback through the first read takes its room from the
/// source: one that fits only without it stays on it, keeps saying why, and
/// is tried at the backoff.
#[test]
fn a_shell_that_fits_only_without_the_fallback_stays_on_it_and_waits_between_tries() {
    let (e, clock, root, loader, fallback) = snug();
    let errors = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = errors.clone();
    e.connect_by_name(loader, "errorChanged", move |e, _| {
        seen.borrow_mut().push(error(e, loader));
    })
    .unwrap();

    let backoff = [0, 1, 2, 4, 8, 16];
    let mut waits = Vec::new();
    for attempt in 0..7 {
        let (made, faults) = (num(&e, root, "made"), out_of_memory(&e));
        if attempt == 0 {
            e.invoke_by_name(loader, "reload", &[]).unwrap();
        } else {
            let from = clock.now();
            let mut due = e.next_timer_deadline().expect("the source is tried again");
            // The engine has timers of its own (it runs again a binding a
            // full heap cut short). However far off one is, no try comes
            // with it: the retry is the backoff's wait from the failure.
            let retry = from + Duration::from_secs(backoff[attempt - 1]);
            let mut others = 0;
            while due < retry {
                clock.set(due);
                turn(&e, &clock, loader, fallback);
                assert_eq!(
                    num(&e, root, "made"),
                    made,
                    "a try before its wait was over"
                );
                others += 1;
                assert!(others < 32, "a timer is always due, now at {due:?}");
                due = e.next_timer_deadline().expect("the source is tried again");
            }
            let wait = due.saturating_since(from);
            waits.push(wait.as_secs());
            if wait >= Duration::from_secs(1) {
                for early in [wait / 2, wait - Duration::from_millis(20)] {
                    clock.set(from + early);
                    turn(&e, &clock, loader, fallback);
                    assert_eq!(num(&e, root, "made"), made, "no try before it is due");
                    assert_eq!(out_of_memory(&e), faults);
                    assert_eq!(e.next_timer_deadline(), Some(due));
                }
            }
            clock.set(due);
        }
        let before = turn(&e, &clock, loader, fallback);
        assert_eq!(
            status(&e, loader),
            2,
            "a source that fits only without the fallback stays on it (try {attempt})"
        );
        assert_eq!(
            get(&e, loader, "item").as_object(),
            Some(fallback),
            "the same fallback"
        );
        assert_eq!(num(&e, fallback, "touched"), 7.0);
        assert_eq!(
            num(&e, root, "made"),
            made + 1.0,
            "one try (try {attempt}, waits {waits:?}); next {:?} at {:?}; error {}; faults {:?}; said {:?}",
            e.next_timer_deadline(),
            clock.now(),
            error(&e, loader),
            e.faults(),
            e.take_warnings()
        );
        assert_eq!(out_of_memory(&e), faults + 1, "{:?}", e.faults());
        assert!(error(&e, loader).contains("out of memory"));
        assert_eq!(before.status, 1, "Ready while the first read is ahead");
        assert!(before.fallback_alive, "held through the first read");
        assert_ne!(before.error, "", "the fault is said until the read is over");
    }
    assert_eq!(waits, backoff, "the backoff");
    assert!(
        errors.borrow().iter().all(|said: &String| !said.is_empty()),
        "a failing try blanked the fault: {:?}",
        errors.borrow()
    );
    let tries = e
        .take_warnings()
        .iter()
        .filter(|w| w.contains("ShellLoader: trying the source again in"))
        .count();
    assert_eq!(tries, 7, "one warning a fault");
}

/// The control of the test above, and the scope of holding: changing
/// `source` or `fallback` discards the held one, and then the source fits.
#[test]
fn changing_the_source_or_the_fallback_discards_the_held_one_and_gives_its_room_back() {
    for (property, to) in [("source", "Other"), ("fallback", "Light")] {
        let (e, clock, root, loader, fallback) = snug();
        let errors = Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = errors.clone();
        e.connect_by_name(loader, "errorChanged", move |e, _| {
            seen.borrow_mut().push(error(e, loader));
        })
        .unwrap();
        e.invoke_by_name(loader, "reload", &[]).unwrap();
        assert!(e.is_alive(fallback), "{property}: held before the change");
        let discarded = get(&e, loader, "item").as_object();
        assert!(
            discarded.is_some() && discarded != Some(fallback),
            "{property}: the retried shell is the item while held"
        );
        assert_ne!(error(&e, loader), "", "{property}: said while held");
        assert!(
            errors.borrow().is_empty(),
            "{property}: unchanged while held"
        );
        set(&e, root, property, Value::from(to));
        turn(&e, &clock, loader, fallback);
        assert!(
            !e.is_alive(fallback),
            "{property}: the held one is discarded"
        );
        assert_eq!(
            *errors.borrow(),
            [""],
            "{property}: the clearing is signalled"
        );
        assert_eq!(status(&e, loader), 1, "{property}: {}", error(&e, loader));
        assert_eq!(shown(&e, loader), "snug", "{property}");
        assert_eq!(error(&e, loader), "", "{property}");
        let item = get(&e, loader, "item").as_object();
        assert_ne!(item, discarded, "{property}: another took its place");
        assert_eq!(
            out_of_memory(&e),
            0,
            "{property}: fits without the fallback: item {item:?}, discarded {discarded:?}, {:?}",
            e.faults()
        );
        assert_eq!(e.next_timer_deadline(), None, "{property}");
    }
}
