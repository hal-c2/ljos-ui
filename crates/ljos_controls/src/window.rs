//! `Window` (QtQuick) and `ApplicationWindow` (QtQuick.Templates).
//!
//! A window says what it wants to be: shown or hidden, windowed or over
//! the screen, a title, a size and its limits. Who shows it decides what
//! that means. A host that owns platform windows ([`WindowSystem::set_host`],
//! the shell) gives every window one of its own and reports back what the
//! platform made of it. Without a host a window is an item of the scene it
//! was put in (one made with no parent, of the first scene there is), and
//! everything a script can see of showing, closing and
//! activating happens here, so that an application behaves the same
//! without a display.
//!
//! Differences from Qt: a window is an item underneath (it has `anchors`
//! and the rest, and is a focus scope: what has focus inside it has
//! active focus while the window is the active one). Documentation:
//! doc.qt.io/qt-6/qml-qtquick-window.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html; pinned by
//! `features/window/` and `features/controls/application-window.feature`.

mod app;
mod screen;
mod system;

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, Font, MetaRef, ObjectId, QmlList, QmlObject, QmlType, QtObject,
    Value, WriteKind, qml_impl,
};
use ljos_quick::focus::{
    FocusReason, WindowAttached, active_focus_item, force_active_focus, window_roots,
};
use ljos_quick::{Item, Rectangle, is_item, set_parent_item, window_of};

pub use app::{Application, ApplicationState, ScreenInfo, ScreenSpec, set_application_arguments};
pub use screen::{Screen, ScreenAttached, device_pixel_ratio};
use system::NO_MAXIMUM;
pub use system::{
    FRAMELESS, KIND_MASK, Modality, ResizeEdges, STAYS_ON_BOTTOM, STAYS_ON_TOP, WindowBackdrop,
    WindowRequest, WindowShow, WindowSpec, WindowSystem,
};

use crate::containers::{Host, bar_height, host_append, host_complete};
use crate::inherit::{self, InheritState, Kind};
use crate::palette::Palette;
use crate::parts::{PartKind, manage_part, settle_parts};
use crate::util::{check, flag, is_a, is_unset, num, object, set_computed};

/// How far up a tree is searched before it counts as broken.
const DEPTH: usize = 256;

/// `Window.Windowed`: what `visibility` holds (numbers read off qml6
/// 6.11.2), and `Window.BlurBackdrop`, `Window.GlassBackdrop`: what `backdrop`
/// holds, Ljós's own.
fn window_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Visibility",
            vec![
                ("Hidden", 0),
                ("AutomaticVisibility", 1),
                ("Windowed", 2),
                ("Minimized", 3),
                ("Maximized", 4),
                ("FullScreen", 5),
            ],
        ),
        (
            "Backdrop",
            vec![("NoBackdrop", 0), ("BlurBackdrop", 1), ("GlassBackdrop", 2)],
        ),
    ]
}

const HIDDEN: i32 = 0;
const AUTOMATIC: i32 = 1;
const WINDOWED: i32 = 2;
const MAXIMIZED: i32 = 4;
const FULL_SCREEN: i32 = 5;

/// What a window remembers that is no property.
#[derive(Debug)]
pub(crate) struct WinState {
    /// The `Window.Visibility` the window has while it is shown.
    pub(crate) wanted: Cell<i32>,
    /// Size and place before the window went over the screen (without a
    /// host, where nothing else remembers them).
    normal: Cell<Option<[f64; 4]>>,
    /// Whether the window was ever shown: only then does closing it ask.
    surfaced: Cell<bool>,
    /// Counts the writes of this module under way, which its own
    /// watchers let pass.
    busy: Cell<u32>,
    /// In no other window: the root of what was loaded.
    root: Cell<bool>,
    /// Whether the root was put under something since.
    adopted: Cell<bool>,
    /// Whether the application gave the window a place.
    pub(crate) placed: Cell<bool>,
    /// What had the keyboard before the window was shown (without a
    /// host): it gets it back when the window hides.
    prev_focus: Cell<Option<ObjectId>>,
    /// Whether the keys go to this very window.
    self_active: Cell<bool>,
    /// The object behind `Window.activeFocusItem` as an item of the
    /// window would ask: what the keys of the window's tree go to.
    attached: Cell<Option<ObjectId>>,
}

