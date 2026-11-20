//! What turns did, recorded for a tool that judges an application's work
//! ([`Engine::record_turns`]): an audit that fails on a turn that freezes
//! the window.
//!
//! A turn's work is the script it ran, counted in the interpreter's polls
//! ([`Engine::script_work`]) rather than timed: the same scripts do the
//! same work on every machine and under any load, so a recorded turn is
//! over a bound in a test the same way on a laptop and on CI. The wall
//! time is kept beside it, for what no poll counts (layout, drawing,
//! native code), and is only as steady as the machine.

use std::time::Duration;

use super::Cause;
use crate::atom::Atom;
use crate::engine::Engine;
use crate::ids::ObjectId;

/// How many turns a record keeps of the heaviest, and of the slowest.
pub const KEPT_TURNS: usize = 16;

/// A QML signal handler that ran in a recorded turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedHandler {
    /// The handler as written: `onRowsChanged`.
    pub name: String,
    /// The type of the object it belongs to.
    pub object: Option<String>,
    /// `file:line:column` of the handler.
    pub at: Option<String>,
    /// The script work it did, with that of the handlers it set off.
    pub work: u64,
}

/// One outermost turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedTurn {
    /// Counts the recorded turns from 1.
    pub number: u64,
    /// What started it, in the words of a turn's budget warning.
    pub started_by: String,
    /// The script work it did ([`Engine::script_work`]).
    pub work: u64,
    /// How long it took on the wall clock.
    pub took: Duration,
    /// The handler in it that did the most script work.
    pub heaviest: Option<RecordedHandler>,
}

/// What the turns did since recording began.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnRecord {
    /// Outermost turns that ended.
    pub turns: u64,
    /// The script work of all of them.
    pub work: u64,
    /// The turns that did the most script work, most first; at most
    /// [`KEPT_TURNS`].
    pub heaviest: Vec<RecordedTurn>,
    /// The turns that took longest on the wall clock, longest first; at
    /// most [`KEPT_TURNS`].
    pub slowest: Vec<RecordedTurn>,
}

/// A handler running in the turn: its name, its object, where it is, and
/// the work done when it began.
struct Open {
    name: Atom,
    obj: ObjectId,
    at: Option<String>,
    began: u64,
}

#[derive(Default)]
pub(super) struct Recorder {
    record: TurnRecord,
    /// Whether this recorder saw the turn in progress begin: a recording
    /// turned on within a turn starts with the next one, as the work done
    /// before it is not this record's.
    in_turn: bool,
    /// The work done when the turn in progress began.
    began: u64,
    open: Vec<Open>,
    /// The heaviest handler of the turn in progress.
    heaviest: Option<RecordedHandler>,
}

/// Keeps `turn` in `kept` when it is among the [`KEPT_TURNS`] largest by
/// `key`, which `kept` is sorted by, largest first.
fn keep<K: Ord>(
    kept: &mut Vec<RecordedTurn>,
    turn: RecordedTurn,
    key: impl Fn(&RecordedTurn) -> K,
) {
    let at = kept.partition_point(|t| key(t) >= key(&turn));
    if at < KEPT_TURNS {
        kept.insert(at, turn);
        kept.truncate(KEPT_TURNS);
    }
}

/// Whether `kept`, sorted largest first by `key`, would take a turn of
/// `size`.
fn would_keep<K: Ord>(kept: &[RecordedTurn], size: K, key: impl Fn(&RecordedTurn) -> K) -> bool {
    kept.len() < KEPT_TURNS || kept.last().is_some_and(|t| key(t) < size)
}

impl Engine {
    /// Records what each turn does from now on (`true`), in a fresh
    /// record, or stops and drops the record (`false`). A recording that
    /// is on stays as it is when turned on again. A tool that judges an
    /// application's turns (an audit) turns it on before the application
    /// loads; each turn then reads the clock and the script's work where it
    /// starts and ends, and each handler its place.
    pub fn record_turns(&self, on: bool) {
        let mut record = self.inner.turns.record.borrow_mut();
        match (on, record.is_some()) {
            (true, false) => *record = Some(Recorder::default()),
            (false, _) => *record = None,
            _ => {}
        }
    }

    /// What the turns did since [`Engine::record_turns`] turned recording
    /// on; `None` while it is off.
    pub fn recorded_turns(&self) -> Option<TurnRecord> {
        let record = self.inner.turns.record.borrow();
        record.as_ref().map(|r| r.record.clone())
    }

