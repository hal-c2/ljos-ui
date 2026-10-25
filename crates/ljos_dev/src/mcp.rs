//! The MCP inspector of a running application (ARCHITECTURE.md §19.4 to
//! §19.7).
//!
//! ```text
//! client ── HTTP on loopback, bearer token ──▶ server thread
//!   InspectorSurface: is there such a tool, does this connection hold its
//!   capability, are the arguments right
//!     └─ UiHandle<request> (16 places; full is `busy`) ── wake ──▶ UI thread
//!          Inspector::pump: copy what the scene shows, answer; a snapshot
//!          is written by a Deferred job that yields; an act is run as an
//!          event, an assignment or a script and answered once the frame
//!          has settled
//! ```
//!
//! **Capabilities.** A connection holds the capabilities it was started
//! with (`--mcp=inspect,input,mutate,eval`) and no other: `inspect` reads,
//! `input` makes events like a person's, `mutate` assigns and calls and
//! reloads, `eval` runs script. None implies another; in particular `eval`
//! is never given by `mutate`. A tool without its capability is refused by
//! name, with how to grant it, before anything is sent to the UI thread.
//!
//! **Idle.** Nothing here has a timer. The UI thread runs for the
//! inspector when a request arrives (the inbox's wake hook) and while a
//! snapshot job has more to write; with a client connected and asking
//! nothing, [`Inspector::pump`] is not called, and if it is it returns at
//! once. The accessibility tree the outline is made from is switched on
//! for one request and given back before `pump` returns.
//!
//! **What is never shown.** The text of a field whose `echoMode` is not
//! `Normal` (and of every editable field when
//! [`InspectorConfig::redact_all`] is set) is `<redacted>` in outlines,
//! searches, properties, pictures and in what the acts report back, and
//! so is any name or string property that is exactly that text. A script
//! cannot be held to the mask, so `app_eval` is refused under `redact_all`
//! and in a field that hides its text; elsewhere what it returns has the
//! secrets masked wherever they occur in a string.

mod act;
mod args;
mod capture;
mod components;
mod edits;
mod export;
mod frames;
mod growth;
mod holders;
mod hot;
mod logs;
mod mappings;
mod marks;
mod memory;
mod models;
mod objects;
mod observe;
mod paint;
mod reload;
mod rust_heap;
mod surface;
mod timeline;
mod tools;
mod trace;
mod values;
mod wakes;
mod watch;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ljos_engine::{Engine, LogEntry, ObjectId};
use ljos_mcp_core::{
    ErrorCode, LineOptions, OutlineOptions, OutlineWriter, RefTable, SurfaceError, ToolReply, delta,
};
use ljos_quick::QuickScene;
use ljos_runtime::{
    Clock, DrainBudget, FrameBudget, FrameReport, Job, JobCx, JobPoll, Lane, ProcId, Scheduler,
    UiInbox, VirtualClock, WakeHook,
};
use serde_json::json;
use tokio::sync::oneshot;

pub use args::{
    FIRST_ACT, INSTRUCTIONS, MAX_LOG_LINES, MAX_NODES, MAX_SCREENSHOT_SIDE, SERVER_NAME, TOOLS,
};
pub use export::TraceDir;
pub use ljos_mcp_core::glue::http::{
    HttpConfig, HttpError, HttpServer, LAN_WARNING, Listen, bind_policy,
};
pub use ljos_mcp_core::{
    Cap, CapSet, ParseCapError, SOCKET_PATH_MAX, SocketPathError, check_socket_path,
};
pub use memory::WindowMemory;
pub use paint::{Drawn, PaintReport, PainterInfo, StartupStep};
pub use rust_heap::CountingAlloc;
pub use surface::{INBOX_CAPACITY, InspectorSurface, MAX_SNAPSHOT_JOBS};
pub use wakes::HostWake;

use args::{Act, Include as Col, Kind, Resource, SnapshotArgs};
use capture::{Capture, Lease};
use frames::FrameLog;
use logs::LogRing;
use paint::{PaintStats, Startup};
use reload::ReloadFeed;
use surface::{Answer, DevReq, SlotGuard};
use tools::{Look, Target};
use watch::ChangeFeed;

use crate::top::{FrameStats, INSPECTOR_PROC};

/// References the inspector remembers. An older one is stale.
const REFS: usize = 4096;
/// Snapshots kept to continue (`cursor`) or to compare with (`since`).
const KEPT_SNAPSHOTS: usize = 4;
/// Lines a `since` answer lists before it says "and more".
const MAX_DELTA_LINES: usize = 200;
/// Nodes a snapshot job writes between two looks at its budget, and the
/// most it writes before it lets the frame go on.
const STEP_NODES: usize = 32;
const CHUNK_NODES: usize = 256;
/// The part of a frame the inspector's jobs may take.
const SLICE: Duration = Duration::from_millis(4);

/// How an inspector is started.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorConfig {
    /// Mask the value of every editable field, not only of password
    /// fields (`--mcp-redact`).
    pub redact_all: bool,
    /// How long a client waits for the UI thread before it is told
    /// `timeout`.
    pub timeout: Duration,
    /// Whether the application's clock is one a test moves. Only said in
    /// `app_status`.
    pub virtual_clock: bool,
    /// What a picture is drawn on where the application draws nothing.
    pub background: [u8; 3],
    /// Where `app_frames`' `export` writes trace files.
    pub exports: TraceDir,
}

impl Default for InspectorConfig {
    fn default() -> Self {
        InspectorConfig {
            redact_all: false,
            timeout: Duration::from_secs(5),
            virtual_clock: false,
            background: [0xff, 0xff, 0xff],
            exports: TraceDir::Nowhere,
        }
    }
}

/// How an outline was asked for, so that its next part is written the
/// same way.
#[derive(Clone, Copy, Debug)]
struct View {
    root_id: u64,
    max_depth: Option<u32>,
    line: LineOptions,
    max_nodes: usize,
}

impl View {
    /// The whole outline, as `app_snapshot` without arguments asks for it.
    fn whole(capture: &Capture) -> View {
        View {
            root_id: capture.node(0).id,
            max_depth: None,
            line: LineOptions::default(),
            max_nodes: MAX_NODES,
        }
    }
}

struct Kept {
    epoch: u64,
    capture: Rc<Capture>,
    view: View,
}

