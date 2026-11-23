//! Faults reach one place (ARCHITECTURE.md §9.3).
//!
//! A JS exception, a JS run cut short, the JS heap running out, a Rust
//! panic in a native type and a component that could not be made are all
//! [`Fault`]s, and all of them come through [`Engine::report_fault`] (or
//! its JS twin, which also maps the stack to QML lines). There a fault is
//! logged, kept, and queued for whoever is responsible for the object it
//! happened in: the nearest UI boundary around it (boundary.rs), or the
//! application when there is none.
//!
//! Nothing is destroyed where a fault is caught. The code that failed may
//! be anywhere on the stack (a handler of the object a boundary is about
//! to discard), so the queue is delivered between turns:
//! [`Engine::deliver_faults`], called by the scene at the start of a frame
//! and by a host without one after each turn.
//!
//! # What a panic leaves behind
//!
//! A panic is caught at the turn boundary: around a native method
//! ([`Engine::invoke`]), around each signal handler, around a completion
//! hook, around a JS entry, and around whatever a host runs in
//! [`Engine::turn`]. Unwinding releases every `RefCell` borrow and the
//! guards the engine holds (tracking depth, write batches), and the
//! reactive graph is left consistent: a binding that panicked has no value
//! and is dirty. What the handler wrote before it panicked stays written,
//! the rest of it never ran, and an object being made may be half
//! initialised. The later handlers of the same signal still run.
//!
//! That is why a panic is a fault of a boundary and not only a log line:
//! the boundary discards the whole subtree the panic happened in and makes
//! it again from its component, so nothing half done is left reachable.
//! With no boundary around the object nothing is destroyed: the
//! application is told ([`Engine::application_fault`]) and decides.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use ljos_js::JsFault;
use ljos_runtime::{Fault, JsErrorInfo, Time};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;

/// Reports kept for [`Engine::faults`].
const MAX_HISTORY: usize = 32;
/// Faults delivered in one go. A boundary whose restart faults again is
/// bounded by its intensity; this bounds what nothing else does.
const MAX_DELIVERED: usize = 256;

/// One step of how a JS exception got where it was thrown, in QML terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    /// The QML function; `None` for the body of a handler or a binding.
    pub function: Option<String>,
    /// The QML file's URL.
    pub file: String,
    /// 1-based.
    pub line: u32,
}

/// A fault and where it happened.
#[derive(Clone, Debug)]
pub struct FaultReport {
    pub fault: Fault,
    /// The object whose code was running: the object a handler or a
    /// binding belongs to, the receiver of a native method.
    pub origin: Option<ObjectId>,
    /// What `origin` was called when the fault happened
    /// ([`Engine::label_of`]): a boundary may destroy it for the fault.
    pub origin_name: Option<String>,
    /// The QML file the fault belongs to: where a JS exception was thrown,
    /// else where `origin` is declared.
    pub file: Option<String>,
    /// 1-based; 0 when unknown.
    pub line: u32,
    /// For a JS exception: the QML functions it passed through, the one
    /// that threw first.
    pub stack: Vec<StackFrame>,
    /// On the engine's clock.
    pub at: Time,
    /// Counts the faults reported, from 1: the history keeps the last 32,
    /// and this says how many came before them.
    pub seq: u64,
    /// What took the fault, once it was delivered.
    pub taken_by: TakenBy,
    /// The boundaries around `origin` when the fault happened, the nearest
    /// first. Taken then, because by the time the fault is delivered the
    /// object may have left the tree.
    pub(crate) boundaries: Vec<ObjectId>,
    /// A boundary reporting that it could not make its own content.
    pub(crate) target: Option<ObjectId>,
    /// For an interrupted script: the object whose run the interrupt
    /// stopped, when it was a run nested in `origin`'s dispatch (a handler
    /// a write set off, a binding a read woke), and the boundaries around
    /// it then, the nearest first. `None` when it stopped `origin`'s own run.
    pub(crate) stopped: Option<Stopped>,
}

/// A run an interrupt stopped: its object and the boundaries around it,
/// the nearest first.
pub(crate) type Stopped = (ObjectId, Vec<ObjectId>);

