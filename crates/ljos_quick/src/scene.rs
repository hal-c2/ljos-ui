//! `QuickScene`: an item tree mirrored into a [`RenderTree`].
//!
//! The scene adopts a root item and everything under it. For each item it
//! installs a few small effects that record what changed (structure,
//! geometry, anchors, inherited visibility, polish inputs, paint inputs)
//! into shared dirty sets and wake the host on the clean-to-dirty edge.
//! Nothing polls: [`QuickScene::frame`] only drains those sets
//! (ARCHITECTURE §6, steps 3-9), and an idle frame returns the previous
//! snapshot with `next_wake: None`.
//!
//! Paint inputs are pulled only for items in the composed snapshot: an
//! item that is hidden or outside the viewport never evaluates its
//! `color` binding.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{
    Engine, EngineError, LogLevel, NodeId as GraphNode, ObjectId, OverrideContent, OverrideId,
    Point, PropIndex, Provenance, RestoreMode, Value,
};
use ljos_render::{ClipSpec, FrameSnapshot, NodeId, RenderTree, hit_test, kurbo};
use ljos_runtime::{FrameBudget, Scheduler, Time};

use crate::accessible::A11yPass;
use crate::anchors::{self, AnchorState};
use crate::enums::gesture_policy;
use crate::input::{MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerKind, TapHandler};
use crate::item::{
    Item, compose_transform, init_item, is_a, is_item, num, scene_transform, write_computed,
};
use crate::loader::{Loader, shown_children};
#[allow(deprecated)]
use crate::log::QuickLog;
use crate::positioners::{self, Axis, Column, Row};
use crate::rectangle::{Rectangle, RectanglePaint};
use crate::text::{self, SharedTextEngine, Text, TextInputs};
use crate::tree::resources;

/// Log category for polish warnings.
pub const POLISH_CATEGORY: &str = "qt.quick.polish";
/// Log category for scene errors (failed effects, bad trees).
pub const SCENE_CATEGORY: &str = "qt.quick.scene";
/// Polish passes per frame before the scene gives up and warns.
pub const MAX_POLISH_PASSES: usize = 10;
/// Time per frame for making the items of asynchronous Loaders
/// (ARCHITECTURE §6.3).
pub const INCUBATION_BUDGET: Duration = Duration::from_millis(3);
/// Qt's default `QStyleHints::mouseDoubleClickInterval`.
pub const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);
/// Qt's default `QStyleHints::startDragDistance`, in logical pixels.
pub const DRAG_THRESHOLD: f64 = 10.0;

/// Extra host work for pipeline step 3: it advances whatever the host
/// animates itself at `now` and returns when it next needs a frame.
///
/// QML `Timer`s and `Qt.callLater` do not need one: [`QuickScene::frame`]
/// runs the engine's timers itself and folds their deadline into
/// [`FrameOutput::next_wake`].
pub type TimerHook = Box<dyn FnMut(&Engine, Time) -> Option<Time>>;

/// What [`QuickScene::frame`] produced.
#[derive(Clone, Debug)]
pub struct FrameOutput {
    pub snapshot: Arc<FrameSnapshot>,
    /// False when `snapshot` is the previous frame's (same `Arc`).
    pub changed: bool,
    /// When the scene next needs a frame; `None` when it is idle.
    pub next_wake: Option<Time>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Item,
    Rectangle,
    Text,
    /// An `Image` or `BorderImage`.
    Image,
    Row,
    Column,
    /// A layout, `Grid` or `Flow`: polished by `crate::layouts`.
    Managed,
    /// A `TextInput` or `TextEdit`: lays itself out, the scene paints it.
    Editor,
}

impl Kind {
    fn of(engine: &Engine, obj: ObjectId) -> Kind {
        if is_a::<crate::text_input::TextEditor>(engine, obj) {
            Kind::Editor
        } else if is_a::<Text>(engine, obj) {
            Kind::Text
        } else if is_a::<Rectangle>(engine, obj) {
            Kind::Rectangle
        } else if is_a::<crate::image::ImageBase>(engine, obj) {
            Kind::Image
        } else if is_a::<Row>(engine, obj) {
            Kind::Row
        } else if is_a::<Column>(engine, obj) {
            Kind::Column
        } else if crate::layouts::managed(engine, obj) {
            Kind::Managed
        } else {
            Kind::Item
        }
    }

    fn paints(self) -> bool {
        matches!(
            self,
            Kind::Rectangle | Kind::Text | Kind::Image | Kind::Editor
        )
    }

    fn polishes(self) -> bool {
        matches!(self, Kind::Text | Kind::Row | Kind::Column | Kind::Managed)
    }
}

/// The geometry of one item, as its render node needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Geom {
    x: f64,
    y: f64,
    z: f64,
    w: f64,
    h: f64,
    visible: bool,
    opacity: f64,
    clip: bool,
    rotation: f64,
    scale: f64,
    origin: i32,
}

impl Geom {
    fn read(e: &Engine, obj: ObjectId) -> Geom {
        Geom {
            x: num(e, obj, Item::X.index),
            y: num(e, obj, Item::Y.index),
            z: num(e, obj, Item::Z.index),
            w: num(e, obj, Item::WIDTH.index),
            h: num(e, obj, Item::HEIGHT.index),
            visible: e.read(obj, Item::VISIBLE.index).to_boolean(),
            opacity: num(e, obj, Item::OPACITY.index),
            clip: e.read(obj, Item::CLIP.index).to_boolean(),
            rotation: num(e, obj, Item::ROTATION.index),
            scale: num(e, obj, Item::SCALE.index),
            origin: num(e, obj, Item::TRANSFORM_ORIGIN.index) as i32,
        }
    }
}

#[derive(Clone, Debug)]
enum Paint {
    Rect(RectanglePaint),
    Text(TextInputs),
    Image(crate::image::ImagePaint),
    Editor(crate::text_input::EditorPaint),
}

/// Dirty state written by effects and drained by `frame`.
#[derive(Default)]
struct Shared {
    geom: HashMap<ObjectId, Geom>,
    content: HashMap<ObjectId, Paint>,
    structure: HashSet<ObjectId>,
    polish: HashSet<ObjectId>,
    dead: Vec<ObjectId>,
    /// Set from the first mark until the frame that drains it.
    dirty: bool,
    waker: Option<Rc<dyn Fn()>>,
}

impl Shared {
    fn pending(&self) -> bool {
        !(self.geom.is_empty()
            && self.content.is_empty()
            && self.structure.is_empty()
            && self.polish.is_empty()
            && self.dead.is_empty())
    }
}

type SharedRc = Rc<RefCell<Shared>>;

/// Records a change; calls the waker on the clean-to-dirty edge only.
fn mark(shared: &SharedRc, f: impl FnOnce(&mut Shared)) {
    let waker = {
        let mut s = shared.borrow_mut();
        f(&mut s);
        if std::mem::replace(&mut s.dirty, true) {
            None
        } else {
            s.waker.clone()
        }
    };
    // Called unborrowed: the host may call back into the scene.
    if let Some(w) = waker {
        w();
    }
}

/// The override layers that make `visible`/`enabled` read false under a
/// hidden or disabled ancestor.
#[derive(Debug, Default)]
struct Inherit {
    visible: Option<OverrideId>,
    enabled: Option<OverrideId>,
}

