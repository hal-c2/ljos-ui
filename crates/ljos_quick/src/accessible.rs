//! `Accessible` (QtQuick) and the scene's accessibility pass.
//!
//! `Accessible.role: Accessible.Button; Accessible.name: "Save"` attach an
//! [`AccessibleAttached`] object to an item. While an assistive technology
//! listens ([`QuickScene::set_accessibility_active`]) the scene keeps an
//! [`ljos_a11y::Tree`] of the items that have a role or a name.
//!
//! The pass is the scene's usual shape (ARCHITECTURE §6, §14): one small
//! effect per item describes it and records the result when something it
//! read changes; [`QuickScene::frame`] drains what was recorded. A rename
//! touches one node, and nothing runs while nothing changes. With no
//! assistive technology there are no effects at all.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-accessible.html.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ljos_a11y::accesskit::Role as AkRole;
use ljos_a11y::{Bounds, NodeData, NodeKey, Role, Tree};
use ljos_engine::{
    Atom, Engine, LogLevel, MetaRef, NodeId as GraphNode, ObjectId, QmlObject, QmlType, QtObject,
    Value,
};
use ljos_render::kurbo;
use ljos_runtime::Time;
use slotmap::{Key, KeyData};

use crate::input::PointerEvent;
use crate::item::{Item, compose_transform, is_a, num};
use crate::loader::shown_children;
use crate::scene::{QuickScene, SCENE_CATEGORY};
use crate::text::Text;

/// `Accessible` (QtQuick): only ever used attached, as `Accessible.name`,
/// and for its role keys, as `Accessible.Button`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Accessible",
    attached = AccessibleAttached,
    uncreatable,
    enums = accessible_enums
)]
pub struct Accessible {}

/// What `Accessible.` attaches to an item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "AccessibleAttached")]
pub struct AccessibleAttached {
    /// A `QAccessible::Role` number ([`ljos_a11y::Role`]).
    #[qml(notify)]
    pub role: i32,
    #[qml(notify)]
    pub name: Atom,
    #[qml(notify)]
    pub description: Atom,
    /// Until it is given a value this follows the role, as Qt documents:
    /// true for buttons, check boxes and the other roles a user tabs to.
    #[qml(notify)]
    pub focusable: bool,
    #[qml(notify)]
    pub ignored: bool,
}

fn accessible_enums() -> crate::enums::EnumTable {
    vec![(
        "Role",
        ljos_a11y::qt_role_keys()
            .into_iter()
            .map(|(k, v)| (k, i64::from(v)))
            .collect(),
    )]
}

fn attacher() -> MetaRef {
    MetaRef::Static(Accessible::static_meta())
}

/// Gives every new `Accessible` object its role-dependent `focusable`.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, _, att| {
        if is_a::<AccessibleAttached>(e, att) {
            // A binding, so that it follows the role and an assignment
            // replaces it. It is in place before the creator applies the
            // item's own `Accessible.focusable`.
            let r = e.bind(att, AccessibleAttached::FOCUSABLE.index, move |e| {
                let role = e.read(att, AccessibleAttached::ROLE.index).to_number() as i32;
                Value::Bool(Role::from_qt(role).is_some_and(Role::focusable_by_default))
            });
            if let Err(err) = r {
                e.log(
                    LogLevel::Warning,
                    SCENE_CATEGORY,
                    format!("Accessible.focusable has no default: {err}"),
                );
            }
        }
        true
    });
}

/// The tree node of an item. Object ids are never 0, which is the root.
fn key_of(obj: ObjectId) -> NodeKey {
    NodeKey(obj.data().as_ffi())
}

fn item_of(key: NodeKey) -> ObjectId {
    KeyData::from_ffi(key.0).into()
}

