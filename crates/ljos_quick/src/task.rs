//! `Task` (Ljos.Core): a Controller that runs async work from its
//! arguments and says where it got to (docs/DIALECT.md §4.19).
//!
//! `run` is called with the elements of `args` and then an `AbortSignal`,
//! as the Task completes and again when an element of `args` changes (by
//! `!==`). The run a newer one replaces is aborted through its signal, and
//! what it gives is dropped. `state` is the one value a script takes apart
//! (`status`, and the `value` and `error` it has); `status`, `value` and
//! `error` are its parts, one property each. A step writes each that
//! changes once, in one batch.
//!
//! A run is a promise, so a pending Task schedules nothing: what wakes the
//! engine is what the run waits on (a reply, a timer), whose settling is a
//! JS job that tells the Task. What a run needs once the Task is gone (its
//! abort) lives in a per-engine table, not in the Task, so the Task's
//! destruction, its host's with it, can abort it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, ImplInfo, LogLevel, MethodDecl, ObjectFlags, ObjectId, PropIndex,
    QmlImplInfo, QmlObject, QmlType, SignalDecl, SignalIndex, TypeTag, Value, qml_impl,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::controller::Controller;

/// `Task` (Ljos.Core).
#[derive(Debug, QmlObject)]
#[qml(module = "Ljos.Core", name = "Task", extends = Controller, default = "data")]
pub struct Task {
    #[qml(base)]
    pub controller: Controller,
    /// What `run` is called with: an array, whose elements come before
    /// the signal. A change of an element starts a run, with `autoRun`.
    #[qml(notify, change = args_changed)]
    pub args: Value,
    /// The work: a function of the elements of `args` and an
    /// `AbortSignal`, which gives a value or a promise of one. A new one
    /// is called at the next run.
    #[qml(notify)]
    pub run: Value,
    /// Whether the Task runs as it completes and as `args` changes; when
    /// false it runs on `start()` only.
    #[qml(notify, rename = "autoRun")]
    pub auto_run: bool,
    /// `{status: "initial"}`, `{status: "pending", value}`,
    /// `{status: "done", value}` or `{status: "failed", error, value}`.
    #[qml(notify, readonly)]
    pub state: Value,
    /// `state.status`.
    #[qml(notify, readonly)]
    pub status: String,
    /// What the last run that was done gave: kept while a new one is
    /// pending and when it fails. Undefined until one is done.
    #[qml(notify, readonly)]
    pub value: Value,
    /// What the run that failed threw or rejected with; undefined but
    /// where the Task failed.
    #[qml(notify, readonly)]
    pub error: Value,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            controller: Controller::default(),
            args: Value::Undefined,
            run: Value::Undefined,
            auto_run: true,
            state: state(INITIAL, &Value::Undefined, None),
            status: INITIAL.into(),
            value: Value::Undefined,
            error: Value::Undefined,
        }
    }
}

const INITIAL: &str = "initial";
const PENDING: &str = "pending";
const DONE: &str = "done";
const FAILED: &str = "failed";

fn state(status: &str, value: &Value, error: Option<&Value>) -> Value {
    let mut fields = vec![("status", Value::from(status))];
    if status != INITIAL {
        fields.push(("value", value.clone()));
    }
    if let Some(error) = error {
        fields.push(("error", error.clone()));
    }
    Value::map(fields)
}

// Written by hand, not by `#[qml_impl]`: `start` takes any number of
// arguments, as `run` does, and a generated thunk takes a fixed list.
impl QmlImplInfo for Task {
    fn impl_info() -> ImplInfo {
        ImplInfo {
            signals: vec![
                SignalDecl {
                    name: "done",
                    params: vec![("value", TypeTag::Var)],
                },
                SignalDecl {
                    name: "failed",
                    params: vec![("error", TypeTag::Var)],
                },
            ],
            methods: vec![
                MethodDecl {
                    name: "start",
                    params: Vec::new(),
                    ret: TypeTag::Void,
                    thunk: start_thunk,
                },
                MethodDecl {
                    name: "abort",
                    params: Vec::new(),
                    ret: TypeTag::Void,
                    thunk: abort_thunk,
                },
            ],
            on_complete: Some(completed),
        }
    }
}

