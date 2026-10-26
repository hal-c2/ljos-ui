//! [`QmlView`]: a QuickScene shown inside an egui `Ui`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use egui::accesskit::{Action, TreeId};
use egui::{
    Context, Event, EventFilter, FocusDirection, Id, Key, Pos2, Rect, Response, Sense, TouchId,
    TouchPhase, Ui, Vec2,
};
use ljos_engine::{Engine, EngineError, ObjectId};
use ljos_quick::{
    Clipboard, Item, MouseButtons, PointerEvent, PointerId, PointerKind, PointerSource, QuickScene,
    TimerHook, WheelEvent,
};
use ljos_render::b0::{EguiSink, EpaintBackend};
use ljos_render::{FrameView, RenderBackend, kurbo};
use ljos_runtime::{Clock, MemoryPressure, SystemClock, Time};

use crate::a11y;
use crate::input::{WheelTrack, map_button, map_modifiers, map_wheel};
use crate::keys::{Fallback, KeyTrack};
use crate::text::{self, TextBridge};

/// A frame of a display whose refresh rate the host never told.
const SIXTY_HZ: Duration = Duration::from_micros(16_667);

/// How long a decoded picture waits for its pass. Pictures arriving
/// while something else draws (an animation, the user's hand) come with
/// that frame. Arriving on their own, they are shown together a few
/// frames apart and not each in a frame of its own: a window filling
/// with hundreds of pictures over a costly backdrop draws it a few
/// times, not a hundred.
const PICTURES_EVERY: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("cannot show the root item: {0}")]
    Root(#[from] EngineError),
}

/// Who decides the size: the same choice as `QQuickView::ResizeMode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeMode {
    /// The view takes all the space the `Ui` offers and writes it to the
    /// root's `width`/`height`.
    #[default]
    RootToView,
    /// The view allocates the root's `width`/`height`.
    ViewToRoot,
    /// The view takes all the space the `Ui` offers and leaves the root's
    /// size alone: for a root that is a window, whose host says how large
    /// the platform made it and keeps a binding on its size alive.
    Fill,
}

/// When the view takes the keys: what a Qt Quick window does by being the
/// window with focus has to be said inside egui, where widgets share one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardPolicy {
    /// The view has the keyboard whenever no egui widget does: for a view
    /// that is the window's content. Its items have active focus as soon as
    /// the window does, as in a Qt Quick window.
    #[default]
    WhenFree,
    /// The view has the keyboard only while it has egui's focus: after a
    /// click on it, a Tab to it or [`QmlView::request_keyboard`]. For a view
    /// that is one widget among others.
    WhenFocused,
}

/// The AccessKit updates of one pass of a view whose tree is grafted
/// ([`QmlView::graft_updates`]), in the order an adapter takes them.
#[derive(Debug)]
pub struct GraftUpdates {
    /// egui's tree, with the graft node in it.
    pub root: egui::accesskit::TreeUpdate,
    /// The scene's tree: all of it the first time, afterwards what
    /// changed. `None` when nothing did.
    pub grafted: Option<egui::accesskit::TreeUpdate>,
    /// egui's focus was on the graft node before its tree was there, and
    /// was told to be elsewhere: one more pass says where it is.
    pub again: bool,
}

/// Counters for tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewStats {
    /// `QuickScene::frame` calls.
    pub frames: u64,
    /// Frames whose snapshot differed from the previous one.
    pub changed: u64,
    /// Pointer events handed to the scene.
    pub pointer_events: u64,
    /// `request_repaint` / `request_repaint_after` calls made by the view.
    pub repaint_requests: u64,
    /// Accessibility nodes written in the last pass; 0 while nothing
    /// listens.
    pub accessible_nodes: usize,
    /// Press actions of an assistive technology that an item took.
    pub accessible_presses: u64,
    /// Focus actions of an assistive technology that an item took.
    pub accessible_focuses: u64,
    /// Other actions of an assistive technology (a step of a scroll bar,
    /// a scroll of a view) that an item took.
    pub accessible_actions: u64,
    /// Key events handed to the scene.
    pub key_events: u64,
    /// Key events the scene accepted, and so took out of egui's input.
    pub keys_accepted: u64,
    /// Text that came with no key press (what an input method composes
    /// and commits, a key egui has no name for) handed to the scene.
    pub text_events: u64,
    /// Text events an item took, and so taken out of egui's input.
    pub texts_accepted: u64,
    /// Wheel and touchpad scroll events handed to the scene.
    pub wheel_events: u64,
    /// Scroll events that moved something in the scene, and so are kept
    /// from the egui scroll areas around the view.
    pub wheels_accepted: u64,
}

/// Wakes a host that runs its own loop, after the given delay
/// ([`QmlView::set_waker`]). Any thread may call it.
pub type Waker = Arc<dyn Fn(Duration) + Send + Sync>;

/// Bridges the scene's waker (an `Rc` closure on the UI thread) to egui.
///
/// While the view itself is running the pipeline the waker is "held": the
/// change it reports will be consumed by the frame about to run, so asking
/// egui for another pass would only cost a wasted frame.
#[derive(Default)]
struct WakeGate {
    ctx: RefCell<Option<Context>>,
    waker: RefCell<Option<Waker>>,
    held: Cell<bool>,
    missed: Cell<bool>,
}

impl WakeGate {
    fn wake(&self) {
        if self.held.get() {
            self.missed.set(true);
        } else if let Some(waker) = &*self.waker.borrow() {
            waker(Duration::ZERO);
        } else if let Some(ctx) = &*self.ctx.borrow() {
            ctx.request_repaint();
        }
    }
}

/// Pointer bookkeeping between egui events.
#[derive(Default)]
struct PointerTrack {
    /// Buttons held since a press that landed in the view.
    held: MouseButtons,
    /// The pointer was last seen over the view (hover is live).
    inside: bool,
    /// The touches that came down on the view, the first first; the
    /// scene follows two.
    touches: Vec<TouchId>,
}