/// What effects recorded for the next [`A11yPass::sync`].
#[derive(Default)]
struct Pending {
    active: bool,
    /// The latest description of an item; `None`: it is not exposed.
    described: HashMap<ObjectId, Option<NodeData>>,
    bounds: HashMap<ObjectId, Bounds>,
    /// Items that got their `Accessible` object after their effect looked
    /// for it.
    late: Vec<ObjectId>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.described.is_empty() && self.bounds.is_empty() && self.late.is_empty()
    }
}

type PendingRc = Rc<RefCell<Pending>>;

#[derive(Default)]
struct ItemState {
    is_text: bool,
    describe: Option<GraphNode>,
    /// Only exposed items watch where they are.
    bounds: Option<GraphNode>,
}

/// The accessibility side of a [`QuickScene`].
pub(crate) struct A11yPass {
    pending: PendingRc,
    /// Asks the host for a frame.
    wake: Rc<dyn Fn()>,
    items: HashMap<ObjectId, ItemState>,
    tree: Tree,
    /// Whether the engine tells this pass about new `Accessible` objects.
    listening: bool,
    /// Items whose visual children changed.
    restructure: HashSet<ObjectId>,
    /// Items that joined the tree and are in no child list yet.
    fresh: Vec<ObjectId>,
}

impl A11yPass {
    pub(crate) fn new(wake: Rc<dyn Fn()>) -> Self {
        A11yPass {
            pending: PendingRc::default(),
            wake,
            items: HashMap::new(),
            tree: Tree::new(),
            listening: false,
            restructure: HashSet::new(),
            fresh: Vec::new(),
        }
    }

    fn active(&self) -> bool {
        self.pending.borrow().active
    }

    /// Whether [`A11yPass::sync`] has work.
    pub(crate) fn pending(&self) -> bool {
        !self.pending.borrow().is_empty()
    }

    fn set_active(&mut self, engine: &Engine, on: bool) {
        if self.active() == on {
            return;
        }
        self.pending.borrow_mut().active = on;
        if on {
            self.listen(engine);
            let items: Vec<ObjectId> = self.items.keys().copied().collect();
            for obj in items {
                self.watch(engine, obj);
            }
            // The effects ran and recorded; the next frame builds the tree.
            (self.wake)();
        } else {
            let g = engine.graph();
            for st in self.items.values_mut() {
                for n in st.describe.take().into_iter().chain(st.bounds.take()) {
                    let _ = g.dispose(n);
                }
            }
            let mut p = self.pending.borrow_mut();
            p.described.clear();
            p.bounds.clear();
            p.late.clear();
            drop(p);
            self.tree = Tree::new();
            self.restructure.clear();
            self.fresh.clear();
        }
    }

    /// An item's describing effect looks for its `Accessible` object once.
    /// One made later (`item.Accessible.name = …` from a handler) changes
    /// nothing the effect read, so the engine says so here.
    fn listen(&mut self, engine: &Engine) {
        if std::mem::replace(&mut self.listening, true) {
            return;
        }
        let pending = Rc::downgrade(&self.pending);
        let wake = Rc::clone(&self.wake);
        engine.on_attached_created(move |e, obj, att| {
            let Some(pending) = pending.upgrade() else {
                // The scene is gone.
                return false;
            };
            if !is_a::<AccessibleAttached>(e, att) {
                return true;
            }
            let active = {
                let mut p = pending.borrow_mut();
                if p.active {
                    p.late.push(obj);
                }
                p.active
            };
            if active {
                wake();
            }
            true
        });
    }

    /// The scene shows `obj` from now on.
    pub(crate) fn adopt(&mut self, engine: &Engine, obj: ObjectId, is_text: bool) {
        self.items.insert(
            obj,
            ItemState {
                is_text,
                ..ItemState::default()
            },
        );
        if self.active() {
            self.watch(engine, obj);
        }
    }

    /// The scene no longer shows `obj`.
    pub(crate) fn forget(&mut self, engine: &Engine, obj: ObjectId) {
        let Some(st) = self.items.remove(&obj) else {
            return;
        };
        let g = engine.graph();
        for n in st.describe.into_iter().chain(st.bounds) {
            let _ = g.dispose(n);
        }
        let mut p = self.pending.borrow_mut();
        p.described.remove(&obj);
        p.bounds.remove(&obj);
        drop(p);
        self.restructure.remove(&obj);
        self.tree.remove(key_of(obj));
    }

