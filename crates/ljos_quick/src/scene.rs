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
    Engine, EngineError, NodeId as GraphNode, ObjectId, OverrideContent, OverrideId, Point,
    PropIndex, Provenance, RestoreMode, Value,
};
use ljos_render::{ClipSpec, FrameSnapshot, NodeId, RenderTree, hit_test, kurbo};
use ljos_runtime::Time;

use crate::anchors::{self, AnchorState};
use crate::enums::gesture_policy;
use crate::input::{MouseArea, MouseButtons, MouseEvent, PointerEvent, PointerKind, TapHandler};
use crate::item::{
    Item, compose_transform, init_item, is_a, is_item, num, scene_transform, write_computed,
};
use crate::log::QuickLog;
use crate::positioners::{self, Axis, Column, Row};
use crate::rectangle::{Rectangle, RectanglePaint};
use crate::text::{self, FixedAdvanceMeasurer, Text, TextInputs, TextMeasurer};
use crate::tree::{child_items, resources, set_parent_item};

/// Log category for polish warnings.
pub const POLISH_CATEGORY: &str = "qt.quick.polish";
/// Log category for scene errors (failed effects, bad trees).
pub const SCENE_CATEGORY: &str = "qt.quick.scene";
/// Polish passes per frame before the scene gives up and warns.
pub const MAX_POLISH_PASSES: usize = 10;
/// Qt's default `QStyleHints::mouseDoubleClickInterval`.
pub const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);
/// Qt's default `QStyleHints::startDragDistance`, in logical pixels.
pub const DRAG_THRESHOLD: f64 = 10.0;

/// A host hook for pipeline step 3 (timers and animations): it advances
/// everything due at `now` and returns when it next needs a frame.
///
/// SHIM for contract item 4 (`Engine::set_clock` / `advance_timers`).
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
    Row,
    Column,
}

impl Kind {
    fn of(engine: &Engine, obj: ObjectId) -> Kind {
        if is_a::<Text>(engine, obj) {
            Kind::Text
        } else if is_a::<Rectangle>(engine, obj) {
            Kind::Rectangle
        } else if is_a::<Row>(engine, obj) {
            Kind::Row
        } else if is_a::<Column>(engine, obj) {
            Kind::Column
        } else {
            Kind::Item
        }
    }

    fn paints(self) -> bool {
        matches!(self, Kind::Rectangle | Kind::Text)
    }

    fn polishes(self) -> bool {
        matches!(self, Kind::Text | Kind::Row | Kind::Column)
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
}

/// An item tree, its render tree, and the frame pipeline between them.
pub struct QuickScene {
    tree: RenderTree,
    shared: SharedRc,
    log: QuickLog,
    entries: HashMap<ObjectId, Entry>,
    by_node: HashMap<NodeId, ObjectId>,
    paint: HashMap<ObjectId, Paint>,
    root: Option<ObjectId>,
    viewport: kurbo::Rect,
    scale_factor: f32,
    frame: u64,
    snapshot: Option<Arc<FrameSnapshot>>,
    measurer: Rc<dyn TextMeasurer>,
    timers: Option<TimerHook>,
    next_timer: Option<Time>,
    pointer: PointerState,
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
    pub fn new(viewport: kurbo::Size) -> Self {
        QuickScene {
            tree: RenderTree::new(),
            shared: SharedRc::default(),
            log: QuickLog::default(),
            entries: HashMap::new(),
            by_node: HashMap::new(),
            paint: HashMap::new(),
            root: None,
            viewport: kurbo::Rect::from_origin_size((0.0, 0.0), viewport),
            scale_factor: 1.0,
            frame: 0,
            snapshot: None,
            measurer: Rc::new(FixedAdvanceMeasurer::default()),
            timers: None,
            next_timer: None,
            pointer: PointerState::default(),
        }
    }

    /// Shows `root` and everything under it, replacing any previous root.
    pub fn set_root(&mut self, engine: &Engine, root: ObjectId) -> Result<(), EngineError> {
        if let Some(old) = self.root.take() {
            self.unadopt(engine, old, true);
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
        Ok(())
    }

    pub fn root(&self) -> Option<ObjectId> {
        self.root
    }

    pub fn set_viewport(&mut self, viewport: kurbo::Rect) {
        if self.viewport != viewport {
            self.viewport = viewport;
            mark(&self.shared, |_| {});
        }
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    /// The host's text measurement. Every Text is laid out again.
    pub fn set_text_measurer(&mut self, m: Rc<dyn TextMeasurer>) {
        self.measurer = m;
        let texts: Vec<ObjectId> = self
            .entries
            .iter()
            .filter(|(_, e)| e.kind == Kind::Text)
            .map(|(o, _)| *o)
            .collect();
        let content: Vec<(ObjectId, NodeId)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.kind == Kind::Text && e.content_effect.is_some())
            .map(|(o, e)| (*o, e.node))
            .collect();
        for (_, n) in &content {
            self.tree.mark_content_dirty(*n);
        }
        mark(&self.shared, |s| s.polish.extend(texts));
    }

    /// Called (unborrowed, at most once per frame) when the scene goes from
    /// idle to needing a frame.
    pub fn set_waker(&mut self, waker: impl Fn() + 'static) {
        self.shared.borrow_mut().waker = Some(Rc::new(waker));
    }

    /// See [`TimerHook`].
    pub fn set_timer_hook(&mut self, hook: TimerHook) {
        self.timers = Some(hook);
    }

    /// Warnings from the scene and its items.
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
    }

