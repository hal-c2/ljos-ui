//! The fallback a `ShellLoader` holds through a first read, where the shell
//! or the fallback is a `Window` that a host gives a window of its own: who
//! ends that read, and what the host still shows when it has ended.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_controls::WindowSystem;
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::roots::{RootSwaps, replace_root};
use ljos_quick::{QuickScene, Rectangle};
use ljos_render::kurbo;
use ljos_runtime::{Fault, Time};

/// The loader is the application's root.
const AT_THE_ROOT: &str = r#"
import QtQuick
import Ljos.Blocks
ShellLoader {
    id: root
    objectName: "loader"
    property bool ready: false
    property bool heavy: false
    property bool refuseClose: false
    property string wanted: "Shell"
    source: root.wanted
    fallback: "Fallback"
}
"#;

/// The loader is in the application's main scene.
const IN_THE_SCENE: &str = r#"
import QtQuick
import Ljos.Blocks
Item {
    id: root
    width: 300; height: 200
    property bool ready: false
    property bool heavy: false
    property bool refuseClose: false
    property string wanted: "Shell"
    ShellLoader {
        objectName: "loader"
        source: root.wanted
        fallback: "Fallback"
    }
}
"#;

const SHELL: &str = r#"
import QtQuick
Window {
    objectName: "shell"
    width: 200; height: 100
    visible: true
    Rectangle {
        width: 50; height: 50
        color: root.heavy && new ArrayBuffer(268435456).byteLength ? "red" : "blue"
    }
    onClosing: (close) => close.accepted = !root.refuseClose
    Component.onCompleted: if (!root.ready) throw new Error("not ready")
}
"#;

const FALLBACK: &str = r#"
import QtQuick
Window {
    objectName: "fallback"
    width: 200; height: 100
    visible: true
    property int touched: 0
}
"#;

const OTHER: &str = r#"
import QtQuick
Rectangle { objectName: "other" }
"#;

fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    ljos_drive::find_below(e, root, name).unwrap_or_else(|| panic!("no object named {name}"))
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(300.0, 200.0));
    s.set_root(e, root).unwrap();
    s
}

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v).unwrap();
}

/// How a visible window stops expecting a scene of its own.
#[derive(Clone, Copy, Debug)]
enum Gone {
    Hidden,
    Closed,
}

impl Gone {
    fn apply(self, e: &Engine, window: ObjectId) {
        match self {
            Gone::Hidden => set(e, window, "visible", Value::Bool(false)),
            Gone::Closed => assert!(WindowSystem::of(e).request_close(e, window)),
        }
    }
}

struct Held {
    e: Engine,
    root: ObjectId,
    loader: ObjectId,
    fallback: ObjectId,
    errors: Rc<RefCell<Vec<String>>>,
    /// The root of what the host's first window shows.
    shown: std::cell::Cell<ObjectId>,
}

impl Held {
    fn new(main: &str, hosted: bool) -> Self {
        use ljos_compiler::{Project, compile_file};
        let e = Engine::new();
        ljos_quick::register(&e);
        ljos_controls::register(&e);
        // As an application that works a binding out when it is read.
        e.set_eager_bindings(false);
        if hosted {
            WindowSystem::of(&e).set_host(|| {});
        }
        let files = [
            ("Main", main),
            ("Shell", SHELL),
            ("Fallback", FALLBACK),
            ("Other", OTHER),
        ];
        let mut project = Project::new(qml_front::QtTypeOracle);
        for (name, source) in files {
            project.add(name, source);
        }
        let mut made = ljos_engine::MapSource::new();
        for (name, _) in files {
            let c = compile_file(&project, name).unwrap_or_else(|d| panic!("{name}: {d:?}"));
            assert!(!c.has_errors(), "{name}: {:?}", c.diagnostics);
            made.insert(name, c.file);
        }
        e.set_component_source(made);
        let root = e.create_from_url("Main", None, Vec::new()).unwrap();
        let loader = if main == AT_THE_ROOT {
            root
        } else {
            find(&e, root, "loader")
        };
        e.deliver_faults();
        e.process_deferred_deletes();
        assert_eq!(e.read_by_name(loader, "status").to_number(), 2.0);
        let fallback = e
            .read_by_name(loader, "item")
            .as_object()
            .expect("the fallback");
        set(&e, fallback, "touched", Value::from(7.0));
        set(&e, root, "ready", Value::Bool(true));
        // So that the heavy shell's binding is refused, not allocated.
        assert!(e.set_script_heap_limit(Some(64 << 20)));
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        e.connect_by_name(loader, "errorChanged", move |e, _| {
            seen.borrow_mut()
                .push(e.read_by_name(loader, "error").to_js_string().to_string());
        })
        .unwrap();
        Held {
            e,
            root,
            loader,
            fallback,
            errors,
            shown: std::cell::Cell::new(root),
        }
    }