/// Writes one part of an outline, a few nodes per frame.
struct SnapshotJob {
    capture: Rc<Capture>,
    writer: Option<OutlineWriter>,
    /// `None` for the tree resource.
    epoch: Option<u64>,
    picture: Option<Vec<u8>>,
    per_copy: usize,
    reply: Option<oneshot::Sender<Answer>>,
    deadline: Instant,
    _slot: Option<SlotGuard>,
}

impl Job<RefTable> for SnapshotJob {
    fn lane(&self) -> Lane {
        Lane::Deferred
    }

    fn owner(&self) -> ProcId {
        INSPECTOR_PROC
    }

    fn poll(&mut self, cx: &mut JobCx<'_, RefTable>) -> JobPoll {
        let waiting = self.reply.as_ref().is_some_and(|r| !r.is_closed());
        if !waiting || Instant::now() >= self.deadline {
            // Nobody reads the answer any more.
            if let Some(reply) = self.reply.take() {
                let _ = reply.send(Err(timed_out()));
            }
            return JobPoll::Done;
        }
        let Some(writer) = self.writer.as_mut() else {
            return JobPoll::Done;
        };
        let mut written = 0;
        loop {
            if writer.step(&self.capture.tree, cx.ctx, STEP_NODES) {
                break;
            }
            written += STEP_NODES;
            if written >= CHUNK_NODES || cx.budget.check() {
                return JobPoll::Yield;
            }
        }
        if let (Some(writer), Some(reply)) = (self.writer.take(), self.reply.take()) {
            let _ = reply.send(Ok(tools::snapshot_reply(
                writer.finish(),
                self.epoch,
                self.per_copy,
                self.picture.take(),
            )));
        }
        JobPoll::Done
    }
}

fn timed_out() -> SurfaceError {
    SurfaceError::new(
        ErrorCode::Timeout,
        "the application's UI thread came to this request after the client had stopped waiting",
    )
}

/// An `app_reload` waiting for the host to read the changed files.
struct Parked {
    reply: oneshot::Sender<Answer>,
    deadline: Instant,
    /// The reload count when it was asked for, to tell whether one happened.
    seq: u64,
}

/// An `app_resize` waiting for the platform to give the window its size.
struct ParkedResize {
    reply: oneshot::Sender<Answer>,
    deadline: Instant,
    size: [f64; 2],
}

/// An `app_metrics` `memory` waiting for the host to say what its windows hold.
struct ParkedMemory {
    reply: oneshot::Sender<Answer>,
    args: memory::MemoryArgs,
}

/// The inspector's side on the UI thread. The host makes one, hands the
/// [`InspectorSurface`] to a server ([`serve`]) or an in-process client,
/// and calls [`pump`](Self::pump) when woken.
pub struct Inspector {
    cfg: InspectorConfig,
    inbox: UiInbox<DevReq>,
    wake: WakeHook,
    jobs: Scheduler<RefTable>,
    refs: RefTable,
    epoch: u64,
    kept: VecDeque<Kept>,
    /// Whether outlines list the items that take the pointer and have no
    /// accessible node: what the last `app_snapshot` that said asked for.
    pointer_lines: bool,
    logs: Rc<RefCell<LogRing>>,
    /// What `app_trace` wrote down.
    traces: Rc<RefCell<trace::TraceLog>>,
    stats: FrameStats,
    /// What the inspector did since the host last asked
    /// ([`take_work`](Self::take_work)).
    work: Option<FrameReport>,
    /// The clock `app_advance` moves; none on the system clock.
    vclock: Option<VirtualClock>,
    feed: Arc<ReloadFeed>,
    /// What `app_watch` waits on.
    changes: Arc<ChangeFeed>,
    /// The reload generation and error as of the last [`observe`](Self::observe).
    seen: Option<(u64, Option<String>)>,
    parked: Option<Parked>,
    reload_wanted: bool,
    /// The host gives the primary window the size `app_resize` asks for.
    resizes_window: bool,
    resize_wanted: Option<[f64; 2]>,
    parked_resize: Option<ParkedResize>,
    /// The host says what each window's renderer keeps, when asked
    /// ([`finish_memory`](Self::finish_memory)).
    measures_render: bool,
    parked_memory: Vec<ParkedMemory>,
    /// How many windows the application has, as the host last said.
    windows: usize,
    /// What paints the primary window, as the host said.
    painter: Option<PainterInfo>,
    startup: Startup,
    paint: PaintStats,
    frames: FrameLog,
    /// What `app_frames` recorded.
    timeline: timeline::Timeline,
    /// The UI thread's wakes `app_frames` keeps with `wakes`.
    wakes: wakes::Wakes,
    /// Who asked the engine to profile bindings (`app_frames`' `bindings`).
    hot: hot::Hot,
    /// The components' instances sampled for `app_frames`' `growth`.
    growth: growth::Growth,
    /// Properties assigned by `app_set_property`, so that `app_inspect`
    /// can say they were assigned by devtools and not by the application.
    edits: edits::Edits,
    /// The model each client follows (`app_objects`' `model` with a
    /// `holder`).
    models: models::Follows,
    marks: marks::Marks,
    /// The mailbox and timer counters move while an inspector is served
    /// (`ljos_runtime::observe`).
    _observing: ljos_runtime::ObserveGuard,
}

impl fmt::Debug for Inspector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inspector")
            .field("cfg", &self.cfg)
            .field("queued", &self.inbox.len())
            .field("jobs", &self.jobs.len())
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

/// Which outline columns were asked for.
fn line_options(include: &[Col]) -> LineOptions {
    LineOptions {
        bounds: include.contains(&Col::Bounds),
        types: include.contains(&Col::Types),
        description: include.contains(&Col::Description),
    }
}

/// The longest an outline may be so that its footer fits with it.
fn outline_bytes(per_copy: usize) -> usize {
    (per_copy / 8 * 7).saturating_sub(512).max(256)
}

