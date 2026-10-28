//! What a turn of the loop did, in order, for `app_frames`' `wake`
//! (ARCHITECTURE.md §19.4): the handlers that ran, the properties that
//! changed with their old and new values, the bindings that ran again,
//! and what the scene did with it (items laid out, moved or sized,
//! redrawn), with the time each run took. It answers why one click took
//! long, or why it redrew everything.
//!
//! The changes are the graph's ([`Graph::set_journaled`]): every value
//! that changed, in the order it changed, wherever the write came from. A
//! run is noted where it starts, with where the graph's changes stood, and
//! timed where it ends; the changes between its start and end are inside
//! it. A take puts the two together. While nothing journals, a run pays
//! one look at a flag.
//!
//! [`Graph::set_journaled`]: ljos_reactive::Graph::set_journaled

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};
use crate::value::Value;

/// The most runs and scene steps kept between two takes; the later are
/// counted.
pub const MAX_JOURNAL_RUNS: usize = 2_000;

/// What a journal entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalKind {
    /// A signal handler ran (`onClicked`).
    Handler,
    /// A binding ran again.
    Binding,
    /// A property's value changed.
    Changed,
    /// The scene laid an item out: a positioner, a layout, a text.
    LaidOut,
    /// The scene moved or sized an item.
    Placed,
    /// The scene rebuilt what an item draws.
    Redrawn,
    /// Host code ran: a message from another thread, a closure given to
    /// [`Engine::turn`]. `type_name` is the mailbox (`host` for a
    /// closure), `name` the message's kind.
    Host,
}

impl JournalKind {
    pub fn name(self) -> &'static str {
        match self {
            JournalKind::Handler => "handler",
            JournalKind::Binding => "binding",
            JournalKind::Changed => "changed",
            JournalKind::LaidOut => "laid out",
            JournalKind::Placed => "placed",
            JournalKind::Redrawn => "redrawn",
            JournalKind::Host => "host",
        }
    }
}

/// One thing a turn did.
#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub kind: JournalKind,
    /// How many handlers and bindings it ran inside.
    pub depth: u32,
    pub object: ObjectId,
    pub type_name: String,
    /// The handler (`onClicked`) or the property; empty for a scene step.
    pub name: String,
    /// Where the handler or binding is written (`file.qml:4:9`).
    pub at: Option<String>,
    /// A change's value before, `None` for one that had none.
    pub old: Option<Value>,
    /// A change's value after.
    pub new: Option<Value>,
    /// When a run or step started; a change has none.
    pub started: Option<Instant>,
    /// How long a run or step took, the runs nested in it included.
    pub took: Option<Duration>,
}

/// What a turn did, in the order each thing started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Journal {
    pub entries: Vec<JournalEntry>,
    /// Runs past [`MAX_JOURNAL_RUNS`] and changes past the graph's limit,
    /// not kept.
    pub dropped: u64,
    /// Changes of objects destroyed before the take, which cannot be named.
    pub gone: u64,
}

struct Run {
    kind: JournalKind,
    object: ObjectId,
    type_name: String,
    name: String,
    at: Option<String>,
    depth: u32,
    started: Instant,
    took: Option<Duration>,
    /// Where the graph's changes stood at its start and at its end.
    from: usize,
    to: usize,
}

#[derive(Default)]
struct State {
    generation: u64,
    runs: Vec<Run>,
    /// The runs not ended yet, innermost last.
    open: Vec<usize>,
    dropped: u64,
}

#[derive(Default)]
pub(crate) struct JournalState {
    on: Cell<bool>,
    state: RefCell<State>,
}

/// One run being journaled; it ends when dropped.
pub(crate) struct JournalRun<'a> {
    engine: &'a Engine,
    generation: u64,
    index: usize,
}

impl Drop for JournalRun<'_> {
    fn drop(&mut self) {
        let to = self.engine.inner.graph.journal_len();
        let mut s = self.engine.inner.journal.state.borrow_mut();
        if s.generation != self.generation {
            return;
        }
        if let Some(at) = s.open.iter().rposition(|&i| i == self.index) {
            s.open.truncate(at);
        }
        if let Some(r) = s.runs.get_mut(self.index) {
            r.took = Some(r.started.elapsed());
            r.to = to;
        }
    }
}

impl Engine {
    /// Journals what each turn does until turned off, for
    /// [`Engine::take_journal`]. Off by default; turning it on or off
    /// starts afresh.
    pub fn set_journaled(&self, on: bool) {
        let j = &self.inner.journal;
        if on == j.on.get() {
            return;
        }
        j.on.set(on);
        self.inner.graph.set_journaled(on);
        let mut s = j.state.borrow_mut();
        *s = State {
            generation: s.generation + 1,
            ..State::default()
        };
    }

    pub fn is_journaled(&self) -> bool {
        self.inner.journal.on.get()
    }

    fn journal_start(
        &self,
        kind: JournalKind,
        object: ObjectId,
        describe: impl FnOnce() -> (String, String, Option<String>),
        ended: bool,
    ) -> Option<usize> {
        let from = self.inner.graph.journal_len();
        let j = &self.inner.journal;
        {
            let mut s = j.state.borrow_mut();
            if s.runs.len() >= MAX_JOURNAL_RUNS {
                s.dropped += 1;
                return None;
            }
        }
        // Named as it starts: the object may be gone at the take.
        let (type_name, name, at) = describe();
        let mut s = j.state.borrow_mut();
        let depth = s.open.len() as u32;
        let index = s.runs.len();
        s.runs.push(Run {
            kind,
            object,
            type_name,
            name,
            at,
            depth,
            started: Instant::now(),
            took: None,
            from,
            to: from,
        });
        if !ended {
            s.open.push(index);
        }
        Some(index)
    }

