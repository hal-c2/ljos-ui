//! The read-only MCP inspector of a running application (ARCHITECTURE.md
//! §19.4 to §19.7).
//!
//! ```text
//! client ── HTTP on loopback, bearer token ──▶ server thread
//!   InspectorSurface: is there such a tool, may this client use it, are
//!   the arguments right
//!     └─ UiHandle<request> (16 places; full is `busy`) ── wake ──▶ UI thread
//!          Inspector::pump: copy what the scene shows, answer; a snapshot
//!          is written by a Deferred job that yields
//! ```
//!
//! **Read-only by construction.** The only requests there are ([`Kind`])
//! read. A tool that acts (`app_click`, `app_set_property`, `app_eval`, …)
//! is refused by name with `capability_denied` before anything is sent to
//! the UI thread, and the capability a client is given is `inspect`
//! whatever it asked for.
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
//! searches, properties and pictures, and so is any name or string
//! property that is exactly that text.

mod args;
mod capture;
mod logs;
mod surface;
mod tools;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ljos_engine::{Engine, LogEntry};
use ljos_mcp_core::{
    ErrorCode, LineOptions, OutlineOptions, OutlineWriter, RefTable, SurfaceError, ToolReply, delta,
};
use ljos_quick::QuickScene;
use ljos_runtime::{
    Clock, DrainBudget, FrameBudget, FrameReport, Job, JobCx, JobPoll, Lane, ProcId, Scheduler,
    UiInbox, WakeHook,
};
use tokio::sync::oneshot;

pub use args::{
    INSTRUCTIONS, MAX_LOG_LINES, MAX_NODES, MAX_SCREENSHOT_SIDE, RESERVED, SERVER_NAME, TOOLS,
};
pub use ljos_mcp_core::glue::http::{HttpError, HttpServer, Listen};
pub use surface::{INBOX_CAPACITY, InspectorSurface, MAX_SNAPSHOT_JOBS};

use args::{Include as Col, Kind, Resource, SnapshotArgs};
use capture::{Capture, Lease};
use logs::LogRing;
use surface::{Answer, DevReq, SlotGuard};
use tools::{Look, Target};

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
}

impl Default for InspectorConfig {
    fn default() -> Self {
        InspectorConfig {
            redact_all: false,
            timeout: Duration::from_secs(5),
            virtual_clock: false,
            background: [0xff, 0xff, 0xff],
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
    logs: Rc<RefCell<LogRing>>,
    stats: FrameStats,
    /// What the inspector did since the host last asked
    /// ([`take_work`](Self::take_work)).
    work: Option<FrameReport>,
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
        let surface = Arc::new(InspectorSurface::new(handle, cfg.timeout));
        let inspector = Inspector {
            cfg,
            inbox,
            wake,
            jobs: Scheduler::new(),
            refs: RefTable::new(REFS),
            epoch: 0,
            kept: VecDeque::new(),
            logs: Rc::new(RefCell::new(LogRing::default())),
            stats: FrameStats::new(),
            work: None,
        };
        (inspector, surface)
    }

    /// Makes the engine's log the one `app_logs` reads. An engine has one
    /// log handler, so what the host did with a line before (print it)
    /// goes in `also`.
    pub fn watch_log(&self, engine: &Engine, mut also: impl FnMut(&LogEntry) + 'static) {
        let ring = self.logs.clone();
        engine.set_log_handler_with_origin(move |entry, at| {
            ring.borrow_mut().push(entry, at);
            also(entry);
        });
    }

    /// Records a frame the host ran, for `app_metrics`.
    pub fn record_frame(&mut self, report: &FrameReport) {
        self.stats.record(report);
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
        let mut lease: Option<Lease> = None;
        let mut capture: Option<Rc<Capture>> = None;
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
                )));
            }
            self.answer(engine, scene, capture.as_ref(), request);
        }
        if let Some(lease) = lease {
            lease.release(engine, scene);
        }
        if answered > 0 {
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
        scene: &QuickScene,
        capture: Option<&Rc<Capture>>,
        request: DevReq,
    ) {
        let DevReq {
            kind,
            reply,
            deadline,
            slot,
            per_copy,
        } = request;
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
            (Kind::Metrics, _) => Ok(tools::metrics(engine, &self.stats, queued, capacity, false)),
            (Kind::Resource(Resource::Metrics), _) => {
                Ok(tools::metrics(engine, &self.stats, queued, capacity, true))
            }
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
                let mut look = Look {
                    engine,
                    scene,
                    capture,
                    refs: &mut self.refs,
                };
                match kind {
                    Kind::Status => look.status(virtual_clock),
                    Kind::Find(args) => look.find(args, per_copy),
                    Kind::Inspect(args) => look.inspect(args, per_copy),
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
        Kind::Logs(_) | Kind::Metrics => false,
        Kind::Resource(r) => *r == Resource::Tree,
        Kind::Snapshot(args) => args.cursor.is_none() || args.image,
        Kind::Status | Kind::Find(_) | Kind::Inspect(_) | Kind::Screenshot(_) => true,
    }
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
pub fn serve(
    surface: Arc<InspectorSurface>,
    listen: Listen,
    token_file: impl Into<PathBuf>,
) -> Result<HttpServer, HttpError> {
    let token_file = token_file.into();
    forget_dead_starts(&token_file);
    let mut config =
        ljos_mcp_core::glue::http::HttpConfig::loopback(surface::granted()).token_file(token_file);
    config.listen = listen;
    HttpServer::start(surface, config)
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
/// process that is gone are touched, and only where the system says which
/// processes there are.
fn forget_dead_starts(token_file: &Path) {
    let proc = Path::new("/proc");
    if start_of(token_file).is_none() || !proc.join("self").exists() {
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
            && !proc.join(pid.to_string()).exists()
        {
            let _ = std::fs::remove_file(path);
        }
    }
}