impl Default for WinState {
    fn default() -> Self {
        WinState {
            wanted: Cell::new(WINDOWED),
            normal: Cell::new(None),
            surfaced: Cell::new(false),
            busy: Cell::new(0),
            root: Cell::new(false),
            adopted: Cell::new(false),
            placed: Cell::new(false),
            prev_focus: Cell::new(None),
            self_active: Cell::new(false),
            attached: Cell::new(None),
        }
    }
}

impl WinState {
    /// Runs `f`, whose writes are this module's own.
    pub(crate) fn own<R>(&self, f: impl FnOnce() -> R) -> R {
        self.busy.set(self.busy.get() + 1);
        let r = f();
        self.busy.set(self.busy.get() - 1);
        r
    }

    fn is_own(&self) -> bool {
        self.busy.get() > 0
    }
}

/// `Window`.
///
/// It is a `Rectangle` underneath so that its `color` paints. `visible`,
/// `x` and `y` are the window's own: the item's stay where the scene
/// needs them.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Window",
    extends = Rectangle,
    attached = WindowAttached,
    enums = window_enums
)]
pub struct Window {
    #[qml(base)]
    pub rect: Rectangle,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub visible: bool,
    /// Where the window is on the desktop.
    #[qml(notify)]
    pub x: i32,
    #[qml(notify)]
    pub y: i32,
    #[qml(notify)]
    pub minimum_width: i32,
    #[qml(notify)]
    pub minimum_height: i32,
    #[qml(notify)]
    pub maximum_width: i32,
    #[qml(notify)]
    pub maximum_height: i32,
    /// What kind of window it is and how it is framed; the host honours
    /// what its platform can.
    #[qml(notify, flags = "Qt.WindowFlags")]
    pub flags: i32,
    #[qml(notify, enumeration = "Qt.WindowModality")]
    pub modality: i32,
    #[qml(notify, enumeration = "Visibility")]
    pub visibility: i32,
    /// What the window system draws behind the window (Ljós's own; Qt
    /// has no such property). Reads what was asked for, whether or not
    /// the window system has it.
    #[qml(notify, enumeration = "Backdrop")]
    pub backdrop: i32,
    #[qml(notify, readonly)]
    pub active: bool,
    /// The window this one belongs to: the window it was declared in,
    /// unless it is given another or none.
    #[qml(notify)]
    pub transient_parent: Option<ObjectId>,
    /// The display the window is on.
    #[qml(notify, readonly)]
    pub screen: Option<ObjectId>,
    /// The item the keys go to, while they go to this window.
    #[qml(notify, readonly)]
    pub active_focus_item: Option<ObjectId>,
    /// Where the window's children are: the window itself, or for an
    /// `ApplicationWindow` the item between its header and its footer.
    #[qml(notify, readonly)]
    pub content_item: Option<ObjectId>,
    #[qml(group)]
    pub palette: Palette,
    #[qml(skip)]
    pub(crate) inherit: Rc<InheritState>,
    #[qml(skip)]
    pub(crate) state: Rc<WinState>,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            rect: Rectangle::default(),
            title: Atom::default(),
            visible: false,
            x: 0,
            y: 0,
            minimum_width: 0,
            minimum_height: 0,
            maximum_width: NO_MAXIMUM,
            maximum_height: NO_MAXIMUM,
            // `Qt.Window`.
            flags: 1,
            modality: 0,
            visibility: HIDDEN,
            backdrop: 0,
            active: false,
            transient_parent: None,
            screen: None,
            active_focus_item: None,
            content_item: None,
            palette: Palette::default(),
            inherit: Rc::default(),
            state: Rc::default(),
        }
    }
}

/// What a `closing` handler is given: `close.accepted = false` keeps the
/// window open.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "CloseEvent",
    extends = QtObject,
    uncreatable
)]
pub struct CloseEvent {
    #[qml(base)]
    pub object: QtObject,
    pub accepted: bool,
}

#[qml_impl]
impl Window {
    /// The window is asked to close, by `close()` or by its user.
    #[qml(signal)]
    fn closing(close: Option<ObjectId>);

    #[qml(invokable)]
    fn show(cx: &Cx) -> Result<(), EngineError> {
        let wanted = state_of(cx.engine, cx.this).map_or(WINDOWED, |s| s.wanted.get());
        present(cx.engine, cx.this, Some(wanted));
        Ok(())
    }

    #[qml(invokable)]
    fn hide(cx: &Cx) -> Result<(), EngineError> {
        present(cx.engine, cx.this, None);
        Ok(())
    }

