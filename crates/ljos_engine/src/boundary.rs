//! UI boundaries (ARCHITECTURE.md §3.4, §9.4).
//!
//! A boundary is an object that makes its content from a component and can
//! make it again: a Loader, an `ErrorBoundary`. A fault in the content is
//! the boundary's to handle. The types that are boundaries live in the
//! crates built on the engine; they register how to stop and start their
//! content ([`Boundary`], [`Engine::register_boundary`]) and the engine
//! does the rest:
//!
//! - It finds the boundary around the object a fault happened in.
//! - It keeps the supervisor tree of §9.4 for them
//!   ([`ljos_runtime::SupervisorTree`]): each boundary that has had a
//!   fault is a supervisor with one child, its content. A boundary inside
//!   another's content is linked to that content, so when it gives up (its
//!   restart intensity is exceeded) the fault is the outer boundary's, and
//!   when the outer content is made again it is gone with it. A boundary
//!   with none around it is under `UiEngineSup`; when it gives up, the
//!   fault is the application's.
//! - It gives every boundary a [`ProcId`], the owner its work is charged
//!   to.
//!
//! Supervisors are made at a boundary's first fault, not when the boundary
//! is made: an application that never faults has a tree of the four
//! standard supervisors, or none at all.
//!
//! An exception in a handler is not a fault of a boundary unless the
//! boundary asks ([`BoundaryConfig::catch_exceptions`]): Qt logs it and
//! carries on, and so does a Loader.

use std::any::Any;
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_runtime::{
    ChildId, ChildKind, ChildSpec, ExitReason, Fault, Intensity, Outcome, ProcId, ProcIdAllocator,
    Restart, StandardSups, Strategy, SupId, SupervisorTree,
};

use crate::engine::Engine;
use crate::fault::{FaultReport, TakenBy};
use crate::ids::ObjectId;
use crate::meta::MetaObject;
use crate::object::{ObjectFlags, QmlType};

/// How deep a search for the boundary around an object goes. Ownership
/// has no cycles; this is for a bug that makes one.
const MAX_DEPTH: usize = 4096;

/// How a boundary wants its content supervised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryConfig {
    /// `Permanent`: the content is made again after a fault. `Temporary`:
    /// it stays stopped (a Loader shows `Loader.Error`).
    pub restart: Restart,
    /// More restarts than this within the window and the boundary gives
    /// up: the fault goes to the boundary around it.
    pub intensity: Intensity,
    /// Whether a JS exception in the content is a fault of this boundary.
    pub catch_exceptions: bool,
}

/// What a boundary type does with its content. One value serves every
/// object of the type; the object is passed in.
pub trait Boundary {
    fn config(&self, engine: &Engine, boundary: ObjectId) -> BoundaryConfig;

    /// Whether `child`, an object `boundary` owns, is content it answers
    /// for. Not everything a boundary owns is: a fault in the fallback an
    /// `ErrorBoundary` shows belongs to the boundary around it.
    fn guards(&self, _engine: &Engine, _boundary: ObjectId, _child: ObjectId) -> bool {
        true
    }

    /// Stops the content after `report`'s fault: discards it and shows
    /// the fault. Called between turns, so nothing of the content is on
    /// the stack.
    fn stop(&self, engine: &Engine, boundary: ObjectId, report: &FaultReport);

    /// Starts the content again, or whatever stands in for it (a
    /// fallback). An error is one more exit of the content: it is retried
    /// only as often as the intensity allows.
    fn start(&self, engine: &Engine, boundary: ObjectId) -> Result<(), Fault>;
}

/// The owner of everything no boundary is around.
pub const APPLICATION_PROC: ProcId = ProcId::new(0, 0);

struct Node {
    sup: SupId,
    content: ChildId,
}

struct Procs {
    alloc: ProcIdAllocator,
    by_object: HashMap<ObjectId, ProcId>,
    /// Size after the last sweep of dead objects.
    swept: usize,
}

#[derive(Default)]
pub(crate) struct Boundaries {
    types: RefCell<Vec<(&'static MetaObject, Rc<dyn Boundary>)>>,
    tree: OnceCell<(SupervisorTree<Engine>, StandardSups)>,
    nodes: RefCell<HashMap<ObjectId, Node>>,
    procs: RefCell<Option<Procs>>,
    /// The fault being delivered, for the content that is stopped for it.
    current: RefCell<Option<FaultReport>>,
}

impl Boundaries {
    fn tree(&self) -> &(SupervisorTree<Engine>, StandardSups) {
        self.tree.get_or_init(SupervisorTree::standard)
    }
}

/// What became of a delivered fault.
pub(crate) enum Tripped {
    /// A boundary took it, or it needs no one.
    Handled(TakenBy),
    /// No boundary took it: it is the application's.
    Unhandled(Box<FaultReport>),
}

/// A boundary's content as a child of its supervisor.
struct Content {
    boundary: ObjectId,
    behaviour: Rc<dyn Boundary>,
}

impl ChildSpec<Engine> for Content {
    fn kind(&self) -> ChildKind {
        ChildKind::Boundary
    }

