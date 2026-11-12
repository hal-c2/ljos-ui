//! `Controller` (Ljos.Core): a non-visual object that acts on the item it
//! is declared in, its host (docs/DIALECT.md §4.20).
//!
//! Its lifecycle is three signals. `attached(host)` runs once the host is
//! complete: after the host's own `Component.onCompleted`, so what the
//! host sets up there is set up, or at once for a Controller put into a
//! host that is complete already. `detached()` runs once, as the
//! Controller is destroyed, which its host's destruction does too.
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
    /// declared in. Unset outside an item.
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
            if !once.replace(true) && e.is_alive(me) && e.is_alive(host) {
                warn(e, attach(e, me, host));
            }
        },
    )?;
    Ok(())
}

fn attach(e: &Engine, me: ObjectId, host: ObjectId) -> Result<(), EngineError> {
    let follows = e.signal_connected(me, signal(UPDATED));
    e.connect(me, SignalIndex::DESTROYED, move |e, _| {
        if follows {
            Controllers::of(e).unfollow(e, me);
        }
        let _ = Controller::emit_detached(&Cx::new(e, me));
    })?;
    Controller::emit_attached(&Cx::new(e, me), Some(host))?;
    if follows && e.is_alive(me) && e.is_alive(host) {
        Controllers::of(e).follow(e, me, host)?;
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
