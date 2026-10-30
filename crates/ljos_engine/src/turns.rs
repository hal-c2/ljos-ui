//! A turn that runs long, and one that never ends (ARCHITECTURE.md §9).
//!
//! Everything a host runs on the UI thread runs in a turn
//! ([`Engine::turn`], [`Engine::begin_turn`]): a closure, an inbox
//! message, a QML handler, a frame of the scene. If that code blocks, the
//! window freezes. Two things say so:
//!
//! - **The budget.** The outermost turn is timed; one that ran past the
//!   budget ([`Engine::set_turn_budget`]) is a warning through
//!   [`Engine::warn`] when it ends, naming what ran. While journaled, a
//!   turn of host code (a closure, a message) is a [`JournalKind::Host`]
//!   entry, so `app_frames` can say where a wake's time went.
//! - **The watchdog.** A turn that never ends never warns when it ends.
//!   The UI thread writes what it is doing into a [`TurnMarker`]: the
//!   turn, when it started, what started it and the phase it is in. A
//!   thread of its own reads it and reports a turn past its threshold
//!   while the turn still runs; an inspector whose request was not
//!   answered reads it to say what the UI thread is stuck in.
//!
//! # What it costs
//!
//! With no budget, no marker and no journal, a turn pays a counter and a
//! flag. Otherwise the outermost turn reads the clock where it starts and
//! where it ends, and nested turns, handlers and bindings read none. A
//! phase is one relaxed store, made only while a marker is attached.
//!
//! The watchdog never polls (rule 6): on Linux, Android, macOS, iOS and
//! Windows the UI thread arms a one-shot kernel timer for an outermost turn
//! and disarms it when the turn ends. The watchdog blocks on those timers,
//! waking only for a firing timer or a marker being added or removed. Other
//! targets sleep until the deadline of the turn in progress and park while
//! no turn runs.
//!
//! [`JournalKind::Host`]: crate::JournalKind::Host

use std::cell::{Cell, OnceCell, RefCell};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::thread::Thread;
use std::time::{Duration, Instant};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::journal::JournalRun;

#[cfg(kernel_timer)]
mod timer;

/// The budget of a debug build's shell ([`Engine::watch_turns_by_default`]).
pub const DEFAULT_TURN_BUDGET: Duration = Duration::from_millis(100);
/// How long a turn runs before the watchdog reports it, unless told
/// otherwise.
pub const DEFAULT_WATCHDOG_THRESHOLD: Duration = Duration::from_secs(1);
/// Sets the budget in milliseconds; `0` turns it off.
pub const BUDGET_VARIABLE: &str = "LJOS_TURN_BUDGET_MS";
/// Sets the watchdog's threshold in milliseconds; `0` turns it off.
pub const WATCHDOG_VARIABLE: &str = "LJOS_TURN_WATCHDOG_MS";
/// How often a long turn may be warned of. A frame that is slow every time
/// would otherwise fill the log; the ones left out are counted.
const WARN_EVERY: Duration = Duration::from_secs(1);

/// What started a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnCause {
    /// A closure the host runs ([`Engine::turn`]), or code of the object
    /// the turn is for.
    Host,
    /// A message from another thread. `kind` is what the mailbox's
    /// [`UiInbox::observe_as`](ljos_runtime::UiInbox::observe_as) names
    /// it, if it names kinds.
    Message {
        mailbox: &'static str,
        kind: Option<&'static str>,
    },
    /// A frame of the scene.
    Frame,
}

/// What the UI thread is doing in a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Phase {
    /// No turn runs.
    Idle,
    /// Host code: a closure, the handling of a message.
    Host,
    /// A QML signal handler.
    Handler,
    /// A binding.
    Binding,
    /// Passing `visible` and `enabled` down the item tree.
    Inherit,
    /// The scene's other work in a frame: timers, animations, deletes.
    Frame,
    /// Layouts and positioners laying their items out.
    Polish,
    /// Objects being made.
    Create,
    /// Items being moved and sized.
    Layout,
    /// What is drawn being put together.
    Render,
}