    #[qml(invokable, name = "showNormal")]
    fn show_normal(cx: &Cx) -> Result<(), EngineError> {
        present(cx.engine, cx.this, Some(WINDOWED));
        Ok(())
    }

    #[qml(invokable, name = "showMinimized")]
    fn show_minimized(cx: &Cx) -> Result<(), EngineError> {
        present(cx.engine, cx.this, Some(3));
        Ok(())
    }

    #[qml(invokable, name = "showMaximized")]
    fn show_maximized(cx: &Cx) -> Result<(), EngineError> {
        present(cx.engine, cx.this, Some(MAXIMIZED));
        Ok(())
    }

    #[qml(invokable, name = "showFullScreen")]
    fn show_full_screen(cx: &Cx) -> Result<(), EngineError> {
        present(cx.engine, cx.this, Some(FULL_SCREEN));
        Ok(())
    }

    /// Asks the window to close: `closing` is emitted, and the window is
    /// hidden unless a handler refuses.
    #[qml(invokable)]
    fn close(cx: &Cx) -> Result<bool, EngineError> {
        Ok(close(cx.engine, cx.this))
    }

    #[qml(invokable)]
    fn raise(cx: &Cx) -> Result<(), EngineError> {
        WindowSystem::of(cx.engine).request(cx.this, WindowRequest::Raise);
        Ok(())
    }

    #[qml(invokable)]
    fn lower(cx: &Cx) -> Result<(), EngineError> {
        WindowSystem::of(cx.engine).request(cx.this, WindowRequest::Lower);
        Ok(())
    }

    /// Asks for the keyboard.
    #[qml(invokable, name = "requestActivate")]
    fn request_activate(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let sys = WindowSystem::of(e);
        if sys.is_hosted() {
            sys.request(obj, WindowRequest::Activate);
        } else if e.untrack(|| flag(e, obj, Window::VISIBLE.index)) {
            force_active_focus(e, obj, FocusReason::ActiveWindow);
        }
        Ok(())
    }

    /// Asks for the user's attention without taking the keyboard.
    #[qml(invokable)]
    fn alert(cx: &Cx, _msec: Value) -> Result<(), EngineError> {
        WindowSystem::of(cx.engine).request(cx.this, WindowRequest::Alert);
        Ok(())
    }

    /// Asks the window system to let the user move the window with the
    /// pointer that is pressed (a title bar of the application's own).
    /// Whether there was one to ask: without a host nothing moves.
    #[qml(invokable, name = "startSystemMove")]
    fn start_system_move(cx: &Cx) -> Result<bool, EngineError> {
        let sys = WindowSystem::of(cx.engine);
        if !sys.is_hosted() {
            return Ok(false);
        }
        sys.request(cx.this, WindowRequest::StartMove);
        Ok(true)
    }

    /// The same for resizing the window by `edges`, a `Qt.Edges` of one
    /// edge or two that meet; anything else is refused.
    #[qml(invokable, name = "startSystemResize")]
    fn start_system_resize(cx: &Cx, edges: Value) -> Result<bool, EngineError> {
        let sys = WindowSystem::of(cx.engine);
        let edges = edges.to_number();
        let Some(edges) = (edges >= 0.0 && edges.fract() == 0.0)
            .then(|| ResizeEdges::from_qt(edges as u32))
            .flatten()
        else {
            return Ok(false);
        };
        if !sys.is_hosted() {
            return Ok(false);
        }
        sys.request(cx.this, WindowRequest::StartResize(edges));
        Ok(true)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        init_window(e, obj)?;
        if is_a::<ApplicationWindow>(e, obj) {
            // Its own hook does the rest, with its own font.
            return Ok(());
        }
        e.write_internal(
            obj,
            Window::CONTENT_ITEM.index,
            Value::Object(obj),
            WriteKind::Initial,
        )?;
        inherit::attach(e, obj, Kind::Window, true)
    }
}

pub(crate) fn state_of(engine: &Engine, win: ObjectId) -> Option<Rc<WinState>> {
    engine.with::<Window, _>(win, |w| w.state.clone()).ok()
}

/// The window `obj` is declared in: the nearest one above it among the
/// items, or else among its owners.
fn enclosing_window(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    engine.untrack(|| {
        let walk = |next: &dyn Fn(ObjectId) -> Option<ObjectId>| {
            let mut cur = next(obj);
            for _ in 0..DEPTH {
                match cur {
                    Some(p) if is_a::<Window>(engine, p) => return Some(p),
                    Some(p) => cur = next(p),
                    None => break,
                }
            }
            None
        };
        walk(&|o| engine.visual_parent(o)).or_else(|| walk(&|o| engine.owner(o)))
    })
}

