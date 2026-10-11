//! The supervisor tree: who restarts what (ARCHITECTURE.md 9.4).
//!
//! A [`SupervisorTree`] is supervisors holding children. A child is a leaf
//! described by a [`ChildSpec`] (a UI boundary's content, a tokio task; the
//! kind is open, see [`ChildKind`]) or another supervisor. Each supervisor
//! decides with the policy in [`Supervisor`](super::Supervisor): strategy,
//! restart kind and restart intensity. A supervisor that exceeds its
//! intensity gives up: its children are stopped and its own exit is its
//! parent's to decide, up to the root.
//!
//! Two things differ from a textbook OTP tree because of what the UI needs:
//!
//! - A supervisor can be *linked* to a leaf ([`SupervisorTree::add_linked_supervisor`]).
//!   A UI boundary inside another boundary's content is made by that
//!   content, not by the outer supervisor, so it cannot be one of its
//!   children. It is linked to the content instead: when it gives up the
//!   content exits with the same reason, and when the content stops it goes
//!   with it.
//! - Nothing here owns a clock or a thread. The owner says when a child
//!   exited (`now` comes from its [`Clock`](crate::Clock), virtual in
//!   tests) and passes the context `Cx` every start and stop runs with.
//!
//! The tree lives on one thread. It holds no borrow while a child starts
//! or stops, so a child may add or remove supervisors and children while it
//! does (a boundary's content registers the boundaries inside it). An exit
//! reported from inside a start or stop is queued and handled when the one
//! in progress is done.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

use crate::fault::Fault;
use crate::time::Time;

use super::{
    ChildId, Directive, ExitReason, Intensity, Restart, Strategy, Supervisor, SupervisorError,
};

/// What a child is. Open on purpose: pinned plugin threads and the
/// development tools are children too, in later milestones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChildKind {
    /// Another supervisor.
    Supervisor,
    /// A UI boundary's content: a Loader's item, an ErrorBoundary's content.
    Boundary,
    /// An async task.
    Task,
    /// A kind this crate has no name for yet.
    Other(&'static str),
}

/// How to start and stop one child. Methods take `&self` and the tree
/// holds no borrow while they run, so they may call back into the tree.
pub trait ChildSpec<Cx> {
    fn kind(&self) -> ChildKind;

    /// Starts the child, or starts it again after it exited or was
    /// stopped. An error counts as one more exit of the child, so a child
    /// that cannot start is retried only as often as the intensity allows.
    fn start(&self, cx: &mut Cx) -> Result<(), Fault>;

    /// Stops the child. `reason` is why: `Shutdown` when a sibling's
    /// restart or the tree takes it down, the fault when it is the one
    /// that failed (see [`SupervisorTree::child_failed`]).
    fn stop(&self, cx: &mut Cx, reason: &ExitReason);
}

/// One supervisor of a tree. Never reused within a tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupId(u64);

/// What became of an exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `child` is left stopped: its restart kind says so. It is the child
    /// that exited, or, after supervisors gave up, the one the exit ended
    /// at.
    Stopped { child: ChildId },
    /// Supervisor `by` restarted `started` (in start order).
    Restarted { by: SupId, started: Vec<ChildId> },
    /// The root gave up. Everything is stopped.
    GaveUp { sup: SupId, reason: ExitReason },
    /// Reported while another exit was being handled; it is handled right
    /// after that one. If it makes the root give up, the call in progress
    /// returns [`Outcome::GaveUp`].
    Queued,
}

/// A child as [`SupervisorTree::children`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildInfo {
    pub id: ChildId,
    pub name: Arc<str>,
    pub kind: ChildKind,
    pub restart: Restart,
    pub running: bool,
    /// The supervisor this child is, for [`ChildKind::Supervisor`].
    pub supervisor: Option<SupId>,
}

enum Up {
    Root,
    /// A child of the supervisor, under this id in its policy.
    Child(SupId, ChildId),
    /// Linked to a leaf of the supervisor.
    Under(SupId, ChildId),
}

enum What<Cx> {
    Leaf(Rc<dyn ChildSpec<Cx>>),
    Sup(SupId),
}