    fn start(&self, engine: &mut Engine) -> Result<(), Fault> {
        if !engine.is_alive(self.boundary) {
            return Ok(());
        }
        let started = engine.untrack(|| self.behaviour.start(engine, self.boundary));
        if let Err(fault) = &started {
            // What is escalated, if this was the last try, is why the
            // content could not be made again.
            let (file, line) = engine.declared_place(Some(self.boundary));
            if let Some(r) = engine.inner.faults.boundaries.current.borrow_mut().as_mut() {
                r.fault = fault.clone();
                r.origin = Some(self.boundary);
                r.file = file;
                r.line = line;
                r.stack.clear();
            }
        }
        started
    }

    fn stop(&self, engine: &mut Engine, reason: &ExitReason) {
        // `Shutdown`: the content of a boundary further out is going, and
        // this boundary with it. There is nothing to do for an object
        // that is about to be destroyed.
        if !matches!(reason, ExitReason::Fault(_)) || !engine.is_alive(self.boundary) {
            return;
        }
        let report = engine.inner.faults.boundaries.current.borrow().clone();
        if let Some(report) = report {
            engine.untrack(|| self.behaviour.stop(engine, self.boundary, &report));
        }
    }
}

impl Engine {
    /// Makes every `T` (and what derives from it) a boundary that behaves
    /// as `behaviour` says.
    pub fn register_boundary<T: QmlType>(&self, behaviour: Rc<dyn Boundary>) {
        let meta = T::static_meta();
        let mut types = self.inner.faults.boundaries.types.borrow_mut();
        match types.iter_mut().find(|(m, _)| std::ptr::eq(*m, meta)) {
            Some(t) => t.1 = behaviour,
            None => types.push((meta, behaviour)),
        }
    }

    /// How `obj` behaves as a boundary, when it is one and alive.
    fn boundary_behaviour(&self, obj: ObjectId) -> Option<Rc<dyn Boundary>> {
        let types = self.inner.faults.boundaries.types.borrow();
        if types.is_empty() {
            return None;
        }
        let meta = self.meta_of(obj)?;
        types
            .iter()
            .find(|(m, _)| meta.inherits(m))
            .map(|(_, b)| b.clone())
    }

    /// The boundaries around `origin`, the nearest first. An object that
    /// is a boundary itself belongs to the one around it.
    pub(crate) fn boundaries_around(&self, origin: Option<ObjectId>) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let Some(mut cur) = origin else {
            return out;
        };
        if self.inner.faults.boundaries.types.borrow().is_empty() {
            return out;
        }
        self.untrack(|| {
            for _ in 0..MAX_DEPTH {
                // Ownership first: it is what a boundary's restart
                // destroys. An item made in Rust and only shown under
                // another has no owner to go by.
                let Some(parent) = self.owner(cur).or_else(|| self.visual_parent(cur)) else {
                    break;
                };
                if let Some(b) = self.boundary_behaviour(parent)
                    && b.guards(self, parent, cur)
                {
                    out.push(parent);
                }
                cur = parent;
            }
        });
        out
    }

    /// The boundary around `obj`, when there is one.
    pub fn boundary_of(&self, obj: ObjectId) -> Option<ObjectId> {
        self.boundaries_around(Some(obj)).first().copied()
    }

    /// Whether `obj` or something that owns it is on its way out.
    pub(crate) fn doomed(&self, obj: ObjectId) -> bool {
        let store = self.inner.store.borrow();
        let mut cur = Some(obj);
        for _ in 0..MAX_DEPTH {
            let Some(o) = cur else {
                return false;
            };
            let Some(h) = store.objects.get(o) else {
                return true;
            };
            if h.flags.contains(ObjectFlags::PENDING_DELETE) {
                return true;
            }
            cur = h.owner;
        }
        false
    }