    /// The visual children of `obj` changed.
    pub(crate) fn children_changed(&mut self, obj: ObjectId) {
        if self.active() {
            self.restructure.insert(obj);
        }
    }

    fn watch(&mut self, engine: &Engine, obj: ObjectId) {
        let Some(st) = self.items.get_mut(&obj) else {
            return;
        };
        if st.describe.is_some() {
            return;
        }
        let pending = Rc::clone(&self.pending);
        let wake = Rc::clone(&self.wake);
        let is_text = st.is_text;
        let effect = engine.effect_in(obj, move |e| {
            if !e.is_alive(obj) {
                return;
            }
            let d = describe(e, obj, is_text);
            pending.borrow_mut().described.insert(obj, d);
            wake();
        });
        match effect {
            Ok(n) => st.describe = Some(n),
            Err(err) => warn(engine, format!("cannot describe an item: {err}")),
        }
    }

    fn watch_bounds(&mut self, engine: &Engine, obj: ObjectId) {
        let Some(st) = self.items.get_mut(&obj) else {
            return;
        };
        if st.bounds.is_some() {
            return;
        }
        let pending = Rc::clone(&self.pending);
        let wake = Rc::clone(&self.wake);
        let effect = engine.effect_in(obj, move |e| {
            if !e.is_alive(obj) {
                return;
            }
            let b = scene_bounds(e, obj);
            pending.borrow_mut().bounds.insert(obj, b);
            wake();
        });
        match effect {
            Ok(n) => st.bounds = Some(n),
            Err(err) => warn(engine, format!("cannot place an item: {err}")),
        }
    }

    /// Brings the tree in step with what the effects recorded. `root` is
    /// the scene's root item.
    pub(crate) fn sync(&mut self, engine: &Engine, root: Option<ObjectId>) {
        if !self.active() {
            return;
        }
        let late = std::mem::take(&mut self.pending.borrow_mut().late);
        for obj in late {
            // A new effect, so that it finds and tracks the new object.
            if let Some(n) = self.items.get_mut(&obj).and_then(|st| st.describe.take()) {
                let _ = engine.graph().dispose(n);
                self.watch(engine, obj);
            }
        }
        let described: Vec<(ObjectId, Option<NodeData>)> =
            self.pending.borrow_mut().described.drain().collect();
        for (obj, d) in described {
            self.apply(engine, obj, d);
        }
        self.place(engine, root);
        let bounds: Vec<(ObjectId, Bounds)> = self.pending.borrow_mut().bounds.drain().collect();
        for (obj, b) in bounds {
            self.tree.set_bounds(key_of(obj), b);
        }
    }

    fn apply(&mut self, engine: &Engine, obj: ObjectId, d: Option<NodeData>) {
        if !self.items.contains_key(&obj) {
            return;
        }
        let key = key_of(obj);
        match d {
            Some(mut data) => {
                if let Some(old) = self.tree.get(key) {
                    // The description does not know where the item is.
                    data.bounds = old.bounds;
                    self.tree.upsert(key, data);
                } else {
                    self.tree.upsert(key, data);
                    self.fresh.push(obj);
                    self.watch_bounds(engine, obj);
                }
            }
            None => {
                // The item's exposed descendants take its place.
                if self.tree.remove(key) {
                    if let Some(n) = self.items.get_mut(&obj).and_then(|st| st.bounds.take()) {
                        let _ = engine.graph().dispose(n);
                    }
                    self.pending.borrow_mut().bounds.remove(&obj);
                }
            }
        }
    }