impl Phase {
    const ALL: [Phase; 10] = [
        Phase::Idle,
        Phase::Host,
        Phase::Handler,
        Phase::Binding,
        Phase::Inherit,
        Phase::Frame,
        Phase::Polish,
        Phase::Create,
        Phase::Layout,
        Phase::Render,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Host => "host code",
            Phase::Handler => "handler",
            Phase::Binding => "binding",
            Phase::Inherit => "inherit/visibility",
            Phase::Frame => "frame",
            Phase::Polish => "polish",
            Phase::Create => "create",
            Phase::Layout => "layout",
            Phase::Render => "render",
        }
    }

    fn from_u8(n: u8) -> Phase {
        Phase::ALL.get(n as usize).copied().unwrap_or(Phase::Idle)
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where an object is declared, kept as the engine has it: an atom is
/// shared, so taking it costs no allocation.
#[derive(Clone, Debug, Default)]
struct Place {
    url: Atom,
    line: u32,
    column: u32,
}

/// What ran in a turn, in parts: written as a turn starts and put into
/// words only when someone asks.
#[derive(Clone, Debug, Default)]
struct Cause {
    message: Option<(&'static str, Option<&'static str>)>,
    frame: bool,
    origin: Option<ObjectId>,
    /// What `origin` is called: its type as the turn starts, the whole
    /// label ([`Engine::label_of`]) in a warning. The watchdog cannot ask
    /// the engine, so a marker keeps the type.
    label: Option<Atom>,
    place: Option<Place>,
    /// The signal whose QML handler ran first, as `clicked` for
    /// `onClicked`.
    handler: Option<Atom>,
}

impl Cause {
    fn of(cause: TurnCause, origin: Option<ObjectId>) -> Cause {
        Cause {
            message: match cause {
                TurnCause::Message { mailbox, kind } => Some((mailbox, kind)),
                _ => None,
            },
            frame: cause == TurnCause::Frame,
            origin,
            ..Cause::default()
        }
    }

    fn is_host(&self) -> bool {
        !self.frame && self.origin.is_none()
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((mailbox, kind)) = self.message {
            match kind {
                Some(kind) => write!(f, "message `{kind}` of mailbox `{mailbox}`")?,
                None => write!(f, "a message of mailbox `{mailbox}`")?,
            }
        } else if self.frame {
            f.write_str("a frame of the scene")?;
        } else if self.origin.is_none() && self.handler.is_none() {
            f.write_str("host code run with `Engine::turn`")?;
        }
        let mut said = self.message.is_some() || self.frame || self.origin.is_none();
        if let Some(handler) = &self.handler {
            if said {
                f.write_str(", in ")?;
            }
            write!(f, "handler {}", crate::qtqml::handler_name(handler))?;
            said = true;
        }
        if self.origin.is_some() {
            f.write_str(if said { " of " } else { "code of " })?;
            match (&self.label, self.origin) {
                (Some(label), _) => write!(f, "{label}")?,
                (None, Some(o)) => write!(f, "object {o:?}")?,
                (None, None) => {}
            }
            if let Some(p) = &self.place {
                write!(f, " declared at {}:{}:{}", p.url, p.line, p.column)?;
            }
        }
        Ok(())
    }
}

/// The turn the UI thread is in, as another thread sees it
/// ([`TurnMarker::in_progress`]).
#[derive(Clone, Debug)]
pub struct TurnInProgress {
    /// Counts the engine's turns from 1.
    pub turn: u64,
    /// How long it has run.
    pub running: Duration,
    pub phase: Phase,
    /// What started it, in words: `message `Slow` of mailbox `application``.
    pub started_by: String,
}

impl fmt::Display for TurnInProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "turn {} has run for {} ms, started by {}, and is in phase {}",
            self.turn,
            self.running.as_millis(),
            self.started_by,
            self.phase
        )
    }
}

type Reporter = Arc<dyn Fn(&str) + Send + Sync>;

struct Shared {
    /// Outermost turns begun.
    turn: AtomicU64,
    /// When the turn in progress began, in nanoseconds since [`epoch`]
    /// plus one; 0 while none runs.
    started: AtomicU64,
    phase: AtomicU8,
    cause: Mutex<Cause>,
    /// The last turn the watchdog reported.
    reported: AtomicU64,
    /// In milliseconds; 0 when the watchdog does not watch this marker.
    threshold: AtomicU64,
    reporter: Mutex<Option<Reporter>>,
    /// The engine is gone.
    closed: AtomicBool,
    #[cfg(kernel_timer)]
    timer: OnceLock<timer::Timer>,
}

#[cfg(kernel_timer)]
impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.get() {
            timer.remove();
        }
    }
}

/// What the UI thread of one engine is doing, readable from any thread.
/// Made by [`Engine::watch_turns`] or [`Engine::turn_marker`].
#[derive(Clone)]
pub struct TurnMarker(Arc<Shared>);

impl fmt::Debug for TurnMarker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TurnMarker")
            .field("in_progress", &self.in_progress())
            .finish()
    }
}

