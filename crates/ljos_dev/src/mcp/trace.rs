//! `app_trace`: what happens to an object over time, as Erlang's
//! observer traces the messages of a process.
//!
//! The engine tells a sink what a traced object is told (its signals, its
//! properties changing, its end; `ljos_engine::Engine::trace`). The sink
//! writes it down here as it happens, with the application's clock, as
//! JSON, so an event holds no object or script value alive. The list is a
//! ring: the oldest give way and are counted. Nothing is written down for
//! an object that is not traced, and while nothing is, nothing here runs.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Engine, ObjectId, TraceSink, Traced};
use ljos_mcp_core::{SurfaceError, ToolReply};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::capture::{self, REDACTED};
use super::objects::handle;
use super::watch::ChangeFeed;

pub const TRACE: &str = "\
Trace objects over time, like Erlang's observer: `target` (a ref, locator or @handle) starts one; \
from then its signals, property changes with new values and its end are written down with the \
app's time. `since` takes an answer's `next_since`; `stop` ends one or all.";

/// The most events kept; older ones give way and are counted.
pub const MAX_KEPT: usize = 1_000;
/// The most events one answer lists unless told otherwise, and at all.
const DEFAULT_MAX: usize = 100;
pub const MAX_EVENTS: usize = 500;
/// The most objects traced at once: each costs a connection per property.
pub const MAX_TRACED: usize = 16;

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TraceArgs {
    /// Start tracing this object (a reference, locator or `@handle`), or
    /// with `stop`, stop tracing it.
    #[serde(default)]
    pub target: Option<String>,
    /// Stop tracing `target`, or every object without one. What was
    /// written down stays.
    #[serde(default)]
    pub stop: bool,
    /// The `next_since` of an earlier answer: only what came after it.
    #[serde(default)]
    pub since: Option<u64>,
    /// The most events to list, oldest first (default 100, at most 500).
    #[serde(default)]
    pub max: Option<usize>,
}

struct Event {
    seq: u64,
    at_us: u64,
    object: ObjectId,
    label: String,
    kind: &'static str,
    name: String,
    /// The new value of a change, the arguments of a signal.
    values: Vec<Value>,
}

/// What was written down, shared by every sink the inspector made.
pub(crate) struct TraceLog {
    events: VecDeque<Event>,
    /// The number the next event gets; the first is 1.
    next: u64,
    dropped: u64,
    changes: Arc<ChangeFeed>,
    /// Every string of every event is masked (`--mcp-redact`).
    redact_all: bool,
}

impl TraceLog {
    pub(crate) fn new(changes: Arc<ChangeFeed>, redact_all: bool) -> Rc<RefCell<TraceLog>> {
        Rc::new(RefCell::new(TraceLog {
            events: VecDeque::new(),
            next: 1,
            dropped: 0,
            changes,
            redact_all,
        }))
    }

    fn push(&mut self, event: Event) {
        if self.events.len() == MAX_KEPT {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
        self.next += 1;
        self.changes.trace();
    }
}

/// The sink the engine tells about the objects the inspector traces.
fn sink(log: &Rc<RefCell<TraceLog>>) -> TraceSink {
    let log = Rc::downgrade(log);
    Rc::new(move |engine: &Engine, obj: ObjectId, what: Traced<'_>| {
        let Some(log) = log.upgrade() else {
            return;
        };
        let meta = engine.meta_of(obj);
        let (kind, name, values): (_, String, Vec<Value>) = match what {
            Traced::Signal { signal, args } => (
                "signal",
                meta.as_ref()
                    .and_then(|m| m.signal(signal))
                    .map_or_else(|| format!("#{}", signal.0), |s| s.name.to_string()),
                args.iter()
                    .map(|a| super::act::to_json(engine, a, 0))
                    .collect(),
            ),
            Traced::Change { prop, value } => (
                "change",
                meta.as_ref()
                    .and_then(|m| m.property(prop))
                    .map_or_else(|| format!("#{}", prop.0), |p| p.name.to_string()),
                vec![super::act::to_json(engine, value, 0)],
            ),
            Traced::Destroyed => ("destroyed", String::new(), Vec::new()),
        };
        let mut log = log.borrow_mut();
        // What a field that hides its text was told is masked as it is
        // written down: what it held then is not what it holds when read.
        let masked =
            log.redact_all || (kind != "destroyed" && capture::hides_its_text(engine, obj));
        let values = if masked {
            values.into_iter().map(mask_strings).collect()
        } else {
            values
        };
        let event = Event {
            seq: log.next,
            at_us: engine.clock().now().as_micros(),
            object: obj,
            label: super::observe::label(engine, obj).0,
            kind,
            name,
            values,
        };
        log.push(event);
    })
}

