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
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

thread_local! {
    static POLISHES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static POLISHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a scene of this thread is laying its items out: an item's
/// length may still change as the items in it are laid out.
pub(crate) fn polishing() -> bool {
    POLISHING.get()
}

/// How many times a scene of this thread polished: what was made before
/// a later count has had its length from what is in it.
pub(crate) fn polishes() -> u64 {
    POLISHES.get()
}

use ljos_engine::{
    Engine, EngineError, LogLevel, NodeId as GraphNode, ObjectId, OverrideContent, OverrideId,
    Point, PropIndex, Provenance, RestoreMode, Value,
};
use ljos_render::{ClipSpec, FrameSnapshot, ImageHandle, NodeId, RenderTree, hit_test, kurbo};
use ljos_runtime::{FrameBudget, Scheduler, Time};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SecondaryMap;

use crate::accessible::A11yPass;
use crate::anchors::{self, AnchorState};
use crate::animation::shown::Shown;
use crate::enums::gesture_policy;
use crate::input::{MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerKind, TapHandler};
use crate::item::{Item, init_item, is_a, is_item, num, scene_transform, write_computed};
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
/// Passes per frame in which views lay out the rows they just made, before
/// those count as passes too.
pub const MAX_ROW_PASSES: usize = 1024;
/// Time per frame for making the items of asynchronous Loaders
/// (ARCHITECTURE §6.3).
pub const INCUBATION_BUDGET: Duration = Duration::from_millis(3);
/// Qt's default `QStyleHints::mouseDoubleClickInterval`.
pub const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);
/// The default of `Qt.styleHints.startDragDistance`, in logical pixels.
/// What decides at an event asks [`crate::system_prefs::start_drag_distance`],
/// which a host may change.
pub const DRAG_THRESHOLD: f64 = crate::system_prefs::START_DRAG_DISTANCE as f64;

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
    /// A `Shape`: paints the paths declared in it.
    Shape,
    /// A `Canvas`: paints in its polish, shows the picture.
    Canvas,
    /// A `MultiEffect`: paints its source with its mask applied.
    Effect,
    /// A `ShaderEffect`: its programs, or its `source` with none.
    Shader,
    /// A `ShaderEffectSource`: a picture of its source item.
    Source,
    /// An item of another crate that paints itself
    /// (`crate::register_paint_item`).
    External,
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
        } else if is_a::<crate::shapes::Shape>(engine, obj) {
            Kind::Shape
        } else if is_a::<crate::canvas::Canvas>(engine, obj) {
            Kind::Canvas
        } else if is_a::<crate::effects::MultiEffect>(engine, obj) {
            Kind::Effect
        } else if is_a::<crate::shader_effect::ShaderEffect>(engine, obj) {
            Kind::Shader
        } else if is_a::<crate::shader_effect::ShaderEffectSource>(engine, obj) {
            Kind::Source
        } else if crate::paint_item::paint_item(engine, obj).is_some() {
            Kind::External
        } else {
            Kind::Item
        }
    }

    fn paints(self) -> bool {
        matches!(
            self,
            Kind::Rectangle
                | Kind::Text
                | Kind::Image
                | Kind::Editor
                | Kind::Shape
                | Kind::Canvas
                | Kind::Effect
                | Kind::Shader
                | Kind::Source
                | Kind::External
        )
    }

    fn polishes(self) -> bool {
        matches!(
            self,
            Kind::Text | Kind::Row | Kind::Column | Kind::Managed | Kind::Canvas
        )
    }
}

/// The geometry of one item, as its render node needs it, but for where
/// it is (`Place`): an item that only moves, as an animated or flicked one
/// does, reads two properties a frame instead of all of these.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Geom {
    z: f64,
    w: f64,
    h: f64,
    visible: bool,
    /// A `Flipable` face turned away.
    faces_away: bool,
    opacity: f64,
    clip: bool,
    rotation: f64,
    scale: f64,
    origin: i32,
    /// The item's `transform` list has entries. What they make is followed
    /// by an effect of its own ([`list_effect`]), so an entry that moves
    /// (a `Rotation`'s angle) does not read all of the above again.
    listed: bool,
}

/// The transform of an item at `place` with geometry `g` and the map of
/// its `transform` list, on a display of `scale` device pixels a point.
fn local_transform(
    place: Place,
    g: &Geom,
    list: &crate::transform::Projective,
    scale: f32,
) -> kurbo::Affine {
    let (mut x, mut y, whole) = place;
    if whole {
        let s = f64::from(scale);
        (x, y) = ((x * s).round() / s, (y * s).round() / s);
    }
    crate::transform::compose(x, y, g.w, g.h, g.rotation, g.scale, g.origin, list)
}

/// A property as the picture shows it: what an Animator on its way shows,
/// else its value. The property is read either way, so that the reader
/// follows it once the Animator is over.
fn shown_num(e: &Engine, shown: &Shown, obj: ObjectId, p: PropIndex) -> f64 {
    let v = num(e, obj, p);
    shown.get(obj, p).unwrap_or(v)
}

impl Geom {
    fn read(e: &Engine, obj: ObjectId, shown: &Shown) -> Geom {
        // In two goes rather than eleven: each new item runs this.
        let [z, w, h, opacity, rotation, scale, origin] = e
            .read_numbers(
                obj,
                [
                    Item::Z.index,
                    Item::WIDTH.index,
                    Item::HEIGHT.index,
                    Item::OPACITY.index,
                    Item::ROTATION.index,
                    Item::SCALE.index,
                    Item::TRANSFORM_ORIGIN.index,
                ],
            )
            .unwrap_or_default();
        let [visible, clip, transform] = e.read_values(
            obj,
            [Item::VISIBLE.index, Item::CLIP.index, Item::TRANSFORM.index],
        );
        let shown = |p: PropIndex, v: f64| shown.get(obj, p).unwrap_or(v);
        Geom {
            z,
            w,
            h,
            visible: visible.to_boolean(),
            faces_away: crate::flipable::faces_away(e, obj),
            opacity: shown(Item::OPACITY.index, opacity),
            clip: clip.to_boolean(),
            rotation: shown(Item::ROTATION.index, rotation),
            scale: shown(Item::SCALE.index, scale),
            origin: origin as i32,
            listed: matches!(transform, Value::List(l) if !l.is_empty()),
        }
    }
}

#[derive(Clone, Debug)]
enum Paint {
    Rect(RectanglePaint),
    Text(TextInputs),
    Image(crate::image::ImagePaint),
    Editor(crate::text_input::EditorPaint),
    Shape(crate::shapes::ShapePaint),
    Canvas(crate::canvas::CanvasPicture),
    Effect(crate::effects::EffectPaint),
    Shader(crate::shader_effect::ShaderEffectPaint),
    Source(crate::shader_effect::SourcePaint),
    External(Rc<dyn crate::paint_item::ItemPaint>),
}

/// Dirty state written by effects and drained by `frame`.
#[derive(Default)]
struct Shared {
    geom: FxHashMap<ObjectId, Geom>,
    place: FxHashMap<ObjectId, Place>,
    /// Items whose place may have changed, read at the frame: the place
    /// effect is told rather than run (see [`QuickScene::adopt`]). An item
    /// is here once for each flush that told it, read once
    /// ([`Entry::place_read`]): a set cost each item that moved a hash
    /// insert every frame.
    placed: Vec<ObjectId>,
    /// The map of a `transform` list, from [`list_effect`].
    list: FxHashMap<ObjectId, crate::transform::Projective>,
    content: FxHashMap<ObjectId, Paint>,
    structure: FxHashSet<ObjectId>,
    polish: FxHashSet<ObjectId>,
    dead: Vec<ObjectId>,
    /// A ShaderEffectSource inside its own source took a picture that
    /// differs from the last, so the next frame takes another.
    pictures: bool,
    /// Set from the first mark until the frame that drains it.
    dirty: bool,
    waker: Option<Rc<dyn Fn()>>,
}

impl Shared {
    fn pending(&self) -> bool {
        !(self.geom.is_empty()
            && self.place.is_empty()
            && self.placed.is_empty()
            && self.list.is_empty()
            && self.content.is_empty()
            && self.structure.is_empty()
            && self.polish.is_empty()
            && self.dead.is_empty()
            && !self.pictures)
    }
}

type SharedRc = Rc<RefCell<Shared>>;

/// What a `ShaderEffectSource` was last given of its source.
struct SourcePicture {
    /// The fingerprint of the snapshot it was made from, which is held as
    /// `effect_base`'s are.
    print: u64,
    _snap: FrameSnapshot,
    image: Option<ImageHandle>,
    /// The `scheduleUpdate()` count it was taken for.
    update: i32,
}

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