/// The instant markers count from: an `Instant` does not fit an atomic.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn since_epoch(at: Instant) -> u64 {
    u64::try_from(at.saturating_duration_since(epoch()).as_nanos()).unwrap_or(u64::MAX)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl TurnMarker {
    fn new() -> TurnMarker {
        TurnMarker(Arc::new(Shared {
            turn: AtomicU64::new(0),
            started: AtomicU64::new(0),
            phase: AtomicU8::new(Phase::Idle as u8),
            cause: Mutex::new(Cause::default()),
            reported: AtomicU64::new(0),
            threshold: AtomicU64::new(0),
            reporter: Mutex::new(None),
            closed: AtomicBool::new(false),
            #[cfg(kernel_timer)]
            timer: OnceLock::new(),
        }))
    }

    /// The turn in progress, if one is.
    pub fn in_progress(&self) -> Option<TurnInProgress> {
        let s = &self.0;
        let turn = s.turn.load(Ordering::Acquire);
        let started = s.started.load(Ordering::Acquire);
        if started == 0 || s.closed.load(Ordering::Acquire) {
            return None;
        }
        let since = epoch() + Duration::from_nanos(started - 1);
        let started_by = lock(&s.cause).to_string();
        Some(TurnInProgress {
            turn,
            running: since.elapsed(),
            phase: Phase::from_u8(s.phase.load(Ordering::Relaxed)),
            started_by,
        })
    }

    /// The phase the UI thread is in now.
    pub fn phase(&self) -> Phase {
        Phase::from_u8(self.0.phase.load(Ordering::Relaxed))
    }

    /// Where the watchdog's reports go, instead of standard error. Called
    /// on the watchdog's thread, with the turn still running.
    pub fn set_reporter(&self, report: impl Fn(&str) + Send + Sync + 'static) {
        *lock(&self.0.reporter) = Some(Arc::new(report));
    }

    /// How long a turn runs before the watchdog reports it. `None` stops
    /// watching this marker; it is still written, for the inspector.
    pub fn set_threshold(&self, threshold: Option<Duration>) {
        let ms = threshold.map_or(0, |t| (t.as_millis() as u64).max(1));
        #[cfg(kernel_timer)]
        if ms > 0 {
            self.0.timer.get_or_init(|| watchdog().waiter.timer());
        }
        self.0.threshold.store(ms, Ordering::Release);
        if ms > 0 {
            watchdog().watch(&self.0);
        }
        #[cfg(kernel_timer)]
        if ms == 0 && self.0.timer.get().is_some() {
            watchdog().disable(&self.0);
        }
    }

    pub fn threshold(&self) -> Option<Duration> {
        match self.0.threshold.load(Ordering::Acquire) {
            0 => None,
            ms => Some(Duration::from_millis(ms)),
        }
    }

    /// Whether the watchdog reported turn `turn`.
    fn reported(&self, turn: u64) -> bool {
        self.0.reported.load(Ordering::Acquire) == turn
    }

    fn begin(&self, at: Instant, phase: Phase, cause: Cause) -> u64 {
        let s = &self.0;
        *lock(&s.cause) = cause;
        s.phase.store(phase as u8, Ordering::Relaxed);
        let threshold = s.threshold.load(Ordering::Relaxed);
        #[cfg(kernel_timer)]
        let _watched = (threshold > 0).then(|| lock(&watchdog().markers));
        let turn = s.turn.fetch_add(1, Ordering::AcqRel) + 1;
        let ns = since_epoch(at);
        s.started.store(ns + 1, Ordering::SeqCst);
        if threshold > 0 {
            #[cfg(kernel_timer)]
            if let Some(timer) = s.timer.get() {
                timer.arm(Duration::from_millis(threshold));
            }
            #[cfg(not(kernel_timer))]
            {
                let threshold = Duration::from_millis(threshold).as_nanos();
                let threshold = u64::try_from(threshold).unwrap_or(u64::MAX);
                watchdog().turn_began(ns.saturating_add(threshold));
            }
        }
        turn
    }

    fn handler(&self, cause: &Cause) {
        *lock(&self.0.cause) = cause.clone();
    }

    fn end(&self) {
        #[cfg(kernel_timer)]
        let _watched = self.0.timer.get().map(|timer| {
            let watched = lock(&watchdog().markers);
            timer.disarm();
            watched
        });
        self.0.started.store(0, Ordering::Release);
        self.0.phase.store(Phase::Idle as u8, Ordering::Relaxed);
    }
}

