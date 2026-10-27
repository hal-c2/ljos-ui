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
use egui_winit::accesskit_winit;
use egui_winit::winit::event::WindowEvent as RawEvent;
use egui_winit::winit::event_loop::EventLoopProxy;
use ljos_controls::{
    ApplicationState, Modality, ResizeEdges, ScreenSpec, WindowRequest, WindowSpec, WindowSystem,
};
use ljos_egui::{QmlView, ResizeMode, WindowDrag};
use ljos_engine::{Engine, LogLevel, ObjectId};
use ljos_host::{
    AppState, ButtonState, Event, HostApp, HostControl, PlatformHost, PointerKind, ResizeEdge,
    Screen, Wake, WindowChange, WindowEvent, WindowId, WindowState, WinitControl, WinitHost,
};
use ljos_runtime::{MemoryPressure, SystemClock};

use crate::present::{self, Frame, LastPicture};
use crate::repaint::RepaintSchedule;
use crate::surface::{self, Gpu, Painter, Starting, Surface};
use crate::{
    Hosting, Setup, SetupError, ShellApp, ShellConfig, ShellError, drags, wayland_drag, windows,
};

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
/// How long an agent that asked for a window size (`app_resize`) waits
/// for the platform to give it, before it is answered with the size there
/// is. Longer than [`SIZE_CHECK`]: XWayland takes more than that.
#[cfg(feature = "mcp")]
const RESIZE_WAIT: Duration = Duration::from_secs(1);

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

/// The texture limit egui is told, which is also the width of its text
/// atlas: 256 KiB of atlas instead of 2 MiB at llvmpipe's 16384.
const EGUI_TEXTURE_SIDE: usize = 2048;

/// How long after start-up, or after a burst of background picture loads,
/// the memory they freed is given back ([`crate::give_back_freed_heap`]):
/// long enough that what follows at once (the first frames, the next
/// picture) reuses it instead.
const HEAP_TRIM_AFTER: Duration = Duration::from_secs(1);

/// How many objects must have gone since the heap was last given back for
/// it to be given back again: a page replaced or a loader emptied, not a
/// delegate scrolled out.
const OBJECTS_GONE_BEFORE_TRIM: usize = 1024;

/// How long the loop must have nothing to do before what the scripts left
/// behind is collected: longer than a frame, so that an animation is not
/// held up by it.
const REST_BEFORE_COLLECTING: Duration = Duration::from_millis(100);

/// What other threads, egui and the accessibility adapter post into the
/// loop.
pub(crate) enum ShellEvent {
    /// The host's waker fired; there is nothing to do but look.
    Wake,
    /// A context asked for a pass (`egui::Context::request_repaint`).
    Repaint {
        slot: u64,
        when: Instant,
        /// The context's pass count when it asked; `None` for a waker,
        /// which is never answered before it is heard.
        pass: Option<u64>,
        /// Asked for at once, not after a delay.
        at_once: bool,
    },
    AccessKit(accesskit_winit::Event),
    /// A step of a drag from another application, which the shell follows
    /// itself where winit does not (`drags`).
    Drag(drags::DragStep),
}

impl std::fmt::Debug for ShellEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellEvent::Wake => f.write_str("Wake"),
            ShellEvent::Repaint { slot, .. } => write!(f, "Repaint({slot})"),
            ShellEvent::AccessKit(_) => f.write_str("AccessKit"),
            ShellEvent::Drag(_) => f.write_str("Drag"),
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

thread_local! {
    /// The window whose pass runs on this thread, while egui runs it.
    static IN_PASS: Cell<Option<u64>> = const { Cell::new(None) };
}

/// A context for one window. Asking it for a repaint posts into the loop,
/// which is what wakes it. While its own pass runs, what it is asked is
/// not heard: egui asks for two more passes after each one with input,
/// and one more after each that asked for one at once, for widgets of its
/// own that answer a frame late. The scene answers in the pass, and says
/// what it wants next ([`QmlView::take_asked`]); everything else wakes the
/// loop with [`waker`].
fn new_context(key: u64, proxy: &Proxy) -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(ljos_egui::font_definitions());
    // A second pass when a widget asks to be laid out again, as egui's
    // own hosts allow.
    ctx.options_mut(|o| o.max_passes = NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
    let proxy = Arc::clone(proxy);
    ctx.set_request_repaint_callback(move |info| {
        if IN_PASS.get() == Some(key) {
            return;
        }
        // A delay too long to say is "not before something else happens".
        let Some(when) = Instant::now().checked_add(info.delay) else {
            return;
        };
        if let Ok(proxy) = proxy.lock() {
            // Fails only when the loop has ended.
            drop(proxy.send_event(ShellEvent::Repaint {
                slot: key,
                when,
                pass: Some(info.current_cumulative_pass_nr),
                at_once: info.delay.is_zero(),
            }));
        }
    });
    ctx
}

/// Wakes the loop for window `key` from any thread, without egui: once
/// egui has asked for a pass it hears no other request until the next
/// one, and the loop leaves out the passes egui asks for itself.
fn waker(key: u64, proxy: &Proxy) -> ljos_egui::Waker {
    let proxy = Arc::clone(proxy);
    Arc::new(move |delay| {
        let Some(when) = Instant::now().checked_add(delay) else {
            return;
        };
        if let Ok(proxy) = proxy.lock() {
            // Fails only when the loop has ended.
            drop(proxy.send_event(ShellEvent::Repaint {
                slot: key,
                when,
                pass: None,
                at_once: delay.is_zero(),
            }));
        }
    })
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

/// The host's icon of the same pixels, which it shares.
fn host_icon(icon: &ljos_platform::Icon) -> ljos_host::Icon {
    ljos_host::Icon {
        width: icon.width(),
        height: icon.height(),
        rgba: Arc::clone(icon.rgba()),
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
    tree_told: bool,
    /// The modifiers the window's keyboard holds, as the platform last
    /// said; egui-winit keeps its own to itself.
    modifiers: ljos_host::Modifiers,
    /// egui's tree as the adapter was last sent it. The same again
    /// changes nothing a reader hears, and still woke the adapter's
    /// thread on every pass.
    sent_root: Option<egui::accesskit::TreeUpdate>,
    /// Pictures the painter has not been given yet.
    pending: egui::TexturesDelta,
    /// What the window asked for through egui (cut, copy, paste).
    actions: Vec<egui_winit::ActionRequested>,
    /// When the platform was asked to let the window draw, and has not.
    redraw_asked: Option<Instant>,
    /// The platform let the window draw after the last frame it showed:
    /// the display has taken that frame, and the next can be drawn at
    /// once. Asking again would cost a turn of the loop to hear yes.
    redraw_granted: bool,
    /// The platform did not let it draw in time: it is out of sight.
    stalled: bool,
    /// The platform said that nothing of the window can be seen.
    occluded: bool,
    /// When a pass that is not drawn last ran, and when the next is due.
    undrawn_pass_at: Option<Instant>,
    undrawn_pass_due: Option<Instant>,
    /// When to see whether the platform gave the size that was asked for.
    size_check: Option<Instant>,
    /// What the QML window was last told of its platform window.
    told_size: Option<[f64; 2]>,
    told_pos: Option<[f64; 2]>,
    told_state: Option<WindowState>,
    /// The platform's count of icon changes when the window was given
    /// its icon.
    told_icons: u64,
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
    /// What the window shows, so that a pass that changes none of it is
    /// not drawn.
    shown: LastPicture,
    /// Passes in a row since the window was last drawn.
    undrawn: u32,
    /// Passes that were not drawn asked for the next at once: it waits
    /// until here, a frame later, as a drawn one waits for the display;
    /// or until the scene's next timer.
    held: Option<Instant>,
    /// The frame part of `held`: what a pass asked for at once waits for.
    paced: Option<Instant>,
    /// When the last drawn pass held the next until: the display's
    /// rhythm, which the next keeps if it began on time.
    beat: Option<Instant>,
    /// When the earliest repaint the next pass answers was due, for the
    /// inspector.
    #[cfg(feature = "mcp")]
    asked: Option<Instant>,
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
            tree_told: false,
            modifiers: ljos_host::Modifiers::default(),
            sent_root: None,
            pending: egui::TexturesDelta::default(),
            actions: Vec::new(),
            redraw_asked: None,
            redraw_granted: false,
            stalled: false,
            occluded: false,
            undrawn_pass_at: None,
            undrawn_pass_due: None,
            size_check: None,
            told_size: None,
            told_pos: None,
            told_state: None,
            told_icons: 0,
            blocked: false,
            broken: false,
            activate: false,
            screen: None,
            refresh: None,
            started: Instant::now(),
            clear: egui::Color32::WHITE,
            shown: LastPicture::default(),
            undrawn: 0,
            held: None,
            paced: None,
            beat: None,
            #[cfg(feature = "mcp")]
            asked: None,
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
    fn want_undrawn_pass(&mut self, now: Instant) {
        let gap = if self.is_root() {
            HEARTBEAT
        } else {
            BACKGROUND
        };
        let at = self
            .undrawn_pass_at
            .map_or(now, |last| (last + gap).max(now));
        self.undrawn_pass_due = Some(self.undrawn_pass_due.map_or(at, |due| due.min(at)));
    }

    /// The platform's window is gone, or another one: nothing that was
    /// known of it holds.
    fn forget_platform(&mut self) {
        self.tree_told = false;
        self.sent_root = None;
        self.redraw_asked = None;
        self.redraw_granted = false;
        self.stalled = false;
        self.occluded = false;
        self.undrawn_pass_due = None;
        self.size_check = None;
        self.told_size = None;
        self.told_pos = None;
        self.told_state = None;
        self.activate = false;
        self.screen = None;
        self.shown.forget();
        self.undrawn = 0;
        self.held = None;
        self.paced = None;
        self.beat = None;
    }

    /// A pass changed nothing to draw; the scene wants the next in
    /// `wake`. Drawn passes are paced by the display, which this one did
    /// not wait for: the next waits for the scene
    /// (`QmlView::wakes_in`). One that comes at once is let through (what
    /// the pass changed is laid out and shown by the next), and from the
    /// second in a row they wait a frame, so that a scene
    /// whose animations show nothing does not spin. A scene that asks for
    /// a later pass is not held past it: it asks for its next frame a
    /// frame after its last, so it does not spin, and a wheel turned while
    /// a frame was presented runs two passes that show nothing yet (the
    /// glide starts where the content is), after which a whole frame's
    /// wait missed the display's next, and the glide was shown at half
    /// its rate.
    fn not_drawn(&mut self, now: Instant, wake: Option<Duration>) {
        self.undrawn += 1;
        let least = if self.undrawn > 1 {
            self.frame_interval()
        } else {
            Duration::ZERO
        };
        let wait = match wake {
            Some(w) if w > Duration::ZERO => w,
            _ => least,
        };
        self.held = (wait > Duration::ZERO).then(|| now + wait);
        self.paced = (least > Duration::ZERO).then(|| now + least);
    }

    /// Something asked for a pass at once: another thread's result, the
    /// inspector, a change. It waits for the frame, or for the scene if
    /// that comes sooner, and not for the scene's timer, which is what it
    /// would wait for until it fires.
    fn asked_at_once(&mut self) {
        self.held = match (self.held, self.paced) {
            (Some(held), Some(paced)) => Some(held.min(paced)),
            (_, paced) => paced,
        };
    }

    /// A pass that began at `began` was drawn. The display holds a drawn
    /// frame until it is shown, which paces the passes, except where
    /// presenting returns at once: a software rasteriser on an X server
    /// without DRI3 does, and an animation there was drawn three times a
    /// frame. The next pass waits until a frame after this one began. A
    /// display that holds the frame has waited for most of that already,
    /// and the passes keep its phase: counted from a frame that began
    /// before, each pass started as long after the display's refresh as
    /// that one had, and those that then missed the next refresh were
    /// shown two apart. A pass that began a little after it was let go
    /// (the loop wakes late, the platform hands the redraw over late) does
    /// not push the next back by as much: counted from each one's start,
    /// the passes came a frame and a millisecond apart on a display that
    /// draws every frame, and one in fifty missed its refresh.
    fn drawn(&mut self, began: Instant, now: Instant) {
        self.undrawn = 0;
        self.redraw_granted = false;
        let interval = self.frame_hold();
        let from = match self.beat {
            Some(beat) if began >= beat && began < beat + interval / 2 => beat,
            _ => began,
        };
        let due = from + interval;
        self.beat = Some(due);
        self.held = (due > now).then_some(due);
        self.paced = self.held;
    }

    /// How long a drawn pass holds the next. A display that does not say
    /// how often it draws may draw more often than the 60 Hz assumed for
    /// it: the sandbox's headless one says 0 and draws at about 62, and a
    /// pass held a 60 Hz frame missed one refresh in thirty. Held three
    /// quarters of that, the platform's own pacing (the frame callback, a
    /// swap that waits) decides, and a presenter that returns at once
    /// still does not draw a frame more than four times in three.
    fn frame_hold(&self) -> Duration {
        match self.refresh.filter(|hz| *hz > 0.0) {
            Some(hz) => Duration::from_secs_f64(1.0 / hz),
            None => Duration::from_secs_f64(0.75 / 60.0),
        }
    }

    /// How long a frame is shown on the window's display.
    fn frame_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.refresh.filter(|hz| *hz > 0.0).unwrap_or(60.0))
    }
}

