//! `Controller` (Ljos.Core): a non-visual object that acts on the item it
//! is declared in, its host (docs/DIALECT.md §4.20).
//!
//! Its lifecycle is three signals. `attached(host)` runs once the host is
//! complete: after the host's own `Component.onCompleted`, so what the
//! host sets up there is set up, or at once for a Controller put into a
//! host that is complete already. `detached()` runs as the Controller is
//! destroyed, which its host's destruction does too, or as it leaves its
//! host for another: one declared in a popup moves to the popup's new
//! content item, and is attached to it.
//! `updated()` runs once after `attached` and then once a frame in which
//! a property of the host changed, before the frame is laid out.
//!
//! What it costs is paid by those that ask: the host's properties are
//! followed only for a Controller that handles `updated`, and a host is
//! told nothing for one that handles none of the three.

use std::cell::{Cell, RefCell};

use ljos_engine::{
    Component, ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectId, QmlList, QmlObject,
    QmlType, QtObject, SignalIndex, qml_impl,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::item::{Item, is_item};

/// `Controller` (Ljos.Core).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Ljos.Core", name = "Controller", extends = QtObject, default = "data")]
pub struct Controller {
    #[qml(base)]
    pub object: QtObject,
    /// The item it is declared in, or the host of the Controller it is
    /// declared in, or the item a popup it is declared in keeps its
    /// content in, which changes as the popup's content item is replaced
    /// ([`adopt`]). Unset outside all of them.
    #[qml(notify, readonly, holds = "Item")]
    pub host: Option<ObjectId>,
    /// What is declared in it: other Controllers, gestures, objects of
    /// its own. Those that act on an item act on its host.
    #[qml(list)]
    pub data: QmlList,
}

#[qml_impl]
impl Controller {
    /// The host is complete, or the Controller was put into one that is.
    #[qml(signal)]
    fn attached(host: Option<ObjectId>);
    /// The Controller is going, with its host or alone.
    #[qml(signal)]
    fn detached();
    /// Once after `attached`, then once a frame in which a property of
    /// the host changed.
    #[qml(signal)]
    fn updated();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        // Adopted before it was complete: put into a popup that is.
        if let Some(host) = host(e, me) {
            return take(e, me, e.owner(me), host);
        }
        let Some(host) = host_of(e, me) else {
            return Ok(());
        };
        e.set_internal(me, Controller::HOST, Some(host))?;
        // What acts on an item is found among the item's resources (the
        // gestures are): one declared in a Controller is the host's too.
        let mut owner = e.owner(me);
        while let Some(o) = owner.filter(|o| *o != host) {
            for child in crate::tree::list(e, o, Controller::DATA.index) {
                share_resource(e, host, child)?;
            }
            owner = e.owner(o);
        }
        if listens(e, me) {
            after_host_completes(e, me, host)?;
        }
        Ok(())
    }
}

/// The item `obj` acts on: the one it is declared in, through the
/// Controllers it is declared in. None when what it is declared in is
/// neither.
pub fn host_of(e: &Engine, obj: ObjectId) -> Option<ObjectId> {
    let mut at = e.owner(obj)?;
    loop {
        if is_item(e, at) {
            return Some(at);
        }
        if !e.inherits(at, Controller::static_meta()) {
            return None;
        }
        at = e.owner(at)?;
    }
}

/// The host a Controller has, once it is complete.
pub fn host(e: &Engine, controller: ObjectId) -> Option<ObjectId> {
    e.get(controller, Controller::HOST).ok().flatten()
}

/// Gives `obj`, if it is a Controller not declared in an item, the item
/// `host` as its host. For an object that is not an item but keeps what
/// is declared in it in an item (a popup and its content item), which
/// calls this for each object declared in it once that item is known,
/// and again when it is replaced: the Controller then leaves the item it
/// had, and is detached from it, and is attached to the new one. What is
/// declared in the Controller goes with it.
pub fn adopt(e: &Engine, obj: ObjectId, host: ObjectId) -> Result<(), EngineError> {
    // One declared in an item acts on that item, whatever holds it.
    if !e.inherits(obj, Controller::static_meta()) || !is_item(e, host) || host_of(e, obj).is_some()
    {
        return Ok(());
    }
    let had = self::host(e, obj);
    if had == Some(host) {
        return Ok(());
    }
    e.set_internal(obj, Controller::HOST, Some(host))?;
    let complete = e
        .flags(obj)
        .is_some_and(|f| f.contains(ljos_engine::ObjectFlags::COMPLETED));
    // One not complete yet takes its host when it completes.
    if !complete {
        return Ok(());
    }
    match had {
        Some(had) => {
            Controllers::of(e).detach(e, obj)?;
            leave(e, obj, had)?;
            take(e, obj, Some(had), host)
        }
        None => take(e, obj, e.owner(obj), host),
    }
}