struct Entry {
    node: NodeId,
    kind: Kind,
    /// The item this one was adopted under; `None` for the root.
    parent: Option<ObjectId>,
    effects: Vec<GraphNode>,
    content_effect: Option<GraphNode>,
    anchors: Rc<RefCell<AnchorState>>,
    inherit: Rc<RefCell<Inherit>>,
    size: kurbo::Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grab {
    Mouse(ObjectId),
    Tap(ObjectId),
    /// An item another crate registered ([`crate::register_pointer_item`]).
    Item(ObjectId),
}

#[derive(Debug, Default)]
struct PointerState {
    grab: Option<Grab>,
    /// TapHandlers with a passive grab (`DragThreshold` policy).
    passive: Vec<ObjectId>,
    press_pos: kurbo::Point,
    /// The grabbing MouseArea accepted `doubleClicked` on this press.
    double: bool,
    last_click: Option<(ObjectId, Time, kurbo::Point)>,
    /// Last tap per handler: time, position, button, count.
    taps: HashMap<ObjectId, (Time, kurbo::Point, MouseButtons, i32)>,
    hovered: Vec<ObjectId>,
    /// The hovered items of [`crate::register_pointer_item`] types.
    item_hovered: Vec<ObjectId>,
}

/// An item tree, its render tree, and the frame pipeline between them.
pub struct QuickScene {
    tree: RenderTree,
    shared: SharedRc,
    #[allow(deprecated)]
    log: QuickLog,
    entries: HashMap<ObjectId, Entry>,
    by_node: HashMap<NodeId, ObjectId>,
    paint: HashMap<ObjectId, Paint>,
    root: Option<ObjectId>,
    /// The layer above the root item (`crate::layer`), once one was made.
    layer: Option<ObjectId>,
    viewport: kurbo::Rect,
    scale_factor: f32,
    frame: u64,
    snapshot: Option<Arc<FrameSnapshot>>,
    /// The UI thread's text engine, and the font generation the Texts
    /// were last laid out under.
    text: SharedTextEngine,
    text_generation: u64,
    timers: Option<TimerHook>,
    next_timer: Option<Time>,
    pointer: PointerState,
    /// Creation jobs of asynchronous Loaders, run a slice per frame.
    incubator: Scheduler<()>,
    /// What the creation jobs of the last frame that ran any cost, until a
    /// development tool takes it (`take_incubation_report`).
    incubation: Option<ljos_runtime::FrameReport>,
    /// What a frame gives the creation jobs: [`INCUBATION_BUDGET`], less
    /// on a display faster than 60 Hz (`set_refresh_rate`).
    incubation_budget: Duration,
    /// The accessibility tree; idle unless an assistive technology listens.
    pub(crate) a11y: A11yPass,
    /// The engine's picture store, known from the first frame on, so that
    /// `needs_frame` sees decoded pictures waiting to be shown.
    images: Option<Rc<crate::image::ImageStore>>,
}

impl std::fmt::Debug for QuickScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickScene")
            .field("root", &self.root)
            .field("items", &self.entries.len())
            .field("viewport", &self.viewport)
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

impl QuickScene {
    /// An empty scene showing `viewport` (logical pixels).
    #[allow(deprecated)]
    pub fn new(viewport: kurbo::Size) -> Self {
        let shared = SharedRc::default();
        let weak = Rc::downgrade(&shared);
        let a11y = A11yPass::new(Rc::new(move || {
            if let Some(s) = weak.upgrade() {
                mark(&s, |_| {});
            }
        }));
        QuickScene {
            tree: RenderTree::new(),
            shared,
            log: QuickLog::default(),
            entries: HashMap::new(),
            by_node: HashMap::new(),
            paint: HashMap::new(),
            root: None,
            layer: None,
            viewport: kurbo::Rect::from_origin_size((0.0, 0.0), viewport),
            scale_factor: 1.0,
            frame: 0,
            snapshot: None,
            text_generation: text::text_engine().borrow().generation(),
            text: text::text_engine(),
            timers: None,
            next_timer: None,
            pointer: PointerState::default(),
            incubator: Scheduler::new(),
            incubation: None,
            incubation_budget: INCUBATION_BUDGET,
            a11y,
            images: None,
        }
    }

    /// Shows `root` and everything under it, replacing any previous root.
    pub fn set_root(&mut self, engine: &Engine, root: ObjectId) -> Result<(), EngineError> {
        if let Some(old) = self.root.take() {
            self.unadopt(engine, old, true);
            crate::focus::tree::remove_window(engine, old);
            crate::keys::reset_shortcuts(engine, old);
        }
        if !is_item(engine, root) {
            return Err(EngineError::WrongType {
                ty: engine
                    .meta_of(root)
                    .map_or_else(|| "?".to_owned(), |m| m.name.to_string()),
                expected: "Item".into(),
            });
        }
        let top = self.tree.root();
        self.adopt(engine, root, None, top, 0)?;
        self.root = Some(root);
        // The scene is the window: its focus item gets active focus.
        crate::focus::tree::add_window(engine, root);
        // A Timer started from a handler changes nothing visible, so
        // without this the host would never ask for its deadline.
        let shared = Rc::downgrade(&self.shared);
        engine.set_timer_wake(move || {
            if let Some(s) = shared.upgrade() {
                mark(&s, |_| {});
            }
        });
        Ok(())
    }

    pub fn root(&self) -> Option<ObjectId> {
        self.root
    }

    /// What the scene shows, in logical pixels.
    pub fn viewport(&self) -> kurbo::Rect {
        self.viewport
    }

    /// Device pixels per logical pixel.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Asks for a frame although nothing changed. A development tool that
    /// ran a frame of its own (the inspector, to read the accessibility
    /// tree) calls this so that the host runs the next one and learns from
    /// it when the scene next wants to be woken.
    pub fn request_frame(&self) {
        mark(&self.shared, |_| {});
    }

    /// What the creation jobs (asynchronous Loaders, delegates) of the
    /// last frame that ran any cost, once. For the `top` overlay and the
    /// inspector's measurements; a host that shows neither never calls it.
    pub fn take_incubation_report(&mut self) -> Option<ljos_runtime::FrameReport> {
        self.incubation.take()
    }

    /// The display's refresh rate, which scales the time a frame gives to
    /// creation jobs ([`crate::incubation_budget`]). A host that never
    /// says is taken to show 60 frames a second.
    pub fn set_refresh_rate(&mut self, hz: f64) {
        self.incubation_budget = crate::incubation::incubation_budget(hz);
    }

    /// What a frame gives the creation jobs.
    pub fn incubation_budget(&self) -> Duration {
        self.incubation_budget
    }

    pub fn set_viewport(&mut self, viewport: kurbo::Rect) {
        if self.viewport != viewport {
            self.viewport = viewport;
            mark(&self.shared, |_| {});
        }
    }

    /// Device pixels per logical pixel. A change rebuilds the display
    /// lists of every painting item on the next frame.
    pub fn set_scale_factor(&mut self, scale: f32) {
        if self.scale_factor == scale {
            return;
        }
        self.scale_factor = scale;
        let nodes: Vec<NodeId> = self
            .entries
            .values()
            .filter(|e| e.kind.paints() && e.content_effect.is_some())
            .map(|e| e.node)
            .collect();
        for n in nodes {
            self.tree.mark_content_dirty(n);
        }
        mark(&self.shared, |_| {});
    }