/// The windows `win` belongs to, the nearest first.
pub(crate) fn transient_chain(engine: &Engine, win: ObjectId) -> Vec<ObjectId> {
    engine.untrack(|| {
        let mut out = Vec::new();
        let mut cur = object(engine, win, Window::TRANSIENT_PARENT.index);
        while let Some(p) = cur {
            if p == win || out.contains(&p) || out.len() >= DEPTH || !is_a::<Window>(engine, p) {
                break;
            }
            out.push(p);
            cur = object(engine, p, Window::TRANSIENT_PARENT.index);
        }
        out
    })
}

pub(crate) fn write_screen(engine: &Engine, win: ObjectId, screen: Option<ObjectId>) {
    let v = screen.map_or(Value::Null, Value::Object);
    check(
        engine,
        "Window.screen",
        set_computed(engine, win, Window::SCREEN.index, v),
    );
}

/// Whether `item` is `ancestor` or below it.
fn is_under(engine: &Engine, item: ObjectId, ancestor: ObjectId) -> bool {
    engine.untrack(|| {
        let mut cur = Some(item);
        for _ in 0..DEPTH {
            match cur {
                Some(o) if o == ancestor => return true,
                Some(o) => cur = engine.visual_parent(o),
                None => return false,
            }
        }
        false
    })
}

/// Writes a property of the window itself; a binding the application has
/// on it stays, as with Qt's setters.
fn put(engine: &Engine, win: ObjectId, prop: ljos_engine::PropIndex, v: Value) {
    if engine.untrack(|| engine.read(win, prop)).change_equals(&v) {
        return;
    }
    check(
        engine,
        "Window",
        engine.write_internal(win, prop, v, WriteKind::Animation),
    );
}

/// Shows the window as `show` (a `Window.Visibility` from 2 on), or
/// hides it. Doing what is already so does nothing.
fn present(engine: &Engine, win: ObjectId, show: Option<i32>) {
    let Some(st) = state_of(engine, win) else {
        return;
    };
    let sys = WindowSystem::of(engine);
    let hosted = sys.is_hosted();
    let was = engine.untrack(|| flag(engine, win, Window::VISIBLE.index));
    st.own(|| {
        engine.batch(|| match show {
            Some(v) => {
                st.wanted.set(v);
                st.surfaced.set(true);
                put(engine, win, Window::VISIBLE.index, Value::Bool(true));
                put(engine, win, Window::VISIBILITY.index, Value::Int(v));
                if !hosted {
                    place(engine, win, &st, v);
                }
            }
            None => {
                put(engine, win, Window::VISIBLE.index, Value::Bool(false));
                put(engine, win, Window::VISIBILITY.index, Value::Int(HIDDEN));
            }
        });
    });
    if !hosted {
        match (was, show.is_some()) {
            (false, true) => shown_focus(engine, win, &st),
            (true, false) => hidden_focus(engine, win, &st),
            _ => {}
        }
    }
    sys.notify();
}

/// Without a host: a window over the screen has the screen's size, and
/// gets its own back afterwards.
fn place(engine: &Engine, win: ObjectId, st: &WinState, show: i32) {
    let size = |w: f64, h: f64| {
        for (prop, v) in [(Item::WIDTH.index, w), (Item::HEIGHT.index, h)] {
            if engine.untrack(|| num(engine, win, prop)) != v {
                check(
                    engine,
                    "Window",
                    engine.write(win, prop, Value::Real(v), WriteKind::Animation),
                );
            }
        }
    };
    match show {
        MAXIMIZED | FULL_SCREEN => {
            if st.normal.get().is_none() {
                st.normal.set(Some(engine.untrack(|| {
                    [
                        num(engine, win, Item::WIDTH.index),
                        num(engine, win, Item::HEIGHT.index),
                        num(engine, win, Window::X.index),
                        num(engine, win, Window::Y.index),
                    ]
                })));
            }
            let [w, h] = WindowSystem::of(engine).screen_size(engine, win);
            size(w, h);
            put(engine, win, Window::X.index, Value::Int(0));
            put(engine, win, Window::Y.index, Value::Int(0));
        }
        WINDOWED => {
            if let Some([w, h, x, y]) = st.normal.take() {
                size(w, h);
                put(engine, win, Window::X.index, Value::Int(x as i32));
                put(engine, win, Window::Y.index, Value::Int(y as i32));
            }
        }
        _ => {}
    }
}

