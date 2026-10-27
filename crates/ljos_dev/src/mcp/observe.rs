//! `app_processes`: the application as an Erlang observer sees a node
//! (ARCHITECTURE.md 3.4), what does work and, as the observer shows each
//! process's message queue with it, the mailboxes it is handed through.
//!
//! The threads, memory and CPU time are the platform's, read by
//! [`super::process`] from what the kernel keeps anyway, so listing them
//! costs nothing until asked. Where a platform does not tell a value, the
//! answer says so instead of giving 0. The UI thread's
//! own processes (Timers, animations, the owners the scheduler charges
//! time to) are read where they live, on the UI thread. The mailboxes
//! are read from [`ljos_runtime::observe`] on the server's thread, so a
//! UI thread that is stuck still shows its inbox filling up.

use std::fmt::Write as _;
use std::time::Instant;

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::ToolReply;
use ljos_runtime::observe::{self, MailboxSample};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::args::{Act, Kind, Resource};
use super::process::{NAME_KEPT, Process, current_tid};
use super::redact::SecretBook;
use super::room::Room;
use super::surface::DevReq;
use crate::top::{FrameStats, owner_name};

pub const PROCESSES: &str = "\
What does work, like Erlang's observer: threads (state, CPU time, wakeups), Timers, animations, \
scheduler owners, the supervision tree, the last faults and what took each, and the UI thread's \
mailboxes (depth, age, refused), even when stuck. `kind` keeps one; `waker`: what will wake it.";

/// The most front messages `app_processes` names per inbox.
pub const MAX_PEEK: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    /// OS threads of the process, the UI thread first.
    Thread,
    /// Running `Timer`s.
    Timer,
    /// Animations under way.
    Animation,
    /// What the frame scheduler charged time to: the application, error
    /// boundaries, the inspector.
    Owner,
    /// The bounded inboxes threads reach the UI thread through. Read
    /// without the UI thread, so a stuck one still shows its inbox.
    Mailbox,
    /// The supervision tree: the standard supervisors and every live
    /// boundary, under the one around it.
    Supervisor,
    /// The last faults (at most 32), in `faults`: where each happened and
    /// what took it.
    Fault,
    /// What will wake the UI thread, each with its owner and where it is
    /// declared: running Timers and animations, answers awaited (fetches,
    /// files, media, dialogs), pictures loading, posted messages, and what
    /// listens (file watchers). Only when asked for by name.
    Waker,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessesArgs {
    /// List only this kind: `thread`, `timer`, `animation`, `owner`,
    /// `mailbox`, `supervisor`, `fault`, or `waker` (what will wake the UI
    /// thread, listed only when asked for).
    #[serde(default)]
    pub kind: Option<ProcessKind>,
    /// List only the mailboxes of this name (`application`, `platform`,
    /// `images`, `inspector`, `reload`).
    #[serde(default)]
    pub mailbox: Option<String>,
    /// Name the kinds of this many front messages per mailbox (at most 20,
    /// default 0). Never their contents.
    #[serde(default)]
    pub peek: Option<usize>,
}

impl ProcessesArgs {
    /// Whether the mailboxes are listed.
    pub(crate) fn wants_mailboxes(&self) -> bool {
        self.kind.is_none_or(|k| k == ProcessKind::Mailbox)
    }

    /// Whether anything is asked of the UI thread.
    pub(crate) fn wants_the_ui_thread(&self) -> bool {
        self.kind != Some(ProcessKind::Mailbox)
    }
}

/// The kind of an inspector request, for its own inbox's counts: the tool
/// it came from.
pub(crate) fn request_kind(req: &DevReq) -> &'static str {
    match &req.kind {
        Kind::Status(_) => "app_status",
        Kind::Snapshot(_) => "app_snapshot",
        Kind::Find(_) => "app_find",
        Kind::Inspect(_) => "app_inspect",
        Kind::Screenshot(_) => "app_screenshot",
        Kind::Logs(_) => "app_logs",
        Kind::Metrics => "app_metrics",
        Kind::Processes(_) => "app_processes",
        Kind::Watch(_) => "app_watch",
        Kind::Objects(_) => "app_objects",
        Kind::Trace(_) => "app_trace",
        Kind::Frames(_) => "app_frames",
        Kind::Audit(_) => "app_audit",
        Kind::Memory(_) => "app_metrics",
        Kind::Release(_) => "holder release",
        Kind::Resource(r) => match r {
            Resource::Tree => "resource tree",
            Resource::Logs => "resource logs",
            Resource::Metrics => "resource metrics",
            Resource::Reload => "resource reload",
        },
        Kind::Act(act) => match act {
            Act::Click(_) => "app_click",
            Act::Type(_) => "app_type",
            Act::Press(_) => "app_press",
            Act::Scroll(_) => "app_scroll",
            Act::Touch(_) => "app_touch",
            Act::Advance(_) => "app_advance",
            Act::Resize(_) => "app_resize",
            Act::Appearance(_) => "app_set_appearance",
            Act::SetProperty(_) => "app_set_property",
            Act::Invoke(_) => "app_invoke",
            Act::Reload => "app_reload",
            Act::Eval(_) => "app_eval",
            Act::Batch(_) => "app_batch",
        },
    }
}

