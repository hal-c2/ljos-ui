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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use ljos_a11y::accesskit::{Role as AkRole, Toggled};
use ljos_a11y::{Bounds, Handled, NodeData, NodeKey, Politeness, Range, Role, RoleFlags, Tree};
use ljos_engine::{
    Atom, Cx, Engine, LogLevel, MetaRef, NodeId as GraphNode, ObjectId, PropIndex, QmlObject,
    QmlType, QtObject, Value, qml_impl,
};
use ljos_render::kurbo;
use ljos_runtime::Time;
use slotmap::{Key, KeyData};

use crate::input::PointerEvent;
use crate::item::{Item, compose_transform, is_a, num};
use crate::loader::shown_children;
use crate::repeater::Repeater;
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
#[qml(name = "AccessibleAttached", enums = accessible_enums)]
pub struct AccessibleAttached {
    /// A `QAccessible::Role` number ([`ljos_a11y::Role`]).
    #[qml(notify, enumeration = "Role")]
    pub role: i32,
    #[qml(notify)]
    pub name: Atom,
    #[qml(notify)]
    pub description: Atom,
    /// Until it is given a value this follows the role, as Qt does: true
    /// once the item has had the role of a button, a check box or another
    /// of the roles a user moves the focus to.
    #[qml(notify)]
    pub focusable: bool,
    /// What the author assigned and nothing more: Qt tells an assistive
    /// technology about an item's active focus without this ever reading
    /// true. Set, it marks an item that draws its own focus (a cell of a
    /// hand-made grid) as the one a screen reader should read.
    #[qml(notify)]
    pub focused: bool,
    #[qml(notify)]
    pub ignored: bool,
    /// Not a Qt property and not writable from QML (the compiler knows
    /// Qt's `Accessible` only): a control that shows something modal says
    /// so here, and the node tells the assistive technology that what is
    /// outside it cannot be reached.
    #[qml(notify)]
    pub modal: bool,
    /// What a test (or a script of an assistive technology) finds the
    /// item by; without it the `objectName` is that.
    #[qml(notify)]
    pub id: Atom,
    /// Until it is given a value this follows the role, as `focusable`
    /// does: true once the item has had the role of a check box, a radio
    /// button or a switch.
    #[qml(notify)]
    pub checkable: bool,
    #[qml(notify)]
    pub checked: bool,
    /// A tristate check box that is neither checked nor unchecked.
    #[qml(notify)]
    pub check_state_mixed: bool,
    #[qml(notify)]
    pub default_button: bool,
    /// Follows the role until it is given a value: true once the item has
    /// been editable text.
    #[qml(notify)]
    pub editable: bool,
    #[qml(notify)]
    pub multi_line: bool,
    #[qml(notify)]
    pub password_edit: bool,
    #[qml(notify)]
    pub pressed: bool,
    /// Follows the role until it is given a value: true once the item has
    /// been static text.
    #[qml(notify)]
    pub read_only: bool,
    #[qml(notify)]
    pub search_edit: bool,
    #[qml(notify)]
    pub selectable: bool,
    #[qml(notify)]
    pub selectable_text: bool,
    #[qml(notify)]
    pub selected: bool,
    /// Documented for Qt 6.12: the item shows or hides something (the
    /// list of a combo box, the children of a tree row).
    #[qml(notify)]
    pub expandable: bool,
    #[qml(notify)]
    pub expanded: bool,
    /// The item whose text names this one. Naming it here names this
    /// item as that item's `labelFor`, unless it says otherwise itself.
    #[qml(notify)]
    pub labelled_by: Option<ObjectId>,
    /// The item this one is the label of; the other way of saying
    /// `labelledBy`.
    #[qml(notify)]
    pub label_for: Option<ObjectId>,
}

#[qml_impl]
impl AccessibleAttached {
    // What an assistive technology asks of the item. A handler does it:
    // the request is the item's once anything is connected (see
    // [`QuickScene::accessible_press`], [`QuickScene::accessible_action`]).
    #[qml(signal)]
    fn press_action();
    #[qml(signal)]
    fn toggle_action();
    #[qml(signal)]
    fn increase_action();
    #[qml(signal)]
    fn decrease_action();
    #[qml(signal)]
    fn scroll_up_action();
    #[qml(signal)]
    fn scroll_down_action();
    #[qml(signal)]
    fn scroll_left_action();
    #[qml(signal)]
    fn scroll_right_action();
    #[qml(signal)]
    fn previous_page_action();
    #[qml(signal)]
    fn next_page_action();
    /// Documented for Qt 6.12.
    #[qml(signal)]
    fn show_on_screen_action();