impl FaultReport {
    /// The kind as a name: `"JsException"`, `"JsInterrupted"`,
    /// `"JsOutOfMemory"`, `"Panic"`, `"Startup"` or `"Internal"`.
    pub fn kind(&self) -> &'static str {
        fault_kind(&self.fault)
    }

    /// What went wrong, without the kind in front: the exception's
    /// message, the panic's payload, why the component could not be made.
    pub fn message(&self) -> String {
        match &self.fault {
            Fault::JsException(info) => info.message.clone(),
            Fault::Panic(m) => m.to_string(),
            Fault::Startup(m) | Fault::Internal(m) => m.clone(),
            other => other.to_string(),
        }
    }

    /// Whether the run an interrupt stopped was code outside `boundary`'s
    /// content that a dispatch begun inside it set off. The dispatch is
    /// still the content's fault, as its own run was cut short too, but
    /// the time went elsewhere.
    pub fn stopped_outside(&self, boundary: ObjectId) -> bool {
        self.stopped
            .as_ref()
            .is_some_and(|(_, around)| !around.contains(&boundary))
    }
}

/// See [`FaultReport::kind`].
pub fn fault_kind(fault: &Fault) -> &'static str {
    match fault {
        Fault::JsException(_) => "JsException",
        Fault::JsInterrupted { .. } => "JsInterrupted",
        Fault::JsOutOfMemory => "JsOutOfMemory",
        Fault::Panic(_) => "Panic",
        Fault::Startup(_) => "Startup",
        Fault::Internal(_) => "Internal",
    }
}

/// What took a fault: what the inspector says of each one it lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TakenBy {
    /// Not delivered yet: it is delivered between turns.
    Waiting,
    /// Nobody: a JS exception no boundary asked for, which is logged, as
    /// in Qt.
    Log,
    /// The boundary that stopped or restarted its content for it. When a
    /// boundary gave up, the one around it that took the fault. `name` is
    /// what it was called then ([`Engine::label_of`]).
    Boundary { object: ObjectId, name: String },
    /// The boundary that took it and left its content shown
    /// ([`crate::Boundary::keeps`]): a script of the content stopped for
    /// its time, whose binding the engine runs again. `name` as for
    /// [`TakenBy::Boundary`].
    Kept { object: ObjectId, name: String },
    /// No boundary took it ([`Engine::application_fault`]).
    Application,
    /// It happened in a subtree already discarded for an earlier fault.
    Discarded,
}

type FaultHandler = Rc<dyn Fn(&Engine, &FaultReport)>;
type Delivered = Box<dyn FnOnce(&Engine)>;

#[derive(Default)]
pub(crate) struct FaultState {
    /// The object whose code is running.
    origin: Cell<Option<ObjectId>>,
    /// The innermost origin a panic unwound through, for the catch that
    /// is further out.
    unwound: Cell<Option<ObjectId>>,
    queue: RefCell<VecDeque<FaultReport>>,
    delivering: Cell<bool>,
    application: RefCell<Option<FaultReport>>,
    history: RefCell<VecDeque<FaultReport>>,
    /// What each [`Engine::noting_faults`] running has noted so far, the
    /// outermost first.
    noting: RefCell<Vec<Vec<FaultReport>>>,
    /// Faults reported so far.
    filed: Cell<u64>,
    /// What [`Engine::after_faults_delivered`] was given, each with the
    /// faults reported by then.
    waiting: RefCell<Vec<(u64, Delivered)>>,
    /// The innermost nested run an interrupt stopped in the dispatch that
    /// unwinds now, with the boundaries around it as they were when it
    /// stopped (a later handler of the same signal may destroy it before
    /// the dispatch reports): `Some(None)` once one with no object of its
    /// own has.
    stopped: RefCell<Option<Option<Stopped>>>,
    handler: RefCell<Option<FaultHandler>>,
    /// Set by the JS bridge for the QML function it is about to call: an
    /// exception in it is the calling script's to catch.
    pub(crate) propagate: Cell<bool>,
    /// The function called with `propagate` threw; the exception is still
    /// pending in the JS context, unless `thrown_message` has one to throw.
    pub(crate) thrown: Cell<bool>,
    /// What to throw for code that stopped as a script throws but is not
    /// one (compiled code): no exception is pending.
    pub(crate) thrown_message: RefCell<Option<String>>,
    /// The objects whose functions an exception left for the calling
    /// script since the outermost run began: what that run's full heap
    /// cut short besides its own origin, which alone is reported. One
    /// caught on the way is in it too, which errs toward breaking.
    thrown_through: RefCell<Vec<ObjectId>>,
    /// The objects script wrote to, or called or signalled on, since the
    /// outermost run began: what that run's full heap may have left half
    /// updated, wherever its code was. Bounded as `thrown_through` is.
    written: RefCell<Vec<ObjectId>>,
    pub(crate) boundaries: crate::boundary::Boundaries,
}