fn micros(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// The mailboxes `args` asks for and their lines, read on the server's
/// thread.
fn mailbox_rows(args: &ProcessesArgs) -> (Vec<Value>, String) {
    let peek = args.peek.unwrap_or(0).min(MAX_PEEK);
    let all: Vec<MailboxSample> = observe::mailboxes(peek)
        .into_iter()
        .filter(|m| args.mailbox.as_deref().is_none_or(|n| n == m.name))
        .collect();
    let rows: Vec<Value> = all
        .iter()
        .map(|m| {
            let mut row = json!({
                "name": m.name,
                "depth": m.depth,
                "capacity": m.capacity,
                "oldest_us": m.oldest.map(micros),
                "sent": m.sent,
                "refused": m.refused,
                "replaced": m.replaced,
                "drained": m.drained,
                "senders": m.senders,
                "waiting_senders": m.waiting,
                "kinds": m.kinds.iter().map(|(k, n)| json!({ "kind": k, "count": n })).collect::<Vec<_>>(),
            });
            if peek > 0 {
                row["peek"] = json!(m.peek);
            }
            row
        })
        .collect();
    let mut text = String::new();
    for m in &all {
        let _ = write!(
            text,
            "mailbox {}: {}/{} queued, sent {}, refused {}, replaced {}, drained {}, {} senders",
            m.name, m.depth, m.capacity, m.sent, m.refused, m.replaced, m.drained, m.senders
        );
        if m.waiting > 0 {
            let _ = write!(text, " ({} waiting for room)", m.waiting);
        }
        if let Some(oldest) = m.oldest {
            let _ = write!(text, ", oldest {:.1} ms", oldest.as_secs_f64() * 1000.0);
        }
        if !m.kinds.is_empty() {
            let kinds: Vec<String> = m.kinds.iter().map(|(k, n)| format!("{k} {n}")).collect();
            let _ = write!(text, " [{}]", kinds.join(", "));
        }
        text.push('\n');
    }
    if all.is_empty() {
        text.push_str("no mailbox by that name\n");
    }
    text.push_str("Mailbox counts are since the inspector attached.");
    (rows, text)
}

/// Adds the mailboxes `args` asks for to `reply`, the answer of the UI
/// thread (or, when it gave none, an empty one).
pub(crate) fn with_mailboxes(
    mut reply: ToolReply,
    args: &ProcessesArgs,
    per_copy: usize,
) -> ToolReply {
    if !args.wants_mailboxes() {
        return reply;
    }
    let (rows, lines) = mailbox_rows(args);
    if reply.structured.get("processes").is_none() {
        reply.structured["processes"] = json!([]);
    }
    reply.structured["mailboxes"] = Value::Array(rows);
    reply.structured["counted_since"] = json!("inspector attached");
    let mut text = match reply.text.take() {
        Some(t) if !t.is_empty() => format!("{t}\n{lines}"),
        _ => lines,
    };
    // The UI thread fitted its rows before the mailboxes came.
    fit_rows(&mut reply.structured, &mut text, per_copy);
    reply.with_text(text)
}

/// Brings the rows of an `app_processes` answer within `per_copy`, with
/// the rest of it as it is. What does not fit is counted, not sent. The
/// other threads of the process give way first: the application's own
/// timers, animations and supervisors are what an agent looks here for.
fn fit_rows(value: &mut Value, text: &mut String, per_copy: usize) {
    let Some(Value::Array(rows)) = value
        .get_mut("processes")
        .map(|r| std::mem::replace(r, json!([])))
    else {
        return;
    };
    let mut room = Room::new(per_copy, value, "");
    let total = rows.len();
    let other_thread = |row: &Value| row["kind"] == "thread" && row.get("thread_name").is_none();
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by_key(|&i| other_thread(&rows[i]));
    let mut keep = vec![false; total];
    for i in order {
        if !room.take(&rows[i], "") {
            break;
        }
        keep[i] = true;
    }
    let rows: Vec<Value> = rows
        .into_iter()
        .zip(keep)
        .filter_map(|(row, kept)| kept.then_some(row))
        .collect();
    let left_out = total - rows.len();
    value["processes"] = Value::Array(rows);
    if left_out > 0 {
        let before = value["processes_left_out"].as_u64().unwrap_or(0);
        value["processes_left_out"] = json!(before + left_out as u64);
        // First, where a text cut to the budget still has it.
        text.insert_str(
            0,
            &format!("{left_out} more not listed; `kind` lists one kind\n"),
        );
    }
}

/// Where `app_processes` reads the process from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessSource {
    /// What the platform tells: its threads, memory and CPU time.
    #[default]
    Platform,
    /// Nothing, as on a platform Ljós cannot read them on: what the
    /// answers say then can be seen on any.
    Unmeasured,
}

