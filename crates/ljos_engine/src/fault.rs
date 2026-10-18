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
}

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
    /// No boundary took it ([`Engine::application_fault`]).
    Application,
    /// It happened in a subtree already discarded for an earlier fault.
    Discarded,
}

type FaultHandler = Rc<dyn Fn(&Engine, &FaultReport)>;

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
    /// Faults reported so far.
    filed: Cell<u64>,
    handler: RefCell<Option<FaultHandler>>,
    /// Set by the JS bridge for the QML function it is about to call: an
    /// exception in it is the calling script's to catch.
    pub(crate) propagate: Cell<bool>,
    /// The function called with `propagate` threw; the exception is still
    /// pending in the JS context.
    pub(crate) thrown: Cell<bool>,
    pub(crate) boundaries: crate::boundary::Boundaries,
}

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
    pub fn turn<R>(&self, origin: Option<ObjectId>, f: impl FnOnce() -> R) -> Option<R> {
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
        let (file, line) = self.declared_place(origin);
        let place = origin.and_then(|o| self.location_of(o)).unwrap_or_default();
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
        });
    }

    /// A boundary could not make its own content: a fault of that
    /// boundary, counted like any other exit of its content.
    pub fn report_boundary_fault(&self, boundary: ObjectId, fault: Fault) {
        let (file, line) = self.declared_place(Some(boundary));
        let place = self.location_of(boundary).unwrap_or_default();
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
    /// through when there is more than the one that threw.
    pub(crate) fn report_js_fault(&self, fault: &JsFault) {
        let state = &self.inner.faults;
        let origin = state.origin.get();
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
                self.warn(text);
                let (declared, declared_line) = self.declared_place(origin);
                self.file_fault(FaultReport {
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
                });
            }
            JsFault::Panic { message, .. } => {
                self.report_fault(origin, Fault::Panic(message.clone().into()));
            }
            other => {
                // The JS host's own words: they name what was running.
                let (file, line) = self.declared_place(origin);
                self.warn(other.to_string());
                self.file_fault(FaultReport {
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
                });
            }
        }
    }

    fn file_fault(&self, mut report: FaultReport) {
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
        obj.and_then(|o| store.objects.get(o)?.ext.as_ref()?.location.as_ref())
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
        if state.delivering.get() || state.queue.borrow().is_empty() {
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
        n
    }

    /// Whether faults wait for [`Engine::deliver_faults`].
    pub fn has_pending_faults(&self) -> bool {
        !self.inner.faults.queue.borrow().is_empty()
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
}