    /// `Accessible.announce(message, politeness)`: an assistive technology
    /// that listens says the message without moving. One that does not
    /// listen hears nothing later either: an announcement is about now.
    #[qml(invokable)]
    fn announce(cx: &Cx, message: Value, politeness: Value) {
        let text = message.to_js_string().to_string();
        if text.is_empty() {
            return;
        }
        let politeness = politeness
            .as_number()
            .map_or(Politeness::Polite, |n| Politeness::from_qt(n as i32));
        let Some(item) = cx.engine.owner(cx.this) else {
            return;
        };
        cx.engine
            .extension::<Announcers>()
            .say(item, &text, politeness);
    }

    /// `Accessible.stripHtml(html)`: the text a screen reader should say
    /// for rich text.
    #[qml(invokable)]
    fn strip_html(html: Value) -> Atom {
        Atom::from(strip_html(html.to_js_string().as_str()).as_str())
    }

    /// `Accessible.setIgnored(ignored)`: the documented other way of
    /// writing `Accessible.ignored`.
    #[qml(invokable)]
    fn set_ignored(cx: &Cx, ignored: Value) {
        let r = cx.engine.write(
            cx.this,
            AccessibleAttached::IGNORED.index,
            Value::Bool(ignored.to_boolean()),
            ljos_engine::WriteKind::Imperative,
        );
        if let Err(err) = r {
            warn(cx.engine, format!("Accessible.setIgnored: {err}"));
        }
    }
}

/// The scenes that hear `Accessible.announce`.
#[derive(Default)]
struct Announcers {
    passes: RefCell<Vec<Hearer>>,
}

struct Hearer {
    pending: Weak<RefCell<Pending>>,
    wake: Rc<dyn Fn()>,
}

impl Announcers {
    fn say(&self, item: ObjectId, text: &str, politeness: Politeness) {
        let mut woken = Vec::new();
        self.passes.borrow_mut().retain(|h| {
            // The scene is gone.
            let Some(pending) = h.pending.upgrade() else {
                return false;
            };
            let mut p = pending.borrow_mut();
            if p.active {
                p.announced.push((item, text.to_string(), politeness));
                woken.push(Rc::clone(&h.wake));
            }
            true
        });
        // Called with nothing borrowed: a host's wake may do anything.
        for wake in woken {
            wake();
        }
    }
}

/// The text of `html` without its markup: tags go, a line break and the
/// ends of a paragraph become new lines, and the character references a
/// label is written with are read. A `<` that never closes takes the rest
/// with it, as it does in Qt.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(['<', '&']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if tail.starts_with('<') {
            let Some(end) = tail.find('>') else {
                return out;
            };
            let tag = tail[1..end].trim().trim_end_matches('/').trim_end();
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(name.as_str(), "br" | "p" | "div" | "li" | "tr")
                || (name.len() == 2 && name.starts_with('h') && name.as_bytes()[1].is_ascii_digit())
            {
                out.push('\n');
            }
            rest = &tail[end + 1..];
        } else {
            match character_reference(tail) {
                Some((c, len)) => {
                    out.push(c);
                    rest = &tail[len..];
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        }
    }
    out.push_str(rest);
    out
}

/// The character `&name;` or `&#number;` at the start of `s` stands for,
/// and how many bytes the reference takes.
fn character_reference(s: &str) -> Option<(char, usize)> {
    let end = s.find(';').filter(|end| *end <= 10)?;
    let body = &s[1..end];
    let c = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let digits = body.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((c, end + 1))
}

fn accessible_enums() -> crate::enums::EnumTable {
    vec![
        (
            "Role",
            ljos_a11y::qt_role_keys()
                .into_iter()
                .map(|(k, v)| (k, i64::from(v)))
                .collect(),
        ),
        (
            "AnnouncementPoliteness",
            vec![
                ("Polite", i64::from(Politeness::Polite.qt_value())),
                ("Assertive", i64::from(Politeness::Assertive.qt_value())),
            ],
        ),
    ]
}

fn attacher() -> MetaRef {
    MetaRef::Static(Accessible::static_meta())
}