/// The names the kernel cut to 15 bytes, whole again.
const WHOLE_NAMES: [(&str, &str); 7] = [
    ("zbus::Connectio", "zbus::Connection"),
    ("ljos-screen-rea", "ljos-screen-reader"),
    ("ljos-desktop-fo", "ljos-desktop-fonts"),
    ("ljos-dev-watche", "ljos-dev-watcher"),
    ("ljos-compute-wo", "ljos-compute-worker"),
    ("ljos-mcp-server", "ljos-mcp-server"),
    ("ljos-settings-w", "ljos-settings-watcher"),
];

/// The name a thread was given, as far as it can be told, and whether the
/// kernel cut it: it keeps [`NAME_KEPT`] bytes.
fn whole_name(name: &str) -> (String, bool) {
    if let Some((_, whole)) = WHOLE_NAMES.iter().find(|(cut, _)| *cut == name) {
        return ((*whole).to_owned(), false);
    }
    (name.to_owned(), name.len() == NAME_KEPT)
}

/// What a thread is for, from the name it was given (cut to 15 bytes by
/// the kernel).
fn thread_kind(name: &str, ui: bool) -> &'static str {
    if ui {
        return "engine";
    }
    let table: [(&str, &str); 21] = [
        ("ljos-compute", "pool"),
        ("ljos-mcp", "inspector"),
        ("tokio", "tokio"),
        ("ljos-media", "media"),
        ("ljos-image", "images"),
        ("ljos-settings", "settings"),
        ("ljos-desktop-fo", "fonts"),
        ("ljos-screen-rea", "screen_reader"),
        ("ljos-fetch", "network"),
        ("ljos-request", "network"),
        ("ljos-dialog", "platform"),
        ("ljos-open-url", "platform"),
        ("ljos-badge", "platform"),
        ("ljos-gpu", "gpu"),
        ("ljos-dev-watche", "reload"),
        ("ljos-", "worker"),
        // A D-Bus connection (accessibility, portals, settings) and the
        // executor and blocking pool it runs on.
        ("zbus", "dbus"),
        ("async-io", "dbus"),
        ("blocking-", "dbus"),
        ("gdbus", "dbus"),
        // Mesa's drawing threads when the device is the CPU.
        ("llvmpipe", "gpu"),
    ];
    table
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map_or("other", |(_, kind)| kind)
}

fn thread_state(state: char) -> &'static str {
    match state {
        'R' => "busy",
        'S' | 'I' => "idle",
        'D' => "blocked",
        'T' | 't' => "stopped",
        _ => "gone",
    }
}

/// `Timer#tick`, `NumberAnimation "fade"`, `Timer (Clock.qml:42)`: what
/// to call an object, and a target any tool takes for it.
pub(crate) fn label(engine: &Engine, obj: ObjectId) -> (String, Option<String>) {
    let Some(name) = engine.label_of(obj) else {
        return ("object".to_owned(), Some(super::objects::handle(obj)));
    };
    let object_name = engine
        .untrack(|| engine.read_by_name(obj, "objectName"))
        .to_js_string()
        .to_string();
    let locator = if !object_name.is_empty() && object_name != "undefined" {
        format!("#{object_name}")
    } else {
        match engine.inspect_object(obj).and_then(|r| r.id) {
            Some(id) => format!("id={id}"),
            None => super::objects::handle(obj),
        }
    };
    (name, Some(locator))
}