    /// Lays every Text out again on the next frame, and asks for that frame.
    /// For a host that changed the fonts of the text engine
    /// ([`crate::text_engine`]) while the scene is showing.
    pub fn refresh_text(&mut self) {
        let texts: Vec<ObjectId> = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.kind, Kind::Text | Kind::Editor))
            .map(|(o, _)| *o)
            .collect();
        let content: Vec<(ObjectId, NodeId)> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                matches!(e.kind, Kind::Text | Kind::Editor) && e.content_effect.is_some()
            })
            .map(|(o, e)| (*o, e.node))
            .collect();
        for (_, n) in &content {
            self.tree.mark_content_dirty(*n);
        }
        mark(&self.shared, |s| s.polish.extend(texts));
    }

    /// Follows the UI thread's text engine: another engine, or other fonts
    /// in the same one, lay every Text out again. Checked on frames that
    /// run anyway.
    fn follow_text_engine(&mut self) {
        let current = text::text_engine();
        let generation = current.borrow().generation();
        if Rc::ptr_eq(&current, &self.text) && generation == self.text_generation {
            return;
        }
        self.text = current;
        self.text_generation = generation;
        self.refresh_text();
    }

    /// Called (unborrowed, at most once per frame) when the scene goes from
    /// idle to needing a frame.
    pub fn set_waker(&mut self, waker: impl Fn() + 'static) {
        self.shared.borrow_mut().waker = Some(Rc::new(waker));
    }

    /// Adds host work to step 3; see [`TimerHook`]. The engine's own timers
    /// run without one.
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.timers = Some(hook);
    }

    /// Always empty: the scene's warnings go to the engine log
    /// ([`Engine::set_log_handler`], [`Engine::take_log`]). Kept, and not
    /// marked deprecated, until its callers are gone: they build with
    /// warnings as errors.
    #[allow(deprecated)]
    pub fn log(&self) -> &QuickLog {
        &self.log
    }

    pub fn render_tree(&self) -> &RenderTree {
        &self.tree
    }

    pub fn node_of(&self, item: ObjectId) -> Option<NodeId> {
        self.entries.get(&item).map(|e| e.node)
    }

    pub fn item_of(&self, node: NodeId) -> Option<ObjectId> {
        self.by_node.get(&node).copied()
    }

    /// The last composed snapshot.
    pub fn snapshot(&self) -> Option<&Arc<FrameSnapshot>> {
        self.snapshot.as_ref()
    }

    /// Whether `frame` has work to do.
    pub fn needs_frame(&self) -> bool {
        self.shared.borrow().dirty
            || self
                .images
                .as_ref()
                .is_some_and(|i| i.has_results() && !i.is_held())
    }

    // ------------------------------------------------------------ frame

    /// Pipeline steps 3-9 at time `now`, in the host's clock.
    ///
    /// Engine timers run against [`Engine::now`]; their next deadline is
    /// translated into the host's clock for `next_wake`, so the two clocks
    /// need not share an epoch.
    pub fn frame(&mut self, engine: &Engine, now: Time) -> FrameOutput {
        self.frame += 1;
        // Marks made during the frame are drained by it or reported
        // through `next_wake`; they must not wake the host a second time.
        self.shared.borrow_mut().dirty = true;
        self.follow_text_engine();
        // Pictures decoded in the background since the last frame.
        let images = crate::image::ImageStore::of(engine);
        images.deliver(engine);
        self.images = Some(images);
        // 3. Timers and animations.
        let engine_now = engine.now();
        engine.advance_timers(engine_now);
        if let Some(hook) = &mut self.timers {
            self.next_timer = hook(engine, now);
        }
        // Faults of the turns since the last frame go to their boundaries
        // here, between turns: what a boundary discards is deleted next
        // and what it makes is laid out in this frame.
        engine.deliver_faults();
        // 4. Deferred deletes.
        let deleted = engine.process_deferred_deletes();
        let dead = std::mem::take(&mut self.shared.borrow_mut().dead);
        if deleted > 0 || !dead.is_empty() {
            self.sweep_dead(engine);
        }
        // Focus asked for while objects were being made.
        crate::focus::tree::settle(engine);
        self.sync_layer(engine);
        // 5. Animations, from the frame's time and nothing else.
        crate::animation::tick(engine, now);
        // 6. Polish.
        self.polish(engine);
        // 7. Incubate. New items join the tree and are laid out in this
        // same frame, so what is shown is never half placed.
        let incubating = self.incubate(engine);
        // 8. Sync.
        self.reconcile(engine);
        self.apply_geometry();
        self.apply_content();
        // 9. Compose; then pull paint inputs for newly visible items.
        let mut snap = self.tree.compose(self.viewport, self.frame);
        let mut synced = self.snapshot.clone();
        for _ in 0..3 {
            if synced.as_ref().is_some_and(|s| Arc::ptr_eq(s, &snap)) {
                break;
            }
            let installed = self.sync_content_effects(engine, &snap);
            synced = Some(Arc::clone(&snap));
            if !installed {
                break;
            }
            self.apply_content();
            snap = self.tree.compose(self.viewport, self.frame);
        }
        let changed = !self
            .snapshot
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &snap));
        self.snapshot = Some(Arc::clone(&snap));
        // Last, so the tree describes the items as this frame left them.
        self.a11y.sync(engine, self.root);
        // Layouts no frame asked for lately are dropped here, on a frame
        // that runs anyway; nothing ages while the scene is idle.
        self.text.borrow_mut().end_frame();
        let pending = {
            let mut s = self.shared.borrow_mut();
            let p = s.pending() || self.a11y.pending();
            // Unfinished jobs need the next frame although nothing they
            // made shows yet.
            s.dirty = p || incubating;
            p
        };
        FrameOutput {
            snapshot: snap,
            changed,
            next_wake: if pending
                || incubating
                || engine.has_pending_calls()
                || engine.has_pending_faults()
                || crate::focus::tree::pending(engine)
            {
                Some(now)
            } else {
                let engine_wake = engine
                    .next_timer_deadline()
                    .map(|d| now + d.saturating_since(engine_now));
                // A running animation asks for its next frame here, and
                // for none once it has stopped.
                let animation_wake = crate::animation::next_wake(engine, now);
                [engine_wake, self.next_timer, animation_wake]
                    .into_iter()
                    .flatten()
                    .min()
            },
        }
    }

    /// Shows the layer above the root item once one was made, and keeps it
    /// the size of the viewport. A map lookup while there is none.
    fn sync_layer(&mut self, engine: &Engine) {
        let want = self
            .root
            .and_then(|r| engine.untrack(|| crate::layer::layer_of(engine, r)));
        if want != self.layer {
            if let Some(old) = self.layer.take() {
                self.unadopt(engine, old, true);
            }
            if let Some(layer) = want {
                let top = self.tree.root();
                match self.adopt(engine, layer, None, top, 1) {
                    Ok(()) => self.layer = Some(layer),
                    Err(e) => engine.log(
                        LogLevel::Warning,
                        SCENE_CATEGORY,
                        format!("adopt failed: {e}"),
                    ),
                }
            }
            self.a11y.set_layer(self.layer);
        }
        if let Some(layer) = self.layer {
            for (p, v) in [
                (Item::WIDTH.index, self.viewport.width()),
                (Item::HEIGHT.index, self.viewport.height()),
            ] {
                let _ = write_computed(engine, layer, p, Value::Real(v));
            }
        }
    }

    /// Step 7: runs queued creation jobs for the incubation budget on the
    /// engine's clock. Returns whether any are left for the next frame.
    fn incubate(&mut self, engine: &Engine) -> bool {
        for job in engine.take_incubations() {
            self.incubator.spawn(Box::new(job));
        }
        if self.incubator.is_empty() {
            return false;
        }
        let clock = engine.clock();
        let budget = FrameBudget::new(clock.now(), self.incubation_budget);
        let report = self.incubator.run_frame(&mut (), budget, &*clock);
        for (owner, fault) in &report.faults {
            // A fault of the boundary the job was making something in.
            engine.report_fault(engine.proc_owner(*owner), fault.clone());
        }
        let made = report.jobs_done > 0;
        // Moved, not copied: keeping it costs a host that never asks nothing.
        self.incubation = Some(report);
        if made {
            // What was made can be shown and measured now.
            self.reconcile(engine);
            self.polish(engine);
        }
        // A finished job may have queued another (a Loader in the item).
        !self.incubator.is_empty() || engine.has_incubations()
    }

    fn sweep_dead(&mut self, engine: &Engine) {
        let dead: Vec<ObjectId> = self
            .entries
            .keys()
            .copied()
            .filter(|o| !engine.is_alive(*o))
            .collect();
        for o in dead {
            if self.root == Some(o) {
                self.root = None;
            }
            self.unadopt(engine, o, true);
        }
    }

    fn polish(&mut self, engine: &Engine) {
        for _ in 0..MAX_POLISH_PASSES {
            let mut set: Vec<(usize, ObjectId)> = {
                let mut s = self.shared.borrow_mut();
                s.polish
                    .drain()
                    .filter(|o| self.entries.contains_key(o))
                    .map(|o| (depth(engine, o), o))
                    .collect()
            };
            if set.is_empty() {
                return;
            }
            // Deepest first, so a positioner sees its children's final size.
            set.sort_by_key(|a| std::cmp::Reverse(a.0));
            for (_, obj) in set {
                if !engine.is_alive(obj) {
                    continue;
                }
                let Some(kind) = self.entries.get(&obj).map(|e| e.kind) else {
                    continue;
                };
                let r = match kind {
                    Kind::Row => positioners::layout(engine, obj, Axis::Horizontal),
                    Kind::Column => positioners::layout(engine, obj, Axis::Vertical),
                    Kind::Managed => crate::layouts::polish(engine, obj),
                    Kind::Text => text::polish(engine, obj, &self.text),
                    // Only after the fonts changed (`refresh_text`).
                    Kind::Editor => crate::text_input::refresh(engine, obj),
                    Kind::Item | Kind::Rectangle | Kind::Image => Ok(()),
                };
                if let Err(e) = r {
                    engine.log(
                        LogLevel::Warning,
                        POLISH_CATEGORY,
                        format!("polish failed: {e}"),
                    );
                }
            }
        }
        let left = std::mem::take(&mut self.shared.borrow_mut().polish);
        if !left.is_empty() {
            engine.log(
                LogLevel::Warning,
                POLISH_CATEGORY,
                format!(
                    "Possible polish loop: {} item(s) still dirty after {MAX_POLISH_PASSES} passes",
                    left.len()
                ),
            );
        }
    }

    // ------------------------------------------------------------ adoption

    fn adopt(
        &mut self,
        engine: &Engine,
        obj: ObjectId,
        parent: Option<ObjectId>,
        parent_node: NodeId,
        index: usize,
    ) -> Result<(), EngineError> {
        if self.entries.contains_key(&obj) {
            return Ok(());
        }
        init_item(engine, obj)?;
        let node = self.tree.insert_child(parent_node, index);
        let kind = Kind::of(engine, obj);
        if kind == Kind::Editor {
            crate::text_input::init(engine, obj)?;
        }
        let anchors = Rc::new(RefCell::new(AnchorState::default()));
        let inherit = Rc::new(RefCell::new(Inherit::default()));
        self.entries.insert(
            obj,
            Entry {
                node,
                kind,
                parent,
                effects: Vec::new(),
                content_effect: None,
                anchors: Rc::clone(&anchors),
                inherit: Rc::clone(&inherit),
                size: kurbo::Size::ZERO,
            },
        );
        self.by_node.insert(node, obj);

        let mut effects = Vec::with_capacity(5);
        let shared = Rc::clone(&self.shared);
        // Root-scoped: an effect scoped to `obj` dies with it silently, and
        // this one has to see the death to drop the render node.
        effects.push(engine.effect(move |e| structure_effect(e, obj, &shared))?);
        effects.push(engine.effect_in(obj, move |e| inherit_effect(e, obj, &inherit))?);
        effects.push(engine.effect_in(obj, move |e| anchors::run(e, obj, &anchors))?);
        let shared = Rc::clone(&self.shared);
        effects.push(engine.effect_in(obj, move |e| geometry_effect(e, obj, &shared))?);
        if kind.polishes() {
            let shared = Rc::clone(&self.shared);
            effects.push(engine.effect_in(obj, move |e| polish_effect(e, obj, kind, &shared))?);
        }
        if let Some(e) = self.entries.get_mut(&obj) {
            e.effects = effects;
        }
        self.a11y.adopt(engine, obj, kind == Kind::Text);

        let children: Vec<ObjectId> = engine
            .untrack(|| shown_children(engine, obj))
            .into_iter()
            .filter(|c| engine.is_alive(*c) && is_item(engine, *c))
            .collect();
        for (i, c) in children.into_iter().enumerate() {
            self.adopt(engine, c, Some(obj), node, i)?;
        }
        Ok(())
    }

    /// Forgets `obj` and its subtree; removes the render nodes if
    /// `remove_node` (only the top of the subtree needs it).
    fn unadopt(&mut self, engine: &Engine, obj: ObjectId, remove_node: bool) {
        let Some(entry) = self.entries.remove(&obj) else {
            return;
        };
        let g = engine.graph();
        for n in entry.effects.iter().chain(entry.content_effect.iter()) {
            let _ = g.dispose(*n);
        }
        self.a11y.forget(engine, obj);
        if !engine.is_alive(obj) {
            crate::focus::tree::forget(engine, obj);
        }
        if engine.is_alive(obj) {
            entry.anchors.borrow_mut().release(engine, obj);
            let mut inh = entry.inherit.borrow_mut();
            let inh = &mut *inh;
            for (p, slot) in [
                (Item::VISIBLE.index, &mut inh.visible),
                (Item::ENABLED.index, &mut inh.enabled),
            ] {
                if let Some(id) = slot.take() {
                    let _ = engine.remove_override(obj, p, id, RestoreMode::BindingOrValue);
                }
            }
        }
        let kids: Vec<NodeId> = self
            .tree
            .node(entry.node)
            .map(|n| n.children().to_vec())
            .unwrap_or_default();
        for k in kids {
            if let Some(c) = self.by_node.get(&k).copied() {
                self.unadopt(engine, c, false);
            }
        }
        self.by_node.remove(&entry.node);
        self.paint.remove(&obj);
        if remove_node {
            let _ = self.tree.remove(entry.node);
        }
        let mut s = self.shared.borrow_mut();
        s.geom.remove(&obj);
        s.content.remove(&obj);
        s.structure.remove(&obj);
        s.polish.remove(&obj);
        drop(s);
        for g in [&mut self.pointer.passive, &mut self.pointer.hovered] {
            g.retain(|o| *o != obj);
        }
        if matches!(self.pointer.grab, Some(Grab::Mouse(o)) if o == obj) {
            self.pointer.grab = None;
        }
    }

    /// Step 8a: render-node structure follows the items' `children`.
    fn reconcile(&mut self, engine: &Engine) {
        let dirty: Vec<ObjectId> = self.shared.borrow_mut().structure.drain().collect();
        let mut orphans = Vec::new();
        for p in dirty {
            if !engine.is_alive(p) {
                continue;
            }
            let Some(pnode) = self.entries.get(&p).map(|e| e.node) else {
                continue;
            };
            self.a11y.children_changed(p);
            let mut want: Vec<ObjectId> = engine
                .untrack(|| shown_children(engine, p))
                .into_iter()
                .filter(|c| engine.is_alive(*c) && is_item(engine, *c))
                .collect();
            let mut seen = HashSet::new();
            want.retain(|c| seen.insert(*c));
            for (i, c) in want.iter().enumerate() {
                match self.entries.get_mut(c) {
                    None => {
                        if let Err(e) = self.adopt(engine, *c, Some(p), pnode, i) {
                            engine.log(
                                LogLevel::Warning,
                                SCENE_CATEGORY,
                                format!("adopt failed: {e}"),
                            );
                        }
                    }
                    Some(ce) => {
                        ce.parent = Some(p);
                        let node = ce.node;
                        let at = self
                            .tree
                            .node(pnode)
                            .and_then(|n| n.children().get(i).copied());
                        if at != Some(node)
                            && let Err(e) = self.tree.reparent(node, pnode, Some(i))
                        {
                            engine.log(
                                LogLevel::Warning,
                                SCENE_CATEGORY,
                                format!("reparent failed: {e}"),
                            );
                        }
                    }
                }
            }
            if let Some(n) = self.tree.node(pnode) {
                for extra in n.children().iter().skip(want.len()) {
                    if let Some(o) = self.by_node.get(extra) {
                        orphans.push((*o, p));
                    }
                }
            }
        }
        for (o, p) in orphans {
            // Still recorded under `p`: nobody claimed it, so it left the
            // scene.
            if self.entries.get(&o).is_some_and(|e| e.parent == Some(p)) {
                self.unadopt(engine, o, true);
            }
        }
    }

    /// Step 8b.
    fn apply_geometry(&mut self) {
        let geoms: Vec<(ObjectId, Geom)> = self.shared.borrow_mut().geom.drain().collect();
        for (obj, g) in geoms {
            let Some(e) = self.entries.get_mut(&obj) else {
                continue;
            };
            let node = e.node;
            let size = kurbo::Size::new(g.w.max(0.0), g.h.max(0.0));
            self.tree.set_local(
                node,
                compose_transform(g.x, g.y, g.w, g.h, g.rotation, g.scale, g.origin),
            );
            if size != e.size {
                e.size = size;
                if e.kind.paints() && e.content_effect.is_some() {
                    self.tree.mark_content_dirty(node);
                }
            }
            self.tree.set_size(node, size);
            self.tree
                .set_opacity(node, g.opacity.clamp(0.0, 1.0) as f32);
            self.tree.set_visible(node, g.visible);
            self.tree
                .set_clip(node, g.clip.then(|| ClipSpec::bounds(size)));
            self.tree.set_z(node, g.z as f32);
        }
    }

    /// Step 8c: rebuild the display lists of items whose paint inputs or
    /// size changed.
    fn apply_content(&mut self) {
        let content: Vec<(ObjectId, Paint)> = self.shared.borrow_mut().content.drain().collect();
        for (obj, p) in content {
            if let Some(e) = self.entries.get(&obj) {
                self.tree.mark_content_dirty(e.node);
                self.paint.insert(obj, p);
            }
        }
        let paint = &self.paint;
        let by_node = &self.by_node;
        let mut te = self.text.borrow_mut();
        self.tree.rebuild_content(self.scale_factor, |cx, b| {
            let Some(p) = by_node.get(&cx.node).and_then(|o| paint.get(o)) else {
                return;
            };
            match p {
                Paint::Rect(r) => r.paint(cx.size, b),
                Paint::Text(t) => text::paint(&mut te, t, cx.size, b),
                Paint::Image(i) => i.paint(cx.size, b),
                Paint::Editor(p) => p.paint(cx.size, b),
            }
        });
    }

    /// Installs paint effects for painting items in `snap` and drops them
    /// for items that left it. Returns whether any were installed.
    fn sync_content_effects(&mut self, engine: &Engine, snap: &FrameSnapshot) -> bool {
        let visible: HashSet<ObjectId> = snap
            .items
            .iter()
            .filter_map(|i| self.by_node.get(&i.node).copied())
            .collect();
        let mut installed = false;
        // A dropped display list changes the tree, so the frame composes
        // again and does not hand out a snapshot that is already stale.
        let mut dropped = false;
        let mut failed = Vec::new();
        for (obj, e) in &mut self.entries {
            if !e.kind.paints() {
                continue;
            }
            let shown = visible.contains(obj);
            match (shown, e.content_effect) {
                (true, None) => {
                    let shared = Rc::clone(&self.shared);
                    let (o, kind) = (*obj, e.kind);
                    match engine.effect_in(o, move |en| content_effect(en, o, kind, &shared)) {
                        Ok(n) => {
                            e.content_effect = Some(n);
                            installed = true;
                        }
                        Err(err) => failed.push(err),
                    }
                }
                (false, Some(n)) => {
                    let _ = engine.graph().dispose(n);
                    e.content_effect = None;
                    // Text that left the screen keeps its numbers only: the
                    // display list holds the glyph runs, so it goes too
                    // (ARCHITECTURE §7.5). Coming back paints it again.
                    if e.kind == Kind::Text {
                        self.paint.remove(obj);
                        self.tree.set_content(e.node, None);
                        dropped = true;
                    }
                    // A picture likewise: the display list is what keeps
                    // its pixels from being dropped under memory pressure
                    // (ARCHITECTURE §8.5).
                    if e.kind == Kind::Image {
                        self.paint.remove(obj);
                        self.tree.set_content(e.node, None);
                        crate::image::hidden(engine, *obj);
                        dropped = true;
                    }
                }
                _ => {}
            }
        }
        for err in failed {
            engine.log(
                LogLevel::Warning,
                SCENE_CATEGORY,
                format!("paint effect failed: {err}"),
            );
        }
        installed || dropped
    }

    // ------------------------------------------------------------ input

    /// Delivers a pointer event (pipeline steps 1-2). Returns whether an
    /// item or handler took it.
    pub fn pointer_event(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        match ev.kind {
            PointerKind::Press => self.press(engine, ev),
            PointerKind::Move => self.moved(engine, ev),
            PointerKind::Release => self.release(engine, ev),
            PointerKind::Cancel => self.cancel(engine, ev),
            PointerKind::Leave => {
                self.update_hover(engine, ev, &[]);
                false
            }
        }
    }

    /// Offers a wheel event to the items under it, topmost first.
    /// Returns whether one of them used it. An event nothing uses changes
    /// nothing, so it asks for no frame.
    pub fn wheel_event(&mut self, engine: &Engine, ev: &crate::input::WheelEvent) -> bool {
        let hits = self.hits(engine, ev.pos);
        crate::flickable::wheel::deliver(engine, &hits, ev)
    }

    /// Items under `pos`, topmost first.
    fn hits(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
        let Some(snap) = &self.snapshot else {
            return Vec::new();
        };
        let mut hits: Vec<ObjectId> = hit_test(snap, pos)
            .filter_map(|n| self.by_node.get(&n).copied())
            .filter(|o| engine.is_alive(*o))
            .collect();
        crate::layer::trim(engine, self.layer, &mut hits);
        hits
    }

    fn press(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let button = ev.button.unwrap_or(MouseButtons::LEFT);
        if let Some(Grab::Mouse(m)) = self.pointer.grab
            && engine.is_alive(m)
        {
            // Another button while one is held goes to the same area.
            let held = ev.buttons.bits() as i32 & accepted_buttons(engine, m);
            set(
                engine,
                m,
                MouseArea::PRESSED_BUTTONS.index,
                Value::Int(held),
            );
            return true;
        }
        if let Some(Grab::Item(o)) = self.pointer.grab
            && engine.is_alive(o)
        {
            // Another button while one is held changes nothing for it.
            return true;
        }
        self.pointer.grab = None;
        self.pointer.passive.clear();
        self.pointer.press_pos = ev.pos;
        self.pointer.double = false;
        crate::flickable::drag::pressed(engine);
        let mut handled = false;
        let hits = self.hits(engine, ev.pos);
        // After the hits: a popup that closes on this press still keeps
        // it from what it covered.
        crate::layer::press(engine, self.layer, ev);
        for obj in hits {
            // An item's pointer handlers see the event before the item.
            for h in resources(engine, obj) {
                if !is_a::<TapHandler>(engine, h)
                    || !engine.read(h, TapHandler::ENABLED.index).to_boolean()
                    || num(engine, h, TapHandler::ACCEPTED_BUTTONS.index) as i32
                        & button.bits() as i32
                        == 0
                {
                    continue;
                }
                set(engine, h, TapHandler::PRESSED.index, Value::Bool(true));
                let policy = num(engine, h, TapHandler::GESTURE_POLICY.index) as i32;
                if policy == gesture_policy::DRAG_THRESHOLD {
                    self.pointer.passive.push(h);
                    handled = true;
                } else {
                    self.pointer.grab = Some(Grab::Tap(h));
                    return true;
                }
            }
            if is_a::<MouseArea>(engine, obj)
                && mouse_area_enabled(engine, obj)
                && accepted_buttons(engine, obj) & button.bits() as i32 != 0
                && self.press_mouse_area(engine, obj, ev, button)
            {
                return true;
            }
            if let Some(h) = crate::pointer_item::pointer_item(engine, obj)
                && (h.press)(engine, obj, ev, local_point(engine, obj, ev.pos))
            {
                self.pointer.grab = Some(Grab::Item(obj));
                return true;
            }
        }
        handled
    }

    fn press_mouse_area(
        &mut self,
        engine: &Engine,
        m: ObjectId,
        ev: &PointerEvent,
        button: MouseButtons,
    ) -> bool {
        let local = local_point(engine, m, ev.pos);
        set(engine, m, MouseArea::PRESSED.index, Value::Bool(true));
        set(
            engine,
            m,
            MouseArea::PRESSED_BUTTONS.index,
            Value::Int(button.bits() as i32),
        );
        set_mouse_pos(engine, m, local);
        let entered = set_contains(engine, m, true);
        if entered {
            emit(engine, m, "entered", &[]);
        }
        if !emit_mouse(engine, m, "pressed", ev, local, false) {
            // Rejected: the press goes on to the items below.
            set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
            set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
            if entered && !hover_enabled(engine, m) && set_contains(engine, m, false) {
                emit(engine, m, "exited", &[]);
            }
            return false;
        }
        self.pointer.grab = Some(Grab::Mouse(m));
        if let Some((last, t, p)) = self.pointer.last_click
            && last == m
            && ev.time.saturating_since(t) <= DOUBLE_CLICK_INTERVAL
            && (ev.pos - p).hypot() <= DRAG_THRESHOLD
        {
            self.pointer.last_click = None;
            self.pointer.double = emit_mouse(engine, m, "doubleClicked", ev, local, false);
        }
        true
    }

    fn moved(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let mut handled = false;
        if (ev.pos - self.pointer.press_pos).hypot() > DRAG_THRESHOLD {
            // A drag is not a tap.
            for h in std::mem::take(&mut self.pointer.passive) {
                cancel_tap(engine, h, ev.pos);
            }
        }
        // A Flickable around whatever holds the press may take it for a
        // drag; the holder is told its press was cancelled.
        let holder = match self.pointer.grab {
            Some(Grab::Mouse(o) | Grab::Item(o)) => Some(o),
            Some(Grab::Tap(h)) => crate::input::tap_handler_target(engine, h),
            None => None,
        };
        if let Some(f) = holder
            .and_then(|o| crate::flickable::drag::steal(engine, o, self.pointer.press_pos, ev))
        {
            self.cancel(engine, ev);
            self.pointer.grab = Some(Grab::Item(f));
        }
        match self.pointer.grab {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                let local = local_point(engine, m, ev.pos);
                set_mouse_pos(engine, m, local);
                let inside = contains_local(engine, m, local);
                if set_contains(engine, m, inside) {
                    emit(engine, m, if inside { "entered" } else { "exited" }, &[]);
                }
                emit_mouse(engine, m, "positionChanged", ev, local, false);
                return true;
            }
            Some(Grab::Tap(h)) if engine.is_alive(h) => {
                let policy = num(engine, h, TapHandler::GESTURE_POLICY.index) as i32;
                let inside = crate::input::tap_handler_target(engine, h)
                    .is_some_and(|t| contains_local(engine, t, local_point(engine, t, ev.pos)));
                if !inside
                    && matches!(
                        policy,
                        gesture_policy::WITHIN_BOUNDS | gesture_policy::DRAG_WITHIN_BOUNDS
                    )
                {
                    self.pointer.grab = None;
                    cancel_tap(engine, h, ev.pos);
                }
                handled = true;
            }
            Some(Grab::Item(o)) if engine.is_alive(o) => {
                if let Some(h) = crate::pointer_item::pointer_item(engine, o) {
                    (h.moved)(engine, o, ev, local_point(engine, o, ev.pos));
                }
                handled = true;
            }
            _ => {}
        }
        let hits = self.hits(engine, ev.pos);
        handled |= self.update_hover(engine, ev, &hits);
        handled || !self.pointer.passive.is_empty()
    }

    fn release(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let mut handled = false;
        crate::layer::release(engine, self.layer, ev);
        match self.pointer.grab {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                let held = ev.buttons.bits() as i32 & accepted_buttons(engine, m);
                if held != 0 {
                    // Other accepted buttons are still down.
                    set(
                        engine,
                        m,
                        MouseArea::PRESSED_BUTTONS.index,
                        Value::Int(held),
                    );
                    return true;
                }
                self.pointer.grab = None;
                let local = local_point(engine, m, ev.pos);
                let inside = contains_local(engine, m, local);
                set_mouse_pos(engine, m, local);
                set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                emit_mouse(engine, m, "released", ev, local, inside);
                if inside && !self.pointer.double {
                    emit_mouse(engine, m, "clicked", ev, local, true);
                    self.pointer.last_click = Some((m, ev.time, ev.pos));
                }
                if engine.is_alive(m) {
                    if hover_enabled(engine, m) {
                        if !self.pointer.hovered.contains(&m) {
                            self.pointer.hovered.push(m);
                        }
                    } else if set_contains(engine, m, false) {
                        emit(engine, m, "exited", &[]);
                    }
                }
                handled = true;
            }
            Some(Grab::Tap(h)) if engine.is_alive(h) => {
                self.pointer.grab = None;
                let inside = crate::input::tap_handler_target(engine, h)
                    .is_some_and(|t| contains_local(engine, t, local_point(engine, t, ev.pos)));
                if inside {
                    self.tap(engine, h, ev);
                } else {
                    cancel_tap(engine, h, ev.pos);
                }
                handled = true;
            }
            Some(Grab::Item(o)) if engine.is_alive(o) => {
                self.pointer.grab = None;
                if let Some(h) = crate::pointer_item::pointer_item(engine, o) {
                    (h.release)(engine, o, ev, local_point(engine, o, ev.pos));
                }
                handled = true;
            }
            _ => self.pointer.grab = None,
        }
        for h in std::mem::take(&mut self.pointer.passive) {
            if engine.is_alive(h) {
                self.tap(engine, h, ev);
                handled = true;
            }
        }
        self.pointer.double = false;
        let hits = self.hits(engine, ev.pos);
        self.update_hover(engine, ev, &hits);
        handled
    }

    fn cancel(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let mut handled = false;
        match self.pointer.grab.take() {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                if !hover_enabled(engine, m) {
                    set_contains(engine, m, false);
                }
                emit(engine, m, "canceled", &[]);
                handled = true;
            }
            Some(Grab::Tap(h)) if engine.is_alive(h) => {
                cancel_tap(engine, h, ev.pos);
                handled = true;
            }
            Some(Grab::Item(o)) if engine.is_alive(o) => {
                if let Some(h) = crate::pointer_item::pointer_item(engine, o) {
                    (h.cancel)(engine, o);
                }
                handled = true;
            }
            _ => {}
        }
        for h in std::mem::take(&mut self.pointer.passive) {
            cancel_tap(engine, h, ev.pos);
            handled = true;
        }
        handled
    }

    fn tap(&mut self, engine: &Engine, h: ObjectId, ev: &PointerEvent) {
        let button = ev.button.unwrap_or(MouseButtons::LEFT);
        let count = match self.pointer.taps.get(&h) {
            Some((t, p, b, c))
                if *b == button
                    && ev.time.saturating_since(*t) <= DOUBLE_CLICK_INTERVAL
                    && (ev.pos - *p).hypot() <= DRAG_THRESHOLD =>
            {
                c + 1
            }
            _ => 1,
        };
        self.pointer
            .taps
            .insert(h, (ev.time, ev.pos, button, count));
        set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
        set(engine, h, TapHandler::TAP_COUNT.index, Value::Int(count));
        let args = [point_value(ev.pos), Value::Int(button.bits() as i32)];
        emit(engine, h, "tapped", &args);
        match count {
            1 => emit(engine, h, "singleTapped", &args),
            2 => emit(engine, h, "doubleTapped", &args),
            _ => {}
        }
    }

    /// Hover for `hoverEnabled` MouseAreas, as Qt 6.11 delivers it
    /// (observed with `qmltestrunner`; see `tests/pointer.rs`): the topmost
    /// hover-enabled area under the pointer takes the hover, together with
    /// the hover-enabled areas it is inside. Areas beneath it get nothing,
    /// even when the top one is disabled.
    fn update_hover(&mut self, engine: &Engine, ev: &PointerEvent, hits: &[ObjectId]) -> bool {
        crate::pointer_item::update_hover(engine, hits, &mut self.pointer.item_hovered);
        crate::pointer_item::hover_moved(engine, &self.pointer.item_hovered, |o| {
            local_point(engine, o, ev.pos)
        });
        let grabbed = match self.pointer.grab {
            Some(Grab::Mouse(m)) => Some(m),
            _ => None,
        };
        let now: Vec<ObjectId> = hover_chain(engine, hits)
            .into_iter()
            .filter(|o| Some(*o) != grabbed)
            .collect();
        let old = std::mem::take(&mut self.pointer.hovered);
        // Leaving runs from the innermost area out, entering from the
        // outermost in, and every exit comes before the first enter.
        for o in old.iter().rev() {
            if !now.contains(o)
                && Some(*o) != grabbed
                && engine.is_alive(*o)
                && set_contains(engine, *o, false)
            {
                emit(engine, *o, "exited", &[]);
            }
        }
        for m in &now {
            let local = local_point(engine, *m, ev.pos);
            set_mouse_pos(engine, *m, local);
            if set_contains(engine, *m, true) {
                emit(engine, *m, "entered", &[]);
            } else if ev.kind == PointerKind::Move {
                // The move that enters an area is not also a position
                // change for it.
                emit_mouse(engine, *m, "positionChanged", ev, local, false);
            }
        }
        let any = !now.is_empty();
        self.pointer.hovered = now;
        any
    }
}