/// An egui widget that shows a [`QuickScene`] rooted at a QML item.
///
/// Every pass it (1) sizes the scene to its rect and pixels-per-point,
/// (2) feeds pointer input in item coordinates, (3) runs `scene.frame` only
/// if the scene is dirty or a timer is due, (4) paints the latest snapshot
/// through B0, and (5) asks egui for another pass only when there is future
/// work: `request_repaint_after` for the next timer, nothing when idle.
///
/// Keys go to the scene while the view has the keyboard (see
/// [`KeyboardPolicy`]); the ones the scene accepts are taken out of egui's
/// input, the others stay for the widgets shown after the view. The scene's
/// window is active while the view has the keyboard and the egui viewport
/// has the system's focus.
///
/// While egui builds an AccessKit tree (an assistive technology listens),
/// the items that carry `Accessible` are nodes of that tree under one node
/// for the view, a press action clicks the item and a focus action gives it
/// active focus. Otherwise the scene does no accessibility work.
pub struct QmlView {
    engine: Rc<Engine>,
    scene: QuickScene,
    backend: EpaintBackend,
    clock: Arc<dyn Clock>,
    id: Id,
    resize: ResizeMode,
    gate: Rc<WakeGate>,
    pointer: PointerTrack,
    wheel: WheelTrack,
    /// The scene used the last scroll event, so what egui still spreads
    /// of it over the coming passes is not for a scroll area around the
    /// view.
    wheel_kept: bool,
    keys: KeyTrack,
    keyboard: KeyboardPolicy,
    /// The keyboard was the view's when the last pass ended.
    had_keyboard: bool,
    next_wake: Option<Time>,
    /// The soonest pass the view asked for since [`QmlView::take_asked`],
    /// with a waker.
    asked: Option<Duration>,
    /// How long the display shows a frame ([`QmlView::set_refresh_rate`]).
    frame_period: Duration,
    size: Option<Vec2>,
    ppp: Option<f32>,
    painted_frame: Option<u64>,
    /// Passes paint; off while the window is hidden ([`QmlView::set_drawn`]).
    drawn: bool,
    /// Where the view is in preparing the scripts input may run.
    scripts: Scripts,
    /// See [`QmlView::set_fold_backdrop`].
    fold_backdrop: bool,
    backdrop: Option<egui::Color32>,
    /// See [`QmlView::set_solid_rects`].
    solid_rects: bool,
    /// See [`QmlView::set_draws_shaders`].
    draws_shaders: bool,
    /// See [`QmlView::set_encodes`].
    encodes: bool,
    emitted: a11y::Emitted,
    /// The text runs below the editors' nodes of the last pass.
    emitted_runs: a11y::TextRuns,
    /// The scene's accessibility tree is an AccessKit tree of its own
    /// with this id, grafted into egui's, and not nodes of egui's tree.
    graft: Option<TreeId>,
    /// The graft node was in egui's tree in the last pass.
    grafted: bool,
    /// The adapter was last told that egui's focus is on the graft node:
    /// the root of egui's tree, where the focus waits while the window's
    /// own focus changes ([`QmlView::release_graft_focus`]).
    graft_focus: Option<egui::accesskit::NodeId>,
    /// What egui said of the system's preferences when it was last asked.
    system: a11y::SystemFeed,
    text: TextBridge,
    stats: ViewStats,
    /// Files dragged over the view from outside, and where the scene was
    /// last told they are.
    file_drag: Option<(ljos_quick::PlatformDrag, Option<kurbo::Point>)>,
    /// Where egui last saw the pointer in the window, kept after it says
    /// the pointer is gone: where a file drag with no position is put.
    pointer_seen: Option<Pos2>,
    /// A drag from another application that the host follows itself
    /// ([`QmlView::window_drag`]), where the scene was last told it is, and
    /// the `Qt.DropAction` it answered.
    window_drag: Option<(ljos_quick::PlatformDrag, Option<kurbo::Point>, i32)>,
    /// Where in the window the scene was last shown: a host's drag comes in
    /// the window's points, between passes.
    shown_at: Option<Rect>,
}

/// One step of a drag from another application, as a window system that
/// says where it is tells it, at points of the window.
#[derive(Clone, Debug)]
pub enum WindowDrag {
    /// It came into the window, carrying `data`, with the actions its
    /// source allows and would like.
    Entered {
        data: ljos_quick::DragData,
        at: Pos2,
        actions: ljos_quick::DragActions,
    },
    /// It moved, or its source changed the actions it allows or would
    /// like.
    Moved {
        at: Pos2,
        actions: Option<ljos_quick::DragActions>,
    },
    Left,
    /// It was let go where it last was.
    Dropped,
}

/// [`Engine::prepare_scripts`], once per view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scripts {
    /// No frame was painted yet.
    Waiting,
    /// The first frame was painted in this egui frame; the first pass of
    /// another prepares them.
    Due(u64),
    Prepared,
}