/// The process as a whole: its threads, its memory, the CPU it used.
/// What the platform does not tell is named in `not_measured`.
fn process_block(process: &Process) -> Value {
    let threads = process.threads.as_ref().map(Vec::len);
    let mut block = json!({ "pid": std::process::id(), "threads": threads });
    let mut unmeasured = Vec::new();
    if threads.is_none() {
        unmeasured.push("threads");
    }
    match process.resident {
        Some(r) => {
            block["rss_bytes"] = json!(r.rss);
            block["own_bytes"] = json!(r.own());
            block["shared_bytes"] = json!(r.shared);
            block["virtual_bytes"] = json!(r.virt);
        }
        None => unmeasured.push("rss_bytes"),
    }
    match process.cpu_us {
        Some(us) => block["cpu_us"] = json!(us),
        None => unmeasured.push("cpu_us"),
    }
    if let Some(w) = process.wakeups {
        block["wakeups"] = json!(w);
    }
    if !unmeasured.is_empty() {
        block["not_measured"] = json!(unmeasured);
    }
    block
}

/// The line `app_processes` ends with: the process, and what of it this
/// platform does not tell.
fn process_text(block: &Value, text: &mut String) {
    let mut parts = Vec::new();
    if let Some(n) = block["threads"].as_u64() {
        parts.push(format!("{n} threads"));
    }
    if let Some(rss) = block["rss_bytes"].as_u64() {
        parts.push(format!(
            "{:.1} MiB resident",
            rss as f64 / (1024.0 * 1024.0)
        ));
    }
    if let Some(us) = block["cpu_us"].as_u64() {
        parts.push(format!("cpu {:.1} ms", us as f64 / 1000.0));
    }
    if let Some(w) = block["wakeups"].as_u64() {
        parts.push(format!("{w} wakeups"));
    }
    let _ = write!(text, "process {}", block["pid"]);
    if !parts.is_empty() {
        let _ = write!(text, ": {}", parts.join(", "));
    }
    text.push('\n');
    let unmeasured: Vec<&str> = block["not_measured"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| match f.as_str()? {
            "threads" => Some("threads"),
            "rss_bytes" => Some("memory"),
            "cpu_us" => Some("CPU time"),
            _ => None,
        })
        .collect();
    if !unmeasured.is_empty() {
        let _ = writeln!(
            text,
            "{}: not measured on this platform",
            unmeasured.join(", ")
        );
    }
}

/// Where `obj` is declared, `file:line:column`.
pub(crate) fn declared_at(engine: &Engine, obj: ObjectId) -> Option<String> {
    engine
        .declared_at(obj)
        .map(|at| at.trim_end_matches(": ").to_owned())
}

/// One row of what will wake the UI thread: `source` is what it is,
/// `owner` the object it is for.
fn waker(engine: &Engine, source: &str, owner: Option<ObjectId>, scheduled: bool) -> Value {
    let mut row = json!({ "kind": "waker", "source": source, "scheduled": scheduled });
    if let Some(o) = owner.filter(|o| engine.is_alive(*o)) {
        let (name, locator) = label(engine, o);
        row["name"] = json!(name);
        row["handle"] = json!(super::objects::handle(o));
        row["locator"] = json!(locator);
        row["at"] = json!(declared_at(engine, o));
    }
    row
}

fn owner_text(row: &Value) -> String {
    let name = row["name"].as_str().unwrap_or("");
    match row["at"].as_str() {
        Some(at) => format!("{name} at {at}"),
        None if name.is_empty() => "the application".to_owned(),
        None => name.to_owned(),
    }
}

