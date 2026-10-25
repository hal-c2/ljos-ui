//! Which bindings and handlers cost the most, for `app_frames`' `bindings`
//! (ARCHITECTURE.md §19.4): per binding or handler where it is written,
//! the property it writes, its runs and the time they took, and which
//! change set each run off; and the binding loops the graph broke.
//!
//! A run is timed where it runs, as [`crate::timing`] does, but by
//! binding: its own time is its time less that of the runs nested in it
//! (a binding that reads another runs it). What set a binding off is
//! found as a trace finds it: the values of what it read at its last run
//! are kept, and a run names the first that has changed. While nothing
//! profiles a run pays one look at a flag. The binding loops are kept
//! whether or not anything profiles: the graph reports one rarely, and
//! one met while the application starts is the one worth seeing.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ljos_reactive::NodeId;

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::value::Value;

/// The most bindings and handlers told apart; others are counted.
const MAX_ENTRIES: usize = 10_000;
/// The runs kept for a trace file: the latest give way.
pub const MAX_RUNS: usize = 20_000;
/// The binding loops told apart.
const MAX_LOOPS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Binding(ObjectId, PropIndex),
    /// `expr` tells apart the handlers of one object for one sender.
    Handler {
        receiver: ObjectId,
        sender: ObjectId,
        expr: u32,
    },
}

/// A binding or a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotKind {
    Binding,
    Handler,
}

/// What set runs of a binding or handler off: for a binding a property
/// it read whose value changed, for a handler the signal.
#[derive(Clone, Debug, PartialEq)]
pub struct HotCause {
    pub object: ObjectId,
    pub type_name: String,
    /// The property, or the signal for a handler.
    pub name: String,
    pub runs: u64,
}

/// A binding or handler that ran while profiled.
#[derive(Clone, Debug, PartialEq)]
pub struct HotEntry {
    pub kind: HotKind,
    /// The object it is written on.
    pub object: ObjectId,
    pub type_name: String,
    /// The property it writes, or the handler's name (`onClicked`).
    pub name: String,
    /// Where it is written (`file.qml:4:9`), when in a file.
    pub at: Option<String>,
    pub runs: u64,
    /// From each run's start to its end.
    pub total: Duration,
    /// Less the runs nested in its runs.
    pub own: Duration,
    pub longest: Duration,
    /// Most runs first; runs whose cause is not known (a first run, a
    /// read that was not a property) are in none.
    pub causes: Vec<HotCause>,
}

/// One run, for a trace file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotRun {
    /// Its index in [`Profile::entries`].
    pub entry: usize,
    pub start: Instant,
    pub duration: Duration,
}

/// What was profiled since profiling started.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// When profiling started; `None` while it is off.
    pub since: Option<Instant>,
    /// In the order they first ran.
    pub entries: Vec<HotEntry>,
    /// The latest [`MAX_RUNS`] runs, by when they ended.
    pub runs: Vec<HotRun>,
    pub dropped_runs: u64,
    /// Bindings and handlers past [`MAX_ENTRIES`], not told apart.
    pub dropped_entries: u64,
}

/// A binding loop the graph broke: the binding whose change came back to
/// it, and the way it came back.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingLoopSeen {
    pub object: ObjectId,
    pub type_name: String,
    pub property: String,
    pub at: Option<String>,
    /// From the binding on, each entry read by the one before it; the last
    /// reads the first. Members that are no property (a handler) are left
    /// out.
    pub cycle: Vec<(ObjectId, String, String)>,
    pub times: u64,
}

struct Row {
    key: Key,
    type_name: String,
    name: String,
    at: Option<String>,
    /// A handler's signal, what sets each of its runs off.
    signal: Option<String>,
    runs: u64,
    total: Duration,
    own: Duration,
    longest: Duration,
    causes: HashMap<(ObjectId, u32), u64>,
}

/// How a binding or handler is named, at its first run.
struct Described {
    type_name: String,
    name: String,
    at: Option<String>,
    signal: Option<String>,
}

struct Open {
    row: u32,
    started: Instant,
    nested: Duration,
}

#[derive(Default)]
struct State {
    generation: u64,
    since: Option<Instant>,
    index: HashMap<Key, u32>,
    rows: Vec<Row>,
    open: Vec<Open>,
    runs: VecDeque<(u32, Instant, Duration)>,
    dropped_runs: u64,
    dropped_entries: u64,
    reads: HashMap<(ObjectId, PropIndex), Vec<(NodeId, Value)>>,
}

struct Loop {
    object: ObjectId,
    prop: PropIndex,
    cycle: Vec<(ObjectId, PropIndex)>,
    times: u64,
}

#[derive(Default)]
pub(crate) struct Profiler {
    on: Cell<bool>,
    state: RefCell<State>,
    loops: RefCell<Vec<Loop>>,
}