/// [`mark`] for the engine's wake, which may come while the scene itself
/// is at work (an object destroyed): then the frame under way sees what
/// is due.
fn wake(shared: &SharedRc) {
    let waker = {
        let Ok(mut s) = shared.try_borrow_mut() else {
            return;
        };
        if std::mem::replace(&mut s.dirty, true) {
            None
        } else {
            s.waker.clone()
        }
    };
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
    /// The last geometry and place applied; the node's transform takes
    /// both, and either may change alone.
    geom: Option<Geom>,
    place: Place,
    /// The map of the item's `transform` list, and the effect that follows
    /// it: made only for an item whose list has entries.
    list: crate::transform::Projective,
    list_effect: Option<GraphNode>,
    /// The [`QuickScene::places_read`] that last read its place.
    place_read: u64,
    /// Adopted under a Flickable, so it may be its content item: that one
    /// is the Flickable's child from when it is made. No other item asks
    /// whether its Flickable moves.
    may_scroll: bool,
}

/// Where an item is in its parent: `x` and `y`, and whether it is drawn
/// on the nearest whole device pixel (the content of a moving Flickable:
/// a scrolled frame then moves what the window shows by whole pixels).
type Place = (f64, f64, bool);

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
    /// The buttons held after the last pointer event.
    held: MouseButtons,
    /// The `Qt.CursorShape` the pointer shows.
    cursor: i32,
}

/// An item tree, its render tree, and the frame pipeline between them.
pub struct QuickScene {
    tree: RenderTree,
    shared: SharedRc,
    #[allow(deprecated)]
    log: QuickLog,
    entries: FxHashMap<ObjectId, Entry>,
    by_node: SecondaryMap<NodeId, ObjectId>,
    /// `Shared::placed` of the last frame, kept for its capacity.
    placed: Vec<ObjectId>,
    /// How many frames read places.
    places_read: u64,
    /// The items whose place was read this frame, kept for its capacity.
    told_moved: Vec<ObjectId>,
    paint: FxHashMap<ObjectId, Paint>,
    /// Each `MultiEffect` as read, before it is given the picture of a
    /// source with children, and a fingerprint of the picture it was given
    /// with what it was made from (held, so no display list it names is
    /// freed and its address taken by another).
    effect_base: HashMap<ObjectId, (crate::effects::EffectPaint, Option<(u64, FrameSnapshot)>)>,
    /// Each `ShaderEffectSource` as read, and the fingerprint, snapshot
    /// and picture of its source it was last given.
    source_base: HashMap<ObjectId, (crate::shader_effect::SourcePaint, Option<SourcePicture>)>,
    /// Each item whose layer a `ShaderEffect` samples, as it is sampled
    /// (showing its picture), and what it was last taken from.
    layer_base: HashMap<ObjectId, (crate::shader_effect::SourcePaint, Option<SourcePicture>)>,
    /// The source items a `ShaderEffectSource` hides (`hideSource`): not
    /// shown in the tree, whatever `visible` says.
    hidden: FxHashSet<ObjectId>,
    /// The rows views made and do not show (`views::cull`), and the
    /// generation of that they were read at.
    culled: (FxHashSet<ObjectId>, u64),
    root: Option<ObjectId>,
    /// The layer above the root item (`crate::layer`), once one was made.
    layer: Option<ObjectId>,
    viewport: kurbo::Rect,
    scale_factor: f32,
    frame: u64,
    snapshot: Option<Arc<FrameSnapshot>>,
    /// The painted nodes and the item generation the content effects were
    /// last matched to. A frame that changes what nodes paint but not
    /// which leaves them as they are, without a look at every item.
    content_synced: (Vec<NodeId>, u64),
    /// Moves when an item joins or leaves the scene, or an effect is read.
    items_gen: u64,
    /// Items the content effects were matched against, for tests.
    content_looked: u64,
    /// The items last matched as shown: with the same items, a frame
    /// that scrolls looks only at those that came or went.
    content_shown: FxHashSet<ObjectId>,
    /// The items under an effect's source, and the item generation and
    /// sources they were found at.
    under_sources: (u64, Vec<ObjectId>, FxHashSet<ObjectId>),
    /// The items with children, and the item generation they were read at.
    parents: (u64, FxHashSet<ObjectId>),
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
    /// Tells this scene's display scale from another's in the picture
    /// store, which makes drawings at the largest.
    id: u64,
    /// The engine's platform, for the same reason: answers of dialogs,
    /// downloads and the settings writer wait there.
    platform: Option<Rc<ljos_platform::Platform>>,
    /// Whether it hears what Animators show (once per scene).
    watching_shown: bool,
    /// The root [`QuickScene::replace_root`] lets go of, while it does:
    /// what it owns is destroyed next and is not given back what the
    /// scene gave it.
    going: Option<ObjectId>,
}