    /// The supervisor node of `boundary`, made on first use: its content
    /// as a child. `above` are the boundaries around it, the nearest
    /// first.
    fn boundary_node(&self, boundary: ObjectId, above: &[ObjectId]) -> Option<ChildId> {
        let b = &self.inner.faults.boundaries;
        let (tree, sups) = b.tree();
        let behaviour = self.boundary_behaviour(boundary)?;
        let cfg = self.untrack(|| behaviour.config(self, boundary));
        let existing = b
            .nodes
            .borrow()
            .get(&boundary)
            .map(|n| n.sup)
            .filter(|s| tree.contains(*s) && !tree.is_down(*s));
        let sup = match existing {
            Some(sup) => {
                // The boundary's properties may have changed since.
                tree.set_intensity(sup, cfg.intensity);
                sup
            }
            None => {
                let name = format!("boundary {boundary:?}");
                let outer = above
                    .split_first()
                    .and_then(|(outer, rest)| self.boundary_node(*outer, rest));
                match outer {
                    Some(owner) => tree
                        .add_linked_supervisor(owner, &name, Strategy::OneForOne, cfg.intensity)
                        .ok()?,
                    // Temporary: when it gives up nothing above restarts
                    // it; the fault is the application's.
                    None => tree
                        .add_supervisor(
                            sups.ui,
                            &name,
                            Restart::Temporary,
                            Strategy::OneForOne,
                            cfg.intensity,
                        )
                        .ok()?,
                }
            }
        };
        // A temporary content is forgotten when it exits, so it may be
        // missing from a supervisor that is still there.
        let content = match tree.children(sup).first() {
            Some(c) => c.id,
            None => tree
                .add_child(
                    sup,
                    "content",
                    cfg.restart,
                    Rc::new(Content {
                        boundary,
                        behaviour,
                    }),
                )
                .ok()?,
        };
        b.nodes.borrow_mut().insert(boundary, Node { sup, content });
        Some(content)
    }

    /// Delivers one fault: to the nearest boundary that takes it.
    pub(crate) fn trip_boundary(&self, report: &FaultReport) -> Tripped {
        let b = &self.inner.faults.boundaries;
        let exception = matches!(report.fault, Fault::JsException(_));
        let unhandled = || {
            if exception {
                // Logged, and nobody asked for more.
                Tripped::Handled(TakenBy::Log)
            } else {
                Tripped::Unhandled(Box::new(report.clone()))
            }
        };
        // The subtree it happened in was discarded for an earlier fault.
        if report.target.is_none() && report.origin.is_some_and(|o| self.doomed(o)) {
            return Tripped::Handled(TakenBy::Discarded);
        }
        let mut chain: Vec<(ObjectId, Rc<dyn Boundary>)> = Vec::new();
        for o in report
            .target
            .into_iter()
            .chain(report.boundaries.iter().copied())
        {
            if chain.iter().any(|(c, _)| *c == o) || self.doomed(o) {
                continue;
            }
            if let Some(behaviour) = self.boundary_behaviour(o) {
                chain.push((o, behaviour));
            }
        }
        let first = if exception {
            chain
                .iter()
                .position(|(o, be)| self.untrack(|| be.config(self, *o)).catch_exceptions)
        } else {
            (!chain.is_empty()).then_some(0)
        };
        let Some(first) = first else {
            return unhandled();
        };
        let ids: Vec<ObjectId> = chain[first..].iter().map(|(o, _)| *o).collect();
        let Some(content) = self.boundary_node(ids[0], &ids[1..]) else {
            return unhandled();
        };
        let (tree, _) = b.tree();
        *b.current.borrow_mut() = Some(report.clone());
        let outcome =
            tree.child_failed(content, report.fault.clone(), self.now(), &mut self.clone());
        let last = b
            .current
            .borrow_mut()
            .take()
            .unwrap_or_else(|| report.clone());
        let by = |f: &dyn Fn(&Node) -> bool| {
            b.nodes.borrow().iter().find(|(_, n)| f(n)).map(|(o, _)| *o)
        };
        let handled = match outcome {
            Ok(Outcome::Restarted { by: sup, .. }) => Some(by(&|n| n.sup == sup).unwrap_or(ids[0])),
            // Handled after the exit being handled now: by the nearest
            // boundary, unless that one gives up then.
            Ok(Outcome::Queued) => Some(ids[0]),
            // Left stopped at a boundary's content (a Loader's): that
            // boundary handled it. Left stopped at a boundary's own
            // supervisor: the outermost boundary gave up.
            Ok(Outcome::Stopped { child }) => by(&|n| n.content == child),
            Ok(Outcome::GaveUp { .. }) | Err(_) => None,
        };
        self.sweep_boundary_nodes();
        match handled {
            Some(object) => Tripped::Handled(TakenBy::Boundary {
                object,
                name: self.label_of(object).unwrap_or_default(),
            }),
            None => Tripped::Unhandled(Box::new(last)),
        }
    }