/// What will wake the UI thread, and its lines: rows that are scheduled
/// (a Timer due, an animation running, an answer under way, a message
/// posted) and rows that listen for news that may never come (a file
/// watcher, a screen reader that may ask). A resting application has
/// none of the first.
fn waker_rows(engine: &Engine, activity: &ProcessActivity, text: &mut String) -> Vec<Value> {
    let mut rows = Vec::new();
    let now = engine.now();
    for t in engine.running_timers() {
        let due_in = t.due.map(|d| d.saturating_since(now));
        let mut row = waker(engine, "timer", Some(t.object), t.due.is_some());
        row["interval_ms"] = json!(t.interval);
        row["repeat"] = json!(t.repeat);
        row["due_in_us"] = json!(due_in.map(micros));
        let _ = writeln!(
            text,
            "timer {}: every {} ms{}, {}",
            owner_text(&row),
            t.interval,
            if t.repeat { "" } else { " once" },
            match due_in {
                Some(d) => format!("next in {:.1} ms", d.as_secs_f64() * 1000.0),
                None => "frozen".to_owned(),
            }
        );
        rows.push(row);
    }
    for (anim, paused) in ljos_quick::running_animations(engine) {
        if paused {
            continue;
        }
        let row = waker(engine, "animation", Some(anim), true);
        let _ = writeln!(text, "animation {}: running, every frame", owner_text(&row));
        rows.push(row);
    }
    for p in ljos_platform::Platform::of(engine).promises() {
        let mut row = waker(engine, "answer", p.owner, !p.standing);
        row["what"] = json!(p.what);
        row["waited_us"] = json!(micros(p.since.elapsed()));
        let _ = writeln!(
            text,
            "{} {}: {}, for {}, waited {:.1} ms",
            if p.standing { "listens" } else { "answer" },
            p.what,
            if p.standing {
                "may never come"
            } else {
                "under way"
            },
            owner_text(&row),
            p.since.elapsed().as_secs_f64() * 1000.0
        );
        rows.push(row);
    }
    for load in ljos_quick::ImageStore::of(engine).loading() {
        let mut row = waker(engine, "image", load.items.first().copied(), true);
        row["url"] = json!(load.url);
        row["waiting"] = json!(load.items.len());
        row["queued"] = json!(load.queued);
        let _ = writeln!(
            text,
            "image {}: {}, for {}{}",
            load.url,
            if load.queued { "queued" } else { "decoding" },
            owner_text(&row),
            if load.items.len() > 1 {
                format!(" and {} more", load.items.len() - 1)
            } else {
                String::new()
            }
        );
        rows.push(row);
    }
    // The inspector's own inbox holds the asking client, not the
    // application's work.
    for m in observe::mailboxes(0) {
        if m.depth == 0 || m.name == "inspector" {
            continue;
        }
        let mut row = waker(engine, "message", None, true);
        row["mailbox"] = json!(m.name);
        row["depth"] = json!(m.depth);
        let _ = writeln!(text, "message: {} queued in {}", m.depth, m.name);
        rows.push(row);
    }
    // On the system's clock, as the host's loop sleeps on it, whatever
    // clock the application's timers keep.
    if let Some(due) = activity.collect_due {
        let due_in = due.saturating_duration_since(Instant::now());
        let mut row = waker(engine, "collector", None, true);
        row["what"] = json!("script garbage");
        row["due_in_us"] = json!(micros(due_in));
        let _ = writeln!(
            text,
            "collector: what the scripts left behind, once resting, in {:.1} ms",
            due_in.as_secs_f64() * 1000.0
        );
        rows.push(row);
    }
    if activity.watching {
        let mut row = waker(engine, "watcher", None, false);
        row["what"] = json!("QML files");
        text.push_str("listens watcher: the QML files, for a change to reload\n");
        rows.push(row);
    }
    rows
}

/// Engine activity that determines what one `app_processes` reply reports.
pub(crate) struct ProcessActivity {
    pub(crate) pending_requests: usize,
    pub(crate) queued_work: bool,
    pub(crate) watching: bool,
    pub(crate) source: ProcessSource,
    /// When the host wakes to collect the scripts' garbage.
    pub(crate) collect_due: Option<Instant>,
}