/// Without a host: a window that is shown takes the keyboard.
fn shown_focus(engine: &Engine, win: ObjectId, st: &WinState) {
    let before = engine
        .untrack(|| window_of(engine, win))
        .and_then(|top| active_focus_item(engine, top))
        .filter(|p| !is_under(engine, *p, win));
    st.prev_focus.set(before);
    force_active_focus(engine, win, FocusReason::ActiveWindow);
}

/// Without a host: a window that hides gives the keyboard back.
fn hidden_focus(engine: &Engine, win: ObjectId, st: &WinState) {
    let before = st
        .prev_focus
        .take()
        .filter(|p| engine.is_alive(*p) && !is_under(engine, *p, win));
    if !engine.untrack(|| flag(engine, win, Item::FOCUS.index)) {
        return;
    }
    match before {
        Some(p) => force_active_focus(engine, p, FocusReason::ActiveWindow),
        // The root of what was loaded keeps its place: what is in it is
        // where the keys of an unshown tree go, as before it was shown.
        None if st.root.get() => {}
        None => put(engine, win, Item::FOCUS.index, Value::Bool(false)),
    }
}

/// A minimum above the size makes the window larger, a maximum below it
/// smaller; the maximum wins where the two disagree.
fn apply_limits(engine: &Engine, win: ObjectId) {
    engine.untrack(|| {
        for (size, min, max) in [
            (
                Item::WIDTH.index,
                Window::MINIMUM_WIDTH.index,
                Window::MAXIMUM_WIDTH.index,
            ),
            (
                Item::HEIGHT.index,
                Window::MINIMUM_HEIGHT.index,
                Window::MAXIMUM_HEIGHT.index,
            ),
        ] {
            let now = num(engine, win, size);
            let max = num(engine, win, max).max(0.0);
            let min = num(engine, win, min).max(0.0).min(max);
            let fit = now.max(min).min(max);
            if fit != now {
                check(
                    engine,
                    "Window",
                    engine.write(win, size, Value::Real(fit), WriteKind::Animation),
                );
            }
        }
    });
}

/// `close()`, and what the user's close button does. Whether the window
/// closed. A window that was never shown is not asked.
pub(crate) fn close(engine: &Engine, win: ObjectId) -> bool {
    let Some(st) = state_of(engine, win) else {
        return false;
    };
    if !st.surfaced.get() {
        return false;
    }
    let event = CloseEvent {
        accepted: true,
        ..CloseEvent::default()
    };
    let accepted = match engine.create_native_incomplete(event, Some(win)) {
        Ok(ev) => {
            check(
                engine,
                "Window.closing",
                engine.emit_by_name(win, "closing", &[Value::Object(ev)]),
            );
            // A handler may have destroyed the window and the event with it.
            if !engine.is_alive(ev) {
                return false;
            }
            let accepted = engine
                .untrack(|| engine.read(ev, CloseEvent::ACCEPTED.index))
                .to_boolean();
            engine.destroy_now(ev);
            accepted
        }
        Err(_) => true,
    };
    if !accepted || !engine.is_alive(win) {
        return false;
    }
    let sys = WindowSystem::of(engine);
    sys.count_close();
    present(engine, win, None);
    sys.notify();
    true
}