// ---------------------------------------------------------------- effects

fn structure_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        mark(shared, |s| s.dead.push(obj));
        return;
    }
    let _ = crate::tree::child_items(e, obj);
    if is_a::<Loader>(e, obj) {
        // A Loader's item is its child from the start but shown only once
        // it is made (`shown_children`).
        let _ = e.read(obj, Loader::ITEM.index);
        let _ = e.read(obj, Loader::STATUS.index);
    }
    // A child that is being made in slices is left out until it is whole
    // (`shown_children`); nothing of the item changes when it is.
    e.watch_incubations();
    mark(shared, |s| {
        s.structure.insert(obj);
    });
}

fn geometry_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let g = Geom::read(e, obj);
    mark(shared, |s| {
        s.geom.insert(obj, g);
    });
}

fn polish_effect(e: &Engine, obj: ObjectId, kind: Kind, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    match kind {
        Kind::Row => positioners::read_inputs(e, obj, Axis::Horizontal),
        Kind::Column => positioners::read_inputs(e, obj, Axis::Vertical),
        Kind::Managed => crate::layouts::read_inputs(e, obj),
        Kind::Text => {
            // Reads the item's size too, where the layout depends on it.
            TextInputs::read(e, obj);
        }
        Kind::Item | Kind::Rectangle | Kind::Image | Kind::Editor => return,
    }
    mark(shared, |s| {
        s.polish.insert(obj);
    });
}