/// Answers `app_processes`, on the UI thread.
pub(crate) fn processes(
    engine: &Engine,
    secrets: &SecretBook,
    stats: &FrameStats,
    activity: ProcessActivity,
    args: &ProcessesArgs,
    per_copy: usize,
) -> ToolReply {
    if args.kind == Some(ProcessKind::Waker) {
        let mut text = String::new();
        let rows = waker_rows(engine, &activity, &mut text);
        let scheduled = rows.iter().filter(|r| r["scheduled"] == true).count();
        let _ = writeln!(
            text,
            "{}",
            match (scheduled, rows.len() - scheduled) {
                (0, 0) => "nothing will wake the UI thread".to_owned(),
                (0, n) => format!("nothing is scheduled to wake the UI thread; {n} listening"),
                (s, n) => format!("{s} scheduled to wake the UI thread, {n} listening"),
            }
        );
        let value = json!({
            "processes": rows,
            "scheduled": scheduled,
            "listening": rows.len() - scheduled,
        });
        return ToolReply::from(value).with_text(text.trim_end().to_owned());
    }
    let wants = |k: ProcessKind| args.kind.is_none_or(|w| w == k);
    let mut rows: Vec<Value> = Vec::new();
    let mut text = String::new();
    let measured = activity.source == ProcessSource::Platform;
    let mut process = None;
    if wants(ProcessKind::Thread) {
        let ui = current_tid().filter(|_| measured);
        let now = if measured {
            Process::now()
        } else {
            Process::default()
        };
        let mut all: Vec<_> = now.threads.iter().flatten().collect();
        // The UI thread first: it is the one the rest serve.
        all.sort_by_key(|t| Some(t.tid) != ui);
        for t in &all {
            let is_ui = Some(t.tid) == ui;
            let kind = thread_kind(&t.name, is_ui);
            // It is answering this request, which says nothing about how
            // busy it is otherwise.
            let state = if is_ui {
                Some(if activity.queued_work { "busy" } else { "idle" })
            } else {
                t.state.map(thread_state)
            };
            let (name, cut) = whole_name(&t.name);
            let mut row = json!({
                "kind": "thread",
                "name": if is_ui { "UI thread" } else { name.as_str() },
                "role": kind,
                "tid": t.tid,
                "state": state,
                "work_us": t.work_us,
                "wakeups": t.wakeups,
                "preempted": t.preempted,
            });
            if is_ui {
                row["thread_name"] = json!(t.name);
                row["platform_requests"] = json!(activity.pending_requests);
            }
            if cut && !is_ui {
                // The kernel kept 15 bytes of a longer name.
                row["truncated"] = json!(true);
            }
            let _ = write!(
                text,
                "thread {} [{kind}] tid {}{}, cpu {:.1} ms",
                if is_ui { "UI thread" } else { name.as_str() },
                t.tid,
                state.map(|s| format!(" {s}")).unwrap_or_default(),
                t.work_us as f64 / 1000.0,
            );
            if let Some(w) = t.wakeups {
                let _ = write!(text, ", wakeups {w}");
            }
            text.push('\n');
            rows.push(row);
        }
        if all.iter().any(|t| t.wakeups.is_none()) {
            text.push_str("wakeups of each thread: not measured on this platform\n");
        }
        if all.iter().any(|t| t.state.is_none()) {
            text.push_str("state of each thread: not measured on this platform\n");
        }
        process = Some(now);
    }
    if wants(ProcessKind::Timer) {
        let now = engine.now();
        for t in engine.running_timers() {
            let (name, locator) = label(engine, t.object);
            let due_in = t.due.map(|d| d.saturating_since(now));
            let state = if t.due.is_some() { "waiting" } else { "frozen" };
            rows.push(json!({
                "kind": "timer",
                "name": name,
                "handle": super::objects::handle(t.object),
                "locator": locator,
                "state": state,
                "interval_ms": t.interval,
                "repeat": t.repeat,
                "due_in_us": due_in.map(micros),
                "wakeups": t.fired,
            }));
            let _ = writeln!(
                text,
                "timer {name} {state}, every {} ms{}, fired {}",
                t.interval,
                if t.repeat { "" } else { " once" },
                t.fired
            );
        }
    }
    if wants(ProcessKind::Animation) {
        for (anim, paused) in ljos_quick::running_animations(engine) {
            let (name, locator) = label(engine, anim);
            let state = if paused { "paused" } else { "running" };
            rows.push(json!({
                "kind": "animation",
                "name": name,
                "handle": super::objects::handle(anim),
                "locator": locator,
                "state": state,
            }));
            let _ = writeln!(text, "animation {name} {state}");
        }
    }
    if wants(ProcessKind::Owner) {
        for (proc, time) in stats.owners(16) {
            let name = owner_name(engine, proc);
            rows.push(json!({
                "kind": "owner",
                "name": name,
                "proc": format!("{}.{}", proc.index(), proc.generation()),
                "work_us": micros(time),
            }));
            let _ = writeln!(text, "owner {name}, {:.2} ms", time.as_secs_f64() * 1000.0);
        }
    }
    if wants(ProcessKind::Supervisor) {
        for s in engine.supervisors() {
            let row = supervisor_row(engine, &s);
            let _ = writeln!(
                text,
                "{}supervisor {} ({}, {} / {} ms, {} used{})",
                "  ".repeat(s.depth),
                row["name"].as_str().unwrap_or_default(),
                row["strategy"].as_str().unwrap_or_default(),
                s.intensity.max_restarts,
                s.intensity.within.as_millis(),
                s.restarts,
                if s.down { ", down" } else { "" }
            );
            rows.push(row);
        }
    }
    let faults = wants(ProcessKind::Fault).then(|| fault_rows(engine, secrets, &mut text));
    if rows.is_empty() && !args.wants_mailboxes() && faults.is_none() {
        text.push_str("nothing of that kind is running\n");
    }
    let mut value = json!({ "processes": rows });
    if let Some(faults) = faults {
        value["faults"] = Value::Array(faults);
    }
    if let Some(process) = process {
        let block = process_block(&process);
        process_text(&block, &mut text);
        value["process"] = block;
    }
    fit_rows(&mut value, &mut text, per_copy);
    ToolReply::from(value).with_text(text.trim_end().to_owned())
}

