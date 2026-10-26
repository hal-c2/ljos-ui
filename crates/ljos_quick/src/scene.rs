//! `QuickScene`: an item tree mirrored into a [`RenderTree`].
//!
//! The scene adopts a root item and everything under it. For each item it
//! installs a few small effects that record what changed (structure,
//! geometry, anchors, inherited visibility, polish inputs, paint inputs)
//! as marks on the item's record and wake the host on the clean-to-dirty
//! edge.
//! Nothing polls: [`QuickScene::frame`] only drains those marks
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

mod records;

use records::Records;

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
    Engine, EngineError, JournalKind, LogLevel, MetaObject, NodeId as GraphNode, ObjectId,
    OverrideContent, OverrideId, PropIndex, Provenance, QmlType, RestoreMode, Value,
};
use ljos_render::{ClipSpec, FrameSnapshot, ImageHandle, NodeId, RenderTree, hit_test, kurbo};
use ljos_runtime::{FrameBudget, Scheduler, Time};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SecondaryMap;
use smallvec::SmallVec;

use crate::accessible::A11yPass;
use crate::anchors;
use crate::animation::shown::Shown;
use crate::enums::gesture_policy;
use crate::handlers::{grab_transition, point_state};
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
/// Polish passes per frame before the scene gives up and warns. A child
/// whose implicit size follows the size its layout gives it (a `Column`
/// whose last item fills what is left of it) moves part of the way to where it
/// settles each pass, and Qt lets it get there within the frame; a loop
/// that never settles stops here, where Qt would spin on.
pub const MAX_POLISH_PASSES: usize = 100;
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

/// What the scene asks of an item's type, worked out once per type
/// ([`MetaObject::class`]): asking the type's bases again for each new item
/// was a dozen walks up its chain an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ItemClass {
    kind: Kind,
    /// It is an `Item`: the scene shows it.
    item: bool,
    /// A `Loader`, whose item is shown only once it is made.
    loader: bool,
    /// A `MouseArea`, whose `enabled` neither follows a disabled ancestor
    /// nor disables what is inside it.
    mouse_area: bool,
    /// A `Flickable`, whose content item may scroll.
    flickable: bool,
}

impl ItemClass {
    fn of_type(m: &MetaObject) -> ItemClass {
        let is = |t: &MetaObject| m.inherits(t);
        let kind = if is(crate::text_input::TextEditor::static_meta()) {
            Kind::Editor
        } else if is(Text::static_meta()) {
            Kind::Text
        } else if is(Rectangle::static_meta()) {
            Kind::Rectangle
        } else if is(crate::image::ImageBase::static_meta()) {
            Kind::Image
        } else if is(Row::static_meta()) {
            Kind::Row
        } else if is(Column::static_meta()) {
            Kind::Column
        } else if crate::layouts::managed_type(m) {
            Kind::Managed
        } else if is(crate::shapes::Shape::static_meta()) {
            Kind::Shape
        } else if is(crate::canvas::Canvas::static_meta()) {
            Kind::Canvas
        } else if is(crate::effects::MultiEffect::static_meta()) {
            Kind::Effect
        } else if is(crate::shader_effect::ShaderEffect::static_meta()) {
            Kind::Shader
        } else if is(crate::shader_effect::ShaderEffectSource::static_meta()) {
            Kind::Source
        } else {
            Kind::Item
        };
        ItemClass {
            kind,
            item: is(Item::static_meta()),
            loader: is(Loader::static_meta()),
            mouse_area: is(MouseArea::static_meta()),
            flickable: is(crate::flickable::Flickable::static_meta()),
        }
    }

    /// The class of `obj`, `None` once it is gone. Whether a type of
    /// another crate paints itself is the engine's to say
    /// (`crate::register_paint_item`), not the type's: asked of what is
    /// no other kind.
    fn of(engine: &Engine, obj: ObjectId) -> Option<ItemClass> {
        let mut c = engine.class_of(obj, ItemClass::of_type)?;
        if c.kind == Kind::Item && crate::paint_item::paint_item(engine, obj).is_some() {
            c.kind = Kind::External;
        }
        Some(c)
    }
}

/// Whether `obj` is an item the scene shows, by its class.
fn shows(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .class_of(obj, ItemClass::of_type)
        .is_some_and(|c| c.item)
}

impl Kind {
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

/// Whether an item's node is in the tree: `visible`, or reading false
/// only because a parent is hidden (`covered`), facing the viewer, and
/// not waiting in a view's pool.
fn shown(g: &Geom, covered: bool, pooled: bool) -> bool {
    (g.visible || covered) && !g.faces_away && !pooled
}

/// The geometry of one item, as its render node needs it, but for where
/// it is (`Place`): an item that only moves, as an animated or flicked one
/// does, reads two properties a frame instead of all of these.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

/// What an item is drawn from. Every item it paints keeps one in its
/// record, so the rare large kinds are held behind a pointer and the
/// common ones (a rectangle) set its size. A text's inputs are shared
/// with its marks ([`Marks::text`]), not copied into each.
#[derive(Clone, Debug)]
enum Paint {
    Rect(RectanglePaint),
    Text(Rc<TextInputs>),
    Image(crate::image::ImagePaint),
    Editor(Box<crate::text_input::EditorPaint>),
    Shape(crate::shapes::ShapePaint),
    Canvas(crate::canvas::CanvasPicture),
    Effect(crate::effects::EffectPaint),
    Shader(crate::shader_effect::ShaderEffectPaint),
    Source(crate::shader_effect::SourcePaint),
    External(Rc<dyn crate::paint_item::ItemPaint>),
}

bitflags::bitflags! {
    /// What of an item waits for the frame ([`Marks`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Dirty: u8 {
        /// Its geometry ([`Geom`]), read at the frame: told by the change
        /// hooks of what it reads ([`reshaped`]).
        const GEOM = 1;
        /// The map of its `transform` list.
        const LIST = 1 << 1;
        const CONTENT = 1 << 2;
        /// Its children.
        const STRUCTURE = 1 << 3;
        const POLISH = 1 << 4;
    }
}

impl Dirty {
    const KINDS: usize = 5;

    /// The queue of a single mark.
    fn slot(self) -> usize {
        self.bits().trailing_zeros() as usize
    }
}

/// What the effects and change hooks of one item tell the scene, until
/// the frame takes it. A value is read only while its mark is set.
struct Marks {
    dirty: Dirty,
    /// What a Text's polish effect last read of it: its paint and its
    /// polish take it from here, so a text's inputs are read once a change
    /// and not by each. Shared with its paint, and only a Text has one, so
    /// the marks of every other item do not hold room for it.
    text: Option<Rc<TextInputs>>,
    /// A Text with a content effect: its polish effect gives it its paint.
    text_shown: bool,
    inherit: Inherit,
    /// The polish effect runs again only to read the inputs once more
    /// after a polish, and marks nothing (`QuickScene::polish`).
    retrack: bool,
}

impl Marks {
    fn new() -> Marks {
        Marks {
            dirty: Dirty::empty(),
            text: None,
            text_shown: false,
            inherit: Inherit::default(),
            retrack: false,
        }
    }
}

/// Dirty state written by effects and drained by `frame`.
#[derive(Default)]
struct Shared {
    /// One for each item the scene shows, made before its effects are, so
    /// a mark for anything else is nothing.
    marks: Records<Marks>,
    /// What the marks `LIST` and `CONTENT` carry, beside the marks: few
    /// items have either at once, and each is large. Kept only for an item
    /// with marks, and taken with the mark.
    lists: FxHashMap<ObjectId, crate::transform::Projective>,
    contents: FxHashMap<ObjectId, Paint>,
    /// The items with each mark set, in the order marked. An item joins on
    /// the edge from clear to set, so it is in each queue once: a set per
    /// mark cost each item marked a hash insert.
    queued: [Vec<ObjectId>; Dirty::KINDS],
    /// Items whose place may have changed, read at the frame: told by the
    /// change hooks of `x` and `y` ([`moved`]). An item is here once for
    /// each change told, read once ([`Record::place_read`]): a set cost
    /// each item that moved a hash insert every frame.
    placed: Vec<ObjectId>,
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
        self.queued.iter().any(|q| !q.is_empty())
            || !self.placed.is_empty()
            || !self.dead.is_empty()
            || self.pictures
    }

    /// Sets the mark `what` of `obj`, after `f` gave it its value.
    fn set(&mut self, obj: ObjectId, what: Dirty, f: impl FnOnce(&mut Marks)) {
        let Some(m) = self.marks.get_mut(obj) else {
            return;
        };
        f(m);
        if !m.dirty.contains(what) {
            m.dirty.insert(what);
            self.queued[what.slot()].push(obj);
        }
    }

    fn is_set(&self, obj: ObjectId, what: Dirty) -> bool {
        self.marks.get(obj).is_some_and(|m| m.dirty.contains(what))
    }

    /// Clears the mark `what` of `obj`, and gives what `f` takes of it
    /// if it was set.
    fn clear<T>(
        &mut self,
        obj: ObjectId,
        what: Dirty,
        f: impl FnOnce(&mut Marks) -> T,
    ) -> Option<T> {
        let m = self.marks.get_mut(obj)?;
        if !m.dirty.contains(what) {
            return None;
        }
        m.dirty.remove(what);
        let q = &mut self.queued[what.slot()];
        if let Some(i) = q.iter().position(|o| *o == obj) {
            q.swap_remove(i);
        }
        Some(f(m))
    }

    /// Marks `obj` to be drawn again from `p`.
    fn paint(&mut self, obj: ObjectId, p: Paint) {
        if self.marks.contains(obj) {
            self.contents.insert(obj, p);
            self.set(obj, Dirty::CONTENT, |_| ());
        }
    }

    /// Marks `obj`'s `transform` list as making `list`.
    fn list(&mut self, obj: ObjectId, list: crate::transform::Projective) {
        if self.marks.contains(obj) {
            self.lists.insert(obj, list);
            self.set(obj, Dirty::LIST, |_| ());
        }
    }

    fn any(&self, what: Dirty) -> bool {
        !self.queued[what.slot()].is_empty()
    }

    /// Takes the items marked `what` and what `f` takes of each, clearing
    /// the mark. The queue keeps its room for the next frame.
    fn take<T>(&mut self, what: Dirty, mut f: impl FnMut(&mut Marks) -> T) -> Vec<(ObjectId, T)> {
        let (marks, q) = (&mut self.marks, &mut self.queued[what.slot()]);
        q.drain(..)
            .filter_map(|o| {
                let m = marks.get_mut(o)?;
                m.dirty.remove(what);
                Some((o, f(m)))
            })
            .collect()
    }

    /// Forgets `obj`'s marks: it left the scene.
    fn forget(&mut self, obj: ObjectId) -> Option<Marks> {
        let m = self.marks.remove(obj)?;
        // Most items that go were drawn and have no mark left.
        for what in m.dirty.iter() {
            self.queued[what.slot()].retain(|o| *o != obj);
        }
        if m.dirty.contains(Dirty::LIST) {
            self.lists.remove(&obj);
        }
        if m.dirty.contains(Dirty::CONTENT) {
            self.contents.remove(&obj);
        }
        Some(m)
    }
}