/// The phase in progress until dropped; the one before comes back then.
#[must_use = "the phase ends when the guard is dropped"]
#[derive(Debug)]
pub struct PhaseGuard<'a> {
    marker: Option<&'a TurnMarker>,
    prev: u8,
}

impl Drop for PhaseGuard<'_> {
    fn drop(&mut self) {
        if let Some(m) = self.marker {
            m.0.phase.store(self.prev, Ordering::Relaxed);
        }
    }
}

/// A turn in progress until dropped ([`Engine::begin_turn`]).
#[must_use = "the turn ends when the guard is dropped"]
pub struct TurnGuard<'a> {
    engine: &'a Engine,
    /// Only the outermost turn is timed.
    outer: bool,
    _journaled: Option<JournalRun<'a>>,
}

impl fmt::Debug for TurnGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TurnGuard")
            .field("outer", &self.outer)
            .finish_non_exhaustive()
    }
}

impl Drop for TurnGuard<'_> {
    fn drop(&mut self) {
        let t = &self.engine.inner.turns;
        t.depth.set(t.depth.get() - 1);
        if self.outer {
            self.engine.end_turn();
        }
    }
}

#[derive(Default)]
pub(crate) struct TurnState {
    depth: Cell<u32>,
    pub(crate) at_end: RefCell<Vec<ObjectId>>,
    budget: Cell<Option<Duration>>,
    /// When the outermost turn began; `None` when it is not timed.
    started: Cell<Option<Instant>>,
    /// The marker's number of the turn in progress.
    turn: Cell<u64>,
    /// What the turn in progress ran, for its warning.
    cause: RefCell<Cause>,
    marker: OnceCell<TurnMarker>,
    last_warned: Cell<Option<Instant>>,
    /// Warnings not given since the last one.
    unwarned: Cell<u32>,
}

impl Drop for TurnState {
    fn drop(&mut self) {
        if let Some(m) = self.marker.get() {
            m.end();
            m.0.closed.store(true, Ordering::Release);
        }
    }
}

/// A duration given in milliseconds by `variable`: `Some(None)` for 0,
/// `None` when it is not set or not a number.
fn from_env(variable: &str) -> Option<Option<Duration>> {
    let ms: u64 = std::env::var(variable).ok()?.trim().parse().ok()?;
    Some((ms > 0).then(|| Duration::from_millis(ms)))
}

/// The watchdog's threshold for a host that has it on unless told
/// otherwise when `on`: [`WATCHDOG_VARIABLE`] when it is set (0 turns the
/// watchdog off), else [`DEFAULT_WATCHDOG_THRESHOLD`] when `on`.
pub fn watchdog_threshold(on: bool) -> Option<Duration> {
    from_env(WATCHDOG_VARIABLE).unwrap_or(on.then_some(DEFAULT_WATCHDOG_THRESHOLD))
}

impl Engine {
    /// Warns of a turn that ran longer than `budget` when it ends, naming
    /// what it ran. `None`, the default, times nothing.
    pub fn set_turn_budget(&self, budget: Option<Duration>) {
        self.inner.turns.budget.set(budget);
    }

    pub fn turn_budget(&self) -> Option<Duration> {
        self.inner.turns.budget.get()
    }

    /// What a host's shell turns on: in a debug build a budget of
    /// [`DEFAULT_TURN_BUDGET`] and the watchdog at
    /// [`DEFAULT_WATCHDOG_THRESHOLD`]; in a release build neither. Either
    /// is set by its variable ([`BUDGET_VARIABLE`], [`WATCHDOG_VARIABLE`])
    /// when that is set, in either build.
    pub fn watch_turns_by_default(&self) {
        let debug = cfg!(debug_assertions);
        let budget = from_env(BUDGET_VARIABLE).unwrap_or(debug.then_some(DEFAULT_TURN_BUDGET));
        self.set_turn_budget(budget);
        if let Some(threshold) = watchdog_threshold(debug) {
            self.watch_turns(threshold);
        }
    }

    /// The marker the UI thread writes what it does into, made on the
    /// first call. Writing it costs a little in every turn from then on,
    /// so only what reads it (an inspector, the watchdog) asks for it.
    pub fn turn_marker(&self) -> TurnMarker {
        self.inner.turns.marker.get_or_init(TurnMarker::new).clone()
    }

