//! `app_trace`: what happens to an object over time, as Erlang's
//! observer traces the messages of a process.
//!
//! The engine tells a sink what a traced object is told (its signals, its
//! properties changing, its end; `ljos_engine::Engine::trace`). The sink
//! writes it down here as it happens, with the application's clock, as
//! JSON, so an event holds no object or script value alive. Each client
//! that names itself with a holder has a ring of its own, which only the
//! objects it traces write to and which goes when the client does; traces
//! without a holder share one. A ring's oldest give way and are counted.
//! Nothing is written down for an object that is not traced, and while
//! nothing is, nothing here runs.
//!
//! Asked with `bindings`, a trace also writes down each time a binding of
//! the object runs again, with what it read that changed and whether it
//! gave the value it had: why the object is busy, not only what changed.
//!
//! What a hidden field holds is masked wherever an event repeats it, not
//! only in the field's own events: another object may copy it
//! (`mirror: password.text`). Each event is masked with the secrets the
//! fields hold as it is written down, the events kept are masked again with
//! a secret once it becomes known, and an answer is masked once more.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
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
use super::redact::{SecretBook, mask_json, mask_text};
use super::room::{self, Room};
use super::watch::ChangeFeed;

pub const TRACE: &str = "\
Trace objects like Erlang's observer: `target` starts one; its signals, changes and end (or only \
`names`) are written down with the app's time, and with `bindings` each binding run and its cause. \
A `holder` reads a ring of its own. `since` takes `next_since`; `stop` ends one or all.";

/// The most events kept; older ones give way and are counted.
pub const MAX_KEPT: usize = 1_000;
/// The most events one answer lists unless told otherwise, and at all.
const DEFAULT_MAX: usize = 100;
pub const MAX_EVENTS: usize = 500;
/// The most objects traced at once: each costs a connection per property.
pub const MAX_TRACED: usize = 16;
/// The most names one trace keeps to.
pub const MAX_NAMES: usize = 64;

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
    /// With `target`: also write down each time a binding of the object
    /// runs again, with what it read that changed, how long it took and
    /// whether it gave the value it had.
    #[serde(default)]
    pub bindings: bool,
    /// The `next_since` of an earlier answer: only what came after it.
    #[serde(default)]
    pub since: Option<u64>,
    /// The most events to list, oldest first (default 100, at most 500).
    #[serde(default)]
    pub max: Option<usize>,
    /// A name for the client: the trace ends once no `app_watch` with
    /// this `holder` has waited for a while. With `stop` and no
    /// `target`, stops the traces of this holder only.
    #[serde(default)]
    pub holder: Option<String>,
    /// With `target`: write down only these of its properties and
    /// signals, by name, and its end. All by default.
    #[serde(default)]
    pub names: Vec<String>,
    /// Sent by the surface when a holder lapses: its ring goes too.
    #[serde(skip)]
    #[schemars(skip)]
    pub release: bool,
}

#[derive(Clone)]
struct Event {
    seq: u64,
    at_us: u64,
    object: ObjectId,
    label: String,
    kind: &'static str,
    name: String,
    /// The new value of a change, the arguments of a signal.
    values: Vec<Value>,
    /// The new value of an enum as QML writes it: `Text.AlignRight`.
    key: Option<String>,
    /// What a binding read that changed, how long it ran and whether it
    /// gave the value it had.
    run: Option<Run>,
}

#[derive(Clone)]
struct Run {
    cause: Vec<(ObjectId, String, String)>,
    us: u64,
    same: bool,
}

impl Event {
    /// Masks each of `secrets` (longest first) in every string the event
    /// holds: a label or a cause can repeat a secret as a value can.
    fn mask(&mut self, secrets: &[String]) {
        if secrets.is_empty() {
            return;
        }
        mask_text(&mut self.label, secrets);
        mask_text(&mut self.name, secrets);
        self.values.iter_mut().for_each(|v| mask_json(v, secrets));
        if let Some(key) = self.key.as_mut() {
            mask_text(key, secrets);
        }
        if let Some(run) = self.run.as_mut() {
            for (_, label, prop) in &mut run.cause {
                mask_text(label, secrets);
                mask_text(prop, secrets);
            }
        }
    }
}

