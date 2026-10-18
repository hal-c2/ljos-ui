//! The windows of an engine, and what a host does with them.
//!
//! A [`super::Window`] says what it wants (a title, a size, to be shown);
//! a host that owns platform windows reads that as a [`WindowSpec`] and
//! tells the window what the platform made of it. Without a host a window
//! is an item of the scene it was put in, and showing, sizing and
//! activating happen right here, so that an application behaves the same
//! in a test without a display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId, Value, WriteKind};
use ljos_quick::{ColorScheme, Item, Rectangle};

use super::app::{self, Application, ApplicationState, ScreenSpec};
use super::{Window, state_of};

/// How a shown window is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowShow {
    #[default]
    Windowed,
    Minimized,
    Maximized,
    FullScreen,
}

impl WindowShow {
    /// From a `Window.Visibility` that shows.
    pub(super) fn from_qt(v: i32) -> Option<WindowShow> {
        match v {
            2 => Some(WindowShow::Windowed),
            3 => Some(WindowShow::Minimized),
            4 => Some(WindowShow::Maximized),
            5 => Some(WindowShow::FullScreen),
            _ => None,
        }
    }

    pub(super) fn qt_value(self) -> i32 {
        match self {
            WindowShow::Windowed => 2,
            WindowShow::Minimized => 3,
            WindowShow::Maximized => 4,
            WindowShow::FullScreen => 5,
        }
    }
}

/// What the window system draws behind a window, seen through what the
/// window's colour leaves see-through (`Window.backdrop`, Ljós's own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBackdrop {
    #[default]
    None,
    /// What is behind the window, blurred.
    Blur,
    /// The window system's own glass; a blur where it has none.
    Glass,
}

impl WindowBackdrop {
    /// From a `Window.Backdrop`; a number that names none is none.
    pub(super) fn from_qt(v: i32) -> WindowBackdrop {
        match v {
            1 => WindowBackdrop::Blur,
            2 => WindowBackdrop::Glass,
            _ => WindowBackdrop::None,
        }
    }
}

/// `Qt.WindowModality`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modality {
    #[default]
    NonModal,
    /// Keeps its user out of the windows it is transient for.
    Window,
    /// Keeps its user out of every other window.
    Application,
}

/// Something a window asked for that is no property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowRequest {
    /// `requestActivate()`: take the keyboard.
    Activate,
    Raise,
    Lower,
    /// `alert(msec)`: ask for the user's attention.
    Alert,
    /// `startSystemMove()`: let the user move the window with the pointer
    /// that is pressed.
    StartMove,
    /// `startSystemResize(edges)`: the same for resizing it by the edge or
    /// the corner [`ResizeEdges`] names.
    StartResize(ResizeEdges),
}

/// One edge of a window, or two that meet in a corner: a `Qt.Edges` that
/// `startSystemResize` accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeEdges {
    pub top: bool,
    pub left: bool,
    pub right: bool,
    pub bottom: bool,
}

impl ResizeEdges {
    /// From `Qt.TopEdge` (1), `LeftEdge` (2), `RightEdge` (4) and
    /// `BottomEdge` (8). `None` for no edge, for opposite edges and for
    /// anything else.
    pub fn from_qt(edges: u32) -> Option<ResizeEdges> {
        if edges & !0xf != 0 {
            return None;
        }
        let e = ResizeEdges {
            top: edges & 1 != 0,
            left: edges & 2 != 0,
            right: edges & 4 != 0,
            bottom: edges & 8 != 0,
        };
        let opposite = (e.top && e.bottom) || (e.left && e.right);
        (edges != 0 && !opposite).then_some(e)
    }
}

/// What a host shows of one window. Sizes and positions are in logical
/// pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    pub title: String,
    pub size: [f64; 2],
    /// Where the application put the window; `None` until it writes `x`
    /// or `y`, which leaves the place to the platform.
    pub position: Option<[f64; 2]>,
    pub min_size: [f64; 2],
    /// `None` on an axis without a limit.
    pub max_size: [Option<f64>; 2],
    pub visible: bool,
    pub show: WindowShow,
    /// The `Qt.WindowFlags` as written.
    pub flags: u32,
    pub modality: Modality,
    /// Red, green, blue, alpha.
    pub color: [u8; 4],
    pub opacity: f64,
    pub transient_parent: Option<ObjectId>,
    pub backdrop: WindowBackdrop,
    /// The colour scheme in effect, which a backdrop's material follows.
    pub color_scheme: ColorScheme,
}