    /// Has the watchdog report a turn that runs past `threshold`, while it
    /// still runs, on standard error unless the marker has a reporter
    /// ([`TurnMarker::set_reporter`]). Its report is reported again each
    /// time the turn's time doubles.
    pub fn watch_turns(&self, threshold: Duration) -> TurnMarker {
        let marker = self.turn_marker();
        if marker.threshold().is_none() {
            marker.set_threshold(Some(threshold));
        }
        marker
    }

    /// Whether a turn is in progress.
    pub fn in_turn(&self) -> bool {
        self.inner.turns.depth.get() > 0
    }

    /// Starts a turn, which ends when the guard is dropped. For what a host
    /// runs that is not a closure ([`Engine::turn`] is), such as a frame.
    /// Unlike [`Engine::turn`] it catches no panic.
    pub fn begin_turn(&self, cause: TurnCause, origin: Option<ObjectId>) -> TurnGuard<'_> {
        let t = &self.inner.turns;
        let depth = t.depth.get();
        t.depth.set(depth + 1);
        if depth > 0 {
            return TurnGuard {
                engine: self,
                outer: false,
                _journaled: None,
            };
        }
        let marker = t.marker.get();
        let journaled = self.is_journaled();
        if t.budget.get().is_none() && marker.is_none() && !journaled {
            return TurnGuard {
                engine: self,
                outer: true,
                _journaled: None,
            };
        }
        let at = Instant::now();
        t.started.set(Some(at));
        let mut c = Cause::of(cause, origin);
        if let Some(m) = marker {
            // Named now: the watchdog cannot ask the engine, and the
            // object may be gone by the time anyone reads it. Atoms are
            // shared, so this allocates nothing.
            if let Some(o) = origin {
                (c.label, c.place) = self.type_and_place(o);
            }
            let phase = match cause {
                TurnCause::Frame => Phase::Frame,
                _ if origin.is_some() => Phase::Handler,
                _ => Phase::Host,
            };
            t.turn.set(m.begin(at, phase, c.clone()));
        }
        let journal = (journaled && c.is_host()).then(|| self.journal_host(&c));
        *t.cause.borrow_mut() = c;
        TurnGuard {
            engine: self,
            outer: true,
            _journaled: journal.flatten(),
        }
    }

    fn end_turn(&self) {
        let t = &self.inner.turns;
        let at_end = std::mem::take(&mut *t.at_end.borrow_mut());
        for obj in at_end {
            self.destroy(obj);
        }
        let Some(started) = t.started.take() else {
            return;
        };
        let took = started.elapsed();
        let reported = match t.marker.get() {
            Some(m) => {
                m.end();
                m.reported(t.turn.get())
            }
            None => false,
        };
        let over = t.budget.get().is_some_and(|b| took > b);
        if !(over || reported) {
            return;
        }
        let now = Instant::now();
        if !reported
            && t.last_warned
                .get()
                .is_some_and(|w| now.duration_since(w) < WARN_EVERY)
        {
            t.unwarned.set(t.unwarned.get() + 1);
            return;
        }
        t.last_warned.set(Some(now));
        let mut c = t.cause.take();
        if let Some(o) = c.origin {
            let (ty, place) = self.type_and_place(o);
            // The whole label now, which a turn could not afford to make.
            c.label = self.label_of(o).map(|l| Atom::new(&l)).or(ty).or(c.label);
            c.place = place.or(c.place);
        }
        let mut text = match (t.budget.get(), over) {
            (Some(b), true) => format!(
                "a turn of the UI thread took {} ms, longer than its budget of {} ms: {c}",
                took.as_millis(),
                b.as_millis()
            ),
            _ => format!("a turn of the UI thread took {} ms: {c}", took.as_millis()),
        };
        if reported {
            text.push_str("; the watchdog reported it while it ran");
        }
        let unwarned = t.unwarned.replace(0);
        if unwarned > 0 {
            text.push_str(&format!(
                " ({unwarned} more turns over the budget in the second before)"
            ));
        }
        self.warn(text);
    }

    /// The QML handler `name` of the object whose code runs starts. The
    /// first one in a turn, and its object, name the turn.
    pub(crate) fn turn_handler(&self, name: &Atom, obj: ObjectId) -> PhaseGuard<'_> {
        let t = &self.inner.turns;
        if t.started.get().is_some() {
            let mut c = t.cause.borrow_mut();
            if c.handler.is_none() {
                c.handler = Some(name.clone());
                if c.origin.is_none() {
                    c.origin = Some(obj);
                    (c.label, c.place) = self.type_and_place(obj);
                }
                if let Some(m) = t.marker.get() {
                    m.handler(&c);
                }
            }
        }
        self.enter_phase(Phase::Handler)
    }

    /// The UI thread is in `phase` until the guard is dropped. Nothing
    /// while no marker is attached.
    pub fn enter_phase(&self, phase: Phase) -> PhaseGuard<'_> {
        let marker = self.inner.turns.marker.get();
        let prev = match marker {
            Some(m) => m.0.phase.swap(phase as u8, Ordering::Relaxed),
            None => 0,
        };
        PhaseGuard { marker, prev }
    }

    /// `obj`'s type and where it is declared, as the engine keeps them.
    fn type_and_place(&self, obj: ObjectId) -> (Option<Atom>, Option<Place>) {
        let Ok(store) = self.inner.store.try_borrow() else {
            return (None, None);
        };
        let Some(o) = store.objects.get(obj) else {
            return (None, None);
        };
        let place = o.location().map(|l| Place {
            url: l.url,
            line: l.line,
            column: l.column,
        });
        (Some(o.meta.name.clone()), place)
    }

    fn journal_host(&self, c: &Cause) -> Option<JournalRun<'_>> {
        let (ty, name) = match c.message {
            Some((mailbox, kind)) => (mailbox, kind.unwrap_or("message")),
            None => ("host", "turn"),
        };
        self.journal_host_run(ty, name)
    }
}