    /// Starts recording the outermost turn beginning. Whether it records.
    pub(super) fn begin_recorded_turn(&self) -> bool {
        let work = self.script_work();
        let mut record = self.inner.turns.record.borrow_mut();
        let Some(r) = record.as_mut() else {
            return false;
        };
        r.began = work;
        r.in_turn = true;
        r.open.clear();
        r.heaviest = None;
        true
    }

    /// Whether the turn in progress is recorded.
    pub(super) fn recording_turn(&self) -> bool {
        self.inner
            .turns
            .record
            .borrow()
            .as_ref()
            .is_some_and(|r| r.in_turn)
    }

    /// Ends the recorded turn, started by `cause`. Naming a kept turn can
    /// run application code (an `objectName` binding), and so start a turn
    /// of its own: `cause` is the caller's, not the turn state's.
    pub(super) fn end_recorded_turn(&self, took: Duration, cause: &Cause) {
        let work = self.script_work();
        let mut record = self.inner.turns.record.borrow_mut();
        let Some(r) = record.as_mut() else {
            return;
        };
        if !std::mem::take(&mut r.in_turn) {
            return;
        }
        let done = work.saturating_sub(r.began);
        r.record.turns += 1;
        r.record.work += done;
        let heavy = would_keep(&r.record.heaviest, done, |t| t.work);
        let slow = would_keep(&r.record.slowest, took, |t| t.took);
        if !(heavy || slow) {
            return;
        }
        let number = r.record.turns;
        let heaviest = r.heaviest.take();
        drop(record);
        // Named only now, and only for a turn that is kept: the words cost
        // what a turn should not pay for.
        let mut c = cause.clone();
        if let Some(o) = c.origin {
            let (ty, place) = self.type_and_place(o);
            c.label = self.label_of(o).map(|l| Atom::new(&l)).or(ty).or(c.label);
            c.place = place.or(c.place);
        }
        let turn = RecordedTurn {
            number,
            started_by: c.to_string(),
            work: done,
            took,
            heaviest,
        };
        let mut record = self.inner.turns.record.borrow_mut();
        let Some(r) = record.as_mut() else {
            return;
        };
        if heavy && slow {
            keep(&mut r.record.heaviest, turn.clone(), |t| t.work);
        } else if heavy {
            keep(&mut r.record.heaviest, turn, |t| t.work);
            return;
        }
        if slow {
            keep(&mut r.record.slowest, turn, |t| t.took);
        }
    }

    /// Starts recording a handler of `obj`. Whether it records: only in a
    /// recorded turn.
    pub(super) fn begin_recorded_handler(
        &self,
        name: &Atom,
        obj: ObjectId,
        at: impl FnOnce() -> Option<String>,
    ) -> bool {
        let t = &self.inner.turns;
        if t.started.get().is_none() || !self.recording_turn() {
            return false;
        }
        let began = self.script_work();
        let at = at();
        let mut record = t.record.borrow_mut();
        let Some(r) = record.as_mut() else {
            return false;
        };
        r.open.push(Open {
            name: name.clone(),
            obj,
            at,
            began,
        });
        true
    }

    pub(super) fn end_recorded_handler(&self) {
        let work = self.script_work();
        let open = {
            let mut record = self.inner.turns.record.borrow_mut();
            let Some(r) = record.as_mut() else {
                return;
            };
            let Some(open) = r.open.pop() else {
                return;
            };
            let done = work.saturating_sub(open.began);
            // The outermost of equally heavy handlers names the turn: it
            // is where the work came in.
            if r.heaviest.as_ref().is_some_and(|h| h.work > done) {
                return;
            }
            (open, done)
        };
        let (open, done) = open;
        let object = self.type_and_place(open.obj).0.map(|a| a.to_string());
        let handler = RecordedHandler {
            name: crate::qtqml::handler_name(&open.name),
            object,
            at: open.at,
            work: done,
        };
        if let Some(r) = self.inner.turns.record.borrow_mut().as_mut() {
            r.heaviest = Some(handler);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Engine, TurnCause};

    #[test]
    fn a_recording_turned_on_within_a_turn_starts_with_the_next() {
        let e = Engine::new();
        e.record_turns(true);
        {
            let _turn = e.begin_turn(TurnCause::Host, None);
            // Off and on again: a fresh record, which did not see this
            // turn begin and so cannot say what it did.
            e.record_turns(false);
            e.record_turns(true);
        }
        assert_eq!(e.recorded_turns().map(|r| r.turns), Some(0));
        drop(e.begin_turn(TurnCause::Host, None));
        assert_eq!(e.recorded_turns().map(|r| r.turns), Some(1));
    }
}