/// `Qt.FramelessWindowHint`.
pub const FRAMELESS: u32 = 0x0000_0800;
/// `Qt.WindowStaysOnTopHint`.
pub const STAYS_ON_TOP: u32 = 0x0004_0000;
/// `Qt.WindowStaysOnBottomHint`.
pub const STAYS_ON_BOTTOM: u32 = 0x0400_0000;
/// The bits of `Qt.WindowFlags` that say what kind of window it is
/// (`Qt.Dialog` is 3, `Qt.Popup` 9, `Qt.Tool` 11, `Qt.ToolTip` 13,
/// `Qt.SplashScreen` 15).
pub const KIND_MASK: u32 = 0x0000_00ff;

/// Qt's "no maximum": the largest size a window may be given.
pub(super) const NO_MAXIMUM: i32 = 16_777_215;

/// The windows of one engine.
#[derive(Default)]
pub struct WindowSystem {
    windows: RefCell<Vec<ObjectId>>,
    hosted: Cell<bool>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
    requests: RefCell<Vec<(ObjectId, WindowRequest)>>,
    /// The displays: what the host said, and the object of each.
    screens: RefCell<Vec<(ScreenSpec, ObjectId)>>,
    app: Cell<Option<ObjectId>>,
    app_state: Cell<ApplicationState>,
    /// Counts the windows that closed (`close()`, or the user), which is
    /// how a host tells closing from hiding.
    closed: Cell<u64>,
}

impl std::fmt::Debug for WindowSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowSystem")
            .field("windows", &self.windows.borrow().len())
            .field("hosted", &self.hosted.get())
            .finish_non_exhaustive()
    }
}

impl WindowSystem {
    /// The one of `engine`.
    pub fn of(engine: &Engine) -> Rc<WindowSystem> {
        engine.extension::<WindowSystem>()
    }

    /// A host takes the windows over: from now on every `Window` is a
    /// tree of its own for the host to show in a platform window, and
    /// `changed` is called (on the engine's thread, while it works)
    /// whenever a window appears or wants something else than before.
    /// Call it before the first window is made.
    pub fn set_host(&self, changed: impl Fn() + 'static) {
        self.hosted.set(true);
        *self.changed.borrow_mut() = Some(Rc::new(changed));
    }

    /// Whether a host shows the windows.
    pub fn is_hosted(&self) -> bool {
        self.hosted.get()
    }

    pub(super) fn notify(&self) {
        let hook = self.changed.borrow().clone();
        if let Some(hook) = hook {
            hook();
        }
    }

    pub(super) fn add(&self, win: ObjectId) {
        self.windows.borrow_mut().push(win);
    }

    /// The windows there are, in the order they were made.
    pub fn windows(&self, engine: &Engine) -> Vec<ObjectId> {
        let mut all = self.windows.borrow_mut();
        all.retain(|w| engine.is_alive(*w));
        all.clone()
    }

    /// What `win` wants to be. `None` when it is no window.
    pub fn spec(&self, engine: &Engine, win: ObjectId) -> Option<WindowSpec> {
        engine.untrack(|| self.spec_tracked(engine, win))
    }

    /// [`WindowSystem::spec`], read so that an effect runs again when the
    /// window wants something else.
    pub(super) fn spec_tracked(&self, engine: &Engine, win: ObjectId) -> Option<WindowSpec> {
        let st = state_of(engine, win)?;
        {
            let int = |p: ljos_engine::PropIndex| engine.read(win, p).to_number() as i32;
            let num = |p: ljos_engine::PropIndex| engine.read(win, p).to_number();
            let limit = |p| Some(int(p)).filter(|v| *v < NO_MAXIMUM).map(f64::from);
            let visible = engine.read(win, Window::VISIBLE.index).to_boolean();
            let color = match engine.read(win, Rectangle::COLOR.index) {
                Value::Color(c) => c.to_rgba8(),
                _ => [255; 4],
            };
            Some(WindowSpec {
                title: engine
                    .read(win, Window::TITLE.index)
                    .to_js_string()
                    .to_string(),
                size: [num(Item::WIDTH.index), num(Item::HEIGHT.index)],
                position: st
                    .placed
                    .get()
                    .then(|| [num(Window::X.index), num(Window::Y.index)]),
                min_size: [
                    f64::from(int(Window::MINIMUM_WIDTH.index).max(0)),
                    f64::from(int(Window::MINIMUM_HEIGHT.index).max(0)),
                ],
                max_size: [
                    limit(Window::MAXIMUM_WIDTH.index),
                    limit(Window::MAXIMUM_HEIGHT.index),
                ],
                visible,
                show: WindowShow::from_qt(st.wanted.get()).unwrap_or_default(),
                flags: int(Window::FLAGS.index) as u32,
                modality: match int(Window::MODALITY.index) {
                    1 => Modality::Window,
                    2 => Modality::Application,
                    _ => Modality::NonModal,
                },
                color,
                opacity: num(Item::OPACITY.index),
                transient_parent: engine
                    .read(win, Window::TRANSIENT_PARENT.index)
                    .as_object()
                    .filter(|p| engine.is_alive(*p)),
                backdrop: WindowBackdrop::from_qt(int(Window::BACKDROP.index)),
                color_scheme: ljos_quick::color_scheme(engine),
            })
        }
    }