fn content_effect(e: &Engine, obj: ObjectId, kind: Kind, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let p = match kind {
        Kind::Rectangle => Paint::Rect(RectanglePaint::read(e, obj)),
        Kind::Text => Paint::Text(TextInputs::read(e, obj)),
        Kind::Image => Paint::Image(crate::image::ImagePaint::read(e, obj)),
        Kind::Editor => Paint::Editor(crate::text_input::EditorPaint::read(e, obj)),
        _ => return,
    };
    mark(shared, |s| {
        s.content.insert(obj, p);
    });
}

/// `visible` and `enabled` read false under a hidden or disabled parent
/// (Qt documents both as inherited). The item's own value sits below an
/// override layer and comes back when the parent turns on again.
fn inherit_effect(e: &Engine, obj: ObjectId, st: &RefCell<Inherit>) {
    if !e.is_alive(obj) {
        return;
    }
    let parent = e
        .visual_parent(obj)
        .filter(|p| e.is_alive(*p) && is_item(e, *p));
    let mut st = st.borrow_mut();
    let st = &mut *st;
    for (p, slot) in [
        (Item::VISIBLE.index, &mut st.visible),
        (Item::ENABLED.index, &mut st.enabled),
    ] {
        let parent_on = if p == Item::ENABLED.index {
            // `MouseArea.enabled` is the area's own switch: it neither
            // follows a disabled ancestor nor disables what is inside.
            is_a::<MouseArea>(e, obj) || parent.is_none_or(|q| item_enabled(e, q))
        } else {
            parent.is_none_or(|q| e.read(q, p).to_boolean())
        };
        if let Err(err) = inherit_one(e, obj, p, slot, parent_on) {
            e.log(
                LogLevel::Warning,
                SCENE_CATEGORY,
                format!("inherit failed: {err}"),
            );
        }
    }
}