fn mask_strings(v: Value) -> Value {
    match v {
        Value::String(_) => json!(REDACTED),
        Value::Array(items) => Value::Array(items.into_iter().map(mask_strings).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, mask_strings(v))).collect())
        }
        other => other,
    }
}

/// Starts or stops what `args` asks, then answers with what is traced and
/// what was written down after `since`.
pub(crate) fn trace(
    engine: &Engine,
    log: &Rc<RefCell<TraceLog>>,
    args: &TraceArgs,
    target: Option<ObjectId>,
) -> Result<ToolReply, SurfaceError> {
    match (target, args.stop) {
        (Some(obj), false) => {
            let traced = engine.traced();
            if !traced.contains(&obj) && traced.len() >= MAX_TRACED {
                return Err(SurfaceError::invalid_argument(format!(
                    "`app_trace`: {MAX_TRACED} objects are traced already; stop one first"
                )));
            }
            engine
                .trace(obj, sink(log))
                .map_err(|e| SurfaceError::invalid_argument(format!("`app_trace`: {e}")))?;
        }
        (Some(obj), true) => {
            engine.untrace(obj);
        }
        (None, true) => {
            for obj in engine.traced() {
                engine.untrace(obj);
            }
        }
        (None, false) => {}
    }
    let mut tracing: Vec<(ObjectId, String)> = engine
        .traced()
        .into_iter()
        .map(|o| (o, super::observe::label(engine, o).0))
        .collect();
    tracing.sort_by(|a, b| a.1.cmp(&b.1));
    let log = log.borrow();
    let since = args.since.unwrap_or(0);
    let max = args.max.unwrap_or(DEFAULT_MAX);
    let after: Vec<&Event> = log.events.iter().filter(|e| e.seq > since).collect();
    let shown = &after[..after.len().min(max)];
    let more = after.len() > shown.len();
    let next_since = shown.last().map_or(log.next - 1, |e| e.seq);
    let mut text = String::new();
    for (_, label) in &tracing {
        let _ = writeln!(text, "tracing {label}");
    }
    let events: Vec<Value> = shown
        .iter()
        .map(|e| {
            let mut v = json!({
                "seq": e.seq,
                "at_us": e.at_us,
                "handle": handle(e.object),
                "object": e.label,
                "kind": e.kind,
            });
            let ms = e.at_us as f64 / 1000.0;
            match e.kind {
                "change" => {
                    v["name"] = json!(e.name);
                    v["value"] = e.values.first().cloned().unwrap_or(Value::Null);
                    let _ = writeln!(
                        text,
                        "{:>4} {ms:>10.1} ms  {} .{} = {}",
                        e.seq, e.label, e.name, v["value"]
                    );
                }
                "signal" => {
                    v["name"] = json!(e.name);
                    v["args"] = Value::Array(e.values.clone());
                    let args: Vec<String> = e.values.iter().map(Value::to_string).collect();
                    let _ = writeln!(
                        text,
                        "{:>4} {ms:>10.1} ms  {} {}({})",
                        e.seq,
                        e.label,
                        e.name,
                        args.join(", ")
                    );
                }
                _ => {
                    let _ = writeln!(text, "{:>4} {ms:>10.1} ms  {} destroyed", e.seq, e.label);
                }
            }
            v
        })
        .collect();
    if events.is_empty() {
        text.push_str("nothing written down since then\n");
    }
    if log.dropped > 0 {
        let _ = writeln!(text, "{} older events gave way", log.dropped);
    }
    let value = json!({
        "tracing": tracing
            .iter()
            .map(|(o, label)| json!({ "handle": handle(*o), "name": label }))
            .collect::<Vec<_>>(),
        "events": events,
        "next_since": next_since,
        "more": more,
        "dropped": log.dropped,
    });
    Ok(ToolReply::from(value).with_text(text))
}

/// Whether answering needs the scene looked at: a target given by locator
/// is found in the outline.
pub(crate) fn needs_scene(args: &TraceArgs) -> bool {
    args.target
        .as_deref()
        .is_some_and(|t| !t.trim_start().starts_with('@'))
}