/// `start(...args)`: runs now, with the arguments given, or with `args`'
/// elements when none are, aborting the run in progress.
fn start_thunk(cx: &Cx<'_>, given: &[Value]) -> Result<Value, EngineError> {
    let (e, me) = (cx.engine, cx.this);
    let args = if given.is_empty() {
        e.untrack(|| e.get(me, Task::ARGS))?
    } else {
        Value::list(given.to_vec())
    };
    begin(e, me, args)?;
    Ok(Value::Undefined)
}

/// `abort()`: the run in progress fails now, with its signal's reason;
/// what it gives later is dropped.
fn abort_thunk(cx: &Cx<'_>, _: &[Value]) -> Result<Value, EngineError> {
    let (e, me) = (cx.engine, cx.this);
    let Some(reason) = Tasks::of(e).abort(e, me) else {
        return Ok(Value::Undefined);
    };
    fail(e, me, reason)?;
    Ok(Value::Undefined)
}

/// The first run, with `autoRun`. After the Controller's own completion,
/// which runs first: the Task has its host.
fn completed(cx: &Cx<'_>) -> Result<(), EngineError> {
    let (e, me) = (cx.engine, cx.this);
    if !e.untrack(|| e.get(me, Task::AUTO_RUN))? {
        return Ok(());
    }
    let args = e.untrack(|| e.get(me, Task::ARGS))?;
    begin(e, me, args)
}

/// `args` changed: a new element runs again, with `autoRun`. An array
/// made again with the same elements does not.
fn args_changed(cx: &Cx<'_>, _: PropIndex) {
    let (e, me) = (cx.engine, cx.this);
    let complete = e
        .flags(me)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if !complete || !e.is_alive(me) || !e.get(me, Task::AUTO_RUN).unwrap_or(false) {
        return;
    }
    let tasks = Tasks::of(e);
    let Ok(args) = e.get(me, Task::ARGS) else {
        return;
    };
    let last = tasks.entries.borrow().get(&me).map(|t| t.args.clone());
    if let Some(last) = last
        && tasks.same(e, &last, &args)
    {
        return;
    }
    warn(e, begin(e, me, args));
}

/// Starts a run with `args`, aborting the one in progress.
fn begin(e: &Engine, me: ObjectId, args: Value) -> Result<(), EngineError> {
    let tasks = Tasks::of(e);
    tasks.abort(e, me);
    let number = tasks.number();
    tasks.watch(e, me)?;
    let value = e.untrack(|| e.get(me, Task::VALUE))?;
    put(e, me, PENDING, &value, None)?;
    let run = e.untrack(|| e.get(me, Task::RUN))?;
    let Some(driver) = tasks.driver(e) else {
        return Ok(());
    };
    // Recorded before the run is called, which may settle it at once.
    tasks.entries.borrow_mut().entry(me).or_default().run = Some(Run {
        number,
        abort: Value::Undefined,
    });
    tasks.numbers.borrow_mut().insert(number, me);
    let started = e
        .try_call_js(&driver.start, &[Value::Int(number), run, args])
        .map_err(|err| EngineError::Invoke(err.to_string()))?;
    let abort = e.js_member(&started, "abort").unwrap_or_default();
    let args = e.js_member(&started, "args").unwrap_or_default();
    let mut entries = tasks.entries.borrow_mut();
    let entry = entries.entry(me).or_default();
    entry.args = args;
    if let Some(run) = entry.run.as_mut().filter(|r| r.number == number) {
        run.abort = abort;
    }
    Ok(())
}