/// A Controller adopted by `host`, which is not what it is declared in:
/// it and what is declared in it go among the host's resources, and a
/// gesture that took `was` for its `parent` and its `target` (what it is
/// declared in, as a gesture does when nobody gives it one, or the host
/// it had) takes the host.
fn take(
    e: &Engine,
    me: ObjectId,
    was: Option<ObjectId>,
    host: ObjectId,
) -> Result<(), EngineError> {
    share_resource(e, host, me)?;
    for child in crate::tree::list(e, me, Controller::DATA.index) {
        if e.inherits(child, Controller::static_meta()) {
            adopt(e, child, host)?;
        } else {
            share_resource(e, host, child)?;
        }
    }
    if let Some(was) = was.filter(|w| *w != host) {
        for name in ["parent", "target"] {
            if let Some(p) = e.find_property(me, name)
                && e.untrack(|| e.read(me, p)).as_object() == Some(was)
            {
                e.write_internal(
                    me,
                    p,
                    ljos_engine::Value::Object(host),
                    ljos_engine::WriteKind::Imperative,
                )?;
            }
        }
    }
    if listens(e, me) {
        after_host_completes(e, me, host)?;
    }
    Ok(())
}

/// Takes an adopted Controller, and what is declared in it but not
/// adopted with it, out of the resources of the host it had, so that a
/// gesture no longer acts on that item. The Controllers declared in it
/// leave it as they are adopted.
fn leave(e: &Engine, me: ObjectId, had: ObjectId) -> Result<(), EngineError> {
    if !e.is_alive(had) {
        return Ok(());
    }
    e.list_remove(had, Item::RESOURCES.index, me)?;
    for child in crate::tree::list(e, me, Controller::DATA.index) {
        if !e.inherits(child, Controller::static_meta()) && !is_item(e, child) {
            e.list_remove(had, Item::RESOURCES.index, child)?;
        }
    }
    Ok(())
}

const ATTACHED: &str = "attached";
const DETACHED: &str = "detached";
const UPDATED: &str = "updated";

/// A signal of the lifecycle. The same index in every derived type: the
/// tables are flattened base first.
fn signal(name: &str) -> SignalIndex {
    Controller::static_meta()
        .signal_index(name)
        .expect("declared by #[qml_impl]")
}

/// Whether anything is told of the Controller's lifecycle.
fn listens(e: &Engine, me: ObjectId) -> bool {
    [ATTACHED, DETACHED, UPDATED]
        .iter()
        .any(|s| e.signal_connected(me, signal(s)))
}

/// Puts `child` among `host`'s resources, where what acts on an item is
/// looked for, unless it is an item (which is a child of what it is
/// declared in) or there already.
fn share_resource(e: &Engine, host: ObjectId, child: ObjectId) -> Result<(), EngineError> {
    if is_item(e, child) || crate::tree::resources(e, host).contains(&child) {
        return Ok(());
    }
    e.list_push(host, Item::RESOURCES.index, child)
}

/// Attaches `me` once `host` has run its own completion handlers: at
/// once if it has (the Controller was made into a complete host), or
/// when the host's attached `Component` says it completed.
fn after_host_completes(e: &Engine, me: ObjectId, host: ObjectId) -> Result<(), EngineError> {
    let complete = e
        .flags(host)
        .is_some_and(|f| f.contains(ljos_engine::ObjectFlags::COMPLETED));
    if complete {
        return attach(e, me, host);
    }
    let component = e.attached_object(host, &MetaRef::Static(Component::static_meta()))?;
    let once = Cell::new(false);
    e.connect_to(
        component,
        e.signal_index(component, "completed")?,
        me,
        move |e, _| {
            // Moved to another host before this one completed: that one
            // attaches it.
            if !once.replace(true)
                && e.is_alive(me)
                && e.is_alive(host)
                && self::host(e, me) == Some(host)
            {
                warn(e, attach(e, me, host));
            }
        },
    )?;
    Ok(())
}

fn attach(e: &Engine, me: ObjectId, host: ObjectId) -> Result<(), EngineError> {
    let follows = e.signal_connected(me, signal(UPDATED));
    let controllers = Controllers::of(e);
    let first = controllers.watched.borrow_mut().insert(me);
    if first {
        e.connect(me, SignalIndex::DESTROYED, move |e, _| {
            let controllers = Controllers::of(e);
            controllers.watched.borrow_mut().remove(&me);
            // Going: there is nobody left to tell that the handler failed.
            let _ = controllers.detach(e, me);
        })?;
    }
    controllers.attached.borrow_mut().insert(me, follows);
    Controller::emit_attached(&Cx::new(e, me), Some(host))?;
    if follows && e.is_alive(me) && e.is_alive(host) {
        controllers.follow(e, me, host)?;
    }
    Ok(())
}