/// One run being profiled; it ends when dropped.
pub(crate) struct ProfiledRun<'a> {
    profiler: &'a Profiler,
    /// The [`State::generation`] it started in: one that profiling was
    /// turned off or on since ends unseen.
    generation: u64,
}

impl Drop for ProfiledRun<'_> {
    fn drop(&mut self) {
        let mut s = self.profiler.state.borrow_mut();
        if s.generation != self.generation {
            return;
        }
        let Some(open) = s.open.pop() else {
            return;
        };
        let took = open.started.elapsed();
        if let Some(parent) = s.open.last_mut() {
            parent.nested += took;
        }
        let row = &mut s.rows[open.row as usize];
        row.runs += 1;
        row.total += took;
        row.own += took.saturating_sub(open.nested);
        row.longest = row.longest.max(took);
        if s.runs.len() == MAX_RUNS {
            s.runs.pop_front();
            s.dropped_runs += 1;
        }
        s.runs.push_back((open.row, open.started, took));
    }
}

impl Profiler {
    fn start(&self, key: Key, describe: impl FnOnce() -> Described) -> Option<ProfiledRun<'_>> {
        let mut s = self.state.borrow_mut();
        let row = match s.index.get(&key) {
            Some(&row) => row,
            None if s.rows.len() >= MAX_ENTRIES => {
                s.dropped_entries += 1;
                return None;
            }
            None => {
                // Named once, at its first run: a name costs a look at the
                // object and its file.
                drop(s);
                let d = describe();
                s = self.state.borrow_mut();
                let row = s.rows.len() as u32;
                s.rows.push(Row {
                    key,
                    type_name: d.type_name,
                    name: d.name,
                    at: d.at,
                    signal: d.signal,
                    runs: 0,
                    total: Duration::ZERO,
                    own: Duration::ZERO,
                    longest: Duration::ZERO,
                    causes: HashMap::new(),
                });
                s.index.insert(key, row);
                row
            }
        };
        s.open.push(Open {
            row,
            started: Instant::now(),
            nested: Duration::ZERO,
        });
        Some(ProfiledRun {
            profiler: self,
            generation: s.generation,
        })
    }

    fn caused(&self, key: Key, cause: (ObjectId, u32)) {
        let mut s = self.state.borrow_mut();
        if let Some(&row) = s.index.get(&key) {
            *s.rows[row as usize].causes.entry(cause).or_default() += 1;
        }
    }
}

impl Engine {
    /// Profiles bindings and handlers until turned off, for
    /// [`Engine::profile`]. Off by default; turning it on starts afresh.
    pub fn set_profiled(&self, on: bool) {
        let p = &self.inner.profile;
        if on == p.on.get() {
            return;
        }
        p.on.set(on);
        let mut s = p.state.borrow_mut();
        // A run that is open now ends unseen.
        *s = State {
            generation: s.generation + 1,
            since: on.then(Instant::now),
            ..State::default()
        };
    }

    pub fn is_profiled(&self) -> bool {
        self.inner.profile.on.get()
    }

