//! The shell's loop (ARCHITECTURE.md §7.3): what [`crate::run`] runs.
//!
//! One platform window for the root, and one for every other QML
//! `Window` that is shown. Each has an egui context, a view of its scene,
//! a painter and, while it is open, a surface. The loop sleeps: a window
//! is drawn when its context asks for a pass (input, a timer, an
//! animation, a message, dirty state) and at no other time.
//!
//! Every event of the host ends with [`ljos_host::Event::AboutToWait`],
//! and all the bookkeeping happens there ([`Desktop::settle`]): what the
//! windows of the application want is compared with what the platform
//! has, due passes are asked for, and the host is told when to wake.

use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::ViewportId;
use egui::accesskit::{TreeId, Uuid};
use egui_wgpu::winit::Painter;
use egui_winit::accesskit_winit;
use egui_winit::winit::event::WindowEvent as RawEvent;
use egui_winit::winit::event_loop::EventLoopProxy;
use ljos_controls::{
    ApplicationState, Modality, ScreenSpec, WindowRequest, WindowSpec, WindowSystem,
};
use ljos_egui::{QmlView, ResizeMode};
use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_host::{
    AppState, ButtonState, Event, HostApp, HostControl, PlatformHost, PointerKind, Screen, Wake,
    WindowChange, WindowEvent, WindowId, WindowState, WinitControl, WinitHost,
};
use ljos_runtime::{MemoryPressure, SystemClock};

use crate::repaint::RepaintSchedule;
use crate::surface::{self, Gpu, Surface};
use crate::{Hosting, Setup, SetupError, ShellApp, ShellConfig, ShellError, windows};

/// The log category of what the shell has to say.
const CATEGORY: &str = "ljos.shell";

/// How often what the windows want is compared with what they have
/// within one turn of the loop, when applying it changes it again (the
/// platform's size is written to the window, which is a change).
const ROUNDS: usize = 4;

/// How long the platform has to answer a size the application asked for.
/// After that the window is told the size it has: a tiling window manager
/// answers nothing when it refuses.
const SIZE_CHECK: Duration = Duration::from_millis(250);

/// How long the platform has to let a window draw before the window is
/// taken to be out of sight. Wayland lets a window draw when it is shown
/// and says nothing when it is not.
const STALL: Duration = Duration::from_millis(250);

/// The shortest time between two passes that are not drawn, for the
/// root's window: its passes run the engine's timers and deliver the
/// messages, seen or not.
const HEARTBEAT: Duration = Duration::from_millis(16);

/// The same for another window that is out of sight: only its own
/// animations wait for it.
const BACKGROUND: Duration = Duration::from_millis(100);

/// What other threads, egui and the accessibility adapter post into the
/// loop.
pub(crate) enum ShellEvent {
    /// The host's waker fired; there is nothing to do but look.
    Wake,
    /// A context asked for a pass (`egui::Context::request_repaint`).
    Repaint {
        slot: u64,
        when: Instant,
        /// The context's pass count when it asked.
        pass: u64,
    },
    AccessKit(accesskit_winit::Event),
}

impl std::fmt::Debug for ShellEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellEvent::Wake => f.write_str("Wake"),
            ShellEvent::Repaint { slot, .. } => write!(f, "Repaint({slot})"),
            ShellEvent::AccessKit(_) => f.write_str("AccessKit"),
        }
    }
}

impl From<Wake> for ShellEvent {
    fn from(_: Wake) -> Self {
        ShellEvent::Wake
    }
}

impl From<accesskit_winit::Event> for ShellEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        ShellEvent::AccessKit(event)
    }
}

/// The way into the loop, for the repaint callbacks, which any thread may
/// call.
type Proxy = Arc<Mutex<EventLoopProxy<ShellEvent>>>;

type Control<'a> = WinitControl<'a, ShellEvent>;

/// A context for one window. Asking it for a repaint posts into the loop,
/// which is what wakes it.
fn new_context(key: u64, proxy: &Proxy) -> egui::Context {
    let ctx = egui::Context::default();
    // A second pass when a widget asks to be laid out again, as egui's
    // own hosts allow.
    ctx.options_mut(|o| o.max_passes = NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
    let proxy = Arc::clone(proxy);
    ctx.set_request_repaint_callback(move |info| {
        // A delay too long to say is "not before something else happens".
        let Some(when) = Instant::now().checked_add(info.delay) else {
            return;
        };
        if let Ok(proxy) = proxy.lock() {
            // Fails only when the loop has ended.
            drop(proxy.send_event(ShellEvent::Repaint {
                slot: key,
                when,
                pass: info.current_cumulative_pass_nr,
            }));
        }
    });
    ctx
}

/// The AccessKit tree of window `key`'s scene. Never the nil id, which is
/// the tree of egui itself.
fn tree_id(key: u64) -> TreeId {
    const SCENES: u128 = 0x6c6a_6f73_7368_656c_6c00_0000_0000_0000;
    TreeId(Uuid::from_u128(SCENES | (u128::from(key) + 1)))
}

fn scaled(size: [f64; 2], by: f64) -> [f64; 2] {
    [size[0] * by, size[1] * by]
}

fn earlier(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// One window of the application: what stays while it is hidden (the
/// context, the view, the painter with its pictures) and what it has
/// while it is open (the surface).
struct Slot {
    key: u64,
    /// The QML `Window` shown. `None` for a root that is a plain item,
    /// which the shell gives a window of its own.
    window: Option<ObjectId>,
    ctx: egui::Context,
    /// `None` for the root's window: its view is the application's.
    view: Option<QmlView>,
    surface: Option<Surface>,
    painter: Option<Painter>,
    /// What the platform's window was last made to be.
    applied: Option<WindowSpec>,
    /// The accessibility adapter was sent the scene's tree.
    told: bool,
    /// Pictures the painter has not been given yet.
    pending: egui::TexturesDelta,
    /// What the window asked for through egui (cut, copy, paste).
    actions: Vec<egui_winit::ActionRequested>,
    /// When the platform was asked to let the window draw, and has not.
    redraw_asked: Option<Instant>,
    /// The platform did not let it draw in time: it is out of sight.
    stalled: bool,
    /// The platform said that nothing of the window can be seen.
    occluded: bool,
    /// When a pass that is not drawn last ran, and when the next is due.
    headless_at: Option<Instant>,
    headless_due: Option<Instant>,
    /// When to see whether the platform gave the size that was asked for.
    size_check: Option<Instant>,
    /// What the QML window was last told of its platform window.
    told_size: Option<[f64; 2]>,
    told_pos: Option<[f64; 2]>,
    told_state: Option<WindowState>,
    /// A modal window keeps the user out of this one.
    blocked: bool,
    /// The platform refused the window; it is not asked again.
    broken: bool,
    /// `requestActivate()` came before the window was on the screen.
    activate: bool,
    /// Which display it is on, and how often that one draws.
    screen: Option<usize>,
    refresh: Option<f64>,
    /// egui's time for this window counts from here.
    started: Instant,
    clear: egui::Color32,
}

impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slot")
            .field("key", &self.key)
            .field("window", &self.window)
            .field("surface", &self.surface)
            .finish_non_exhaustive()
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        // egui takes a delta that is dropped with pictures in it for a
        // mistake. Here it is none: the window they were for is gone.
        self.pending.clear();
    }
}