    /// Forgets the nodes of boundaries that are gone, and of supervisors
    /// the tree dropped (they were linked to content that was restarted).
    fn sweep_boundary_nodes(&self) {
        let b = &self.inner.faults.boundaries;
        let (tree, _) = b.tree();
        let gone: Vec<(ObjectId, SupId, bool)> = b
            .nodes
            .borrow()
            .iter()
            .filter_map(|(o, n)| {
                let alive = self.is_alive(*o);
                let kept = tree.contains(n.sup) && !tree.is_down(n.sup);
                (!alive || !kept).then_some((*o, n.sup, kept))
            })
            .collect();
        for (o, sup, _) in gone {
            b.nodes.borrow_mut().remove(&o);
            // Nothing starts or stops here: the content went with its
            // object.
            tree.remove_supervisor(sup);
        }
    }

    /// How often `boundary`'s content was restarted within its window, as
    /// of its last fault: restarts leave the count when the next fault is
    /// counted, not as time passes. For tests and tools.
    pub fn boundary_recent_restarts(&self, boundary: ObjectId) -> usize {
        let b = &self.inner.faults.boundaries;
        let sup = b.nodes.borrow().get(&boundary).map(|n| n.sup);
        match (sup, b.tree.get()) {
            (Some(sup), Some((tree, _))) => tree.recent_restarts(sup),
            _ => 0,
        }
    }

    /// The supervisor tree, each supervisor before what it supervises,
    /// for tools. A boundary is in it from its first fault until it gives
    /// up or goes; before any, the tree has the standard supervisors.
    pub fn supervisors(&self) -> Vec<SupervisorInfo> {
        let b = &self.inner.faults.boundaries;
        let (tree, _) = b.tree();
        let of: HashMap<SupId, (ObjectId, ChildId)> = b
            .nodes
            .borrow()
            .iter()
            .map(|(o, n)| (n.sup, (*o, n.content)))
            .collect();
        let mut out = Vec::new();
        let mut stack = vec![(tree.root(), 0)];
        while let Some((sup, depth)) = stack.pop() {
            let Some((strategy, intensity)) = tree.policy(sup) else {
                continue;
            };
            let children = tree.children(sup);
            let boundary = of.get(&sup).copied();
            let content = boundary.and_then(|(_, content)| {
                children
                    .iter()
                    .find(|c| c.id == content)
                    .map(|c| (c.restart, c.running))
            });
            out.push(SupervisorInfo {
                depth,
                name: tree.name(sup).map(|n| n.to_string()).unwrap_or_default(),
                boundary: boundary.map(|(o, _)| o),
                strategy,
                intensity,
                restarts: tree.recent_restarts(sup),
                down: tree.is_down(sup),
                content,
            });
            // Pushed last first, so that they come out in start order; a
            // boundary inside another's content is under that content.
            let mut below = Vec::new();
            for c in &children {
                match c.supervisor {
                    Some(s) => below.push(s),
                    None => below.extend(tree.linked(c.id)),
                }
            }
            stack.extend(below.into_iter().rev().map(|s| (s, depth + 1)));
        }
        out
    }

    /// The supervisor tree as text, for tools; `None` before the first
    /// fault of a boundary.
    pub fn supervisor_dump(&self) -> Option<String> {
        self.inner
            .faults
            .boundaries
            .tree
            .get()
            .map(|(tree, _)| tree.dump())
    }

    // ---------------------------------------------------------------- owners

    /// The owner id of the boundary `boundary`, given on first use.
    pub fn boundary_proc(&self, boundary: ObjectId) -> ProcId {
        let mut procs = self.inner.faults.boundaries.procs.borrow_mut();
        let procs = procs.get_or_insert_with(|| {
            let mut alloc = ProcIdAllocator::new();
            // The application's, so that no boundary is given it.
            let app = alloc.alloc();
            debug_assert_eq!(app, APPLICATION_PROC);
            Procs {
                alloc,
                by_object: HashMap::new(),
                swept: 0,
            }
        });
        if let Some(p) = procs.by_object.get(&boundary) {
            return *p;
        }
        if procs.by_object.len() >= procs.swept * 2 + 16 {
            // Ids of boundaries that are gone can be given out again.
            let store = self.inner.store.borrow();
            let dead: Vec<ObjectId> = procs
                .by_object
                .keys()
                .copied()
                .filter(|o| !store.objects.contains_key(*o))
                .collect();
            for o in dead {
                if let Some(p) = procs.by_object.remove(&o) {
                    procs.alloc.free(p);
                }
            }
            procs.swept = procs.by_object.len();
        }
        let p = procs.alloc.alloc();
        procs.by_object.insert(boundary, p);
        p
    }