fn inherit_one(
    e: &Engine,
    obj: ObjectId,
    p: PropIndex,
    slot: &mut Option<OverrideId>,
    parent_on: bool,
) -> Result<(), EngineError> {
    let own = e.read(obj, p).to_boolean();
    match (parent_on, *slot) {
        (true, None) => Ok(()),
        (true, Some(id)) => {
            *slot = None;
            // The layer is a binding until something assigns through it;
            // an assignment while covered is the item's new own value.
            let assigned = e.provenance(obj, p) == Some(Provenance::Assigned);
            let mode = if assigned {
                RestoreMode::None
            } else {
                RestoreMode::BindingOrValue
            };
            e.remove_override(obj, p, id, mode)
        }
        (false, None) => {
            if own {
                *slot = Some(push_off(e, obj, p)?);
            }
            Ok(())
        }
        (false, Some(id)) => {
            if own {
                // `true` was assigned while the parent is off: keep it as
                // the own value, and keep reading false.
                e.remove_override(obj, p, id, RestoreMode::None)?;
                *slot = Some(push_off(e, obj, p)?);
            }
            Ok(())
        }
    }
}

fn push_off(e: &Engine, obj: ObjectId, p: PropIndex) -> Result<OverrideId, EngineError> {
    e.push_override(
        obj,
        p,
        OverrideContent::Binding(Box::new(|_| Value::Bool(false))),
    )
}