/// Shows a window once there is a picture in it.
fn show_when_drawn(
    host: &mut Control<'_>,
    surface: &mut Surface,
    activate: &mut bool,
    screens_stale: &mut bool,
) {
    if !surface.shown {
        surface.shown = true;
        host.change_window(surface.id, WindowChange::Visible(true));
        if std::mem::take(activate) {
            host.change_window(surface.id, WindowChange::Activate);
        }
        // Only now does the platform say which display it is on.
        *screens_stale = true;
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
    let state = surface.shown_as(slot.told_state);
    if slot.told_state != Some(state) {
        slot.told_state = Some(state);
        if let Some(win) = slot.window {
            sys.shown_as(engine, win, windows::show(state));
        }
    }
}

/// A key, a button, a touch or a turn of the wheel: what a user does, and
/// not where the pointer happens to be.
#[cfg(feature = "mcp")]
fn is_an_act(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Pointer(pointer) => matches!(
            pointer.kind,
            PointerKind::Wheel(_) | PointerKind::Button(_, ButtonState::Pressed)
        ),
        event => starts_input(event),
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

/// Whether a press asks for a paste, by the chords of Qt's
/// `QKeySequence::Paste` that egui-winit knows too: Command+V (Control+V
/// off macOS), with Shift or without, Shift+Insert, and the Paste key. The
/// V is the key that types it, or the V's place on a keyboard that types
/// no Latin letter there.
fn asks_paste(key: &ljos_host::KeyEvent, held: ljos_host::Modifiers, mac: bool) -> bool {
    if key.state != ButtonState::Pressed {
        return false;
    }
    let command = if mac { held.meta } else { held.control };
    let other = if mac { held.control } else { held.meta };
    match &key.key {
        ljos_host::Key::Named(name) => {
            name == "Paste"
                || (!mac
                    && name == "Insert"
                    && held.shift
                    && !held.control
                    && !held.alt
                    && !held.meta)
        }
        ljos_host::Key::Character(c) if command && !held.alt && !other => {
            let latin = c.chars().all(|c| c.is_ascii_alphabetic());
            c.eq_ignore_ascii_case("v") || (!latin && key.code.as_deref() == Some("KeyV"))
        }
        _ => false,
    }
}

/// Keeps the modifiers a window's keyboard holds. One that lost the
/// keyboard holds none: the release goes to the window that has it.
fn follow_modifiers(held: &mut ljos_host::Modifiers, event: &WindowEvent) {
    match event {
        WindowEvent::Modifiers(m) => *held = *m,
        WindowEvent::Focus(false) => *held = ljos_host::Modifiers::default(),
        _ => {}
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

/// What a pass of the primary window costs, as the inspector is told.
#[cfg(feature = "mcp")]
struct Meter {
    started: Instant,
    built: Option<Instant>,
    drawn: ljos_dev::mcp::Drawn,
    uploads: usize,
    upload_bytes: usize,
}

#[cfg(feature = "mcp")]
impl Meter {
    fn new(frame: &Frame, pending: &egui::TexturesDelta) -> Self {
        use ljos_dev::mcp::Drawn;
        Self {
            started: Instant::now(),
            built: None,
            drawn: match frame {
                Frame::Nothing => Drawn::Unchanged,
                Frame::Part(_) => Drawn::Part,
                Frame::Whole => Drawn::Whole,
            },
            uploads: pending.set.values().map(|d| d.len()).sum(),
            upload_bytes: pending
                .set
                .values()
                .flat_map(|d| d.iter())
                .map(|d| d.image.width() * d.image.height() * d.image.bytes_per_pixel())
                .sum(),
        }
    }

    /// What is drawn is ready; uploading, drawing and presenting follow.
    fn built(&mut self) {
        self.built = Some(Instant::now());
    }

    /// The pass ended; `asked` is when the repaint it answered was due,
    /// `steps` what the painter timed of it.
    fn report(self, asked: Option<Instant>, steps: present::Steps) -> ljos_dev::mcp::PaintReport {
        let end = Instant::now();
        let built = self.built.unwrap_or(self.started);
        ljos_dev::mcp::PaintReport {
            drawn: self.drawn,
            build: built - self.started,
            paint: end - built,
            uploads: self.uploads,
            upload_bytes: self.upload_bytes,
            since_asked: asked.map(|at| end.saturating_duration_since(at)),
            upload: steps.upload,
            draw: steps.draw,
            present: steps.present,
            moved: steps.moved,
        }
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
    /// The platform's windows are to be made what the application's want
    /// (`sync`): a QML window wants something else than its platform
    /// window has, or a window came or went.
    needs_sync: Rc<Cell<bool>>,
    /// The device of the first window that opened.
    gpu: Option<Gpu>,
    /// The graphics instances made while the application starts, until
    /// the first window takes one.
    starting: Option<Starting>,
    /// What draws the windows: GL, unless `LJOS_PAINTER` says wgpu.
    painter_choice: surface::Choice,
    /// The shell chose the processor ([`surface::Chosen::automatic`]).
    painter_automatic: bool,
    /// A window drawn on the processor shows what only GL draws: every
    /// window goes to GL once the pass ends.
    to_gl: bool,
    /// Why the loop ended, when it was not asked to.
    failure: Rc<RefCell<Option<ShellError>>>,
    screens: Vec<Screen>,
    screens_stale: bool,
    /// The platform's count of icon changes when the icons were last
    /// shown ([`ljos_platform::Platform::icons_changed`]).
    icons_shown: u64,
    /// How many windows had closed when it was last asked whether the
    /// application ends.
    closes_seen: u64,
    /// The user closed the window of a root that is a plain item.
    root_closed: bool,
    next_key: u64,
    last_root: Option<ObjectId>,
    /// The size the first root that is a plain item said for itself, with
    /// [`ShellConfig::root_sized`]; read once, so that a reload does not
    /// undo what the user made of the window.
    root_size: Option<[f32; 2]>,
    /// Windows can be drawn into.
    live: bool,
    app_state: AppState,
    /// Windows that cannot be shown; each was said once.
    refused: Vec<ObjectId>,
    warned_hidden_root: bool,
    exiting: bool,
    /// A window was drawn.
    drawn_once: bool,
    /// When the shell started: the inspector times the start-up from here.
    #[cfg(feature = "mcp")]
    began: Instant,
    /// When an agent that asked for a size of the primary window is
    /// answered with the size there is.
    #[cfg(feature = "mcp")]
    resize_due: Option<Instant>,
    /// What woke the loop in this turn and what it led to, gathered only
    /// while a client keeps the wakes ([`ShellApp::records_wakes`]).
    #[cfg(feature = "mcp")]
    wake: Option<ljos_dev::mcp::HostWake>,
    /// The scripts the windows may run on input were prepared
    /// ([`Engine::prepare_scripts`]).
    scripts_prepared: bool,
    /// When the heap is next given back to the system, once start-up, a
    /// burst of picture loads or of objects going is over.
    heap_trim: Option<Instant>,
    /// The most objects there were since the heap was last given back.
    objects_high: HighWater,
    /// Pictures were loading in the background at the last settle.
    loading: bool,
    /// When what the scripts left behind is next looked at.
    collect: AfterRest,
    /// What follows drags from other applications, where the shell does
    /// that itself. Shared with `drag_out`, which the Engine calls.
    drags: Rc<RefCell<Option<drags::Drags>>>,
    /// What carries the application's drags out of its windows, where
    /// the shell does.
    drag_out: Option<Rc<drags::DragOut>>,
    /// The window a drag from another application is over, by its key.
    drag_over: Option<u64>,
}

/// Something to do once the loop has rested for a while after it last
/// settled, and once a rest, when there is something to do. The wheel
/// turning settles the loop at every notch, with a rest between each and
/// the next that the loop cannot foresee: something done at the start of
/// every rest is done at every notch.
#[derive(Debug, Default)]
struct AfterRest {
    due: Option<Instant>,
}

impl AfterRest {
    /// The loop settled at `now`, with more to do before `rest` is over
    /// when `busy`, and something to do after it when `wanted`. True when
    /// the rest it waited for is over.
    ///
    /// A wake that is not busy leaves the rest as it was: what woke the
    /// loop (a question from an agent, a timer the system ran a little
    /// early) was not work, and starting the rest over at each would wake
    /// the loop a rest later, then again, for as long as it is asked.
    fn settled(&mut self, busy: bool, wanted: bool, now: Instant, rest: Duration) -> bool {
        match self.due {
            _ if busy || !wanted => {
                self.due = None;
                false
            }
            Some(t) if t <= now => {
                self.due = None;
                true
            }
            Some(_) => false,
            None => {
                self.due = Some(now + rest);
                false
            }
        }
    }

    fn next(&self) -> Option<Instant> {
        self.due
    }
}

/// The most of something there was since it was last reset: enough of
/// it gone says memory it held is free.
#[derive(Debug, Default)]
struct HighWater(usize);

impl HighWater {
    /// There are `now`. True when at least `gone` of the most there were
    /// went, which the next reset forgets.
    fn fell(&mut self, now: usize, gone: usize) -> bool {
        self.0 = self.0.max(now);
        self.0 - now >= gone
    }

    fn reset(&mut self, now: usize) {
        self.0 = now;
    }
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
        needs_sync: Rc<Cell<bool>>,
        failure: Rc<RefCell<Option<ShellError>>>,
        starting: Starting,
    ) -> Self {
        #[cfg(feature = "mcp")]
        let app = {
            let mut app = app;
            app.set_resizes_window();
            app.set_measures_render();
            app
        };
        Self {
            app,
            config,
            slots: vec![Slot::new(0, None, ctx, None)],
            repaints: RepaintSchedule::default(),
            proxy,
            needs_sync,
            gpu: None,
            painter_choice: starting.chosen().choice,
            painter_automatic: starting.chosen().automatic,
            to_gl: false,
            starting: Some(starting),
            failure,
            screens: Vec::new(),
            screens_stale: true,
            icons_shown: 0,
            closes_seen: 0,
            root_closed: false,
            next_key: 1,
            last_root: None,
            root_size: None,
            live: false,
            app_state: AppState::Inactive,
            refused: Vec::new(),
            warned_hidden_root: false,
            exiting: false,
            drawn_once: false,
            // `run` says when the shell started.
            #[cfg(feature = "mcp")]
            began: Instant::now(),
            #[cfg(feature = "mcp")]
            resize_due: None,
            #[cfg(feature = "mcp")]
            wake: None,
            scripts_prepared: false,
            heap_trim: None,
            objects_high: HighWater::default(),
            collect: AfterRest::default(),
            loading: false,
            drags: Rc::default(),
            drag_out: None,
            drag_over: None,
        }
    }

    fn slot_of(&self, id: WindowId) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.surface.as_ref().is_some_and(|s| s.id == id))
    }

    /// The window the shell gives a root that is a plain item.
    fn root_spec(&self) -> WindowSpec {
        let [w, h] = self.root_size.unwrap_or(self.config.size);
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
            backdrop: ljos_controls::WindowBackdrop::None,
            color_scheme: ljos_a11y::ColorScheme::Unknown,
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
        self.needs_sync.set(false);
        // Before the windows are matched: the root the application put in
        // the window of the old one takes that platform window, where a
        // `Window` made and shown on its own would get one of its own.
        self.app.swap_roots();
        let root = self.app.view().scene().root();
        if root != self.last_root {
            // A scene's timers start in its first pass, and nothing else
            // asks for that one when the root has no window to open.
            self.repaints.request(self.slots[0].key, Instant::now());
        }
        self.last_root = root;
        let root_window = root.filter(|r| sys.spec(engine, *r).is_some());
        if self.config.root_sized
            && self.root_size.is_none()
            && let Some(root) = root.filter(|_| root_window.is_none())
        {
            // Before the view makes the root as large as the window, and
            // laid out, as a Text knows its implicit size only then: the
            // first frame is at the root's size, not the default.
            self.app.view_mut().scene_mut().lay_out(engine);
            let side = |name, or| {
                let n = engine.read_by_name(root, name).to_number();
                #[allow(clippy::cast_possible_truncation)]
                if n >= 1.0 { n.ceil() as f32 } else { or }
            };
            let [w, h] = self.config.size;
            self.root_size = Some([side("width", w), side("height", h)]);
        }
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
            // The window is cleared to an opaque background (`apply`):
            // painting it again is one more pass over every pixel, which
            // a software rasteriser pays for in every frame.
            let opaque = self.config.background.to_rgba8()[3] == 255;
            let backdrop = root_window.is_none() && !opaque;
            self.app.set_backdrop(backdrop);
            // The window is cleared to what the view leaves for it, unless
            // a backdrop is painted between the two.
            self.app.view_mut().set_fold_backdrop(!backdrop);
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
                view.set_waker(waker(key, &self.proxy));
                view.bind_context(&ctx);
                // The window is cleared to what the view leaves for it.
                view.set_fold_backdrop(true);
                // The waker is the one a decoded picture wakes, and that
                // too stays the root's.
                let root = self.slots[0].key;
                self.app.view_mut().set_waker(waker(root, &self.proxy));
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
        // Every window of the application goes by its desktop entry, which
        // is how the desktop finds its icon and its badge.
        let platform = ljos_platform::Platform::of(engine);
        attrs.app_id = Some(platform.desktop_file_name());
        let icons = platform.icons_changed();
        attrs.icon = platform
            .window_icon(self.slots[i].window)
            .map(|icon| host_icon(&icon));
        let id = match host.create_window(&attrs) {
            Ok(id) => id,
            Err(e) => return self.fail(host, i, ShellError::Window(e), engine),
        };
        let Some(window) = host.window(id).cloned() else {
            return;
        };
        if let Some(drags) = self.drags.borrow().as_ref()
            && let Err(why) = drags.window_opened(&window)
        {
            engine.log(LogLevel::Warning, CATEGORY, why);
        }
        let display = host.event_loop().owned_display_handle();
        if self.gpu.is_none() {
            // The drivers are still starting: the first frame's layout
            // and text need no device, and are done while they do.
            if self.starting.is_some() {
                let ppp = window.scale_factor() as f32 * self.slots[i].ctx.zoom_factor();
                let px = window.inner_size();
                let size = egui::vec2(px.width as f32, px.height as f32) / ppp;
                match self.slots[i].view.as_mut() {
                    Some(view) => view.lay_out_ahead(size, ppp),
                    None => self.app.view_mut().lay_out_ahead(size, ppp),
                }
            }
            // Without a device from them, the painter makes its own and
            // says why there is none.
            self.gpu = self
                .starting
                .take()
                .and_then(|starting| starting.device_for(&window));
        }
        let slot = &mut self.slots[i];
        let attached = surface::attach(
            &mut slot.painter,
            &slot.ctx,
            &mut self.gpu,
            display,
            &window,
            self.painter_choice,
        );
        let painter = match (attached, slot.painter.as_ref()) {
            (Ok(why), Some(painter)) => {
                if let Some(why) = why {
                    engine.log(
                        LogLevel::Warning,
                        CATEGORY,
                        format!("drawing with wgpu: GL could not be had ({why})"),
                    );
                }
                painter
            }
            (Err(e), _) => {
                drop(window);
                host.destroy_window(id);
                return self.fail(host, i, ShellError::Gpu(e), engine);
            }
            (Ok(_), None) => return,
        };
        #[cfg(feature = "mcp")]
        if slot.is_root() {
            let info = painter.describe();
            self.app
                .record_startup(ljos_dev::mcp::StartupStep::Device, self.began.elapsed());
            if let Some(info) = info {
                self.app.set_painter(info);
            }
        }
        // Ljós's textures follow the GPU's limit; egui's follows a small
        // one, since egui makes its text atlas, which Ljós barely uses, as
        // wide as its limit.
        let side = painter.max_texture_side();
        if let Some(side) = side {
            ljos_render::b0::set_max_texture_side(&slot.ctx, side);
        }
        let mut state = egui_winit::State::new(
            slot.ctx.clone(),
            ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            host.event_loop().system_theme(),
            side.map(|s| s.min(EGUI_TEXTURE_SIDE)),
        );
        // egui-winit's own clipboard chords read its clipboard, which has
        // no system behind it here, and drop the press when it is empty
        // (#130); they swallow the Shift of Ctrl+Shift+V always. Off, every
        // chord reaches the scene as the key it is, and the scene's editors
        // copy, cut and paste through the platform's clipboard.
        state.set_clipboard_shortcuts(false);
        // While the window is unseen: the adapter has to be there before
        // the platform asks about the window.
        state.init_accesskit(host.event_loop(), &window, host.proxy().clone());
        // Before the first picture, so that what the application does to the
        // platform's window is there when it is first seen.
        ljos_platform::Platform::of(engine).native_window_opened(engine, slot.window, &*window);
        slot.forget_platform();
        slot.surface = Some(Surface::new(id, window, state));
        slot.told_state = Some(attrs.state);
        slot.told_icons = icons;
        note_size(slot, engine, sys, false);
        self.repaints.request(slot.key, Instant::now());
        self.screens_stale = true;
        self.tell_view(i);
    }

    /// Tells the view of slot `i` what its painter draws.
    fn tell_view(&mut self, i: usize) {
        let painter = self.slots[i].painter.as_ref();
        let fills = painter.is_some_and(Painter::fills_rects);
        let shaders = painter.is_some_and(Painter::draws_shaders);
        let encodes = !painter.is_some_and(Painter::draws_scene);
        let view = match self.slots[i].view.as_mut() {
            Some(view) => view,
            None => self.app.view_mut(),
        };
        view.set_drawn(true);
        view.set_solid_rects(fills);
        view.set_draws_shaders(shaders);
        view.set_encodes(encodes);
    }

    /// Hands every window drawn on the processor to GL, which the shell
    /// chose as EGL would draw in software too: a window shows what only
    /// GL draws. Starting EGL here takes as long as it would have at
    /// start-up, once.
    fn leave_processor(&mut self, host: &mut Control<'_>) {
        self.painter_automatic = false;
        self.painter_choice = surface::Choice::Gl;
        let engine = Rc::clone(self.app.engine());
        engine.log(
            LogLevel::Info,
            CATEGORY,
            "drawn with GL from now on: a window shows what the processor does not draw",
        );
        for i in 0..self.slots.len() {
            let slot = &mut self.slots[i];
            if !matches!(slot.painter, Some(Painter::Cpu(_))) {
                continue;
            }
            slot.painter = None;
            // A window not open gets its painter when it opens.
            let Some(window) = slot.surface.as_ref().map(|s| Arc::clone(&s.window)) else {
                continue;
            };
            let display = host.event_loop().owned_display_handle();
            match surface::attach(
                &mut slot.painter,
                &slot.ctx,
                &mut self.gpu,
                display,
                &window,
                surface::Choice::Gl,
            ) {
                Ok(Some(why)) => engine.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!("drawing with wgpu: GL could not be had ({why})"),
                ),
                Ok(None) => {}
                Err(e) => {
                    engine.log(
                        LogLevel::Warning,
                        CATEGORY,
                        format!("a window cannot be drawn: {e}"),
                    );
                    continue;
                }
            }
            if let Some(side) = slot.painter.as_ref().and_then(Painter::max_texture_side) {
                ljos_render::b0::set_max_texture_side(&slot.ctx, side);
            }
            resend_font_atlas(&slot.ctx);
            slot.shown.forget();
            #[cfg(feature = "mcp")]
            if slot.is_root()
                && let Some(info) = slot.painter.as_ref().and_then(Painter::describe)
            {
                self.app.set_painter(info);
            }
            let key = slot.key;
            self.repaints.request(key, Instant::now());
            self.tell_view(i);
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
            painter.forget_window();
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
        crate::give_back_freed_heap();
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
        crate::give_back_freed_heap();
    }

    /// One pass of slot `i`: input, the scene, and (`paint`) the picture
    /// into its window. A pass that is not painted is for a window that
    /// cannot be seen, or that the root does not have: its scene runs all
    /// the same. Then the pictures the pass asked for start loading,
    /// drawn or not, so that none waits for a frame that never comes.
    /// A painted pass first waits a moment for the large pictures whose
    /// decode is under way, which Qt would show in that frame.
    fn pass(&mut self, host: &mut Control<'_>, i: usize, paint: bool) {
        if paint {
            let engine = Rc::clone(self.app.engine());
            let images = ljos_quick::ImageStore::of(&engine);
            if images.has_deferred() {
                // Laid out first, so that they decode at the size drawn.
                self.app.view_mut().scene_mut().lay_out(&engine);
            }
            if self.drawn_once {
                let moving = self.moving(i);
                images.wait_for_later_frame(&engine, moving);
            } else {
                images.wait_for_frame(&engine, ljos_quick::ImageStore::FIRST_FRAME_WAIT);
            }
        }
        self.run_pass(host, i, paint);
        if std::mem::take(&mut self.to_gl) {
            self.leave_processor(host);
        }
        #[cfg(feature = "mcp")]
        if self.slots[i].is_root() && self.app.waits_for_resize() {
            // The pass that took the request has not asked the platform
            // yet: that is done once it ends (`serve_resize`).
            let settled = self.resize_due.is_some_and(|t| t <= Instant::now());
            self.app.finish_resize(settled);
            if !self.app.waits_for_resize() {
                self.resize_due = None;
            }
        }
        #[cfg(feature = "mcp")]
        if self.slots[i].is_root() && self.app.waits_for_memory() {
            self.serve_memory();
        }
        let slot = &mut self.slots[i];
        let view = match slot.view.as_mut() {
            Some(view) => view,
            None => self.app.view_mut(),
        };
        if let Some(after) = view.take_asked() {
            let key = slot.key;
            self.asked(key, Instant::now() + after, after.is_zero());
        }
        let engine = self.app.engine();
        ljos_quick::ImageStore::of(engine).frame_drawn(engine);
    }

    /// Whether what slot `i` shows changes frame after frame: the pointer
    /// is held or wheeled, a finger is down, or the scene asks for the
    /// next frame within one, as a flick or an animation does.
    fn moving(&mut self, i: usize) -> bool {
        let Self { app, slots, .. } = self;
        let slot = &mut slots[i];
        if slot.ctx.input(|input| input.pointer.any_down()) {
            return true;
        }
        let input = slot.surface.as_mut().is_some_and(|surface| {
            surface.state.egui_input_mut().events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::MouseWheel { .. }
                        | egui::Event::Touch { .. }
                        | egui::Event::Zoom(_)
                        | egui::Event::Rotate(_)
                )
            })
        });
        let period =
            Duration::from_secs_f64(1.0 / slot.refresh.filter(|hz| *hz > 0.0).unwrap_or(60.0));
        let view = match slot.view.as_ref() {
            Some(view) => view,
            None => app.view(),
        };
        input || view.wakes_in().is_some_and(|wake| wake <= period)
    }

    fn run_pass(&mut self, host: &mut Control<'_>, i: usize, paint: bool) {
        #[cfg(feature = "mcp")]
        self.note_wake(|w| w.passes += 1);
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
        let began = Instant::now();
        // What was asked until now, this pass answers. egui forgets a
        // delayed repaint with every pass too, and is asked again.
        repaints.forget(slot.key);
        if paint {
            // A redraw the shell did not ask for is the platform's: what
            // the window showed may be gone (exposed, resized).
            if slot.redraw_asked.is_none() {
                slot.shown.forget();
                if let Some(Painter::Cpu(painter)) = slot.painter.as_mut() {
                    painter.forget();
                }
            }
            slot.redraw_asked = None;
            slot.stalled = false;
        } else {
            slot.undrawn_pass_at = Some(Instant::now());
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
        raw.time = Some(began.duration_since(slot.started).as_secs_f64());

        IN_PASS.set(Some(slot.key));
        let output = match slot.view.as_mut() {
            None => slot.ctx.run_ui(raw, |ui| {
                app.begin_pass();
                app.show(ui);
            }),
            Some(view) => {
                #[cfg(feature = "mcp")]
                app.time_window(view);
                let output = slot.ctx.run_ui(raw, |ui| {
                    view.show(ui);
                });
                #[cfg(feature = "mcp")]
                app.record_window_frames(slot.window, view);
                output
            }
        };
        IN_PASS.set(None);
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
            surface::free_now(slot.painter.as_mut(), &mut slot.pending);
            return;
        };
        surface.info.events.clear();

        // egui's tree first, then the scene's, which hangs in it (§14).
        if let Some(mut root) = platform_output.accesskit_update.take() {
            // A screen reader says which window it is in by the title.
            let title = slot.applied.as_ref().map(|s| s.title.as_str());
            let id = egui::accesskit_root_id().accesskit_id();
            if let Some(title) = title.filter(|t| !t.is_empty())
                && let Some((_, node)) = root.nodes.iter_mut().find(|(n, _)| *n == id)
            {
                node.set_label(title);
            }
            let view = match slot.view.as_mut() {
                Some(view) => view,
                None => app.view_mut(),
            };
            let updates = view.graft_updates(root, &mut slot.tree_told);
            if let Some(adapter) = surface.state.accesskit.as_mut()
                && (updates.grafted.is_some() || slot.sent_root.as_ref() != Some(&updates.root))
            {
                slot.sent_root = Some(updates.root.clone());
                adapter.update_if_active(|| updates.root);
                if let Some(grafted) = updates.grafted {
                    adapter.update_if_active(|| grafted);
                }
            }
            if updates.again {
                repaints.request(slot.key, Instant::now());
            }
        }
        // The clipboard is the platform's, which reads and writes the
        // system's only when asked. egui-winit's keeps a thread on the
        // window's own connection, which woke on every event it got.
        let clipboard = ljos_platform::Platform::of(&engine).clipboard();
        platform_output.commands.retain(|command| match command {
            egui::OutputCommand::CopyText(text) => {
                // As a copy in Qt, a write the system refuses is dropped.
                let _ = clipboard.set_text(ljos_platform::ClipboardMode::Clipboard, text);
                false
            }
            _ => true,
        });
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
                    let text = ljos_platform::Platform::of(&engine)
                        .clipboard()
                        .text(ljos_platform::ClipboardMode::Clipboard);
                    match text.filter(|t| !t.is_empty()) {
                        Some(text) => egui::Event::Paste(text.replace("\r\n", "\n")),
                        None => continue,
                    }
                }
                // Pictures of a window are the development tools' to take.
                egui_winit::ActionRequested::Screenshot(_) => continue,
                // A hidden window shows nothing; the Engine's own dirty
                // state, not a widget, says when a frame is painted.
                egui_winit::ActionRequested::PaintWhileHidden => continue,
            };
            surface.state.egui_input_mut().events.push(event);
        }

        let Some(painter) = slot.painter.as_mut().filter(|_| paint) else {
            surface::forget_freed(&mut slot.pending);
            return;
        };
        let view = match slot.view.as_ref() {
            Some(view) => view,
            None => app.view(),
        };
        let clear = view.backdrop().unwrap_or(slot.clear);
        let wake = view.wakes_in();
        let now = Instant::now();
        if let Painter::Cpu(painter) = painter {
            // egui's pictures are never drawn: the scene is.
            slot.pending.clear();
            let Some(snapshot) = view.scene().snapshot() else {
                slot.not_drawn(now, wake);
                return;
            };
            let [r, g, b, a] = clear.to_array();
            let clear = ljos_render::Rgba::from_premul(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                f32::from(a) / 255.0,
            );
            if self.painter_automatic && crate::cpu::leaves_out(snapshot, !shapes.is_empty(), clear)
            {
                // Not drawn: the window is drawn by GL in the next pass.
                self.to_gl = true;
                repaints.request(slot.key, now);
                return;
            }
            let mut warn = |what: String| engine.log(LogLevel::Warning, CATEGORY, what);
            let drew = painter.draw(
                snapshot,
                pixels_per_point,
                clear,
                !shapes.is_empty(),
                &surface.window,
                &mut warn,
            );
            if !drew {
                slot.not_drawn(now, wake);
                #[cfg(feature = "mcp")]
                if let Some(w) = self.wake.as_mut() {
                    w.unchanged += 1;
                }
                return;
            }
            #[cfg(feature = "mcp")]
            if slot.view.is_none() && !self.drawn_once {
                app.record_startup(ljos_dev::mcp::StartupStep::FirstFrame, self.began.elapsed());
            }
            show_when_drawn(host, surface, &mut slot.activate, screens_stale);
            slot.drawn(began, Instant::now());
            #[cfg(feature = "mcp")]
            if let Some(w) = self.wake.as_mut() {
                w.drawn += 1;
            }
            if !std::mem::replace(&mut self.drawn_once, true) {
                self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
            }
            return;
        }
        // Where a pass changed the picture is worked out for a painter that
        // draws all of it too while a client follows items in it.
        #[cfg(feature = "mcp")]
        let follows = slot.view.is_none() && app.watches_items();
        #[cfg(not(feature = "mcp"))]
        let follows = false;
        let change = if painter.wants_areas() || follows {
            slot.shown.change(&shapes, pixels_per_point, clear)
        } else {
            slot.shown.whole_change(&shapes, pixels_per_point, clear)
        };
        #[cfg(feature = "mcp")]
        let changed = follows.then(|| change.boxes());
        let change = if painter.wants_areas() {
            change.with(painter.resampled(&shapes, &slot.pending, pixels_per_point))
        } else {
            change.whole()
        };
        let frame = painter.frame(change, &slot.pending, pixels_per_point);
        // The root's always: its view is the application's. Another
        // window's only while a client records frames.
        #[cfg(feature = "mcp")]
        let mut meter = if slot.view.is_none() {
            app.inspected()
        } else {
            app.records_frames()
        }
        .then(|| Meter::new(&frame, &slot.pending));
        #[cfg(feature = "mcp")]
        let asked = slot.asked.take();
        if frame == Frame::Nothing {
            painter.paint(
                pixels_per_point,
                clear,
                &frame,
                &[],
                &mut slot.pending,
                &surface.window,
            );
            slot.not_drawn(now, wake);
            #[cfg(feature = "mcp")]
            if let Some(w) = self.wake.as_mut() {
                w.unchanged += 1;
            }
            #[cfg(feature = "mcp")]
            if let Some(meter) = meter {
                let report = meter.report(None, present::Steps::default());
                match slot.view {
                    None => app.record_paint(&report),
                    Some(_) => app.record_window_paint(slot.window, &report),
                }
            }
            return;
        }
        let kept = slot.shown.keep(shapes, pixels_per_point, clear);
        let shapes = if let Frame::Part(areas) = &frame {
            let mut part = kept.to_vec();
            present::clip_to(&mut part, areas, pixels_per_point);
            painter.prepare(&part, pixels_per_point)
        } else {
            painter.prepare(kept, pixels_per_point)
        };
        let primitives = present::tessellate(&slot.ctx, shapes, pixels_per_point);
        #[cfg(feature = "mcp")]
        if let Some(meter) = meter.as_mut() {
            meter.built();
        }
        let steps = painter.paint(
            pixels_per_point,
            clear,
            &frame,
            &primitives,
            &mut slot.pending,
            &surface.window,
        );
        #[cfg(not(feature = "mcp"))]
        let _ = steps;
        #[cfg(feature = "mcp")]
        if let Some(changed) = changed {
            app.record_changed(&engine, changed.as_deref());
        }
        #[cfg(feature = "mcp")]
        if let Some(meter) = meter.take_if(|_| slot.view.is_some()) {
            app.record_window_paint(slot.window, &meter.report(asked, steps));
        }
        #[cfg(feature = "mcp")]
        if let Some(meter) = meter {
            app.record_paint(&meter.report(asked, steps));
            if !self.drawn_once {
                app.record_startup(ljos_dev::mcp::StartupStep::FirstFrame, self.began.elapsed());
            }
        }
        show_when_drawn(host, surface, &mut slot.activate, screens_stale);
        slot.drawn(began, Instant::now());
        #[cfg(feature = "mcp")]
        if let Some(w) = self.wake.as_mut() {
            w.drawn += 1;
        }
        if !std::mem::replace(&mut self.drawn_once, true) {
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
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
        #[cfg(feature = "mcp")]
        if let Some(w) = self.wake.as_mut() {
            match event {
                // Not asked by the shell: the platform's own.
                WindowEvent::RedrawRequested if self.slots[i].redraw_asked.is_none() => {
                    w.redraws += 1;
                }
                WindowEvent::RedrawRequested => w.requested_redraws += 1,
                event if starts_input(event) => w.input += 1,
                _ => w.window += 1,
            }
        }
        let engine = Rc::clone(self.app.engine());
        let sys = WindowSystem::of(&engine);
        match event {
            WindowEvent::RedrawRequested => {
                self.slots[i].redraw_granted = true;
                return self.pass(host, i, true);
            }
            WindowEvent::CloseRequested => return self.close_requested(i, &engine, &sys),
            WindowEvent::Destroyed => return,
            _ => {}
        }
        let caps = host.capabilities();
        #[cfg(feature = "mcp")]
        if is_an_act(event) {
            self.app
                .record_startup(ljos_dev::mcp::StartupStep::FirstInput, self.began.elapsed());
        }
        let slot = &mut self.slots[i];
        if slot.blocked && starts_input(event) {
            return;
        }
        let Some(surface) = slot.surface.as_mut() else {
            return;
        };
        if slot.tree_told && matches!(event, WindowEvent::Focus(_)) {
            // The adapters before egui 0.37 could not take the window's
            // focus changing while the focus is in the scene's tree
            // (`QmlView::release_graft_focus`); those of 0.37 can, and the
            // release stays until a screen reader on a desktop has been
            // heard to do without it. The pass the change asks for says
            // where it is. An adapter that was not told the
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
                slot.sent_root = None;
                self.repaints.request(slot.key, Instant::now());
            }
        }
        follow_modifiers(&mut slot.modifiers, event);
        if let WindowEvent::Key(key) = event
            && asks_paste(key, slot.modifiers, cfg!(target_os = "macos"))
        {
            // The press reaches the scene whatever the clipboard holds; the
            // text, when it holds some, is the engine's before the press
            // asks for it, as egui's paste event gives it under eframe.
            let text = ljos_platform::Platform::of(&engine)
                .clipboard()
                .text(ljos_platform::ClipboardMode::Clipboard);
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                ljos_quick::Clipboard::of(&engine).set_text(&engine, &text.replace("\r\n", "\n"));
            }
        }
        if let Some(raw) = raw
            && surface.state.on_window_event(&surface.window, raw).repaint
        {
            // The hold paces the passes egui asks for, until the scene's
            // next wish, which may be a long way off (a press waits for
            // its press-and-hold). What the platform said is heard now.
            slot.held = None;
            self.repaints.request(slot.key, Instant::now());
        }
        match *event {
            WindowEvent::Resized { .. } => {
                slot.shown.forget();
                slot.held = None;
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
                slot.shown.forget();
                surface.info.occluded = Some(hidden);
                note_state(slot, &engine, &sys);
                self.repaints.request(slot.key, Instant::now());
            }
            _ => {}
        }
    }

    /// A step of a drag from another application: the view of the window
    /// it is over is told, and the window system hears the view's answer.
    fn drag_step(&mut self, step: drags::DragStep) {
        use drags::{At, DragStep};
        if matches!(step, DragStep::OutEnded) {
            // `AboutToWait`, which follows, settles it.
            return;
        }
        let by_target = |slots: &[Slot], target: usize| {
            slots.iter().position(|s| {
                s.surface.as_ref().and_then(|x| drags::target_of(&x.window)) == Some(target)
            })
        };
        let i = match &step {
            DragStep::Offered { target, .. } | DragStep::Entered { target, .. } => {
                let i = by_target(&self.slots, *target);
                self.drag_over = i.map(|i| self.slots[i].key);
                i
            }
            _ => self
                .drag_over
                .and_then(|key| self.slots.iter().position(|s| s.key == key)),
        };
        let Some(i) = i else {
            // Over no window the shell shows, or one closed since.
            match step {
                DragStep::Offered {
                    answer, answered, ..
                } => {
                    answer.taken(None);
                    answered();
                }
                DragStep::Entered { answer, .. } | DragStep::Moved { answer, .. } => {
                    answer.taken(None);
                }
                DragStep::Dropped { answer } => answer.dropped(None),
                DragStep::Left | DragStep::OutEnded => {}
            }
            return;
        };
        let slot = &mut self.slots[i];
        let key = slot.key;
        // Wayland says logical pixels and X11 the window's own; egui's
        // points are logical pixels over the zoom.
        let zoom = slot.ctx.zoom_factor();
        let scale = slot
            .surface
            .as_ref()
            .map_or(1.0, |x| x.window.scale_factor());
        let point = |at: At| {
            let (x, y) = match at {
                At::Logical(x, y) => (x, y),
                At::Physical(x, y) => (x / scale, y / scale),
            };
            egui::pos2(x as f32 / zoom, y as f32 / zoom)
        };
        let view = match slot.view.as_mut() {
            Some(view) => view,
            None => self.app.view_mut(),
        };
        match step {
            DragStep::Offered {
                at,
                formats,
                actions,
                answer,
                answered,
                ..
            } => {
                answer.taken(view.window_drag_offered(&formats, point(at), actions));
                answered();
                // The scene is not told until the data is read.
                return;
            }
            DragStep::Entered {
                at,
                data,
                actions,
                answer,
                ..
            } => {
                answer.taken(view.window_drag(WindowDrag::Entered {
                    data,
                    at: point(at),
                    actions,
                }));
            }
            DragStep::Moved {
                at,
                actions,
                answer,
            } => {
                answer.taken(view.window_drag(WindowDrag::Moved {
                    at: point(at),
                    actions,
                }));
            }
            DragStep::Left => {
                self.drag_over = None;
                view.window_drag(WindowDrag::Left);
            }
            DragStep::Dropped { answer } => {
                self.drag_over = None;
                answer.dropped(view.window_drag(WindowDrag::Dropped));
            }
            DragStep::OutEnded => {}
        }
        self.repaints.request(key, Instant::now());
    }

    /// What became of the application's drag out of its windows: the
    /// views let go of the press the window system took for it, and the
    /// application hears how it ended.
    fn drag_out_settle(&mut self) {
        let Some(out) = self.drag_out.clone() else {
            return;
        };
        if out.took_pointer() {
            for i in 0..self.slots.len() {
                let key = self.slots[i].key;
                let taken = match self.slots[i].view.as_mut() {
                    Some(view) => view.pointer_taken(),
                    None => self.app.view_mut().pointer_taken(),
                };
                if taken {
                    self.repaints.request(key, Instant::now());
                }
            }
        }
        out.settle(&Rc::clone(self.app.engine()));
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
                self.needs_sync.set(true);
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
                slot.tree_told = false;
                slot.sent_root = None;
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                if let Some(surface) = slot.surface.as_mut() {
                    surface.state.on_accesskit_action_request(request);
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                slot.ctx.disable_accesskit();
                slot.tree_told = false;
                slot.sent_root = None;
                return;
            }
        }
        self.repaints.request(slot.key, Instant::now());
    }

    /// A context or a waker asked for a pass.
    fn repaint_asked(&mut self, key: u64, when: Instant, pass: Option<u64>, at_once: bool) {
        let Some(slot) = self.slots.iter().find(|s| s.key == key) else {
            return;
        };
        // A pass that ran since answered it, unless it was asked while
        // that pass ran.
        let now = slot.ctx.cumulative_pass_nr_for(ViewportId::ROOT);
        if pass.is_none_or(|pass| now == pass || now == pass + 1) {
            self.asked(key, when, at_once);
        }
    }

    /// Window `key` is to run a pass at `when`.
    fn asked(&mut self, key: u64, when: Instant, at_once: bool) {
        let Some(slot) = self.slots.iter_mut().find(|s| s.key == key) else {
            return;
        };
        if at_once {
            slot.asked_at_once();
        }
        #[cfg(feature = "mcp")]
        {
            slot.asked = Some(slot.asked.map_or(when, |asked| asked.min(when)));
        }
        self.repaints.request(key, when);
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
                // Animations step, and a frame may spend on creating
                // items, as often as the display shows one.
                #[cfg(feature = "mcp")]
                if slot.is_root() {
                    app.set_frame_budget(slot.frame_interval());
                }
                let view = match slot.view.as_mut() {
                    Some(view) => view,
                    None => app.view_mut(),
                };
                view.set_refresh_rate(hz);
            }
        }
    }

    /// Tells the `app_metrics` `memory` requests that wait what each window's scene
    /// and renderer hold, the root's first. A renderer is counted whether
    /// or not its window is on the screen: what it keeps is held all the
    /// same.
    #[cfg(feature = "mcp")]
    fn serve_memory(&mut self) {
        let windows: Vec<_> = self
            .slots
            .iter()
            .map(|slot| {
                let view = slot.view.as_ref().unwrap_or_else(|| self.app.view());
                ljos_dev::mcp::WindowMemory {
                    window: slot.window,
                    scene: view.scene().memory(),
                    render: Some(view.backend().memory()),
                }
            })
            .collect();
        self.app.finish_memory(&windows);
    }

    /// Asks the platform for the size an agent wants the primary window
    /// to have (`app_resize`), as for a `Window` whose size the
    /// application wrote; the pass at the size given answers the agent.
    #[cfg(feature = "mcp")]
    fn serve_resize(&mut self, host: &mut Control<'_>, engine: &Engine, sys: &WindowSystem) {
        let Some(size) = self.app.take_resize_request() else {
            return;
        };
        let Some(i) = self.slots.iter().position(Slot::is_root) else {
            return self.app.finish_resize(true);
        };
        let size = match self.spec_of(i, engine, sys) {
            Some(spec) => windows::within_limits(&spec, size),
            None => size,
        };
        let slot = &mut self.slots[i];
        let Some(id) = slot.surface.as_ref().map(|s| s.id) else {
            return self.app.finish_resize(true);
        };
        let zoom = f64::from(slot.ctx.zoom_factor());
        host.change_window(id, WindowChange::Size(scaled(size, zoom)));
        note_size(slot, engine, sys, false);
        let now = Instant::now();
        self.resize_due = Some(now + RESIZE_WAIT);
        self.repaints.request(slot.key, now);
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
                WindowRequest::StartMove => host.change_window(surface.id, WindowChange::StartMove),
                WindowRequest::StartResize(edges) => {
                    host.change_window(surface.id, WindowChange::StartResize(resize_edge(edges)));
                }
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
    /// last window was closed unless it keeps running without one
    /// (`Qt.application.quitOnLastWindowClosed`). A window that is only
    /// hidden ends nothing.
    fn should_exit(&mut self, engine: &Engine, sys: &WindowSystem) -> bool {
        if engine.quit_requested() {
            return true;
        }
        let closed = sys.closed_count() + u64::from(self.root_closed);
        if closed == self.closes_seen {
            return false;
        }
        self.closes_seen = closed;
        let root_shown = self.slots[0].window.is_none() && !self.root_closed;
        let any_shown = sys
            .windows(engine)
            .iter()
            .any(|w| sys.spec(engine, *w).is_some_and(|s| s.visible));
        !root_shown && !any_shown && ljos_controls::quit_on_last_window_closed(engine)
    }

    /// The user opened the application again. A root that is an item
    /// and was closed has no window the application could show: it is
    /// shown again.
    fn reopened(&mut self, has_visible_windows: bool) {
        if self.root_closed && !has_visible_windows {
            self.root_closed = false;
            self.needs_sync.set(true);
        }
        ljos_controls::application_reopened(self.app.engine(), has_visible_windows);
    }

    /// Asks the platform to let the windows draw whose time has come, and
    /// runs the passes of those that cannot be seen. At the start of a
    /// turn (`early`) it only asks: asked then, the platform lets the
    /// window draw later in the same turn, after the turn's input; asked
    /// at its end, it took a turn of its own to hear yes, each frame. What
    /// it would run itself waits for the end of the turn, for the input.
    fn schedule(&mut self, host: &mut Control<'_>, now: Instant, early: bool) {
        let mut draw = Vec::new();
        for key in self.repaints.take_due(now) {
            let Some(i) = self.slots.iter().position(|s| s.key == key) else {
                continue;
            };
            let slot = &mut self.slots[i];
            let held = slot.held.filter(|t| *t > now);
            match (slot.surface.as_ref(), held) {
                // The display would have held a drawn frame this long.
                (Some(_), Some(until)) if !slot.out_of_sight() => self.repaints.request(key, until),
                (Some(_), _) if early && !slot.out_of_sight() && slot.redraw_granted => {
                    self.repaints.request(key, now);
                }
                // The platform would answer at once: it is not asked.
                (Some(_), _) if !slot.out_of_sight() && slot.redraw_granted => {
                    slot.redraw_asked.get_or_insert(now);
                    draw.push(i);
                }
                (Some(surface), _) if !slot.out_of_sight() => {
                    host.request_redraw(surface.id);
                    slot.redraw_asked.get_or_insert(now);
                    #[cfg(feature = "mcp")]
                    if let Some(w) = self.wake.as_mut() {
                        w.redraws_asked += 1;
                    }
                }
                (Some(_), _) => slot.want_undrawn_pass(now),
                // The root's scene runs without a window: it is where
                // the timers and the messages are.
                (None, _) if slot.is_root() => slot.want_undrawn_pass(now),
                // Another window's scene waits until it is shown.
                (None, _) => {}
            }
        }
        if early {
            return;
        }
        let mut due = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !slot.stalled && slot.redraw_asked.is_some_and(|t| now >= t + STALL) {
                // The request stays with the platform: when the window
                // can be seen again it is drawn, and no longer stalled.
                slot.stalled = true;
                slot.want_undrawn_pass(now);
            }
            if slot.undrawn_pass_due.is_some_and(|t| t <= now) {
                slot.undrawn_pass_due = None;
                due.push(i);
            }
        }
        for i in draw {
            self.pass(host, i, true);
        }
        for i in due {
            self.pass(host, i, false);
        }
    }

    /// When the loop has to wake by itself.
    fn next_wake(&self) -> Option<Instant> {
        let mut next = earlier(self.repaints.next(), self.heap_trim);
        for slot in &self.slots {
            let stall = slot
                .redraw_asked
                .filter(|_| !slot.stalled)
                .map(|t| t + STALL);
            for t in [slot.size_check, slot.undrawn_pass_due, stall] {
                next = earlier(next, t);
            }
        }
        #[cfg(feature = "mcp")]
        {
            next = earlier(next, self.resize_due);
        }
        next
    }

    /// Gives the platform the icons the application changed since they
    /// were last given: its own, and each open window's.
    fn show_icons(&mut self, host: &mut Control<'_>, engine: &Engine) {
        let platform = ljos_platform::Platform::of(engine);
        let changed = platform.icons_changed();
        if changed != self.icons_shown {
            self.icons_shown = changed;
            let icon = platform.application_icon().map(|i| host_icon(&i));
            host.set_application_icon(icon.as_ref());
        }
        for slot in &mut self.slots {
            if let Some(surface) = &slot.surface
                && slot.told_icons != changed
            {
                slot.told_icons = changed;
                let icon = platform.window_icon(slot.window).map(|i| host_icon(&i));
                host.change_window(surface.id, WindowChange::Icon(icon));
            }
        }
    }

    fn unsettled(&self) -> bool {
        self.needs_sync.get() || self.app.view().scene().root() != self.last_root
    }

    /// Everything that waited was delivered: the bookkeeping, and then
    /// the loop sleeps until the time given to the host.
    fn settle(&mut self, host: &mut Control<'_>) {
        if self.exiting {
            return;
        }
        let engine = Rc::clone(self.app.engine());
        let sys = WindowSystem::of(&engine);
        let mut last = None;
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
            self.show_icons(host, &engine);
            self.serve_requests(host, &sys);
            #[cfg(feature = "mcp")]
            self.serve_resize(host, &engine, &sys);
            let now = Instant::now();
            last = Some(now);
            for slot in &mut self.slots {
                if slot.size_check.is_some_and(|t| t <= now) {
                    slot.size_check = None;
                    note_size(slot, &engine, &sys, true);
                }
            }
            // The pass at the size there is answers the agent that waits.
            #[cfg(feature = "mcp")]
            if self.resize_due.is_some_and(|t| t <= now)
                && let Some(slot) = self.slots.iter().find(|s| s.is_root())
            {
                self.repaints.request(slot.key, now);
            }
            sys.set_application_state(&engine, self.application_state());
            if self.should_exit(&engine, &sys) {
                self.exiting = true;
                host.exit();
                return;
            }
            self.schedule(host, now, false);
            if !self.unsettled() {
                break;
            }
        }
        // Once, after the first frame is shown: the first click then runs
        // its handler and not the start of the JS host too.
        if self.drawn_once && !self.scripts_prepared {
            self.scripts_prepared = true;
            engine.prepare_scripts();
        }
        self.trim_heap_when_done(&engine);
        // The last round's time: what ran since is too short to matter
        // to a rest of seconds, and a pass reads no clock it need not.
        self.collect_scripts_when_resting(&engine, last.unwrap_or_else(Instant::now));
        let mut next = earlier(self.next_wake(), self.collect.next());
        if self.unsettled() {
            // Applying what the windows want keeps changing what they
            // want. Not in a spin: at the pace of the display.
            next = earlier(next, Some(Instant::now() + HEARTBEAT));
        }
        #[cfg(feature = "mcp")]
        self.record_rest(next);
        host.wake_at(next);
    }

    /// Tells the inspector the loop sleeps until `next`, and where the
    /// primary window's last pass saw the pointer: a client that sent
    /// input from outside waits for this, not for a while.
    #[cfg(feature = "mcp")]
    fn record_rest(&mut self, next: Option<Instant>) {
        if !self.app.inspected() {
            return;
        }
        // What the window shows is not yet the scene while its pass is
        // due, held to the display's pace, or waits for the window system.
        // A pass asked for seconds ahead (egui's delayed repaint) is no
        // frame owed now, and a window out of sight draws nothing until
        // it is seen again.
        let now = Instant::now();
        let frame_owed = self.slots.iter().find(|s| s.is_root()).is_some_and(|slot| {
            slot.surface.is_some()
                && !slot.out_of_sight()
                && (slot.redraw_asked.is_some()
                    || self
                        .repaints
                        .wants_by(slot.key, now + slot.frame_interval()))
        });
        let pointer = self.slots.iter().find(|s| s.is_root()).and_then(|slot| {
            slot.ctx.input(|i| {
                i.pointer.latest_pos().map(|p| ljos_dev::mcp::PointerSeen {
                    x: p.x,
                    y: p.y,
                    pressed: i.pointer.any_down(),
                })
            })
        });
        self.app.record_rest(
            ljos_dev::mcp::Rest {
                wake_at: next,
                frame_owed,
                pointer,
            },
            self.collect.next(),
        );
    }

    #[cfg(feature = "mcp")]
    fn note_wake(&mut self, note: impl FnOnce(&mut ljos_dev::mcp::HostWake)) {
        if let Some(w) = self.wake.as_mut() {
            note(w);
        }
    }

    /// A turn of the loop ended: what woke it goes to the inspector, and
    /// the next turn is noted only while a client keeps wakes. Nothing
    /// else costs anything while none does.
    #[cfg(feature = "mcp")]
    fn end_wake(&mut self) {
        if let Some(w) = self.wake.take() {
            self.app.record_wake(&w);
        }
        if self.app.records_wakes() {
            self.wake = Some(ljos_dev::mcp::HostWake::default());
        }
    }

    /// Start-up, a burst of picture loads and a page of objects going
    /// free most of what they used (the GPU driver's shader compiles, QML
    /// being loaded, pictures being decoded, the objects and bindings of a
    /// page a `StackView` replaced), but glibc keeps freed memory inside
    /// its heaps. A while after each is over, it is given back. Each is an
    /// event: an application that sits idle, or animates, trims nothing
    /// more.
    fn trim_heap_when_done(&mut self, engine: &Engine) {
        let loading = ljos_quick::ImageStore::of(engine).pending() > 0;
        if std::mem::replace(&mut self.loading, loading) && !loading {
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
        }
        let objects = engine.object_count();
        if self.objects_high.fell(objects, OBJECTS_GONE_BEFORE_TRIM) {
            // Pushed back while more go: one trim after the last.
            self.objects_high.reset(objects);
            self.heap_trim = Some(Instant::now() + HEAP_TRIM_AFTER);
        }
        if loading {
            // Not in the middle of a burst: its pictures reuse the room.
            self.heap_trim = self.heap_trim.map(|_| Instant::now() + HEAP_TRIM_AFTER);
        }
        if self.heap_trim.is_some_and(|t| t <= Instant::now()) {
            self.heap_trim = None;
            // Start-up is over and the window idle: what a first scroll
            // would otherwise compile is compiled now.
            for painter in self.slots.iter_mut().filter_map(|s| s.painter.as_mut()) {
                painter.warm();
            }
            crate::give_back_freed_heap();
        }
    }

    /// QuickJS collects by itself only after the heap grew by megabytes,
    /// and an application that then rests keeps all of it. What a burst
    /// of work left behind is collected once the loop has rested a while
    /// instead, and only once it grew: a collection walks the whole
    /// script heap, so it is not done at every rest of a scroll, and a
    /// loop whose scripts made nothing to collect is not woken for one.
    fn collect_scripts_when_resting(&mut self, engine: &Engine, now: Instant) {
        let soon = now + REST_BEFORE_COLLECTING;
        let busy = self.unsettled() || self.next_wake().is_some_and(|t| t < soon);
        let wanted = engine.wants_idle_collect();
        if self
            .collect
            .settled(busy, wanted, now, REST_BEFORE_COLLECTING)
        {
            engine.collect_if_idle();
        }
    }

    /// The loop ends. What draws goes while the display is still there.
    fn shut_down(&mut self) {
        self.exiting = true;
        // Before the loop lets its display go: the thread reads from it.
        let drags = self.drags.borrow_mut().take();
        if let Some(drags) = drags {
            drags.stop();
        }
        for slot in &mut self.slots {
            slot.surface = None;
            slot.painter = None;
        }
        self.gpu = None;
        self.starting = None;
    }
}