impl Inspector {
    /// An inspector and the surface a server offers for it. `wake` makes
    /// the UI thread run (a repaint request); it is called from the
    /// server's thread when a request arrives, and from `pump` while a
    /// snapshot is not finished.
    pub fn new(cfg: InspectorConfig, wake: WakeHook) -> (Inspector, Arc<InspectorSurface>) {
        let (inbox, handle) = UiInbox::channel_with_wake(INBOX_CAPACITY, wake.clone());
        inbox.observe_as("inspector", Some(observe::request_kind));
        let feed = ReloadFeed::new();
        let changes = ChangeFeed::new();
        let surface = Arc::new(InspectorSurface::new(
            handle,
            cfg.timeout,
            feed.clone(),
            changes.clone(),
        ));
        let traces = trace::TraceLog::new(changes.clone(), cfg.redact_all);
        let changes_for_models = changes.clone();
        let inspector = Inspector {
            cfg,
            inbox,
            wake,
            jobs: Scheduler::new(),
            refs: RefTable::new(REFS),
            epoch: 0,
            kept: VecDeque::new(),
            pointer_lines: false,
            logs: Rc::new(RefCell::new(LogRing::default())),
            traces,
            stats: FrameStats::new(),
            work: None,
            vclock: None,
            feed,
            changes,
            seen: None,
            parked: None,
            reload_wanted: false,
            resizes_window: false,
            resize_wanted: None,
            parked_resize: None,
            measures_render: false,
            parked_memory: Vec::new(),
            windows: 1,
            painter: None,
            startup: Startup::default(),
            paint: PaintStats::default(),
            frames: FrameLog::default(),
            timeline: timeline::Timeline::default(),
            wakes: wakes::Wakes::default(),
            hot: hot::Hot::default(),
            growth: growth::Growth::default(),
            edits: edits::Edits::default(),
            models: models::Follows::new(changes_for_models),
            marks: marks::Marks::default(),
            _observing: ljos_runtime::observe::enable(),
        };
        (inspector, surface)
    }

    /// Says that the application runs on `clock`, which `app_advance` may
    /// move. Without it that tool is `unavailable`.
    pub fn set_virtual_clock(&mut self, clock: VirtualClock) {
        self.cfg.virtual_clock = true;
        self.vclock = Some(clock);
    }

    /// Says whether the host watches the files and so can reload them when
    /// asked ([`take_reload_request`](Self::take_reload_request)). Without
    /// it `app_reload` is `unavailable`.
    pub fn set_watched(&mut self, watched: bool) {
        self.feed.set_watched(watched);
    }

    /// Says that the host has a platform window and gives it the size
    /// `app_resize` asks for ([`take_resize_request`](Self::take_resize_request)),
    /// laying the scene out at the size the platform then gives. Without
    /// it the tool sizes the scene alone, as for a host with no window.
    pub fn set_resizes_window(&mut self) {
        self.resizes_window = true;
    }

    /// Says that the host draws its windows and tells, when an
    /// `app_metrics` `memory` waits ([`waits_for_memory`](Self::waits_for_memory)),
    /// what each one's scene and renderer hold
    /// ([`finish_memory`](Self::finish_memory)). Without it the tool tells
    /// of the inspector's scene alone, with nothing drawing it.
    pub fn set_measures_render(&mut self) {
        self.measures_render = true;
    }

    /// Whether an `app_metrics` `memory` waits for the host to say what its windows
    /// hold.
    pub fn waits_for_memory(&self) -> bool {
        !self.parked_memory.is_empty()
    }

    /// Answers the `app_metrics` `memory` requests that wait, with what the host's
    /// windows hold, the primary one first.
    pub fn finish_memory(&mut self, engine: &Engine, windows: &[WindowMemory]) {
        let driver = self.painter.as_ref().and_then(|p| p.adapter.as_deref());
        for parked in std::mem::take(&mut self.parked_memory) {
            let _ = parked
                .reply
                .send(Ok(memory::memory(engine, windows, &parked.args, driver)));
        }
    }

    /// Says how many windows the application has. The inspector sees the
    /// scene of the primary window only; `app_status` tells a client how
    /// many there are so that it does not take the one it sees for all.
    pub fn set_windows(&mut self, count: usize) {
        self.windows = count.max(1);
    }

    /// Says what paints the primary window, for `app_status`.
    pub fn set_painter(&mut self, painter: PainterInfo) {
        self.painter = Some(painter);
    }

    /// Records when a step of the start-up ended, timed from when the host
    /// started, for `app_status`. A step recorded again keeps its first
    /// time.
    pub fn record_startup(&mut self, step: StartupStep, at: Duration) {
        self.startup.record(step, at);
    }

    /// Records a pass of the primary window the host painted or found
    /// unchanged, for `app_metrics`.
    pub fn record_paint(&mut self, report: &PaintReport) {
        if report.drawn != Drawn::Unchanged {
            self.changes.paint();
        }
        self.paint.record(report);
        self.frames.record_paint(report);
        self.timeline.record_paint(None, report);
    }

    /// Records a pass of another window the host painted, for `app_frames`
    /// only: the paint totals are the main window's.
    pub fn record_window_paint(&mut self, window: Option<ObjectId>, report: &PaintReport) {
        self.timeline.record_paint(window, report);
    }

    /// Whether a client records frames: only then does the host time the
    /// passes over other windows.
    pub fn records_frames(&self) -> bool {
        self.timeline.recording()
    }

    /// Whether a client keeps the UI thread's wakes: only then does the
    /// host gather what it saw in a turn of its loop for
    /// [`Inspector::record_wake`].
    pub fn records_wakes(&self) -> bool {
        self.wakes.recording()
    }

    /// A turn of the host's loop ended: what the host saw in it, with what
    /// the inspector saw since the last ([`Inspector::pump`],
    /// [`Inspector::record_scene_frames`]). Does nothing while no client
    /// keeps wakes.
    pub fn record_wake(&mut self, host: &HostWake) {
        if self.wakes.record(host) {
            self.changes.wake();
        }
    }

    /// Whether a client follows items with `app_watch`: only then need the
    /// host work out where a pass changed the picture and tell
    /// [`Inspector::record_changed`].
    pub fn watches_items(&self) -> bool {
        self.changes.follows_any()
    }

    /// A pass of the primary window changed the picture in `areas`, boxes
    /// `[x, y, width, height]` in the window's points, or all of it for
    /// `None`. Each followed item that is shown and lies in one of them
    /// was drawn again.
    pub fn record_changed(&mut self, engine: &Engine, areas: Option<&[[f64; 4]]>) {
        let followed = self.changes.followed_items();
        if followed.is_empty() {
            return;
        }
        let meets = |a: &[f64; 4], b: &[f64; 4]| {
            a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
        };
        let drawn: Vec<String> = engine.untrack(|| {
            followed
                .into_iter()
                .filter(|h| {
                    let Some(obj) = objects::object_of(h) else {
                        return false;
                    };
                    if !engine.is_alive(obj) || !tools::visible_to_the_top(engine, obj) {
                        return false;
                    }
                    let item = tools::window_box(engine, obj);
                    match areas {
                        None => true,
                        Some(areas) => areas.iter().any(|a| meets(a, &item)),
                    }
                })
                .collect()
        });
        self.changes.drawn_again(&drawn);
    }