// ---------------------------------------------------------------- helpers

fn depth(engine: &Engine, obj: ObjectId) -> usize {
    let mut d = 0;
    let mut cur = engine.visual_parent(obj);
    while let Some(p) = cur {
        d += 1;
        if d > 4096 {
            break;
        }
        cur = engine.visual_parent(p);
    }
    d
}

fn set(engine: &Engine, obj: ObjectId, p: PropIndex, v: Value) {
    let _ = write_computed(engine, obj, p, v);
}

fn emit(engine: &Engine, obj: ObjectId, name: &str, args: &[Value]) {
    let _ = engine.emit_by_name(obj, name, args);
}

/// Whether `obj` leaves the items inside it enabled. A MouseArea's own
/// `enabled` does not count (Qt documents that it does not cascade), so
/// the answer comes from the nearest ancestor that is not one.
fn item_enabled(e: &Engine, obj: ObjectId) -> bool {
    let mut cur = Some(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let Some(o) = cur.filter(|o| e.is_alive(*o) && is_item(e, *o)) else {
            break;
        };
        if !is_a::<MouseArea>(e, o) {
            return e.read(o, Item::ENABLED.index).to_boolean();
        }
        cur = e.visual_parent(o);
    }
    true
}

/// Whether a MouseArea takes pointer events: its own `enabled`, inside
/// enabled items.
fn mouse_area_enabled(e: &Engine, m: ObjectId) -> bool {
    e.read(m, Item::ENABLED.index).to_boolean()
        && e.visual_parent(m).is_none_or(|p| item_enabled(e, p))
}