/// The one watchdog thread of the process: it watches every marker with a
/// threshold.
struct Watchdog {
    markers: Mutex<Vec<Watched>>,
    thread: OnceLock<Thread>,
    #[cfg(kernel_timer)]
    waiter: timer::Waiter,
    #[cfg(not(kernel_timer))]
    /// When the thread will next wake, in nanoseconds since [`epoch`];
    /// `u64::MAX` while it is parked with no turn to watch.
    next_wake: AtomicU64,
    #[cfg(all(test, kernel_timer))]
    wakes: AtomicU64,
    #[cfg(all(test, kernel_timer))]
    check_points: Mutex<Vec<std::sync::mpsc::Sender<()>>>,
}

struct Watched {
    marker: Weak<Shared>,
    /// The turn last reported, and when it is due to be again.
    turn: u64,
    again: Duration,
    #[cfg(kernel_timer)]
    timer: timer::Timer,
}

fn watchdog() -> &'static Watchdog {
    static WATCHDOG: OnceLock<Watchdog> = OnceLock::new();
    WATCHDOG.get_or_init(|| Watchdog {
        markers: Mutex::new(Vec::new()),
        thread: OnceLock::new(),
        #[cfg(kernel_timer)]
        waiter: timer::Waiter::new(),
        #[cfg(not(kernel_timer))]
        next_wake: AtomicU64::new(u64::MAX),
        #[cfg(all(test, kernel_timer))]
        wakes: AtomicU64::new(0),
        #[cfg(all(test, kernel_timer))]
        check_points: Mutex::new(Vec::new()),
    })
}

impl Watchdog {
    fn start(&'static self) {
        self.thread.get_or_init(|| {
            std::thread::Builder::new()
                .name("ljos-turn-watchdog".into())
                .spawn(move || self.run())
                .expect("the watchdog thread starts")
                .thread()
                .clone()
        });
    }
}

#[cfg(kernel_timer)]
impl Watchdog {
    fn watch(&'static self, marker: &Arc<Shared>) {
        {
            let mut all = lock(&self.markers);
            all.retain(|w| w.marker.strong_count() > 0);
            if !all.iter().any(|w| w.marker.as_ptr() == Arc::as_ptr(marker)) {
                all.push(Watched {
                    marker: Arc::downgrade(marker),
                    turn: 0,
                    again: Duration::ZERO,
                    timer: marker
                        .timer
                        .get()
                        .expect("a watched marker has a timer")
                        .clone(),
                });
            }
            let threshold = marker.threshold.load(Ordering::Acquire);
            if let Some(now) = TurnMarker(Arc::clone(marker)).in_progress() {
                let until = Duration::from_millis(threshold).saturating_sub(now.running);
                marker
                    .timer
                    .get()
                    .expect("a watched marker has a timer")
                    .arm(until);
            }
        }
        self.start();
        self.waiter.changed();
    }

    fn disable(&self, marker: &Shared) {
        if let Some(timer) = marker.timer.get() {
            let _watched = lock(&self.markers);
            timer.disarm();
            self.waiter.changed();
        }
    }