    pub(super) fn request(&self, win: ObjectId, what: WindowRequest) {
        self.requests.borrow_mut().push((win, what));
        self.notify();
    }

    /// What the windows asked for since the last call, in order.
    pub fn take_requests(&self) -> Vec<(ObjectId, WindowRequest)> {
        std::mem::take(&mut *self.requests.borrow_mut())
    }

    /// The user asked for `win` to be closed: the window emits `closing`
    /// and is hidden unless a handler refuses. Whether it closed.
    pub fn request_close(&self, engine: &Engine, win: ObjectId) -> bool {
        super::close(engine, win)
    }

    pub(super) fn count_close(&self) {
        self.closed.set(self.closed.get() + 1);
    }

    /// How many windows closed so far. A host that ends the application
    /// with its last window looks when this has grown.
    pub fn closed_count(&self) -> u64 {
        self.closed.get()
    }

    /// The platform says `win` has this size now.
    pub fn resized(&self, engine: &Engine, win: ObjectId, size: [f64; 2]) {
        let Some(st) = state_of(engine, win) else {
            return;
        };
        st.own(|| {
            engine.batch(|| {
                for (prop, v) in [(Item::WIDTH.index, size[0]), (Item::HEIGHT.index, size[1])] {
                    if engine.untrack(|| engine.read(win, prop)).to_number() != v {
                        let _ = engine.write(win, prop, Value::Real(v), WriteKind::Host);
                    }
                }
            });
        });
    }

    /// The platform says `win` is at this place now.
    pub fn moved(&self, engine: &Engine, win: ObjectId, at: [f64; 2]) {
        let Some(st) = state_of(engine, win) else {
            return;
        };
        st.own(|| {
            engine.batch(|| {
                for (prop, v) in [(Window::X.index, at[0]), (Window::Y.index, at[1])] {
                    let v = v.round() as i32;
                    if engine.untrack(|| engine.read(win, prop)).to_number() as i32 != v {
                        let _ = engine.write_internal(win, prop, Value::Int(v), WriteKind::Host);
                    }
                }
            });
        });
    }

    /// The platform says `win` is shown this way now (its user maximized
    /// it, or brought it back).
    pub fn shown_as(&self, engine: &Engine, win: ObjectId, show: WindowShow) {
        let Some(st) = state_of(engine, win) else {
            return;
        };
        let visible = engine
            .untrack(|| engine.read(win, Window::VISIBLE.index))
            .to_boolean();
        if !visible || st.wanted.get() == show.qt_value() {
            return;
        }
        st.wanted.set(show.qt_value());
        st.own(|| {
            let _ = engine.write_internal(
                win,
                Window::VISIBILITY.index,
                Value::Int(show.qt_value()),
                WriteKind::Host,
            );
        });
    }

    /// Whether a modal window keeps the user out of `win`: an
    /// application-modal window is shown, or a window-modal one that is
    /// transient for `win`. The host drops the input `win` would get.
    pub fn blocked(&self, engine: &Engine, win: ObjectId) -> bool {
        self.windows(engine).into_iter().any(|m| {
            if m == win {
                return false;
            }
            let Some(spec) = self.spec(engine, m).filter(|s| s.visible) else {
                return false;
            };
            match spec.modality {
                Modality::NonModal => false,
                Modality::Application => true,
                Modality::Window => super::transient_chain(engine, m).contains(&win),
            }
        })
    }

    // -------------------------------------------------------------- screens