/// What an assistive technology asks of a node besides a press and the
/// focus. Named as the `Accessible` signals of Qt are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccessibleAction {
    Increase,
    Decrease,
    /// Take this value, in the units of the node's [`ljos_a11y::Range`].
    SetValue(f64),
    ScrollUp {
        page: bool,
    },
    ScrollDown {
        page: bool,
    },
    ScrollLeft {
        page: bool,
    },
    ScrollRight {
        page: bool,
    },
    /// Come into view (`Accessible.onShowOnScreenAction`).
    ShowOnScreen,
    /// Show what the item hides: the list of a combo box.
    Expand,
    /// Hide it again.
    Collapse,
}

impl AccessibleAction {
    /// The `Accessible` signal that tells an item of the request, for the
    /// requests that have one. A page up or to the left is the previous
    /// page, a page down or to the right the next: Qt has no signal for a
    /// page in each direction.
    fn signal(self) -> Option<&'static str> {
        use AccessibleAction as A;
        Some(match self {
            A::Increase => "increaseAction",
            A::Decrease => "decreaseAction",
            A::ScrollUp { page: false } => "scrollUpAction",
            A::ScrollDown { page: false } => "scrollDownAction",
            A::ScrollLeft { page: false } => "scrollLeftAction",
            A::ScrollRight { page: false } => "scrollRightAction",
            A::ScrollUp { page: true } | A::ScrollLeft { page: true } => "previousPageAction",
            A::ScrollDown { page: true } | A::ScrollRight { page: true } => "nextPageAction",
            A::ShowOnScreen => "showOnScreenAction",
            A::SetValue(_) | A::Expand | A::Collapse => return None,
        })
    }
}

/// Whether the item's author handles `signal` of its `Accessible` object
/// (untracked: a connection is not something an effect can follow).
fn handles(e: &Engine, att: ObjectId, signal: &str) -> bool {
    e.signal_index(att, signal)
        .is_ok_and(|s| e.signal_connected(att, s))
}

/// What an item type says of itself to an assistive technology beyond its
/// role and name, and what it does when asked.
#[derive(Clone, Copy, Debug)]
pub struct AccessibleItem {
    /// Adds to the node of an exposed item (its value, how far it is
    /// scrolled). It runs inside the effect that describes the item, so
    /// the node follows whatever it reads.
    pub describe: fn(&Engine, ObjectId, &mut NodeData),
    /// Returns whether the item took the action.
    pub act: fn(&Engine, ObjectId, AccessibleAction) -> bool,
}

#[derive(Default)]
struct AccessibleItems {
    types: RefCell<Vec<(MetaRef, AccessibleItem)>>,
}

/// Makes the accessibility pass ask items of type `meta` (and of the
/// types derived from it) through `hooks`. A later registration wins.
pub fn register_accessible_item(engine: &Engine, meta: MetaRef, hooks: AccessibleItem) {
    engine
        .extension::<AccessibleItems>()
        .types
        .borrow_mut()
        .push((meta, hooks));
}

fn accessible_item(engine: &Engine, obj: ObjectId) -> Option<AccessibleItem> {
    let reg = engine.extension::<AccessibleItems>();
    let types = reg.types.borrow();
    if types.is_empty() {
        return None;
    }
    let meta = engine.meta_of(obj)?;
    types
        .iter()
        .rev()
        .find(|(m, _)| meta.inherits(m))
        .map(|(_, h)| *h)
}

/// A part of an item that is a node of its own inside the item's node: a
/// link in a text. An assistive technology can go to it and press it; a
/// press is a click at [`AccessiblePart::at`], so whatever a pointer click
/// there does happens.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessiblePart {
    pub role: AkRole,
    pub name: String,
    /// Where it is, in the item's coordinates.
    pub rect: kurbo::Rect,
    /// A point of the item that is on the part (the middle of the box
    /// around a link of two lines need not be).
    pub at: kurbo::Point,
}

/// Lists the parts of an item. Called in an effect: what it reads is
/// what makes it run again.
pub type AccessibleParts = fn(&Engine, ObjectId) -> Vec<AccessiblePart>;

#[derive(Default)]
struct PartTypes {
    types: RefCell<Vec<(MetaRef, AccessibleParts)>>,
}

/// Makes the accessibility pass list the parts of exposed items of type
/// `meta` (and of the types derived from it) through `parts`. A later
/// registration wins.
pub fn register_accessible_parts(engine: &Engine, meta: MetaRef, parts: AccessibleParts) {
    engine
        .extension::<PartTypes>()
        .types
        .borrow_mut()
        .push((meta, parts));
}