    fn run(&self) {
        loop {
            let timers: Vec<_> = lock(&self.markers)
                .iter()
                .map(|w| w.timer.clone())
                .collect();
            match self.waiter.wait(&timers) {
                timer::Wake::Changed => self.check(),
                timer::Wake::Fired(index) => {
                    debug_assert!(index < timers.len());
                    self.check();
                }
            }
            #[cfg(test)]
            {
                self.wakes.fetch_add(1, Ordering::Relaxed);
                for check_point in lock(&self.check_points).drain(..) {
                    let _ = check_point.send(());
                }
            }
        }
    }

    fn check(&self) {
        let mut due: Vec<(Arc<Shared>, String)> = Vec::new();
        {
            let mut all = lock(&self.markers);
            all.retain(|w| {
                w.marker
                    .upgrade()
                    .is_some_and(|s| s.threshold.load(Ordering::Acquire) != 0)
            });
            for w in all.iter_mut() {
                let Some(s) = w.marker.upgrade() else {
                    continue;
                };
                let threshold = s.threshold.load(Ordering::Acquire);
                let marker = TurnMarker(s);
                let Some(now) = marker.in_progress() else {
                    continue;
                };
                if w.turn != now.turn {
                    w.turn = now.turn;
                    w.again = Duration::from_millis(threshold);
                }
                if now.running >= w.again {
                    due.push((Arc::clone(&marker.0), stuck_text(&now)));
                    marker.0.reported.store(now.turn, Ordering::Release);
                    w.again *= 2;
                    w.timer.arm(w.again.saturating_sub(now.running));
                }
            }
        }
        // Reported outside the lock: a reporter is the host's code.
        for (s, text) in due {
            let reporter = lock(&s.reporter).clone();
            match reporter {
                Some(r) => r(&text),
                None => eprintln!("warning: {text}"),
            }
        }
    }

    #[cfg(test)]
    fn check_point(&self) -> std::sync::mpsc::Receiver<()> {
        let (send, receive) = std::sync::mpsc::channel();
        lock(&self.check_points).push(send);
        self.waiter.changed();
        receive
    }
}

#[cfg(not(kernel_timer))]
impl Watchdog {
    fn watch(&'static self, marker: &Arc<Shared>) {
        {
            let mut all = lock(&self.markers);
            all.retain(|w| w.marker.strong_count() > 0);
            if !all.iter().any(|w| w.marker.as_ptr() == Arc::as_ptr(marker)) {
                all.push(Watched {
                    marker: Arc::downgrade(marker),
                    turn: 0,
                    again: Duration::ZERO,
                });
            }
        }
        self.start();
        if let Some(thread) = self.thread.get() {
            thread.unpark();
        }
    }

    /// A marker's turn began: wakes the thread only if its scheduled wake
    /// is later than this turn's first deadline.
    fn turn_began(&self, deadline: u64) {
        if self.next_wake.load(Ordering::SeqCst) > deadline
            && let Some(t) = self.thread.get()
        {
            t.unpark();
        }
    }

    fn run(&self) {
        loop {
            // While awake every new turn unparks: a turn that begins after
            // `check` looked would otherwise wait out a wake computed
            // without it. The unpark only makes the next park return.
            self.next_wake.store(u64::MAX, Ordering::SeqCst);
            match self.check() {
                Some(wait) => {
                    let wake = Instant::now().checked_add(wait);
                    self.next_wake
                        .store(wake.map_or(u64::MAX, since_epoch), Ordering::SeqCst);
                    std::thread::park_timeout(wait);
                }
                None => {
                    // A turn that began since the first look already has a
                    // deadline to sleep for instead of parking here.
                    if self.check().is_none() {
                        std::thread::park();
                    }
                }
            }
        }
    }

    /// Reports what is due and says how long until the next deadline;
    /// `None` when no watched turn runs.
    fn check(&self) -> Option<Duration> {
        let mut due: Vec<(Arc<Shared>, String)> = Vec::new();
        let mut next: Option<Duration> = None;
        {
            let mut all = lock(&self.markers);
            all.retain(|w| w.marker.strong_count() > 0);
            for w in all.iter_mut() {
                let Some(s) = w.marker.upgrade() else {
                    continue;
                };
                let threshold = s.threshold.load(Ordering::Acquire);
                if threshold == 0 {
                    continue;
                }
                let marker = TurnMarker(s);
                let Some(now) = marker.in_progress() else {
                    continue;
                };
                if w.turn != now.turn {
                    w.turn = now.turn;
                    w.again = Duration::from_millis(threshold);
                }
                if now.running >= w.again {
                    due.push((Arc::clone(&marker.0), stuck_text(&now)));
                    marker.0.reported.store(now.turn, Ordering::Release);
                    w.again *= 2;
                }
                let wait = w.again.saturating_sub(now.running);
                next = Some(next.map_or(wait, |n| n.min(wait)));
            }
        }
        // Reported outside the lock: a reporter is the host's code.
        for (s, text) in due {
            let reporter = lock(&s.reporter).clone();
            match reporter {
                Some(r) => r(&text),
                None => eprintln!("warning: {text}"),
            }
        }
        next
    }
}