impl std::fmt::Debug for QmlView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QmlView")
            .field("id", &self.id)
            .field("scene", &self.scene)
            .field("resize", &self.resize)
            .field("next_wake", &self.next_wake)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl QmlView {
    /// A view of `root`, which must be an `Item`.
    pub fn new(engine: Rc<Engine>, root: ObjectId) -> Result<Self, ViewError> {
        let mut scene = QuickScene::new(kurbo::Size::ZERO);
        scene.set_root(&engine, root)?;
        let gate = Rc::new(WakeGate::default());
        let g = Rc::clone(&gate);
        scene.set_waker(move || g.wake());
        Ok(Self {
            engine,
            scene,
            backend: EpaintBackend::new(),
            clock: Arc::new(SystemClock::new()),
            id: Id::unique(("ljos_qml_view", root)),
            resize: ResizeMode::default(),
            gate,
            pointer: PointerTrack::default(),
            wheel: WheelTrack::default(),
            wheel_kept: false,
            keys: KeyTrack::default(),
            keyboard: KeyboardPolicy::default(),
            had_keyboard: false,
            next_wake: None,
            frame_period: SIXTY_HZ,
            size: None,
            ppp: None,
            painted_frame: None,
            drawn: true,
            scripts: Scripts::Waiting,
            fold_backdrop: false,
            backdrop: None,
            solid_rects: false,
            draws_shaders: false,
            encodes: true,
            emitted: a11y::Emitted::new(),
            emitted_runs: a11y::TextRuns::new(),
            graft: None,
            grafted: false,
            graft_focus: None,
            system: a11y::SystemFeed::default(),
            text: TextBridge::default(),
            stats: ViewStats::default(),
            file_drag: None,
            pointer_seen: None,
            window_drag: None,
            shown_at: None,
            asked: None,
        })
    }

    /// The clock that stamps input and drives timers. Tests pass a
    /// `VirtualClock`; it must be the clock the timer hook reasons in.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// A stable id; the accessibility pass derives per-item ids from it.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_resize_mode(mut self, mode: ResizeMode) -> Self {
        self.resize = mode;
        self
    }

    pub fn resize_mode(&self) -> ResizeMode {
        self.resize
    }

    /// Changes who decides the size, for a view whose root was replaced
    /// by one of another kind. The size the view last had is forgotten, so
    /// the next pass sizes the root again if the new mode does that.
    pub fn set_resize_mode(&mut self, mode: ResizeMode) {
        if self.resize != mode {
            self.resize = mode;
            self.size = None;
        }
    }

    pub fn with_keyboard_policy(mut self, policy: KeyboardPolicy) -> Self {
        self.keyboard = policy;
        self
    }

    /// Keeps the scene's accessibility tree out of egui's: the view puts
    /// one graft node into egui's tree and the scene's tree is an
    /// AccessKit tree of its own, `tree`, that hangs there. For a host
    /// that owns the platform's adapter, which is the only one that can
    /// hand it a second tree: after each pass it sends egui's update and
    /// then [`QmlView::take_accessibility_update`]. What it gains is that
    /// only the nodes that changed are told, where egui's tree is told
    /// whole every pass.
    ///
    /// Without this the view writes its nodes into egui's tree, which any
    /// egui application shows as it is.
    pub fn with_accessibility_graft(mut self, tree: TreeId) -> Self {
        self.graft = Some(tree);
        self
    }

    /// The node of egui's tree the scene's tree hangs at, while it is
    /// there: after a pass in which something listened, of a view made
    /// [`QmlView::with_accessibility_graft`].
    ///
    /// AccessKit refuses the focus on this node before the grafted tree
    /// was sent once: a host keeps egui's update from saying so until
    /// then.
    pub fn accessibility_graft_node(&self) -> Option<egui::accesskit::NodeId> {
        (self.graft.is_some() && self.grafted).then(|| self.focus_id().accesskit_id())
    }

    /// What the grafted tree has to say since the last call: everything
    /// the first time, then the nodes that changed and where the focus
    /// is. `full` asks for everything again, for an adapter that has not
    /// been told (it was just activated). `None` when nothing changed,
    /// when nothing listens and for a view without a graft.
    ///
    /// Send it straight after egui's own update of the same pass: the
    /// graft node has to be there first.
    pub fn take_accessibility_update(&mut self, full: bool) -> Option<egui::accesskit::TreeUpdate> {
        let id = self.graft?;
        if !self.grafted {
            return None;
        }
        let tree = self.scene.accessibility_tree_mut();
        let mut update = if full {
            tree.full_update()
        } else {
            tree.take_update()?
        };
        update.tree_id = id;
        Some(update)
    }

    /// What a host that owns the adapter sends after a pass of a view
    /// with a graft: `root` is what egui said in that pass
    /// (`PlatformOutput::accesskit_update`), `told` whether this adapter
    /// was sent the grafted tree before; the host keeps it, and clears it
    /// when the adapter is activated anew.
    ///
    /// It keeps the two rules AccessKit has for a graft: the grafted tree
    /// follows the update that carries its graft node, and the focus is
    /// not on the graft node before the grafted tree was told.
    pub fn graft_updates(
        &mut self,
        mut root: egui::accesskit::TreeUpdate,
        told: &mut bool,
    ) -> GraftUpdates {
        let Some(node) = self.accessibility_graft_node() else {
            // Without the graft node an adapter drops the grafted tree.
            *told = false;
            self.graft_focus = None;
            return GraftUpdates {
                root,
                grafted: None,
                again: false,
            };
        };
        let mut again = false;
        if !*told
            && root.focus == node
            && let Some(tree) = &root.tree
        {
            // The pass after this one says where the focus is.
            root.focus = tree.root;
            again = true;
        }
        let grafted = self.take_accessibility_update(!*told);
        *told = true;
        self.graft_focus = match &root.tree {
            Some(tree) if root.focus == node => Some(tree.root),
            _ => None,
        };
        GraftUpdates {
            root,
            grafted,
            again,
        }
    }

    /// What a host that owns the adapter sends it before the adapter
    /// hears that its window gained or lost the focus; `None` when there
    /// is nothing to send. The host then asks for a pass, which says
    /// again where the focus is.
    ///
    /// The adapters up to accesskit_consumer 0.38 took the change of the
    /// window's focus for an update of egui's tree, and named the focused
    /// node by the id it has in its own tree. With the focus inside the
    /// grafted tree that id is none of egui's, and the adapter panicked.
    /// Those of egui 0.37 (consumer 0.39) do not; the release is kept for
    /// a host on an older adapter, and costs one update.
    pub fn release_graft_focus(&mut self) -> Option<egui::accesskit::TreeUpdate> {
        let root = self.graft_focus.take()?;
        Some(egui::accesskit::TreeUpdate {
            nodes: Vec::new(),
            tree: None,
            tree_id: TreeId::ROOT,
            focus: root,
        })
    }

    /// The id of the egui widget that holds egui's focus for the whole
    /// scene: which item has active focus is the scene's own business.
    pub fn focus_id(&self) -> Id {
        self.id.with("keyboard")
    }

    /// Gives the view egui's focus, and with it the keyboard.
    pub fn request_keyboard(&self, ctx: &Context) {
        ctx.memory_mut(|m| m.request_focus(self.focus_id()));
    }

    /// Whether the platform's input method was last told that an editor
    /// has focus.
    pub fn text_input_active(&self) -> bool {
        self.text.enabled()
    }

    /// Whether keys went to the scene when the last pass ended.
    pub fn has_keyboard(&self) -> bool {
        self.had_keyboard
    }

    pub fn engine(&self) -> &Rc<Engine> {
        &self.engine
    }

    pub fn scene(&self) -> &QuickScene {
        &self.scene
    }

    pub fn scene_mut(&mut self) -> &mut QuickScene {
        &mut self.scene
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn stats(&self) -> ViewStats {
        self.stats
    }

    /// The renderer, for what it holds (`image_textures`, `glyph_atlas`).
    pub fn backend(&self) -> &EpaintBackend {
        &self.backend
    }

    /// Gives back what the view can rebuild (ARCHITECTURE §8.5): the
    /// renderer's meshes, glyph atlas and textures, and the scene's
    /// copies of pictures. A host calls it when the platform reports
    /// memory pressure or the view's window is hidden; the next pass
    /// builds again what it draws.
    pub fn trim(&mut self, pressure: MemoryPressure) {
        self.backend.trim(pressure);
        self.scene.trim(&self.engine, pressure);
        // The renderer may have forgotten the nodes of the last snapshot.
        self.painted_frame = None;
    }

    /// Whether passes paint. A host turns it off for a window that is not
    /// on the screen, whose passes still run the scene: the renderer then
    /// builds nothing that [`QmlView::trim`] gave back until it is shown.
    pub fn set_drawn(&mut self, drawn: bool) {
        self.drawn = drawn;
    }

    /// Whether an opaque rectangle that is painted first and covers the
    /// whole window may be left out of the shapes and said by
    /// [`QmlView::backdrop`] instead. A host that clears its window to
    /// that colour then draws the same picture with one full-window pass
    /// less. Off by default: the host has to clear to it.
    pub fn set_fold_backdrop(&mut self, fold: bool) {
        self.fold_backdrop = fold;
    }

    /// Whether an opaque rectangle with square corners and no border that
    /// lies on whole pixels is handed to egui as a `Shape::Rect` rather
    /// than a mesh (`EguiSink::solid_rects`). For a host whose painter
    /// fills such a rectangle cheaper than it draws a mesh. Off by
    /// default.
    pub fn set_solid_rects(&mut self, on: bool) {
        self.solid_rects = on;
    }

    /// Whether a `ShaderEffect` with a program of its own is handed to
    /// egui as a paint callback holding a `ljos_render::b0::ShaderPaint`
    /// (`EguiSink::shaders`). For a host whose painter runs it; elsewhere
    /// the effect draws nothing, as in Qt's software scene graph. Off by
    /// default.
    pub fn set_draws_shaders(&mut self, on: bool) {
        self.draws_shaders = on;
    }

    /// Whether a pass hands the scene to egui to draw. Off for a host that
    /// draws [`QmlView::scene`]'s snapshot itself, such as a window drawn
    /// on the processor: the pass still runs the scene and its input. On
    /// by default.
    pub fn set_encodes(&mut self, on: bool) {
        self.encodes = on;
    }

    /// The colour the last pass left for the host to clear its window to
    /// (see [`QmlView::set_fold_backdrop`]).
    pub fn backdrop(&self) -> Option<egui::Color32> {
        self.backdrop
    }

    /// How long until the scene wants its next pass, as the last pass
    /// left it; `None` when it waits for nothing. A host that did not draw
    /// a pass waits for this, not for what it was asked: it is early by
    /// as much as a pass may be (see
    /// [`QmlView::set_refresh_rate`]), so that such a host asks for the
    /// display's frame in time.
    pub fn wakes_in(&self) -> Option<Duration> {
        let early = self.clock.now() + self.early();
        self.next_wake.map(|t| t.saturating_since(early))
    }

    /// The refresh rate of the display the view is on: an animation asks
    /// for one frame per refresh, and a frame gives its creation jobs a
    /// share of one. A host that never says is taken to show 60 frames a
    /// second.
    ///
    /// A pass less than half a frame before what the scene waits for runs
    /// it: a host that draws when the display asks for a frame then shows
    /// every frame of an animation, also on a display a little faster
    /// than it said, where waiting for the exact time would miss a frame
    /// every few dozen. Timers still fire no earlier than they are due.
    pub fn set_refresh_rate(&mut self, hz: f64) {
        if !(hz.is_finite() && hz > 0.0) {
            return;
        }
        self.frame_period = Duration::from_secs_f64(1.0 / hz);
        self.scene.set_refresh_rate(hz);
        ljos_quick::set_animation_frame_period(&self.engine, self.frame_period);
    }

    /// How much before its time a wake of the scene is taken as due.
    fn early(&self) -> Duration {
        self.frame_period / 2
    }

    /// The egui id of an item's accessibility node, if the item is exposed.
    /// It is made of the view's id and the item, so it is the same every
    /// pass. With [`QmlView::with_accessibility_graft`] the item is a node
    /// of the grafted tree instead, and its id there is its key
    /// (`QuickScene::accessible_node`).
    pub fn accessible_id(&self, item: ObjectId) -> Option<Id> {
        let key = self.scene.accessible_node(item)?;
        Some(a11y::node_id(self.id, key))
    }

    /// See [`TimerHook`]; times come from [`QmlView::clock`].
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.scene.set_timer_hook(hook);
        // The hook has not been asked yet; make sure the next pass does.
        self.next_wake = Some(Time::ZERO);
        self.gate.wake();
    }

    /// Lets changes made outside a pass (a message handler, a background
    /// result) wake egui. `show` does this too; call it early when the
    /// scene can change before the first pass.
    pub fn bind_context(&mut self, ctx: &Context) {
        *self.gate.ctx.borrow_mut() = Some(ctx.clone());
        if self.gate.waker.borrow().is_some() {
            return;
        }
        // A decoded picture is waiting: the worker thread wakes egui, and
        // the pass that follows hands the result to the items.
        let ctx = ctx.clone();
        ljos_quick::ImageStore::of(&self.engine)
            .set_waker(move || ctx.request_repaint_after(PICTURES_EVERY));
    }

    /// Wakes the host with `waker` and not through egui, for a host that
    /// runs its own loop. egui asks for passes of its own after each one
    /// with input, and once it has asked it hears no other request until
    /// the next pass: a host that leaves those passes out could miss a
    /// wake. With a waker, the passes the view asks for in a pass are not
    /// told to egui either: the host takes them ([`QmlView::take_asked`])
    /// after each pass.
    pub fn set_waker(&mut self, waker: Waker) {
        let pictures = Arc::clone(&waker);
        ljos_quick::ImageStore::of(&self.engine).set_waker(move || pictures(PICTURES_EVERY));
        *self.gate.waker.borrow_mut() = Some(waker);
    }

    /// How soon the view asked for its next pass in the passes since the
    /// last call, with a waker ([`QmlView::set_waker`]); `None` when it
    /// did not.
    pub fn take_asked(&mut self) -> Option<Duration> {
        self.asked.take()
    }

    /// Runs `f` with the scene's waker held, for work done right before a
    /// pass that will show its result anyway (the shell's inbox drain).
    pub fn without_wake<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let was = self.gate.held.replace(true);
        let r = f(self);
        self.gate.held.set(was);
        r
    }

    /// Shows the scene, filling the `Ui`'s available space (or the root's
    /// size with [`ResizeMode::ViewToRoot`]).
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        if self.gate.ctx.borrow().is_none() {
            self.bind_context(&ctx);
        }
        let desired = match self.resize {
            ResizeMode::RootToView | ResizeMode::Fill => ui.available_size(),
            ResizeMode::ViewToRoot => self.root_size(),
        };
        // Before the view's widget exists in this pass: what the scene
        // leaves of Tab and the arrows moves egui's focus when it does.
        self.deliver_keys(ui);
        // The widget's id does not depend on where in the `Ui` the view is,
        // so the keyboard stays when the layout around it changes.
        let (rect, space) = ui.allocate_exact_size(desired, Sense::hover());
        let mut response = ui.interact(space.rect, self.focus_id(), Sense::click_and_drag());
        response.set_intrinsic_size(desired);
        if response.is_pointer_button_down_on() && ui.input(|i| i.pointer.any_pressed()) {
            response.request_focus();
        }
        self.show_at(ui, rect);
        response
    }

    /// Hands the pass's key events to the scene, if the keyboard was the
    /// view's when they were typed.
    fn deliver_keys(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let id = self.focus_id();
        let focused = ctx.memory(|m| m.focused());
        let mine = focused == Some(id);
        // egui drops the focus for Escape before any widget runs, unless
        // the focused widget asked to keep it; the request only holds from
        // the second pass with focus on.
        let dropped = self.had_keyboard
            && focused.is_none()
            && (self.keyboard == KeyboardPolicy::WhenFree
                || ui.input(|i| {
                    i.events.iter().any(|e| {
                        matches!(e, Event::Key { key: Key::Escape, pressed: true, modifiers, .. }
                            if !modifiers.any())
                    })
                }));
        if !(mine || dropped) {
            self.keys.clear();
            if focused.is_none() && self.takes_free_keyboard(ui) {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            return;
        }
        ctx.memory_mut(|m| {
            if !mine {
                m.request_focus(id);
            }
            // The scene decides about Tab and the arrows first; egui has
            // already read them on its own in the first pass with focus.
            m.move_focus(FocusDirection::None);
        });
        let events = ui.input(|i| i.events.clone());
        let keys = self.keys.translate(&events, ctx.os());
        let texts = text::collect(&events, &keys);
        if keys.is_empty() && texts.is_empty() {
            return;
        }
        // The window may have got the keyboard in this very pass: what
        // came after that was typed into it.
        if ui.input(|i| i.focused) && !self.scene.is_window_active(&self.engine) {
            self.scene.set_window_active(&self.engine, true);
        }

        // The frame of this pass shows what the keys change.
        let held = self.gate.held.replace(true);
        let mut taken: Vec<usize> = Vec::new();
        let mut released: Vec<Key> = Vec::new();
        let mut left = None;
        // Keys and text reach the scene in the order they were typed.
        let mut texts = texts.into_iter().peekable();
        for k in &keys {
            let at = k.sources[0].unwrap_or(usize::MAX);
            while let Some((i, t)) = texts.next_if(|(i, _)| *i < at) {
                self.deliver_text(i, &t, &mut taken);
            }
            // The platform read its clipboard for the paste: an editor
            // that takes the chord pastes that.
            if let Some(Event::Paste(pasted)) = events.get(at) {
                Clipboard::of(&self.engine).set_text(&self.engine, pasted);
            }
            self.stats.key_events += 1;
            if self.scene.key_event(&self.engine, &k.event) {
                self.stats.keys_accepted += 1;
                taken.extend(k.sources.iter().flatten());
                released.extend(k.held);
            } else if k.fallback.is_some() {
                left = k.fallback;
            }
        }
        for (i, t) in texts {
            self.deliver_text(i, &t, &mut taken);
        }
        self.gate.held.set(held);

        if !taken.is_empty() {
            ui.input_mut(|i| {
                let mut at = 0;
                i.events.retain(|_| {
                    at += 1;
                    !taken.contains(&(at - 1))
                });
                for k in &released {
                    i.keys_down.remove(k);
                }
            });
        }
        match left {
            Some(Fallback::Move(direction)) => ctx.memory_mut(|m| m.move_focus(direction)),
            Some(Fallback::Release) if self.keyboard == KeyboardPolicy::WhenFocused => {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            _ => {}
        }
    }

    /// Hands text that is not a key press to the scene; `at` is its place
    /// in the pass's events, noted in `taken` when an item took it.
    fn deliver_text(&mut self, at: usize, what: &text::PlatformText, taken: &mut Vec<usize>) {
        self.stats.text_events += 1;
        if text::deliver(&mut self.scene, &self.engine, what) {
            self.stats.texts_accepted += 1;
            taken.push(at);
        }
    }

    /// Whether a keyboard no widget has is the view's. Not while egui keeps
    /// the view from input: under a modal layer or in a disabled `Ui`.
    fn takes_free_keyboard(&self, ui: &Ui) -> bool {
        self.keyboard == KeyboardPolicy::WhenFree
            && ui.is_enabled()
            && ui.ctx().memory(|m| m.allows_interaction(ui.layer_id()))
    }

    /// Makes the scene's window active exactly while keys reach it.
    fn sync_keyboard(&mut self, ui: &Ui) {
        let id = self.focus_id();
        let focused = ui.ctx().memory_mut(|m| {
            // From the second pass with focus on, egui leaves Tab, the
            // arrows and Escape to the view.
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
            m.focused()
        });
        // With nothing focused a view that takes the free keyboard gets it
        // back in the next pass, so its window does not blink inactive in
        // between (after a Tab nobody else wanted, for one).
        self.had_keyboard =
            focused == Some(id) || (focused.is_none() && self.takes_free_keyboard(ui));
        let active = self.had_keyboard && ui.input(|i| i.focused);
        if active != self.scene.is_window_active(&self.engine) {
            self.scene.set_window_active(&self.engine, active);
        }
    }

    fn root_size(&self) -> Vec2 {
        let Some(root) = self.scene.root() else {
            return Vec2::ZERO;
        };
        let w = self.engine.get(root, Item::WIDTH).unwrap_or(0.0);
        let h = self.engine.get(root, Item::HEIGHT).unwrap_or(0.0);
        Vec2::new(w as f32, h as f32)
    }

    /// Lays the scene out at `size` points and `pixels_per_point` before
    /// it is first shown, as the first pass would. A host that waits for
    /// its painter to start does it meanwhile; the first pass then only
    /// draws, unless its size or scale turns out to be another. Nothing
    /// once the scene has a frame.
    pub fn lay_out_ahead(&mut self, size: Vec2, pixels_per_point: f32) {
        if self.scene.snapshot().is_some() {
            return;
        }
        let was = self.gate.held.replace(true);
        let size = match self.resize {
            ResizeMode::RootToView | ResizeMode::Fill => size,
            ResizeMode::ViewToRoot => self.root_size(),
        };
        self.fit(size, pixels_per_point);
        self.run_frame(self.clock.now());
        self.gate.held.set(was);
    }

    /// The scene's viewport is `size` points at `ppp` pixels a point.
    fn fit(&mut self, size: Vec2, ppp: f32) {
        if self.ppp != Some(ppp) {
            self.ppp = Some(ppp);
            // Text is laid out in logical pixels with unhinted metrics, so a
            // new scale changes how it is painted and not where it breaks.
            self.scene.set_scale_factor(ppp);
        }
        if self.size != Some(size) {
            self.size = Some(size);
            self.scene.set_viewport(kurbo::Rect::new(
                0.0,
                0.0,
                f64::from(size.x),
                f64::from(size.y),
            ));
            if self.resize == ResizeMode::RootToView
                && let Some(root) = self.scene.root()
            {
                // Like Qt's SizeRootObjectToView this is an imperative
                // write; it replaces a binding on the root's size.
                let _ = self.engine.set(root, Item::WIDTH, f64::from(size.x));
                let _ = self.engine.set(root, Item::HEIGHT, f64::from(size.y));
            }
        }
    }

    /// Pipeline steps 3-9.
    fn run_frame(&mut self, now: Time) {
        let out = self.scene.frame(&self.engine, now);
        self.stats.frames += 1;
        if out.changed {
            self.stats.changed += 1;
        }
        self.next_wake = out.next_wake;
    }

    fn show_at(&mut self, ui: &mut Ui, rect: Rect) {
        let ctx = ui.ctx().clone();
        // In the pass after the first frame, so that frame is not kept
        // waiting, and before input, so the first click or key runs its
        // handler and not the start of the JS host too.
        // egui may run passes again within a frame and show only the last,
        // so it is the next frame and not the next pass.
        if let Scripts::Due(painted) = self.scripts
            && ctx.cumulative_frame_nr() != painted
        {
            self.scripts = Scripts::Prepared;
            self.engine.prepare_scripts();
        }
        self.gate.held.set(true);
        self.gate.missed.set(false);

        // Before the frame, so that it is laid out and painted with them.
        self.system.feed(&ctx, &self.engine);

        // The accessibility tree exists only while something reads it.
        // Switched here, so the frame of this pass builds it.
        let listening = a11y::listening(ui);
        if listening != self.scene.accessibility_active() {
            self.scene.set_accessibility_active(&self.engine, listening);
        }

        self.shown_at = Some(rect);

        // 1. Size and scale.
        let ppp = ctx.pixels_per_point();
        self.fit(rect.size(), ppp);

        // 2. Input, against the snapshot the user is looking at.
        let now = self.clock.now();
        self.deliver_input(ui, rect, now);
        self.deliver_files(ui, rect);
        self.sync_keyboard(ui);

        // 3-9. The pipeline, only when something asked for it.
        let timer_due = self.next_wake.is_some_and(|t| t <= now + self.early());
        if self.scene.needs_frame() || timer_due || self.scene.snapshot().is_none() {
            self.run_frame(now);
        }
        self.gate.held.set(false);

        // The input method follows the editor with focus; the frame has
        // just said where its cursor is.
        self.text.sync(&self.scene, &self.engine, ui, rect);
        // What an editor copied goes to the platform's clipboard.
        if let Some(copied) = Clipboard::of(&self.engine).take_outgoing() {
            ctx.copy_text(copied);
        }

        // Paint. egui keeps no retained shapes, so every pass re-emits the
        // snapshot; B0 caches meshes per node, so this is mostly Arc clones.
        self.backdrop = None;
        if self.drawn && !self.encodes && self.scene.snapshot().is_some() {
            if self.scripts == Scripts::Waiting {
                self.scripts = Scripts::Due(ctx.cumulative_frame_nr());
            }
        } else if self.drawn
            && let Some(snap) = self.scene.snapshot().cloned()
        {
            let view = FrameView::new(&snap, ppp);
            if self.painted_frame != Some(snap.frame) {
                self.backend.sync(&view);
                self.painted_frame = Some(snap.frame);
            }
            let painter = ui.painter_at(rect);
            let mut sink = EguiSink::new(Some(&ctx), rect.min);
            // Only a scene that is the whole window can be its background.
            let window = ctx.viewport_rect();
            let scene = Rect::from_min_size(
                rect.min,
                Vec2::new(snap.viewport.width() as f32, snap.viewport.height() as f32),
            );
            sink.fold_backdrop = self.fold_backdrop
                && scene.contains_rect(window)
                && painter.clip_rect().contains_rect(window);
            sink.solid_rects = self.solid_rects;
            sink.shaders = self.draws_shaders;
            self.backend.encode(&view, &mut sink);
            sink.paint(&painter);
            self.backdrop = sink.backdrop;
            if self.scripts == Scripts::Waiting {
                self.scripts = Scripts::Due(ctx.cumulative_frame_nr());
            }
        }

        // Accessibility. Like the shapes, egui keeps no nodes between
        // passes, so they are repeated while something listens.
        self.grafted = false;
        match self.graft {
            Some(tree) if listening => {
                // One node, the same every pass: the scene's tree is told
                // to the adapter by the host, and only what changed of it.
                a11y::graft(&ctx, self.focus_id(), rect, tree);
                self.grafted = true;
                self.stats.accessible_nodes = self.scene.accessibility_tree().len();
            }
            None if listening => {
                let tree = self.scene.accessibility_tree();
                let holder = self.focus_id();
                let runs = &mut self.emitted_runs;
                a11y::emit(ui, self.id, holder, rect, tree, &mut self.emitted, runs);
                self.stats.accessible_nodes = self.emitted.len();
            }
            _ => {
                self.emitted.clear();
                self.emitted_runs.clear();
                self.stats.accessible_nodes = 0;
            }
        }

        // Schedule the next pass.
        let missed = self.gate.missed.replace(false) && self.scene.needs_frame();
        match self.next_wake {
            _ if missed || matches!(self.scripts, Scripts::Due(_)) => {
                self.request_repaint(&ctx, None)
            }
            Some(t) if t <= now => self.request_repaint(&ctx, None),
            // egui forgets a delayed repaint after each pass, so ask again
            // every pass until the timer fires.
            Some(t) => self.request_repaint(&ctx, Some(t - now)),
            None => {}
        }
    }

    fn request_repaint(&mut self, ctx: &Context, after: Option<Duration>) {
        self.stats.repaint_requests += 1;
        if self.gate.waker.borrow().is_some() {
            let after = after.unwrap_or(Duration::ZERO);
            self.asked = Some(self.asked.map_or(after, |asked| asked.min(after)));
            return;
        }
        match after {
            None => ctx.request_repaint(),
            Some(d) => ctx.request_repaint_after(d),
        }
    }

    /// egui puts the arrow back every pass, so the scene's cursor is said
    /// again in each one the pointer is over the view or held.
    fn show_cursor(&self, ctx: &Context) {
        if self.pointer.inside || !self.pointer.held.is_empty() {
            ctx.set_cursor_icon(cursor_icon(self.scene.cursor_shape()));
        }
    }

    fn deliver_input(&mut self, ui: &Ui, rect: Rect, now: Time) {
        let ctx = ui.ctx();
        let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
        if events.is_empty() {
            self.show_cursor(ctx);
            self.keep_wheel(ui);
            return;
        }
        let os = ctx.os();
        let mods = map_modifiers(modifiers, os);
        let clip = ui.clip_rect();
        let layer = ui.layer_id();
        // A press or hover only belongs to the view when nothing egui drew
        // on top (a window, a popup) covers that point.
        let over = |p: Pos2| {
            rect.contains(p) && clip.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l == layer)
        };
        let local =
            |p: Pos2| kurbo::Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y));
        let base = |kind, p: Pos2| PointerEvent {
            kind,
            pos: local(p),
            button: None,
            buttons: MouseButtons::empty(),
            modifiers: mods,
            id: PointerId::default(),
            source: PointerSource::MOUSE,
            time: now,
        };

        // egui-winit reports a touch as `Touch` and also emulates the first
        // one as mouse events; once a touch is seen, the emulation is noise.
        let touch_pass = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let mut out: Vec<PointerEvent> = Vec::new();
        // The touch points of a pass go together: a pinch's step is both
        // fingers' moves.
        let mut touches = Vec::new();
        let mut presses = Vec::new();
        let mut focuses = Vec::new();
        let mut actions = Vec::new();
        let mut text_actions = Vec::new();
        for e in &events {
            match e {
                Event::AccessKitActionRequest(req) => {
                    // egui's tree names a node by the id the view gave it;
                    // a grafted tree by the scene's own key.
                    let tree = self.scene.accessibility_tree();
                    let key = match self.graft {
                        None if req.target_tree == TreeId::ROOT => {
                            self.emitted.get(&req.target_node).copied()
                        }
                        Some(id) if req.target_tree == id => {
                            a11y::grafted_key(tree, req.target_node)
                        }
                        _ => None,
                    };
                    let Some(key) = key else {
                        continue;
                    };
                    if req.action == Action::Click {
                        presses.push(key);
                    }
                    if req.action == Action::Focus {
                        focuses.push(key);
                    }
                    if let Some(action) = a11y::item_action(req) {
                        actions.push((key, action));
                    }
                    let runs = &self.emitted_runs;
                    let action = match self.graft {
                        None => a11y::text_action(req, key, tree, |id| runs.get(&id).copied()),
                        Some(_) => a11y::text_action(req, key, tree, |id| tree.text_run(id)),
                    };
                    if let Some(action) = action {
                        text_actions.push((key, action));
                    }
                }
                Event::Touch { id, phase, pos, .. } => {
                    use ljos_quick::touch::{TouchEvent, TouchPhase as Phase};
                    let mine = self.pointer.touches.contains(id);
                    let phase = match phase {
                        // The fingers after the first belong to it.
                        TouchPhase::Start
                            if !mine && (!self.pointer.touches.is_empty() || over(*pos)) =>
                        {
                            self.pointer.touches.push(*id);
                            Phase::Begin
                        }
                        TouchPhase::Move if mine => Phase::Move,
                        TouchPhase::End | TouchPhase::Cancel if mine => {
                            self.pointer.touches.retain(|t| t != id);
                            if *phase == TouchPhase::End {
                                Phase::End
                            } else {
                                Phase::Cancel
                            }
                        }
                        _ => continue,
                    };
                    touches.push(TouchEvent {
                        id: PointerId(id.0.wrapping_add(1)),
                        phase,
                        pos: local(*pos),
                        time: now,
                    });
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    phase,
                    source,
                    modifiers,
                } => {
                    let scroll =
                        map_wheel(&mut self.wheel, *unit, *delta, *phase, *source, rect.size());
                    // The event has no position of its own: it is for
                    // what the pointer is over.
                    let Some(p) = ui.input(|i| i.pointer.latest_pos()).filter(|p| over(*p)) else {
                        continue;
                    };
                    // What came before it has to have happened first.
                    for ev in out.drain(..) {
                        self.scene.pointer_event(&self.engine, &ev);
                        self.stats.pointer_events += 1;
                    }
                    if !touches.is_empty() {
                        self.scene.touch_event(&self.engine, &touches);
                        self.stats.pointer_events += touches.len() as u64;
                        touches.clear();
                    }
                    let ev = WheelEvent {
                        pos: local(p),
                        angle_delta: scroll.angle_delta,
                        pixel_delta: scroll.pixel_delta,
                        phase: scroll.phase,
                        modifiers: map_modifiers(*modifiers, os),
                        time: now,
                    };
                    self.stats.wheel_events += 1;
                    self.wheel_kept = self.scene.wheel_event(&self.engine, &ev);
                    if self.wheel_kept {
                        self.stats.wheels_accepted += 1;
                    }
                }
                _ if touch_pass || !self.pointer.touches.is_empty() => {}
                Event::PointerMoved(p) => {
                    if !self.pointer.held.is_empty() || over(*p) {
                        self.pointer.inside = over(*p);
                        let mut m = base(PointerKind::Move, *p);
                        m.buttons = self.pointer.held;
                        out.push(m);
                    } else if self.pointer.inside {
                        self.pointer.inside = false;
                        out.push(base(PointerKind::Leave, *p));
                    }
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let b = map_button(*button);
                    let mut ev = base(PointerKind::Press, *pos);
                    ev.modifiers = map_modifiers(*modifiers, os);
                    ev.button = Some(b);
                    if *pressed {
                        // Extra buttons follow the grab; a fresh press must
                        // land on the view.
                        if self.pointer.held.is_empty() && !over(*pos) {
                            continue;
                        }
                        self.pointer.held |= b;
                        self.pointer.inside = true;
                        ev.buttons = self.pointer.held;
                        out.push(ev);
                    } else if self.pointer.held.contains(b) {
                        self.pointer.held.remove(b);
                        ev.kind = PointerKind::Release;
                        ev.buttons = self.pointer.held;
                        out.push(ev);
                        if self.pointer.held.is_empty() && !over(*pos) {
                            self.pointer.inside = false;
                            out.push(base(PointerKind::Leave, *pos));
                        }
                    }
                }
                Event::PointerGone => {
                    // While a button is held the platform keeps the pointer
                    // captured; hover ends at the release instead.
                    if self.pointer.held.is_empty() && self.pointer.inside {
                        self.pointer.inside = false;
                        out.push(base(PointerKind::Leave, Pos2::ZERO));
                    }
                }
                Event::WindowFocused(false) if !self.pointer.held.is_empty() => {
                    self.pointer.held = MouseButtons::empty();
                    self.pointer.inside = false;
                    out.push(base(PointerKind::Cancel, Pos2::ZERO));
                }
                _ => {}
            }
        }
        if !touches.is_empty() {
            self.scene.touch_event(&self.engine, &touches);
        }
        for ev in &out {
            self.scene.pointer_event(&self.engine, ev);
        }
        self.stats.pointer_events += (out.len() + touches.len()) as u64;
        self.show_cursor(ctx);
        self.keep_wheel(ui);
        for key in focuses {
            if self.scene.accessible_focus(&self.engine, key) {
                self.stats.accessible_focuses += 1;
                // Active focus needs an active window, and the window is
                // active while the view has egui's focus.
                ctx.memory_mut(|m| m.request_focus(self.focus_id()));
            }
        }
        for (key, action) in actions {
            if self.scene.accessible_action(&self.engine, key, action) {
                self.stats.accessible_actions += 1;
            }
        }
        for (key, action) in text_actions {
            if self.scene.accessible_text_action(&self.engine, key, action) {
                self.stats.accessible_actions += 1;
            }
        }
        // The press is a click of its own on the scene's one pointer; in the
        // middle of a real press it would break that press's grab.
        if self.pointer.held.is_empty() && self.pointer.touches.is_empty() {
            for key in presses {
                if self.scene.accessible_press(&self.engine, key, now) {
                    self.stats.accessible_presses += 1;
                }
            }
        }
    }
}