fn accessible_parts(engine: &Engine, obj: ObjectId) -> Option<AccessibleParts> {
    let reg = engine.extension::<PartTypes>();
    let types = reg.types.borrow();
    if types.is_empty() {
        return None;
    }
    let meta = engine.meta_of(obj)?;
    types
        .iter()
        .rev()
        .find(|(m, _)| meta.inherits(m))
        .map(|(_, h)| *h)
}

/// The keys of the nodes of parts are counted up from here. An item's key
/// has these bits only after its slot was reused three thousand million
/// times; the tree's own nodes (text runs, announcements) have one of the
/// two bits and never both.
const FIRST_PART_KEY: u64 = 3 << 62;

/// A part as the tree is told it: in the scene's coordinates.
struct PlacedPart {
    role: AkRole,
    name: String,
    bounds: Bounds,
    at: (f64, f64),
}

/// Gives every new `Accessible` object its role-dependent `focusable`.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, _, att| {
        if is_a::<AccessibleAttached>(e, att) {
            // A binding, so that it follows the role and an assignment
            // replaces it. It is in place before the creator applies the
            // item's own `Accessible.focusable`.
            //
            // Qt turns the default on with a focusable role and never off
            // again: a button that becomes a slider stays focusable.
            let was = Cell::new(false);
            let r = e.bind(att, AccessibleAttached::FOCUSABLE.index, move |e| {
                let role = e.read(att, AccessibleAttached::ROLE.index).to_number() as i32;
                if Role::from_qt(role).is_some_and(Role::focusable_by_default) {
                    was.set(true);
                }
                Value::Bool(was.get())
            });
            if let Err(err) = r {
                e.log(
                    LogLevel::Warning,
                    SCENE_CATEGORY,
                    format!("Accessible.focusable has no default: {err}"),
                );
            }
            if let Err(err) = follow_role(e, att).and_then(|()| mirror_labels(e, att)) {
                warn(e, format!("Accessible has no defaults: {err}"));
            }
        }
        true
    });
}

/// A property and the roles that turn its default on.
type RoleDefault = (fn() -> PropIndex, fn(Role) -> bool);

/// The properties whose default a role turns on, with the roles that do.
/// Found by asking Qt 6.11 for the property of an item that has the role
/// and was told nothing else (`features/a11y/properties.feature`).
const ROLE_DEFAULTS: [RoleDefault; 3] = [
    (
        || AccessibleAttached::CHECKABLE.index,
        Role::checkable_by_default,
    ),
    (
        || AccessibleAttached::EDITABLE.index,
        Role::editable_by_default,
    ),
    (
        || AccessibleAttached::READ_ONLY.index,
        Role::read_only_by_default,
    ),
];

/// Turns the role's defaults on when the role changes. Like `focusable`
/// they stay on under another role. The values are derived, so the
/// author's assignment, and a control's own binding, still replace them.
fn follow_role(e: &Engine, att: ObjectId) -> Result<(), ljos_engine::EngineError> {
    e.watch(att, AccessibleAttached::ROLE.index, move |e, v| {
        let Some(role) = Role::from_qt(v.to_number() as i32) else {
            return;
        };
        for (prop, on) in ROLE_DEFAULTS {
            let prop = prop();
            if on(role) && crate::item::is_unset(e, att, prop) {
                let _ = e.write_derived(att, prop, Value::Bool(true));
            }
        }
    })?;
    Ok(())
}

/// Naming a label names what it labels, from either side: `labelFor` on
/// the label reads as `labelledBy` on the other item and the reverse. An
/// item that says something of its own keeps it.
fn mirror_labels(e: &Engine, att: ObjectId) -> Result<(), ljos_engine::EngineError> {
    let (by, of) = (
        AccessibleAttached::LABELLED_BY.index,
        AccessibleAttached::LABEL_FOR.index,
    );
    for (prop, back) in [(by, of), (of, by)] {
        e.watch(att, prop, move |e, v| {
            let Some(me) = e.owner(att) else {
                return;
            };
            let Some(other) = v.as_object().filter(|o| e.is_alive(*o) && *o != me) else {
                return;
            };
            let Ok(theirs) = e.attached_object(other, &attacher()) else {
                return;
            };
            if e.read(theirs, back).as_object().is_none() {
                let _ = e.write_derived(theirs, back, Value::Object(me));
            }
        })?;
    }
    Ok(())
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
    /// What items announced since the last sync, in order.
    announced: Vec<(ObjectId, String, Politeness)>,
    /// The latest parts of an item ([`register_accessible_parts`]).
    parts: HashMap<ObjectId, Vec<PlacedPart>>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.described.is_empty()
            && self.bounds.is_empty()
            && self.late.is_empty()
            && self.announced.is_empty()
            && self.parts.is_empty()
    }
}