    // ------------------------------------------------------------ frame

    /// Pipeline steps 3-9 at time `now`.
    pub fn frame(&mut self, engine: &Engine, now: Time) -> FrameOutput {
        self.frame += 1;
        // 3. Timers and animations.
        if let Some(hook) = &mut self.timers {
            self.next_timer = hook(engine, now);
        }
        // 4. Deferred deletes.
        let deleted = engine.process_deferred_deletes();
        let dead = std::mem::take(&mut self.shared.borrow_mut().dead);
        if deleted > 0 || !dead.is_empty() {
            self.sweep_dead(engine);
        }
        // 5. (Loader incubation lands with contract item 5.)
        // 6. Polish.
        self.polish(engine);
        // 7. (Layout of attached layouts: none in QtQuick.)
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
        let pending = {
            let mut s = self.shared.borrow_mut();
            let p = s.pending();
            s.dirty = p;
            p
        };
        FrameOutput {
            snapshot: snap,
            changed,
            next_wake: if pending { Some(now) } else { self.next_timer },
        }
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
                    Kind::Text => polish_text(engine, obj, &*self.measurer),
                    Kind::Item | Kind::Rectangle => Ok(()),
                };
                if let Err(e) = r {
                    self.log
                        .warn(POLISH_CATEGORY, format!("polish failed: {e}"));
                }
            }
        }
        let left = std::mem::take(&mut self.shared.borrow_mut().polish);
        if !left.is_empty() {
            self.log.warn(
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
        effects.push(engine.effect(move |e| structure_effect(e, obj, &shared))?);
        let log = self.log.clone();
        effects.push(engine.effect(move |e| inherit_effect(e, obj, &inherit, &log))?);
        let log = self.log.clone();
        effects.push(engine.effect(move |e| anchors::run(e, obj, &anchors, &log))?);
        let shared = Rc::clone(&self.shared);
        effects.push(engine.effect(move |e| geometry_effect(e, obj, &shared))?);
        if kind.polishes() {
            let shared = Rc::clone(&self.shared);
            effects.push(engine.effect(move |e| polish_effect(e, obj, kind, &shared))?);
        }
        if let Some(e) = self.entries.get_mut(&obj) {
            e.effects = effects;
        }

        let children: Vec<ObjectId> = engine
            .untrack(|| child_items(engine, obj))
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
            let mut want: Vec<ObjectId> = engine
                .untrack(|| child_items(engine, p))
                .into_iter()
                .filter(|c| engine.is_alive(*c) && is_item(engine, *c))
                .collect();
            let mut seen = HashSet::new();
            want.retain(|c| seen.insert(*c));
            for (i, c) in want.iter().enumerate() {
                match self.entries.get_mut(c) {
                    None => {
                        if let Err(e) = self.adopt(engine, *c, Some(p), pnode, i) {
                            self.log.warn(SCENE_CATEGORY, format!("adopt failed: {e}"));
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
                            self.log
                                .warn(SCENE_CATEGORY, format!("reparent failed: {e}"));
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
        let m = &*self.measurer;
        self.tree.rebuild_content(self.scale_factor, |cx, b| {
            let Some(p) = by_node.get(&cx.node).and_then(|o| paint.get(o)) else {
                return;
            };
            match p {
                Paint::Rect(r) => r.paint(cx.size, b),
                Paint::Text(t) => text::paint(m, t, cx.size, b),
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
                    match engine.effect(move |en| content_effect(en, o, kind, &shared)) {
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
                }
                _ => {}
            }
        }
        for err in failed {
            self.log
                .warn(SCENE_CATEGORY, format!("paint effect failed: {err}"));
        }
        installed
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

    /// Items under `pos`, topmost first.
    fn hits(&self, engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
        let Some(snap) = &self.snapshot else {
            return Vec::new();
        };
        hit_test(snap, pos)
            .filter_map(|n| self.by_node.get(&n).copied())
            .filter(|o| engine.is_alive(*o))
            .collect()
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
        self.pointer.grab = None;
        self.pointer.passive.clear();
        self.pointer.press_pos = ev.pos;
        self.pointer.double = false;
        let mut handled = false;
        for obj in self.hits(engine, ev.pos) {
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
                && engine.read(obj, Item::ENABLED.index).to_boolean()
                && accepted_buttons(engine, obj) & button.bits() as i32 != 0
                && self.press_mouse_area(engine, obj, ev, button)
            {
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
            _ => {}
        }
        let hits = self.hits(engine, ev.pos);
        handled |= self.update_hover(engine, ev, &hits);
        handled || !self.pointer.passive.is_empty()
    }

    fn release(&mut self, engine: &Engine, ev: &PointerEvent) -> bool {
        let mut handled = false;
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

    /// Hover for `hoverEnabled` MouseAreas. Deviation: every hover-enabled
    /// MouseArea under the pointer gets it, not just the topmost.
    fn update_hover(&mut self, engine: &Engine, ev: &PointerEvent, hits: &[ObjectId]) -> bool {
        let grabbed = match self.pointer.grab {
            Some(Grab::Mouse(m)) => Some(m),
            _ => None,
        };
        let now: Vec<ObjectId> = hits
            .iter()
            .copied()
            .filter(|o| {
                Some(*o) != grabbed
                    && is_a::<MouseArea>(engine, *o)
                    && hover_enabled(engine, *o)
                    && engine.read(*o, Item::ENABLED.index).to_boolean()
            })
            .collect();
        let old = std::mem::take(&mut self.pointer.hovered);
        for o in &old {
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
            }
            if ev.kind == PointerKind::Move {
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
    let parent = e.read(obj, Item::PARENT.index).as_object();
    if parent != e.visual_parent(obj) {
        // `parent` was assigned directly; bring the lists in step.
        let _ = e.untrack(|| set_parent_item(e, obj, parent));
    }
    let _ = child_items(e, obj);
    mark(shared, |s| {
        s.structure.insert(obj);
    });
}

fn geometry_effect(e: &Engine, obj: ObjectId, shared: &SharedRc) {
    if !e.is_alive(obj) {
        mark(shared, |s| s.dead.push(obj));
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
        Kind::Text => {
            TextInputs::read(e, obj);
            e.read(obj, Item::WIDTH.index);
        }
        Kind::Item | Kind::Rectangle => return,
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
        _ => return,
    };
    mark(shared, |s| {
        s.content.insert(obj, p);
    });
}

/// `visible` and `enabled` read false under a hidden or disabled parent
/// (Qt documents both as inherited). The item's own value sits below an
/// override layer and comes back when the parent turns on again.
fn inherit_effect(e: &Engine, obj: ObjectId, st: &RefCell<Inherit>, log: &QuickLog) {
    if !e.is_alive(obj) {
        return;
    }
    let parent = e
        .read(obj, Item::PARENT.index)
        .as_object()
        .filter(|p| e.is_alive(*p) && is_item(e, *p));
    let mut st = st.borrow_mut();
    let st = &mut *st;
    for (p, slot) in [
        (Item::VISIBLE.index, &mut st.visible),
        (Item::ENABLED.index, &mut st.enabled),
    ] {
        let parent_on = parent.is_none_or(|q| e.read(q, p).to_boolean());
        if let Err(err) = inherit_one(e, obj, p, slot, parent_on) {
            log.warn(SCENE_CATEGORY, format!("inherit failed: {err}"));
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

fn polish_text(engine: &Engine, obj: ObjectId, m: &dyn TextMeasurer) -> Result<(), EngineError> {
    let (t, width) = engine.untrack(|| {
        (
            TextInputs::read(engine, obj),
            num(engine, obj, Item::WIDTH.index),
        )
    });
    let natural = text::layout(m, &t, None);
    let wraps = t.wrap != crate::enums::wrap_mode::NO_WRAP;
    let constrained = wraps || t.elide == crate::enums::elide::RIGHT;
    let actual = if constrained {
        text::layout(m, &t, Some(width))
    } else {
        natural.clone()
    };
    // An empty Text is still one line tall.
    let implicit_height = if natural.lines.is_empty() {
        natural.line_advance
    } else if wraps {
        actual.height()
    } else {
        natural.height()
    };
    engine.batch(|| {
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_WIDTH.index,
            Value::Real(natural.width()),
        )?;
        write_computed(
            engine,
            obj,
            Item::IMPLICIT_HEIGHT.index,
            Value::Real(implicit_height),
        )?;
        write_computed(
            engine,
            obj,
            Text::CONTENT_WIDTH.index,
            Value::Real(actual.width()),
        )?;
        write_computed(
            engine,
            obj,
            Text::CONTENT_HEIGHT.index,
            Value::Real(actual.height()),
        )?;
        write_computed(
            engine,
            obj,
            Text::LINE_COUNT.index,
            Value::Int(actual.lines.len() as i32),
        )?;
        write_computed(
            engine,
            obj,
            Text::TRUNCATED.index,
            Value::Bool(actual.truncated),
        )?;
        write_computed(
            engine,
            obj,
            Item::BASELINE_OFFSET.index,
            Value::Real(actual.metrics.ascent),
        )
    })
}

fn set(engine: &Engine, obj: ObjectId, p: PropIndex, v: Value) {
    let _ = write_computed(engine, obj, p, v);
}

fn emit(engine: &Engine, obj: ObjectId, name: &str, args: &[Value]) {
    let _ = engine.emit_by_name(obj, name, args);
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