impl Slot {
    fn new(key: u64, window: Option<ObjectId>, ctx: egui::Context, view: Option<QmlView>) -> Self {
        Self {
            key,
            window,
            ctx,
            view,
            surface: None,
            painter: None,
            applied: None,
            told: false,
            pending: egui::TexturesDelta::default(),
            actions: Vec::new(),
            redraw_asked: None,
            stalled: false,
            occluded: false,
            headless_at: None,
            headless_due: None,
            size_check: None,
            told_size: None,
            told_pos: None,
            told_state: None,
            blocked: false,
            broken: false,
            activate: false,
            screen: None,
            refresh: None,
            started: Instant::now(),
            clear: egui::Color32::WHITE,
        }
    }

    fn is_root(&self) -> bool {
        self.view.is_none()
    }

    /// Open, but nothing of it is on the screen: drawing it would be for
    /// nobody, and on some platforms would not wait for the display.
    fn out_of_sight(&self) -> bool {
        self.occluded
            || self.stalled
            || self
                .surface
                .as_ref()
                .is_some_and(|s| s.window.is_minimized() == Some(true))
    }

    /// A pass that is not drawn is due at `now`, or as soon after it as
    /// such passes may follow each other.
    fn want_headless(&mut self, now: Instant) {
        let gap = if self.is_root() {
            HEARTBEAT
        } else {
            BACKGROUND
        };
        let at = self.headless_at.map_or(now, |last| (last + gap).max(now));
        self.headless_due = Some(self.headless_due.map_or(at, |due| due.min(at)));
    }

    /// The platform's window is gone, or another one: nothing that was
    /// known of it holds.
    fn forget_platform(&mut self) {
        self.told = false;
        self.redraw_asked = None;
        self.stalled = false;
        self.occluded = false;
        self.headless_due = None;
        self.size_check = None;
        self.told_size = None;
        self.told_pos = None;
        self.told_state = None;
        self.activate = false;
        self.screen = None;
    }
}

/// Tells the QML window the size its platform window has, when that is
/// another than it was last told (or `force`: the application wrote a
/// size the platform did not give).
fn note_size(slot: &mut Slot, engine: &Engine, sys: &WindowSystem, force: bool) {
    let (Some(surface), Some(painter)) = (slot.surface.as_mut(), slot.painter.as_mut()) else {
        return;
    };
    let Some(size) = surface.fit(&slot.ctx, painter) else {
        return;
    };
    if force || slot.told_size != Some(size) {
        slot.told_size = Some(size);
        if let Some(win) = slot.window {
            sys.resized(engine, win, size);
        }
    }
}

/// Tells the QML window how its platform window is shown (its user
/// maximized it, or brought it back).
fn note_state(slot: &mut Slot, engine: &Engine, sys: &WindowSystem) {
    let Some(surface) = slot.surface.as_ref() else {
        return;
    };
    // Before it is on the screen the platform has not done what the
    // window was opened with.
    if !surface.shown {
        return;
    }
    let state = surface.shown_as();
    if slot.told_state != Some(state) {
        slot.told_state = Some(state);
        if let Some(win) = slot.window {
            sys.shown_as(engine, win, windows::show(state));
        }
    }
}

/// What a modal window keeps from the windows behind it: everything a
/// user starts. Releases and the pointer leaving still arrive, so that
/// nothing stays held.
fn starts_input(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Text(_) | WindowEvent::Touch(_) => true,
        WindowEvent::Key(key) => key.state == ButtonState::Pressed,
        WindowEvent::Pointer(pointer) => matches!(
            pointer.kind,
            PointerKind::Moved
                | PointerKind::Entered
                | PointerKind::Wheel(_)
                | PointerKind::Button(_, ButtonState::Pressed)
        ),
        _ => false,
    }
}

fn screen_spec(screen: &Screen) -> ScreenSpec {
    let scale = if screen.scale > 0.0 {
        screen.scale
    } else {
        1.0
    };
    ScreenSpec {
        name: screen.name.clone(),
        size: screen.logical_size(),
        position: [
            f64::from(screen.position[0]) / scale,
            f64::from(screen.position[1]) / scale,
        ],
        scale,
    }
}