fn stuck_text(t: &TurnInProgress) -> String {
    format!(
        "the UI thread is stuck: turn {} has run for {} ms and has not ended. It was started by \
         {} and is in phase {}",
        t.turn,
        t.running.as_millis(),
        t.started_by,
        t.phase
    )
}

#[cfg(all(test, kernel_timer))]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const PATIENCE: Duration = Duration::from_secs(30);
    const THRESHOLD: Duration = Duration::from_millis(20);

    fn test_watchdog() -> (&'static Watchdog, TurnMarker) {
        let watchdog = Box::leak(Box::new(Watchdog {
            markers: Mutex::new(Vec::new()),
            thread: OnceLock::new(),
            waiter: timer::Waiter::new(),
            wakes: AtomicU64::new(0),
            check_points: Mutex::new(Vec::new()),
        }));
        let marker = TurnMarker::new();
        marker.0.threshold.store(20, Ordering::Release);
        marker.0.timer.get_or_init(|| watchdog.waiter.timer());
        lock(&watchdog.markers).push(Watched {
            marker: Arc::downgrade(&marker.0),
            turn: 0,
            again: Duration::ZERO,
            timer: marker.0.timer.get().unwrap().clone(),
        });
        watchdog.start();
        (watchdog, marker)
    }

    fn begin(marker: &TurnMarker) {
        marker.begin(Instant::now(), Phase::Host, Cause::default());
    }

    #[test]
    fn one_hundred_short_turns_cause_no_watchdog_wakes() {
        let (watchdog, marker) = test_watchdog();
        let before = watchdog.wakes.load(Ordering::Relaxed);

        for _ in 0..100 {
            begin(&marker);
            marker.end();
        }

        let checked = watchdog.check_point();
        checked
            .recv_timeout(PATIENCE)
            .expect("the watchdog did not reach the test checkpoint after 100 short turns");
        let wakes = watchdog.wakes.load(Ordering::Relaxed) - before - 1;
        assert_eq!(wakes, 0, "100 short turns caused {wakes} watchdog wakes");
    }

    #[test]
    fn a_turn_past_its_threshold_is_reported_again_at_double() {
        let (_watchdog, marker) = test_watchdog();
        let (send, receive) = mpsc::channel();
        let reports = Arc::new(Mutex::new(Vec::new()));
        let reported = Arc::clone(&reports);
        let started = Instant::now();
        marker.set_reporter(move |text| {
            let mut reports = lock(&reported);
            reports.push((text.to_owned(), started.elapsed()));
            if reports.len() == 2 {
                let _ = send.send(reports.clone());
            }
        });

        marker.begin(started, Phase::Host, Cause::default());
        let reports = receive.recv_timeout(PATIENCE).unwrap_or_else(|error| {
            panic!("a 20 ms threshold did not produce two reports while the turn ran: {error}; saw {reports:?}")
        });
        assert!(
            marker.in_progress().is_some(),
            "the turn ended before its reports"
        );
        assert!(
            reports[0]
                .0
                .starts_with("the UI thread is stuck: turn 1 has run for "),
            "{reports:?}"
        );
        assert!(
            reports[0].1 >= THRESHOLD,
            "the first report came before its threshold: {reports:?}"
        );
        assert!(
            reports[1].1 >= THRESHOLD * 2,
            "the second report did not wait for the doubled threshold: {reports:?}"
        );
        marker.end();
    }

    #[test]
    fn a_turn_that_ends_before_its_threshold_is_never_reported() {
        let (watchdog, marker) = test_watchdog();
        let (send, receive) = mpsc::channel();
        marker.set_reporter(move |text| {
            let _ = send.send(text.to_owned());
        });

        begin(&marker);
        marker.end();

        let checked = watchdog.check_point();
        checked
            .recv_timeout(PATIENCE)
            .expect("the watchdog did not reach the test checkpoint after the short turn");
        assert_eq!(
            receive.try_recv().ok(),
            None,
            "a turn that ended before its 20 ms threshold was reported"
        );
    }
}