/// What run `number` gave: the Task it is the run of is done or has
/// failed, unless a newer run or `abort()` replaced it.
fn settle(e: &Engine, number: i32, ok: bool, outcome: Value) -> Result<(), EngineError> {
    let tasks = Tasks::of(e);
    let Some(me) = tasks.numbers.borrow_mut().remove(&number) else {
        return Ok(());
    };
    {
        let mut entries = tasks.entries.borrow_mut();
        let Some(entry) = entries.get_mut(&me) else {
            return Ok(());
        };
        if entry.run.as_ref().is_none_or(|r| r.number != number) {
            return Ok(());
        }
        entry.run = None;
    }
    if !e.is_alive(me) {
        return Ok(());
    }
    if ok {
        put(e, me, DONE, &outcome, None)?;
        e.emit_typed(me, Task::static_meta(), signal("done"), &[outcome])
    } else {
        fail(e, me, outcome)
    }
}

fn fail(e: &Engine, me: ObjectId, error: Value) -> Result<(), EngineError> {
    let value = e.untrack(|| e.get(me, Task::VALUE))?;
    put(e, me, FAILED, &value, Some(&error))?;
    e.emit_typed(me, Task::static_meta(), signal("failed"), &[error])
}

/// Where the Task got to: each property that changes is written once, in
/// one batch, so a binding of two of them runs once.
fn put(
    e: &Engine,
    me: ObjectId,
    status: &str,
    value: &Value,
    error: Option<&Value>,
) -> Result<(), EngineError> {
    let (was_status, was_value, was_error) = e.untrack(|| {
        Ok::<_, EngineError>((
            e.get(me, Task::STATUS)?,
            e.get(me, Task::VALUE)?,
            e.get(me, Task::ERROR)?,
        ))
    })?;
    let error_now = error.cloned().unwrap_or_default();
    let value_same = same_value(&was_value, value);
    let error_same = same_value(&was_error, &error_now);
    if was_status == status && value_same && error_same {
        return Ok(());
    }
    e.batch(|| {
        e.set_internal(me, Task::STATE, state(status, value, error))?;
        if was_status != status {
            e.set_internal(me, Task::STATUS, status.to_owned())?;
        }
        if !value_same {
            e.set_internal(me, Task::VALUE, value.clone())?;
        }
        if !error_same {
            e.set_internal(me, Task::ERROR, error_now)?;
        }
        Ok(())
    })
}

/// Whether a part is the same value it was: the same JS object, or equal
/// data.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Js(a), Value::Js(b)) => a == b,
        (Value::Js(_), _) | (_, Value::Js(_)) => false,
        (a, b) => a.change_equals(b),
    }
}

fn signal(name: &str) -> SignalIndex {
    Task::static_meta()
        .signal_index(name)
        .expect("declared in Task's ImplInfo")
}

fn warn(e: &Engine, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(LogLevel::Warning, "ljos.task", format!("Task: {err}"));
    }
}

/// What a Task's run needs from scripts: the native end of its promise.
/// One per engine, made with the first run.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "TaskHelper", uncreatable)]
pub struct TaskHelper {}

#[qml_impl]
impl TaskHelper {
    /// Run `number` gave `outcome`: its value when `ok`, else what it
    /// threw or rejected with.
    #[qml(invokable)]
    fn settle(cx: &Cx, number: i32, ok: bool, outcome: Value) {
        warn(cx.engine, settle(cx.engine, number, ok, outcome));
    }
}

/// The runs in progress, and the arguments of each Task's last run, by
/// Task. Per engine.
#[derive(Default)]
struct Tasks {
    driver: RefCell<Option<Rc<Driver>>>,
    next: Cell<i32>,
    entries: RefCell<FxHashMap<ObjectId, Entry>>,
    /// Which Task each run in progress is of.
    numbers: RefCell<FxHashMap<i32, ObjectId>>,
    /// The Tasks whose destruction aborts their run: connected once.
    watched: RefCell<FxHashSet<ObjectId>>,
}

#[derive(Default)]
struct Entry {
    /// The elements the last run was called with, copied.
    args: Value,
    run: Option<Run>,
}

struct Run {
    number: i32,
    /// Aborts the run's signal and gives its reason.
    abort: Value,
}

struct Driver {
    start: Value,
    same: Value,
}

impl Tasks {
    fn of(e: &Engine) -> Rc<Self> {
        e.extension::<Self>()
    }