    /// Says how long a frame may take before `app_metrics` logs it as
    /// slow: a frame of the primary window's display. 1/60 s until told.
    pub fn set_frame_budget(&mut self, budget: Duration) {
        self.frames.set_budget(budget);
    }

    /// Notes a reload that happened since the last call, so that clients
    /// are told. The host calls this after every reload outcome it has
    /// handled, applied or failed, and whenever it has run a frame; it
    /// compares two numbers and does nothing else. A client that waits for
    /// a reload sleeps until this publishes one: nothing wakes the host on
    /// its behalf, so a host that does not call this leaves it waiting out
    /// its time.
    pub fn observe(&mut self, engine: &Engine) {
        let generation = engine.reload_generation();
        let error = engine.last_reload_error().map(|e| e.to_string());
        let Some((was, was_error)) = self.seen.as_mut() else {
            // The first look is the baseline: what happened before the
            // inspector attached is not an event.
            self.feed.baseline(generation);
            self.seen = Some((generation, error));
            return;
        };
        if generation != *was {
            *was = generation;
            *was_error = error;
            self.feed.applied(generation);
            self.changes.reload();
        } else {
            match error {
                Some(e) if was_error.as_deref() != Some(e.as_str()) => {
                    *was_error = Some(e.clone());
                    self.feed.failed(e);
                    self.changes.reload();
                }
                Some(_) => {}
                None => *was_error = None,
            }
        }
    }

    /// Whether an agent asked for a reload that the host has not done. The
    /// host reads the changed files now, however recent the change
    /// ([`DevSession::reload_now`](crate::DevSession::reload_now)), applies
    /// what that brings and calls [`finish_reload`](Self::finish_reload)
    /// once [`DevSession::take_flushed`](crate::DevSession::take_flushed)
    /// says it is done.
    pub fn take_reload_request(&mut self) -> bool {
        std::mem::take(&mut self.reload_wanted)
    }

    /// Answers the `app_reload` that was waiting, with how the reload went.
    pub fn finish_reload(&mut self, engine: &Engine) {
        self.observe(engine);
        let Some(parked) = self.parked.take() else {
            return;
        };
        let now = self.feed.state();
        let value = match &now.last {
            Some(o) if now.seq != parked.seq => {
                if o.applied {
                    json!({ "outcome": "applied", "generation": now.generation })
                } else {
                    json!({
                        "outcome": "failed",
                        "generation": now.generation,
                        "message": o.message,
                    })
                }
            }
            // Nothing new, but the files may be what they were when the
            // reload before failed: "unchanged" would hide that.
            _ => match engine.last_reload_error() {
                Some(fault) => json!({
                    "outcome": "failed",
                    "generation": now.generation,
                    "message": fault.to_string(),
                }),
                None => json!({ "outcome": "unchanged", "generation": now.generation }),
            },
        };
        let text = match value["outcome"].as_str() {
            Some("applied") => format!("reloaded; generation {}", now.generation),
            Some("failed") => format!(
                "the reload failed and the application goes on as it was: {}",
                value["message"].as_str().unwrap_or_default()
            ),
            _ => "no file changed; nothing was reloaded".to_string(),
        };
        let _ = parked
            .reply
            .send(Ok(ToolReply::from(value).with_text(text)));
    }

    /// The size an agent asked the primary window to have, that the host
    /// has not asked of the platform yet.
    pub fn take_resize_request(&mut self) -> Option<[f64; 2]> {
        self.resize_wanted.take()
    }

    /// Whether an `app_resize` waits for the window to have its size.
    pub fn waits_for_resize(&self) -> bool {
        self.parked_resize.is_some()
    }

    /// Answers the `app_resize` that waits, once the scene has the size
    /// asked for, or with the size it has when `settled`: the platform
    /// gave another size, or said nothing in the time it is given.
    pub fn finish_resize(&mut self, scene: &QuickScene, settled: bool) {
        let Some(parked) = &self.parked_resize else {
            return;
        };
        let viewport = scene.viewport();
        let has = [viewport.width(), viewport.height()];
        let there = has
            .iter()
            .zip(parked.size)
            .all(|(has, asked)| (has - asked).abs() < 0.5);
        if !there && !settled && Instant::now() < parked.deadline {
            return;
        }
        let Some(parked) = self.parked_resize.take() else {
            return;
        };
        let _ = parked
            .reply
            .send(Ok(resized(parked.size, has, scene.scale_factor())));
    }

    /// Asks the host for the window's new size and answers once it has it.
    fn start_resize(
        &mut self,
        scene: &mut QuickScene,
        a: &args::ResizeArgs,
        reply: oneshot::Sender<Answer>,
        deadline: Instant,
    ) {
        if let Some(scale) = a.scale {
            scene.set_scale_factor(scale as f32);
        }
        let size = [f64::from(a.width), f64::from(a.height)];
        // A later size is what the window ends up with: the one asked
        // before is answered now, with what it has.
        if let Some(before) = self.parked_resize.take() {
            let viewport = scene.viewport();
            let has = [viewport.width(), viewport.height()];
            let _ = before
                .reply
                .send(Ok(resized(before.size, has, scene.scale_factor())));
        }
        self.parked_resize = Some(ParkedResize {
            reply,
            deadline,
            size,
        });
        self.resize_wanted = Some(size);
        (self.wake)();
    }

    fn start_reload(&mut self, reply: oneshot::Sender<Answer>, deadline: Instant) {
        let state = self.feed.state();
        if !state.watched {
            let _ = reply.send(Err(SurfaceError::unavailable(
                "this application does not watch its files, so there is nothing to reload. Start \
                 it with `--watch` (`ljos run --watch`)",
            )));
            return;
        }
        if self
            .parked
            .as_ref()
            .is_some_and(|p| !p.reply.is_closed() && Instant::now() < p.deadline)
        {
            let _ = reply.send(Err(SurfaceError::new(
                ErrorCode::Busy,
                "another `app_reload` is waiting for the application; ask again when it is answered",
            )));
            return;
        }
        self.parked = Some(Parked {
            reply,
            deadline,
            seq: state.seq,
        });
        self.reload_wanted = true;
        // The host reads the files the next time it runs.
        (self.wake)();
    }