fn init_window(engine: &Engine, win: ObjectId) -> Result<(), EngineError> {
    let e = engine;
    let Some(st) = state_of(e, win) else {
        return Ok(());
    };
    let sys = WindowSystem::of(e);
    let hosted = sys.is_hosted();
    let parent = e.untrack(|| e.visual_parent(win));
    // Whether an item is above it says nothing: who loads a file may put
    // its root under an item of its own before the root is complete.
    st.root.set(enclosing_window(e, win).is_none());
    sys.add(win);

    if is_unset(e, win, Window::TRANSIENT_PARENT.index) {
        let inside = enclosing_window(e, win).map_or(Value::Null, Value::Object);
        e.write_internal(
            win,
            Window::TRANSIENT_PARENT.index,
            inside,
            WriteKind::Initial,
        )?;
    }
    write_screen(e, win, sys.screen(e, 0));

    st.placed
        .set(!is_unset(e, win, Window::X.index) || !is_unset(e, win, Window::Y.index));
    for prop in [Window::X.index, Window::Y.index] {
        let st = st.clone();
        e.watch(win, prop, move |e, _| {
            if !st.is_own() {
                st.placed.set(true);
                WindowSystem::of(e).notify();
            }
        })?;
    }

    // With a host a window is a tree of its own, wherever it was declared.
    if hosted && parent.is_some() {
        set_parent_item(e, win, None)?;
    }
    if !hosted && parent.is_none() {
        let st = st.clone();
        e.call_at(e.now(), move |e| adopt_loose(e, win, &st));
    }

    // `visible` and `visibility` say the same thing two ways.
    {
        let st = st.clone();
        e.watch(win, Window::VISIBLE.index, move |e, v| {
            if !st.is_own() {
                present(e, win, v.to_boolean().then(|| st.wanted.get()));
            }
        })?;
    }
    {
        let st = st.clone();
        e.watch(win, Window::VISIBILITY.index, move |e, v| {
            if !st.is_own() {
                present(
                    e,
                    win,
                    match v.to_number() as i32 {
                        HIDDEN => None,
                        AUTOMATIC => Some(WINDOWED),
                        v => Some(v.clamp(WINDOWED, FULL_SCREEN)),
                    },
                );
            }
        })?;
    }
    let (visible, visibility) = e.untrack(|| {
        (
            flag(e, win, Window::VISIBLE.index),
            num(e, win, Window::VISIBILITY.index) as i32,
        )
    });
    if visibility >= WINDOWED {
        present(e, win, Some(visibility.min(FULL_SCREEN)));
    } else if visible {
        present(e, win, Some(WINDOWED));
    } else {
        // `AutomaticVisibility` on a window nothing shows reads as hidden.
        st.own(|| put(e, win, Window::VISIBILITY.index, Value::Int(HIDDEN)));
    }

    for prop in [
        Window::MINIMUM_WIDTH.index,
        Window::MINIMUM_HEIGHT.index,
        Window::MAXIMUM_WIDTH.index,
        Window::MAXIMUM_HEIGHT.index,
    ] {
        e.watch(win, prop, move |e, _| apply_limits(e, win))?;
    }
    apply_limits(e, win);

    // The keys of a tree go to its top's focus item: a window that is a
    // tree of its own, or the root of what was loaded, is that item, and
    // so is a window that starts shown.
    let shown = e.untrack(|| flag(e, win, Window::VISIBLE.index));
    if hosted || st.root.get() || shown {
        force_active_focus(e, win, FocusReason::Other);
    }

    let attached = e
        .attached_object(win, &MetaRef::Static(Window::static_meta()))
        .ok();
    st.attached.set(attached);
    {
        let st = st.clone();
        e.effect_in(win, move |e| {
            let sys = WindowSystem::of(e);
            let parent = e.visual_parent(win);
            if parent.is_some() {
                if sys.is_hosted() {
                    // Runs again without the parent.
                    check(e, "Window", e.untrack(|| set_parent_item(e, win, None)));
                    // Among another window's items the keys were somebody
                    // else's, and it may have lost its claim to them
                    // there: in its own tree they are its own.
                    e.untrack(|| force_active_focus(e, win, FocusReason::Other));
                    return;
                }
                if st.root.get() && !st.adopted.replace(true) {
                    // Loaded into a scene that shows other things too:
                    // the newest window is the active one.
                    e.untrack(|| force_active_focus(e, win, FocusReason::ActiveWindow));
                }
            }
            let visible = flag(e, win, Window::VISIBLE.index);
            // Without a host the root of what was loaded is what its scene
            // shows, shown or not; any other window is seen while shown.
            let seen = sys.is_hosted() || st.root.get() || visible;
            check(
                e,
                "Window",
                e.untrack(|| set_computed(e, win, Item::VISIBLE.index, Value::Bool(seen))),
            );
            let keys = flag(e, win, Item::ACTIVE_FOCUS.index);
            st.self_active.set(visible && keys);
            // What follows reads these.
            let _ = e.read(win, Window::TRANSIENT_PARENT.index);
            if let Some(att) = attached {
                let _ = e.read(att, WindowAttached::ACTIVE_FOCUS_ITEM.index);
            }
            e.untrack(|| activity(e));
        })?;
    }

    if hosted {
        e.effect_in(win, move |e| {
            let sys = WindowSystem::of(e);
            let _ = sys.spec_tracked(e, win);
            sys.notify();
        })?;
    }
    sys.notify();
    Ok(())
}