/// What one client was told, newest last.
#[derive(Default)]
struct Ring {
    events: VecDeque<Event>,
    dropped: u64,
}

/// One client's trace of an object.
struct Watch {
    holder: Option<String>,
    /// The properties and signals it keeps to; all when empty.
    names: Vec<String>,
    bindings: bool,
}

impl Watch {
    fn wants(&self, event: &Event) -> bool {
        event.kind == "destroyed" || self.names.is_empty() || self.names.contains(&event.name)
    }
}

/// What was written down, shared by every sink the inspector made.
pub(crate) struct TraceLog {
    /// By holder; the traces without one share the ring of `None`.
    rings: HashMap<Option<String>, Ring>,
    /// The number the next event gets; the first is 1.
    next: u64,
    changes: Arc<ChangeFeed>,
    /// Every string of every event is masked (`--mcp-redact`).
    redact_all: bool,
    /// Who traces each object.
    watches: HashMap<ObjectId, Vec<Watch>>,
    /// The secrets the events are masked with.
    book: SecretBook,
    /// The scene's root as of the last request: the hidden fields below it
    /// hold the secrets an event is masked with as it is written down.
    root: Option<ObjectId>,
}

impl TraceLog {
    pub(crate) fn new(changes: Arc<ChangeFeed>, redact_all: bool) -> Rc<RefCell<TraceLog>> {
        Rc::new(RefCell::new(TraceLog {
            rings: HashMap::new(),
            next: 1,
            changes,
            redact_all,
            watches: HashMap::new(),
            book: SecretBook::default(),
            root: None,
        }))
    }