    /// Makes the engine's log the one `app_logs` reads. An engine has one
    /// log handler, so what the host did with a line before (print it)
    /// goes in `also`.
    pub fn watch_log(&self, engine: &Engine, mut also: impl FnMut(&LogEntry) + 'static) {
        let ring = self.logs.clone();
        let changes = self.changes.clone();
        engine.set_log_handler_with_origin(move |entry, at| {
            ring.borrow_mut().push(entry, at);
            changes.log();
            also(entry);
        });
    }

    /// Records where the time of the scene's frames in this pass went, for
    /// `app_frames`, with how many messages the host took from other
    /// threads. Call it at the end of every pass, before painting; while
    /// nobody records, the scene keeps no times and this does nothing.
    pub fn record_scene_frames(&mut self, scene: &mut QuickScene, drained: usize) {
        if let Some(times) = scene.take_frame_times() {
            self.wakes.ran(&times, drained);
            self.timeline.record(times, drained, None);
        }
    }

    /// As [`Inspector::record_scene_frames`], for the scene of another
    /// window, which `app_frames` does not reach (see
    /// [`Inspector::time_window`]).
    pub fn record_window_frames(&mut self, window: Option<ObjectId>, scene: &mut QuickScene) {
        if let Some(times) = scene.take_frame_times() {
            self.wakes.ran(&times, 0);
            self.timeline.record(times, 0, window);
        }
    }

    /// Times the scene of another window exactly while a client records
    /// frames or keeps wakes; call it before its frame runs.
    pub fn time_window(&self, engine: &Engine, scene: &mut QuickScene) {
        let on = self.timeline.recording() || self.wakes.recording();
        if on != scene.is_timed() {
            scene.set_timed(engine, on);
        }
    }

    /// Records a frame the host ran, for `app_metrics`.
    pub fn record_frame(&mut self, report: &FrameReport) {
        self.changes.frame();
        self.stats.record(report);
        self.frames.record_frame(report);
    }

    /// The work the inspector did since this was last called, by lane and
    /// under its own name ([`crate::top::INSPECTOR_PROC`]): for a host whose `top`
    /// overlay shows what ran in a frame, and must show an agent's looking
    /// as that and not as the application's. `app_metrics` has it already.
    pub fn take_work(&mut self) -> Option<FrameReport> {
        self.work.take()
    }

    fn note_work(&mut self, work: &FrameReport) {
        self.stats.record_work(work);
        crate::top::add_work(self.work.get_or_insert_with(FrameReport::default), work);
    }

    /// Whether a call to [`pump`](Self::pump) would do anything.
    pub fn has_work(&self) -> bool {
        !self.inbox.is_empty() || !self.jobs.is_empty()
    }

    /// Answers what was asked since the last call and lets the snapshot
    /// jobs write for a slice of the frame. Returns at once when there is
    /// nothing to do. Call it before the frame is painted: a request that
    /// looks at the scene leaves it asking for a frame.
    pub fn pump(&mut self, engine: &Engine, scene: &mut QuickScene, clock: &dyn Clock) {
        self.observe(engine);
        self.growth.tick(engine);
        if !self.has_work() {
            return;
        }
        let started = Instant::now();
        let mut requests = Vec::new();
        let drain = self
            .inbox
            .drain(DrainBudget::count(INBOX_CAPACITY), clock, |r| {
                requests.push(r)
            });
        let answered = requests.len();
        let asked_by: Vec<_> = requests.iter().map(|r| r.client.clone()).collect();
        let mut lease: Option<Lease> = None;
        let mut capture: Option<Rc<Capture>> = None;
        for request in &requests {
            if let Kind::Snapshot(SnapshotArgs {
                pointer: Some(on), ..
            }) = &request.kind
            {
                self.pointer_lines = *on;
            }
        }
        for request in requests {
            if Instant::now() >= request.deadline || request.reply.is_closed() {
                let _ = request.reply.send(Err(timed_out()));
                continue;
            }
            if needs_scene(&request.kind) && capture.is_none() {
                lease = Some(Lease::take(engine, scene, clock));
                capture = Some(Rc::new(capture::capture(
                    engine,
                    scene,
                    self.cfg.redact_all,
                    self.pointer_lines,
                )));
            }
            self.answer(engine, scene, clock, &mut capture, request);
        }
        if let Some(lease) = lease {
            lease.release(engine, scene);
        }
        if answered > 0 {
            self.wakes.asked(&asked_by);
            self.timeline.asked(asked_by);
            // What the inspector did itself is in the measurements it
            // gives, under its own name: an agent that looks at where the
            // time went must be able to tell its own looking apart.
            let spent = started.elapsed();
            let mut work = FrameReport::default();
            work.per_lane[Lane::Deferred.index()] = spent;
            work.top_owners.push((INSPECTOR_PROC, spent));
            work.jobs_polled = answered;
            work.jobs_done = answered;
            work.drained = drain.drained;
            work.drain_time = drain.elapsed;
            self.note_work(&work);
        }
        if !self.jobs.is_empty() {
            let report =
                self.jobs
                    .run_frame(&mut self.refs, FrameBudget::new(clock.now(), SLICE), clock);
            self.note_work(&report);
        }
        if self.has_work() {
            (self.wake)();
        }
    }

    /// `app_frames` with `wakes`. The scenes are timed while wakes are
    /// kept: their frames say whether a timer or an animation woke them.
    fn answer_wakes(
        &mut self,
        engine: &Engine,
        scene: &mut QuickScene,
        args: &timeline::FramesArgs,
    ) -> Result<ToolReply, SurfaceError> {
        if let Some(record) = args.record {
            self.wakes.ask(record, args.holder.as_deref())?;
            let on = self.wakes.recording() || self.timeline.recording();
            if on != scene.is_timed() {
                scene.set_timed(engine, on);
            }
        }
        Ok(self.wakes.answer(args.since, args.max))
    }

    fn next_epoch(&mut self) -> u64 {
        self.epoch += 1;
        self.epoch
    }