    fn number(&self) -> i32 {
        let n = self.next.get().wrapping_add(1);
        self.next.set(n);
        n
    }

    fn driver(&self, e: &Engine) -> Option<Rc<Driver>> {
        if let Some(d) = self.driver.borrow().as_ref() {
            return Some(d.clone());
        }
        let made = e
            .create_native_incomplete(TaskHelper {}, None)
            .and_then(|helper| {
                let install = e.eval_js(DRIVER)?;
                e.try_call_js(&install, &[Value::Object(helper)])
                    .map_err(|err| EngineError::Invoke(err.to_string()))
            });
        let made = match made {
            Ok(v) => v,
            Err(err) => {
                warn(e, Err(err));
                return None;
            }
        };
        let driver = Rc::new(Driver {
            start: e.js_member(&made, "start")?,
            same: e.js_member(&made, "same")?,
        });
        *self.driver.borrow_mut() = Some(driver.clone());
        Some(driver)
    }

    fn same(&self, e: &Engine, a: &Value, b: &Value) -> bool {
        let Some(driver) = self.driver(e) else {
            return false;
        };
        e.try_call_js(&driver.same, &[a.clone(), b.clone()])
            .is_ok_and(|v| matches!(v, Value::Bool(true)))
    }

    fn watch(&self, e: &Engine, me: ObjectId) -> Result<(), EngineError> {
        if !self.watched.borrow_mut().insert(me) {
            return Ok(());
        }
        e.connect(me, SignalIndex::DESTROYED, move |e, _| {
            let tasks = Tasks::of(e);
            tasks.watched.borrow_mut().remove(&me);
            tasks.abort(e, me);
            tasks.entries.borrow_mut().remove(&me);
        })?;
        Ok(())
    }

    /// Aborts `me`'s run in progress, if it has one, and gives its
    /// signal's reason.
    fn abort(&self, e: &Engine, me: ObjectId) -> Option<Value> {
        let run = self.entries.borrow_mut().get_mut(&me)?.run.take()?;
        self.numbers.borrow_mut().remove(&run.number);
        match e.try_call_js(&run.abort, &[]) {
            Ok(reason) => Some(reason),
            Err(err) => {
                warn(e, Err(EngineError::Invoke(err.to_string())));
                Some(Value::Undefined)
            }
        }
    }
}

/// The JS end. `start` calls the run with the arguments and a signal, at
/// once, and settles through the helper as its promise does; a run that
/// throws has failed. Where no `AbortController` is (an engine without
/// `fetch`), the signal is a plain one with the same members.
const DRIVER: &str = r#"(function (helper) {
  "use strict";
  const elements = (args) => {
    if (args === undefined || args === null) return [];
    if (Array.isArray(args)) return args;
    if (typeof args === "object" && typeof args.length === "number") return Array.from(args);
    return [args];
  };
  const abortError = () => {
    if (typeof DOMException === "function") return new DOMException("signal is aborted without reason", "AbortError");
    const error = new Error("signal is aborted without reason");
    error.name = "AbortError";
    return error;
  };
  const plainController = () => {
    const signal = {
      aborted: false,
      reason: undefined,
      onabort: null,
      throwIfAborted() { if (this.aborted) throw this.reason; },
    };
    return {
      signal,
      abort(reason) {
        if (signal.aborted) return;
        signal.aborted = true;
        signal.reason = reason === undefined ? abortError() : reason;
        if (typeof signal.onabort === "function") signal.onabort({ type: "abort", target: signal });
      },
    };
  };
  const start = (number, run, args) => {
    const given = Array.from(elements(args));
    const controller = typeof AbortController === "function" ? new AbortController() : plainController();
    new Promise((resolve) => resolve(run(...given, controller.signal))).then(
      (value) => helper.settle(number, true, value),
      (error) => helper.settle(number, false, error),
    );
    return {
      args: given,
      abort: () => { controller.abort(); return controller.signal.reason; },
    };
  };
  const same = (a, b) => {
    a = elements(a);
    b = elements(b);
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
    return true;
  };
  return { start, same };
})"#;