    /// The owner work done for `obj` is charged to: `obj` when it is a
    /// boundary, else the boundary around it, else the application.
    pub fn proc_for(&self, obj: ObjectId) -> ProcId {
        if self.inner.faults.boundaries.types.borrow().is_empty() {
            return APPLICATION_PROC;
        }
        let boundary = if self.boundary_behaviour(obj).is_some() {
            Some(obj)
        } else {
            self.boundary_of(obj)
        };
        boundary.map_or(APPLICATION_PROC, |b| self.boundary_proc(b))
    }

    /// The boundary an owner id was given to, while it is alive.
    pub fn proc_owner(&self, proc: ProcId) -> Option<ObjectId> {
        let procs = self.inner.faults.boundaries.procs.borrow();
        procs
            .as_ref()?
            .by_object
            .iter()
            .find(|(o, p)| **p == proc && self.is_alive(**o))
            .map(|(o, _)| *o)
    }

    // -------------------------------------------------------------- content

    /// Makes an object of the `Component` `component` that `owner` owns
    /// without making it a value of the owner's default property: a
    /// boundary whose default property is the component itself must not
    /// have what it makes of it written back over it. `placed` gets the
    /// root before any of its bindings runs, the place to give it its
    /// visual parent.
    pub fn component_create_owned(
        &self,
        component: ObjectId,
        owner: ObjectId,
        placed: impl FnOnce(&Engine, ObjectId),
    ) -> Result<ObjectId, crate::EngineError> {
        self.create_delegate(component, Some(owner), |e, root, _| placed(e, root))
    }

    // ------------------------------------------------------------- snapshots

    /// What the natives under `root` hold that their [`Migrate`]
    /// implementations say is worth keeping, for a restart of the subtree
    /// from its component. `None` when nothing does.
    ///
    /// [`Migrate`]: crate::Migrate
    pub fn snapshot_subtree(&self, root: ObjectId) -> Option<SubtreeSnapshot> {
        if !self.has_migrators() {
            return None;
        }
        let mut entries = Vec::new();
        let mut stack = vec![(root, Vec::new())];
        while let Some((obj, path)) = stack.pop() {
            for (migrator, state) in self.migrate_snapshot(obj) {
                entries.push(Kept {
                    path: path.clone(),
                    migrator,
                    state,
                });
            }
            for (i, c) in self.children(obj).into_iter().enumerate() {
                let mut p = path.clone();
                p.push(i as u32);
                stack.push((c, p));
            }
        }
        (!entries.is_empty()).then_some(SubtreeSnapshot { entries })
    }

    /// Gives the subtree under `root`, made again from the same
    /// component, what [`Engine::snapshot_subtree`] kept. An object is
    /// found by its place among what its owner owns and takes the state
    /// only when it is of the type the state came from; a state that
    /// cannot be given is logged and dropped.
    pub fn restore_subtree(&self, root: ObjectId, snapshot: SubtreeSnapshot) {
        for kept in snapshot.entries {
            let mut obj = Some(root);
            for i in &kept.path {
                obj = obj.and_then(|o| self.children(o).get(*i as usize).copied());
            }
            let Some(obj) = obj else {
                continue;
            };
            if let Err(e) = self.migrate_restore(obj, kept.migrator, kept.state) {
                let place = self.location_of(obj).unwrap_or_default();
                self.warn(format!("{place}state not restored: {e}"));
            }
        }
    }
}

/// One supervisor of [`Engine::supervisors`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupervisorInfo {
    /// 0 for the root.
    pub depth: usize,
    /// The tree's name for it: `app`, `ui`, `services`, `plugins`, or
    /// `boundary …` for a boundary's.
    pub name: String,
    /// The boundary it supervises the content of.
    pub boundary: Option<ObjectId>,
    pub strategy: Strategy,
    pub intensity: Intensity,
    /// Restarts it counts inside its window, as of its last fault.
    pub restarts: usize,
    /// It gave up and nothing restarted it.
    pub down: bool,
    /// For a boundary: how its content restarts, and whether it runs.
    pub content: Option<(Restart, bool)>,
}

struct Kept {
    /// The object's place: the index among its owner's children, from the
    /// root down.
    path: Vec<u32>,
    migrator: usize,
    state: Box<dyn Any>,
}

/// See [`Engine::snapshot_subtree`].
pub struct SubtreeSnapshot {
    entries: Vec<Kept>,
}

impl std::fmt::Debug for SubtreeSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubtreeSnapshot")
            .field("objects", &self.entries.len())
            .finish()
    }
}