impl<Cx> Clone for What<Cx> {
    fn clone(&self) -> Self {
        match self {
            What::Leaf(s) => What::Leaf(s.clone()),
            What::Sup(s) => What::Sup(*s),
        }
    }
}

struct Entry<Cx> {
    id: ChildId,
    name: Arc<str>,
    restart: Restart,
    what: What<Cx>,
    running: bool,
    /// Supervisors linked to this leaf.
    linked: Vec<SupId>,
}

struct Node<Cx> {
    name: Arc<str>,
    policy: Supervisor,
    up: Up,
    /// In start order, as in `policy`.
    children: Vec<Entry<Cx>>,
    /// It gave up and nobody restarted it.
    down: bool,
}

struct Pending {
    child: ChildId,
    reason: ExitReason,
    now: Time,
    stop_first: bool,
}

struct Inner<Cx> {
    nodes: HashMap<SupId, Node<Cx>>,
    owner: HashMap<ChildId, SupId>,
    root: SupId,
    next_sup: u64,
    next_child: u64,
    busy: bool,
    queue: VecDeque<Pending>,
}

/// See the module documentation.
pub struct SupervisorTree<Cx> {
    inner: RefCell<Inner<Cx>>,
}

impl<Cx> std::fmt::Debug for SupervisorTree<Cx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.dump())
    }
}

/// `ServiceSup` of ARCHITECTURE.md 9.4: 5 restarts in 30 s.
pub const SERVICE_INTENSITY: Intensity = Intensity::new(5, std::time::Duration::from_secs(30));
/// `PluginSup` of ARCHITECTURE.md 9.4: 3 restarts in 60 s.
pub const PLUGIN_INTENSITY: Intensity = Intensity::new(3, std::time::Duration::from_secs(60));
/// What `AppSup` and `UiEngineSup` allow of their own children.
pub const APP_INTENSITY: Intensity = Intensity::new(3, std::time::Duration::from_secs(5));

/// The supervisors of the standard tree (ARCHITECTURE.md 9.4):
///
/// ```text
/// app ─┬─ ui        boundaries
///      ├─ services  one_for_one, 5 restarts / 30 s
///      └─ plugins   one_for_one, 3 / 60 s (its children are transient)
/// ```
///
/// `DevSup` is added by the development tools when they exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandardSups {
    pub app: SupId,
    pub ui: SupId,
    pub services: SupId,
    pub plugins: SupId,
}

impl<Cx> SupervisorTree<Cx> {
    /// A tree with its root supervisor.
    pub fn new(name: &str, strategy: Strategy, intensity: Intensity) -> Self {
        let root = SupId(0);
        let mut nodes = HashMap::new();
        nodes.insert(
            root,
            Node {
                name: name.into(),
                policy: Supervisor::new(strategy, intensity),
                up: Up::Root,
                children: Vec::new(),
                down: false,
            },
        );
        Self {
            inner: RefCell::new(Inner {
                nodes,
                owner: HashMap::new(),
                root,
                next_sup: 1,
                next_child: 1,
                busy: false,
                queue: VecDeque::new(),
            }),
        }
    }

    /// The standard tree. A fault in the UI must not take the services
    /// down and the other way round, so `app` is one-for-one and its three
    /// supervisors are permanent.
    pub fn standard() -> (Self, StandardSups) {
        let tree = Self::new("app", Strategy::OneForOne, APP_INTENSITY);
        let app = tree.root();
        let add = |name, intensity| {
            tree.add_supervisor(
                app,
                name,
                Restart::Permanent,
                Strategy::OneForOne,
                intensity,
            )
            .expect("the root exists")
        };
        let sups = StandardSups {
            app,
            ui: add("ui", APP_INTENSITY),
            services: add("services", SERVICE_INTENSITY),
            plugins: add("plugins", PLUGIN_INTENSITY),
        };
        (tree, sups)
    }

    pub fn root(&self) -> SupId {
        self.inner.borrow().root
    }