    /// The start of a run of the binding of `prop` of `obj`, profiled if
    /// profiling is on.
    pub(crate) fn profile_binding(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Option<ProfiledRun<'_>> {
        let p = &self.inner.profile;
        if !p.on.get() {
            return None;
        }
        let key = Key::Binding(obj, prop);
        let run = p.start(key, || {
            let (ty, name) = self.describe(obj, prop);
            Described {
                type_name: ty.to_string(),
                name: name.to_string(),
                at: self.binding_place(obj, prop),
                signal: None,
            }
        })?;
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.node(prop));
        if let Some(node) = node {
            if let Some(cause) = self.changed_read(obj, prop, node) {
                p.caused(key, (cause.0, cause.1.0));
            }
            // What it reads is known once it has run.
            let weak = Rc::downgrade(&self.inner);
            self.inner.graph.after_evaluation(move || {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let engine = Engine::from_inner(inner);
                if !engine.is_profiled() {
                    return;
                }
                let reads = engine.reads_now(node);
                engine
                    .inner
                    .profile
                    .state
                    .borrow_mut()
                    .reads
                    .insert((obj, prop), reads);
            });
        }
        Some(run)
    }

    /// What the binding of `prop` of `obj`, whose node is `node`, read at
    /// its last run whose value has changed since: the first, as a
    /// property. A read the graph gave a node since (a property written
    /// the first time) has changed too.
    fn changed_read(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        node: NodeId,
    ) -> Option<(ObjectId, PropIndex)> {
        let s = self.inner.profile.state.borrow();
        let kept = s.reads.get(&(obj, prop))?;
        let g = &self.inner.graph;
        let changed =
            g.sources(node)
                .into_iter()
                .find(|n| match kept.iter().find(|(k, _)| k == n) {
                    Some((_, was)) => g.peek(*n).is_none_or(|now| now != *was),
                    None => true,
                })?;
        drop(s);
        let k = self.inner.store.borrow().node_keys.get(changed).copied()?;
        Some((k.object, k.prop))
    }

    /// The start of a run of the handler `expr` written on `receiver` for
    /// `signal` of `sender`, profiled if profiling is on. `at` says where
    /// it is written.
    pub(crate) fn profile_handler(
        &self,
        receiver: ObjectId,
        sender: ObjectId,
        expr: u32,
        signal: &str,
        at: impl FnOnce() -> Option<String>,
    ) -> Option<ProfiledRun<'_>> {
        let p = &self.inner.profile;
        if !p.on.get() {
            return None;
        }
        let key = Key::Handler {
            receiver,
            sender,
            expr,
        };
        let run = p.start(key, || Described {
            type_name: self
                .meta_of(receiver)
                .map_or_else(|| "?".to_owned(), |m| m.name.to_string()),
            name: crate::qtqml::handler_name(signal),
            at: at(),
            signal: Some(signal.to_owned()),
        })?;
        p.caused(key, (sender, 0));
        Some(run)
    }

    /// What was profiled since profiling started.
    pub fn profile(&self) -> Profile {
        let s = self.inner.profile.state.borrow();
        let entries = s
            .rows
            .iter()
            .map(|r| {
                let (kind, object) = match r.key {
                    Key::Binding(o, _) => (HotKind::Binding, o),
                    Key::Handler { receiver, .. } => (HotKind::Handler, receiver),
                };
                let mut causes: Vec<HotCause> = r
                    .causes
                    .iter()
                    .map(|(&(o, i), &runs)| {
                        let (type_name, name) = match r.key {
                            Key::Binding(..) => {
                                let (t, n) = self.describe(o, PropIndex(i));
                                (t.to_string(), n.to_string())
                            }
                            Key::Handler { .. } => (
                                self.meta_of(o)
                                    .map_or_else(|| "?".to_owned(), |m| m.name.to_string()),
                                r.signal.clone().unwrap_or_default(),
                            ),
                        };
                        HotCause {
                            object: o,
                            type_name,
                            name,
                            runs,
                        }
                    })
                    .collect();
                causes.sort_by(|a, b| b.runs.cmp(&a.runs).then_with(|| a.name.cmp(&b.name)));
                HotEntry {
                    kind,
                    object,
                    type_name: r.type_name.clone(),
                    name: r.name.clone(),
                    at: r.at.clone(),
                    runs: r.runs,
                    total: r.total,
                    own: r.own,
                    longest: r.longest,
                    causes,
                }
            })
            .collect();
        Profile {
            since: s.since,
            entries,
            runs: s
                .runs
                .iter()
                .map(|&(entry, start, duration)| HotRun {
                    entry: entry as usize,
                    start,
                    duration,
                })
                .collect(),
            dropped_runs: s.dropped_runs,
            dropped_entries: s.dropped_entries,
        }
    }

    /// The graph broke a binding loop: `cycle` are the nodes it went
    /// round, from `node` on.
    pub(crate) fn loop_seen(&self, node: NodeId, cycle: &[NodeId]) {
        let (key, cycle) = {
            let store = self.inner.store.borrow();
            let Some(key) = store.node_keys.get(node).copied() else {
                return;
            };
            let cycle: Vec<(ObjectId, PropIndex)> = cycle
                .iter()
                .filter_map(|n| store.node_keys.get(*n).map(|k| (k.object, k.prop)))
                .collect();
            (key, cycle)
        };
        let mut loops = self.inner.profile.loops.borrow_mut();
        if let Some(l) = loops
            .iter_mut()
            .find(|l| l.object == key.object && l.prop == key.prop)
        {
            l.times += 1;
            l.cycle = cycle;
            return;
        }
        if loops.len() < MAX_LOOPS {
            loops.push(Loop {
                object: key.object,
                prop: key.prop,
                cycle,
                times: 1,
            });
        }
    }

    /// The binding loops the graph broke since the engine started, the
    /// first [`MAX_LOOPS`] told apart.
    pub fn binding_loops(&self) -> Vec<BindingLoopSeen> {
        let loops = self.inner.profile.loops.borrow();
        loops
            .iter()
            .map(|l| {
                let (ty, prop) = self.describe(l.object, l.prop);
                BindingLoopSeen {
                    object: l.object,
                    type_name: ty.to_string(),
                    property: prop.to_string(),
                    at: self.binding_place(l.object, l.prop),
                    cycle: l
                        .cycle
                        .iter()
                        .map(|&(o, p)| {
                            let (t, n) = self.describe(o, p);
                            (o, t.to_string(), n.to_string())
                        })
                        .collect(),
                    times: l.times,
                }
            })
            .collect()
    }
}
