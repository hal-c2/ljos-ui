//! A turn that runs long (ARCHITECTURE.md §9).
//!
//! Everything a host runs on the UI thread runs in a turn
//! ([`Engine::turn`], [`Engine::begin_turn`]): a closure, an inbox
//! message, a QML handler, a frame of the scene. If that code blocks, the
//! window freezes. The outermost turn is timed; one that ran past the
//! budget ([`Engine::set_turn_budget`]) is a warning through
//! [`Engine::warn`] when it ends, naming what ran. While journaled, a turn
//! of host code (a closure, a message) is a [`JournalKind::Host`] entry,
//! so `app_frames` can say where a wake's time went.
//!
//! # What it costs
//!
//! With no budget and no journal, a turn pays a counter and a flag.
//! Otherwise the outermost turn reads the clock where it starts and where
//! it ends, and nested turns, handlers and bindings read none.
//!
//! [`JournalKind::Host`]: crate::JournalKind::Host

use std::cell::{Cell, RefCell};
use std::fmt;
use std::time::{Duration, Instant};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::journal::JournalRun;

/// The budget of a debug build's shell ([`Engine::watch_turns_by_default`]).
pub const DEFAULT_TURN_BUDGET: Duration = Duration::from_millis(100);
/// Sets the budget in milliseconds; `0` turns it off.
pub const BUDGET_VARIABLE: &str = "LJOS_TURN_BUDGET_MS";
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
    /// What `origin` is called ([`Engine::label_of`]), made only for a
    /// warning.
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
    budget: Cell<Option<Duration>>,
    /// When the outermost turn began; `None` when it is not timed.
    started: Cell<Option<Instant>>,
    /// What the turn in progress ran, for its warning.
    cause: RefCell<Cause>,
    last_warned: Cell<Option<Instant>>,
    /// Warnings not given since the last one.
    unwarned: Cell<u32>,
}

/// A duration given in milliseconds by `variable`: `Some(None)` for 0,
/// `None` when it is not set or not a number.
fn from_env(variable: &str) -> Option<Option<Duration>> {
    let ms: u64 = std::env::var(variable).ok()?.trim().parse().ok()?;
    Some((ms > 0).then(|| Duration::from_millis(ms)))
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
    /// [`DEFAULT_TURN_BUDGET`], in a release build none. [`BUDGET_VARIABLE`]
    /// sets it when it is set, in either build.
    pub fn watch_turns_by_default(&self) {
        let debug = cfg!(debug_assertions);
        let budget = from_env(BUDGET_VARIABLE).unwrap_or(debug.then_some(DEFAULT_TURN_BUDGET));
        self.set_turn_budget(budget);
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
        let journaled = self.is_journaled();
        if depth > 0 || (t.budget.get().is_none() && !journaled) {
            return TurnGuard {
                engine: self,
                outer: false,
                _journaled: None,
            };
        }
        t.started.set(Some(Instant::now()));
        let c = Cause::of(cause, origin);
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
        let Some(started) = t.started.take() else {
            return;
        };
        let took = started.elapsed();
        let Some(budget) = t.budget.get().filter(|b| took > *b) else {
            return;
        };
        let now = Instant::now();
        if t.last_warned
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
            c.label = self.label_of(o).map(|l| Atom::new(&l)).or(ty);
            c.place = place;
        }
        let mut text = format!(
            "a turn of the UI thread took {} ms, longer than its budget of {} ms: {c}",
            took.as_millis(),
            budget.as_millis()
        );
        let unwarned = t.unwarned.replace(0);
        if unwarned > 0 {
            text.push_str(&format!(
                " ({unwarned} more turns over the budget in the second before)"
            ));
        }
        self.warn(text);
    }

    /// The QML handler of the signal `name` starts. The first one in a
    /// turn names the turn.
    pub(crate) fn turn_handler(&self, name: &Atom) {
        let t = &self.inner.turns;
        if t.started.get().is_some() {
            let mut c = t.cause.borrow_mut();
            if c.handler.is_none() {
                c.handler = Some(name.clone());
            }
        }
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