    fn journal_run(&self, index: Option<usize>) -> Option<JournalRun<'_>> {
        Some(JournalRun {
            engine: self,
            generation: self.inner.journal.state.borrow().generation,
            index: index?,
        })
    }

    /// The start of a run of the binding of `prop` of `obj`, journaled if
    /// journaling is on.
    pub(crate) fn journal_binding(&self, obj: ObjectId, prop: PropIndex) -> Option<JournalRun<'_>> {
        if !self.inner.journal.on.get() {
            return None;
        }
        let index = self.journal_start(
            JournalKind::Binding,
            obj,
            || {
                let (ty, name) = self.describe(obj, prop);
                (
                    ty.to_string(),
                    name.to_string(),
                    self.binding_place(obj, prop),
                )
            },
            false,
        );
        self.journal_run(index)
    }

    /// The start of a run of a handler written on `receiver` for `signal`,
    /// journaled if journaling is on. `at` says where it is written.
    pub(crate) fn journal_handler(
        &self,
        receiver: ObjectId,
        signal: &str,
        at: impl FnOnce() -> Option<String>,
    ) -> Option<JournalRun<'_>> {
        if !self.inner.journal.on.get() {
            return None;
        }
        let index = self.journal_start(
            JournalKind::Handler,
            receiver,
            || {
                let ty = self
                    .meta_of(receiver)
                    .map_or_else(|| "?".to_owned(), |m| m.name.to_string());
                (ty, crate::qtqml::handler_name(signal), at())
            },
            false,
        );
        self.journal_run(index)
    }

    /// The start of a turn of host code, journaled if journaling is on.
    /// It belongs to no object.
    pub(crate) fn journal_host_run(&self, ty: &str, name: &str) -> Option<JournalRun<'_>> {
        if !self.inner.journal.on.get() {
            return None;
        }
        let index = self.journal_start(
            JournalKind::Host,
            ObjectId::default(),
            || (ty.to_owned(), name.to_owned(), None),
            false,
        );
        self.journal_run(index)
    }

    /// The scene did `kind` to `obj`, in `took`; nothing while journaling
    /// is off. For the scene's steps, which run outside the engine.
    pub fn journal_step(&self, kind: JournalKind, obj: ObjectId, took: Option<Duration>) {
        if !self.inner.journal.on.get() {
            return;
        }
        let index = self.journal_start(
            kind,
            obj,
            || {
                let ty = self
                    .meta_of(obj)
                    .map_or_else(|| "?".to_owned(), |m| m.name.to_string());
                (ty, String::new(), None)
            },
            true,
        );
        if let Some(i) = index
            && let Some(r) = self.inner.journal.state.borrow_mut().runs.get_mut(i)
        {
            r.took = took;
            if let Some(t) = took {
                r.started = Instant::now().checked_sub(t).unwrap_or(r.started);
            }
        }
    }

    /// What was journaled since the last take, in the order each thing
    /// started; a change comes inside the runs it was made in.
    pub fn take_journal(&self) -> Journal {
        if !self.inner.journal.on.get() {
            return Journal::default();
        }
        let (changes, dropped_changes) = self.inner.graph.take_journal();
        let (runs, dropped) = {
            let mut s = self.inner.journal.state.borrow_mut();
            // What is still running now ends in the next take, if at all:
            // its place among the changes is gone with them.
            s.generation += 1;
            s.open.clear();
            (std::mem::take(&mut s.runs), std::mem::take(&mut s.dropped))
        };
        let mut journal = Journal {
            entries: Vec::with_capacity(runs.len() + changes.len()),
            dropped: dropped + dropped_changes,
            gone: 0,
        };
        // Runs nest: those that hold the change being placed are a stack.
        let mut holding: Vec<usize> = Vec::new();
        let mut runs = runs.into_iter().peekable();
        let mut changes = changes.into_iter().enumerate().peekable();
        loop {
            let run_first = match (runs.peek(), changes.peek()) {
                (Some(r), Some((i, _))) => r.from <= *i,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if run_first {
                let Some(r) = runs.next() else { break };
                holding.truncate(r.depth as usize);
                holding.push(r.to);
                journal.entries.push(JournalEntry {
                    kind: r.kind,
                    depth: r.depth,
                    object: r.object,
                    type_name: r.type_name,
                    name: r.name,
                    at: r.at,
                    old: None,
                    new: None,
                    started: Some(r.started),
                    took: r.took,
                });
                continue;
            }
            let Some((i, c)) = changes.next() else { break };
            while holding.last().is_some_and(|&to| to <= i) {
                holding.pop();
            }
            let key = self.inner.store.borrow().node_keys.get(c.node).copied();
            let Some(key) = key else {
                journal.gone += 1;
                continue;
            };
            let (ty, name) = self.describe(key.object, key.prop);
            journal.entries.push(JournalEntry {
                kind: JournalKind::Changed,
                depth: holding.len() as u32,
                object: key.object,
                type_name: ty.to_string(),
                name: name.to_string(),
                at: None,
                old: c.old,
                new: Some(c.new),
                started: None,
                took: None,
            });
        }
        journal
    }
}