type SharedRc = Rc<RefCell<Shared>>;

/// The scenes showing an engine's items, for the change hooks of what
/// they read ([`moved`], [`reshaped`]). A scene that goes is let go.
#[derive(Default)]
struct Scenes(RefCell<Vec<std::rc::Weak<RefCell<Shared>>>>);

impl Scenes {
    fn add(&self, shared: &SharedRc) {
        let mut all = self.0.borrow_mut();
        if all
            .iter()
            .any(|w| std::ptr::eq(w.as_ptr(), Rc::as_ptr(shared)))
        {
            return;
        }
        all.retain(|w| w.strong_count() > 0);
        all.push(Rc::downgrade(shared));
    }
}

/// Marks `obj` with `f` in each scene that shows it.
fn mark_shown(e: &Engine, obj: ObjectId, f: impl Fn(&mut Shared)) {
    let scenes = e.extension::<Scenes>();
    // Taken out of the list first: a mark wakes the host, which may make
    // or drop a scene.
    let shown: SmallVec<[SharedRc; 2]> = scenes
        .0
        .borrow()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect();
    for s in shown {
        if s.borrow().marks.contains(obj) {
            mark(&s, &f);
        }
    }
}

/// The change hook of an item's `children`, and of a Loader's `item` and
/// `status` (its item is shown only once it is made, `shown_children`):
/// what it shows is matched to them at the frame. A mark where an effect
/// cost each item a node and a run as it was made.
pub(crate) fn restructured(cx: &ljos_engine::Cx<'_>, _: PropIndex) {
    let obj = cx.this;
    mark_shown(cx.engine, obj, |s| s.set(obj, Dirty::STRUCTURE, |_| ()));
}

/// [`Engine::on_destroyed`]: a scene that showed one of `gone` sweeps its
/// dead at the next frame.
pub(crate) fn destroyed(e: &Engine, gone: &[ObjectId]) {
    let scenes = e.extension::<Scenes>();
    let shown: SmallVec<[SharedRc; 2]> = scenes
        .0
        .borrow()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect();
    for s in shown {
        let first = gone.iter().find(|o| s.borrow().marks.contains(**o));
        if let Some(o) = first {
            mark(&s, |s| s.dead.push(*o));
        }
    }
}

/// [`Engine::on_incubated`]: the item an incubation made, left out of its
/// parent while it was half made, is shown now.
pub(crate) fn incubated(e: &Engine, made: ObjectId) {
    if let Some(p) = e.visual_parent(made) {
        mark_shown(e, p, |s| s.set(p, Dirty::STRUCTURE, |_| ()));
    }
}

/// The change hook of an item's `x` and `y`: its place is read at the
/// frame. A mark where an effect each item cost a node and, as it moved,
/// a run.
pub(crate) fn moved(cx: &ljos_engine::Cx<'_>, _: PropIndex) {
    let obj = cx.this;
    mark_shown(cx.engine, obj, |s| s.placed.push(obj));
}

/// How many times a `cursorShape` has changed: a scene whose pointer is
/// still works its cursor out again at the frame when this moved on.
#[derive(Default)]
struct CursorShapes(std::cell::Cell<u64>);

/// The change hook of a MouseArea's or a pointer handler's `cursorShape`:
/// a shape that changes under a pointer that stays shows at once, as on
/// qml6. Every scene is woken; it looks again only if its pointer is in it.
pub(crate) fn cursor_shape_changed(cx: &ljos_engine::Cx<'_>, _: PropIndex) {
    let changes = cx.engine.extension::<CursorShapes>();
    changes.0.set(changes.0.get() + 1);
    let scenes: SmallVec<[SharedRc; 2]> = cx
        .engine
        .extension::<Scenes>()
        .0
        .borrow()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .collect();
    for s in scenes {
        wake(&s);
    }
}

/// The change hook of what an item's [`Geom`] reads: it is read again at
/// the frame.
pub(crate) fn reshaped(cx: &ljos_engine::Cx<'_>, _: PropIndex) {
    let obj = cx.this;
    mark_shown(cx.engine, obj, |s| s.set(obj, Dirty::GEOM, |_| ()));
}

/// The change hook of an item's `visible`: its geometry is read again at
/// the frame, and what it inherits is passed on at once. A script that
/// hides an item and reads a child's `visible` next reads false, as on
/// Qt, so this cannot wait for the frame.
pub(crate) fn visibility(cx: &ljos_engine::Cx<'_>, p: PropIndex) {
    reshaped(cx, p);
    inherited(cx.engine, cx.this, true);
}

/// The change hook of an item's `enabled`: passed on at once, as
/// [`visibility`] is.
pub(crate) fn enabling(cx: &ljos_engine::Cx<'_>, _: PropIndex) {
    inherited(cx.engine, cx.this, true);
}

/// `obj` was given another visual parent: it takes what the new one
/// passes on, before whoever moved it reads it.
pub(crate) fn reparented(e: &Engine, obj: ObjectId) {
    inherited(e, obj, false);
}

/// In each scene that shows `obj`, it takes again what its parent passes
/// on: its own value may have changed under a hidden parent, or the parent
/// may be another. With `down`, so do its children. A child whose value
/// changes tells its own children through its hook, so this goes no
/// deeper, but for a `MouseArea`, whose `enabled` its children skip.
fn inherited(e: &Engine, obj: ObjectId, down: bool) {
    let scenes = e.extension::<Scenes>();
    let shown: SmallVec<[SharedRc; 2]> = scenes
        .0
        .borrow()
        .iter()
        .filter_map(std::rc::Weak::upgrade)
        .filter(|s| s.borrow().marks.contains(obj))
        .collect();
    for s in shown {
        inherit(e, obj, &s);
        if down {
            pass_down(e, obj, &s);
        }
    }
}

fn pass_down(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    for c in e.visual_children(obj) {
        let Some(st) = shared.borrow().marks.get(c).map(|m| m.inherit) else {
            continue;
        };
        inherit(e, c, shared);
        if st.mouse_area {
            pass_down(e, c, shared);
        }
    }
}

/// [`moved`] for what a place reads beside `x` and `y`: whether the
/// Flickable `obj` is the content of moves.
pub(crate) fn place_again(e: &Engine, obj: ObjectId) {
    mark_shown(e, obj, |s| s.placed.push(obj));
}

/// [`reshaped`] for what a geometry reads beside the item's own
/// properties: whether a Flipable shows the side `obj` is.
pub(crate) fn reshape(e: &Engine, obj: ObjectId) {
    mark_shown(e, obj, |s| s.set(obj, Dirty::GEOM, |_| ()));
}

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
#[derive(Clone, Copy, Debug, Default)]
struct Inherit {
    visible: Option<OverrideId>,
    enabled: Option<OverrideId>,
    /// A `MouseArea`, whose `enabled` is its own ([`inherit`]).
    mouse_area: bool,
    /// [`inherit`] is changing the item's stack, whose change hook asks
    /// for it again.
    busy: bool,
}