fn snake(s: impl std::fmt::Debug) -> String {
    let mut out = String::new();
    for (i, c) in format!("{s:?}").chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// One supervisor of the tree. A boundary's is called by its boundary.
fn supervisor_row(engine: &Engine, s: &ljos_engine::SupervisorInfo) -> Value {
    let mut row = json!({
        "kind": "supervisor",
        "name": s.name,
        "depth": s.depth,
        "strategy": snake(s.strategy),
        "max_restarts": s.intensity.max_restarts,
        "within_ms": u64::try_from(s.intensity.within.as_millis()).unwrap_or(u64::MAX),
        "restarts": s.restarts,
        "state": if s.down { "down" } else { "running" },
    });
    if let Some(b) = s.boundary {
        let (name, locator) = label(engine, b);
        row["name"] = json!(name);
        row["handle"] = json!(super::objects::handle(b));
        row["locator"] = json!(locator);
    }
    if let Some((restart, running)) = s.content {
        row["content"] = json!(if running { "running" } else { "stopped" });
        row["content_restart"] = json!(snake(restart));
    }
    row
}

/// The faults on record, the oldest first, and a line for each. What a
/// fault says (its message, where it was, its stack) may repeat what a
/// hidden field held when it was thrown, so it is masked with every secret
/// the inspector has known.
fn fault_rows(engine: &Engine, secrets: &SecretBook, text: &mut String) -> Vec<Value> {
    use ljos_engine::TakenBy;
    let faults = engine.faults();
    let mut rows = Vec::with_capacity(faults.len());
    for f in &faults {
        let alive = |o: ObjectId| engine.is_alive(o).then(|| super::objects::handle(o));
        let (taken_by, taken_handle) = match &f.taken_by {
            TakenBy::Waiting => ("waiting".to_owned(), None),
            TakenBy::Log => ("log".to_owned(), None),
            TakenBy::Application => ("application".to_owned(), None),
            TakenBy::Discarded => ("discarded".to_owned(), None),
            TakenBy::Boundary { object, name } => (name.clone(), alive(*object)),
        };
        let origin_handle = f.origin.and_then(alive);
        let masked = |s: &str| {
            let mut s = s.to_owned();
            secrets.mask_text(&mut s);
            s
        };
        let stack: Vec<Value> = f
            .stack
            .iter()
            .map(|s| {
                let mut frame = json!({ "function": s.function, "file": s.file, "line": s.line });
                secrets.mask_json(&mut frame);
                frame
            })
            .collect();
        let message = masked(&f.message());
        let fault_file = f.file.as_deref().map(masked);
        let origin = f.origin_name.as_deref().map(masked);
        let now = engine.now();
        rows.push(json!({
            "seq": f.seq,
            "fault": f.kind(),
            "message": message,
            "file": fault_file,
            "line": f.line,
            "at_ms": f.at.as_micros() / 1000,
            "ago_ms": u64::try_from(now.saturating_since(f.at).as_millis()).unwrap_or(u64::MAX),
            // What it was called then: it may have gone with the content
            // the fault stopped, and then it has no handle.
            "origin": origin,
            "origin_handle": origin_handle,
            "origin_gone": f.origin.is_some() && origin_handle.is_none(),
            "taken_by": taken_by,
            "taken_by_handle": taken_handle,
            "stack": stack,
        }));
        let file = fault_file
            .as_deref()
            .map(|p| p.rsplit('/').next().unwrap_or(p));
        let _ = writeln!(
            text,
            "fault {} {} at {}:{}: {}, taken by {taken_by}",
            f.seq,
            f.kind(),
            file.unwrap_or("?"),
            f.line,
            message
        );
    }
    if faults.is_empty() {
        text.push_str("no faults\n");
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::process::Thread;

    #[test]
    fn a_thread_is_known_by_its_name() {
        assert_eq!(thread_kind("ljos-compute", false), "pool");
        assert_eq!(thread_kind("ljos-screen-rea", false), "screen_reader");
        assert_eq!(thread_kind("ljos-xml", false), "worker");
        assert_eq!(thread_kind("gdbus", false), "dbus");
        assert_eq!(thread_kind("zbus::Connectio", false), "dbus");
        assert_eq!(thread_kind("llvmpipe-3", false), "gpu");
        assert_eq!(thread_kind("pipewire", false), "other");
        assert_eq!(thread_kind("gdbus", true), "engine");
    }

    #[test]
    fn a_cut_name_is_made_whole_or_said_to_be_cut() {
        assert_eq!(
            whole_name("zbus::Connectio"),
            ("zbus::Connection".into(), false)
        );
        // A name as long as the kernel keeps may have been longer. Where
        // names are kept whole, none is cut.
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let kept = "t".repeat(NAME_KEPT);
            assert_eq!(whole_name(&kept), (kept.clone(), true));
        }
        assert_eq!(whole_name("tokio-rt"), ("tokio-rt".into(), false));
    }

    /// The host wakes a rest after scripts made garbage, to collect it:
    /// a waker of its own, scheduled, gone once collected.
    #[test]
    fn the_collection_a_rest_away_is_a_waker() {
        let engine = Engine::new();
        let activity = |collect_due| ProcessActivity {
            pending_requests: 0,
            queued_work: false,
            watching: false,
            source: ProcessSource::Platform,
            collect_due,
        };
        let collectors = |rows: &[Value]| {
            rows.iter()
                .filter(|r| r["source"] == "collector")
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut text = String::new();
        let due = Instant::now() + std::time::Duration::from_millis(100);
        let rows = waker_rows(&engine, &activity(Some(due)), &mut text);
        let found = collectors(&rows);
        assert_eq!(found.len(), 1, "{rows:?}");
        assert_eq!(found[0]["kind"], "waker");
        assert_eq!(found[0]["scheduled"], true);
        let due_in = found[0]["due_in_us"].as_u64().unwrap();
        assert!(due_in <= 100_000, "{due_in}");
        assert!(
            text.contains("collector: what the scripts left behind"),
            "{text}"
        );
        let mut text = String::new();
        let rows = waker_rows(&engine, &activity(None), &mut text);
        assert!(collectors(&rows).is_empty(), "{rows:?}");
    }

    #[test]
    fn what_is_not_measured_is_said_and_not_given_as_zero() {
        let block = process_block(&Process::default());
        assert_eq!(block["threads"], Value::Null);
        assert!(block.get("rss_bytes").is_none());
        assert!(block.get("cpu_us").is_none());
        assert_eq!(
            block["not_measured"],
            json!(["threads", "rss_bytes", "cpu_us"])
        );
        let mut text = String::new();
        process_text(&block, &mut text);
        assert!(
            text.ends_with("threads, memory, CPU time: not measured on this platform\n"),
            "{text}"
        );
    }

    #[test]
    fn a_platform_that_counts_wakeups_for_the_process_gives_them_there() {
        let thread = Thread {
            tid: 1,
            name: String::new(),
            state: None,
            work_us: 0,
            wakeups: None,
            preempted: None,
        };
        let process = Process {
            threads: Some(vec![thread; 3]),
            resident: None,
            cpu_us: Some(2500),
            wakeups: Some(40),
        };
        let block = process_block(&process);
        assert_eq!(block["wakeups"], 40);
        assert_eq!(block["not_measured"], json!(["rss_bytes"]));
        let mut text = String::new();
        process_text(&block, &mut text);
        assert!(
            text.contains(": 3 threads, cpu 2.5 ms, 40 wakeups\n"),
            "{text}"
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn the_process_is_measured() {
        let block = process_block(&Process::now());
        assert!(block["threads"].as_u64().unwrap() >= 1);
        assert!(block["rss_bytes"].as_u64().unwrap() > 0);
        assert!(block["own_bytes"].as_u64().unwrap() > 0);
        assert!(block.get("not_measured").is_none());
    }
}