/// Without a host, a window made with no parent (`createObject(null)`)
/// that no scene shows is an item of the first tree a scene does show,
/// as a window declared there would be: only in a scene are its items
/// laid out. It is asked once the work that made the window is over,
/// since whoever loads a file shows its root only after the root is
/// complete; Qt too lays such a window out only after `createObject`
/// returned.
fn adopt_loose(engine: &Engine, win: ObjectId, st: &WinState) {
    if !engine.is_alive(win) || WindowSystem::of(engine).is_hosted() {
        return;
    }
    engine.untrack(|| {
        if engine.visual_parent(win).is_some() || window_of(engine, win).is_some() {
            return;
        }
        let Some(top) = window_roots(engine).into_iter().find(|r| *r != win) else {
            return;
        };
        // It is not the root of what was loaded, which a scene shows
        // whether it is shown or not.
        st.root.set(false);
        check(engine, "Window", set_parent_item(engine, win, Some(top)));
    });
}

/// The window an item is in: the nearest one above it. What a popup shows
/// is in the window of the popup's parent, as in Qt, where the popup's
/// item is in that window's overlay; without a host a window declared in
/// an item has no overlay of its own, so the layer would lead elsewhere.
fn window_above(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    let mut cur = item;
    for _ in 0..DEPTH {
        if is_a::<Window>(engine, cur) {
            return Some(cur);
        }
        if let Some(parent) = crate::popup::popup_of(engine, cur)
            .and_then(|p| object(engine, p, crate::popup::Popup::PARENT.index))
        {
            cur = parent;
            continue;
        }
        match engine.visual_parent(cur) {
            Some(p) => cur = p,
            None => {
                let host = ljos_quick::layer_host(engine, cur)?;
                if host == cur {
                    return None;
                }
                cur = host;
            }
        }
    }
    None
}

/// Says of every window whether it is the active one, and which item its
/// keys go to. A window is active while the keys go to it, to a window it
/// belongs to or to one that belongs to it
/// (doc.qt.io/qt-6/qwindow.html#isActive).
fn activity(engine: &Engine) {
    let sys = WindowSystem::of(engine);
    let wins = sys.windows(engine);
    let keyed: Vec<ObjectId> = wins
        .iter()
        .copied()
        .filter(|w| state_of(engine, *w).is_some_and(|s| s.self_active.get()))
        .collect();
    let chains: Vec<(ObjectId, Vec<ObjectId>)> = keyed
        .iter()
        .map(|w| (*w, transient_chain(engine, *w)))
        .collect();
    for win in wins {
        let visible = flag(engine, win, Window::VISIBLE.index);
        let own = transient_chain(engine, win);
        let active = visible
            && chains
                .iter()
                .any(|(k, above)| *k == win || above.contains(&win) || own.contains(k));
        // The tree says where the keys go before the items hear of it,
        // so this is right whichever of them is told first.
        let focus = keyed
            .contains(&win)
            .then(|| state_of(engine, win)?.attached.get())
            .flatten()
            .and_then(|att| object(engine, att, WindowAttached::ACTIVE_FOCUS_ITEM.index))
            .filter(|f| window_above(engine, *f) == Some(win));
        check(
            engine,
            "Window.active",
            set_computed(engine, win, Window::ACTIVE.index, Value::Bool(active)),
        );
        check(
            engine,
            "Window.activeFocusItem",
            set_computed(
                engine,
                win,
                Window::ACTIVE_FOCUS_ITEM.index,
                focus.map_or(Value::Null, Value::Object),
            ),
        );
    }
}

/// The window `item` is in: the nearest `Window` above it, or else the top
/// of the tree a scene shows it in, which is the window then (tracked).
fn window_for(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    window_above(engine, item).or_else(|| window_of(engine, item))
}