    /// Recomputes the child lists that can have changed: of each new node
    /// and the node above it, and of the node at or above each item whose
    /// children changed. Each list is found by walking the items below its
    /// node down to the next exposed ones, so the work is the size of
    /// what changed, not of the scene.
    fn place(&mut self, engine: &Engine, root: Option<ObjectId>) {
        if self.restructure.is_empty() && self.fresh.is_empty() {
            return;
        }
        let mut targets: HashSet<Option<ObjectId>> = HashSet::new();
        for obj in std::mem::take(&mut self.restructure) {
            targets.insert(self.exposed_from(engine, Some(obj)));
        }
        for obj in std::mem::take(&mut self.fresh) {
            if !self.tree.contains(key_of(obj)) {
                continue;
            }
            targets.insert(Some(obj));
            let above = engine.untrack(|| engine.visual_parent(obj));
            targets.insert(self.exposed_from(engine, above));
        }
        for t in targets {
            let start = match t {
                Some(obj) => engine.untrack(|| shown_children(engine, obj)),
                None => root.into_iter().collect(),
            };
            let kids = self.exposed_below(engine, start);
            self.tree
                .set_children(t.map_or(NodeKey::ROOT, key_of), &kids);
        }
    }

    /// The exposed item at or above `from`; `None` is the scene itself.
    fn exposed_from(&self, engine: &Engine, from: Option<ObjectId>) -> Option<ObjectId> {
        let mut cur = from;
        // Bounded so a corrupt parent cycle cannot hang.
        for _ in 0..4096 {
            let o = cur.filter(|o| self.items.contains_key(o))?;
            if self.tree.contains(key_of(o)) {
                return Some(o);
            }
            cur = engine.untrack(|| engine.visual_parent(o));
        }
        None
    }

    /// The exposed items reached from `start` without passing through
    /// another exposed item, in visual-tree order.
    fn exposed_below(&self, engine: &Engine, start: Vec<ObjectId>) -> Vec<NodeKey> {
        let mut out = Vec::new();
        let mut stack = start;
        stack.reverse();
        while let Some(o) = stack.pop() {
            // Not shown by the scene (a Loader's item still being made).
            if !self.items.contains_key(&o) {
                continue;
            }
            let key = key_of(o);
            if self.tree.contains(key) {
                out.push(key);
                continue;
            }
            let mut kids = engine.untrack(|| shown_children(engine, o));
            kids.reverse();
            stack.extend(kids);
        }
        out
    }
}

fn warn(engine: &Engine, msg: String) {
    engine.log(LogLevel::Warning, SCENE_CATEGORY, msg);
}

/// What `obj` tells an assistive technology, or `None` when it is not
/// exposed. Runs inside the item's effect: every read is a dependency.
fn describe(e: &Engine, obj: ObjectId, is_text: bool) -> Option<NodeData> {
    // `visible` already reads false under a hidden ancestor.
    if !e.read(obj, Item::VISIBLE.index).to_boolean() {
        return None;
    }
    let att = e.existing_attached_object(obj, &attacher())?;
    if e.read(att, AccessibleAttached::IGNORED.index).to_boolean() {
        return None;
    }
    let number = e.read(att, AccessibleAttached::ROLE.index).to_number() as i32;
    let qt_role = Role::from_qt(number).unwrap_or_default();
    let mut name = e
        .read(att, AccessibleAttached::NAME.index)
        .to_js_string()
        .to_string();
    let role = match qt_role.accesskit() {
        Some(role) => {
            if name.is_empty() && is_text {
                // A Text that has a role says what it shows.
                name = e.read(obj, Text::TEXT.index).to_js_string().to_string();
            }
            role
        }
        // A name without a role still has to be something: text is a
        // label, anything else a named group.
        None if !name.is_empty() && is_text => AkRole::Label,
        None if !name.is_empty() => AkRole::Group,
        None => return None,
    };
    Some(NodeData {
        qt_role,
        role,
        name,
        description: e
            .read(att, AccessibleAttached::DESCRIPTION.index)
            .to_js_string()
            .to_string(),
        author_id: e
            .read(obj, QtObject::OBJECT_NAME.index)
            .to_js_string()
            .to_string(),
        bounds: Bounds::default(),
        focusable: e
            .read(att, AccessibleAttached::FOCUSABLE.index)
            .to_boolean(),
        disabled: !e.read(obj, Item::ENABLED.index).to_boolean(),
        press: qt_role.pressable(),
    })
}