type PendingRc = Rc<RefCell<Pending>>;

#[derive(Default)]
struct ItemState {
    is_text: bool,
    describe: Option<GraphNode>,
    /// Only exposed items watch where they are.
    bounds: Option<GraphNode>,
    /// Only exposed items of a type with parts list them.
    parts: Option<GraphNode>,
    /// The nodes of the item's parts, in their order.
    part_keys: Vec<NodeKey>,
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
    /// The layer above the root item: a second child of the scene.
    layer: Option<ObjectId>,
    /// The scene's own child list is to be made again.
    top_dirty: bool,
    /// The item behind the node of a part, and the point of the scene a
    /// press on the part lands on.
    part_owner: HashMap<NodeKey, (ObjectId, (f64, f64))>,
    /// How many part nodes were ever made: their keys are never reused.
    next_part: u64,
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
            layer: None,
            top_dirty: false,
            part_owner: HashMap::new(),
            next_part: 0,
        }
    }

    /// The scene shows `layer` above its root item (or no longer does).
    pub(crate) fn set_layer(&mut self, layer: Option<ObjectId>) {
        self.layer = layer;
        self.top_dirty = true;
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
                if let Some(n) = st.parts.take() {
                    let _ = g.dispose(n);
                }
                st.part_keys.clear();
            }
            self.part_owner.clear();
            let mut p = self.pending.borrow_mut();
            p.described.clear();
            p.bounds.clear();
            p.late.clear();
            p.announced.clear();
            p.parts.clear();
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
        engine
            .extension::<Announcers>()
            .passes
            .borrow_mut()
            .push(Hearer {
                pending: Rc::downgrade(&self.pending),
                wake: Rc::clone(&self.wake),
            });
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
        self.drop_parts(engine, obj);
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

    /// Lists the parts of an exposed item from now on, if its type has
    /// any.
    fn watch_parts(&mut self, engine: &Engine, obj: ObjectId) {
        let Some(list) = accessible_parts(engine, obj) else {
            return;
        };
        let Some(st) = self.items.get_mut(&obj) else {
            return;
        };
        if st.parts.is_some() {
            return;
        }
        let pending = Rc::clone(&self.pending);
        let wake = Rc::clone(&self.wake);
        let effect = engine.effect_in(obj, move |e| {
            if !e.is_alive(obj) {
                return;
            }
            let parts = list(e, obj);
            let placed = if parts.is_empty() {
                Vec::new()
            } else {
                let t = crate::item::scene_transform(e, obj);
                parts
                    .into_iter()
                    .map(|p| {
                        let r = t.transform_rect_bbox(p.rect);
                        let at = t * p.at;
                        PlacedPart {
                            role: p.role,
                            name: p.name,
                            bounds: Bounds::new(r.x0, r.y0, r.x1, r.y1),
                            at: (at.x, at.y),
                        }
                    })
                    .collect()
            };
            pending.borrow_mut().parts.insert(obj, placed);
            wake();
        });
        match effect {
            Ok(n) => st.parts = Some(n),
            Err(err) => warn(engine, format!("cannot list the parts of an item: {err}")),
        }
    }

    /// The item is no longer exposed: neither are its parts.
    fn drop_parts(&mut self, engine: &Engine, obj: ObjectId) {
        let Some(st) = self.items.get_mut(&obj) else {
            return;
        };
        if let Some(n) = st.parts.take() {
            let _ = engine.graph().dispose(n);
        }
        for key in std::mem::take(&mut st.part_keys) {
            self.tree.remove(key);
            self.part_owner.remove(&key);
        }
        self.pending.borrow_mut().parts.remove(&obj);
    }

    /// Makes the nodes of the parts of `obj` say `list`. A part keeps its
    /// node while the item has that many parts, so a link that only moved
    /// or was renamed is the same node to a screen reader.
    fn apply_parts(&mut self, engine: &Engine, obj: ObjectId, list: Vec<PlacedPart>) {
        let owner = key_of(obj);
        if !self.tree.contains(owner) {
            return;
        }
        let Some(st) = self.items.get_mut(&obj) else {
            return;
        };
        let mut keys = std::mem::take(&mut st.part_keys);
        let before = keys.len();
        for key in keys.drain(list.len().min(before)..) {
            self.tree.remove(key);
            self.part_owner.remove(&key);
        }
        while keys.len() < list.len() {
            keys.push(NodeKey(FIRST_PART_KEY | self.next_part));
            self.next_part += 1;
        }
        for (key, part) in keys.iter().zip(list) {
            let mut data = NodeData::new(part.role);
            data.name = part.name;
            data.press = true;
            data.bounds = part.bounds;
            self.tree.upsert(*key, data);
            self.part_owner.insert(*key, (obj, part.at));
        }
        let changed = keys.len() != before;
        if let Some(st) = self.items.get_mut(&obj) {
            st.part_keys = keys;
        }
        if changed {
            let start = engine.untrack(|| shown_children(engine, obj));
            let mut kids = self.exposed_below(engine, start);
            if let Some(st) = self.items.get(&obj) {
                kids.extend(&st.part_keys);
            }
            self.tree.set_children(owner, &kids);
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
        let parts: Vec<(ObjectId, Vec<PlacedPart>)> =
            self.pending.borrow_mut().parts.drain().collect();
        for (obj, list) in parts {
            self.apply_parts(engine, obj, list);
        }
        let bounds: Vec<(ObjectId, Bounds)> = self.pending.borrow_mut().bounds.drain().collect();
        for (obj, b) in bounds {
            self.tree.set_bounds(key_of(obj), b);
        }
        let announced = std::mem::take(&mut self.pending.borrow_mut().announced);
        for (item, text, politeness) in announced {
            // Every scene of the engine hears of it; the one that shows
            // the item says it.
            if self.items.contains_key(&item) {
                self.tree.announce(&text, politeness);
            }
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
                    self.watch_parts(engine, obj);
                }
            }
            None => {
                self.drop_parts(engine, obj);
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
        let top_dirty = std::mem::take(&mut self.top_dirty);
        if self.restructure.is_empty() && self.fresh.is_empty() && !top_dirty {
            return;
        }
        let mut targets: HashSet<Option<ObjectId>> = HashSet::new();
        if top_dirty {
            targets.insert(None);
        }
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
                None => root.into_iter().chain(self.layer).collect(),
            };
            let mut kids = self.exposed_below(engine, start);
            // The item's own parts come after the items inside it.
            if let Some(st) = t.and_then(|obj| self.items.get(&obj)) {
                kids.extend(&st.part_keys);
            }
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
    let flag = |p: PropIndex| e.read(att, p).to_boolean();
    let mut name = e
        .read(att, AccessibleAttached::NAME.index)
        .to_js_string()
        .to_string();
    // What a control's own state adds to its role (a checkable menu item)
    // is the control's to say in its hook; here only what the author
    // wrote refines the role.
    let refined = qt_role.refined(RoleFlags {
        default_button: flag(AccessibleAttached::DEFAULT_BUTTON.index),
        editable: flag(AccessibleAttached::EDITABLE.index),
        multi_line: flag(AccessibleAttached::MULTI_LINE.index),
        password_edit: flag(AccessibleAttached::PASSWORD_EDIT.index),
        search_edit: flag(AccessibleAttached::SEARCH_EDIT.index),
        selectable: flag(AccessibleAttached::SELECTABLE.index),
        ..RoleFlags::default()
    });
    let role = match refined {
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
    let id = e.read(att, AccessibleAttached::ID.index).to_js_string();
    let author_id = if id.is_empty() {
        e.read(obj, QtObject::OBJECT_NAME.index).to_js_string()
    } else {
        id
    };
    let checked = flag(AccessibleAttached::CHECKED.index);
    let mixed = flag(AccessibleAttached::CHECK_STATE_MIXED.index);
    // Saying that it is checked says that it can be.
    let toggled =
        (flag(AccessibleAttached::CHECKABLE.index) || checked || mixed).then_some(if mixed {
            Toggled::Mixed
        } else if checked {
            Toggled::True
        } else {
            Toggled::False
        });
    let expanded = flag(AccessibleAttached::EXPANDED.index);
    let selected = flag(AccessibleAttached::SELECTED.index);
    let labelled_by = e
        .read(att, AccessibleAttached::LABELLED_BY.index)
        .as_object()
        .filter(|o| e.is_alive(*o))
        .map(key_of);
    let handles = |signal| handles(e, att, signal);
    let mut data = NodeData {
        qt_role,
        role,
        name,
        description: e
            .read(att, AccessibleAttached::DESCRIPTION.index)
            .to_js_string()
            .to_string(),
        author_id: author_id.to_string(),
        bounds: Bounds::default(),
        focusable: flag(AccessibleAttached::FOCUSABLE.index),
        // Every item on the way to the focus item reads `activeFocus`, so
        // the tree can name the nearest exposed one when the focus item
        // itself is not exposed.
        focused: e.read(obj, Item::ACTIVE_FOCUS.index).to_boolean()
            || flag(AccessibleAttached::FOCUSED.index),
        disabled: !e.read(obj, Item::ENABLED.index).to_boolean(),
        // An item whose author handles the press is pressed whatever its
        // role is.
        press: qt_role.pressable() || handles("pressAction") || handles("toggleAction"),
        modal: flag(AccessibleAttached::MODAL.index),
        toggled,
        pressed: flag(AccessibleAttached::PRESSED.index),
        expanded: (flag(AccessibleAttached::EXPANDABLE.index) || expanded).then_some(expanded),
        selected: (flag(AccessibleAttached::SELECTABLE.index) || selected).then_some(selected),
        read_only: flag(AccessibleAttached::READ_ONLY.index),
        labelled_by: labelled_by.into_iter().collect(),
        role_description: qt_role.role_description().unwrap_or_default().to_string(),
        handled: Handled {
            increase: handles("increaseAction"),
            decrease: handles("decreaseAction"),
            scroll_up: handles("scrollUpAction") || handles("previousPageAction"),
            scroll_down: handles("scrollDownAction") || handles("nextPageAction"),
            scroll_left: handles("scrollLeftAction") || handles("previousPageAction"),
            scroll_right: handles("scrollRightAction") || handles("nextPageAction"),
            show_on_screen: handles("showOnScreenAction"),
        },
        ..NodeData::new(role)
    };
    if let Some(hooks) = accessible_item(e, obj) {
        (hooks.describe)(e, obj, &mut data);
    }
    if data.range.is_none() && ranged(qt_role) {
        data.range = own_range(e, obj);
    }
    if data.position_in_set.is_none()
        && qt_role.in_set()
        && let Some((place, of)) =
            repeated_place(e, obj).or_else(|| crate::views::place_in_view(e, obj))
    {
        data.position_in_set = Some(place);
        data.size_of_set = Some(of);
    }
    Some(data)
}

/// The place of an item a `Repeater` made among the items it made, and
/// how many the model has: `Accessible` has no property for it, and the
/// model is what knows. Reads the parent's children and the Repeater's
/// `count`, so the node follows rows that come, go and move.
fn repeated_place(e: &Engine, obj: ObjectId) -> Option<(usize, usize)> {
    // A Repeater owns what it makes and shows it in its own parent.
    let repeater = e.owner(obj).filter(|o| is_a::<Repeater>(e, *o))?;
    let parent = e.visual_parent(obj)?;
    let place = crate::tree::child_items(e, parent)
        .into_iter()
        .filter(|c| e.owner(*c) == Some(repeater))
        .position(|c| c == obj)?
        + 1;
    let of = e.read(repeater, Repeater::COUNT.index).to_number() as usize;
    // While rows are being made the count may be behind the items.
    Some((place, of.max(place)))
}

/// The roles of an item that has a value between two ends. The
/// documentation of `Accessible.role` names the properties such an item
/// provides for a slider, a spin box, a dial and a scroll bar; a progress
/// bar and an indicator say where they are the same way.
fn ranged(role: Role) -> bool {
    matches!(
        role,
        Role::Slider
            | Role::SpinBox
            | Role::Dial
            | Role::ScrollBar
            | Role::ProgressBar
            | Role::Indicator
    )
}

/// The range an item says in properties of its own: `value`,
/// `minimumValue` and `maximumValue`, and `stepSize` when it can be
/// stepped. An item without the first three has no range to tell.
fn own_range(e: &Engine, obj: ObjectId) -> Option<Range> {
    let number = |name: &str| {
        let p = e.untrack(|| e.property_index(obj, name)).ok()?;
        let n = e.read(obj, p).to_number();
        n.is_finite().then_some(n)
    };
    Some(Range {
        value: number("value")?,
        min: number("minimumValue")?,
        max: number("maximumValue")?,
        step: number("stepSize").unwrap_or(0.0),
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
        // Whoever makes the scene keep the tree is the screen reader as far
        // as the application can tell (`Qt.styleHints.accessibility`).
        crate::system_prefs::update_system_prefs(engine, |p| p.screen_reader_active = active);
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
        // A part is a node of the item it is a part of.
        if let Some((owner, _)) = self.a11y.part_owner.get(&key) {
            return Some(*owner);
        }
        // An announcement is a node of the tree with no item behind it.
        (!key.is_announcement() && self.a11y.tree.contains(key)).then(|| item_of(key))
    }

    /// The node of an item, if it is exposed.
    pub fn accessible_node(&self, item: ObjectId) -> Option<NodeKey> {
        let key = key_of(item);
        self.a11y.tree.contains(key).then_some(key)
    }

    /// The press action of an assistive technology. An item whose author
    /// handles it (`Accessible.onPressAction`, or `onToggleAction` on an
    /// item that can be checked) is told, and does the rest itself. Any
    /// other item gets a click in the middle of the node, so whatever a
    /// pointer click there does happens (the item's `MouseArea` or
    /// `TapHandler`). Returns whether something took it; a disabled node,
    /// or one whose role is not pressed, takes nothing.
    pub fn accessible_press(&mut self, engine: &Engine, key: NodeKey, now: Time) -> bool {
        let Some(data) = self.a11y.tree.get(key) else {
            return false;
        };
        if data.disabled || !data.press || key.is_announcement() {
            return false;
        }
        let (mut x, mut y) = data.bounds.center();
        // A part has no `Accessible` object to ask: the click is all.
        let part = self.a11y.part_owner.get(&key).copied();
        if let Some((_, at)) = part {
            (x, y) = at;
        }
        let item = item_of(key);
        if part.is_none()
            && let Some(att) = engine.existing_attached_object(item, &attacher())
        {
            let checkable = engine.untrack(|| {
                engine
                    .read(att, AccessibleAttached::CHECKABLE.index)
                    .to_boolean()
            });
            let told = [("toggleAction", checkable), ("pressAction", true)]
                .into_iter()
                .find(|(signal, on)| *on && handles(engine, att, signal));
            if let Some((signal, _)) = told {
                return engine.emit_by_name(att, signal, &[]).is_ok();
            }
        }
        let pressed = self.pointer_event(engine, &PointerEvent::press(x, y, now));
        let released = self.pointer_event(engine, &PointerEvent::release(x, y, now));
        // No pointer is there: without this a hover-enabled area would
        // stay hovered.
        self.pointer_event(engine, &PointerEvent::leave(now));
        pressed || released
    }

    /// Any other action of an assistive technology (a step of a scroll
    /// bar, a scroll of a view). An item whose author handles the request
    /// (`Accessible.onIncreaseAction` and the like) is told; otherwise the
    /// item's type does it ([`register_accessible_item`]). Returns whether
    /// something took it; a disabled node takes nothing.
    pub fn accessible_action(
        &mut self,
        engine: &Engine,
        key: NodeKey,
        action: AccessibleAction,
    ) -> bool {
        if key.is_announcement() || self.a11y.tree.get(key).is_none_or(|data| data.disabled) {
            return false;
        }
        // A part can be pressed and nothing else.
        if self.a11y.part_owner.contains_key(&key) {
            return false;
        }
        let item = item_of(key);
        if !engine.is_alive(item) {
            return false;
        }
        if let Some(signal) = action.signal()
            && let Some(att) = engine.existing_attached_object(item, &attacher())
            && handles(engine, att, signal)
        {
            return engine.emit_by_name(att, signal, &[]).is_ok();
        }
        accessible_item(engine, item).is_some_and(|hooks| (hooks.act)(engine, item, action))
    }

    /// The node an assistive technology should read as having the
    /// keyboard, as of the last [`QuickScene::frame`]: the focus item, or
    /// the nearest exposed focus scope around it. `None` when nothing
    /// exposed has it (the window itself does, then).
    pub fn accessible_focus_node(&self) -> Option<NodeKey> {
        self.a11y.tree.focus()
    }

    /// The focus action of an assistive technology: the node's item gets
    /// active focus as `forceActiveFocus()` gives it, so the focus scopes
    /// around it come along and a scope hands it on to its own focus
    /// item. Returns whether the node took it; a disabled node, or one
    /// that is not `Accessible.focusable`, does not.
    pub fn accessible_focus(&mut self, engine: &Engine, key: NodeKey) -> bool {
        let Some(data) = self.a11y.tree.get(key) else {
            return false;
        };
        if data.disabled || !data.focusable || key.is_announcement() {
            return false;
        }
        let item = item_of(key);
        if !engine.is_alive(item) {
            return false;
        }
        crate::focus::force_active_focus(engine, item, crate::focus::FocusReason::Other);
        true
    }
}