/// What the scene keeps of one item it shows ([`Records`]): one record in
/// place of a map per concern, each of which an item joined and left.
struct Record {
    node: NodeId,
    class: ItemClass,
    /// The item this one was adopted under; `None` for the root.
    parent: Option<ObjectId>,
    /// The effects made with it, newest last. Room for those that come
    /// later too, so letting go of them all takes no allocation.
    effects: SmallVec<[GraphNode; 8]>,
    /// It is on screen and its paint is followed: by `content_effect`, or
    /// for a Text by its polish effect, which reads all its paint does.
    painting: bool,
    content_effect: Option<GraphNode>,
    paint: Option<Paint>,
    size: kurbo::Size,
    /// The last geometry and place applied; the node's transform takes
    /// both, and either may change alone.
    geom: Option<Geom>,
    place: Place,
    /// The map of the item's `transform` list, and the effect that follows
    /// it: made only for an item whose list has entries. Boxed, as few
    /// items have one: the identity where it is `None`.
    list: Option<Box<crate::transform::Projective>>,
    list_effect: Option<GraphNode>,
    /// The [`QuickScene::places_read`] that last read its place.
    place_read: u64,
    /// Adopted under a Flickable, so it may be its content item: that one
    /// is the Flickable's child from when it is made. No other item asks
    /// whether its Flickable moves.
    may_scroll: bool,
}

impl Record {
    fn list(&self) -> &crate::transform::Projective {
        self.list
            .as_deref()
            .unwrap_or(&crate::transform::Projective::IDENTITY)
    }

    /// Keeps the box an item's list was given, as a list that moves (a
    /// `Rotation`'s angle) is set every frame.
    fn set_list(&mut self, list: crate::transform::Projective) {
        match &mut self.list {
            Some(had) => **had = list,
            None if list == crate::transform::Projective::IDENTITY => {}
            None => self.list = Some(Box::new(list)),
        }
    }
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
    /// The same, and those of them whose tap was cancelled: as in Qt they
    /// watch the point until it comes up.
    watched: Vec<ObjectId>,
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
    /// The cursor is a TableView's over the end of a column or row, which
    /// it keeps while a button is held.
    cursor_resizes: bool,
    /// The [`CursorShapes`] count the cursor was worked out at.
    cursor_shapes: u64,
    /// Where the pointer is over the scene; none once it has left.
    at: Option<kurbo::Point>,
    /// What is under the pointer at the frame it was found in, topmost
    /// first: what the cursor is worked out from.
    hits: Vec<ObjectId>,
    hits_at: Option<(kurbo::Point, u64)>,
    /// The item a right-click no one else took asks for its context menu
    /// when the button comes up, as in Qt.
    context: Option<ObjectId>,
}

/// What a scene keeps, for a tool that shows where memory went. Counts
/// are exact; `bytes` is an estimate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneMemory {
    /// Items the scene shows.
    pub items: usize,
    pub tree: ljos_render::TreeMemory,
    /// The tree's and the scene's own tables.
    pub bytes: usize,
}

