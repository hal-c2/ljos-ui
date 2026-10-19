//! `app_processes`: the application as an Erlang observer sees a node
//! (ARCHITECTURE.md 3.4), what does work and, as the observer shows each
//! process's message queue with it, the mailboxes it is handed through.
//!
//! The threads are read from `/proc/self/task`, which the kernel keeps
//! anyway, so listing them costs nothing until asked. The UI thread's
//! own processes (Timers, animations, the owners the scheduler charges
//! time to) are read where they live, on the UI thread. The mailboxes
//! are read from [`ljos_runtime::observe`] on the server's thread, so a
//! UI thread that is stuck still shows its inbox filling up.

use std::fmt::Write as _;

use ljos_engine::{Engine, ObjectId};
use ljos_mcp_core::ToolReply;
use ljos_runtime::observe::{self, MailboxSample};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::args::{Act, Kind, Resource};
use super::surface::DevReq;
use crate::top::{FrameStats, owner_name};

pub const PROCESSES: &str = "\
What does work, like Erlang's observer: threads (state, CPU time, wakeups), Timers, animations, \
scheduler owners, the supervision tree, the last faults and what took each, and the mailboxes \
to the UI thread (depth, age, refused), shown even when it is stuck. `kind` keeps one.";

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
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessesArgs {
    /// List only this kind: `thread`, `timer`, `animation`, `owner`,
    /// `mailbox`, `supervisor` or `fault`.
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
pub(crate) fn with_mailboxes(mut reply: ToolReply, args: &ProcessesArgs) -> ToolReply {
    if !args.wants_mailboxes() {
        return reply;
    }
    let (rows, lines) = mailbox_rows(args);
    if reply.structured.get("processes").is_none() {
        reply.structured["processes"] = json!([]);
    }
    reply.structured["mailboxes"] = Value::Array(rows);
    reply.structured["counted_since"] = json!("inspector attached");
    let text = match reply.text.take() {
        Some(t) if !t.is_empty() => format!("{t}\n{lines}"),
        _ => lines,
    };
    reply.with_text(text)
}

/// One OS thread as `/proc` shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Thread {
    tid: u32,
    name: String,
    /// `R`, `S`, `D`, ...
    state: char,
    work_us: u64,
    wakeups: u64,
    preempted: u64,
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
/// kernel cut it: it keeps 15 bytes.
fn whole_name(name: &str) -> (String, bool) {
    if let Some((_, whole)) = WHOLE_NAMES.iter().find(|(cut, _)| *cut == name) {
        return ((*whole).to_owned(), false);
    }
    (name.to_owned(), name.len() == 15)
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

#[cfg(target_os = "linux")]
fn current_tid() -> Option<u32> {
    std::fs::read_link("/proc/thread-self")
        .ok()?
        .file_name()?
        .to_str()?
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
fn threads() -> Vec<Thread> {
    let Ok(dir) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    let mut all: Vec<Thread> = dir
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(read_thread)
        .collect();
    all.sort_by_key(|t| t.tid);
    all
}

#[cfg(target_os = "linux")]
fn read_thread(tid: u32) -> Option<Thread> {
    let base = format!("/proc/self/task/{tid}");
    let stat = std::fs::read_to_string(format!("{base}/stat")).ok()?;
    // The name is in parentheses and may hold either; it ends at the last.
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let name = stat.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = stat.get(close + 2..)?.split_whitespace().collect();
    let state = rest.first()?.chars().next()?;
    // utime and stime, fields 14 and 15, in clock ticks of 10 ms; the
    // scheduler's own count in nanoseconds is finer when it is there.
    let ticks: u64 = rest.get(11)?.parse::<u64>().ok()? + rest.get(12)?.parse::<u64>().ok()?;
    let work_us = std::fs::read_to_string(format!("{base}/schedstat"))
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(ticks * 10_000, |ns| ns / 1000);
    let status = std::fs::read_to_string(format!("{base}/status")).unwrap_or_default();
    let field = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key)?.trim().parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some(Thread {
        tid,
        name,
        state,
        work_us,
        wakeups: field("voluntary_ctxt_switches:"),
        preempted: field("nonvoluntary_ctxt_switches:"),
    })
}

#[cfg(not(target_os = "linux"))]
fn current_tid() -> Option<u32> {
    None
}

#[cfg(not(target_os = "linux"))]
fn threads() -> Vec<Thread> {
    Vec::new()
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

/// Bytes of the process: its address space, what is resident, and of
/// that what is backed by files or shared (libraries, the binary, shared
/// buffers), which other processes may hold too.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resident {
    pub(crate) virt: u64,
    pub(crate) rss: u64,
    pub(crate) shared: u64,
}

impl Resident {
    /// Resident and the process's own: heap, stacks, anonymous maps.
    pub(crate) fn own(&self) -> u64 {
        self.rss.saturating_sub(self.shared)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn now() -> Option<Resident> {
        // Pages, as `statm` counts them; 4 KiB on every Linux Ljós runs on
        // but the few with larger pages, which `smaps` would tell. The
        // kernel keeps these counts, so reading them walks nothing.
        const PAGE: u64 = 4096;
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let mut pages = statm
            .split_whitespace()
            .map(|f| f.parse::<u64>().unwrap_or(0));
        Some(Resident {
            virt: pages.next()? * PAGE,
            rss: pages.next()? * PAGE,
            shared: pages.next()? * PAGE,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn now() -> Option<Resident> {
        None
    }
}

/// The process as a whole: its threads, its memory, the CPU it used.
#[cfg(target_os = "linux")]
fn process_block(threads: usize) -> Value {
    let Resident { virt, rss, shared } = Resident::now().unwrap_or_default();
    // utime and stime of every thread there was, ended ones too, in
    // clock ticks of 10 ms.
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest: Vec<&str> = stat
        .rfind(')')
        .and_then(|c| stat.get(c + 2..))
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let field = |i: usize| rest.get(i).and_then(|f| f.parse::<u64>().ok()).unwrap_or(0);
    let cpu_us = (field(11) + field(12)) * 10_000;
    json!({
        "pid": std::process::id(),
        "threads": threads,
        "rss_bytes": rss,
        "shared_bytes": shared,
        "virtual_bytes": virt,
        "cpu_us": cpu_us,
    })
}

#[cfg(not(target_os = "linux"))]
fn process_block(threads: usize) -> Value {
    json!({ "pid": std::process::id(), "threads": threads })
}

/// Answers `app_processes`, on the UI thread.
pub(crate) fn processes(
    engine: &Engine,
    stats: &FrameStats,
    pending_requests: usize,
    queued_work: bool,
    args: &ProcessesArgs,
) -> ToolReply {
    let wants = |k: ProcessKind| args.kind.is_none_or(|w| w == k);
    let mut rows: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut thread_count = 0;
    if wants(ProcessKind::Thread) {
        let ui = current_tid();
        let mut all = threads();
        thread_count = all.len();
        // The UI thread first: it is the one the rest serve.
        all.sort_by_key(|t| Some(t.tid) != ui);
        for t in &all {
            let is_ui = Some(t.tid) == ui;
            let kind = thread_kind(&t.name, is_ui);
            // It is answering this request, which says nothing about how
            // busy it is otherwise.
            let state = if is_ui {
                if queued_work { "busy" } else { "idle" }
            } else {
                thread_state(t.state)
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
                row["platform_requests"] = json!(pending_requests);
            }
            if cut && !is_ui {
                // The kernel kept 15 bytes of a longer name.
                row["truncated"] = json!(true);
            }
            let _ = writeln!(
                text,
                "thread {} [{kind}] tid {} {state}, cpu {:.1} ms, wakeups {}",
                if is_ui { "UI thread" } else { name.as_str() },
                t.tid,
                t.work_us as f64 / 1000.0,
                t.wakeups
            );
            rows.push(row);
        }
        if all.is_empty() {
            text.push_str("threads: not listed on this platform\n");
        }
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
    let faults = wants(ProcessKind::Fault).then(|| fault_rows(engine, &mut text));
    if rows.is_empty() && !args.wants_mailboxes() && faults.is_none() {
        text.push_str("nothing of that kind is running\n");
    }
    let mut value = json!({ "processes": rows });
    if let Some(faults) = faults {
        value["faults"] = Value::Array(faults);
    }
    if wants(ProcessKind::Thread) {
        let process = process_block(thread_count);
        if let Some(rss) = process["rss_bytes"].as_u64() {
            let _ = writeln!(
                text,
                "process {}: {} threads, {:.1} MiB resident",
                process["pid"],
                thread_count,
                rss as f64 / (1024.0 * 1024.0)
            );
        }
        value["process"] = process;
    }
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

/// The faults on record, the oldest first, and a line for each.
fn fault_rows(engine: &Engine, text: &mut String) -> Vec<Value> {
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
        let stack: Vec<Value> = f
            .stack
            .iter()
            .map(|s| json!({ "function": s.function, "file": s.file, "line": s.line }))
            .collect();
        let now = engine.now();
        rows.push(json!({
            "seq": f.seq,
            "fault": f.kind(),
            "message": f.message(),
            "file": f.file,
            "line": f.line,
            "at_ms": f.at.as_micros() / 1000,
            "ago_ms": u64::try_from(now.saturating_since(f.at).as_millis()).unwrap_or(u64::MAX),
            // What it was called then: it may have gone with the content
            // the fault stopped, and then it has no handle.
            "origin": f.origin_name,
            "origin_handle": origin_handle,
            "origin_gone": f.origin.is_some() && origin_handle.is_none(),
            "taken_by": taken_by,
            "taken_by_handle": taken_handle,
            "stack": stack,
        }));
        let file = f.file.as_deref().map(|p| p.rsplit('/').next().unwrap_or(p));
        let _ = writeln!(
            text,
            "fault {} {} at {}:{}: {}, taken by {taken_by}",
            f.seq,
            f.kind(),
            file.unwrap_or("?"),
            f.line,
            f.message()
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
        assert_eq!(
            whole_name("some-long-threa"),
            ("some-long-threa".into(), true)
        );
        assert_eq!(whole_name("tokio-rt"), ("tokio-rt".into(), false));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_thread_is_among_the_threads() {
        let me = current_tid().unwrap();
        // The kernel walks a task directory by position, so a thread that
        // ends meanwhile (another test's) can make one listing skip a
        // living thread.
        let mine = (0..20)
            .find_map(|_| threads().into_iter().find(|t| t.tid == me))
            .unwrap();
        assert_eq!(mine.state, 'R');
        assert!(!mine.name.is_empty());
    }
}