    /// The host says which displays there are, the primary one first.
    /// Each keeps its object while it is in the list at the same place.
    pub fn set_screens(&self, engine: &Engine, specs: &[ScreenSpec]) {
        let mut screens = self.screens.borrow_mut();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for (i, spec) in specs.iter().enumerate() {
            match screens.get_mut(i) {
                Some((old, obj)) if engine.is_alive(*obj) => {
                    if old != spec {
                        app::update_screen(engine, *obj, spec);
                        *old = spec.clone();
                    }
                }
                _ => {
                    let Ok(obj) = app::make_screen(engine, spec) else {
                        continue;
                    };
                    if i < screens.len() {
                        screens[i] = (spec.clone(), obj);
                    } else {
                        screens.push((spec.clone(), obj));
                    }
                    added.push(obj);
                }
            }
        }
        while screens.len() > specs.len().max(1) {
            if let Some((_, obj)) = screens.pop() {
                removed.push(obj);
            }
        }
        drop(screens);
        if let Some(app) = self.app.get().filter(|a| engine.is_alive(*a)) {
            for obj in &removed {
                let _ = engine.list_remove(app, Application::SCREENS.index, *obj);
            }
            for obj in &added {
                let _ = engine.list_push(app, Application::SCREENS.index, *obj);
            }
        }
        // A window on a display that went is on the primary one now.
        if !removed.is_empty() {
            let primary = self.screen(engine, 0);
            for win in self.windows(engine) {
                let on = engine
                    .untrack(|| engine.read(win, Window::SCREEN.index))
                    .as_object();
                if on.is_some_and(|o| removed.contains(&o)) {
                    super::write_screen(engine, win, primary);
                }
            }
        }
        for obj in removed {
            engine.destroy(obj);
        }
    }

    /// The object of display `index`; of the primary one when there is
    /// no such display. Made on first use: one screen of 800 by 800 until
    /// a host says otherwise.
    pub fn screen(&self, engine: &Engine, index: usize) -> Option<ObjectId> {
        if self.screens.borrow().is_empty() {
            let spec = ScreenSpec::default();
            let obj = app::make_screen(engine, &spec).ok()?;
            self.screens.borrow_mut().push((spec, obj));
        }
        let screens = self.screens.borrow();
        screens.get(index).or(screens.first()).map(|(_, o)| *o)
    }

    /// The size of the display `win` is on.
    pub(super) fn screen_size(&self, engine: &Engine, win: ObjectId) -> [f64; 2] {
        let on = engine
            .untrack(|| engine.read(win, Window::SCREEN.index))
            .as_object();
        let screens = self.screens.borrow();
        screens
            .iter()
            .find(|(_, o)| Some(*o) == on)
            .or(screens.first())
            .map_or(ScreenSpec::default().size, |(s, _)| s.size)
    }

    /// The host says which display `win` is on.
    pub fn set_window_screen(&self, engine: &Engine, win: ObjectId, index: usize) {
        if state_of(engine, win).is_some() {
            super::write_screen(engine, win, self.screen(engine, index));
        }
    }

    // ---------------------------------------------------------- application

    /// The host says how much of the application its user has.
    pub fn set_application_state(&self, engine: &Engine, state: ApplicationState) {
        if self.app_state.replace(state) == state {
            return;
        }
        if let Some(app) = self.app.get().filter(|a| engine.is_alive(*a)) {
            self.show_application_state(engine, app);
        }
    }

    pub fn application_state(&self) -> ApplicationState {
        self.app_state.get()
    }

    fn show_application_state(&self, engine: &Engine, app: ObjectId) {
        let state = self.app_state.get();
        engine.batch(|| {
            let _ = engine.write_internal(
                app,
                Application::STATE.index,
                Value::Int(state.qt_value()),
                WriteKind::Imperative,
            );
            let _ = engine.write_internal(
                app,
                Application::ACTIVE.index,
                Value::Bool(state == ApplicationState::Active),
                WriteKind::Imperative,
            );
        });
    }

    /// The singleton was made: it says what the host said so far.
    pub(super) fn adopt_application(&self, engine: &Engine, app: ObjectId) {
        self.app.set(Some(app));
        self.show_application_state(engine, app);
        self.screen(engine, 0);
        let all: Vec<ObjectId> = self.screens.borrow().iter().map(|(_, o)| *o).collect();
        for obj in all {
            let _ = engine.list_push(app, Application::SCREENS.index, obj);
        }
    }
}