    fn keep(&mut self, epoch: u64, capture: Rc<Capture>, view: View) {
        if self.kept.len() == KEPT_SNAPSHOTS {
            self.kept.pop_front();
        }
        self.kept.push_back(Kept {
            epoch,
            capture,
            view,
        });
    }

    fn answer(
        &mut self,
        engine: &Engine,
        scene: &mut QuickScene,
        clock: &dyn Clock,
        capture: &mut Option<Rc<Capture>>,
        request: DevReq,
    ) {
        let DevReq {
            kind,
            reply,
            deadline,
            slot,
            per_copy,
            caps,
            client: _,
        } = request;
        if let Kind::Act(act) = &kind {
            if matches!(act, Act::Reload) {
                self.start_reload(reply, deadline);
                return;
            }
            if let Act::Resize(a) = act
                && self.resizes_window
            {
                self.start_resize(scene, a, reply, deadline);
                return;
            }
            let Some(before) = capture.clone() else {
                let _ = reply.send(Err(SurfaceError::new(
                    ErrorCode::Internal,
                    "the scene was not looked at for an act that needs it",
                )));
                return;
            };
            let (answer, after) = self.run_act(engine, scene, clock, before, act, per_copy);
            // The next request of this round sees what the act did.
            *capture = Some(after);
            let _ = reply.send(answer);
            return;
        }
        if let Kind::Memory(args) = &kind {
            if self.measures_render {
                // The host answers it once this pass is over, when it can
                // look at every window and what draws it.
                self.parked_memory.push(ParkedMemory {
                    reply,
                    args: args.clone(),
                });
                (self.wake)();
                return;
            }
            let windows = [WindowMemory {
                window: None,
                scene: scene.memory(),
                render: None,
            }];
            let driver = self.painter.as_ref().and_then(|p| p.adapter.as_deref());
            let _ = reply.send(Ok(memory::memory(engine, &windows, args, driver)));
            return;
        }
        if let Kind::Frames(args) = &kind {
            let answer = args.mode().and_then(|mode| match mode {
                timeline::Mode::Frames => {
                    let keeps = self.wakes.recording();
                    timeline::frames(engine, scene, &mut self.timeline, args, keeps)
                }
                timeline::Mode::Wakes => self.answer_wakes(engine, scene, args),
                timeline::Mode::Bindings => {
                    if let Some(record) = args.record {
                        self.hot.ask(engine, record, args.holder.as_deref())?;
                    }
                    Ok(self
                        .hot
                        .answer(engine, args.sort.unwrap_or_default(), args.max))
                }
                timeline::Mode::Growth => {
                    if let Some(record) = args.record {
                        self.growth.ask(engine, record, args.holder.as_deref())?;
                    }
                    Ok(self.growth.answer(engine, args.max))
                }
                timeline::Mode::Export(name) => {
                    export::export(engine, &self.cfg.exports, name, &self.timeline, &self.wakes)
                }
            });
            let _ = reply.send(answer);
            return;
        }
        let scene: &QuickScene = scene;
        let capture = capture.as_ref();
        let queued = self.inbox.len();
        let capacity = self.inbox.capacity();
        let background = self.cfg.background;
        let virtual_clock = self.cfg.virtual_clock;
        // Everything but a snapshot is answered here and now.
        let now: Answer = match (&kind, capture) {
            (Kind::Logs(args), _) => Ok(self.logs.borrow().answer(args, per_copy)),
            (Kind::Resource(Resource::Logs), _) => Ok(self
                .logs
                .borrow()
                .answer(&args::LogsArgs::default(), per_copy)),
            (Kind::Resource(Resource::Reload), _) => {
                self.observe(engine);
                let value = self.feed.state().json(false);
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                Ok(ToolReply::from(value).with_text(text))
            }
            (Kind::Metrics, _) => Ok(tools::metrics(
                engine,
                &self.stats,
                &self.paint,
                &self.frames,
                queued,
                capacity,
                false,
            )),
            (Kind::Processes(args), _) => Ok(observe::processes(
                engine,
                &self.stats,
                ljos_platform::Platform::of(engine).pending(),
                // Frames asked for by more requests or by motion.
                queued > 0 || ljos_quick::running_animation_count(engine) > 0,
                self.feed.state().watched,
                args,
            )),
            (Kind::Objects(args), _) if args.release => {
                if let Some(holder) = &args.holder {
                    self.models.end(holder);
                    self.marks.end(holder);
                }
                Ok(ToolReply::from(json!({})))
            }
            (Kind::Objects(args), _) if args.mark => {
                self.marks.mark(engine, args.holder.as_deref())
            }
            (Kind::Objects(args), _) if args.since_mark => {
                self.marks.since(engine, args.holder.as_deref(), args.max)
            }
            (Kind::Objects(args), _) if args.models => Ok(models::list(engine, args.max, per_copy)),
            (Kind::Objects(args), _) if args.components => Ok(components::list(engine, per_copy)),
            (Kind::Objects(args), _) if args.component.is_some() => components::instances(
                engine,
                args.component.as_deref().unwrap_or_default(),
                args.offset,
                args.limit,
            ),
            (Kind::Objects(args), capture) if args.model.is_some() => {
                let what = args.model.as_deref().unwrap_or_default();
                let found = match capture {
                    Some(capture) if objects::needs_scene(args) => {
                        let mut look = Look {
                            engine,
                            scene,
                            capture,
                            refs: &mut self.refs,
                        };
                        look.object(what, "app_objects")
                    }
                    _ => match objects::parse_handle(engine, what) {
                        Some(found) => found,
                        None => Err(SurfaceError::new(
                            ErrorCode::Internal,
                            "the scene was not looked at for a target that needs it",
                        )),
                    },
                };
                found
                    .and_then(|obj| {
                        models::rows(engine, obj, what, args.offset, args.limit, per_copy)
                    })
                    .map(|(reply, model)| {
                        if let Some(holder) = &args.holder {
                            self.models.follow(holder, &model);
                        }
                        reply
                    })
            }
            (Kind::Objects(args), capture) => {
                let found = match capture {
                    Some(capture) if objects::needs_scene(args) => {
                        let mut look = Look {
                            engine,
                            scene,
                            capture,
                            refs: &mut self.refs,
                        };
                        let what = args.root.as_deref().unwrap_or_default();
                        look.object(what, "app_objects").map(Some)
                    }
                    _ => Ok(None),
                };
                found.and_then(|found| objects::objects(engine, scene, args, found, per_copy))
            }
            (Kind::Trace(args), capture) => {
                let found = match (&args.target, capture) {
                    (Some(what), Some(capture)) if trace::needs_scene(args) => {
                        let mut look = Look {
                            engine,
                            scene,
                            capture,
                            refs: &mut self.refs,
                        };
                        look.object(what, "app_trace").map(Some)
                    }
                    (Some(what), _) => match objects::parse_handle(engine, what) {
                        Some(found) => found.map(Some),
                        None => Err(SurfaceError::new(
                            ErrorCode::Internal,
                            "the scene was not looked at for a target that needs it",
                        )),
                    },
                    (None, _) => Ok(None),
                };
                found.and_then(|found| trace::trace(engine, &self.traces, args, found))
            }
            // The server answers it without the UI thread.
            (Kind::Watch(_), _) => Err(SurfaceError::new(
                ErrorCode::Internal,
                "a watch reached the UI thread",
            )),
            (Kind::Resource(Resource::Metrics), _) => Ok(tools::metrics(
                engine,
                &self.stats,
                &self.paint,
                &self.frames,
                queued,
                capacity,
                true,
            )),
            (Kind::Snapshot(args), capture) => {
                match self.start_snapshot(engine, scene, capture, args, per_copy) {
                    Ok(Started::Answer(reply)) => Ok(reply),
                    Ok(Started::Write {
                        capture,
                        options,
                        epoch,
                        picture,
                    }) => {
                        self.jobs.spawn(Box::new(SnapshotJob {
                            writer: Some(OutlineWriter::new(&capture.tree, options)),
                            capture,
                            epoch: Some(epoch),
                            picture,
                            per_copy,
                            reply: Some(reply),
                            deadline,
                            _slot: slot,
                        }));
                        return;
                    }
                    Err(e) => Err(e),
                }
            }
            (Kind::Resource(Resource::Tree), Some(capture)) => {
                let options = OutlineOptions::whole(MAX_NODES, outline_bytes(per_copy));
                self.jobs.spawn(Box::new(SnapshotJob {
                    writer: Some(OutlineWriter::new(&capture.tree, options)),
                    capture: capture.clone(),
                    epoch: None,
                    picture: None,
                    per_copy,
                    reply: Some(reply),
                    deadline,
                    _slot: slot,
                }));
                return;
            }
            (kind, Some(capture)) => {
                let reload = self.feed.state().json(false);
                let windows = self.windows;
                let host = paint::status(self.painter.as_ref(), &self.startup);
                let mut look = Look {
                    engine,
                    scene,
                    capture,
                    refs: &mut self.refs,
                };
                match kind {
                    Kind::Status(_) => look.status(virtual_clock, caps, &reload, windows, host),
                    Kind::Find(args) => look.find(args, per_copy),
                    Kind::Inspect(args) => look.inspect(args, per_copy, &self.edits),
                    Kind::Screenshot(args) => look.screenshot(args, background),
                    // Answered above.
                    _ => Err(SurfaceError::new(
                        ErrorCode::Internal,
                        "request not handled",
                    )),
                }
            }
            (_, None) => Err(SurfaceError::new(
                ErrorCode::Internal,
                "the scene was not looked at for a request that needs it",
            )),
        };
        let _ = reply.send(now);
    }