/// The item's box in scene coordinates: the bounding box of its
/// rectangle under its own and its ancestors' transforms. Runs inside an
/// effect, and reads a size only where a rotation or a scale makes the
/// transform depend on it, so resizing a container does not re-place
/// everything inside.
fn scene_bounds(e: &Engine, obj: ObjectId) -> Bounds {
    let w = num(e, obj, Item::WIDTH.index).max(0.0);
    let h = num(e, obj, Item::HEIGHT.index).max(0.0);
    let mut t = kurbo::Affine::IDENTITY;
    let mut cur = Some(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let Some(o) = cur.filter(|o| e.is_alive(*o) && crate::item::is_item(e, *o)) else {
            break;
        };
        let x = num(e, o, Item::X.index);
        let y = num(e, o, Item::Y.index);
        let rotation = num(e, o, Item::ROTATION.index);
        let scale = num(e, o, Item::SCALE.index);
        let local = if rotation == 0.0 && scale == 1.0 {
            kurbo::Affine::translate((x, y))
        } else {
            compose_transform(
                x,
                y,
                num(e, o, Item::WIDTH.index),
                num(e, o, Item::HEIGHT.index),
                rotation,
                scale,
                num(e, o, Item::TRANSFORM_ORIGIN.index) as i32,
            )
        };
        t = local * t;
        cur = e.visual_parent(o);
    }
    let r = t.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, w, h));
    Bounds::new(r.x0, r.y0, r.x1, r.y1)
}

impl QuickScene {
    /// Turns the accessibility tree on or off. A host turns it on while
    /// an assistive technology (or a test) listens; off, the scene does no
    /// accessibility work at all. Turning it on asks for a frame, which
    /// builds the tree.
    pub fn set_accessibility_active(&mut self, engine: &Engine, active: bool) {
        self.a11y.set_active(engine, active);
    }

    pub fn accessibility_active(&self) -> bool {
        self.a11y.active()
    }

    /// The exposed items, as of the last [`QuickScene::frame`]. Empty
    /// while accessibility is off.
    pub fn accessibility_tree(&self) -> &Tree {
        &self.a11y.tree
    }

    /// The tree, for a host that takes its changes
    /// ([`Tree::take_changes`], [`Tree::take_update`]).
    pub fn accessibility_tree_mut(&mut self) -> &mut Tree {
        &mut self.a11y.tree
    }

    /// The item behind a node of the tree.
    pub fn accessible_item(&self, key: NodeKey) -> Option<ObjectId> {
        self.a11y.tree.contains(key).then(|| item_of(key))
    }

    /// The node of an item, if it is exposed.
    pub fn accessible_node(&self, item: ObjectId) -> Option<NodeKey> {
        let key = key_of(item);
        self.a11y.tree.contains(key).then_some(key)
    }

    /// The press action of an assistive technology: a click in the middle
    /// of the node, so whatever a pointer click there does happens (the
    /// item's `MouseArea` or `TapHandler`). Returns whether something
    /// took it; a disabled node, or one whose role is not pressed, takes
    /// nothing.
    pub fn accessible_press(&mut self, engine: &Engine, key: NodeKey, now: Time) -> bool {
        let Some(data) = self.a11y.tree.get(key) else {
            return false;
        };
        if data.disabled || !data.press {
            return false;
        }
        let (x, y) = data.bounds.center();
        let pressed = self.pointer_event(engine, &PointerEvent::press(x, y, now));
        let released = self.pointer_event(engine, &PointerEvent::release(x, y, now));
        // No pointer is there: without this a hover-enabled area would
        // stay hovered.
        self.pointer_event(engine, &PointerEvent::leave(now));
        pressed || released
    }
}