impl Drop for QuickScene {
    fn drop(&mut self) {
        if let Some(images) = &self.images {
            images.forget_scene(self.id);
        }
    }
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
            entries: FxHashMap::default(),
            by_node: SecondaryMap::new(),
            placed: Vec::new(),
            places_read: 0,
            told_moved: Vec::new(),
            paint: FxHashMap::default(),
            effect_base: HashMap::new(),
            source_base: HashMap::new(),
            layer_base: HashMap::new(),
            hidden: FxHashSet::default(),
            culled: (FxHashSet::default(), 0),
            root: None,
            layer: None,
            viewport: kurbo::Rect::from_origin_size((0.0, 0.0), viewport),
            scale_factor: 1.0,
            frame: 0,
            snapshot: None,
            content_synced: (Vec::new(), 0),
            items_gen: 1,
            content_looked: 0,
            content_shown: FxHashSet::default(),
            under_sources: (0, Vec::new(), FxHashSet::default()),
            parents: (0, FxHashSet::default()),
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
            id: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            platform: None,
            watching_shown: false,
            going: None,
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
        if !std::mem::replace(&mut self.watching_shown, true) {
            let shared = Rc::downgrade(&self.shared);
            crate::animation::shown::watch(engine, move |e, obj, prop| {
                let Some(s) = shared.upgrade() else {
                    return false;
                };
                e.untrack(|| shown_changed(e, obj, prop, &s));
                true
            });
        }
        self.root = Some(root);
        // The scene is the window: its focus item gets active focus.
        crate::focus::tree::add_window(engine, root);
        // canPaste hears of other applications' copies when the window
        // comes back or an editor gets the keyboard.
        crate::Clipboard::follow_window(engine, root);
        // A Timer started from a handler changes nothing visible, so
        // without this the host would never ask for its deadline.
        let shared = Rc::downgrade(&self.shared);
        engine.set_timer_wake(move || {
            if let Some(s) = shared.upgrade() {
                wake(&s);
            }
        });
        Ok(())
    }

    /// [`QuickScene::set_root`] for a host that destroys the old root
    /// right after (a reload): the visibility and enabled state the scene
    /// handed down to the objects that root owns are not put back, which
    /// would run their bindings once more. Objects it shows but does not
    /// own get theirs back as with `set_root`.
    pub fn replace_root(&mut self, engine: &Engine, root: ObjectId) -> Result<(), EngineError> {
        self.going = self.root;
        let r = self.set_root(engine, root);
        self.going = None;
        r
    }

    pub fn root(&self) -> Option<ObjectId> {
        self.root
    }

    /// Makes this scene the one the engine's timers wake. An engine has
    /// one such hook and [`QuickScene::set_root`] takes it for the scene
    /// it is called on: a host that shows several scenes of one engine
    /// gives it back to the scene whose frames always run.
    pub fn take_timer_wake(&self, engine: &Engine) {
        let shared = Rc::downgrade(&self.shared);
        engine.set_timer_wake(move || {
            if let Some(s) = shared.upgrade() {
                wake(&s);
            }
        });
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

    /// Gives back memory the scene can rebuild (ARCHITECTURE §8.5): the
    /// canvases' copies of images and glyph outlines, and decoded pictures
    /// nothing shows. What is on screen stays. Returns the bytes of
    /// pictures freed.
    pub fn trim(&mut self, engine: &Engine, pressure: ljos_runtime::MemoryPressure) -> usize {
        let canvases = self
            .entries
            .iter()
            .filter(|(_, e)| e.kind == Kind::Canvas)
            .map(|(id, _)| *id);
        crate::canvas::trim(engine, canvases);
        crate::image::ImageStore::of(engine).trim(pressure)
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
        // Pictures are taken in device pixels.
        for (_, seen) in self
            .source_base
            .values_mut()
            .chain(self.layer_base.values_mut())
        {
            *seen = None;
        }
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

    /// Lays out what waits for it (text, positioners, layouts) without a
    /// frame: no time passes and nothing is painted. A host that sizes its
    /// window from the root calls it first, since a Text knows its implicit
    /// size only once it is laid out.
    pub fn lay_out(&mut self, engine: &Engine) {
        self.follow_text_engine();
        self.polish(engine);
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
        self.by_node.get(node).copied()
    }

    /// Items the frames so far matched paint effects against.
    #[doc(hidden)]
    pub fn content_effects_looked_at(&self) -> u64 {
        self.content_looked
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
            || self.platform.as_ref().is_some_and(|p| p.has_answers())
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
        {
            let mut s = self.shared.borrow_mut();
            s.dirty = true;
            s.pictures = false;
        }
        self.follow_text_engine();
        // Pictures decoded in the background since the last frame.
        let images = crate::image::ImageStore::of(engine);
        images.deliver(engine);
        images.show_at(
            engine,
            self.id,
            self.scale_factor,
            self.viewport.width().max(self.viewport.height()),
        );
        self.images = Some(images);
        // Answers of the platform's services (dialogs, downloads).
        let platform = ljos_platform::Platform::of(engine);
        platform.deliver(engine);
        self.platform = Some(platform);
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
        crate::long_press::tick(engine, now);
        crate::flickable::fling::tick(engine, now);
        crate::canvas::tick(engine);
        crate::views::path_view::tick(engine, now);
        // 6. Polish.
        self.polish(engine);
        // 7. Incubate. New items join the tree and are laid out in this
        // same frame, so what is shown is never half placed.
        let incubating = self.incubate(engine);
        // 8. Sync.
        self.reconcile(engine);
        self.apply_geometry(engine);
        self.refresh_culled(engine);
        self.apply_content();
        self.draw_effect_sources(engine);
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
            self.draw_effect_sources(engine);
            snap = self.tree.compose(self.viewport, self.frame);
        }
        let changed = !self
            .snapshot
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &snap));
        self.snapshot = Some(Arc::clone(&snap));
        // Last, so the tree describes the items as this frame left them.
        self.a11y.set_viewport(self.viewport);
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
                || engine.has_due_calls()
                || engine.has_deferred_deletes()
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
                let fling_wake = crate::flickable::fling::next_wake(engine, now);
                let canvas_wake = crate::canvas::next_wake(engine, now);
                let path_view_wake = crate::views::path_view::next_wake(engine, now);
                let held_wake = crate::long_press::next_wake(
                    engine,
                    now,
                    changed,
                    crate::animation::frame_period(engine),
                );
                [
                    engine_wake,
                    self.next_timer,
                    animation_wake,
                    fling_wake,
                    canvas_wake,
                    path_view_wake,
                    held_wake,
                ]
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
        crate::views::incubation_begins(engine);
        let report = self.incubator.run_frame(&mut (), budget, &*clock);
        crate::views::incubation_ends(engine);
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
        let (mut passes, mut rows, mut free) = (0, 0, false);
        while passes < MAX_POLISH_PASSES {
            // Items made since the last sync (a view's delegates) join the
            // tree first, so that they are laid out in this polish and not
            // a frame later.
            if !self.shared.borrow().structure.is_empty() {
                self.reconcile(engine);
            }
            // Counted per pass: a view measures a row it made once a pass
            // has run since, and makes the next one then (`Fill::wait_for`).
            POLISHES.set(POLISHES.get() + 1);
            if !free {
                passes += 1;
            }
            free = false;
            let mut set: Vec<(usize, ObjectId)> = {
                let mut s = self.shared.borrow_mut();
                s.polish
                    .drain()
                    .filter(|o| self.entries.contains_key(o))
                    .map(|o| (depth(engine, o), o))
                    .collect()
            };
            if set.is_empty() {
                // The views that waited for the items in their delegates
                // lay their rows out now; what they make is polished next.
                POLISHING.set(false);
                let laid = crate::views::polish_ends(engine);
                {
                    let s = self.shared.borrow();
                    if s.polish.is_empty() && s.structure.is_empty() {
                        return;
                    }
                }
                // A view that made a row lays it out in the next pass and
                // then makes the one after it, as Qt does: those passes
                // fill a window and are not a loop.
                if laid && rows < MAX_ROW_PASSES {
                    rows += 1;
                    free = true;
                }
                continue;
            }
            POLISHING.set(true);
            // Deepest first, so a positioner sees its children's final size.
            // Items of one depth go in creation order: the set's own order
            // changes from run to run, and two layouts that share an item
            // (LayoutItemProxy) give it the size of whichever runs last.
            set.sort_by_key(|a| (std::cmp::Reverse(a.0), a.1));
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
                    Kind::Canvas => crate::canvas::polish(engine, obj),
                    Kind::Item
                    | Kind::Rectangle
                    | Kind::Image
                    | Kind::Shape
                    | Kind::Effect
                    | Kind::Shader
                    | Kind::Source
                    | Kind::External => Ok(()),
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
        POLISHING.set(false);
        crate::views::polish_ends(engine);
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
        let may_scroll = parent.is_some_and(|p| is_a::<crate::flickable::Flickable>(engine, p));
        if kind == Kind::Editor {
            crate::text_input::init(engine, obj)?;
        }
        let anchors = Rc::new(RefCell::new(AnchorState::default()));
        let inherit = Rc::new(RefCell::new(Inherit::default()));
        self.items_gen += 1;
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
                geom: None,
                place: (0.0, 0.0, false),
                list: crate::transform::Projective::IDENTITY,
                list_effect: None,
                place_read: 0,
                may_scroll,
            },
        );
        self.by_node.insert(node, obj);

        let mut effects = Vec::with_capacity(6);
        let shared = Rc::clone(&self.shared);
        // Root-scoped: an effect scoped to `obj` dies with it silently, and
        // this one has to see the death to drop the render node.
        effects.push(engine.effect(move |e| structure_effect(e, obj, &shared))?);
        let lifted = Rc::clone(&self.shared);
        effects.push(engine.effect_in(obj, move |e| inherit_effect(e, obj, &inherit, &lifted))?);
        effects.push(engine.effect_in(obj, move |e| anchors::run(e, obj, &anchors))?);
        // What an Animator shows is one table for the engine: looked up
        // here, not on each of the runs that follow an animated value.
        let shown = crate::animation::shown::shown(engine);
        let (place, shared) = (Rc::clone(&shown), Rc::clone(&self.shared));
        // Told, not run, on a change: an item that moves every frame then
        // costs the graph no run, and its place is read once, at the frame.
        let told = Rc::clone(&self.shared);
        effects.push(engine.effect_told_in(
            obj,
            move |e| place_effect(e, obj, may_scroll, &place, &shared),
            move || {
                mark(&told, |s| {
                    s.placed.push(obj);
                })
            },
        )?);
        let shared = Rc::clone(&self.shared);
        effects.push(engine.effect_in(obj, move |e| geometry_effect(e, obj, &shown, &shared))?);
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
        self.items_gen += 1;
        let g = engine.graph();
        for n in entry
            .effects
            .iter()
            .chain(entry.content_effect.iter())
            .chain(entry.list_effect.iter())
        {
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
            let restore = RestoreMode::BindingOrValue;
            let mut going = None;
            for (p, slot) in [
                (Item::VISIBLE.index, &mut inh.visible),
                (Item::ENABLED.index, &mut inh.enabled),
            ] {
                if let Some(id) = slot.take() {
                    let goes = *going.get_or_insert_with(|| {
                        self.going.is_some_and(|root| owned_by(engine, obj, root))
                    });
                    if !goes {
                        let _ = engine.remove_override(obj, p, id, restore);
                    }
                }
            }
        }
        let kids: Vec<NodeId> = self
            .tree
            .node(entry.node)
            .map(|n| n.children().to_vec())
            .unwrap_or_default();
        for k in kids {
            if let Some(c) = self.by_node.get(k).copied() {
                self.unadopt(engine, c, false);
            }
        }
        self.by_node.remove(entry.node);
        self.paint.remove(&obj);
        if remove_node {
            let _ = self.tree.remove(entry.node);
        }
        let mut s = self.shared.borrow_mut();
        s.geom.remove(&obj);
        s.place.remove(&obj);
        // Its entry is gone, so a place told is not read.
        s.list.remove(&obj);
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
        let mut dirty: Vec<ObjectId> = self.shared.borrow_mut().structure.drain().collect();
        // In creation order, so the same scene builds the same way each run.
        dirty.sort_unstable();
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
            let mut seen = FxHashSet::default();
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
                        if ce.parent != Some(p) {
                            ce.parent = Some(p);
                            self.items_gen += 1;
                        }
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
                    if let Some(o) = self.by_node.get(*extra) {
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
    /// Reads the place of each item the place effect was told about.
    /// Unborrowed: a binding a read brings up to date may set off effects
    /// that mark the scene.
    fn read_places(&mut self, engine: &Engine) {
        std::mem::swap(&mut self.placed, &mut self.shared.borrow_mut().placed);
        if self.placed.is_empty() {
            return;
        }
        let shown = crate::animation::shown::shown(engine);
        self.places_read += 1;
        // Set on the entry as it is read: by way of `Shared::place` each
        // moved item cost an insert, a drain and a look more, every frame.
        for obj in self.placed.drain(..) {
            let Some(e) = self.entries.get_mut(&obj) else {
                continue;
            };
            if std::mem::replace(&mut e.place_read, self.places_read) == self.places_read {
                continue;
            }
            // Both at one look at the object, which also says it lives.
            if let Some([x, y]) = engine.read_numbers(obj, [Item::X.index, Item::Y.index]) {
                e.place = (
                    shown.get(obj, Item::X.index).unwrap_or(x),
                    shown.get(obj, Item::Y.index).unwrap_or(y),
                    e.may_scroll && crate::flickable::content_moving(engine, obj),
                );
                // Moved here, while the entry is at hand: a new geometry or
                // transform list this frame sets it again from this place.
                if let Some(g) = &e.geom {
                    self.tree.set_local(
                        e.node,
                        local_transform(e.place, g, &e.list, self.scale_factor),
                    );
                }
                self.told_moved.push(obj);
            }
        }
        // A place the effect's first run gave this frame is older than
        // the one just read.
        let mut s = self.shared.borrow_mut();
        if !s.place.is_empty() {
            for obj in &self.told_moved {
                s.place.remove(obj);
            }
        }
    }

    fn apply_geometry(&mut self, engine: &Engine) {
        self.read_places(engine);
        // Each item's tree node is set on its own, so neither list needs
        // an order; what only moved is told apart while the maps stand.
        let (geoms, moved) = {
            let mut s = self.shared.borrow_mut();
            let s = &mut *s;
            let mut moved = Vec::with_capacity(self.told_moved.len());
            // Before the places, so an item that also moved is told once.
            for (obj, list) in s.list.drain() {
                if let Some(e) = self.entries.get_mut(&obj) {
                    e.list = list;
                    if !s.geom.contains_key(&obj) && !s.place.contains_key(&obj) {
                        moved.push(obj);
                    }
                }
            }
            for (obj, place) in s.place.drain() {
                if let Some(e) = self.entries.get_mut(&obj) {
                    e.place = place;
                    if !s.geom.contains_key(&obj) {
                        moved.push(obj);
                    }
                }
            }
            // Each was moved as its place was read.
            self.told_moved.clear();
            let geoms: Vec<(ObjectId, Geom)> = s.geom.drain().collect();
            (geoms, moved)
        };
        for (obj, g) in &geoms {
            if self
                .entries
                .get(obj)
                .is_some_and(|e| e.list_effect.is_some() != g.listed)
            {
                self.follow_list(engine, *obj, g.listed);
            }
        }
        for (obj, g) in geoms {
            let Some(e) = self.entries.get_mut(&obj) else {
                continue;
            };
            e.geom = Some(g);
            let node = e.node;
            let size = kurbo::Size::new(g.w.max(0.0), g.h.max(0.0));
            self.tree.set_local(
                node,
                local_transform(e.place, &g, &e.list, self.scale_factor),
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
            // An item that reads false only because a parent is hidden is
            // left shown in the tree: the parent prunes it anyway, and a
            // `MultiEffect` drawing that parent shows it as Qt does.
            let covered = e.inherit.borrow().visible.is_some();
            self.tree
                .set_visible(node, (g.visible || covered) && !g.faces_away);
            self.tree.set_drawn(node, self.drawn(obj));
            self.tree
                .set_clip(node, g.clip.then(|| ClipSpec::bounds(size)));
            if let Some(Paint::Editor(ed)) = self.paint.get(&obj) {
                self.tree
                    .set_content_clip(node, ed.clip(size).map(ClipSpec::rect));
            }
            self.tree.set_z(node, g.z as f32);
        }
        // What only moved or turned: its transform, and nothing else.
        for obj in moved {
            if let Some(e) = self.entries.get(&obj)
                && let Some(g) = &e.geom
            {
                self.tree.set_local(
                    e.node,
                    local_transform(e.place, g, &e.list, self.scale_factor),
                );
            }
        }
    }

    /// Makes or drops the effect that follows `obj`'s `transform` list,
    /// as the list came to have entries or lost them.
    fn follow_list(&mut self, engine: &Engine, obj: ObjectId, listed: bool) {
        let Some(e) = self.entries.get_mut(&obj) else {
            return;
        };
        if let Some(n) = e.list_effect.take() {
            let _ = engine.graph().dispose(n);
        }
        let list = if listed {
            let shared = Rc::clone(&self.shared);
            match engine.effect_in(obj, move |e| list_effect(e, obj, &shared)) {
                Ok(n) => e.list_effect = Some(n),
                Err(err) => engine.log(
                    LogLevel::Warning,
                    SCENE_CATEGORY,
                    format!("transform effect failed: {err}"),
                ),
            }
            // The effect ran as it was made, unless a batch holds it back:
            // this frame shows the list either way.
            self.shared
                .borrow_mut()
                .list
                .remove(&obj)
                .unwrap_or_else(|| engine.untrack(|| crate::transform::list_map(engine, obj)))
        } else {
            self.shared.borrow_mut().list.remove(&obj);
            crate::transform::Projective::IDENTITY
        };
        e.list = list;
    }

    /// Step 8c: rebuild the display lists of items whose paint inputs or
    /// size changed.
    fn apply_content(&mut self) {
        let mut content: Vec<(ObjectId, Paint)> =
            self.shared.borrow_mut().content.drain().collect();
        content.sort_unstable_by_key(|c| c.0);
        let mut sources: FxHashSet<ObjectId> = FxHashSet::default();
        let mut shaders = false;
        for (obj, p) in content {
            if let Some(e) = self.entries.get(&obj) {
                self.tree.mark_content_dirty(e.node);
                let p = match p {
                    Paint::Effect(fx) => {
                        self.items_gen += 1;
                        self.effect_base.insert(obj, (fx.clone(), None));
                        Paint::Effect(fx)
                    }
                    // What it was given of the source stays: one that is
                    // not live keeps it, and one that is takes it again
                    // only when the source changed.
                    Paint::Source(src) => {
                        self.items_gen += 1;
                        sources.insert(obj);
                        let seen = self.source_base.remove(&obj).and_then(|(_, seen)| seen);
                        let shown = src.with_picture(
                            seen.as_ref().and_then(|s| s.image.clone()),
                            self.scale_factor,
                        );
                        self.source_base.insert(obj, (src, seen));
                        Paint::Source(shown)
                    }
                    Paint::Shader(fx) => {
                        shaders = true;
                        for (item, how) in fx.layers() {
                            let had = self.layer_base.remove(&item);
                            // A layer newly sampled draws its item and the
                            // items in it even while hidden.
                            if had.is_none() {
                                self.items_gen += 1;
                            }
                            let seen = had.and_then(|(_, seen)| seen);
                            let shown = how.with_picture(
                                seen.as_ref().and_then(|s| s.image.clone()),
                                self.scale_factor,
                            );
                            self.layer_base.insert(item, (shown, seen));
                        }
                        Paint::Shader(fx)
                    }
                    p => p,
                };
                if let Paint::Editor(ed) = &p {
                    let clip = ed.clip(e.size).map(ClipSpec::rect);
                    self.tree.set_content_clip(e.node, clip);
                }
                self.paint.insert(obj, p);
            }
        }
        // A layer no effect samples any more is no longer taken.
        if shaders {
            let paint = &self.paint;
            let entries = &self.entries;
            let sampled: FxHashSet<ObjectId> = paint
                .iter()
                .filter(|(o, _)| entries.contains_key(o))
                .filter_map(|(_, p)| match p {
                    Paint::Shader(fx) => Some(fx.layers().map(|(item, _)| item)),
                    _ => None,
                })
                .flatten()
                .collect();
            let had = self.layer_base.len();
            self.layer_base.retain(|o, _| sampled.contains(o));
            if self.layer_base.len() != had {
                self.items_gen += 1;
            }
        }
        // What a ShaderEffect samples of one is read from the scene's
        // paint, so a change of how it is sampled paints the effect again.
        if !sources.is_empty() {
            for (obj, p) in &self.paint {
                if let Paint::Shader(fx) = p
                    && fx.sources().any(|s| sources.contains(&s))
                    && let Some(e) = self.entries.get(obj)
                {
                    self.tree.mark_content_dirty(e.node);
                }
            }
        }
        self.refresh_hidden();
        let paint = &self.paint;
        let layers = &self.layer_base;
        let by_node = &self.by_node;
        let mut te = self.text.borrow_mut();
        self.tree.rebuild_content(self.scale_factor, |cx, b| {
            let Some(p) = by_node.get(cx.node).and_then(|o| paint.get(o)) else {
                return;
            };
            match p {
                Paint::Rect(r) => r.paint(cx.size, b),
                Paint::Text(t) => text::paint(&mut te, t, cx.size, b),
                Paint::Image(i) => i.paint(cx.size, b),
                Paint::Editor(p) => p.paint(cx.size, b),
                Paint::Shape(s) => s.paint(cx.size, b),
                Paint::Canvas(c) => c.paint(cx.size, b),
                Paint::Effect(p) => p.paint(cx.size, b),
                Paint::Shader(p) => p.paint(
                    cx.size,
                    |s| match (paint.get(&s), layers.get(&s)) {
                        (Some(Paint::Source(src)), _) => Some(src.texture()),
                        (_, Some((layer, _))) => Some(layer.texture()),
                        _ => None,
                    },
                    b,
                ),
                Paint::Source(p) => p.paint(cx.size, cx.scale_factor, b),
                Paint::External(p) => p.paint(cx.size, &mut te, b),
            }
        });
    }

    /// Gives each `MultiEffect` whose source has children a picture of
    /// the source with them, as Qt renders the whole item into the
    /// texture the effect samples. The source itself stays where it is:
    /// "MultiEffect type renders a new visual item alongside the source
    /// item", and hiding it is the user's choice. Drawn again only when
    /// what the source shows changed.
    fn draw_effect_sources(&mut self, engine: &Engine) {
        if self.effect_base.is_empty() && self.source_base.is_empty() && self.layer_base.is_empty()
        {
            return;
        }
        let entries = &self.entries;
        self.effect_base.retain(|o, _| entries.contains_key(o));
        let redrawn = self.draw_shader_sources();
        if self.effect_base.is_empty() {
            if redrawn {
                self.apply_content();
            }
            return;
        }
        // Who has children changes only with the items, not every frame.
        if self.parents.0 != self.items_gen {
            let parents = self.entries.values().filter_map(|e| e.parent).collect();
            self.parents = (self.items_gen, parents);
        }
        let parents = &self.parents.1;
        let mut redrawn = redrawn;
        for (obj, (base, seen)) in &mut self.effect_base {
            let Some(src) = base.source().filter(|s| parents.contains(s)) else {
                continue;
            };
            let (Some(src_entry), Some(fx)) = (self.entries.get(&src), self.entries.get(obj))
            else {
                continue;
            };
            let Some(snap) = self.tree.compose_subtree(src_entry.node) else {
                continue;
            };
            let print = fingerprint(&snap);
            if seen.as_ref().is_some_and(|(p, _)| *p == print) {
                continue;
            }
            let paint = match crate::effects::picture(&snap) {
                Some(img) => base.with_source(engine, img),
                None => base.clone(),
            };
            *seen = Some((print, snap));
            self.paint.insert(*obj, Paint::Effect(paint));
            self.tree.mark_content_dirty(fx.node);
            redrawn = true;
        }
        if redrawn {
            self.apply_content();
        }
    }

    /// Takes the picture of each `ShaderEffectSource`'s source that
    /// changed, unless it is not `live` and has one (and `scheduleUpdate()`
    /// was not called since), and marks the `ShaderEffect`s that sample it.
    /// Returns whether any was taken.
    ///
    /// A source that holds the ShaderEffectSource changes with each picture
    /// it is given, so it is drawn again in the next frame, until the
    /// picture comes out the same: a tunnel that ends where its pictures
    /// are smaller than a pixel, and then costs nothing.
    fn draw_shader_sources(&mut self) -> bool {
        let entries = &self.entries;
        self.source_base.retain(|o, _| entries.contains_key(o));
        let mut taken: FxHashSet<ObjectId> = FxHashSet::default();
        let mut again = false;
        for (obj, (base, seen)) in &mut self.source_base {
            let asked = seen.as_ref().is_some_and(|s| s.update != base.update);
            if !base.live && seen.is_some() && !asked {
                continue;
            }
            let (Some(fx), Some(src)) = (self.entries.get(obj), base.source) else {
                continue;
            };
            let Some(src_entry) = self.entries.get(&src) else {
                continue;
            };
            let Some(snap) = self.tree.compose_subtree(src_entry.node) else {
                continue;
            };
            let print = fingerprint(&snap);
            if !asked && seen.as_ref().is_some_and(|s| s.print == print) {
                continue;
            }
            let picture = crate::effects::picture_at(&snap, self.scale_factor);
            let same = seen.as_ref().is_some_and(|s| match (&s.image, &picture) {
                (Some(a), Some(b)) => {
                    a.width() == b.width() && a.height() == b.height() && *a.data() == *b.data()
                }
                (None, None) => true,
                _ => false,
            });
            if same && let Some(s) = seen {
                s.print = print;
                s._snap = snap;
                s.update = base.update;
                continue;
            }
            let inside = {
                let mut at = Some(*obj);
                std::iter::from_fn(|| {
                    let o = at?;
                    at = self.entries.get(&o).and_then(|e| e.parent);
                    Some(o)
                })
                .any(|o| o == src)
            };
            again |= inside;
            let shown = base.with_picture(picture.clone(), self.scale_factor);
            *seen = Some(SourcePicture {
                print,
                _snap: snap,
                image: picture,
                update: base.update,
            });
            self.paint.insert(*obj, Paint::Source(shown));
            self.tree.mark_content_dirty(fx.node);
            taken.insert(*obj);
        }
        taken.extend(self.draw_layers());
        if again {
            mark(&self.shared, |s| s.pictures = true);
        }
        if taken.is_empty() {
            return false;
        }
        for (obj, p) in &self.paint {
            if let Paint::Shader(fx) = p
                && fx.sources().any(|s| taken.contains(&s))
                && let Some(e) = self.entries.get(obj)
            {
                self.tree.mark_content_dirty(e.node);
            }
        }
        true
    }

    /// Takes the picture of each sampled layer whose item changed, unless
    /// the layer is not `live` and has one. Returns the items taken.
    fn draw_layers(&mut self) -> Vec<ObjectId> {
        let mut taken = Vec::new();
        for (item, (shown, seen)) in &mut self.layer_base {
            if !shown.live && seen.is_some() {
                continue;
            }
            let Some(entry) = self.entries.get(item) else {
                continue;
            };
            let Some(snap) = self.tree.compose_subtree(entry.node) else {
                continue;
            };
            let print = fingerprint(&snap);
            if seen.as_ref().is_some_and(|s| s.print == print) {
                continue;
            }
            let picture = crate::effects::picture_at(&snap, self.scale_factor);
            *shown = shown.with_picture(picture.clone(), self.scale_factor);
            *seen = Some(SourcePicture {
                print,
                _snap: snap,
                image: picture,
                update: 0,
            });
            taken.push(*item);
        }
        taken
    }

    /// Stops painting the source items `hideSource` names, and paints again
    /// those it no longer does. They still take the pointer, as in Qt:
    /// "setting this property to true will not prevent mouse or keyboard
    /// input from reaching the source item".
    fn refresh_hidden(&mut self) {
        let now: FxHashSet<ObjectId> = self
            .source_base
            .iter()
            .filter(|(o, (b, _))| b.hide && self.entries.contains_key(o))
            .filter_map(|(_, (b, _))| b.source)
            .collect();
        if now == self.hidden {
            return;
        }
        let changed: Vec<ObjectId> = now.symmetric_difference(&self.hidden).copied().collect();
        self.hidden = now;
        for obj in changed {
            if let Some(e) = self.entries.get(&obj) {
                self.tree.set_drawn(e.node, self.drawn(obj));
            }
        }
    }

    /// Whether `obj` is painted, `visible` aside.
    fn drawn(&self, obj: ObjectId) -> bool {
        !self.hidden.contains(&obj) && !self.culled.0.contains(&obj)
    }

    /// Stops painting the rows views made and do not show, and paints
    /// again those they show now.
    fn refresh_culled(&mut self, engine: &Engine) {
        let generation = crate::views::cull::generation(engine);
        if generation == self.culled.1 {
            return;
        }
        let now = crate::views::cull::all(engine);
        let changed: Vec<ObjectId> = now.symmetric_difference(&self.culled.0).copied().collect();
        self.culled = (now, generation);
        for obj in changed {
            if let Some(e) = self.entries.get(&obj) {
                self.tree.set_drawn(e.node, self.drawn(obj));
            }
        }
    }

    /// The items that are drawn as part of a `MultiEffect`'s or a
    /// `ShaderEffectSource`'s source: the source and what is below it,
    /// shown or not.
    fn effect_drawn(&mut self) -> &FxHashSet<ObjectId> {
        let mut sources: Vec<ObjectId> = self
            .effect_base
            .values()
            .filter_map(|(b, _)| b.source())
            .chain(self.source_base.values().filter_map(|(b, _)| b.source))
            .chain(self.layer_base.keys().copied())
            .collect();
        sources.sort_unstable();
        sources.dedup();
        // A parent changes only with the item generation: the walk up from
        // every item is done again only when that or the sources change.
        if self.under_sources.0 == self.items_gen && self.under_sources.1 == sources {
            return &self.under_sources.2;
        }
        let drawn = self.under(&sources);
        self.under_sources = (self.items_gen, sources, drawn);
        &self.under_sources.2
    }

    /// `sources` and the items below them.
    fn under(&self, sources: &[ObjectId]) -> FxHashSet<ObjectId> {
        if sources.is_empty() {
            return FxHashSet::default();
        }
        self.entries
            .iter()
            .filter(|(obj, _)| {
                let mut at = Some(**obj);
                while let Some(o) = at {
                    if sources.binary_search(&o).is_ok() {
                        return true;
                    }
                    at = self.entries.get(&o).and_then(|e| e.parent);
                }
                false
            })
            .map(|(o, _)| *o)
            .collect()
    }

    /// Installs paint effects for painting items in `snap` and drops them
    /// for items that left it. Returns whether any were installed.
    fn sync_content_effects(&mut self, engine: &Engine, snap: &FrameSnapshot) -> bool {
        let (nodes, generation) = &self.content_synced;
        let same_items = *generation == self.items_gen;
        if same_items
            && nodes.len() == snap.items.len()
            && nodes.iter().zip(&snap.items).all(|(n, i)| *n == i.node)
        {
            return false;
        }
        let mut visible: FxHashSet<ObjectId> = snap
            .items
            .iter()
            .filter_map(|i| self.by_node.get(i.node).copied())
            .collect();
        // A hidden source is still drawn by its effect.
        visible.extend(self.effect_drawn().iter().copied());
        // With the same items, an item neither shown before nor now was
        // matched already: a scroll looks at the rows that came or went.
        let looked: Vec<ObjectId> = if same_items {
            visible
                .symmetric_difference(&self.content_shown)
                .copied()
                .collect()
        } else {
            self.entries.keys().copied().collect()
        };
        self.content_looked += looked.len() as u64;
        let mut installed = false;
        // A dropped display list changes the tree, so the frame composes
        // again and does not hand out a snapshot that is already stale.
        let mut dropped = false;
        let mut failed = Vec::new();
        for obj in looked {
            let Some(e) = self.entries.get_mut(&obj) else {
                continue;
            };
            if !e.kind.paints() {
                continue;
            }
            let shown = visible.contains(&obj);
            // A Shape paints where its paths are, which need not be where
            // the item is: only its display list tells whether it is in the
            // frame, so it has one from the start and keeps it.
            // A ShaderEffectSource is often not shown itself, only
            // sampled: it reads its source all the same.
            let shown = shown || matches!(e.kind, Kind::Shape | Kind::Source);
            match (shown, e.content_effect) {
                (true, None) => {
                    let shared = Rc::clone(&self.shared);
                    let (o, kind) = (obj, e.kind);
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
                        self.paint.remove(&obj);
                        self.tree.set_content(e.node, None);
                        dropped = true;
                    }
                    // A picture likewise: the display list is what keeps
                    // its pixels from being dropped under memory pressure
                    // (ARCHITECTURE §8.5).
                    if e.kind == Kind::Image {
                        self.paint.remove(&obj);
                        self.tree.set_content(e.node, None);
                        crate::image::hidden(engine, obj);
                        dropped = true;
                    }
                }
                _ => {}
            }
        }
        // What failed is tried again on the next frame.
        self.content_synced = if failed.is_empty() {
            (snap.items.iter().map(|i| i.node).collect(), self.items_gen)
        } else {
            (Vec::new(), 0)
        };
        self.content_shown = visible;
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
        // A wheel event tells the buttons that are held (`wheel.buttons`).
        self.pointer.held = match ev.kind {
            PointerKind::Press | PointerKind::Move | PointerKind::Release => ev.buttons,
            PointerKind::Cancel | PointerKind::Leave => MouseButtons::empty(),
        };
        let handled = self.deliver(engine, ev);
        self.pointer.cursor = self.cursor_for(engine, ev);
        handled
    }

    /// The `Qt.CursorShape` the pointer shows over the scene, as of the
    /// last pointer event: `Qt.ArrowCursor` (0) unless something under it
    /// says otherwise.
    pub fn cursor_shape(&self) -> i32 {
        self.pointer.cursor
    }

    /// The cursor after `ev`. While a button is held it stays as it was
    /// at the press, so a resize drag keeps it off the column's end.
    fn cursor_for(&self, engine: &Engine, ev: &PointerEvent) -> i32 {
        match ev.kind {
            PointerKind::Leave | PointerKind::Cancel => 0,
            _ if !ev.buttons.is_empty() => self.pointer.cursor,
            _ => {
                let hits = self.hits(engine, ev.pos);
                crate::views::table::resize_cursor(engine, &hits, |o| {
                    local_point(engine, o, ev.pos)
                })
                .unwrap_or(0)
            }
        }
    }

    fn deliver(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        match ev.kind {
            PointerKind::Press => self.press(engine, ev),
            PointerKind::Move => self.moved(engine, ev),
            PointerKind::Release => self.release(engine, ev),
            PointerKind::Cancel => {
                let handled = self.cancel(engine, ev);
                crate::handlers::canceled(engine);
                handled
            }
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
        self.wheel_event_held(engine, ev, self.pointer.held)
    }

    /// [`QuickScene::wheel_event`] for a host that knows which buttons are
    /// held without having sent the presses (a test that names them).
    pub fn wheel_event_held(
        &mut self,
        engine: &Engine,
        ev: &crate::input::WheelEvent,
        buttons: MouseButtons,
    ) -> bool {
        let hits = self.hits(engine, ev.pos);
        crate::wheel::deliver(engine, &hits, ev, buttons)
    }

    /// Offers a pinch the platform recognised (a touchpad's) to the
    /// `PinchHandler`s under it. Returns whether one took it.
    pub fn pinch_event(&mut self, engine: &Engine, g: &crate::handlers::PinchGesture) -> bool {
        let hits = self.hits(engine, g.center);
        crate::handlers::pinch_gesture(engine, g, &hits)
    }

    /// Items under `pos`, topmost first.
    fn hits(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
        let Some(snap) = &self.snapshot else {
            return Vec::new();
        };
        let mut hits: Vec<ObjectId> = hit_test(snap, pos)
            .filter_map(|n| self.by_node.get(n).copied())
            .filter(|o| engine.is_alive(*o))
            // A Shape that takes points by its fill lets the rest through.
            .filter(|o| crate::shapes::takes_pointer(engine, *o, || local_point(engine, *o, pos)))
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
        crate::flickable::drag::pressed(engine, ev.time);
        crate::handlers::pressed(engine, ev);
        let mut handled = false;
        let hits = self.hits(engine, ev.pos);
        // After the hits: a popup that closes on this press still keeps
        // it from what it covered.
        crate::layer::press(engine, self.layer, ev);
        // A press on content that is flung stops it, and is not a click
        // on what was flung past.
        if let Some(f) = crate::flickable::fling::catch(engine, &hits) {
            self.pointer.grab = Some(Grab::Item(f));
            return true;
        }
        // The end of a table's column or row the user resizes is the
        // table's, before the cells' over it.
        if button == MouseButtons::LEFT
            && let Some(v) =
                crate::views::table::catch_resize(engine, &hits, |o| local_point(engine, o, ev.pos))
        {
            self.pointer.grab = Some(Grab::Item(v));
            return true;
        }
        for obj in hits {
            // Drag and point handlers grab passively, and see it first.
            handled |= crate::handlers::press(engine, obj, ev);
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
                // As in Qt the point is there when `pressed` changes, and
                // says so after it.
                crate::input::tap_point_pressed(engine, h, ev);
                set(engine, h, TapHandler::PRESSED.index, Value::Bool(true));
                crate::input::tap_point_changed(engine, h);
                crate::long_press::pressed(engine, h, ev.time);
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
            if let Some(h) = crate::pointer_item::pointer_item(engine, obj) {
                // Areas around it that drag from their children hear it first.
                crate::mouse_drag::filter_press(engine, obj, ev);
                if (h.press)(engine, obj, ev, local_point(engine, obj, ev.pos)) {
                    self.pointer.grab = Some(Grab::Item(obj));
                    return true;
                }
                crate::mouse_drag::filter_undo(engine);
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
        // Areas around it that drag from their children hear it first.
        crate::mouse_drag::filter_press(engine, m, ev);
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
            crate::mouse_drag::filter_undo(engine);
            return false;
        }
        self.pointer.grab = Some(Grab::Mouse(m));
        crate::mouse_drag::pressed(engine, m, ev);
        crate::press_and_hold::pressed(engine, m);
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
        // The layer sees a drag before anything under it does: a drawer
        // dragged in from the edge takes the pointer from what was pressed
        // there, passive grabs included.
        if !ev.buttons.is_empty()
            && let Some(o) = crate::layer::take(engine, self.layer, self.pointer.press_pos, ev)
            && self.pointer.grab != Some(Grab::Item(o))
        {
            self.cancel(engine, ev);
            crate::handlers::canceled(engine);
            self.pointer.grab = Some(Grab::Item(o));
        }
        // A pointer handler whose gesture begins takes the pointer from
        // what holds it. As qml6 tells it, what held the pointer hears that
        // its press was cancelled before the handler becomes active, and a
        // TapHandler that only watched the press after.
        let held = match self.pointer.grab {
            Some(Grab::Mouse(o) | Grab::Tap(o) | Grab::Item(o)) => Some(o),
            None => None,
        };
        if let Some(h) = crate::handlers::moved(engine, ev, held) {
            let watching = std::mem::take(&mut self.pointer.passive);
            self.cancel(engine, ev);
            crate::handlers::activate(engine, h, ev, held);
            for t in watching {
                cancel_tap(engine, t, ev.pos);
            }
            self.pointer.grab = Some(Grab::Item(h));
            handled = true;
        }
        if (ev.pos - self.pointer.press_pos).hypot()
            > crate::system_prefs::start_drag_distance(engine)
        {
            // A drag is not a tap.
            for h in std::mem::take(&mut self.pointer.passive) {
                cancel_tap(engine, h, ev.pos);
            }
            // Nor a long press, for a handler that keeps the press.
            if let Some(Grab::Tap(h)) = self.pointer.grab {
                crate::long_press::moved_away(engine, h);
            }
        }
        for h in &self.pointer.passive {
            if engine.is_alive(*h) {
                crate::input::tap_point_moved(engine, *h, ev);
            }
        }
        // An area that drags from its children hears the move first, and
        // takes the press when its drag starts.
        if let Some(m) = crate::mouse_drag::filter_move(engine, ev) {
            self.cancel(engine, ev);
            self.pointer.grab = Some(Grab::Mouse(m));
            return true;
        }
        // A Flickable around whatever holds the press may take it for a
        // drag; the holder is told its press was cancelled.
        let holder = match self.pointer.grab {
            Some(Grab::Mouse(o) | Grab::Item(o)) => Some(o),
            Some(Grab::Tap(h)) => crate::input::tap_handler_target(engine, h),
            None => None,
        };
        // So may a PathView around it.
        if let Some(f) = holder.and_then(|o| {
            crate::flickable::drag::steal(engine, o, self.pointer.press_pos, ev)
                .or_else(|| crate::views::path_view::steal(engine, o, self.pointer.press_pos, ev))
        }) {
            self.cancel(engine, ev);
            self.pointer.grab = Some(Grab::Item(f));
        }
        match self.pointer.grab {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                // The target moves first: the position is then the one in
                // the area where it went.
                crate::mouse_drag::moved(engine, m, ev);
                let local = local_point(engine, m, ev.pos);
                let inside = contains_local(engine, m, local);
                // As in Qt, crossing the edge is told before the position
                // changes: `entered` and `exited` read the last one.
                if set_contains(engine, m, inside) {
                    emit(engine, m, if inside { "entered" } else { "exited" }, &[]);
                }
                set_mouse_pos(engine, m, local);
                emit_mouse(engine, m, "positionChanged", ev, local, false);
                // Nothing else hovers while a MouseArea holds the press.
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
                } else {
                    crate::input::tap_point_moved(engine, h, ev);
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
        crate::mouse_drag::filter_release(engine, ev);
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
                // A press that became a drag is no click.
                let inside =
                    contains_local(engine, m, local) && !crate::mouse_drag::dragged(engine, m);
                set_mouse_pos(engine, m, local);
                set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                let was_held = crate::press_and_hold::ended(engine, m);
                let released = MouseEvent {
                    was_held,
                    ..mouse_event(ev, local, inside)
                };
                emit_mouse_event(engine, m, "released", released);
                if inside && !was_held && !self.pointer.double {
                    emit_mouse(engine, m, "clicked", ev, local, true);
                    self.pointer.last_click = Some((m, ev.time, ev.pos));
                }
                crate::mouse_drag::released(engine, m);
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
        // After the taps, as in Qt: a PointHandler lets go after the
        // TapHandler beside it has tapped.
        crate::handlers::released(engine, ev);
        self.pointer.double = false;
        // A MouseArea's hover follows at the next move, as in Qt; the
        // other items' hover follows now.
        let hits = self.hits(engine, ev.pos);
        self.update_item_hover(engine, ev, &hits);
        handled
    }

    fn cancel(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let mut handled = false;
        match self.pointer.grab.take() {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                crate::press_and_hold::ended(engine, m);
                set(engine, m, MouseArea::PRESSED.index, Value::Bool(false));
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                if !hover_enabled(engine, m) {
                    set_contains(engine, m, false);
                }
                emit(engine, m, "canceled", &[]);
                crate::mouse_drag::released(engine, m);
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
        crate::mouse_drag::filter_cancel(engine);
        handled
    }

    fn tap(&mut self, engine: &Engine, h: ObjectId, ev: &PointerEvent) {
        // A press that was a long press is no tap when it is let go.
        if crate::long_press::ended(engine, h) {
            crate::input::tap_point_moved(engine, h, ev);
            set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
            if engine.is_alive(h) && crate::input::tap_point_clear(engine, h) {
                crate::input::tap_point_changed(engine, h);
            }
            return;
        }
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
        // As in Qt the tap is told while the handler still reads as
        // pressed, with the point where the button came up and no button
        // in it; then the handler is not pressed, then the point is empty.
        crate::input::tap_point_moved(engine, h, ev);
        set(engine, h, TapHandler::TAP_COUNT.index, Value::Int(count));
        let args = [point_value(ev.pos), Value::Int(button.bits() as i32)];
        emit(engine, h, "tapped", &args);
        match count {
            1 => emit(engine, h, "singleTapped", &args),
            2 => emit(engine, h, "doubleTapped", &args),
            _ => {}
        }
        if !engine.is_alive(h) {
            return;
        }
        set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
        if crate::input::tap_point_clear(engine, h) {
            crate::input::tap_point_changed(engine, h);
        }
    }

    /// Hover of the items that are not MouseAreas (`pointer_item.rs`).
    fn update_item_hover(&mut self, engine: &Engine, ev: &PointerEvent, hits: &[ObjectId]) {
        crate::pointer_item::update_hover(engine, hits, &mut self.pointer.item_hovered);
        crate::handlers::hover(engine, ev, hits);
        crate::pointer_item::hover_moved(engine, &self.pointer.item_hovered, |o| {
            local_point(engine, o, ev.pos)
        });
    }

    /// Hover for `hoverEnabled` MouseAreas, as Qt 6.11 delivers it
    /// (observed on qml6; `features/input/mouse-area-hover*.feature`): the
    /// topmost hover-enabled area under the pointer takes the hover,
    /// together with the hover-enabled areas it is inside. Areas beneath it
    /// get nothing, even when the top one is disabled.
    fn update_hover(&mut self, engine: &Engine, ev: &PointerEvent, hits: &[ObjectId]) -> bool {
        self.update_item_hover(engine, ev, hits);
        let grabbed = match self.pointer.grab {
            Some(Grab::Mouse(m)) => Some(m),
            _ => None,
        };
        let now: Vec<ObjectId> = hover_chain(engine, hits)
            .into_iter()
            .filter(|o| Some(*o) != grabbed)
            .collect();
        // Hover carries no button, whatever is held.
        let hover = PointerEvent {
            button: None,
            buttons: MouseButtons::empty(),
            ..*ev
        };
        // The innermost area first, each with its own position change
        // before the next one hears anything.
        for m in &now {
            let local = local_point(engine, *m, ev.pos);
            let entering = !engine
                .read(*m, MouseArea::CONTAINS_MOUSE.index)
                .to_boolean();
            if entering {
                // `containsMouse` and the position change together, so
                // that a handler of either reads the other as it is now.
                engine.batch(|| {
                    set_contains(engine, *m, true);
                    set_mouse_pos(engine, *m, local);
                });
                emit(engine, *m, "entered", &[]);
            } else {
                set_mouse_pos(engine, *m, local);
            }
            if ev.kind == PointerKind::Move && engine.is_alive(*m) {
                emit_mouse(engine, *m, "positionChanged", &hover, local, false);
            }
        }
        // The areas that were left exit last, in the order they were
        // entered, and keep the last position they had.
        let mut hovered = std::mem::take(&mut self.pointer.hovered);
        hovered.retain(|o| {
            if now.contains(o) {
                return true;
            }
            if Some(*o) != grabbed && engine.is_alive(*o) && set_contains(engine, *o, false) {
                emit(engine, *o, "exited", &[]);
            }
            false
        });
        for m in &now {
            if !hovered.contains(m) {
                hovered.push(*m);
            }
        }
        let any = !now.is_empty();
        self.pointer.hovered = hovered;
        any
    }
}

// ---------------------------------------------------------------- effects

fn structure_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        mark(shared, |s| s.dead.push(obj));
        return;
    }
    // Only for the subscription: the list is read when the structure is
    // rebuilt, and copying it here made each child added cost as much as
    // all the children before it.
    let _ = e.read(obj, Item::CHILDREN.index);
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

/// Run where `obj` is alive: as its effect, or by [`shown_changed`].
fn place_effect(e: &Engine, obj: ObjectId, may_scroll: bool, shown: &Shown, shared: &SharedRc) {
    let place = (
        shown_num(e, shown, obj, Item::X.index),
        shown_num(e, shown, obj, Item::Y.index),
        may_scroll && crate::flickable::content_moving(e, obj),
    );
    mark(shared, |s| {
        s.place.insert(obj, place);
    });
}

/// Run where `obj` is alive, as [`place_effect`] is.
fn geometry_effect(e: &Engine, obj: ObjectId, shown: &Shown, shared: &SharedRc) {
    let g = Geom::read(e, obj, shown);
    mark(shared, |s| {
        s.geom.insert(obj, g);
    });
}

/// Follows the entries of a `transform` list. Made only for an item whose
/// list has some (see [`QuickScene::follow_list`]).
fn list_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let list = crate::transform::list_map(e, obj);
    mark(shared, |s| {
        s.list.insert(obj, list);
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
        Kind::Canvas => crate::canvas::read_inputs(e, obj),
        Kind::Item
        | Kind::Rectangle
        | Kind::Image
        | Kind::Editor
        | Kind::Shape
        | Kind::Effect
        | Kind::Shader
        | Kind::Source
        | Kind::External => {
            return;
        }
    }
    mark(shared, |s| {
        s.polish.insert(obj);
    });
}

/// An Animator showed another value of `prop`, or stopped showing one:
/// whatever draws that property reads it again.
fn shown_changed(e: &Engine, obj: ObjectId, prop: PropIndex, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let shown = crate::animation::shown::shown(e);
    if prop == Item::X.index || prop == Item::Y.index {
        // Not told whether it may scroll: asked, which is only dearer.
        place_effect(e, obj, true, &shown, shared);
    } else if [Item::OPACITY, Item::ROTATION, Item::SCALE]
        .iter()
        .any(|p| p.index == prop)
    {
        geometry_effect(e, obj, &shown, shared);
    } else if is_a::<crate::shader_effect::ShaderEffect>(e, obj) {
        content_effect(e, obj, Kind::Shader, shared);
    }
}

fn content_effect(e: &Engine, obj: ObjectId, kind: Kind, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let p = match kind {
        Kind::Rectangle => Paint::Rect(RectanglePaint::read(e, obj)),
        Kind::Text => Paint::Text(TextInputs::read(e, obj)),
        Kind::Image => Paint::Image(crate::image::ImagePaint::read_on_screen(e, obj)),
        Kind::Editor => Paint::Editor(crate::text_input::EditorPaint::read(e, obj)),
        Kind::Shape => Paint::Shape(crate::shapes::ShapePaint::read(e, obj)),
        Kind::Canvas => Paint::Canvas(crate::canvas::CanvasPicture::read(e, obj)),
        Kind::Effect => Paint::Effect(crate::effects::EffectPaint::read(e, obj)),
        Kind::Shader => Paint::Shader(crate::shader_effect::ShaderEffectPaint::read(e, obj)),
        Kind::Source => Paint::Source(crate::shader_effect::SourcePaint::read(e, obj)),
        Kind::External => match crate::paint_item::paint_item(e, obj) {
            Some(read) => {
                crate::paint_item::track_repaint(e, obj);
                Paint::External(read(e, obj))
            }
            None => return,
        },
        _ => return,
    };
    mark(shared, |s| {
        s.content.insert(obj, p);
    });
}

/// `visible` and `enabled` read false under a hidden or disabled parent
/// (Qt documents both as inherited). The item's own value sits below an
/// override layer and comes back when the parent turns on again.
fn inherit_effect(e: &Engine, obj: ObjectId, st: &RefCell<Inherit>, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let covered = st.borrow().visible.is_some();
    let parent = e
        .visual_parent(obj)
        .filter(|p| e.is_alive(*p) && is_item(e, *p));
    let mut guard = st.borrow_mut();
    let st = &mut *guard;
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
    // A covered item is kept shown in the tree (see `apply_geometry`).
    // Uncovered, its own `visible` may be the false it read under the
    // cover, which is no change, so the geometry is not read again by
    // itself.
    let lifted = covered && st.visible.is_none();
    drop(guard);
    if lifted {
        let g = e.untrack(|| Geom::read(e, obj, &crate::animation::shown::shown(e)));
        mark(shared, |s| {
            s.geom.insert(obj, g);
        });
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

pub(crate) fn set(engine: &Engine, obj: ObjectId, p: PropIndex, v: Value) {
    let _ = write_computed(engine, obj, p, v);
}

pub(crate) fn emit(engine: &Engine, obj: ObjectId, name: &str, args: &[Value]) {
    let _ = engine.emit_by_name(obj, name, args);
}

/// Whether `obj` leaves the items inside it enabled. A MouseArea's own
/// `enabled` does not count (Qt documents that it does not cascade), so
/// the answer comes from the nearest ancestor that is not one.
pub(crate) fn item_enabled(e: &Engine, obj: ObjectId) -> bool {
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
pub(crate) fn mouse_area_enabled(e: &Engine, m: ObjectId) -> bool {
    e.read(m, Item::ENABLED.index).to_boolean()
        && e.visual_parent(m).is_none_or(|p| item_enabled(e, p))
}

/// The MouseAreas the pointer hovers, innermost first, given the items
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
    chain
}

fn accepted_buttons(engine: &Engine, m: ObjectId) -> i32 {
    num(engine, m, MouseArea::ACCEPTED_BUTTONS.index) as i32
}

pub(crate) fn hover_enabled(engine: &Engine, m: ObjectId) -> bool {
    engine.read(m, MouseArea::HOVER_ENABLED.index).to_boolean()
}

/// Sets `containsMouse`; true if it changed.
pub(crate) fn set_contains(engine: &Engine, m: ObjectId, v: bool) -> bool {
    let cur = engine.read(m, MouseArea::CONTAINS_MOUSE.index).to_boolean();
    if cur == v {
        return false;
    }
    set(engine, m, MouseArea::CONTAINS_MOUSE.index, Value::Bool(v));
    true
}

/// Sets `mouseX` and `mouseY`. As in Qt both tell of a change at every
/// new position, also the one that kept its value.
pub(crate) fn set_mouse_pos(engine: &Engine, m: ObjectId, local: kurbo::Point) {
    let x = MouseArea::MOUSE_X.index;
    let y = MouseArea::MOUSE_Y.index;
    let old = kurbo::Point::new(num(engine, m, x), num(engine, m, y));
    if old == local {
        return;
    }
    engine.batch(|| {
        for (p, v, same) in [
            (x, local.x, old.x == local.x),
            (y, local.y, old.y == local.y),
        ] {
            if same {
                let _ = engine.notify(m, p);
            } else {
                set(engine, m, p, Value::Real(v));
            }
        }
    });
}

pub(crate) fn local_point(engine: &Engine, obj: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| scene_transform(engine, obj)).inverse() * pos
}

pub(crate) fn contains_local(engine: &Engine, obj: ObjectId, p: kurbo::Point) -> bool {
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
    crate::long_press::ended(engine, h);
    set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
    // As in Qt a handler that held the press alone has an empty point when
    // it says `canceled`; one that only watched still has the point.
    let alone =
        num(engine, h, TapHandler::GESTURE_POLICY.index) as i32 != gesture_policy::DRAG_THRESHOLD;
    let mut changed = alone && crate::input::tap_point_clear(engine, h);
    emit(engine, h, "canceled", &[point_value(pos)]);
    if engine.is_alive(h) {
        changed |= crate::input::tap_point_clear(engine, h);
        if changed {
            crate::input::tap_point_changed(engine, h);
        }
    }
}

/// Emits a MouseArea signal with a fresh `mouse` object and returns its
/// `accepted` after the handlers ran.
pub(crate) fn emit_mouse(
    engine: &Engine,
    m: ObjectId,
    name: &str,
    ev: &PointerEvent,
    local: kurbo::Point,
    is_click: bool,
) -> bool {
    emit_mouse_event(engine, m, name, mouse_event(ev, local, is_click))
}

fn mouse_event(ev: &PointerEvent, local: kurbo::Point, is_click: bool) -> MouseEvent {
    MouseEvent {
        x: local.x,
        y: local.y,
        button: ev.button.map_or(0, |b| b.bits() as i32),
        buttons: ev.buttons.bits() as i32,
        modifiers: ev.modifiers.bits() as i32,
        is_click,
        ..MouseEvent::default()
    }
}

/// Emits `name` of `m` with `native` as its `mouse`. Returns whether the
/// handler left the event accepted.
pub(crate) fn emit_mouse_event(
    engine: &Engine,
    m: ObjectId,
    name: &str,
    native: MouseEvent,
) -> bool {
    let Ok(me) = engine.create_native_incomplete(native, Some(m)) else {
        return true;
    };
    emit(engine, m, name, &[Value::Object(me)]);
    let accepted = engine.read(me, MouseEvent::ACCEPTED.index).to_boolean();
    engine.destroy_now(me);
    accepted
}

/// What a composed subtree shows, to tell whether it must be drawn again.
fn fingerprint(snap: &FrameSnapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    snap.viewport.width().to_bits().hash(&mut h);
    snap.viewport.height().to_bits().hash(&mut h);
    for i in &snap.items {
        i.node.hash(&mut h);
        for c in i.world.as_coeffs() {
            c.to_bits().hash(&mut h);
        }
        i.opacity.to_bits().hash(&mut h);
        i.size.width.to_bits().hash(&mut h);
        i.size.height.to_bits().hash(&mut h);
        i.list
            .as_ref()
            .map(|l| Arc::as_ptr(l) as usize)
            .hash(&mut h);
        i.clip.map(|c| c.0).hash(&mut h);
    }
    for c in &snap.clips {
        format!("{c:?}").hash(&mut h);
    }
    h.finish()
}

/// Whether `root` is `obj` or owns it, however deep.
fn owned_by(engine: &Engine, obj: ObjectId, root: ObjectId) -> bool {
    let mut at = Some(obj);
    while let Some(o) = at {
        if o == root {
            return true;
        }
        at = engine.owner(o);
    }
    false
}