impl<M: Send + 'static> HostApp<WinitHost<ShellEvent>> for Desktop<M> {
    fn event(&mut self, host: &mut Control<'_>, event: Event<'_, RawEvent, ShellEvent>) {
        match event {
            Event::SurfaceCreated => {
                self.live = true;
                self.needs_sync.set(true);
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
            Event::User(ShellEvent::Repaint {
                slot,
                when,
                pass,
                at_once,
            }) => {
                #[cfg(feature = "mcp")]
                self.note_wake(|w| w.posted += 1);
                self.repaint_asked(slot, when, pass, at_once);
                // A message that finds the window's frame due asks for it
                // now, as a deadline's wake does: the platform's yes comes
                // later in this turn, not in a turn of its own.
                if !self.exiting && !self.unsettled() {
                    self.schedule(host, Instant::now(), true);
                }
            }
            Event::User(ShellEvent::AccessKit(event)) => {
                #[cfg(feature = "mcp")]
                self.note_wake(|w| w.accessibility += 1);
                self.accesskit_event(event);
            }
            Event::User(ShellEvent::Drag(step)) => self.drag_step(step),
            Event::Reopen {
                has_visible_windows,
            } => self.reopened(has_visible_windows),
            Event::AboutToWait => {
                #[cfg(feature = "mcp")]
                {
                    let dirty = self.needs_sync.get();
                    self.note_wake(|w| w.dirty = dirty);
                }
                if let Some(drags) = self.drags.borrow_mut().as_mut() {
                    drags.dispatch();
                }
                self.drag_out_settle();
                self.settle(host);
                #[cfg(feature = "mcp")]
                self.end_wake();
            }
            Event::Exiting => self.shut_down(),
            Event::MemoryPressure => self.trim(),
            // Each is followed by `AboutToWait`, which looks at everything.
            Event::Wake => {
                #[cfg(feature = "mcp")]
                self.note_wake(|w| w.deadline = true);
                if !self.exiting && !self.unsettled() {
                    self.schedule(host, Instant::now(), true);
                }
            }
            Event::User(ShellEvent::Wake) => {
                #[cfg(feature = "mcp")]
                self.note_wake(|w| w.posted += 1);
            }
            Event::OpenUrl(_) | Event::SaveState => {}
        }
    }
}