    fn start_snapshot(
        &mut self,
        engine: &Engine,
        scene: &QuickScene,
        capture: Option<&Rc<Capture>>,
        args: &SnapshotArgs,
        per_copy: usize,
    ) -> Result<Started, SurfaceError> {
        let max_bytes = outline_bytes(per_copy);
        let picture = |look: &mut Look<'_>, background| -> Result<Option<Vec<u8>>, SurfaceError> {
            if !args.image {
                return Ok(None);
            }
            let shot = look.screenshot(&args::ScreenshotArgs::default(), background)?;
            Ok(shot.images.into_iter().next().map(|i| i.data))
        };
        if let Some(cursor) = &args.cursor {
            let (epoch, skip) = tools::parse_cursor(cursor)?;
            let Some(kept) = self.kept.iter().find(|k| k.epoch == epoch) else {
                return Err(SurfaceError::invalid_argument(format!(
                    "snapshot {epoch} is not kept any more (the last {KEPT_SNAPSHOTS} are), so \
                     cursor `{cursor}` cannot be continued. Take a new `app_snapshot`"
                )));
            };
            let view = kept.view;
            let kept_capture = kept.capture.clone();
            let Some(root) = kept_capture.tree.index_of(view.root_id) else {
                return Err(SurfaceError::new(
                    ErrorCode::Internal,
                    "the snapshot lost the item it starts at",
                ));
            };
            let picture = match capture {
                Some(now) => picture(
                    &mut Look {
                        engine,
                        scene,
                        capture: now,
                        refs: &mut self.refs,
                    },
                    self.cfg.background,
                )?,
                None => None,
            };
            return Ok(Started::Write {
                options: OutlineOptions {
                    root,
                    max_depth: view.max_depth,
                    max_nodes: args.max_nodes.unwrap_or(view.max_nodes).min(MAX_NODES),
                    max_bytes,
                    skip,
                    line: view.line,
                },
                capture: kept_capture,
                epoch,
                picture,
            });
        }
        let Some(capture) = capture else {
            return Err(SurfaceError::new(
                ErrorCode::Internal,
                "the scene was not looked at for a snapshot",
            ));
        };
        let whole = View {
            root_id: capture.node(0).id,
            max_depth: None,
            line: LineOptions::default(),
            max_nodes: MAX_NODES,
        };
        if let Some(since) = args.since {
            let Some(old) = self.kept.iter().find(|k| k.epoch == since) else {
                return Err(SurfaceError::not_found(format!(
                    "snapshot {since} is not kept (the last {KEPT_SNAPSHOTS} are, and the newest \
                     is {}), so there is nothing to compare with. Take a whole `app_snapshot`",
                    self.epoch
                )));
            };
            let old = old.capture.clone();
            let changes = delta(
                &old.tree,
                &capture.tree,
                since,
                &mut self.refs,
                MAX_DELTA_LINES,
            );
            let epoch = self.next_epoch();
            self.keep(epoch, capture.clone(), whole);
            let mut reply = tools::delta_reply(changes, since, epoch);
            if let Some(png) = picture(
                &mut Look {
                    engine,
                    scene,
                    capture,
                    refs: &mut self.refs,
                },
                self.cfg.background,
            )? {
                reply = reply.with_image("image/png", png);
            }
            return Ok(Started::Answer(reply));
        }
        let mut look = Look {
            engine,
            scene,
            capture,
            refs: &mut self.refs,
        };
        let root = match &args.root {
            None => 0,
            Some(what) => match look.resolve(what, "app_snapshot")? {
                Target::Node(index) => index,
                Target::Unlisted(_) => {
                    return Err(SurfaceError::not_found(format!(
                        "`{what}` is not in the outline (it has no role, or it is not shown), so \
                         no outline starts at it. `app_inspect` can still look at it"
                    )));
                }
            },
        };
        let picture = picture(&mut look, self.cfg.background)?;
        let view = View {
            root_id: capture.node(root).id,
            max_depth: args.depth,
            line: line_options(&args.include),
            max_nodes: args.max_nodes.unwrap_or(MAX_NODES).min(MAX_NODES),
        };
        let epoch = self.next_epoch();
        self.keep(epoch, capture.clone(), view);
        Ok(Started::Write {
            options: OutlineOptions {
                root,
                max_depth: view.max_depth,
                max_nodes: view.max_nodes,
                max_bytes,
                skip: 0,
                line: view.line,
            },
            capture: capture.clone(),
            epoch,
            picture,
        })
    }
}