    /// The loader at the root of a window that shows its fallback's.
    fn shown_at_the_root() -> Self {
        let h = Held::new(AT_THE_ROOT, true);
        RootSwaps::of(&h.e).shows(&h.e, None, h.root);
        h.swap(true);
        assert_eq!(h.shown.get(), h.fallback);
        h
    }

    /// The same, the swap to the source's window refused while it is held.
    fn refused_swap() -> Self {
        let h = Held::shown_at_the_root();
        h.retry();
        h.swap(false);
        assert_eq!(h.shown.get(), h.fallback, "the host shows it still");
        h.assert_held();
        h
    }

    /// The same, after the period's end: the one exception.
    fn left_shown() -> Self {
        let h = Held::refused_swap();
        h.frame_shell();
        h.assert_released_but_shown();
        h
    }

    fn swap(&self, can: bool) {
        let (root, _) = RootSwaps::of(&self.e).swap_in(&self.e, self.shown.get(), &mut |_, _| can);
        self.shown.set(root);
    }

    fn retry(&self) {
        self.e.invoke_by_name(self.loader, "reload", &[]).unwrap();
        self.assert_held();
    }

    fn item(&self) -> ObjectId {
        self.e
            .read_by_name(self.loader, "item")
            .as_object()
            .expect("a shell")
    }

    fn status(&self) -> f64 {
        self.e.read_by_name(self.loader, "status").to_number()
    }

    fn error(&self) -> String {
        self.e
            .read_by_name(self.loader, "error")
            .to_js_string()
            .to_string()
    }

    fn frame_shell(&self) {
        scene(&self.e, self.item()).frame(&self.e, Time::ZERO);
    }

    fn assert_held(&self) {
        assert!(!self.e.is_going(self.fallback), "the fallback is held");
        assert_eq!(self.status(), 1.0, "Ready meanwhile");
        assert_ne!(self.error(), "", "and the reason is said");
        assert!(self.errors.borrow().is_empty(), "with no signal");
    }

    fn assert_cleared(&self) {
        assert_eq!(self.status(), 1.0);
        assert_eq!(self.error(), "");
        assert_eq!(*self.errors.borrow(), [""], "cleared with one signal");
    }

    fn assert_released(&self) {
        assert!(self.e.is_going(self.fallback), "the fallback goes");
        self.assert_cleared();
    }

    fn assert_released_but_shown(&self) {
        assert!(!self.e.is_going(self.fallback), "the host shows it still");
        self.assert_cleared();
    }

    fn assert_shown_again(&self) {
        assert_eq!(self.item(), self.fallback, "the same fallback");
        assert!(!self.e.is_going(self.fallback));
        assert_eq!(
            self.e.read_by_name(self.fallback, "touched").to_number(),
            7.0
        );
        assert_eq!(self.status(), 2.0);
        assert!(self.error().contains("out of memory"), "{}", self.error());
    }

    fn fault_in_shell(&self) {
        self.e.report_fault(Some(self.item()), Fault::JsOutOfMemory);
        self.e.deliver_faults();
    }
}

#[test]
fn a_window_beside_the_main_one_releases_the_fallback_at_its_own_first_frame() {
    let h = Held::new(IN_THE_SCENE, true);
    let mut main = scene(&h.e, h.root);
    main.frame(&h.e, Time::ZERO);
    h.retry();
    for _ in 0..3 {
        main.frame(&h.e, Time::ZERO);
        h.e.deliver_faults();
        h.assert_held();
    }
    h.frame_shell();
    h.assert_released();
}