/// How many objects [`FaultState::thrown_through`], and
/// [`FaultState::written`], tell apart. One more
/// is kept, to say that there were more.
pub(crate) const THROWN_THROUGH_KEPT: usize = 16;

/// Names the object whose code runs until it is dropped. On unwinding it
/// leaves the innermost such object for the catch.
pub(crate) struct OriginGuard<'a> {
    state: &'a FaultState,
    prev: Option<ObjectId>,
}

impl Drop for OriginGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() && self.state.unwound.get().is_none() {
            self.state.unwound.set(self.state.origin.get());
        }
        self.state.origin.set(self.prev);
    }
}

impl Engine {
    /// Runs `f` with the origin the panic being caught unwound out of set
    /// aside: a fault `f` reports names its own origin, and the caught one
    /// keeps its origin for whoever reports it after.
    pub(crate) fn apart_from_unwound<R>(&self, f: impl FnOnce() -> R) -> R {
        let first = self.inner.faults.unwound.take();
        let r = f();
        self.inner.faults.unwound.set(first);
        r
    }

    /// `obj`'s code runs until the guard is dropped. `None` leaves the
    /// origin as it is.
    pub(crate) fn origin(&self, obj: Option<ObjectId>) -> OriginGuard<'_> {
        let state = &self.inner.faults;
        let prev = state.origin.get();
        if obj.is_some() {
            state.origin.set(obj);
        }
        OriginGuard { state, prev }
    }

    /// Runs `f` as one turn: a panic in it is caught, reported as a fault
    /// of `origin` (or of the object whose code was running when it
    /// happened) and gives `None`. For everything a host runs on the
    /// engine that is not already inside one: an input event, a message
    /// from another thread, a frame.
    ///
    /// The turn is timed and watched as [`Engine::begin_turn`] says.
    pub fn turn<R>(&self, origin: Option<ObjectId>, f: impl FnOnce() -> R) -> Option<R> {
        self.turn_for(crate::TurnCause::Host, origin, f)
    }

    /// The object whose code is running.
    pub(crate) fn running_origin(&self) -> Option<ObjectId> {
        self.inner.faults.origin.get()
    }

    /// Code of `obj` threw out to the script that called it.
    pub(crate) fn note_thrown_through(&self, obj: Option<ObjectId>) {
        let Some(obj) = obj else {
            return;
        };
        let state = &self.inner.faults;
        let mut through = state.thrown_through.borrow_mut();
        if through.len() <= THROWN_THROUGH_KEPT && !through.contains(&obj) {
            through.push(obj);
        }
    }

    /// Script wrote to `obj`, or called or signalled on it: a full heap
    /// later in the run may leave it half updated. On every write a script
    /// makes, so most are of the object written last and cost one compare.
    pub(crate) fn note_written(&self, obj: ObjectId) {
        let mut written = self.inner.faults.written.borrow_mut();
        if written.last() != Some(&obj)
            && written.len() <= THROWN_THROUGH_KEPT
            && !written.contains(&obj)
        {
            written.push(obj);
        }
    }

    /// What [`Engine::note_written`] noted since the outermost run began.
    /// A copy: asking what a boundary discards may run script, which notes
    /// its own writes.
    pub(crate) fn written_in_run(&self) -> Vec<ObjectId> {
        self.inner.faults.written.borrow().clone()
    }

    /// An outermost run of script begins: nothing was thrown out of or
    /// written to yet.
    pub(crate) fn forget_thrown_through(&self) {
        self.inner.faults.thrown_through.borrow_mut().clear();
        self.inner.faults.written.borrow_mut().clear();
    }

    /// [`Engine::turn`] for something other than host code: a message
    /// from another thread is named by its mailbox and kind when the turn
    /// runs long.
    pub fn turn_for<R>(
        &self,
        cause: crate::TurnCause,
        origin: Option<ObjectId>,
        f: impl FnOnce() -> R,
    ) -> Option<R> {
        let _turn = self.begin_turn(cause, origin);
        let guard = self.origin(origin);
        // The engine is usable after a panic by construction (see the
        // module documentation), which is what unwind safety asks about.
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(r) => Some(r),
            Err(payload) => {
                let at = self.inner.faults.origin.get();
                drop(guard);
                self.report_fault(at, Fault::from_panic(payload));
                None
            }
        }
    }

    /// The one place a fault goes. It is logged, kept for
    /// [`Engine::faults`] and queued for the boundary around `origin`;
    /// the host is asked for a turn to deliver it in.
    pub fn report_fault(&self, origin: Option<ObjectId>, fault: Fault) {
        // Whoever caught the panic, the object it unwound out of first is
        // the one whose code failed.
        let origin = match &fault {
            Fault::Panic(_) => self.inner.faults.unwound.take().or(origin),
            _ => origin,
        };
        self.run_again_after(origin, &[], &fault);
        let (file, line) = self.declared_place(origin);
        let place = origin.and_then(|o| self.declared_at(o)).unwrap_or_default();
        self.warn(format!("{place}{fault}"));
        self.file_fault(FaultReport {
            fault,
            origin,
            file,
            line,
            stack: Vec::new(),
            at: self.now(),
            boundaries: self.boundaries_around(origin),
            target: None,
            seq: 0,
            origin_name: origin.and_then(|o| self.label_of(o)),
            taken_by: TakenBy::Waiting,
            stopped: None,
        });
    }

    /// A boundary could not make its own content: a fault of that
    /// boundary, counted like any other exit of its content.
    pub fn report_boundary_fault(&self, boundary: ObjectId, fault: Fault) {
        let (file, line) = self.declared_place(Some(boundary));
        let place = self.declared_at(boundary).unwrap_or_default();
        self.warn(format!("{place}{fault}"));
        self.file_fault(FaultReport {
            fault,
            origin: Some(boundary),
            file,
            line,
            stack: Vec::new(),
            at: self.now(),
            boundaries: self.boundaries_around(Some(boundary)),
            target: Some(boundary),
            seq: 0,
            origin_name: self.label_of(boundary),
            taken_by: TakenBy::Waiting,
            stopped: None,
        });
    }

    /// The QML frames a JS exception's stack passed through, and the trace
    /// printed under its message: one `    at function (file:line)` line per
    /// frame, given only when there is more than the frame that threw.
    pub(crate) fn js_trace(&self, stack: &str) -> (Vec<StackFrame>, Option<String>) {
        let frames = self.qml_stack(stack);
        let trace = (frames.len() > 1).then(|| {
            frames
                .iter()
                .map(|f| {
                    let file = f.file.rsplit('/').next().unwrap_or(&f.file);
                    match &f.function {
                        Some(name) => format!("    at {name} ({file}:{})", f.line),
                        None => format!("    at {file}:{}", f.line),
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        });
        (frames, trace)
    }

    /// A failed JS run. An exception keeps the line Qt logs for it
    /// (`file:line: message`), followed by the QML functions it passed
    /// through when there is more than the one that threw. `stopped` is
    /// the nested run an interrupt stopped in the dispatch, as its
    /// outermost run gave it back ([`Engine::swap_js_stopped`]).
    pub(crate) fn report_js_fault(&self, fault: &JsFault, stopped: Option<Stopped>) {
        let state = &self.inner.faults;
        let origin = state.origin.get();
        if !matches!(fault, JsFault::Panic { .. }) {
            let through = state.thrown_through.borrow().clone();
            self.run_again_after(origin, &through, &fault.clone().into());
        }
        match fault {
            JsFault::JsException {
                message,
                stack,
                file,
                line,
                ..
            } => {
                let (frames, trace) = self.js_trace(stack);
                let mut text = match (file, line) {
                    (Some(file), Some(line)) => format!("{file}:{line}: {message}"),
                    _ => message.clone(),
                };
                if let Some(trace) = &trace {
                    text.push('\n');
                    text.push_str(trace);
                }
                let (declared, declared_line) = self.declared_place(origin);
                self.warn_and_file(
                    text,
                    FaultReport {
                        fault: Fault::JsException(JsErrorInfo {
                            message: message.clone(),
                            file: file.as_deref().map(Into::into),
                            line: line.unwrap_or(0),
                            column: 0,
                            stack: trace,
                        }),
                        origin,
                        file: file.clone().or(declared),
                        line: line.unwrap_or(declared_line),
                        stack: frames,
                        at: self.now(),
                        boundaries: self.boundaries_around(origin),
                        target: None,
                        seq: 0,
                        origin_name: origin.and_then(|o| self.label_of(o)),
                        taken_by: TakenBy::Waiting,
                        stopped: None,
                    },
                );
            }
            JsFault::Panic { message, .. } => {
                self.report_fault(origin, Fault::Panic(message.clone().into()));
            }
            other => {
                // The JS host's own words: they name what was running.
                let (file, line) = self.declared_place(origin);
                let stopped = stopped.filter(|_| matches!(other, JsFault::JsInterrupted { .. }));
                self.warn_and_file(
                    other.to_string(),
                    FaultReport {
                        fault: other.clone().into(),
                        origin,
                        file,
                        line,
                        stack: Vec::new(),
                        at: self.now(),
                        boundaries: self.boundaries_around(origin),
                        target: None,
                        seq: 0,
                        origin_name: origin.and_then(|o| self.label_of(o)),
                        taken_by: TakenBy::Waiting,
                        stopped,
                    },
                );
            }
        }
    }

    /// Logs `msg` and files `report`, or holds both while a binding runs
    /// during a creation (delayed_errors.rs): a run that is fixed by a
    /// later one is neither logged nor anybody's fault.
    fn warn_and_file(&self, msg: String, report: FaultReport) {
        if let Some((msg, report)) = self.hold_fault(msg, report) {
            self.warn(msg);
            self.file_fault(report);
        }
    }

    /// Puts `with` in place of the run noted as stopped, giving back what
    /// was noted. A dispatch begins with nothing and takes its own back
    /// when it returns, restoring the one it began in: a dispatch made
    /// while an interrupted one unwinds (an object it let go of, held
    /// again) neither drops that one's nor lends it its own.
    pub(crate) fn swap_js_stopped(&self, with: Option<Option<Stopped>>) -> Option<Option<Stopped>> {
        self.inner.faults.stopped.replace(with)
    }

    /// A nested run came back interrupted. The first to come back so is
    /// the run the interrupt stopped, and its object is the one running now.
    pub(crate) fn note_js_stopped(&self) {
        let state = &self.inner.faults;
        if state.stopped.borrow().is_some() {
            return;
        }
        let stopped = state
            .origin
            .get()
            .map(|o| (o, self.boundaries_around(Some(o))));
        *state.stopped.borrow_mut() = Some(stopped);
    }

    /// How many faults have been reported: a fault whose
    /// [`FaultReport::seq`] is at most this was reported by now.
    pub fn faults_reported(&self) -> u64 {
        self.inner.faults.filed.get()
    }

    pub(crate) fn file_fault(&self, mut report: FaultReport) {
        let state = &self.inner.faults;
        state.filed.set(state.filed.get() + 1);
        report.seq = state.filed.get();
        // An exception is only somebody's fault inside a boundary that
        // asked for exceptions; without any boundary around it there is
        // nothing to deliver and no reason to wake the host.
        let logged = matches!(report.fault, Fault::JsException(_)) && report.boundaries.is_empty();
        if logged {
            report.taken_by = TakenBy::Log;
        }
        for noted in state.noting.borrow_mut().iter_mut() {
            noted.push(report.clone());
        }
        {
            let mut history = state.history.borrow_mut();
            if history.len() == MAX_HISTORY {
                history.pop_front();
            }
            history.push_back(report.clone());
        }
        if logged {
            return;
        }
        state.queue.borrow_mut().push_back(report);
        self.wake_host();
    }

    /// Writes down in the history what took fault `seq`.
    fn taken(&self, seq: u64, by: TakenBy) {
        let mut history = self.inner.faults.history.borrow_mut();
        if let Some(r) = history.iter_mut().rev().find(|r| r.seq == seq) {
            r.taken_by = by;
        }
    }

    /// The URL and line `obj` is declared at.
    pub(crate) fn declared_place(&self, obj: Option<ObjectId>) -> (Option<String>, u32) {
        let store = self.inner.store.borrow();
        obj.and_then(|o| store.objects.get(o)?.location())
            .map_or((None, 0), |l| (Some(l.url.to_string()), l.line))
    }

    /// The QML side of a raw QuickJS stack, the frame that threw first.
    ///
    /// The compiler wraps a QML function as
    /// `export function fN(…) { return (function name() { … }).apply(this, $args); }`,
    /// so a call shows as three frames: `name`, `apply (native)` and `fN`.
    /// Only the first is the user's. A plain method is compiled as `fN`
    /// itself, named by the line table. A handler or a binding is the body
    /// of its `fN` and has no name of its own.
    fn qml_stack(&self, raw: &str) -> Vec<StackFrame> {
        let Some(js) = self.inner.js.borrow().clone() else {
            return Vec::new();
        };
        let lines = js.lines.borrow();
        let mut out = Vec::new();
        let mut after_apply = false;
        for l in raw.lines() {
            let Some(frame) = RawFrame::parse(l) else {
                continue;
            };
            let wrapper = after_apply && is_generated_name(frame.name);
            after_apply = frame.name == "apply" && frame.at.is_none();
            let Some((module, line)) = frame.at else {
                continue;
            };
            if wrapper {
                continue;
            }
            let Some((file, line)) = lines.map(module, line) else {
                continue;
            };
            let named = !frame.name.is_empty()
                && frame.name != "<anonymous>"
                && !is_generated_name(frame.name);
            let function = if named {
                Some(frame.name.to_owned())
            } else {
                lines.method_name(module, frame.name).map(str::to_owned)
            };
            out.push(StackFrame {
                function,
                file,
                line,
            });
        }
        out
    }

    /// Hands the queued faults to their boundaries, or to the application.
    /// Returns how many. Call it between turns, never from inside a
    /// handler: a boundary destroys and creates objects here. Not
    /// re-entrant; a fault reported while one is delivered is delivered
    /// by the same call.
    pub fn deliver_faults(&self) -> usize {
        let state = &self.inner.faults;
        if state.delivering.get() {
            return 0;
        }
        // What is asked during this delivery waits for a later one.
        let waiting = state.waiting.take();
        if state.queue.borrow().is_empty() {
            self.drop_idle_run_again();
            self.tell_delivered(waiting);
            return 0;
        }
        state.delivering.set(true);
        let mut n = 0;
        while n < MAX_DELIVERED {
            let Some(report) = state.queue.borrow_mut().pop_front() else {
                break;
            };
            n += 1;
            // A boundary that panics while it restarts must not leave the
            // queue stuck.
            let seq = report.seq;
            let delivered = catch_unwind(AssertUnwindSafe(|| self.trip_boundary(&report)));
            let unhandled = match delivered {
                Ok(crate::boundary::Tripped::Handled(by)) => {
                    self.taken(seq, by);
                    None
                }
                Ok(crate::boundary::Tripped::Unhandled(r)) => Some(*r),
                Err(payload) => {
                    state.unwound.take();
                    let mut r = report;
                    r.fault = Fault::from_panic(payload);
                    Some(r)
                }
            };
            if let Some(r) = unhandled {
                self.taken(seq, TakenBy::Application);
                self.set_application_fault(r);
            }
        }
        state.delivering.set(false);
        self.drop_idle_run_again();
        self.tell_delivered(waiting);
        n
    }

    /// Whether faults wait for [`Engine::deliver_faults`].
    pub fn has_pending_faults(&self) -> bool {
        !self.inner.faults.queue.borrow().is_empty()
    }

    /// Runs `f` once the faults reported by now have all been delivered:
    /// at the end of the next [`Engine::deliver_faults`] that leaves none
    /// of them queued, one with nothing to deliver included. Asked from
    /// inside a delivery, it waits for a later one.
    pub fn after_faults_delivered(&self, f: impl FnOnce(&Engine) + 'static) {
        let state = &self.inner.faults;
        let by_now = (state.filed.get(), Box::new(f) as Delivered);
        state.waiting.borrow_mut().push(by_now);
    }

    /// Runs those of `waiting` whose faults are all delivered; the others
    /// wait on, ahead of what was asked meanwhile.
    fn tell_delivered(&self, waiting: Vec<(u64, Delivered)>) {
        if waiting.is_empty() {
            return;
        }
        let state = &self.inner.faults;
        let first_queued = state.queue.borrow().front().map(|r| r.seq);
        let (over, mut kept): (Vec<_>, Vec<_>) = waiting
            .into_iter()
            .partition(|(by_then, _)| first_queued.is_none_or(|seq| *by_then < seq));
        {
            let mut asked = state.waiting.borrow_mut();
            kept.append(&mut asked);
            *asked = kept;
        }
        for (_, f) in over {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| f(self))) {
                state.unwound.take();
                self.report_fault(None, Fault::from_panic(payload));
            }
        }
    }

    fn set_application_fault(&self, report: FaultReport) {
        let state = &self.inner.faults;
        *state.application.borrow_mut() = Some(report.clone());
        let handler = state.handler.borrow().clone();
        if let Some(h) = handler {
            h(self, &report);
        }
    }

    /// The latest fault no boundary took: the application root is the
    /// boundary of everything that has none. Nothing was destroyed for
    /// it. What a shell's error overlay shows.
    pub fn application_fault(&self) -> Option<FaultReport> {
        self.inner.faults.application.borrow().clone()
    }

    /// Forgets the application's fault (the overlay was dismissed, the
    /// root was made again).
    pub fn clear_application_fault(&self) {
        self.inner.faults.application.borrow_mut().take();
    }

    /// Calls `f` whenever a fault reaches the application, at delivery.
    /// One handler; a second call replaces it.
    pub fn set_fault_handler(&self, f: impl Fn(&Engine, &FaultReport) + 'static) {
        *self.inner.faults.handler.borrow_mut() = Some(Rc::new(f));
    }

    /// Runs `f` and gives every fault filed while it ran, the oldest
    /// first, however many. What judges what `f` made by its faults asks
    /// this, not [`Engine::faults`], which keeps only the latest.
    pub fn noting_faults<R>(&self, f: impl FnOnce() -> R) -> (R, Vec<FaultReport>) {
        struct Noting<'a>(&'a FaultState);
        impl Drop for Noting<'_> {
            fn drop(&mut self) {
                self.0.noting.borrow_mut().pop();
            }
        }
        let state = &self.inner.faults;
        let depth = {
            let mut noting = state.noting.borrow_mut();
            noting.push(Vec::new());
            noting.len()
        };
        let noting = Noting(state);
        let r = f();
        let noted = std::mem::take(&mut state.noting.borrow_mut()[depth - 1]);
        drop(noting);
        (r, noted)
    }

    /// The latest faults of every kind, the oldest first: at most 32,
    /// whoever handled them.
    pub fn faults(&self) -> Vec<FaultReport> {
        self.inner.faults.history.borrow().iter().cloned().collect()
    }

    /// Runs a native method or hook of `origin`'s; a panic in it is
    /// reported and is an error to the caller.
    pub(crate) fn guarded_result<R>(
        &self,
        origin: ObjectId,
        f: impl FnOnce() -> Result<R, EngineError>,
    ) -> Result<R, EngineError> {
        let guard = self.origin(Some(origin));
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(payload) => {
                drop(guard);
                let fault = Fault::from_panic(payload);
                let message = fault.to_string();
                self.report_fault(Some(origin), fault);
                Err(EngineError::Invoke(message))
            }
        }
    }

    /// Runs one handler or hook of `origin`'s; a panic in it is reported
    /// and ends only it. False when it panicked.
    pub(crate) fn guarded(&self, origin: ObjectId, f: impl FnOnce()) -> bool {
        self.turn(Some(origin), f).is_some()
    }
}