/// How a snapshot request goes on.
enum Started {
    /// Answered already (`since`).
    Answer(ToolReply),
    /// An outline to write over the next frames.
    Write {
        capture: Rc<Capture>,
        options: OutlineOptions,
        epoch: u64,
        picture: Option<Vec<u8>>,
    },
}

/// Whether answering needs the scene looked at now. The log and the
/// measurements do not, and neither does the next part of a snapshot
/// already taken.
fn needs_scene(kind: &Kind) -> bool {
    match kind {
        Kind::Logs(_)
        | Kind::Metrics
        | Kind::Processes(_)
        | Kind::Watch(_)
        | Kind::Frames(_)
        | Kind::Memory(_) => false,
        Kind::Objects(args) => objects::needs_scene(args),
        Kind::Trace(args) => trace::needs_scene(args),
        Kind::Resource(r) => *r == Resource::Tree,
        Kind::Snapshot(args) => args.cursor.is_none() || args.image,
        Kind::Act(act) => !matches!(act, Act::Reload),
        Kind::Status(_) | Kind::Find(_) | Kind::Inspect(_) | Kind::Screenshot(_) => true,
    }
}

/// The answer to an `app_resize` the platform was asked for: the size the
/// window has, which the platform may have made another than `asked`.
fn resized(asked: [f64; 2], has: [f64; 2], scale: f32) -> ToolReply {
    let [width, height] = has.map(f64::round);
    let text = if has
        .iter()
        .zip(asked)
        .all(|(has, asked)| (has - asked).abs() < 0.5)
    {
        format!("resized the window to {width}x{height}")
    } else {
        format!(
            "asked for {}x{}; the platform gave the window {width}x{height}",
            asked[0], asked[1]
        )
    };
    ToolReply::from(json!({
        "width": tools::num(width),
        "height": tools::num(height),
        "scale": tools::num(f64::from(scale)),
        "asked": { "width": asked[0], "height": asked[1] },
    }))
    .with_text(text)
}

/// Where a served inspector writes its token file unless told otherwise:
/// `target/ljos/mcp-<pid>.json` below `dir`.
pub fn default_token_file(dir: impl Into<PathBuf>) -> PathBuf {
    dir.into()
        .join("target")
        .join("ljos")
        .join(format!("mcp-{}.json", std::process::id()))
}

/// Serves `surface` to clients on this machine: on a loopback port (`0`
/// lets the system pick) or a Unix socket only its owner can open. The
/// address and a token made for this start are written to `token_file`,
/// which only its owner can read; a client without the token, or one that
/// is a web page (`Origin`) or asks for another host name, is turned away.
/// Dropping the server stops it and removes the file; the files of earlier
/// starts that were killed before they could are removed here.
///
/// The server has a thread of its own that sleeps in the kernel until a
/// client writes; it does not touch the application.
///
/// Clients hold the capability `inspect` and no other; [`serve_as`] grants
/// more.
pub fn serve(
    surface: Arc<InspectorSurface>,
    listen: Listen,
    token_file: impl Into<PathBuf>,
) -> Result<HttpServer, HttpError> {
    serve_as(surface, listen, token_file, CapSet::of([Cap::Inspect]))
}

/// As [`serve`], with every client holding `caps`.
pub fn serve_as(
    surface: Arc<InspectorSurface>,
    listen: Listen,
    token_file: impl Into<PathBuf>,
    caps: CapSet,
) -> Result<HttpServer, HttpError> {
    let mut config = HttpConfig::loopback(caps);
    config.listen = listen;
    serve_with(surface, config, token_file)
}

/// As [`serve_as`], with everything about the transport given: the address
/// (and the word that lets it be one beyond this machine), and whether the
/// token file is removed when a signal ends the process.
pub fn serve_with(
    surface: Arc<InspectorSurface>,
    config: HttpConfig,
    token_file: impl Into<PathBuf>,
) -> Result<HttpServer, HttpError> {
    let token_file = token_file.into();
    forget_dead_starts(&token_file);
    HttpServer::start(surface, config.token_file(token_file))
}

/// The process id in a token file's name, when it is named as
/// [`default_token_file`] names it.
fn start_of(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    name.strip_prefix("mcp-")?
        .strip_suffix(".json")?
        .parse()
        .ok()
}

/// Removes the token files that earlier starts left next to `token_file`.
/// A server removes its own when it stops, but one that was killed (Ctrl-C
/// in the terminal) could not, and a client that looks in the directory
/// would find an address where nothing listens. Only files named for a
/// process the system says is gone are touched.
fn forget_dead_starts(token_file: &Path) {
    if start_of(token_file).is_none() {
        return;
    }
    let Some(entries) = token_file
        .parent()
        .and_then(|dir| std::fs::read_dir(dir).ok())
    else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(pid) = start_of(&path)
            && ljos_mcp_core::process_alive(pid) == Some(false)
        {
            let _ = std::fs::remove_file(path);
        }
    }
}