impl QmlView {
    /// Files dragged over the window from another application go to the
    /// scene's DropAreas, as a drag with no source whose key is
    /// `text/uri-list`. egui says only that files hover and that they were
    /// dropped, not where: the drag is where the pointer is.
    ///
    /// winit 0.30 gives a file drag no position and moves no pointer while
    /// it lasts, so a drag that comes in from another window finds egui
    /// with no pointer at all. Such a drag is where the pointer was last
    /// seen in the view, else at the view's centre, and is still dropped
    /// there; dropped on nothing that takes it, it is logged.
    ///
    /// This is the one way a file drop reaches the scene. Every host egui
    /// runs in reports the window system's file events here (egui-winit
    /// for the shell), per window, and only the view knows where its scene
    /// is in the window and which tree it shows.
    fn deliver_files(&mut self, ui: &Ui, rect: Rect) {
        use ljos_quick::{
            DragData, PlatformDrag, PlatformDragKind, platform_drag, platform_drag_target,
        };
        let (hovered, dropped, pos) = ui.input(|i| {
            (
                i.raw.hovered_files.clone(),
                i.raw.dropped_files.clone(),
                i.pointer.latest_pos(),
            )
        });
        if pos.is_some() {
            self.pointer_seen = pos;
        }
        if hovered.is_empty() && dropped.is_empty() && self.file_drag.is_none() {
            return;
        }
        let guessed = pos.is_none();
        let at = match pos {
            Some(p) => Some(p).filter(|p| rect.contains(*p)),
            None => self
                .pointer_seen
                .filter(|p| rect.contains(*p))
                .or_else(|| rect.is_positive().then(|| rect.center())),
        }
        .map(|p| kurbo::Point::new(f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)));
        let root = self.scene.root();
        let engine = &self.engine;
        let start = |files: Vec<std::path::PathBuf>| {
            let drag = PlatformDrag::new(DragData::files(files));
            match root {
                Some(root) => drag.over(engine, root),
                None => drag,
            }
        };
        if !dropped.is_empty() {
            let (drag, told) = match self.file_drag.take() {
                Some(d) => d,
                None => (
                    start(dropped.iter().map(|f| f.path().to_path_buf()).collect()),
                    None,
                ),
            };
            // A drop where the scene was not told the drag is first comes
            // there, as `platform_drag` would do itself, so that what will
            // take it is known before it is let go.
            if let Some(at) = at.filter(|at| told != Some(*at)) {
                platform_drag(engine, &drag, &PlatformDragKind::Move(at));
            }
            if guessed && (at.is_none() || platform_drag_target(engine).is_none()) {
                engine.log(
                    ljos_engine::LogLevel::Warning,
                    "qt.quick.dnd",
                    format!(
                        "{} dropped file(s) taken by no DropArea: the window system \
                         said not where they were dropped, and they were put at {}",
                        dropped.len(),
                        at.map_or_else(
                            || "no place in the view".to_owned(),
                            |p| format!("{},{}", p.x, p.y)
                        ),
                    ),
                );
            }
            let kind = at.map_or(PlatformDragKind::Leave, PlatformDragKind::Drop);
            platform_drag(engine, &drag, &kind);
        } else if !hovered.is_empty() {
            let (drag, told) = self.file_drag.get_or_insert_with(|| {
                (
                    start(hovered.iter().filter_map(|f| f.path.clone()).collect()),
                    None,
                )
            });
            // egui keeps the hovered files in every pass while the drag
            // lasts; the scene hears of the drag only when it went somewhere.
            if *told != at {
                *told = at;
                let kind = at.map_or(PlatformDragKind::Leave, PlatformDragKind::Move);
                platform_drag(engine, drag, &kind);
            }
        } else if let Some((drag, _)) = self.file_drag.take() {
            platform_drag(engine, &drag, &PlatformDragKind::Leave);
        }
    }

    /// The window system took the pointer, as it does for a drag it
    /// carries out of the window: a press held here is cancelled, as Qt
    /// cancels the grab of what started the drag. Whether one was held;
    /// the host asks for a pass after it.
    pub fn pointer_taken(&mut self) -> bool {
        if self.pointer.held.is_empty() {
            return false;
        }
        self.pointer.held = MouseButtons::empty();
        self.pointer.inside = false;
        let now = self.clock.now();
        self.scene
            .pointer_event(&self.engine, &PointerEvent::cancel(now));
        true
    }

    /// Tells the scene of a step of a drag from another application, for
    /// a host that follows such drags itself rather than through egui,
    /// which knows only of files. Says which action a `DropArea` under the
    /// drag takes it with, or, for [`WindowDrag::Dropped`], took the drop
    /// with (`None` when none does): what the host answers the window
    /// system.
    ///
    /// Outside the view's place in the window the drag is outside the
    /// scene. It runs between passes; the host asks for one after it.
    pub fn window_drag(&mut self, step: WindowDrag) -> Option<ljos_quick::DropAction> {
        use ljos_quick::DropAction;
        use ljos_quick::{IGNORE_ACTION, PlatformDrag, PlatformDragKind, platform_drag};
        let engine = &self.engine;
        let rect = self.shown_at;
        let inside = |p: Pos2| {
            rect.filter(|r| r.contains(p))
                .map(|r| kurbo::Point::new(f64::from(p.x - r.min.x), f64::from(p.y - r.min.y)))
        };
        let (at, changed) = match step {
            WindowDrag::Entered { data, at, actions } => {
                if let Some((drag, ..)) = self.window_drag.take() {
                    platform_drag(engine, &drag, &PlatformDragKind::Leave);
                }
                let drag = PlatformDrag::new(data).with_actions(actions);
                let drag = match self.scene.root() {
                    Some(root) => drag.over(engine, root),
                    None => drag,
                };
                self.window_drag = Some((drag, None, IGNORE_ACTION));
                (at, false)
            }
            WindowDrag::Moved { at, actions } => {
                let changed = match (self.window_drag.as_mut(), actions) {
                    (Some((drag, ..)), Some(actions)) => {
                        let before = (drag.supported_actions, drag.proposed_action);
                        drag.set_actions(actions);
                        before != (drag.supported_actions, drag.proposed_action)
                    }
                    _ => false,
                };
                (at, changed)
            }
            WindowDrag::Left => {
                if let Some((drag, ..)) = self.window_drag.take() {
                    platform_drag(engine, &drag, &PlatformDragKind::Leave);
                }
                return None;
            }
            WindowDrag::Dropped => {
                let (drag, told, _) = self.window_drag.take()?;
                return match told {
                    Some(at) => DropAction::from_qt(platform_drag(
                        engine,
                        &drag,
                        &PlatformDragKind::Drop(at),
                    )),
                    None => {
                        platform_drag(engine, &drag, &PlatformDragKind::Leave);
                        None
                    }
                };
            }
        };
        let (drag, told, action) = self.window_drag.as_mut()?;
        let at = inside(at);
        // A source that proposes another action where the drag is asks
        // the area again.
        if *told != at || changed {
            *told = at;
            *action = match at {
                Some(at) => platform_drag(engine, drag, &PlatformDragKind::Move(at)),
                None => platform_drag(engine, drag, &PlatformDragKind::Leave),
            };
        }
        DropAction::from_qt(*action)
    }

    /// Keeps a scroll the scene used from also scrolling an egui scroll
    /// area around the view. egui spreads one turn of the wheel over
    /// several passes and a scroll area reads it when its content has been
    /// shown, so the view takes it away in each of those passes.
    fn keep_wheel(&mut self, ui: &Ui) {
        if !self.wheel_kept {
            return;
        }
        if ui.input(|i| i.is_scrolling() || i.smooth_scroll_delta != Vec2::ZERO) {
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        } else {
            self.wheel_kept = false;
        }
    }
}

impl egui::Widget for &mut QmlView {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui)
    }
}

/// The egui cursor for a `Qt.CursorShape`; the arrow for one it has not.
fn cursor_icon(shape: i32) -> egui::CursorIcon {
    use egui::CursorIcon as C;
    match shape {
        2 => C::Crosshair,
        3 => C::Wait,
        4 => C::Text,
        5 => C::ResizeVertical,
        6 => C::ResizeHorizontal,
        7 => C::ResizeNeSw,
        8 => C::ResizeNwSe,
        9 | 20 => C::Move,
        10 => C::None,
        11 => C::ResizeRow,
        12 => C::ResizeColumn,
        13 => C::PointingHand,
        14 => C::NotAllowed,
        15 => C::Help,
        16 => C::Progress,
        17 => C::Grab,
        18 => C::Grabbing,
        19 => C::Copy,
        21 => C::Alias,
        _ => C::Default,
    }
}