/// One line of a QuickJS stack: `at name (module:line:col)`,
/// `at name (native)` or `at module:line:col`.
struct RawFrame<'a> {
    name: &'a str,
    /// The module and the JS line; `None` for a native frame.
    at: Option<(&'a str, u32)>,
}

impl<'a> RawFrame<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let rest = line.trim().strip_prefix("at ")?;
        let (name, pos) = match rest.rsplit_once(" (") {
            Some((name, pos)) => (name, pos.strip_suffix(')')?),
            None => ("", rest),
        };
        if pos == "native" {
            return Some(RawFrame { name, at: None });
        }
        // `module:line:col`; the module's name may hold colons of its own.
        let mut parts = pos.rsplitn(3, ':');
        let (_col, line, module) = (parts.next()?, parts.next()?, parts.next()?);
        Some(RawFrame {
            name,
            at: Some((module, line.parse().ok()?)),
        })
    }
}

/// `f0`, `f12`: the name the compiler gives each function it writes.
fn is_generated_name(name: &str) -> bool {
    name.strip_prefix('f')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panic caught while another's origin waits to be reported names
    /// its own origin, and leaves the waiting one to whoever reports it.
    #[test]
    fn a_fault_reported_apart_from_an_unwound_panic_leaves_it_its_origin() {
        let e = Engine::new();
        let obj = e.create_native(crate::QtObject::default(), None).unwrap();
        e.inner.faults.unwound.set(Some(obj));
        e.apart_from_unwound(|| e.report_fault(None, Fault::Panic("told".into())));
        e.report_fault(None, Fault::Panic("made".into()));
        let filed = e.faults();
        assert_eq!(filed.len(), 2, "{filed:?}");
        assert_eq!(filed[0].origin, None, "{filed:?}");
        assert_eq!(filed[1].origin, Some(obj), "{filed:?}");
    }

    #[test]
    fn stack_lines_are_parsed() {
        let f = RawFrame::parse("    at inner (Main.qml.js:11:15)").unwrap();
        assert_eq!((f.name, f.at), ("inner", Some(("Main.qml.js", 11))));
        let f = RawFrame::parse("    at apply (native)").unwrap();
        assert_eq!((f.name, f.at), ("apply", None));
        let f = RawFrame::parse("    at Main.qml.js?generation=2:4:1").unwrap();
        assert_eq!((f.name, f.at), ("", Some(("Main.qml.js?generation=2", 4))));
        assert!(RawFrame::parse("Error: no").is_none());
    }

    #[test]
    fn generated_names() {
        assert!(is_generated_name("f0"));
        assert!(is_generated_name("f12"));
        assert!(!is_generated_name("f"));
        assert!(!is_generated_name("fetch"));
        assert!(!is_generated_name("f1x"));
    }

    /// A flag, and what sets it.
    fn flag() -> (std::rc::Rc<Cell<bool>>, impl FnOnce(&Engine) + 'static) {
        let ran = std::rc::Rc::new(Cell::new(false));
        let set = ran.clone();
        (ran, move |_: &Engine| set.set(true))
    }

    #[test]
    fn what_waits_for_a_delivery_runs_after_one_with_nothing_to_deliver() {
        let e = Engine::new();
        let (ran, f) = flag();
        e.after_faults_delivered(f);
        assert!(!ran.get(), "not where it was asked");
        assert_eq!(e.deliver_faults(), 0);
        assert!(ran.get());
    }

    #[test]
    fn what_waits_for_a_delivery_does_not_run_in_one_nested_in_it() {
        let e = Engine::new();
        let (ran, f) = flag();
        let early = std::rc::Rc::new(Cell::new(false));
        let (seen, saw) = (ran.clone(), early.clone());
        e.set_fault_handler(move |e, _| {
            assert_eq!(e.deliver_faults(), 0, "nested");
            saw.set(seen.get());
        });
        e.report_fault(None, Fault::Panic("one".into()));
        e.after_faults_delivered(f);
        e.deliver_faults();
        assert!(!early.get(), "ran before the delivery was over");
        assert!(ran.get());
    }

    #[test]
    fn what_waits_for_a_delivery_waits_for_the_one_that_takes_the_last_fault() {
        let e = Engine::new();
        for _ in 0..=MAX_DELIVERED {
            e.report_fault(None, Fault::Panic("many".into()));
        }
        let (ran, f) = flag();
        e.after_faults_delivered(f);
        assert_eq!(e.deliver_faults(), MAX_DELIVERED);
        assert!(!ran.get(), "one of them is still queued");
        assert_eq!(e.deliver_faults(), 1);
        assert!(ran.get());
    }

    #[test]
    fn a_fault_reported_later_does_not_hold_up_what_waits_for_a_delivery() {
        let e = Engine::new();
        let (ran, f) = flag();
        e.after_faults_delivered(f);
        for _ in 0..=MAX_DELIVERED {
            e.report_fault(None, Fault::Panic("later".into()));
        }
        e.deliver_faults();
        assert!(e.has_pending_faults());
        assert!(ran.get());
    }

    #[test]
    fn what_is_asked_inside_a_delivery_waits_for_a_later_one() {
        let e = Engine::new();
        let ran = std::rc::Rc::new(Cell::new(false));
        let set = ran.clone();
        e.set_fault_handler(move |e, _| {
            let set = set.clone();
            e.after_faults_delivered(move |_| set.set(true));
        });
        e.report_fault(None, Fault::Panic("one".into()));
        e.deliver_faults();
        assert!(!ran.get(), "ran in the delivery it was asked in");
        e.deliver_faults();
        assert!(ran.get());
    }
}