fn warn(e: &Engine, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.warn(format!("Controller: {err}"));
    }
}

/// The Controllers that handle `updated`, by host. Per engine.
#[derive(Default)]
pub(crate) struct Controllers {
    hosts: RefCell<FxHashMap<ObjectId, Followed>>,
    /// Hosts that changed since their Controllers were last told, in the
    /// order they changed.
    due: RefCell<Vec<ObjectId>>,
    /// The host whose Controllers are being told: what they write does
    /// not make it due again.
    telling: Cell<Option<ObjectId>>,
    /// The Controllers attached to a host, each with whether it follows
    /// the host for `updated`.
    attached: RefCell<FxHashMap<ObjectId, bool>>,
    /// The Controllers whose destruction detaches them: connected once,
    /// however many hosts they are attached to in turn.
    watched: RefCell<rustc_hash::FxHashSet<ObjectId>>,
}

#[derive(Default)]
struct Followed {
    controllers: SmallVec<[ObjectId; 2]>,
    /// The host's change signals, connected while a Controller follows.
    connections: Vec<ConnectionId>,
}

impl Controllers {
    fn of(e: &Engine) -> std::rc::Rc<Self> {
        e.extension::<Self>()
    }

    fn follow(&self, e: &Engine, me: ObjectId, host: ObjectId) -> Result<(), EngineError> {
        let first = {
            let mut hosts = self.hosts.borrow_mut();
            let f = hosts.entry(host).or_default();
            f.controllers.push(me);
            f.controllers.len() == 1
        };
        if first {
            let signals: Vec<SignalIndex> = e
                .meta_of(host)
                .map(|m| m.properties.iter().filter_map(|p| p.notify).collect())
                .unwrap_or_default();
            let mut connections = Vec::with_capacity(signals.len());
            for s in signals {
                connections.push(e.connect(host, s, move |e, _| changed(e, host))?);
            }
            if let Some(f) = self.hosts.borrow_mut().get_mut(&host) {
                f.connections = connections;
            }
        }
        // Once after `attached`.
        self.make_due(e, host);
        Ok(())
    }

    /// Detaches `me` from its host if it is attached: as it is destroyed,
    /// or as it leaves the host for another.
    fn detach(&self, e: &Engine, me: ObjectId) -> Result<(), EngineError> {
        let Some(follows) = self.attached.borrow_mut().remove(&me) else {
            return Ok(());
        };
        if follows {
            self.unfollow(e, me);
        }
        Controller::emit_detached(&Cx::new(e, me))
    }

    fn unfollow(&self, e: &Engine, me: ObjectId) {
        let gone = {
            let mut hosts = self.hosts.borrow_mut();
            let Some((&host, f)) = hosts.iter_mut().find(|(_, f)| f.controllers.contains(&me))
            else {
                return;
            };
            f.controllers.retain(|c| *c != me);
            if f.controllers.is_empty() {
                hosts.remove(&host).map(|f| f.connections)
            } else {
                None
            }
        };
        for c in gone.into_iter().flatten() {
            e.disconnect(c);
        }
    }

    fn make_due(&self, e: &Engine, host: ObjectId) {
        if self.telling.get() == Some(host) {
            return;
        }
        let mut due = self.due.borrow_mut();
        if due.contains(&host) {
            return;
        }
        due.push(host);
        drop(due);
        crate::scene::updates_wanted(e);
    }
}

fn changed(e: &Engine, host: ObjectId) {
    Controllers::of(e).make_due(e, host);
}

/// Whether a Controller is to be told its host was updated.
pub(crate) fn pending(e: &Engine) -> bool {
    !Controllers::of(e).due.borrow().is_empty()
}

/// Tells the Controllers of the hosts that changed: once each, in the
/// order the hosts changed. A frame's step, before layout.
pub(crate) fn tell_updated(e: &Engine) {
    let me = Controllers::of(e);
    let due = std::mem::take(&mut *me.due.borrow_mut());
    for host in due {
        let controllers = match me.hosts.borrow().get(&host) {
            Some(f) => f.controllers.clone(),
            None => continue,
        };
        me.telling.set(Some(host));
        e.batch(|| {
            for c in controllers {
                if e.is_alive(c) {
                    warn(e, Controller::emit_updated(&Cx::new(e, c)));
                }
            }
        });
        me.telling.set(None);
    }
}