/// The application behind the host.
struct Desktop<M> {
    app: ShellApp<M>,
    config: ShellConfig,
    /// The first is the root's window and is always there.
    slots: Vec<Slot>,
    repaints: RepaintSchedule<u64>,
    proxy: Proxy,
    /// A QML window wants something else than its platform window has.
    dirty: Rc<Cell<bool>>,
    /// The device of the first window that opened.
    gpu: Option<Gpu>,
    /// Why the loop ended, when it was not asked to.
    failure: Rc<RefCell<Option<ShellError>>>,
    screens: Vec<Screen>,
    screens_stale: bool,
    /// How many windows had closed when it was last asked whether the
    /// application ends.
    closed: u64,
    /// The user closed the window of a root that is a plain item.
    root_closed: bool,
    next_key: u64,
    last_root: Option<ObjectId>,
    /// Windows can be drawn into.
    live: bool,
    app_state: AppState,
    /// Windows that cannot be shown; each was said once.
    refused: Vec<ObjectId>,
    warned_hidden_root: bool,
    exiting: bool,
}

impl<M> std::fmt::Debug for Desktop<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Desktop")
            .field("slots", &self.slots)
            .field("repaints", &self.repaints)
            .finish_non_exhaustive()
    }
}

impl<M: Send + 'static> Desktop<M> {
    fn new(
        app: ShellApp<M>,
        config: ShellConfig,
        ctx: egui::Context,
        proxy: Proxy,
        dirty: Rc<Cell<bool>>,
        failure: Rc<RefCell<Option<ShellError>>>,
    ) -> Self {
        Self {
            app,
            config,
            slots: vec![Slot::new(0, None, ctx, None)],
            repaints: RepaintSchedule::default(),
            proxy,
            dirty,
            gpu: None,
            failure,
            screens: Vec::new(),
            screens_stale: true,
            closed: 0,
            root_closed: false,
            next_key: 1,
            last_root: None,
            live: false,
            app_state: AppState::Inactive,
            refused: Vec::new(),
            warned_hidden_root: false,
            exiting: false,
        }
    }

    fn slot_of(&self, id: WindowId) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.surface.as_ref().is_some_and(|s| s.id == id))
    }

    /// The window the shell gives a root that is a plain item.
    fn root_spec(&self) -> WindowSpec {
        let [w, h] = self.config.size;
        WindowSpec {
            title: self.config.title.clone(),
            size: [f64::from(w), f64::from(h)],
            position: None,
            min_size: self
                .config
                .min_size
                .map_or([0.0; 2], |[w, h]| [f64::from(w), f64::from(h)]),
            max_size: [None, None],
            visible: !self.root_closed,
            show: ljos_controls::WindowShow::Windowed,
            // `Qt.Window`.
            flags: 1,
            modality: Modality::NonModal,
            color: self.config.background.to_rgba8(),
            opacity: 1.0,
            transient_parent: None,
        }
    }

    /// What window `i` is to be now; `None` when its QML window is gone.
    fn spec_of(&self, i: usize, engine: &Engine, sys: &WindowSystem) -> Option<WindowSpec> {
        let Some(win) = self.slots[i].window else {
            return Some(self.root_spec());
        };
        let mut spec = sys.spec(engine, win)?;
        if i == 0 {
            // What a root window leaves unsaid is the application's.
            if spec.title.is_empty() {
                spec.title.clone_from(&self.config.title);
            }
            if spec.size[0] <= 0.0 || spec.size[1] <= 0.0 {
                let [w, h] = self.config.size;
                spec.size = [f64::from(w), f64::from(h)];
            }
        }
        Some(spec)
    }

    /// Makes the platform's windows what the application's windows want.
    fn sync(&mut self, host: &mut Control<'_>, engine: &Rc<Engine>, sys: &WindowSystem) {
        self.dirty.set(false);
        let root = self.app.view().scene().root();
        if root != self.last_root {
            // A scene's timers start in its first pass, and nothing else
            // asks for that one when the root has no window to open.
            self.repaints.request(self.slots[0].key, Instant::now());
        }
        self.last_root = root;
        let root_window = root.filter(|r| sys.spec(engine, *r).is_some());
        if self.slots[0].window != root_window || self.slots[0].applied.is_none() {
            let slot = &mut self.slots[0];
            slot.window = root_window;
            slot.told_size = None;
            slot.told_pos = None;
            slot.told_state = None;
            // A window's size is the platform's to say; a plain item is
            // as large as the window the shell made for it.
            self.app
                .view_mut()
                .set_resize_mode(if root_window.is_some() {
                    ResizeMode::Fill
                } else {
                    ResizeMode::RootToView
                });
            self.app.set_backdrop(root_window.is_none());
        }

        let windows = sys.windows(engine);
        let mut i = 1;
        while i < self.slots.len() {
            let stays = self.slots[i]
                .window
                .is_some_and(|w| windows.contains(&w) && Some(w) != root_window);
            if stays {
                i += 1;
            } else {
                self.hide(host, i);
                let slot = self.slots.remove(i);
                self.repaints.forget(slot.key);
            }
        }
        self.refused.retain(|w| windows.contains(w));
        for win in &windows {
            let known = Some(*win) == root_window
                || self.refused.contains(win)
                || self.slots.iter().any(|s| s.window == Some(*win));
            // A window that was never shown needs nothing yet.
            if !known && sys.spec(engine, *win).is_some_and(|s| s.visible) {
                self.add_slot(*win, engine);
            }
        }

        for i in 0..self.slots.len() {
            if let Some(spec) = self.spec_of(i, engine, sys) {
                self.apply(host, i, spec, engine, sys);
            }
        }

        let application_modal = windows.iter().any(|w| {
            sys.spec(engine, *w)
                .is_some_and(|s| s.visible && s.modality == Modality::Application)
        });
        for slot in &mut self.slots {
            slot.blocked = match slot.window {
                Some(win) => sys.blocked(engine, win),
                None => application_modal,
            };
        }

        if let Some(win) = root_window
            && !self.warned_hidden_root
            && self.slots[0].surface.is_none()
            && !self.slots[0].broken
            && sys.closed_count() == 0
        {
            self.warned_hidden_root = true;
            engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!(
                    "the root {} is not visible: nothing is shown until its `visible` is true",
                    engine
                        .meta_of(win)
                        .map_or_else(|| "Window".to_owned(), |m| m.name.to_string())
                ),
            );
        }
    }

    /// A window of its own for `win`, which is shown for the first time.
    fn add_slot(&mut self, win: ObjectId, engine: &Rc<Engine>) {
        let key = self.next_key;
        self.next_key += 1;
        let ctx = new_context(key, &self.proxy);
        let view = QmlView::new(Rc::clone(engine), win);
        // Making a view made its scene the one the engine's timers wake.
        // That stays the root's: its passes run whether it is seen or
        // not, which no other window's do.
        self.app.view().scene().take_timer_wake(engine);
        match view {
            Ok(view) => {
                let mut view = view
                    .with_clock(Arc::clone(self.app.view().clock()))
                    .with_resize_mode(ResizeMode::Fill)
                    .with_accessibility_graft(tree_id(key));
                view.bind_context(&ctx);
                // Binding made this context the one a decoded picture
                // wakes, and that too stays the root's.
                let root_ctx = self.slots[0].ctx.clone();
                self.app.view_mut().bind_context(&root_ctx);
                self.slots.push(Slot::new(key, Some(win), ctx, Some(view)));
            }
            Err(e) => {
                engine.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!("a window cannot be shown: {e}"),
                );
                self.refused.push(win);
            }
        }
    }

    /// Opens, changes or closes the platform's window of slot `i` so that
    /// it is what `spec` says.
    fn apply(
        &mut self,
        host: &mut Control<'_>,
        i: usize,
        spec: WindowSpec,
        engine: &Engine,
        sys: &WindowSystem,
    ) {
        let caps = host.capabilities();
        let wants = spec.visible && self.live && !self.slots[i].broken;
        let open = self.slots[i].surface.is_some();
        match (open, wants) {
            (false, false) => {}
            (false, true) => self.open(host, i, &spec, engine, sys),
            (true, false) => self.hide(host, i),
            (true, true) => {
                let old = self.slots[i]
                    .applied
                    .clone()
                    .unwrap_or_else(|| spec.clone());
                if windows::needs_new_window(&old, &spec, caps) {
                    self.hide(host, i);
                    self.open(host, i, &spec, engine, sys);
                } else {
                    self.change(host, i, &old, &spec, engine, sys);
                }
            }
        }
        let slot = &mut self.slots[i];
        let [r, g, b, a] = spec.color;
        slot.clear = if windows::transparent(&spec) {
            egui::Color32::TRANSPARENT
        } else {
            egui::Color32::from_rgba_unmultiplied(r, g, b, a)
        };
        slot.applied = Some(spec);
    }

    /// Tells an open window what changed between `old` and `new`.
    fn change(
        &mut self,
        host: &mut Control<'_>,
        i: usize,
        old: &WindowSpec,
        new: &WindowSpec,
        engine: &Engine,
        sys: &WindowSystem,
    ) {
        let caps = host.capabilities();
        let slot = &mut self.slots[i];
        let Some(id) = slot.surface.as_ref().map(|s| s.id) else {
            return;
        };
        // egui's points are the platform's logical pixels times its zoom.
        let zoom = f64::from(slot.ctx.zoom_factor());
        for change in windows::changes(old, new, caps) {
            let change = match change {
                // The platform's own moves and its user's maximizing are
                // written to the window, and are not asked back of it.
                WindowChange::Position(at) if slot.told_pos == Some(at) => continue,
                WindowChange::State(state) if slot.told_state == Some(state) => continue,
                WindowChange::State(state) => {
                    slot.told_state = Some(state);
                    WindowChange::State(state)
                }
                WindowChange::MinSize(size) => WindowChange::MinSize(size.map(|s| scaled(s, zoom))),
                WindowChange::MaxSize(size) => WindowChange::MaxSize(size.map(|s| scaled(s, zoom))),
                other => other,
            };
            host.change_window(id, change);
        }
        let has = slot.told_size.unwrap_or(new.size);
        if let Some(size) = windows::size_request(old, new, has) {
            host.change_window(id, WindowChange::Size(scaled(size, zoom)));
            // Some platforms resize at once and say nothing; others
            // answer later, or never when they refuse.
            note_size(slot, engine, sys, false);
            slot.size_check = Some(Instant::now() + SIZE_CHECK);
            self.repaints.request(slot.key, Instant::now());
        }
    }

    /// Opens the platform's window of slot `i`, unseen until the first
    /// picture is in it.
    fn open(
        &mut self,
        host: &mut Control<'_>,
        i: usize,
        spec: &WindowSpec,
        engine: &Engine,
        sys: &WindowSystem,
    ) {
        let caps = host.capabilities();
        let zoom = f64::from(self.slots[i].ctx.zoom_factor());
        let mut attrs = windows::attributes(spec, caps);
        attrs.size = attrs.size.map(|s| scaled(s, zoom));
        attrs.min_size = attrs.min_size.map(|s| scaled(s, zoom));
        attrs.max_size = attrs.max_size.map(|s| scaled(s, zoom));
        // Every window of the application goes by the application's name.
        attrs.app_id = Some(self.config.title.clone());
        let id = match host.create_window(&attrs) {
            Ok(id) => id,
            Err(e) => return self.fail(host, i, ShellError::Window(e), engine),
        };
        let Some(window) = host.window(id).cloned() else {
            return;
        };
        let display = host.event_loop().owned_display_handle();
        let gpu = self.gpu.clone();
        let slot = &mut self.slots[i];
        let painter = slot
            .painter
            .get_or_insert_with(|| surface::painter(&slot.ctx, gpu.as_ref(), display));
        let surfaced =
            pollster::block_on(painter.set_window(ViewportId::ROOT, Some(Arc::clone(&window))));
        if let Err(e) = surfaced {
            drop(window);
            host.destroy_window(id);
            return self.fail(host, i, ShellError::Gpu(e.to_string()), engine);
        }
        if self.gpu.is_none() {
            self.gpu = Gpu::of(painter);
        }
        let mut state = egui_winit::State::new(
            slot.ctx.clone(),
            ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            host.event_loop().system_theme(),
            painter.max_texture_side(),
        );
        // While the window is unseen: the adapter has to be there before
        // the platform asks about the window.
        state.init_accesskit(host.event_loop(), &window, host.proxy().clone());
        slot.forget_platform();
        slot.surface = Some(Surface::new(id, window, state));
        slot.told_state = Some(attrs.state);
        note_size(slot, engine, sys, false);
        self.repaints.request(slot.key, Instant::now());
        self.screens_stale = true;
        match self.slots[i].view.as_mut() {
            Some(view) => view.set_drawn(true),
            None => self.app.view_mut().set_drawn(true),
        }
    }

    /// The platform refused a window. Without the root's there is no
    /// application to show; another window is left out.
    fn fail(&mut self, host: &mut Control<'_>, i: usize, error: ShellError, engine: &Engine) {
        self.slots[i].broken = true;
        if i == 0 {
            *self.failure.borrow_mut() = Some(error);
            self.exiting = true;
            host.exit();
        } else {
            engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!("a window cannot be shown: {error}"),
            );
        }
    }

    /// Takes the window of slot `i` off the screen. The surface goes
    /// before the window it draws into.
    fn hide(&mut self, host: &mut Control<'_>, i: usize) {
        let slot = &mut self.slots[i];
        let Some(surface) = slot.surface.take() else {
            return;
        };
        if let Some(painter) = slot.painter.as_mut() {
            painter.gc_viewports(&egui::ViewportIdSet::default());
        }
        let id = surface.id;
        drop(surface);
        host.destroy_window(id);
        slot.forget_platform();
        // Nothing listens to a window that is not there; the next one
        // has an adapter of its own, which asks anew.
        slot.ctx.disable_accesskit();
        self.repaints.forget(slot.key);
        self.screens_stale = true;
        // What only drawing needs is built again when it is shown
        // (ARCHITECTURE §8.5); its scene keeps running meanwhile.
        let view = match self.slots[i].view.as_mut() {
            Some(view) => view,
            None => self.app.view_mut(),
        };
        view.set_drawn(false);
        view.trim(MemoryPressure::Critical);
    }

    /// The system is short of memory: hidden windows give back all they
    /// can rebuild, shown ones what they did not draw lately.
    fn trim(&mut self) {
        for i in 0..self.slots.len() {
            let level = if self.slots[i].surface.is_some() {
                MemoryPressure::Moderate
            } else {
                MemoryPressure::Critical
            };
            let view = match self.slots[i].view.as_mut() {
                Some(view) => view,
                None => self.app.view_mut(),
            };
            view.trim(level);
        }
    }

    /// One pass of slot `i`: input, the scene, and (`paint`) the picture
    /// into its window. A pass that is not painted is for a window that
    /// cannot be seen, or that the root does not have: its scene runs all
    /// the same.
    fn pass(&mut self, host: &mut Control<'_>, i: usize, paint: bool) {
        let engine = Rc::clone(self.app.engine());
        let sys = WindowSystem::of(&engine);
        let Self {
            app,
            slots,
            repaints,
            config,
            screens_stale,
            ..
        } = self;
        let slot = &mut slots[i];
        // What was asked until now, this pass answers. egui forgets a
        // delayed repaint with every pass too, and is asked again.
        repaints.forget(slot.key);
        if paint {
            slot.redraw_asked = None;
            slot.stalled = false;
        } else {
            slot.headless_at = Some(Instant::now());
        }
        note_size(slot, &engine, &sys, false);

        let mut raw = match slot.surface.as_mut() {
            Some(surface) => {
                egui_winit::update_viewport_info(
                    &mut surface.info,
                    &slot.ctx,
                    &surface.window,
                    false,
                );
                let mut raw = surface.state.take_egui_input(&surface.window);
                raw.viewports.insert(ViewportId::ROOT, surface.info.clone());
                raw
            }
            None => {
                // No window: the scene is as large as it would be.
                let size = slot
                    .applied
                    .as_ref()
                    .map(|s| s.size)
                    .filter(|s| s[0] > 0.0 && s[1] > 0.0)
                    .map_or(config.size, |s| [s[0] as f32, s[1] as f32]);
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    focused: false,
                    ..egui::RawInput::default()
                }
            }
        };
        raw.time = Some(slot.started.elapsed().as_secs_f64());

        let output = match slot.view.as_mut() {
            None => slot.ctx.run_ui(raw, |ui| {
                app.begin_pass();
                app.show(ui);
            }),
            Some(view) => slot.ctx.run_ui(raw, |ui| {
                view.show(ui);
            }),
        };
        let egui::FullOutput {
            mut platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = output;
        slot.pending.append(textures_delta);

        let Some(surface) = slot.surface.as_mut() else {
            surface::forget_freed(&mut slot.pending);
            surface::free_now(slot.painter.as_ref(), &mut slot.pending);
            return;
        };
        surface.info.events.clear();

        // egui's tree first, then the scene's, which hangs in it (§14).
        if let Some(root) = platform_output.accesskit_update.take() {
            let view = match slot.view.as_mut() {
                Some(view) => view,
                None => app.view_mut(),
            };
            let updates = view.graft_updates(root, &mut slot.told);
            if let Some(adapter) = surface.state.accesskit.as_mut() {
                adapter.update_if_active(|| updates.root);
                if let Some(grafted) = updates.grafted {
                    adapter.update_if_active(|| grafted);
                }
            }
            if updates.again {
                slot.ctx.request_repaint();
            }
        }
        surface.state.handle_platform_output_with_event_loop(
            &surface.window,
            host.event_loop(),
            platform_output,
        );
        if let Some(out) = viewport_output.get(&ViewportId::ROOT) {
            egui_winit::process_viewport_commands(
                &slot.ctx,
                &mut surface.info,
                out.commands.iter().cloned(),
                &surface.window,
                &mut slot.actions,
            );
        }
        for action in slot.actions.drain(..) {
            let event = match action {
                egui_winit::ActionRequested::Cut => egui::Event::Cut,
                egui_winit::ActionRequested::Copy => egui::Event::Copy,
                egui_winit::ActionRequested::Paste => {
                    match surface.state.clipboard_text().filter(|t| !t.is_empty()) {
                        Some(text) => egui::Event::Paste(text.replace("\r\n", "\n")),
                        None => continue,
                    }
                }
                // Pictures of a window are the development tools' to take.
                egui_winit::ActionRequested::Screenshot(_) => continue,
            };
            surface.state.egui_input_mut().events.push(event);
        }

        let Some(painter) = slot.painter.as_mut().filter(|_| paint) else {
            surface::forget_freed(&mut slot.pending);
            return;
        };
        let primitives = slot.ctx.tessellate(shapes, pixels_per_point);
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            pixels_per_point,
            slot.clear.to_normalized_gamma_f32(),
            &primitives,
            &mut slot.pending,
            Vec::new(),
            &surface.window,
        );
        if !surface.shown {
            surface.shown = true;
            host.change_window(surface.id, WindowChange::Visible(true));
            if std::mem::take(&mut slot.activate) {
                host.change_window(surface.id, WindowChange::Activate);
            }
            // Only now does the platform say which display it is on.
            *screens_stale = true;
        }
    }

    fn window_event(
        &mut self,
        host: &mut Control<'_>,
        id: WindowId,
        event: &WindowEvent,
        raw: Option<&RawEvent>,
    ) {
        let Some(i) = self.slot_of(id) else {
            return;
        };
        let engine = Rc::clone(self.app.engine());
        let sys = WindowSystem::of(&engine);
        match event {
            WindowEvent::RedrawRequested => return self.pass(host, i, true),
            WindowEvent::CloseRequested => return self.close_requested(i, &engine, &sys),
            WindowEvent::Destroyed => return,
            _ => {}
        }
        let caps = host.capabilities();
        let slot = &mut self.slots[i];
        if slot.blocked && starts_input(event) {
            return;
        }
        let Some(surface) = slot.surface.as_mut() else {
            return;
        };
        if slot.told && matches!(event, WindowEvent::Focus(_)) {
            // An adapter cannot take the window's focus changing while
            // the focus is in the scene's tree
            // (`QmlView::release_graft_focus`); the pass the change asks
            // for says where it is. An adapter that was not told the
            // trees has no focus to release, and must not hear of one
            // before it hears of them.
            let view = match slot.view.as_mut() {
                Some(view) => view,
                None => self.app.view_mut(),
            };
            if let Some(release) = view.release_graft_focus()
                && let Some(adapter) = surface.state.accesskit.as_mut()
            {
                adapter.update_if_active(|| release);
                self.repaints.request(slot.key, Instant::now());
            }
        }
        if let Some(raw) = raw
            && surface.state.on_window_event(&surface.window, raw).repaint
        {
            self.repaints.request(slot.key, Instant::now());
        }
        match *event {
            WindowEvent::Resized { .. } => {
                note_size(slot, &engine, &sys, false);
                note_state(slot, &engine, &sys);
                self.repaints.request(slot.key, Instant::now());
                self.screens_stale = true;
            }
            WindowEvent::Moved { x, y } if caps.window_position => {
                let scale = surface.window.scale_factor();
                let at = [
                    (f64::from(x) / scale).round(),
                    (f64::from(y) / scale).round(),
                ];
                if slot.told_pos != Some(at) {
                    slot.told_pos = Some(at);
                    if let Some(win) = slot.window {
                        sys.moved(&engine, win, at);
                    }
                }
                self.screens_stale = true;
            }
            WindowEvent::Focus(focused) => {
                note_state(slot, &engine, &sys);
                self.screens_stale = true;
                if !focused {
                    // The window that gets the focus hears of it next:
                    // this one is told now, with what was typed into it
                    // until here, so that no two windows are the active
                    // one in between. The picture follows.
                    let key = slot.key;
                    self.pass(host, i, false);
                    self.repaints.request(key, Instant::now());
                }
            }
            WindowEvent::Occluded(hidden) => {
                slot.occluded = hidden;
                surface.info.occluded = Some(hidden);
                note_state(slot, &engine, &sys);
                self.repaints.request(slot.key, Instant::now());
            }
            _ => {}
        }
    }

    /// The user asked for window `i` to be closed.
    fn close_requested(&mut self, i: usize, engine: &Engine, sys: &WindowSystem) {
        let slot = &self.slots[i];
        if slot.blocked {
            return;
        }
        match slot.window {
            // The window says whether it closes (`closing`), and hides
            // itself when it does.
            Some(win) => {
                sys.request_close(engine, win);
            }
            None => {
                self.root_closed = true;
                self.dirty.set(true);
            }
        }
    }

    fn accesskit_event(&mut self, event: accesskit_winit::Event) {
        let Some(slot) = self.slots.iter_mut().find(|s| {
            s.surface
                .as_ref()
                .is_some_and(|s| s.window.id() == event.window_id)
        }) else {
            return;
        };
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                slot.ctx.enable_accesskit();
                slot.told = false;
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                if let Some(surface) = slot.surface.as_mut() {
                    surface.state.on_accesskit_action_request(request);
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                slot.ctx.disable_accesskit();
                slot.told = false;
                return;
            }
        }
        self.repaints.request(slot.key, Instant::now());
    }

    /// A context asked for a pass.
    fn repaint_asked(&mut self, key: u64, when: Instant, pass: u64) {
        let Some(slot) = self.slots.iter().find(|s| s.key == key) else {
            return;
        };
        // A pass that ran since answered it, unless it was asked while
        // that pass ran.
        let now = slot.ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        if now == pass || now == pass + 1 {
            self.repaints.request(key, when);
        }
    }

    /// The displays, and which of them each window is on.
    fn read_screens(&mut self, host: &Control<'_>, engine: &Engine, sys: &WindowSystem) {
        let screens = host.screens();
        if !screens.is_empty() && screens != self.screens {
            let specs: Vec<ScreenSpec> = screens.iter().map(screen_spec).collect();
            sys.set_screens(engine, &specs);
            self.screens = screens;
            for slot in &mut self.slots {
                slot.screen = None;
            }
        }
        let Self {
            app,
            slots,
            screens,
            ..
        } = self;
        for slot in slots {
            let Some(on) = slot.surface.as_ref().and_then(|s| host.screen_of(s.id)) else {
                continue;
            };
            let index = screens
                .iter()
                .position(|s| s.name == on.name && s.position == on.position)
                .unwrap_or(0);
            if slot.screen != Some(index) {
                slot.screen = Some(index);
                if let Some(win) = slot.window {
                    sys.set_window_screen(engine, win, index);
                }
            }
            if let Some(hz) = on.refresh_hz
                && slot.refresh != Some(hz)
            {
                slot.refresh = Some(hz);
                // What a frame may spend on creating items follows how
                // long the display shows one.
                let view = match slot.view.as_mut() {
                    Some(view) => view,
                    None => app.view_mut(),
                };
                view.scene_mut().set_refresh_rate(hz);
            }
        }
    }

    /// What the windows asked for that is no property.
    fn serve_requests(&mut self, host: &mut Control<'_>, sys: &WindowSystem) {
        for (win, request) in sys.take_requests() {
            let Some(slot) = self.slots.iter_mut().find(|s| s.window == Some(win)) else {
                continue;
            };
            let Some(surface) = slot.surface.as_ref() else {
                continue;
            };
            match request {
                WindowRequest::Activate if surface.shown => {
                    host.change_window(surface.id, WindowChange::Activate);
                }
                WindowRequest::Activate => slot.activate = true,
                WindowRequest::Alert => host.change_window(surface.id, WindowChange::Alert),
                // No desktop host restacks a window.
                WindowRequest::Raise | WindowRequest::Lower => {}
            }
        }
    }

    fn application_state(&self) -> ApplicationState {
        let shown = self
            .slots
            .iter()
            .any(|s| s.surface.as_ref().is_some_and(|s| s.shown));
        match self.app_state {
            _ if !shown => ApplicationState::Hidden,
            AppState::Active => ApplicationState::Active,
            AppState::Inactive => ApplicationState::Inactive,
            AppState::Hidden => ApplicationState::Hidden,
            AppState::Suspended => ApplicationState::Suspended,
        }
    }

    /// The application ends when it says so (`Qt.quit()`), and when its
    /// last window was closed. A window that is only hidden ends nothing.
    fn should_exit(&mut self, engine: &Engine, sys: &WindowSystem) -> bool {
        if engine.quit_requested() {
            return true;
        }
        let closed = sys.closed_count() + u64::from(self.root_closed);
        if closed == self.closed {
            return false;
        }
        self.closed = closed;
        let root_shown = self.slots[0].window.is_none() && !self.root_closed;
        let any_shown = sys
            .windows(engine)
            .iter()
            .any(|w| sys.spec(engine, *w).is_some_and(|s| s.visible));
        !root_shown && !any_shown
    }

    /// Asks the platform to let the windows draw whose time has come, and
    /// runs the passes of those that cannot be seen.
    fn schedule(&mut self, host: &mut Control<'_>, now: Instant) {
        for key in self.repaints.take_due(now) {
            let Some(slot) = self.slots.iter_mut().find(|s| s.key == key) else {
                continue;
            };
            match slot.surface.as_ref() {
                Some(surface) if !slot.out_of_sight() => {
                    host.request_redraw(surface.id);
                    slot.redraw_asked.get_or_insert(now);
                }
                Some(_) => slot.want_headless(now),
                // The root's scene runs without a window: it is where
                // the timers and the messages are.
                None if slot.is_root() => slot.want_headless(now),
                // Another window's scene waits until it is shown.
                None => {}
            }
        }
        let mut due = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !slot.stalled && slot.redraw_asked.is_some_and(|t| now >= t + STALL) {
                // The request stays with the platform: when the window
                // can be seen again it is drawn, and no longer stalled.
                slot.stalled = true;
                slot.want_headless(now);
            }
            if slot.headless_due.is_some_and(|t| t <= now) {
                slot.headless_due = None;
                due.push(i);
            }
        }
        for i in due {
            self.pass(host, i, false);
        }
    }

    /// When the loop has to wake by itself.
    fn next_wake(&self) -> Option<Instant> {
        let mut next = self.repaints.next();
        for slot in &self.slots {
            let stall = slot
                .redraw_asked
                .filter(|_| !slot.stalled)
                .map(|t| t + STALL);
            for t in [slot.size_check, slot.headless_due, stall] {
                next = earlier(next, t);
            }
        }
        next
    }

    fn unsettled(&self) -> bool {
        self.dirty.get() || self.app.view().scene().root() != self.last_root
    }

    /// Everything that waited was delivered: the bookkeeping, and then
    /// the loop sleeps until the time given to the host.
    fn settle(&mut self, host: &mut Control<'_>) {
        if self.exiting {
            return;
        }
        let engine = Rc::clone(self.app.engine());
        let sys = WindowSystem::of(&engine);
        for _ in 0..ROUNDS {
            if self.unsettled() {
                self.sync(host, &engine, &sys);
                if self.exiting {
                    return;
                }
            }
            if std::mem::take(&mut self.screens_stale) {
                self.read_screens(host, &engine, &sys);
            }
            self.serve_requests(host, &sys);
            let now = Instant::now();
            for slot in &mut self.slots {
                if slot.size_check.is_some_and(|t| t <= now) {
                    slot.size_check = None;
                    note_size(slot, &engine, &sys, true);
                }
            }
            sys.set_application_state(&engine, self.application_state());
            if self.should_exit(&engine, &sys) {
                self.exiting = true;
                host.exit();
                return;
            }
            self.schedule(host, now);
            if !self.unsettled() {
                break;
            }
        }
        let mut next = self.next_wake();
        if self.unsettled() {
            // Applying what the windows want keeps changing what they
            // want. Not in a spin: at the pace of the display.
            next = earlier(next, Some(Instant::now() + HEARTBEAT));
        }
        host.wake_at(next);
    }

    /// The loop ends. What draws goes while the display is still there.
    fn shut_down(&mut self) {
        self.exiting = true;
        for slot in &mut self.slots {
            slot.surface = None;
            slot.painter = None;
        }
        self.gpu = None;
    }
}