#[test]
fn a_fault_in_that_windows_first_frame_shows_the_same_fallback() {
    let h = Held::new(IN_THE_SCENE, true);
    set(&h.e, h.root, "heavy", Value::Bool(true));
    h.retry();
    h.frame_shell();
    assert!(h.e.has_pending_faults(), "the read faulted");
    h.assert_held();
    h.e.deliver_faults();
    h.assert_shown_again();
}

#[test]
fn a_window_a_timer_made_inside_another_frame_waits_for_its_own() {
    for at_the_root in [false, true] {
        let h = if at_the_root {
            Held::shown_at_the_root()
        } else {
            Held::new(IN_THE_SCENE, true)
        };
        let beside = h.e.create_native(Rectangle::default(), None).unwrap();
        let mut other = scene(&h.e, beside);
        let loader = h.loader;
        h.e.call_at(h.e.now(), move |e| {
            e.invoke_by_name(loader, "reload", &[]).unwrap();
        });
        let due = h.e.next_timer_deadline().expect("a timer");
        other.frame(&h.e, due);
        assert_eq!(h.status(), 1.0, "made in that frame");
        h.assert_held();
        if at_the_root {
            assert!(RootSwaps::of(&h.e).is_pending());
            h.swap(true);
        }
        h.frame_shell();
        h.assert_released();
    }
}

#[test]
fn a_window_hidden_or_closed_before_its_frame_releases_the_fallback() {
    for gone in [Gone::Hidden, Gone::Closed] {
        for by_a_frame in [false, true] {
            let h = Held::new(IN_THE_SCENE, true);
            let mut main = scene(&h.e, h.root);
            main.frame(&h.e, Time::ZERO);
            h.retry();
            // A host makes a window's scene before the window: being that
            // scene's root doesn't wait for a frame that need not come.
            let _own = scene(&h.e, h.item());
            gone.apply(&h.e, h.item());
            h.assert_held();
            if by_a_frame {
                main.frame(&h.e, Time::ZERO);
            } else {
                h.e.deliver_faults();
            }
            h.assert_released();
        }
    }
}

#[test]
fn a_close_the_window_rejects_releases_nothing() {
    let h = Held::new(IN_THE_SCENE, true);
    h.retry();
    set(&h.e, h.root, "refuseClose", Value::Bool(true));
    assert!(!WindowSystem::of(&h.e).request_close(&h.e, h.item()));
    h.e.deliver_faults();
    h.assert_held();
}

#[test]
fn a_window_without_a_host_is_an_item_of_the_scene_that_shows_it() {
    let h = Held::new(IN_THE_SCENE, false);
    let mut main = scene(&h.e, h.root);
    main.frame(&h.e, Time::ZERO);
    h.retry();
    h.e.deliver_faults();
    h.assert_held();
    main.frame(&h.e, Time::ZERO);
    h.assert_released();
}

#[test]
fn the_swap_that_shows_the_sources_window_does_not_destroy_the_held_one() {
    let h = Held::shown_at_the_root();
    h.retry();
    h.swap(true);
    assert_eq!(h.shown.get(), h.item());
    h.assert_held();
    h.frame_shell();
    h.assert_released();
}

#[test]
fn a_fault_after_the_swap_presents_the_held_window_again() {
    let h = Held::shown_at_the_root();
    h.retry();
    h.swap(true);
    h.fault_in_shell();
    h.assert_shown_again();
    h.swap(true);
    assert_eq!(h.shown.get(), h.fallback);
    assert!(!h.e.is_going(h.fallback));
}

#[test]
fn a_held_window_that_never_got_the_platforms_is_not_destroyed_while_held() {
    let h = Held::new(AT_THE_ROOT, true);
    RootSwaps::of(&h.e).shows(&h.e, None, h.root);
    h.retry();
    h.swap(true);
    assert_eq!(h.shown.get(), h.item());
    h.frame_shell();
    h.assert_released();
}

#[test]
fn a_refused_swap_alone_does_not_end_the_first_read() {
    let h = Held::refused_swap();
    let mut still_shown = scene(&h.e, h.fallback);
    still_shown.frame(&h.e, Time::ZERO);
    h.e.deliver_faults();
    h.assert_held();
    h.frame_shell();
    h.assert_released_but_shown();
}