/// The MouseAreas the pointer hovers, outermost first, given the items
/// under it (topmost first).
fn hover_chain(e: &Engine, hits: &[ObjectId]) -> Vec<ObjectId> {
    let takes_hover = |o: ObjectId| is_a::<MouseArea>(e, o) && hover_enabled(e, o);
    // A disabled area still takes the hover away from what is beneath it;
    // one inside a disabled item is not there at all.
    let Some(top) = hits
        .iter()
        .copied()
        .find(|o| takes_hover(*o) && e.visual_parent(*o).is_none_or(|p| item_enabled(e, p)))
    else {
        return Vec::new();
    };
    let mut chain = Vec::new();
    let mut cur = Some(top);
    for _ in 0..4096 {
        let Some(o) = cur else { break };
        // An ancestor is hovered whether or not the pointer is inside it.
        if takes_hover(o) && e.read(o, Item::ENABLED.index).to_boolean() {
            chain.push(o);
        }
        cur = e.visual_parent(o);
    }
    chain.reverse();
    chain
}

fn accepted_buttons(engine: &Engine, m: ObjectId) -> i32 {
    num(engine, m, MouseArea::ACCEPTED_BUTTONS.index) as i32
}

fn hover_enabled(engine: &Engine, m: ObjectId) -> bool {
    engine.read(m, MouseArea::HOVER_ENABLED.index).to_boolean()
}

/// Sets `containsMouse`; true if it changed.
fn set_contains(engine: &Engine, m: ObjectId, v: bool) -> bool {
    let cur = engine.read(m, MouseArea::CONTAINS_MOUSE.index).to_boolean();
    if cur == v {
        return false;
    }
    set(engine, m, MouseArea::CONTAINS_MOUSE.index, Value::Bool(v));
    true
}

fn set_mouse_pos(engine: &Engine, m: ObjectId, local: kurbo::Point) {
    engine.batch(|| {
        set(engine, m, MouseArea::MOUSE_X.index, Value::Real(local.x));
        set(engine, m, MouseArea::MOUSE_Y.index, Value::Real(local.y));
    });
}

fn local_point(engine: &Engine, obj: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| scene_transform(engine, obj)).inverse() * pos
}

fn contains_local(engine: &Engine, obj: ObjectId, p: kurbo::Point) -> bool {
    let w = num(engine, obj, Item::WIDTH.index);
    let h = num(engine, obj, Item::HEIGHT.index);
    p.x >= 0.0 && p.y >= 0.0 && p.x < w && p.y < h
}

fn point_value(p: kurbo::Point) -> Value {
    Value::Point(Arc::new(Point { x: p.x, y: p.y }))
}

fn cancel_tap(engine: &Engine, h: ObjectId, pos: kurbo::Point) {
    if !engine.is_alive(h) {
        return;
    }
    set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
    emit(engine, h, "canceled", &[point_value(pos)]);
}

/// Emits a MouseArea signal with a fresh `mouse` object and returns its
/// `accepted` after the handlers ran.
fn emit_mouse(
    engine: &Engine,
    m: ObjectId,
    name: &str,
    ev: &PointerEvent,
    local: kurbo::Point,
    is_click: bool,
) -> bool {
    let native = MouseEvent {
        x: local.x,
        y: local.y,
        button: ev.button.map_or(0, |b| b.bits() as i32),
        buttons: ev.buttons.bits() as i32,
        modifiers: ev.modifiers.bits() as i32,
        is_click,
        ..MouseEvent::default()
    };
    let Ok(me) = engine.create_native(native, Some(m)) else {
        return true;
    };
    emit(engine, m, name, &[Value::Object(me)]);
    let accepted = engine.read(me, MouseEvent::ACCEPTED.index).to_boolean();
    engine.destroy_now(me);
    accepted
}