impl<M: Send + 'static> HostApp<WinitHost<ShellEvent>> for Desktop<M> {
    fn event(&mut self, host: &mut Control<'_>, event: Event<'_, RawEvent, ShellEvent>) {
        match event {
            Event::SurfaceCreated => {
                self.live = true;
                self.dirty.set(true);
                self.screens_stale = true;
            }
            Event::SurfaceDestroyed => {
                self.live = false;
                for i in 0..self.slots.len() {
                    self.hide(host, i);
                }
            }
            Event::Window { id, event, raw } => self.window_event(host, id, &event, raw),
            Event::AppState(state) => self.app_state = state,
            Event::User(ShellEvent::Repaint { slot, when, pass }) => {
                self.repaint_asked(slot, when, pass);
            }
            Event::User(ShellEvent::AccessKit(event)) => self.accesskit_event(event),
            Event::AboutToWait => self.settle(host),
            Event::Exiting => self.shut_down(),
            Event::MemoryPressure => self.trim(),
            // Each is followed by `AboutToWait`, which looks at everything.
            Event::Wake | Event::User(ShellEvent::Wake) | Event::OpenUrl(_) | Event::SaveState => {}
        }
    }
}

/// [`crate::run`].
pub(crate) fn run<M: Send + 'static>(
    config: ShellConfig,
    setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
) -> Result<(), ShellError> {
    let host = WinitHost::<ShellEvent>::new()?;
    // Before `setup`: bindings may measure text while the root is being
    // built.
    if config.system_fonts {
        ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
    }
    let proxy: Proxy = Arc::new(Mutex::new(host.proxy()));
    let ctx = new_context(0, &proxy);
    // True from the start: the first turn of the loop opens the windows.
    let dirty = Rc::new(Cell::new(true));
    let hosting = Hosting {
        tree: tree_id(0),
        changed: {
            let dirty = Rc::clone(&dirty);
            Box::new(move || dirty.set(true))
        },
    };
    let app = ShellApp::hosted(&ctx, &config, Arc::new(SystemClock::new()), setup, hosting)?;
    let failure = Rc::new(RefCell::new(None));
    let desktop = Desktop::new(app, config, ctx, proxy, dirty, Rc::clone(&failure));
    host.run(desktop)?;
    match failure.borrow_mut().take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_has_an_accessibility_tree_of_its_own_and_none_has_eguis() {
        assert_ne!(tree_id(0), TreeId::ROOT);
        assert_ne!(tree_id(0), tree_id(1));
        assert_ne!(tree_id(1), tree_id(2));
    }

    #[test]
    fn the_loop_wakes_for_the_earlier_of_two_times() {
        let t = Instant::now();
        let later = t + Duration::from_millis(5);
        assert_eq!(earlier(None, None), None);
        assert_eq!(earlier(Some(t), None), Some(t));
        assert_eq!(earlier(None, Some(later)), Some(later));
        assert_eq!(earlier(Some(later), Some(t)), Some(t));
    }

    #[test]
    fn a_modal_window_keeps_presses_out_and_lets_releases_through() {
        use ljos_host::{PointerButton, PointerEvent};
        let pointer = |kind| WindowEvent::Pointer(PointerEvent { kind, pos: None });
        let button = |state| pointer(PointerKind::Button(PointerButton::Primary, state));
        assert!(starts_input(&button(ButtonState::Pressed)));
        assert!(starts_input(&pointer(PointerKind::Moved)));
        assert!(!starts_input(&button(ButtonState::Released)));
        assert!(!starts_input(&pointer(PointerKind::Left)));
        assert!(!starts_input(&WindowEvent::Focus(true)));
        assert!(!starts_input(&WindowEvent::CloseRequested));
    }

    #[test]
    fn a_display_is_told_in_logical_pixels() {
        let spec = screen_spec(&Screen {
            name: "DP-1".to_owned(),
            size: [3840, 2160],
            position: [1920, 0],
            scale: 2.0,
            refresh_hz: Some(60.0),
        });
        assert_eq!(spec.size, [1920.0, 1080.0]);
        assert_eq!(spec.position, [960.0, 0.0]);
        assert_eq!(spec.scale, 2.0);
    }

    #[test]
    fn a_window_out_of_sight_is_not_run_more_often_than_its_kind_allows() {
        let t = Instant::now();
        let mut root = Slot::new(0, None, egui::Context::default(), None);
        root.want_headless(t);
        assert_eq!(root.headless_due, Some(t), "nothing ran before: now");
        root.headless_at = Some(t);
        root.headless_due = None;
        root.want_headless(t + Duration::from_millis(1));
        assert_eq!(root.headless_due, Some(t + HEARTBEAT));
        // A later request does not push an earlier one back.
        root.want_headless(t + Duration::from_millis(500));
        assert_eq!(root.headless_due, Some(t + HEARTBEAT));
    }
}