    /// A supervisor that is a child of `parent`. When it gives up, `parent`
    /// treats that as the exit of a child with restart kind `restart`.
    pub fn add_supervisor(
        &self,
        parent: SupId,
        name: &str,
        restart: Restart,
        strategy: Strategy,
        intensity: Intensity,
    ) -> Result<SupId, SupervisorError> {
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;
        if !inner.nodes.contains_key(&parent) {
            return Err(SupervisorError::UnknownSupervisor);
        }
        let id = SupId(inner.next_sup);
        inner.next_sup += 1;
        let child = ChildId(inner.next_child);
        inner.next_child += 1;
        let node = inner.nodes.get_mut(&parent).expect("checked above");
        node.policy.add_child(child, restart)?;
        node.children.push(Entry {
            id: child,
            name: name.into(),
            restart,
            what: What::Sup(id),
            running: true,
            linked: Vec::new(),
        });
        inner.owner.insert(child, parent);
        inner.nodes.insert(
            id,
            Node {
                name: name.into(),
                policy: Supervisor::new(strategy, intensity),
                up: Up::Child(parent, child),
                children: Vec::new(),
                down: false,
            },
        );
        Ok(id)
    }

    /// A supervisor started by the leaf `owner` and linked to it: when it
    /// gives up, `owner` is stopped and exits with the same reason; when
    /// `owner` stops or exits, the supervisor's children are stopped and it
    /// is forgotten (a restarted `owner` links what it starts anew).
    pub fn add_linked_supervisor(
        &self,
        owner: ChildId,
        name: &str,
        strategy: Strategy,
        intensity: Intensity,
    ) -> Result<SupId, SupervisorError> {
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;
        let parent = *inner
            .owner
            .get(&owner)
            .ok_or(SupervisorError::UnknownChild(owner))?;
        let id = SupId(inner.next_sup);
        let entry = inner
            .nodes
            .get_mut(&parent)
            .and_then(|n| n.children.iter_mut().find(|e| e.id == owner))
            .ok_or(SupervisorError::UnknownChild(owner))?;
        if matches!(entry.what, What::Sup(_)) {
            return Err(SupervisorError::NotALeaf(owner));
        }
        entry.linked.push(id);
        inner.next_sup += 1;
        inner.nodes.insert(
            id,
            Node {
                name: name.into(),
                policy: Supervisor::new(strategy, intensity),
                up: Up::Under(parent, owner),
                children: Vec::new(),
                down: false,
            },
        );
        Ok(id)
    }

    /// Adds a child that is already running (its owner started it).
    pub fn add_child(
        &self,
        sup: SupId,
        name: &str,
        restart: Restart,
        spec: Rc<dyn ChildSpec<Cx>>,
    ) -> Result<ChildId, SupervisorError> {
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;
        let node = inner
            .nodes
            .get_mut(&sup)
            .ok_or(SupervisorError::UnknownSupervisor)?;
        let child = ChildId(inner.next_child);
        inner.next_child += 1;
        node.policy.add_child(child, restart)?;
        node.children.push(Entry {
            id: child,
            name: name.into(),
            restart,
            what: What::Leaf(spec),
            running: true,
            linked: Vec::new(),
        });
        inner.owner.insert(child, sup);
        Ok(child)
    }

    /// Starts a child and adds it. A child that does not start is not
    /// added; the fault is the caller's.
    pub fn start_child(
        &self,
        sup: SupId,
        name: &str,
        restart: Restart,
        spec: Rc<dyn ChildSpec<Cx>>,
        cx: &mut Cx,
    ) -> Result<ChildId, StartError> {
        if !self.inner.borrow().nodes.contains_key(&sup) {
            return Err(StartError::Supervisor(SupervisorError::UnknownSupervisor));
        }
        spec.start(cx).map_err(StartError::Fault)?;
        match self.add_child(sup, name, restart, spec.clone()) {
            Ok(id) => Ok(id),
            Err(e) => {
                // The supervisor went away while the child started.
                spec.stop(cx, &ExitReason::Shutdown);
                Err(StartError::Supervisor(e))
            }
        }
    }

    /// Stops `child` and takes it out of the tree, with what is linked to
    /// it.
    pub fn terminate_child(&self, child: ChildId, cx: &mut Cx) -> Result<(), SupervisorError> {
        if !self.inner.borrow().owner.contains_key(&child) {
            return Err(SupervisorError::UnknownChild(child));
        }
        self.stop_one(child, &ExitReason::Shutdown, cx);
        self.forget_child(child);
        Ok(())
    }