#[test]
fn a_fault_in_the_frame_after_a_refused_swap_shows_the_same_window() {
    let h = Held::shown_at_the_root();
    set(&h.e, h.root, "heavy", Value::Bool(true));
    h.retry();
    h.swap(false);
    h.frame_shell();
    h.assert_held();
    h.e.deliver_faults();
    h.assert_shown_again();
    let swaps = RootSwaps::of(&h.e);
    assert!(!swaps.is_pending(), "it is what the host shows");
    assert_eq!(swaps.presented(h.loader), Some(h.fallback));
}

#[test]
fn hidden_or_closed_after_a_refused_swap_releases_and_keeps_what_is_shown() {
    for gone in [Gone::Hidden, Gone::Closed] {
        let h = Held::refused_swap();
        gone.apply(&h.e, h.item());
        h.assert_held();
        h.e.deliver_faults();
        h.assert_released_but_shown();
    }
}

#[test]
fn tries_a_host_refuses_hold_one_fallback_however_many_there_are() {
    let h = Held::shown_at_the_root();
    for round in 0..5 {
        h.errors.borrow_mut().clear();
        h.retry();
        h.e.invoke_by_name(h.loader, "reload", &[]).unwrap();
        h.swap(false);
        let live = h
            .e
            .children(h.loader)
            .into_iter()
            .filter(|o| !h.e.is_going(*o))
            .filter(|o| h.e.read_by_name(*o, "objectName").to_js_string().as_str() == "fallback")
            .count();
        assert_eq!(live, 1, "round {round}");
        h.fault_in_shell();
        h.assert_shown_again();
        h.e.process_deferred_deletes();
    }
}

#[test]
fn a_fault_in_the_window_left_shown_does_not_take_the_shell_down() {
    let h = Held::left_shown();
    let shell = h.item();
    h.e.report_fault(Some(h.fallback), Fault::JsOutOfMemory);
    h.e.deliver_faults();
    assert_eq!(h.item(), shell);
    assert_eq!(h.status(), 1.0);
}

#[test]
fn a_fault_after_the_exception_gets_a_new_fallback_in_the_old_ones_place() {
    let h = Held::left_shown();
    h.fault_in_shell();
    let new = h.item();
    assert_ne!(new, h.fallback, "a new one");
    assert!(
        !h.e.is_going(h.fallback),
        "until the host shows the new one"
    );
    h.swap(true);
    assert_eq!(h.shown.get(), new);
    assert!(h.e.is_going(h.fallback));
}

#[test]
fn what_the_loader_presents_next_retires_the_window_left_shown() {
    let h = Held::left_shown();
    set(&h.e, h.root, "wanted", Value::from("Other"));
    h.swap(true);
    assert_eq!(h.shown.get(), h.loader);
    assert!(h.e.is_going(h.fallback));
}

#[test]
fn a_root_put_in_the_loaders_place_takes_the_window_left_shown_with_it() {
    let h = Held::left_shown();
    let new = h.e.create_native(Rectangle::default(), None).unwrap();
    replace_root(&h.e, h.loader, new).unwrap();
    h.swap(true);
    assert_eq!(h.shown.get(), new);
    assert!(h.e.is_going(h.fallback));
}

#[test]
fn the_loaders_destruction_takes_the_window_left_shown() {
    let h = Held::left_shown();
    h.e.destroy(h.loader);
    assert!(h.e.is_going(h.fallback));
}

#[test]
fn a_window_left_shown_that_the_host_hides_is_destroyed() {
    let h = Held::left_shown();
    RootSwaps::of(&h.e).hides(h.fallback);
    assert!(h.e.is_going(h.fallback));
}

#[test]
fn a_new_source_destroys_the_held_window_once_what_replaces_it_is_shown() {
    let h = Held::shown_at_the_root();
    h.retry();
    set(&h.e, h.root, "wanted", Value::from("Other"));
    assert!(!h.e.is_going(h.fallback), "the host shows it still");
    h.swap(true);
    assert_eq!(h.shown.get(), h.loader);
    assert!(h.e.is_going(h.fallback));
    h.assert_cleared();
}