/// [`crate::run`].
pub(crate) fn run<M: Send + 'static>(
    config: ShellConfig,
    setup: impl FnOnce(&mut Setup<'_, M>) -> Result<ObjectId, SetupError> + 'static,
) -> Result<(), ShellError> {
    #[cfg(feature = "mcp")]
    let began = Instant::now();
    // Only with the system's fonts: the bundled ones have none of the
    // desktop's families.
    let desktop_font = config
        .system_fonts
        .then(crate::desktop_font::Reading::start);
    let host = WinitHost::<ShellEvent>::with_x11_preferred(crate::platform::prefers_x11(
        config.prefer_x11,
        std::env::var("LJOS_PREFER_X11").ok().as_deref(),
    ))?;
    // The drivers start while the application is built.
    // A window that can be seen through is not chosen for the
    // processor: its pixels have no alpha.
    let opaque = config.background.to_rgba8()[3] == 255;
    let chosen = surface::Chosen::from_env(
        std::env::var("LJOS_PAINTER").ok().as_deref(),
        config.drawn_on_processor,
        || opaque && only_software(&host.display_handle()),
    );
    let starting = Starting::start(host.display_handle(), chosen);
    // Before `setup`: bindings may measure text while the root is being
    // built. They wait for the fonts; making the engine and registering
    // its types do not.
    match config
        .system_fonts
        .then(crate::system_fonts::take)
        .flatten()
    {
        Some(reading) => ljos_quick::set_text_engine_later(reading),
        None if config.system_fonts => {
            ljos_quick::set_text_engine(ljos_quick::TextEngine::with_system_fonts());
        }
        None => crate::system_fonts::forget(),
    }
    // Before the engine is built: `font` defaults to it everywhere.
    if let Some(font) = desktop_font {
        font.apply();
    }
    let proxy: Proxy = Arc::new(Mutex::new(host.proxy()));
    let ctx = new_context(0, &proxy);
    // True from the start: the first turn of the loop opens the windows.
    let needs_sync = Rc::new(Cell::new(true));
    let hosting = Hosting {
        wayland: host.is_wayland(),
        tree: tree_id(0),
        wake: waker(0, &proxy),
        changed: {
            let needs_sync = Rc::clone(&needs_sync);
            Box::new(move || needs_sync.set(true))
        },
    };
    #[cfg_attr(not(feature = "mcp"), allow(unused_mut))]
    let mut app = ShellApp::hosted(&ctx, &config, Arc::new(SystemClock::new()), setup, hosting)?;
    if chosen.automatic {
        app.engine().log(
            LogLevel::Info,
            CATEGORY,
            "drawn on the processor: EGL would draw in software",
        );
    }
    #[cfg(feature = "mcp")]
    app.record_startup(ljos_dev::mcp::StartupStep::Built, began.elapsed());
    let failure = Rc::new(RefCell::new(None));
    #[cfg_attr(not(feature = "mcp"), allow(unused_mut))]
    let mut desktop = Desktop::new(
        app,
        config,
        ctx,
        proxy,
        needs_sync,
        Rc::clone(&failure),
        starting,
    );
    #[cfg(feature = "mcp")]
    {
        desktop.began = began;
    }
    *desktop.drags.borrow_mut() = drags::Drags::start(&host.display_handle(), {
        let proxy = Arc::clone(&desktop.proxy);
        Arc::new(move |step| {
            if let Ok(proxy) = proxy.lock() {
                // An error is a loop that has ended, and the drag with it.
                let _ = proxy.send_event(ShellEvent::Drag(step));
            }
        })
    });
    // What follows drags in carries them out too.
    let follows = desktop.drags.borrow().is_some();
    if !follows && wayland_drag::is_wayland(&host.display_handle()) {
        crate::platform::note_no_wayland_drops(desktop.app.engine());
    }
    if follows {
        let drag_out = Rc::new(drags::DragOut::new(Rc::clone(&desktop.drags)));
        ljos_platform::Platform::of(desktop.app.engine()).set_drag_out(Some(
            Rc::clone(&drag_out) as Rc<dyn ljos_platform::DragOutService>
        ));
        desktop.drag_out = Some(drag_out);
    }
    host.run(desktop)?;
    match failure.borrow_mut().take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// `edges` is one edge or a corner; `ResizeEdges::from_qt` refused the rest.
fn resize_edge(edges: ResizeEdges) -> ResizeEdge {
    match (edges.top, edges.bottom, edges.left, edges.right) {
        (true, _, true, _) => ResizeEdge::NorthWest,
        (true, _, _, true) => ResizeEdge::NorthEast,
        (true, ..) => ResizeEdge::North,
        (_, true, true, _) => ResizeEdge::SouthWest,
        (_, true, _, true) => ResizeEdge::SouthEast,
        (_, true, ..) => ResizeEdge::South,
        (_, _, true, _) => ResizeEdge::West,
        _ => ResizeEdge::East,
    }
}

/// Whether EGL would offer only a software rasteriser
/// ([`crate::software`]); never where there is no EGL.
fn only_software(display: &egui_winit::winit::event_loop::OwnedDisplayHandle) -> bool {
    #[cfg(gl_painter)]
    return crate::software::only_software(display);
    #[cfg(not(gl_painter))]
    {
        let _ = display;
        false
    }
}

/// Makes egui's font atlas again, so that it is sent whole with the next
/// pass: a painter that was not given it, as one that drew on the
/// processor was not, has none. egui makes its fonts again when how a
/// missing glyph is drawn changes; it is changed and changed back.
fn resend_font_atlas(ctx: &egui::Context) {
    use egui::epaint::text::MissingGlyphPolicy;
    let policy = ctx.missing_glyph_policy();
    let other = match policy {
        MissingGlyphPolicy::Tofu => MissingGlyphPolicy::Panic,
        _ => MissingGlyphPolicy::Tofu,
    };
    ctx.set_missing_glyph_policy(other);
    ctx.set_missing_glyph_policy(policy);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: ljos_host::Key, code: &str) -> ljos_host::KeyEvent {
        ljos_host::KeyEvent {
            key,
            code: Some(code.to_owned()),
            state: ButtonState::Pressed,
            repeat: false,
            text: None,
        }
    }

    fn v() -> ljos_host::KeyEvent {
        press(ljos_host::Key::Character("v".into()), "KeyV")
    }

    const CTRL: ljos_host::Modifiers = ljos_host::Modifiers {
        shift: false,
        control: true,
        alt: false,
        meta: false,
    };

    /// Issue #130: the chords that paste are known before egui-winit
    /// hears them, Shift or not, so the clipboard's text is there when
    /// the scene's press asks for it.
    #[test]
    fn the_paste_chords_are_known_with_their_modifiers() {
        let shift = ljos_host::Modifiers {
            shift: true,
            ..CTRL
        };
        let cmd = ljos_host::Modifiers {
            meta: true,
            ..Default::default()
        };
        let shift_only = ljos_host::Modifiers {
            shift: true,
            ..Default::default()
        };
        assert!(asks_paste(&v(), CTRL, false));
        assert!(asks_paste(&v(), shift, false));
        assert!(asks_paste(&v(), cmd, true));
        let big_v = press(ljos_host::Key::Character("V".into()), "KeyV");
        assert!(asks_paste(&big_v, shift, false));
        // A layout whose V key types no Latin letter still pastes there.
        let cyrillic = press(ljos_host::Key::Character("м".into()), "KeyV");
        assert!(asks_paste(&cyrillic, CTRL, false));
        let insert = press(ljos_host::Key::Named("Insert".into()), "Insert");
        assert!(asks_paste(&insert, shift_only, false));
        let paste = press(ljos_host::Key::Named("Paste".into()), "Paste");
        assert!(asks_paste(&paste, Default::default(), false));

        assert!(!asks_paste(&v(), Default::default(), false));
        assert!(!asks_paste(&v(), CTRL, true));
        assert!(!asks_paste(&v(), cmd, false));
        let alt = ljos_host::Modifiers { alt: true, ..CTRL };
        assert!(!asks_paste(&v(), alt, false));
        // A Latin layout's V elsewhere on the keyboard is the letter typed.
        let dvorak = press(ljos_host::Key::Character("k".into()), "KeyV");
        assert!(!asks_paste(&dvorak, CTRL, false));
        let released = ljos_host::KeyEvent {
            state: ButtonState::Released,
            ..v()
        };
        assert!(!asks_paste(&released, CTRL, false));
    }

    /// A window that lost the keyboard holds no modifier: the release
    /// went to another window.
    #[test]
    fn the_modifiers_held_follow_the_platform_and_drop_with_the_focus() {
        let mut held = ljos_host::Modifiers::default();
        follow_modifiers(&mut held, &WindowEvent::Modifiers(CTRL));
        assert_eq!(held, CTRL);
        follow_modifiers(&mut held, &WindowEvent::Key(v()));
        assert_eq!(held, CTRL);
        follow_modifiers(&mut held, &WindowEvent::Focus(false));
        assert_eq!(held, ljos_host::Modifiers::default());
    }

    #[test]
    fn every_window_has_an_accessibility_tree_of_its_own_and_none_has_eguis() {
        assert_ne!(tree_id(0), TreeId::ROOT);
        assert_ne!(tree_id(0), tree_id(1));
        assert_ne!(tree_id(1), tree_id(2));
    }

    /// The wheel settles the loop every notch, resting between: the
    /// scripts' heap is looked at once, a rest after the last notch.
    #[test]
    fn a_turning_wheel_looks_at_the_scripts_once_it_stops() {
        let rest = Duration::from_millis(100);
        let t = Instant::now();
        let at = |ms| t + Duration::from_millis(ms);
        let mut collect = AfterRest::default();
        let mut looked = 0;
        for notch in 0..20 {
            // The notch's scroll, and the rest after it.
            assert!(!collect.settled(true, true, at(notch * 40), rest));
            looked += u32::from(collect.settled(false, true, at(notch * 40 + 16), rest));
            assert!(collect.next().is_some());
        }
        assert_eq!(looked, 0, "not during the scroll");
        let due = collect.next().unwrap();
        assert_eq!(due, at(19 * 40 + 16) + rest);
        assert!(collect.settled(false, true, due, rest));
        assert_eq!(collect.next(), None, "and then the loop sleeps");
    }

    /// An agent asking every 50 ms, or a timer the system runs a little
    /// before it was due, wakes the loop with nothing to do: the rest
    /// goes on, and the scripts are looked at once, when it was due.
    #[test]
    fn wakes_that_are_not_work_leave_the_rest_as_it_was() {
        let rest = Duration::from_millis(100);
        let t = Instant::now();
        let at = |ms| t + Duration::from_millis(ms);
        let mut collect = AfterRest::default();
        assert!(!collect.settled(false, true, at(0), rest));
        assert!(!collect.settled(false, true, at(50), rest));
        assert!(!collect.settled(false, true, at(99), rest));
        assert_eq!(collect.next(), Some(at(100)));
        assert!(collect.settled(false, true, at(100), rest));
        assert_eq!(collect.next(), None);
    }

    /// Scripts that made nothing since the collector last ran leave
    /// nothing to wake for, however often the loop rests.
    #[test]
    fn a_rest_with_nothing_to_collect_wakes_for_nothing() {
        let rest = Duration::from_millis(100);
        let t = Instant::now();
        let mut collect = AfterRest::default();
        for ms in [0, 100, 250, 1000] {
            assert!(!collect.settled(false, false, t + Duration::from_millis(ms), rest));
            assert_eq!(collect.next(), None);
        }
        // Collected by other means while a rest was under way.
        assert!(!collect.settled(false, true, t, rest));
        assert!(!collect.settled(false, false, t + Duration::from_millis(10), rest));
        assert_eq!(collect.next(), None);
    }

    /// A page of objects replaced by another goes after the new one is
    /// made: the fall from the most there were is what counts, once.
    #[test]
    fn a_page_of_objects_gone_is_noticed_once() {
        let mut high = HighWater::default();
        assert!(!high.fell(100, 1024));
        assert!(!high.fell(14_000, 1024), "the first page");
        assert!(!high.fell(28_000, 1024), "the next, both alive");
        assert!(!high.fell(27_500, 1024), "a few delegates");
        assert!(high.fell(14_000, 1024), "the first page went");
        high.reset(14_000);
        assert!(!high.fell(14_000, 1024), "and is not noticed again");
        assert!(!high.fell(13_500, 1024));
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
        root.want_undrawn_pass(t);
        assert_eq!(root.undrawn_pass_due, Some(t), "nothing ran before: now");
        root.undrawn_pass_at = Some(t);
        root.undrawn_pass_due = None;
        root.want_undrawn_pass(t + Duration::from_millis(1));
        assert_eq!(root.undrawn_pass_due, Some(t + HEARTBEAT));
        // A later request does not push an earlier one back.
        root.want_undrawn_pass(t + Duration::from_millis(500));
        assert_eq!(root.undrawn_pass_due, Some(t + HEARTBEAT));
    }

    #[test]
    fn passes_that_are_not_drawn_wait_for_the_scene_or_a_frame_from_the_second_in_a_row() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        // egui's pass after the drawn one runs when it asks.
        slot.not_drawn(t, None);
        assert_eq!(slot.held, None);
        // The next waits a frame, and each after it.
        slot.not_drawn(t + ms(1), Some(Duration::ZERO));
        assert_eq!(slot.held, Some(t + ms(21)));
        slot.not_drawn(t + ms(21), None);
        assert_eq!(slot.held, Some(t + ms(41)));
        // A drawn pass holds the next a frame after it began, and the
        // pass after that, which drew nothing, runs at once again.
        slot.drawn(t + ms(41), t + ms(41));
        assert_eq!(slot.held, Some(t + ms(61)));
        slot.not_drawn(t + ms(50), None);
        assert_eq!(slot.held, None);
        // An animation's next frame is when the scene said, not the
        // frame earlier egui made of it.
        slot.drawn(t + ms(60), t + ms(60));
        slot.not_drawn(t + ms(60), Some(ms(15)));
        assert_eq!(slot.held, Some(t + ms(75)));
    }

    // A wheel turned while a frame was presented: its pass and egui's
    // next show nothing yet, and the glide's first frame is when the
    // scene asks, not a frame after them.
    #[test]
    fn passes_not_drawn_wait_for_the_scene_when_it_asks_before_a_frame() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        slot.drawn(t, t);
        slot.not_drawn(t + ms(1), Some(ms(8)));
        assert_eq!(slot.held, Some(t + ms(9)));
        slot.not_drawn(t + ms(2), Some(ms(7)));
        assert_eq!(slot.held, Some(t + ms(9)));
        slot.asked_at_once();
        assert_eq!(slot.held, Some(t + ms(9)), "not the frame, which is later");
        // A scene that asks for nothing still waits a frame.
        slot.not_drawn(t + ms(9), None);
        assert_eq!(slot.held, Some(t + ms(29)));
    }

    // A long Timer is pending and the pass drew nothing: a picture
    // decoded on another thread, or the inspector, asks for a pass at once.
    #[test]
    fn a_pass_asked_for_at_once_is_not_held_until_the_scenes_timer() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        slot.not_drawn(t, Some(Duration::from_secs(60)));
        assert_eq!(slot.held, Some(t + Duration::from_secs(60)));
        slot.asked_at_once();
        assert_eq!(
            slot.held, None,
            "the first pass not drawn runs what is asked at once"
        );
        // From the second in a row, it still waits a frame, not the Timer.
        slot.not_drawn(t + ms(1), Some(Duration::from_secs(60)));
        slot.asked_at_once();
        assert_eq!(slot.held, Some(t + ms(21)));
        // A pass that took a frame to draw holds nothing back.
        slot.drawn(t + ms(2), t + ms(22));
        slot.asked_at_once();
        assert_eq!(slot.held, None);
    }

    #[test]
    fn drawn_passes_are_a_frame_apart_when_the_display_does_not_hold_them() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        // Presenting returned at once: the next waits for its frame, and
        // so does one asked for at once.
        slot.drawn(t, t + ms(3));
        assert_eq!(slot.held, Some(t + ms(20)));
        slot.asked_at_once();
        assert_eq!(slot.held, Some(t + ms(20)));
        slot.drawn(t + ms(20), t + ms(23));
        assert_eq!(slot.held, Some(t + ms(40)));
        // A display that holds each frame until it is shown is not
        // waited for again.
        slot.drawn(t + ms(40), t + ms(60));
        assert_eq!(slot.held, None);
        slot.drawn(t + ms(61), t + ms(81));
        assert_eq!(slot.held, None);
    }

    // The display's refresh was at 80 and 100 ms; a pass began late, at 91,
    // and was shown at 100. The next is not held past the refresh after.
    #[test]
    fn a_pass_that_began_late_holds_the_next_only_a_frame_after_it() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        slot.drawn(t + ms(61), t + ms(80));
        slot.drawn(t + ms(91), t + ms(100));
        assert_eq!(slot.held, Some(t + ms(111)));
        slot.drawn(t + ms(111), t + ms(120));
        assert_eq!(slot.held, Some(t + ms(131)));
    }

    // The sandbox's headless display says no refresh and draws at about
    // 62 Hz: a drawn pass does not hold the next past its next refresh.
    #[test]
    fn a_display_that_does_not_say_its_refresh_is_not_held_to_60_hz() {
        let t = Instant::now();
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.drawn(t, t + Duration::from_millis(3));
        assert_eq!(slot.held, Some(t + Duration::from_secs_f64(0.75 / 60.0)));
        assert!(slot.held < Some(t + Duration::from_secs_f64(1.0 / 62.0)));
    }

    // The loop let each pass go a millisecond after it was due: the passes
    // keep the display's rhythm, and do not slip a millisecond a frame
    // until one misses its refresh.
    #[test]
    fn passes_that_begin_a_little_late_keep_the_rhythm() {
        let t = Instant::now();
        let ms = Duration::from_millis;
        let mut slot = Slot::new(0, None, egui::Context::default(), None);
        slot.refresh = Some(50.0);
        slot.drawn(t, t + ms(5));
        assert_eq!(slot.held, Some(t + ms(20)));
        slot.drawn(t + ms(21), t + ms(26));
        assert_eq!(slot.held, Some(t + ms(40)));
        slot.drawn(t + ms(41), t + ms(46));
        assert_eq!(slot.held, Some(t + ms(60)));
        // One that ran early, asked for at once, starts a rhythm of its own.
        slot.drawn(t + ms(55), t + ms(57));
        assert_eq!(slot.held, Some(t + ms(75)));
    }
}