    /// Takes `child` out of the tree without stopping anything: it is gone
    /// already (its object was destroyed). What is linked to it is
    /// forgotten too. False when the tree has no such child.
    pub fn remove_child(&self, child: ChildId) -> bool {
        self.forget_child(child)
    }

    /// Takes `sup` and everything below it out of the tree without
    /// stopping anything. False for the root and for a supervisor the tree
    /// does not have.
    pub fn remove_supervisor(&self, sup: SupId) -> bool {
        let up = {
            let inner = self.inner.borrow();
            match inner.nodes.get(&sup).map(|n| &n.up) {
                None | Some(Up::Root) => return false,
                Some(Up::Child(p, c)) => (*p, *c, true),
                Some(Up::Under(p, c)) => (*p, *c, false),
            }
        };
        match up {
            (_, child, true) => {
                self.forget_child(child);
            }
            (parent, owner, false) => {
                let mut inner = self.inner.borrow_mut();
                if let Some(e) = inner
                    .nodes
                    .get_mut(&parent)
                    .and_then(|n| n.children.iter_mut().find(|e| e.id == owner))
                {
                    e.linked.retain(|s| *s != sup);
                }
                drop(inner);
                self.forget_sup(sup);
            }
        }
        true
    }

    /// Stops `sup`'s children, the last started first, and takes it out of
    /// the tree.
    pub fn terminate_supervisor(&self, sup: SupId, cx: &mut Cx) -> bool {
        if sup == self.root() || !self.inner.borrow().nodes.contains_key(&sup) {
            return false;
        }
        self.stop_children(sup, cx);
        self.remove_supervisor(sup)
    }

    /// `child` exited by itself for `reason` at `now`: it is gone already,
    /// so it is not stopped. What follows is its supervisor's decision.
    pub fn child_exited(
        &self,
        child: ChildId,
        reason: ExitReason,
        now: Time,
        cx: &mut Cx,
    ) -> Result<Outcome, SupervisorError> {
        self.exit(
            Pending {
                child,
                reason,
                now,
                stop_first: false,
            },
            cx,
        )
    }

    /// `child` failed with `fault` but is still there (a UI subtree after
    /// a panic in one of its handlers): it is stopped with the fault as the
    /// reason, then handled as an exit.
    pub fn child_failed(
        &self,
        child: ChildId,
        fault: Fault,
        now: Time,
        cx: &mut Cx,
    ) -> Result<Outcome, SupervisorError> {
        self.exit(
            Pending {
                child,
                reason: ExitReason::Fault(fault),
                now,
                stop_first: true,
            },
            cx,
        )
    }

    /// Stops everything, the last started first and what a child started
    /// before the child. The tree keeps its shape; nothing is restarted.
    pub fn shutdown(&self, cx: &mut Cx) {
        let root = self.root();
        self.stop_children(root, cx);
    }

    // ------------------------------------------------------ introspection

    pub fn name(&self, sup: SupId) -> Option<Arc<str>> {
        self.inner.borrow().nodes.get(&sup).map(|n| n.name.clone())
    }

    /// The supervisor `sup` is a child of or linked under.
    pub fn parent(&self, sup: SupId) -> Option<SupId> {
        match self.inner.borrow().nodes.get(&sup)?.up {
            Up::Root => None,
            Up::Child(p, _) | Up::Under(p, _) => Some(p),
        }
    }

    /// The supervisor that has `child`.
    pub fn owner(&self, child: ChildId) -> Option<SupId> {
        self.inner.borrow().owner.get(&child).copied()
    }

    pub fn contains(&self, sup: SupId) -> bool {
        self.inner.borrow().nodes.contains_key(&sup)
    }

    /// Whether `sup` gave up and was not restarted.
    pub fn is_down(&self, sup: SupId) -> bool {
        self.inner.borrow().nodes.get(&sup).is_some_and(|n| n.down)
    }

    /// `sup`'s children in start order; empty for an unknown supervisor.
    pub fn children(&self, sup: SupId) -> Vec<ChildInfo> {
        let inner = self.inner.borrow();
        let Some(node) = inner.nodes.get(&sup) else {
            return Vec::new();
        };
        node.children
            .iter()
            .map(|e| ChildInfo {
                id: e.id,
                name: e.name.clone(),
                kind: match &e.what {
                    What::Leaf(s) => s.kind(),
                    What::Sup(_) => ChildKind::Supervisor,
                },
                restart: e.restart,
                running: e.running,
                supervisor: match &e.what {
                    What::Leaf(_) => None,
                    What::Sup(s) => Some(*s),
                },
            })
            .collect()
    }