/// Binds what `Window.` tells an item of its window beyond focus, which
/// `ljos_quick` binds. A window that is no `Window` (the top of what a
/// scene shows) is its own content item and reads as shown while it is.
fn bind_attached(engine: &Engine, item: ObjectId, att: ObjectId) -> Result<(), EngineError> {
    let e = engine;
    e.bind_initial(att, WindowAttached::WINDOW.index, move |e| {
        window_for(e, item).map_or(Value::Null, Value::Object)
    })?;
    e.bind_initial(
        att,
        WindowAttached::CONTENT_ITEM.index,
        move |e| match window_for(e, item) {
            Some(w) if is_a::<Window>(e, w) => {
                object(e, w, Window::CONTENT_ITEM.index).map_or(Value::Null, Value::Object)
            }
            Some(w) => Value::Object(w),
            None => Value::Null,
        },
    )?;
    for (prop, size) in [
        (WindowAttached::WIDTH.index, Item::WIDTH.index),
        (WindowAttached::HEIGHT.index, Item::HEIGHT.index),
    ] {
        e.bind_initial(att, prop, move |e| {
            Value::Int(window_for(e, item).map_or(0, |w| num(e, w, size) as i32))
        })?;
    }
    e.bind_initial(att, WindowAttached::VISIBILITY.index, move |e| {
        Value::Int(match window_for(e, item) {
            Some(w) if is_a::<Window>(e, w) => num(e, w, Window::VISIBILITY.index) as i32,
            Some(_) => WINDOWED,
            None => HIDDEN,
        })
    })?;
    Ok(())
}

pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, item, att| {
        if is_a::<WindowAttached>(e, att) && is_item(e, item) {
            check(e, "Window", bind_attached(e, item, att));
        }
        true
    });
    engine.register_type(
        "QtQuick",
        ljos_engine::Version::new(2, 15),
        MetaRef::Static(CloseEvent::static_meta()),
    );
    ljos_quick::register_window_position(engine, |e, item| {
        let w = window_above(e, item)?;
        e.with(w, |w: &Window| ljos_engine::Point {
            x: f64::from(w.x),
            y: f64::from(w.y),
        })
        .ok()
    });
    ljos_quick::focus::register_focus_scope(engine, MetaRef::Static(Window::static_meta()));
    app::register(engine);
}

/// `ApplicationWindow`: a window with a header, a footer and a
/// background, whose children go between the two bars.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ApplicationWindow",
    extends = Window,
    default = "contentData"
)]
pub struct ApplicationWindow {
    #[qml(base)]
    pub window: Window,
    #[qml(notify, reset = inherit::FONT_RESET)]
    pub font: Font,
    /// What the controls in the window have until they are given one.
    #[qml(notify, reset = inherit::LOCALE_RESET)]
    pub locale: Value,
    #[qml(notify)]
    pub background: Option<ObjectId>,
    #[qml(notify)]
    pub header: Option<ObjectId>,
    #[qml(notify)]
    pub footer: Option<ObjectId>,
    /// Lies across the top of the window, above the header.
    #[qml(notify)]
    pub menu_bar: Option<ObjectId>,
    #[qml(notify, list, append = window_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
}

const HOST: Host = Host {
    content_item: Window::CONTENT_ITEM.index,
    content_data: ApplicationWindow::CONTENT_DATA.index,
    content_children: ApplicationWindow::CONTENT_CHILDREN.index,
};

fn window_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    host_append(cx, child, HOST)
}

/// How far down the header of an application window begins: below the
/// menu bar (tracked).
fn header_top(engine: &Engine, win: ObjectId) -> f64 {
    bar_height(engine, win, ApplicationWindow::MENU_BAR.index)
}

/// Where the content of an application window is: all of it but the
/// bars (tracked).
fn content_rect(engine: &Engine, win: ObjectId) -> [f64; 4] {
    let top = header_top(engine, win) + bar_height(engine, win, ApplicationWindow::HEADER.index);
    let bottom = bar_height(engine, win, ApplicationWindow::FOOTER.index);
    [
        0.0,
        top,
        num(engine, win, Item::WIDTH.index),
        (num(engine, win, Item::HEIGHT.index) - top - bottom).max(0.0),
    ]
}

#[qml_impl]
impl ApplicationWindow {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        settle_parts(e, obj);
        host_complete(e, obj, HOST)?;
        manage_part(
            e,
            obj,
            ApplicationWindow::BACKGROUND.index,
            PartKind::Background(None),
        )?;
        manage_part(
            e,
            obj,
            Window::CONTENT_ITEM.index,
            PartKind::Content(content_rect),
        )?;
        // The menu bar over the header (`z` 2 and 1 on Qt 6.11.2).
        manage_part(
            e,
            obj,
            ApplicationWindow::MENU_BAR.index,
            PartKind::Across(2.0, |_, _| 0.0),
        )?;
        manage_part(
            e,
            obj,
            ApplicationWindow::HEADER.index,
            PartKind::Across(1.0, header_top),
        )?;
        manage_part(e, obj, ApplicationWindow::FOOTER.index, PartKind::Footer)?;
        inherit::attach(e, obj, Kind::ApplicationWindow, true)
    }
}