/// An item tree, its render tree, and the frame pipeline between them.
pub struct QuickScene {
    tree: RenderTree,
    shared: SharedRc,
    #[allow(deprecated)]
    log: QuickLog,
    items: Records<Record>,
    by_node: SecondaryMap<NodeId, ObjectId>,
    /// `Shared::placed` of the last frame, kept for its capacity.
    placed: Vec<ObjectId>,
    /// How many frames read places.
    places_read: u64,
    /// The items whose place was read this frame, kept for its capacity.
    told_moved: Vec<ObjectId>,
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
    /// The delegates waiting in views' pools (`views::cull::pool`): not
    /// shown and not hit, whatever `visible` says. Read with `culled`.
    pooled: FxHashSet<ObjectId>,
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
    pub(crate) touch: crate::touch::Touches,
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
    /// The frames' times added up while a timeline records
    /// (frame_times.rs); `None`, and no frame reads a clock for it.
    pub(crate) timing: Option<crate::frame_times::FrameTimes>,
    /// Input reached the scene since the last frame, while timed.
    pub(crate) saw_input: std::cell::Cell<bool>,
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
            .field("items", &self.items.len())
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
            items: Records::default(),
            by_node: SecondaryMap::new(),
            placed: Vec::new(),
            places_read: 0,
            told_moved: Vec::new(),
            effect_base: HashMap::new(),
            source_base: HashMap::new(),
            layer_base: HashMap::new(),
            hidden: FxHashSet::default(),
            culled: (FxHashSet::default(), 0),
            pooled: FxHashSet::default(),
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
            touch: crate::touch::Touches::default(),
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
            timing: None,
            saw_input: std::cell::Cell::new(false),
        }
    }

    /// Shows `root` and everything under it, replacing any previous root.
    /// A root it cannot show leaves the scene as it was, the previous root
    /// shown.
    pub fn set_root(&mut self, engine: &Engine, root: ObjectId) -> Result<(), EngineError> {
        if !crate::roots::lasts(engine, root) {
            return Err(EngineError::dead(root));
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
        let old = self.root;
        // The new tree is adopted beside the old one and the old one goes
        // only once it is: undoing a part-made adoption puts nothing of the
        // old one back. Where one tree holds the other, the old one goes
        // first, as the new one shows some of what it shows.
        let nested =
            old.is_some_and(|o| self.items.contains(root) || shown_within(engine, o, root));
        if nested && let Some(old) = self.root.take() {
            self.forget_root(engine, old);
        }
        if let Err(err) = self.adopt(engine, root, None, top, 0) {
            self.unadopt(engine, root, true);
            if nested && let Some(old) = old {
                let going = self.going.take();
                if self.adopt(engine, old, None, top, 0).is_ok() {
                    self.root = Some(old);
                    crate::focus::tree::add_window(engine, old);
                } else {
                    self.unadopt(engine, old, true);
                }
                self.going = going;
            }
            return Err(err);
        }
        if let Some(old) = self.root.take() {
            self.forget_root(engine, old);
        }
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

    fn forget_root(&mut self, engine: &Engine, old: ObjectId) {
        self.unadopt(engine, old, true);
        crate::focus::tree::remove_window(engine, old);
        crate::keys::reset_shortcuts(engine, old);
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

    pub(crate) fn text_engine(&self) -> &SharedTextEngine {
        &self.text
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
            .items
            .iter()
            .filter(|(_, e)| e.class.kind == Kind::Canvas)
            .map(|(id, _)| id);
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
            .items
            .values()
            .filter(|e| e.class.kind.paints() && e.painting)
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
            .items
            .iter()
            .filter(|(_, e)| matches!(e.class.kind, Kind::Text | Kind::Editor))
            .map(|(o, _)| o)
            .collect();
        for e in self.items.values() {
            if matches!(e.class.kind, Kind::Text | Kind::Editor) && e.painting {
                self.tree.mark_content_dirty(e.node);
            }
        }
        mark(&self.shared, |s| {
            for o in texts {
                s.set(o, Dirty::POLISH, |_| ());
            }
        });
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

    /// What the scene keeps: its render tree and display lists, and its
    /// own table of the items it shows. One pass over the tree's nodes.
    pub fn memory(&self) -> SceneMemory {
        let tree = self.tree.memory();
        SceneMemory {
            items: self.items.len(),
            bytes: tree.bytes + self.items.bytes() + self.shared.borrow().marks.bytes(),
            tree,
        }
    }

    pub fn node_of(&self, item: ObjectId) -> Option<NodeId> {
        self.items.get(item).map(|e| e.node)
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
        use crate::frame_times::{FrameTimes, Laps, lap};
        self.frame += 1;
        let mut laps = self.timing.is_some().then(Laps::start);
        let mut steps = FrameTimes::default();
        // An animation under way as the frame starts is stepped by it.
        let animating = laps.is_some() && crate::animation::any_stepping(engine);
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
        lap(&mut laps, &mut steps.other);
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
        lap(&mut laps, &mut steps.timers);
        // 4. Deferred deletes.
        let deleted = engine.process_deferred_deletes();
        let dead = std::mem::take(&mut self.shared.borrow_mut().dead);
        if deleted > 0 || !dead.is_empty() {
            self.sweep_dead(engine);
        }
        // Focus asked for while objects were being made.
        crate::focus::tree::settle(engine);
        self.sync_layer(engine);
        lap(&mut laps, &mut steps.other);
        // 5. Animations, from the frame's time and nothing else.
        crate::animation::tick(engine, now);
        crate::long_press::tick(engine, now);
        crate::flickable::fling::tick(engine, now);
        crate::canvas::tick(engine);
        crate::views::path_view::tick(engine, now);
        lap(&mut laps, &mut steps.animations);
        // 6. Polish.
        self.polish(engine);
        lap(&mut laps, &mut steps.layout);
        // 7. Incubate. New items join the tree and are laid out in this
        // same frame, so what is shown is never half placed.
        let incubating = self.incubate(engine);
        lap(&mut laps, &mut steps.create);
        // 8. Sync.
        self.reconcile(engine);
        self.apply_geometry(engine);
        self.refresh_culled(engine);
        self.journal_content(engine);
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
            self.journal_content(engine);
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
        self.cursor_again(engine);
        lap(&mut laps, &mut steps.scene);
        if let Some(laps) = laps {
            self.add_frame_times(engine, laps, steps, animating);
        }
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
        let dead: Vec<ObjectId> = self.items.keys().filter(|o| !engine.is_alive(*o)).collect();
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
            if self.shared.borrow().any(Dirty::STRUCTURE) {
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
                s.take(Dirty::POLISH, |_| ())
                    .into_iter()
                    .map(|(o, ())| (depth(engine, o), o))
                    .collect()
            };
            if set.is_empty() {
                // The views that waited for the items in their delegates
                // lay their rows out now; what they make is polished next.
                POLISHING.set(false);
                let laid = crate::views::polish_ends(engine);
                {
                    let s = self.shared.borrow();
                    if !s.any(Dirty::POLISH) && !s.any(Dirty::STRUCTURE) {
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
                let Some((kind, told)) = self.items.get(obj).map(|e| {
                    let told = matches!(e.class.kind, Kind::Row | Kind::Column)
                        .then(|| e.effects.last().copied())
                        .flatten();
                    (e.class.kind, told)
                }) else {
                    continue;
                };
                let journaled = engine.is_journaled().then(std::time::Instant::now);
                let r = match kind {
                    Kind::Row => positioners::layout(engine, obj, Axis::Horizontal),
                    Kind::Column => positioners::layout(engine, obj, Axis::Vertical),
                    Kind::Managed => crate::layouts::polish(engine, obj),
                    Kind::Text => {
                        let read = (self.shared.borrow().marks.get(obj))
                            .and_then(|m| m.text.as_deref().cloned());
                        text::polish(engine, obj, &self.text, read)
                    }
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
                if let Some(started) = journaled
                    && kind.polishes()
                {
                    engine.journal_step(JournalKind::LaidOut, obj, Some(started.elapsed()));
                }
                if let Err(e) = r {
                    engine.log(
                        LogLevel::Warning,
                        POLISH_CATEGORY,
                        format!("polish failed: {e}"),
                    );
                }
                // Its effect was told of a change and read nothing since:
                // it reads the inputs again, which brings their bindings up
                // to date, and follows the children it has now.
                if let Some(n) = told {
                    // In a batch it runs at the batch's end, and a change
                    // until then is not told: that run marks it again.
                    if !engine.graph().batching()
                        && let Some(m) = self.shared.borrow_mut().marks.get_mut(obj)
                    {
                        m.retrack = true;
                    }
                    let _ = engine.graph().refresh(n);
                }
            }
        }
        POLISHING.set(false);
        crate::views::polish_ends(engine);
        let left = self.shared.borrow_mut().take(Dirty::POLISH, |_| ());
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
        if self.items.contains(obj) {
            return Ok(());
        }
        let class = ItemClass::of(engine, obj).ok_or_else(|| EngineError::dead(obj))?;
        if parent.is_none() {
            // A root or a layer: the engine's change hooks find the scene
            // from now on.
            engine.extension::<Scenes>().add(&self.shared);
        }
        init_item(engine, obj)?;
        let node = self.tree.insert_child(parent_node, index);
        let kind = class.kind;
        let may_scroll =
            parent.is_some_and(|p| self.items.get(p).is_some_and(|r| r.class.flickable));
        if kind == Kind::Editor {
            crate::text_input::init(engine, obj)?;
        }
        self.items_gen += 1;
        self.items.insert(
            obj,
            Record {
                node,
                class,
                parent,
                effects: SmallVec::new(),
                painting: false,
                content_effect: None,
                paint: None,
                size: kurbo::Size::ZERO,
                geom: None,
                place: (0.0, 0.0, false),
                list: None,
                list_effect: None,
                place_read: 0,
                may_scroll,
            },
        );
        // Before the effects, whose first runs mark it. Its place and
        // geometry are read at the frame, and from then on as their change
        // hooks tell.
        {
            let mut s = self.shared.borrow_mut();
            let mut marks = Marks::new();
            marks.inherit.mouse_area = class.mouse_area;
            s.marks.insert(obj, marks);
            s.set(obj, Dirty::GEOM, |_| ());
            s.placed.push(obj);
        }
        self.by_node.insert(node, obj);

        let mut effects = SmallVec::<[GraphNode; 8]>::new();
        // What it inherits is passed on by the hooks of its parent's
        // `visible` and `enabled` and by a reparent; here it takes it once.
        engine.untrack(|| inherit(engine, obj, &self.shared));
        // Its anchors have an effect of their own only once one is given.
        anchors::follow(engine, obj)?;
        if kind == Kind::Text {
            // What the text read as it was completed is handed over, not
            // read again; from now on each change is.
            let shared = Rc::clone(&self.shared);
            let sink: text::Sink = Rc::new(move |e, obj, t| text_polish(e, obj, t, &shared));
            effects.push(text::follow_into(engine, obj, sink)?);
        } else if matches!(kind, Kind::Row | Kind::Column) {
            // Told, not run: a positioner reads every child, and the
            // children of one change one at a time (all of them as a
            // hidden parent shows them), so a run per change would read
            // them all once per child. It reads them once more after it
            // is polished instead.
            let shared = Rc::clone(&self.shared);
            let told = Rc::downgrade(&self.shared);
            let n = engine.effect_told_in(
                obj,
                move |e| polish_effect(e, obj, kind, &shared),
                move || {
                    if let Some(shared) = told.upgrade() {
                        mark(&shared, |s| s.set(obj, Dirty::POLISH, |_| ()));
                    }
                },
            )?;
            // Last, where `QuickScene::polish` finds it: a field of its
            // own would make every item's record larger for the few
            // positioners.
            effects.push(n);
        } else if kind.polishes() {
            // A layout's own inputs say what changed (`layouts::read_inputs`).
            let shared = Rc::clone(&self.shared);
            effects.push(engine.effect_in(obj, move |e| polish_effect(e, obj, kind, &shared))?);
        }
        if let Some(e) = self.items.get_mut(obj) {
            e.effects = effects;
        }
        self.a11y.adopt(engine, obj, kind == Kind::Text);

        let children: Vec<ObjectId> = engine
            .untrack(|| shown_children(engine, obj))
            .into_iter()
            .filter(|c| shows(engine, *c))
            .collect();
        for (i, c) in children.into_iter().enumerate() {
            self.adopt(engine, c, Some(obj), node, i)?;
        }
        Ok(())
    }

    /// Forgets `obj` and its subtree; removes the render nodes if
    /// `remove_node` (only the top of the subtree needs it).
    fn unadopt(&mut self, engine: &Engine, obj: ObjectId, remove_node: bool) {
        let Some(record) = self.items.remove(obj) else {
            return;
        };
        // First, so nothing that follows marks it: it is not shown again.
        let marks = self.shared.borrow_mut().forget(obj);
        self.items_gen += 1;
        if record.class.kind == Kind::Text {
            // Its effect is among `effects`; this also forgets the sink.
            text::unfollow(engine, obj);
        }
        let mut effects = record.effects;
        effects.extend(record.content_effect.into_iter().chain(record.list_effect));
        engine.graph().dispose_all(&effects);
        anchors::unfollow(engine, obj);
        self.a11y.forget(engine, obj);
        if !engine.is_alive(obj) {
            crate::focus::tree::forget(engine, obj);
        }
        if engine.is_alive(obj) {
            let mut inh = marks.map(|m| m.inherit).unwrap_or_default();
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
            .node(record.node)
            .map(|n| n.children().to_vec())
            .unwrap_or_default();
        for k in kids {
            if let Some(c) = self.by_node.get(k).copied() {
                self.unadopt(engine, c, false);
            }
        }
        self.by_node.remove(record.node);
        if remove_node {
            let _ = self.tree.remove(record.node);
        }
        for g in [&mut self.pointer.passive, &mut self.pointer.hovered] {
            g.retain(|o| *o != obj);
        }
        if matches!(self.pointer.grab, Some(Grab::Mouse(o)) if o == obj) {
            self.pointer.grab = None;
        }
    }

    /// Step 8a: render-node structure follows the items' `children`.
    fn reconcile(&mut self, engine: &Engine) {
        let mut dirty: Vec<ObjectId> = (self.shared.borrow_mut().take(Dirty::STRUCTURE, |_| ()))
            .into_iter()
            .map(|(o, ())| o)
            .collect();
        // In creation order, so the same scene builds the same way each run.
        dirty.sort_unstable();
        let mut orphans = Vec::new();
        for p in dirty {
            if !engine.is_alive(p) {
                continue;
            }
            let Some(pnode) = self.items.get(p).map(|e| e.node) else {
                continue;
            };
            self.a11y.children_changed(p);
            let mut want: Vec<ObjectId> = engine
                .untrack(|| shown_children(engine, p))
                .into_iter()
                .filter(|c| shows(engine, *c))
                .collect();
            let mut seen = FxHashSet::default();
            want.retain(|c| seen.insert(*c));
            for (i, c) in want.iter().enumerate() {
                match self.items.get_mut(*c) {
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
                            // Whether it is a Flipable's side turned away
                            // is its parent's to say.
                            self.shared.borrow_mut().set(*c, Dirty::GEOM, |_| ());
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
            if self.items.get(o).is_some_and(|e| e.parent == Some(p)) {
                self.unadopt(engine, o, true);
            }
        }
    }

    /// Step 8b.
    /// Reads the place of each item whose `x` or `y` was told changed.
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
            let Some(e) = self.items.get_mut(obj) else {
                continue;
            };
            if std::mem::replace(&mut e.place_read, self.places_read) == self.places_read {
                continue;
            }
            // Both at one look at the object, which also says it lives.
            // Untracked: the change hooks tell of the next change.
            let xy = engine.untrack(|| engine.read_numbers(obj, [Item::X.index, Item::Y.index]));
            if let Some([x, y]) = xy {
                e.place = (
                    shown.get(obj, Item::X.index).unwrap_or(x),
                    shown.get(obj, Item::Y.index).unwrap_or(y),
                    e.may_scroll
                        && engine.untrack(|| crate::flickable::content_moving(engine, obj)),
                );
                // Moved here, while the entry is at hand: a new geometry or
                // transform list this frame sets it again from this place.
                if let Some(g) = &e.geom {
                    self.tree.set_local(
                        e.node,
                        local_transform(e.place, g, e.list(), self.scale_factor),
                    );
                }
                self.told_moved.push(obj);
            }
        }
    }

    fn apply_geometry(&mut self, engine: &Engine) {
        self.read_places(engine);
        // Each item's tree node is set on its own, so neither list needs
        // an order; what only moved is told apart while the maps stand.
        let (geoms, moved) = {
            let mut s = self.shared.borrow_mut();
            let mut moved = Vec::with_capacity(self.told_moved.len());
            // Before the places, so an item that also moved is told once.
            for (obj, ()) in s.take(Dirty::LIST, |_| ()) {
                let list = s.lists.remove(&obj);
                if let (Some(e), Some(list)) = (self.items.get_mut(obj), list) {
                    e.set_list(list);
                    if !s.is_set(obj, Dirty::GEOM) {
                        moved.push(obj);
                    }
                }
            }
            // Each was moved as its place was read.
            self.told_moved.clear();
            // With whether a hidden parent covers it, which is the
            // inherit effect's to say; the geometry is read below.
            let geoms: Vec<(ObjectId, (Geom, bool))> = s.take(Dirty::GEOM, |m| {
                (Geom::default(), m.inherit.visible.is_some())
            });
            (geoms, moved)
        };
        // Read where it is put, unborrowed and untracked, as the places
        // are: what a read brings up to date may mark the scene, and the
        // change hooks tell of the next change.
        let mut geoms = geoms;
        let animated = crate::animation::shown::shown(engine);
        engine.untrack(|| {
            for (obj, (g, _)) in &mut geoms {
                *g = Geom::read(engine, *obj, &animated);
            }
        });
        if engine.is_journaled() {
            for obj in geoms.iter().map(|g| g.0).chain(moved.iter().copied()) {
                engine.journal_step(JournalKind::Placed, obj, None);
            }
        }
        for (obj, (g, _)) in &geoms {
            if self
                .items
                .get(*obj)
                .is_some_and(|e| e.list_effect.is_some() != g.listed)
            {
                self.follow_list(engine, *obj, g.listed);
            }
        }
        for (obj, (g, covered)) in geoms {
            let Some(e) = self.items.get_mut(obj) else {
                continue;
            };
            e.geom = Some(g);
            let node = e.node;
            let size = kurbo::Size::new(g.w.max(0.0), g.h.max(0.0));
            self.tree.set_local(
                node,
                local_transform(e.place, &g, e.list(), self.scale_factor),
            );
            if size != e.size {
                e.size = size;
                if e.class.kind.paints() && e.painting {
                    self.tree.mark_content_dirty(node);
                }
            }
            self.tree.set_size(node, size);
            self.tree
                .set_opacity(node, g.opacity.clamp(0.0, 1.0) as f32);
            // An item that reads false only because a parent is hidden is
            // left shown in the tree: the parent prunes it anyway, and a
            // `MultiEffect` drawing that parent shows it as Qt does.
            self.tree
                .set_visible(node, shown(&g, covered, self.pooled.contains(&obj)));
            let drawn = !self.hidden.contains(&obj) && !self.culled.0.contains(&obj);
            self.tree.set_drawn(node, drawn);
            self.tree
                .set_clip(node, g.clip.then(|| ClipSpec::bounds(size)));
            if let Some(Paint::Editor(ed)) = &e.paint {
                self.tree
                    .set_content_clip(node, ed.clip(size).map(ClipSpec::rect));
            }
            self.tree.set_z(node, g.z as f32);
        }
        // What only moved or turned: its transform, and nothing else.
        for obj in moved {
            if let Some(e) = self.items.get(obj)
                && let Some(g) = &e.geom
            {
                self.tree.set_local(
                    e.node,
                    local_transform(e.place, g, e.list(), self.scale_factor),
                );
            }
        }
    }

    /// Makes or drops the effect that follows `obj`'s `transform` list,
    /// as the list came to have entries or lost them.
    fn follow_list(&mut self, engine: &Engine, obj: ObjectId, listed: bool) {
        let Some(e) = self.items.get_mut(obj) else {
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
            let ran = {
                let mut s = self.shared.borrow_mut();
                s.clear(obj, Dirty::LIST, |_| ())
                    .and_then(|()| s.lists.remove(&obj))
            };
            ran.unwrap_or_else(|| engine.untrack(|| crate::transform::list_map(engine, obj)))
        } else {
            let mut s = self.shared.borrow_mut();
            if s.clear(obj, Dirty::LIST, |_| ()).is_some() {
                s.lists.remove(&obj);
            }
            crate::transform::Projective::IDENTITY
        };
        e.set_list(list);
    }

    /// Tells a journaling engine which items step 8c redraws.
    fn journal_content(&self, engine: &Engine) {
        if !engine.is_journaled() {
            return;
        }
        let mut redrawn = self.shared.borrow().queued[Dirty::CONTENT.slot()].clone();
        redrawn.sort_unstable();
        for obj in redrawn {
            engine.journal_step(JournalKind::Redrawn, obj, None);
        }
    }

    /// Step 8c: rebuild the display lists of items whose paint inputs or
    /// size changed.
    fn apply_content(&mut self) {
        let mut content: Vec<(ObjectId, Paint)> = {
            let mut s = self.shared.borrow_mut();
            let taken = s.take(Dirty::CONTENT, |_| ());
            (taken.into_iter())
                .filter_map(|(o, ())| Some((o, s.contents.remove(&o)?)))
                .collect()
        };
        content.sort_unstable_by_key(|c| c.0);
        let mut sources: FxHashSet<ObjectId> = FxHashSet::default();
        let mut shaders = false;
        for (obj, p) in content {
            if let Some(e) = self.items.get_mut(obj) {
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
                e.paint = Some(p);
            }
        }
        // A layer no effect samples any more is no longer taken.
        if shaders {
            let sampled: FxHashSet<ObjectId> = (self.items.values())
                .filter_map(|e| match &e.paint {
                    Some(Paint::Shader(fx)) => Some(fx.layers().map(|(item, _)| item)),
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
            for e in self.items.values() {
                if let Some(Paint::Shader(fx)) = &e.paint
                    && fx.sources().any(|s| sources.contains(&s))
                {
                    self.tree.mark_content_dirty(e.node);
                }
            }
        }
        self.refresh_hidden();
        let items = &self.items;
        let paint = |o: ObjectId| items.get(o).and_then(|e| e.paint.as_ref());
        let layers = &self.layer_base;
        let by_node = &self.by_node;
        let mut te = self.text.borrow_mut();
        self.tree.rebuild_content(self.scale_factor, |cx, b| {
            let Some(p) = by_node.get(cx.node).and_then(|o| paint(*o)) else {
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
                    |s| match (paint(s), layers.get(&s)) {
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
        let items = &self.items;
        self.effect_base.retain(|o, _| items.contains(*o));
        let redrawn = self.draw_shader_sources();
        if self.effect_base.is_empty() {
            if redrawn {
                self.apply_content();
            }
            return;
        }
        // Who has children changes only with the items, not every frame.
        if self.parents.0 != self.items_gen {
            let parents = self.items.values().filter_map(|e| e.parent).collect();
            self.parents = (self.items_gen, parents);
        }
        let parents = &self.parents.1;
        // What a ShaderEffectSource or a layer samples is drawn even while
        // hidden, and so is each effect in it.
        let sampled: FxHashSet<NodeId> = (self.source_base.values())
            .filter_map(|(s, _)| s.source)
            .chain(self.layer_base.keys().copied())
            .filter_map(|o| self.items.get(o).map(|e| e.node))
            .collect();
        let mut redrawn = redrawn;
        for (obj, (base, seen)) in &mut self.effect_base {
            let Some(src) = base.source().filter(|s| parents.contains(s)) else {
                continue;
            };
            let (Some(src_node), Some(fx_node)) = (
                self.items.get(src).map(|e| e.node),
                self.items.get(*obj).map(|e| e.node),
            ) else {
                continue;
            };
            // A hidden effect is drawn when it is shown: closing a panel
            // with a shadow is not to cost a picture of it.
            if !in_view(&self.tree, fx_node, &sampled) {
                continue;
            }
            let Some(snap) = self.tree.compose_subtree(src_node) else {
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
            if let Some(e) = self.items.get_mut(*obj) {
                e.paint = Some(Paint::Effect(paint));
            }
            self.tree.mark_content_dirty(fx_node);
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
        let items = &self.items;
        self.source_base.retain(|o, _| items.contains(*o));
        let mut taken: FxHashSet<ObjectId> = FxHashSet::default();
        let mut again = false;
        for (obj, (base, seen)) in &mut self.source_base {
            let asked = seen.as_ref().is_some_and(|s| s.update != base.update);
            if !base.live && seen.is_some() && !asked {
                continue;
            }
            let (Some(fx_node), Some(src)) = (self.items.get(*obj).map(|e| e.node), base.source)
            else {
                continue;
            };
            let Some(src_node) = self.items.get(src).map(|e| e.node) else {
                continue;
            };
            let Some(snap) = self.tree.compose_subtree(src_node) else {
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
                    at = self.items.get(o).and_then(|e| e.parent);
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
            if let Some(e) = self.items.get_mut(*obj) {
                e.paint = Some(Paint::Source(shown));
            }
            self.tree.mark_content_dirty(fx_node);
            taken.insert(*obj);
        }
        taken.extend(self.draw_layers());
        if again {
            mark(&self.shared, |s| s.pictures = true);
        }
        if taken.is_empty() {
            return false;
        }
        for e in self.items.values() {
            if let Some(Paint::Shader(fx)) = &e.paint
                && fx.sources().any(|s| taken.contains(&s))
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
            let Some(entry) = self.items.get(*item) else {
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
            .filter(|(o, (b, _))| b.hide && self.items.contains(**o))
            .filter_map(|(_, (b, _))| b.source)
            .collect();
        if now == self.hidden {
            return;
        }
        let changed: Vec<ObjectId> = now.symmetric_difference(&self.hidden).copied().collect();
        self.hidden = now;
        for obj in changed {
            if let Some(e) = self.items.get(obj) {
                self.tree.set_drawn(e.node, self.drawn(obj));
            }
        }
    }

    /// Whether `obj` is painted, `visible` aside.
    fn drawn(&self, obj: ObjectId) -> bool {
        !self.hidden.contains(&obj) && !self.culled.0.contains(&obj)
    }

    /// Stops painting the rows views made and do not show, and paints
    /// again those they show now; and leaves out of the tree what waits
    /// in a view's pool, and puts back what left it.
    fn refresh_culled(&mut self, engine: &Engine) {
        let generation = crate::views::cull::generation(engine);
        if generation == self.culled.1 {
            return;
        }
        let now = crate::views::cull::all(engine);
        let changed: Vec<ObjectId> = now.symmetric_difference(&self.culled.0).copied().collect();
        self.culled = (now, generation);
        for obj in changed {
            if let Some(e) = self.items.get(obj) {
                self.tree.set_drawn(e.node, self.drawn(obj));
            }
        }
        let pooled = crate::views::cull::pooled(engine);
        let changed: Vec<ObjectId> = pooled.symmetric_difference(&self.pooled).copied().collect();
        self.pooled = pooled;
        for obj in changed {
            // Not laid out yet: the geometry, when it comes, reads the pool.
            let Some((node, Some(g))) = self.items.get(obj).map(|e| (e.node, e.geom)) else {
                continue;
            };
            let inherit =
                (self.shared.borrow().marks.get(obj)).is_some_and(|m| m.inherit.visible.is_some());
            self.tree
                .set_visible(node, shown(&g, inherit, self.pooled.contains(&obj)));
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
        self.items
            .keys()
            .filter(|obj| {
                let mut at = Some(*obj);
                while let Some(o) = at {
                    if sources.binary_search(&o).is_ok() {
                        return true;
                    }
                    at = self.items.get(o).and_then(|e| e.parent);
                }
                false
            })
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
            self.items.keys().collect()
        };
        self.content_looked += looked.len() as u64;
        let mut installed = false;
        // A dropped display list changes the tree, so the frame composes
        // again and does not hand out a snapshot that is already stale.
        let mut dropped = false;
        let mut failed = Vec::new();
        for obj in looked {
            let Some(e) = self.items.get_mut(obj) else {
                continue;
            };
            let kind = e.class.kind;
            if !kind.paints() {
                continue;
            }
            let shown = visible.contains(&obj);
            // A Shape paints where its paths are, which need not be where
            // the item is: only its display list tells whether it is in the
            // frame, so it has one from the start and keeps it.
            // A ShaderEffectSource is often not shown itself, only
            // sampled: it reads its source all the same.
            let shown = shown || matches!(kind, Kind::Shape | Kind::Source);
            match (shown, e.painting) {
                (true, false) if kind == Kind::Text => {
                    // An effect here would read nothing: what the text
                    // paints is what its polish effect read, and that
                    // effect tells of each change from now on.
                    e.painting = true;
                    engine.untrack(|| content_effect(engine, obj, Kind::Text, &self.shared));
                    installed = true;
                }
                (true, false) => {
                    let shared = Rc::clone(&self.shared);
                    let o = obj;
                    match engine.effect_in(o, move |en| content_effect(en, o, kind, &shared)) {
                        Ok(n) => {
                            e.painting = true;
                            e.content_effect = Some(n);
                            installed = true;
                        }
                        Err(err) => failed.push(err),
                    }
                }
                (false, true) => {
                    if let Some(n) = e.content_effect.take() {
                        let _ = engine.graph().dispose(n);
                    }
                    e.painting = false;
                    if kind == Kind::Text
                        && let Some(m) = self.shared.borrow_mut().marks.get_mut(obj)
                    {
                        m.text_shown = false;
                    }
                    // Text that left the screen keeps its numbers only: the
                    // display list holds the glyph runs, so it goes too
                    // (ARCHITECTURE §7.5). Coming back paints it again.
                    if kind == Kind::Text {
                        e.paint = None;
                        self.tree.set_content(e.node, None);
                        dropped = true;
                    }
                    // A picture likewise: the display list is what keeps
                    // its pixels from being dropped under memory pressure
                    // (ARCHITECTURE §8.5).
                    if kind == Kind::Image {
                        e.paint = None;
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
        self.note_input();
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

    /// The cursor after `ev`: the shape of the topmost thing under the
    /// pointer that has one ([`cursor_of`]). Over the end of a TableView's
    /// column it is the TableView's, kept while the end is dragged, off it
    /// too.
    fn cursor_for(&mut self, engine: &Engine, ev: &PointerEvent) -> i32 {
        self.pointer.cursor_shapes = engine.extension::<CursorShapes>().0.get();
        if matches!(ev.kind, PointerKind::Leave | PointerKind::Cancel) {
            self.pointer.at = None;
            self.pointer.cursor_resizes = false;
            return 0;
        }
        self.pointer.at = Some(ev.pos);
        if !ev.buttons.is_empty() && self.pointer.cursor_resizes {
            return self.pointer.cursor;
        }
        // A move has found what is under it already, in this frame's
        // picture; what else comes looks once.
        if self.pointer.hits_at != Some((ev.pos, self.frame)) {
            self.pointer.hits = self.hits(engine, ev.pos);
            self.pointer.hits_at = Some((ev.pos, self.frame));
        }
        let hits = &self.pointer.hits;
        let resize = if ev.buttons.is_empty() {
            crate::views::table::resize_cursor(engine, hits, |o| local_point(engine, o, ev.pos))
        } else {
            None
        };
        self.pointer.cursor_resizes = resize.is_some();
        resize.unwrap_or_else(|| cursor_of(engine, hits))
    }

    /// At the frame, after a `cursorShape` changed: the cursor of what the
    /// pointer was last over. What has come under a pointer that stays
    /// shows at its next move, as on qml6.
    fn cursor_again(&mut self, engine: &Engine) {
        let changes = engine.extension::<CursorShapes>().0.get();
        if changes == self.pointer.cursor_shapes {
            return;
        }
        self.pointer.cursor_shapes = changes;
        let Some(at) = self.pointer.at else {
            return;
        };
        if self.pointer.cursor_resizes || self.pointer.hits_at.is_none_or(|(p, _)| p != at) {
            return;
        }
        self.pointer.cursor = cursor_of(engine, &self.pointer.hits);
    }

    fn deliver(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        match ev.kind {
            PointerKind::Press => self.press(engine, ev),
            PointerKind::Move => self.moved(engine, ev),
            PointerKind::Release => self.release(engine, ev),
            PointerKind::Cancel => {
                let mut handled = self.cancel(engine, ev);
                for h in std::mem::take(&mut self.pointer.watched) {
                    tap_ungrab(
                        engine,
                        h,
                        grab_transition::CANCEL_GRAB_PASSIVE,
                        ev.pos,
                        point_state::UPDATED,
                    );
                    handled = true;
                }
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
        self.note_input();
        let hits = self.hits(engine, ev.pos);
        crate::wheel::deliver(engine, &hits, ev, buttons)
    }

    /// Offers a pinch the platform recognised (a touchpad's) to the
    /// `PinchHandler`s under it. Returns whether one took it.
    pub fn pinch_event(&mut self, engine: &Engine, g: &crate::handlers::PinchGesture) -> bool {
        self.note_input();
        let hits = self.hits(engine, g.center);
        crate::handlers::pinch_gesture(engine, g, &hits)
    }

    /// The items under `pos` (scene coordinates), topmost first, as a
    /// press there meets them: what an open modal popup shuts off is left
    /// out. For tools that ask what covers what (the design audit).
    pub fn items_at(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
        self.hits(engine, pos)
    }

    /// Whether `obj` is a row a view made and does not show (or keeps in
    /// its pool): it and what is inside it are neither painted nor laid
    /// out until the view shows it. For tools that ask what is shown (the
    /// design audit).
    pub fn is_culled(&self, obj: ObjectId) -> bool {
        self.culled.0.contains(&obj) || self.pooled.contains(&obj)
    }

    /// The items drawn under `pos`, topmost first, those a modal popup
    /// shuts off included.
    pub fn items_drawn_at(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
        let Some(snap) = &self.snapshot else {
            return Vec::new();
        };
        hit_test(snap, pos)
            .filter_map(|n| self.by_node.get(n).copied())
            .filter(|o| engine.is_alive(*o))
            .collect()
    }

    /// Items under `pos`, topmost first.
    pub(crate) fn hits(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
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
        self.pointer.watched.clear();
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
        for &obj in &hits {
            // Drag and point handlers grab passively, and see it first.
            handled |= crate::handlers::press(engine, obj, ev);
            // An item's pointer handlers see the event before the item.
            for h in resources(engine, obj) {
                if !is_a::<TapHandler>(engine, h)
                    || !engine.read(h, TapHandler::ENABLED.index).to_boolean()
                    || num(engine, h, TapHandler::ACCEPTED_BUTTONS.index) as i32
                        & button.bits() as i32
                        == 0
                    || !crate::input::TAP_ACCEPTED.fit(engine, h, ev.source, ev.modifiers)
                {
                    continue;
                }
                // As in Qt the point is there when `pressed` changes, and
                // says so after it.
                crate::input::tap_point_pressed(engine, h, ev);
                // The grab is told first, as in Qt.
                let policy = num(engine, h, TapHandler::GESTURE_POLICY.index) as i32;
                let passive = policy == gesture_policy::DRAG_THRESHOLD;
                let grab = if passive {
                    grab_transition::GRAB_PASSIVE
                } else {
                    grab_transition::GRAB_EXCLUSIVE
                };
                let point =
                    crate::input::tap_grab_point(engine, h, ev.pos, point_state::PRESSED, true);
                emit(engine, h, "grabChanged", &[Value::Int(grab), point]);
                if !engine.is_alive(h) {
                    continue;
                }
                set(engine, h, TapHandler::PRESSED.index, Value::Bool(true));
                crate::input::tap_point_changed(engine, h);
                crate::long_press::pressed(engine, h, ev.time);
                if passive {
                    self.pointer.passive.push(h);
                    self.pointer.watched.push(h);
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
        if !handled
            && button == MouseButtons::RIGHT
            && let Some((o, _)) = crate::context_menu::taker(engine, hits)
        {
            self.pointer.context = Some(o);
            return true;
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
        set_mouse_pos(engine, m, local);
        let entered = set_contains(engine, m, true);
        if entered {
            emit(engine, m, "entered", &[]);
        }
        // As in Qt, `pressed` comes before `pressedChanged`.
        let taken = engine.batch(|| {
            set_pressed(engine, m, true);
            set(
                engine,
                m,
                MouseArea::PRESSED_BUTTONS.index,
                Value::Int(button.bits() as i32),
            );
            let taken = emit_mouse(engine, m, "pressed", ev, local, false);
            if !taken {
                set_pressed(engine, m, false);
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
            }
            taken
        });
        if !taken {
            // Rejected: the press goes on to the items below.
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
                cancel_tap(engine, t, ev.pos, None);
            }
            self.pointer.grab = Some(Grab::Item(h));
            handled = true;
        }
        if (ev.pos - self.pointer.press_pos).hypot()
            > crate::system_prefs::start_drag_distance(engine)
        {
            // A drag is not a tap.
            for h in std::mem::take(&mut self.pointer.passive) {
                cancel_tap(engine, h, ev.pos, None);
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
                    cancel_tap(engine, h, ev.pos, Some(LetGo::Moved));
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
        self.pointer.hits = hits;
        self.pointer.hits_at = Some((ev.pos, self.frame));
        handled || !self.pointer.passive.is_empty()
    }

    fn release(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        if let Some(o) = self.pointer.context.take() {
            self.pointer.grab = None;
            if engine.is_alive(o)
                && let Some((o, hook)) = crate::context_menu::taker(engine, [o])
            {
                let at = local_point(engine, o, ev.pos);
                (hook.request)(engine, o, at, crate::context_menu::ContextMenuReason::Mouse);
            }
            return true;
        }
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
                let was_held = crate::press_and_hold::ended(engine, m);
                let released = MouseEvent {
                    was_held,
                    ..mouse_event(ev, local, inside)
                };
                // As in Qt, `released` reads `pressed` as false but comes
                // before `pressedChanged`: what is bound to `pressed`, a
                // `Drag.active` that the handler drops, still holds.
                engine.batch(|| {
                    set_pressed(engine, m, false);
                    set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                    emit_mouse_event(engine, m, "released", released);
                });
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
                    tap_ungrab(
                        engine,
                        h,
                        grab_transition::UNGRAB_EXCLUSIVE,
                        ev.pos,
                        point_state::RELEASED,
                    );
                } else {
                    cancel_tap(engine, h, ev.pos, Some(LetGo::Released));
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
        // After every tap, as in Qt, and for a tap that was cancelled too.
        for h in std::mem::take(&mut self.pointer.watched) {
            tap_ungrab(
                engine,
                h,
                grab_transition::UNGRAB_PASSIVE,
                ev.pos,
                point_state::RELEASED,
            );
            handled = true;
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
        let mut handled = self.pointer.context.take().is_some();
        match self.pointer.grab.take() {
            Some(Grab::Mouse(m)) if engine.is_alive(m) => {
                crate::press_and_hold::ended(engine, m);
                set_pressed(engine, m, false);
                set(engine, m, MouseArea::PRESSED_BUTTONS.index, Value::Int(0));
                if !hover_enabled(engine, m) {
                    set_contains(engine, m, false);
                }
                emit(engine, m, "canceled", &[]);
                crate::mouse_drag::released(engine, m);
                handled = true;
            }
            Some(Grab::Tap(h)) if engine.is_alive(h) => {
                cancel_tap(engine, h, ev.pos, Some(LetGo::Taken));
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
            cancel_tap(engine, h, ev.pos, None);
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
        let point = crate::input::tap_event_point(engine, h, ev.pos, point_state::RELEASED);
        let args = [point, Value::Int(button.bits() as i32)];
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

/// Follows the entries of a `transform` list. Made only for an item whose
/// list has some (see [`QuickScene::follow_list`]).
fn list_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    let list = crate::transform::list_map(e, obj);
    mark(shared, |s| s.list(obj, list));
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
            text_polish(e, obj, TextInputs::read(e, obj), shared);
            return;
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
    let retrack =
        (shared.borrow_mut().marks.get_mut(obj)).is_some_and(|m| std::mem::take(&mut m.retrack));
    if !retrack {
        mark(shared, |s| s.set(obj, Dirty::POLISH, |_| ()));
    }
}

/// What the inputs `t` a Text read make the scene do: lay it out again,
/// paint it again, or nothing.
fn text_polish(e: &Engine, obj: ObjectId, t: TextInputs, shared: &SharedRc) {
    // A new text was measured as it was completed; laid out as it still
    // is, it is not polished to be measured again.
    let measured = text::completion_measured(e, obj, &t);
    let (same, same_layout, shown) = match shared.borrow().marks.get(obj) {
        Some(m) => match &m.text {
            Some(was) => (**was == t, was.lays_out_as(&t), m.text_shown),
            None => (false, false, m.text_shown),
        },
        // Not shown by the scene.
        None => return,
    };
    // Something it reads changed and it came out the same (the width its
    // own measuring gave it, where it does not elide to it): neither what
    // it measures nor how it looks moves. A document changes inside (a
    // picture arrives) without changing the inputs.
    if t.rich.is_none() && same {
        return;
    }
    // Only how it is painted changed (a colour): it measures the same.
    let polish = !measured && !(t.rich.is_none() && same_layout);
    // A text that paints nothing (hidden, or empty: a field's placeholder)
    // is measured when one of its sizes is read, not by the frame's polish
    // and the second round of bindings its writes would start. One that is
    // only not shown yet (new, as a view makes its rows) is measured by
    // the polish, which its first frame needs anyway.
    let blank = t.rich.is_none() && t.text.is_empty();
    if polish && (blank || (!shown && !e.untrack(|| e.read(obj, Item::VISIBLE.index)).to_boolean()))
    {
        let lazy = t.clone();
        let t = Rc::new(t);
        let keep = |s: &mut Shared, t: Rc<TextInputs>| {
            if let Some(m) = s.marks.get_mut(obj) {
                m.text = Some(t);
            }
            s.clear(obj, Dirty::POLISH, |_| ());
        };
        if shown {
            mark(shared, |s| {
                s.paint(obj, Paint::Text(Rc::clone(&t)));
                keep(s, t);
            });
        } else {
            // Nothing for a frame to do.
            keep(&mut shared.borrow_mut(), t);
        }
        if let Err(err) = e.untrack(|| text::measure_on_read(e, obj, lazy)) {
            e.log(ljos_engine::LogLevel::Warning, "qml", err.to_string());
        }
        return;
    }
    let t = Rc::new(t);
    if !polish && !shown {
        // Nothing for a frame to do.
        if let Some(m) = shared.borrow_mut().marks.get_mut(obj) {
            m.text = Some(t);
        }
        return;
    }
    mark(shared, |s| {
        if shown {
            s.paint(obj, Paint::Text(Rc::clone(&t)));
        }
        if let Some(m) = s.marks.get_mut(obj) {
            m.text = Some(t);
        }
        if polish {
            s.set(obj, Dirty::POLISH, |_| ());
        }
    });
}

/// An Animator showed another value of `prop`, or stopped showing one:
/// whatever draws that property reads it again.
fn shown_changed(e: &Engine, obj: ObjectId, prop: PropIndex, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    if prop == Item::X.index || prop == Item::Y.index {
        mark(shared, |s| s.placed.push(obj));
    } else if [Item::OPACITY, Item::ROTATION, Item::SCALE]
        .iter()
        .any(|p| p.index == prop)
    {
        mark(shared, |s| s.set(obj, Dirty::GEOM, |_| ()));
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
        Kind::Text => {
            // Painted from what the polish effect read, which tells it of
            // every change from now on; called untracked, not as an effect.
            // Read here only where that effect has not run yet: when it
            // does, it sees the text shown and paints it again.
            let read = shared.borrow_mut().marks.get_mut(obj).and_then(|m| {
                m.text_shown = true;
                m.text.clone()
            });
            Paint::Text(read.unwrap_or_else(|| Rc::new(TextInputs::read(e, obj))))
        }
        Kind::Image => Paint::Image(crate::image::ImagePaint::read_on_screen(e, obj)),
        Kind::Editor => Paint::Editor(Box::new(crate::text_input::EditorPaint::read(e, obj))),
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
    mark(shared, |s| s.paint(obj, p));
}

/// `visible` and `enabled` read false under a hidden or disabled parent
/// (Qt documents both as inherited). The item's own value sits below an
/// override layer and comes back when the parent turns on again. Called
/// at adoption and by [`inherited`], not as an effect: an effect an item
/// cost a node and a run for what changes seldom.
fn inherit(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        return;
    }
    // Each change to the stack tells the item's own hook, which asks for
    // this again: what it would do is what is being done.
    let Some((covered, mouse_area)) = shared.borrow_mut().marks.get_mut(obj).and_then(|m| {
        let i = &mut m.inherit;
        (!std::mem::replace(&mut i.busy, true)).then_some((i.visible.is_some(), i.mouse_area))
    }) else {
        return;
    };
    let parent = e.visual_parent(obj).filter(|p| shows(e, *p));
    for p in [Item::VISIBLE.index, Item::ENABLED.index] {
        let parent_on = if p == Item::ENABLED.index {
            // `MouseArea.enabled` is the area's own switch: it neither
            // follows a disabled ancestor nor disables what is inside.
            mouse_area || parent.is_none_or(|q| item_enabled(e, q))
        } else {
            parent.is_none_or(|q| e.read(q, p).to_boolean())
        };
        if let Err(err) = inherit_one(e, obj, p, shared, parent_on) {
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
    let uncovered = shared.borrow_mut().marks.get_mut(obj).is_some_and(|m| {
        m.inherit.busy = false;
        m.inherit.visible.is_none()
    });
    if covered && uncovered {
        mark(shared, |s| s.set(obj, Dirty::GEOM, |_| ()));
    }
}

/// The override that covers `p` of `obj` in this scene; `None` outside it.
fn cover(shared: &SharedRc, obj: ObjectId, p: PropIndex) -> Option<Option<OverrideId>> {
    let s = shared.borrow();
    let m = s.marks.get(obj)?;
    Some(if p == Item::VISIBLE.index {
        m.inherit.visible
    } else {
        m.inherit.enabled
    })
}

fn set_cover(shared: &SharedRc, obj: ObjectId, p: PropIndex, id: Option<OverrideId>) {
    if let Some(m) = shared.borrow_mut().marks.get_mut(obj) {
        if p == Item::VISIBLE.index {
            m.inherit.visible = id;
        } else {
            m.inherit.enabled = id;
        }
    }
}

fn inherit_one(
    e: &Engine,
    obj: ObjectId,
    p: PropIndex,
    shared: &SharedRc,
    parent_on: bool,
) -> Result<(), EngineError> {
    let Some(slot) = cover(shared, obj, p) else {
        return Ok(());
    };
    let own = e.read(obj, p).to_boolean();
    match (parent_on, slot) {
        (true, None) => Ok(()),
        (true, Some(id)) => {
            set_cover(shared, obj, p, None);
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
                let id = push_off(e, obj, p)?;
                set_cover(shared, obj, p, Some(id));
            }
            Ok(())
        }
        (false, Some(id)) => {
            if own {
                // `true` was assigned while the parent is off: keep it as
                // the own value, and keep reading false.
                set_cover(shared, obj, p, None);
                e.remove_override(obj, p, id, RestoreMode::None)?;
                let id = push_off(e, obj, p)?;
                set_cover(shared, obj, p, Some(id));
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

/// The `Qt.CursorShape` the pointer shows over `hits` (topmost first): the
/// first that has one gives it. A handler of the item that is hovered or
/// active and was given a shape comes before the item itself
/// ([`crate::handlers::cursors`]); a MouseArea always has one, the arrow
/// until given another, also while its own `enabled` is false, but not
/// inside a disabled item (observed on qml6, `features/input/pointer-cursor.feature`).
fn cursor_of(e: &Engine, hits: &[ObjectId]) -> i32 {
    let handlers = crate::handlers::cursors(e);
    for &o in hits {
        if !e.is_alive(o) {
            continue;
        }
        if let Some(&(_, shape)) = handlers.iter().find(|(item, _)| *item == o) {
            return shape;
        }
        if is_a::<MouseArea>(e, o) && e.visual_parent(o).is_none_or(|p| item_enabled(e, p)) {
            return e.read(o, MouseArea::CURSOR_SHAPE.index).to_number() as i32;
        }
    }
    0
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
    show_contains_press(engine, m);
    true
}

/// Sets `pressed`, and `containsPress` with it.
pub(crate) fn set_pressed(engine: &Engine, m: ObjectId, v: bool) {
    set(engine, m, MouseArea::PRESSED.index, Value::Bool(v));
    show_contains_press(engine, m);
}

fn show_contains_press(engine: &Engine, m: ObjectId) {
    let on = engine.untrack(|| {
        engine.read(m, MouseArea::PRESSED.index).to_boolean()
            && engine.read(m, MouseArea::CONTAINS_MOUSE.index).to_boolean()
    });
    set(engine, m, MouseArea::CONTAINS_PRESS.index, Value::Bool(on));
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

/// How a TapHandler that held the point alone let go of it.
#[derive(Clone, Copy)]
enum LetGo {
    /// The pointer left its bounds.
    Moved,
    /// The button came up outside them.
    Released,
    /// Something else took the point.
    Taken,
}

/// Tells that the TapHandler `h` let go of the point at `at`.
fn tap_ungrab(engine: &Engine, h: ObjectId, transition: i32, at: kurbo::Point, state: i32) {
    if engine.is_alive(h) {
        let point = crate::input::tap_grab_point(engine, h, at, state, false);
        emit(engine, h, "grabChanged", &[Value::Int(transition), point]);
    }
}

/// `let_go`: how a handler that held the point alone let go of it, told
/// between `pressed` and `canceled` as in Qt. A handler that only watched
/// the point lets go of it at the release.
fn cancel_tap(engine: &Engine, h: ObjectId, pos: kurbo::Point, let_go: Option<LetGo>) {
    if !engine.is_alive(h) {
        return;
    }
    crate::long_press::ended(engine, h);
    set(engine, h, TapHandler::PRESSED.index, Value::Bool(false));
    if let Some(how) = let_go
        && num(engine, h, TapHandler::GESTURE_POLICY.index) as i32 != gesture_policy::DRAG_THRESHOLD
    {
        let (transition, state) = match how {
            LetGo::Moved => (grab_transition::UNGRAB_EXCLUSIVE, point_state::UPDATED),
            LetGo::Released => (grab_transition::UNGRAB_EXCLUSIVE, point_state::RELEASED),
            LetGo::Taken => (grab_transition::CANCEL_GRAB_EXCLUSIVE, point_state::UPDATED),
        };
        tap_ungrab(engine, h, transition, pos, state);
        if !engine.is_alive(h) {
            return;
        }
    }
    // As in Qt a handler that held the press alone has an empty point when
    // it says `canceled`; one that only watched still has the point.
    let alone =
        num(engine, h, TapHandler::GESTURE_POLICY.index) as i32 != gesture_policy::DRAG_THRESHOLD;
    let point = crate::input::tap_event_point(engine, h, pos, point_state::UPDATED);
    let mut changed = alone && crate::input::tap_point_clear(engine, h);
    emit(engine, h, "canceled", &[point]);
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
/// Whether `node` and the nodes it is in are shown, or one of them is
/// `sampled`.
fn in_view(tree: &RenderTree, node: NodeId, sampled: &FxHashSet<NodeId>) -> bool {
    let mut at = Some(node);
    while let Some(id) = at {
        let Some(n) = tree.node(id) else {
            return false;
        };
        if sampled.contains(&id) {
            return true;
        }
        if n.is_pruned() {
            return false;
        }
        at = n.parent();
    }
    true
}

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
/// Whether `obj` is `root` or shown under it.
fn shown_within(engine: &Engine, obj: ObjectId, root: ObjectId) -> bool {
    let mut at = Some(obj);
    while let Some(o) = at {
        if o == root {
            return true;
        }
        at = engine.visual_parent(o);
    }
    false
}

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

#[cfg(all(test, target_pointer_width = "64"))]
mod sizes {
    /// Every item the scene shows has marks and a record, each in a list
    /// of its own, so what they hold inline every item pays for. What is
    /// large and that few items have is held apart: a text's inputs, the
    /// content and list a mark carries, the map of a `transform` list and
    /// a text field's paint. Each was in the marks or the record, which
    /// were 688 and 576 bytes (B29).
    #[test]
    fn an_items_marks_and_record_stay_small() {
        assert_eq!(size_of::<super::Marks>(), 56);
        assert_eq!(size_of::<super::Record>(), 328);
        assert_eq!(size_of::<super::Paint>(), 104);
    }
}