    /// The supervisors linked to the leaf `child`.
    pub fn linked(&self, child: ChildId) -> Vec<SupId> {
        let inner = self.inner.borrow();
        inner
            .owner
            .get(&child)
            .and_then(|s| inner.nodes.get(s))
            .and_then(|n| n.children.iter().find(|e| e.id == child))
            .map(|e| e.linked.clone())
            .unwrap_or_default()
    }

    /// Restarts `sup` counts inside its window now.
    pub fn recent_restarts(&self, sup: SupId) -> usize {
        self.inner
            .borrow()
            .nodes
            .get(&sup)
            .map_or(0, |n| n.policy.recent_restarts())
    }

    /// Changes what `sup` allows. Restarts already counted stay counted.
    pub fn set_intensity(&self, sup: SupId, intensity: Intensity) -> bool {
        match self.inner.borrow_mut().nodes.get_mut(&sup) {
            Some(n) => {
                n.policy.set_intensity(intensity);
                true
            }
            None => false,
        }
    }

    /// How many supervisors the tree has, the root included.
    pub fn supervisor_count(&self) -> usize {
        self.inner.borrow().nodes.len()
    }

    /// The tree as indented text, for logs and the dev tools.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_sup(self.root(), 0, &mut out);
        out
    }

    fn dump_sup(&self, sup: SupId, depth: usize, out: &mut String) {
        let (line, kids): (String, Vec<(ChildInfo, Vec<SupId>)>) = {
            let inner = self.inner.borrow();
            let Some(n) = inner.nodes.get(&sup) else {
                return;
            };
            let i = n.policy.intensity();
            let line = format!(
                "{}{} ({:?}, {} / {:?}{})",
                "  ".repeat(depth),
                n.name,
                n.policy.strategy(),
                i.max_restarts,
                i.within,
                if n.down { ", down" } else { "" }
            );
            drop(inner);
            let kids = self
                .children(sup)
                .into_iter()
                .map(|c| {
                    let linked = self.linked(c.id);
                    (c, linked)
                })
                .collect();
            (line, kids)
        };
        let _ = writeln!(out, "{line}");
        for (c, linked) in kids {
            match c.supervisor {
                Some(s) => self.dump_sup(s, depth + 1, out),
                None => {
                    let _ = writeln!(
                        out,
                        "{}{} [{:?}, {:?}{}]",
                        "  ".repeat(depth + 1),
                        c.name,
                        c.kind,
                        c.restart,
                        if c.running { "" } else { ", stopped" }
                    );
                    for s in linked {
                        self.dump_sup(s, depth + 2, out);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ the work

    fn exit(&self, first: Pending, cx: &mut Cx) -> Result<Outcome, SupervisorError> {
        {
            let mut inner = self.inner.borrow_mut();
            if !inner.owner.contains_key(&first.child) {
                return Err(SupervisorError::UnknownChild(first.child));
            }
            if inner.busy {
                inner.queue.push_back(first);
                return Ok(Outcome::Queued);
            }
            inner.busy = true;
        }
        // Not left set if a start or stop panics: the tree must stay
        // usable for the fault that panic becomes.
        struct Busy<'a, Cx>(&'a SupervisorTree<Cx>);
        impl<Cx> Drop for Busy<'_, Cx> {
            fn drop(&mut self) {
                if let Ok(mut inner) = self.0.inner.try_borrow_mut() {
                    inner.busy = false;
                }
            }
        }
        let _busy = Busy(self);

        let mut outcome = self.handle(first, cx);
        loop {
            let next = self.inner.borrow_mut().queue.pop_front();
            let Some(next) = next else { break };
            // A queued exit of a child that was stopped meanwhile is moot.
            if !self.is_running(next.child) {
                continue;
            }
            let o = self.handle(next, cx);
            if matches!(o, Outcome::GaveUp { .. }) {
                outcome = o;
            }
        }
        Ok(outcome)
    }

    fn is_running(&self, child: ChildId) -> bool {
        let inner = self.inner.borrow();
        inner
            .owner
            .get(&child)
            .and_then(|s| inner.nodes.get(s))
            .and_then(|n| n.children.iter().find(|e| e.id == child))
            .is_some_and(|e| e.running)
    }

    fn handle(&self, exit: Pending, cx: &mut Cx) -> Outcome {
        let Pending {
            mut child,
            mut reason,
            now,
            stop_first: mut stop,
        } = exit;
        let mut started = Vec::new();
        loop {
            let Some(sup) = self.owner(child) else {
                // Taken out of the tree by one of the stops below.
                return Outcome::Stopped { child };
            };
            if stop {
                self.stop_one(child, &reason, cx);
            } else {
                // Gone already; what it started goes with it.
                self.stop_linked(child, cx);
                self.set_running(child, false);
            }
            let directive = {
                let mut inner = self.inner.borrow_mut();
                let Some(node) = inner.nodes.get_mut(&sup) else {
                    return Outcome::Stopped { child };
                };
                match node.policy.on_exit(child, &reason, now) {
                    Ok(d) => d,
                    Err(_) => return Outcome::Stopped { child },
                }
            };
            match directive {
                Directive::NoRestart => {
                    self.sync(sup);
                    return Outcome::Stopped { child };
                }
                Directive::Restart { terminate, start } => {
                    for t in terminate {
                        self.stop_one(t, &ExitReason::Shutdown, cx);
                    }
                    self.sync(sup);
                    let mut failed = None;
                    for s in start {
                        match self.start_one(s, cx) {
                            Ok(()) => started.push(s),
                            Err(fault) => {
                                failed = Some((s, fault));
                                break;
                            }
                        }
                    }
                    match failed {
                        None => return Outcome::Restarted { by: sup, started },
                        Some((s, fault)) => {
                            // One more exit of that child, so the intensity
                            // bounds how often a child that cannot start is
                            // tried.
                            started.retain(|c| *c != s);
                            child = s;
                            reason = ExitReason::Fault(fault);
                            stop = false;
                        }
                    }
                }
                Directive::Escalate => {
                    self.stop_children(sup, cx);
                    let up = {
                        let mut inner = self.inner.borrow_mut();
                        let Some(node) = inner.nodes.get_mut(&sup) else {
                            return Outcome::Stopped { child };
                        };
                        node.down = true;
                        match node.up {
                            Up::Root => None,
                            Up::Child(_, c) => Some((c, false)),
                            Up::Under(_, c) => Some((c, true)),
                        }
                    };
                    match up {
                        None => return Outcome::GaveUp { sup, reason },
                        Some((c, linked)) => {
                            child = c;
                            // A leaf it is linked to is still running and
                            // goes down with it; a supervisor that is a
                            // child has just stopped.
                            stop = linked;
                        }
                    }
                }
            }
        }
    }

    fn set_running(&self, child: ChildId, running: bool) {
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;
        if let Some(e) = inner
            .owner
            .get(&child)
            .and_then(|s| inner.nodes.get_mut(s))
            .and_then(|n| n.children.iter_mut().find(|e| e.id == child))
        {
            e.running = running;
        }
    }

    /// The entry's spec and running state, without holding the borrow.
    fn entry(&self, child: ChildId) -> Option<(What<Cx>, bool)> {
        let inner = self.inner.borrow();
        inner
            .owner
            .get(&child)
            .and_then(|s| inner.nodes.get(s))
            .and_then(|n| n.children.iter().find(|e| e.id == child))
            .map(|e| (e.what.clone(), e.running))
    }

    /// Stops what is linked to the leaf `child` and forgets it.
    fn stop_linked(&self, child: ChildId, cx: &mut Cx) {
        loop {
            // One at a time: stopping one may unlink others.
            let next = {
                let mut inner = self.inner.borrow_mut();
                let inner = &mut *inner;
                inner
                    .owner
                    .get(&child)
                    .and_then(|s| inner.nodes.get_mut(s))
                    .and_then(|n| n.children.iter_mut().find(|e| e.id == child))
                    .and_then(|e| e.linked.pop())
            };
            let Some(sup) = next else { break };
            self.stop_children(sup, cx);
            self.forget_sup(sup);
        }
    }

    /// Stops one child if it runs: what it started first, then itself.
    fn stop_one(&self, child: ChildId, reason: &ExitReason, cx: &mut Cx) {
        let Some((what, running)) = self.entry(child) else {
            return;
        };
        match what {
            What::Leaf(spec) => {
                self.stop_linked(child, cx);
                if running {
                    self.set_running(child, false);
                    spec.stop(cx, reason);
                }
            }
            What::Sup(sup) => {
                self.stop_children(sup, cx);
                self.set_running(child, false);
            }
        }
    }

    /// Stops `sup`'s children, the last started first.
    fn stop_children(&self, sup: SupId, cx: &mut Cx) {
        let ids: Vec<ChildId> = {
            let inner = self.inner.borrow();
            match inner.nodes.get(&sup) {
                Some(n) => n.children.iter().rev().map(|e| e.id).collect(),
                None => return,
            }
        };
        for c in ids {
            self.stop_one(c, &ExitReason::Shutdown, cx);
        }
    }

    fn start_one(&self, child: ChildId, cx: &mut Cx) -> Result<(), Fault> {
        let Some((what, running)) = self.entry(child) else {
            return Ok(());
        };
        if running {
            return Ok(());
        }
        match what {
            What::Leaf(spec) => {
                // Marked before it starts, so that what it links while
                // starting finds a running owner.
                self.set_running(child, true);
                if let Err(fault) = spec.start(cx) {
                    self.stop_linked(child, cx);
                    self.set_running(child, false);
                    return Err(fault);
                }
                Ok(())
            }
            What::Sup(sup) => {
                let ids: Vec<ChildId> = {
                    let mut inner = self.inner.borrow_mut();
                    let Some(n) = inner.nodes.get_mut(&sup) else {
                        return Ok(());
                    };
                    // A restarted supervisor starts with a clean record.
                    n.policy.reset();
                    n.down = false;
                    n.children.iter().map(|e| e.id).collect()
                };
                self.set_running(child, true);
                for c in ids {
                    if let Err(fault) = self.start_one(c, cx) {
                        self.stop_children(sup, cx);
                        self.set_running(child, false);
                        return Err(fault);
                    }
                }
                Ok(())
            }
        }
    }

    /// Drops the entries the policy dropped (temporary children that
    /// exited or were stopped), with what hangs below them.
    fn sync(&self, sup: SupId) {
        let gone: Vec<ChildId> = {
            let inner = self.inner.borrow();
            let Some(n) = inner.nodes.get(&sup) else {
                return;
            };
            let kept: Vec<ChildId> = n.policy.children().collect();
            n.children
                .iter()
                .map(|e| e.id)
                .filter(|c| !kept.contains(c))
                .collect()
        };
        for c in gone {
            self.forget_child(c);
        }
    }

    fn forget_child(&self, child: ChildId) -> bool {
        let below: Vec<SupId> = {
            let mut inner = self.inner.borrow_mut();
            let inner = &mut *inner;
            let Some(sup) = inner.owner.remove(&child) else {
                return false;
            };
            let Some(node) = inner.nodes.get_mut(&sup) else {
                return false;
            };
            node.policy.remove_child(child);
            let Some(i) = node.children.iter().position(|e| e.id == child) else {
                return false;
            };
            let e = node.children.remove(i);
            match e.what {
                What::Leaf(_) => e.linked,
                What::Sup(s) => vec![s],
            }
        };
        for s in below {
            self.forget_sup(s);
        }
        true
    }

    fn forget_sup(&self, sup: SupId) {
        let mut below = Vec::new();
        {
            let mut inner = self.inner.borrow_mut();
            let Some(node) = inner.nodes.remove(&sup) else {
                return;
            };
            for e in node.children {
                inner.owner.remove(&e.id);
                match e.what {
                    What::Leaf(_) => below.extend(e.linked),
                    What::Sup(s) => below.push(s),
                }
            }
        }
        for s in below {
            self.forget_sup(s);
        }
    }
}

/// Why [`SupervisorTree::start_child`] added nothing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    #[error("the child did not start: {0}")]
    Fault(Fault),
    #[error(transparent)]
    Supervisor(SupervisorError),
}