    /// Makes `secrets` known, and masks the events kept with what is new
    /// among them: an event written down before its secret was seen must
    /// not show it once the field lets the secret go.
    pub(crate) fn learn<'a>(&mut self, secrets: impl IntoIterator<Item = &'a String>) {
        let new = self.book.learn(secrets);
        if new.is_empty() {
            return;
        }
        for ring in self.rings.values_mut() {
            ring.events.iter_mut().for_each(|e| e.mask(&new));
        }
    }

    /// Writes `event` into the ring of each client that wants it.
    fn push(&mut self, event: Event) {
        let to: Vec<Option<String>> = self
            .watches
            .get(&event.object)
            .into_iter()
            .flatten()
            .filter(|w| w.wants(&event))
            .map(|w| w.holder.clone())
            .collect();
        if to.is_empty() {
            return;
        }
        for holder in to {
            let ring = self.rings.entry(holder).or_default();
            if ring.events.len() == MAX_KEPT {
                ring.events.pop_front();
                ring.dropped += 1;
            }
            ring.events.push_back(event.clone());
        }
        self.next += 1;
        self.changes.trace();
    }

    /// Whether any client of `obj` asked for its bindings.
    fn bindings(&self, obj: ObjectId) -> bool {
        self.watches
            .get(&obj)
            .is_some_and(|ws| ws.iter().any(|w| w.bindings))
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
        let mut key = None;
        let mut run = None;
        let prop_name = |prop: ljos_engine::PropIndex| {
            meta.as_ref()
                .and_then(|m| m.property(prop))
                .map_or_else(|| format!("#{}", prop.0), |p| p.name.to_string())
        };
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
            Traced::Change { prop, value } => {
                key = engine.key_text(obj, prop, value);
                (
                    "change",
                    prop_name(prop),
                    vec![super::act::to_json(engine, value, 0)],
                )
            }
            Traced::Binding {
                prop,
                cause,
                us,
                same,
            } => {
                let cause = cause
                    .iter()
                    .map(|(o, p)| {
                        let name = engine
                            .meta_of(*o)
                            .and_then(|m| m.property(*p).map(|p| p.name.to_string()))
                            .unwrap_or_else(|| format!("#{}", p.0));
                        (*o, super::observe::label(engine, *o).0, name)
                    })
                    .collect();
                run = Some(Run { cause, us, same });
                ("binding", prop_name(prop), Vec::new())
            }
            Traced::Destroyed => ("destroyed", String::new(), Vec::new()),
        };
        // What the fields hide now, before the event is kept: another
        // object may repeat it, and the field may let it go before anyone
        // reads the trace.
        let (root, redact_all) = {
            let log = log.borrow();
            (log.root, log.redact_all)
        };
        let active = root.map(|root| capture::secrets_below(engine, root, redact_all));
        let mut log = log.borrow_mut();
        if let Some(active) = &active {
            log.learn(active);
        }
        // What a field that hides its text was told is masked as it is
        // written down: what it held then is not what it holds when read.
        let masked =
            log.redact_all || (kind != "destroyed" && capture::hides_its_text(engine, obj));
        let values = if masked {
            values.into_iter().map(mask_strings).collect()
        } else {
            values
        };
        let mut event = Event {
            seq: log.next,
            at_us: engine.clock().now().as_micros(),
            object: obj,
            label: super::observe::label(engine, obj).0,
            kind,
            name,
            values,
            key: key.filter(|_| !masked),
            run,
        };
        event.mask(log.book.secrets());
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
///
/// `root` is the scene's: what the fields below it hide is masked in what
/// is written down and told.
pub(crate) fn trace(
    engine: &Engine,
    root: Option<ObjectId>,
    log: &Rc<RefCell<TraceLog>>,
    args: &TraceArgs,
    target: Option<ObjectId>,
    per_copy: usize,
) -> Result<ToolReply, SurfaceError> {
    if args.bindings && (target.is_none() || args.stop) {
        return Err(SurfaceError::invalid_argument(
            "`app_trace`: `bindings` goes with a `target` to start tracing",
        ));
    }
    // Read with the log let go: a read can run a binding a trace writes down.
    let redact_all = log.borrow().redact_all;
    let now = root.map(|root| capture::secrets_below(engine, root, redact_all));
    {
        let mut log = log.borrow_mut();
        log.root = root;
        if let Some(now) = &now {
            log.learn(now);
        }
    }
    let traced = engine.traced();
    // An object that ended is traced no longer, whoever held it.
    log.borrow_mut().watches.retain(|o, _| traced.contains(o));
    match (target, args.stop) {
        (Some(obj), false) => {
            if !traced.contains(&obj) && traced.len() >= MAX_TRACED {
                return Err(SurfaceError::invalid_argument(format!(
                    "`app_trace`: {MAX_TRACED} objects are traced already; stop one first"
                )));
            }
            if let Some(name) = args.names.iter().find(|n| {
                engine.property_index(obj, n).is_err() && engine.signal_index(obj, n).is_err()
            }) {
                return Err(SurfaceError::invalid_argument(format!(
                    "`app_trace`: {} has no property or signal `{name}`",
                    super::observe::label(engine, obj).0
                )));
            }
            // An object traced already keeps its connections, so another
            // client's trace of it loses nothing.
            if !traced.contains(&obj) {
                engine
                    .trace(obj, sink(log))
                    .map_err(|e| SurfaceError::invalid_argument(format!("`app_trace`: {e}")))?;
            }
            let bindings = {
                let mut log = log.borrow_mut();
                let ws = log.watches.entry(obj).or_default();
                ws.retain(|w| w.holder != args.holder);
                ws.push(Watch {
                    holder: args.holder.clone(),
                    names: args.names.clone(),
                    bindings: args.bindings,
                });
                log.bindings(obj)
            };
            engine.trace_bindings(obj, bindings);
        }
        (Some(obj), true) => {
            let left = {
                let mut log = log.borrow_mut();
                // Without a holder the agent stops the object for everyone.
                match &args.holder {
                    Some(h) => {
                        if let Some(ws) = log.watches.get_mut(&obj) {
                            ws.retain(|w| w.holder.as_ref() != Some(h));
                        }
                    }
                    None => {
                        log.watches.remove(&obj);
                    }
                }
                let left = log.watches.get(&obj).is_some_and(|ws| !ws.is_empty());
                if !left {
                    log.watches.remove(&obj);
                }
                left.then(|| log.bindings(obj))
            };
            match left {
                Some(bindings) => {
                    engine.trace_bindings(obj, bindings);
                }
                None => {
                    engine.untrace(obj);
                }
            }
        }
        (None, true) => {
            let mut rebind = Vec::new();
            let stop = {
                let mut log = log.borrow_mut();
                match &args.holder {
                    Some(h) => {
                        let mut gone = Vec::new();
                        for (o, ws) in &mut log.watches {
                            let before = ws.len();
                            ws.retain(|w| w.holder.as_ref() != Some(h));
                            if ws.is_empty() {
                                gone.push(*o);
                            } else if ws.len() != before {
                                rebind.push(*o);
                            }
                        }
                        for o in &gone {
                            log.watches.remove(o);
                        }
                        if args.release {
                            log.rings.remove(&Some(h.clone()));
                        }
                        gone
                    }
                    None => {
                        log.watches.clear();
                        traced
                    }
                }
            };
            for obj in stop {
                engine.untrace(obj);
            }
            for obj in rebind {
                let bindings = log.borrow().bindings(obj);
                engine.trace_bindings(obj, bindings);
            }
        }
        (None, false) => {}
    }
    let log = log.borrow();
    // A holder is shown what it traces; an agent without one, everything.
    let names_of = |o: ObjectId| -> Option<Vec<String>> {
        match log.watches.get(&o) {
            Some(ws) => ws
                .iter()
                .find(|w| w.holder == args.holder)
                .map(|w| w.names.clone())
                .or_else(|| args.holder.is_none().then(Vec::new)),
            None => args.holder.is_none().then(Vec::new),
        }
    };
    let mut tracing: Vec<(ObjectId, String, Vec<String>)> = engine
        .traced()
        .into_iter()
        .filter_map(|o| {
            names_of(o).map(|names| {
                let mut label = super::observe::label(engine, o).0;
                log.book.mask_text(&mut label);
                (o, label, names)
            })
        })
        .collect();
    let follows = |o: ObjectId| engine.traces_bindings(o);
    tracing.sort_by(|a, b| a.1.cmp(&b.1));
    let since = args.since.unwrap_or(0);
    let max = args.max.unwrap_or(DEFAULT_MAX);
    let ring = log.rings.get(&args.holder);
    let dropped = ring.map_or(0, |r| r.dropped);
    let after: Vec<&Event> = ring
        .into_iter()
        .flat_map(|r| r.events.iter())
        .filter(|e| e.seq > since)
        .collect();
    let mut text = String::new();
    for (o, label, names) in &tracing {
        let too = if follows(*o) { ", bindings too" } else { "" };
        let only = if names.is_empty() {
            String::new()
        } else {
            format!(", only {}", names.join(", "))
        };
        let _ = writeln!(text, "tracing {label}{only}{too}");
    }
    let tracing_json = || -> Vec<Value> {
        tracing
            .iter()
            .map(|(o, label, names)| {
                json!({
                    "handle": handle(*o),
                    "name": label,
                    "bindings": follows(*o),
                    "names": names,
                })
            })
            .collect()
    };
    let row = |e: &Event| -> (Value, String) {
        let mut line = String::new();
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
                let shown = match &e.key {
                    Some(key) => {
                        v["key"] = json!(key);
                        format!("{key} ({})", v["value"])
                    }
                    None => v["value"].to_string(),
                };
                let _ = writeln!(
                    line,
                    "{:>4} {ms:>10.1} ms  {} .{} = {shown}",
                    e.seq, e.label, e.name
                );
            }
            "binding" => {
                let Some(run) = &e.run else {
                    return (v, line);
                };
                v["name"] = json!(e.name);
                v["cause"] = run
                        .cause
                        .iter()
                        .map(|(o, label, p)| {
                            json!({ "handle": handle(*o), "object": label, "property": p })
                        })
                        .collect();
                v["us"] = json!(run.us);
                v["same"] = json!(run.same);
                let cause: Vec<String> = run
                    .cause
                    .iter()
                    .map(|(_, l, p)| format!("{l}.{p}"))
                    .collect();
                let _ = writeln!(
                    line,
                    "{:>4} {ms:>10.1} ms  {} binding {} ← {} ({} µs){}",
                    e.seq,
                    e.label,
                    e.name,
                    if cause.is_empty() {
                        "?".to_string()
                    } else {
                        cause.join(", ")
                    },
                    run.us,
                    if run.same {
                        ", gave the value it had"
                    } else {
                        ""
                    }
                );
            }
            "signal" => {
                v["name"] = json!(e.name);
                v["args"] = Value::Array(e.values.clone());
                let args: Vec<String> = e.values.iter().map(Value::to_string).collect();
                let _ = writeln!(
                    line,
                    "{:>4} {ms:>10.1} ms  {} {}({})",
                    e.seq,
                    e.label,
                    e.name,
                    args.join(", ")
                );
            }
            _ => {
                let _ = writeln!(line, "{:>4} {ms:>10.1} ms  {} destroyed", e.seq, e.label);
            }
        }
        (v, line)
    };
    // What does not fit in this answer is the next one's: `next_since`
    // stops at the last event told.
    let mut room = Room::new(
        per_copy,
        &json!({ "tracing": tracing_json(), "events": [], "next_since": log.next, "more": true }),
        &text,
    );
    let mut events: Vec<Value> = Vec::new();
    let mut told = None;
    let kept = room::rows(&mut room, &after[..after.len().min(max)], false, |e| {
        let (mut v, mut line) = row(e);
        log.book.mask_json(&mut v);
        log.book.mask_text(&mut line);
        (v, line)
    });
    for (e, v, line) in kept {
        text.push_str(&line);
        if !line.ends_with('\n') {
            text.push('\n');
        }
        events.push(v);
        told = Some(e.seq);
    }
    let more = after.len() > events.len();
    let next_since = told.unwrap_or(log.next - 1);
    if events.is_empty() {
        text.push_str("nothing written down since then\n");
    }
    if dropped > 0 {
        let _ = writeln!(text, "{dropped} older events gave way");
    }
    let value = json!({
        "tracing": tracing_json(),
        "events": events,
        "next_since": next_since,
        "more": more,
        "dropped": dropped,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(label: &str, values: Vec<Value>, cause: &str) -> Event {
        Event {
            seq: 1,
            at_us: 0,
            object: ObjectId::default(),
            label: label.to_string(),
            kind: "change",
            name: "copy".to_string(),
            values,
            key: Some(format!("Key.{label}")),
            run: Some(Run {
                cause: vec![(ObjectId::default(), cause.to_string(), "text".to_string())],
                us: 1,
                same: false,
            }),
        }
    }

    #[test]
    fn an_event_is_masked_in_every_string_it_holds() {
        let mut e = event(
            "Item \"hunter2\"",
            vec![
                json!("hunter2"),
                json!({ "said": ["was hunter2"] }),
                json!(7),
            ],
            "hunter2 field",
        );
        e.mask(&["hunter2".to_string()]);
        assert_eq!(e.label, "Item \"<redacted>\"");
        assert_eq!(e.values[0], json!("<redacted>"));
        assert_eq!(e.values[1], json!({ "said": ["was <redacted>"] }));
        assert_eq!(e.values[2], json!(7));
        assert_eq!(e.key.as_deref(), Some("Key.Item \"<redacted>\""));
        assert_eq!(e.run.unwrap().cause[0].1, "<redacted> field");
    }

    #[test]
    fn a_secret_learnt_later_masks_the_events_kept_and_is_not_forgotten() {
        let log = TraceLog::new(ChangeFeed::new(), false);
        let mut log = log.borrow_mut();
        log.rings.entry(None).or_default().events.push_back(event(
            "mirror",
            vec![json!("hunter2")],
            "field",
        ));
        log.learn(&["hunter2".to_string()]);
        // The field lets it go; what was written down stays masked.
        log.learn(&["swordfish".to_string()]);
        let kept = &log.rings[&None].events[0];
        assert_eq!(kept.values[0], json!("<redacted>"));
        assert_eq!(log.book.secrets().len(), 2);
    }

    #[test]
    fn an_ordinary_event_is_told_as_it_was() {
        let mut e = event("mirror", vec![json!("hello")], "field");
        e.mask(&["hunter2".to_string()]);
        assert_eq!(e.values[0], json!("hello"));
        assert_eq!(e.label, "mirror");
    }
}
