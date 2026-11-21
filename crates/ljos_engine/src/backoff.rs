//! When to try again after a fault that was not the code's own.
//!
//! A full script heap, an interrupted script or a panic in native work
//! may be over by the next try: what filled the heap may let go, what ran
//! long may have finished. So whatever such a fault stopped (a `Loader`'s
//! item, a view's delegate, a singleton, an `ErrorBoundary`'s content) is
//! tried again, with no last time. Qt has none of these faults to recover
//! from: a script that never returns hangs it, and running out of memory
//! ends it.
//!
//! [`Backoff`] spaces the tries: the first at once, then, for each fault
//! that comes soon after the try before it ([`SECOND_CHANCE`]), a wait
//! from [`FIRST_WAIT`] doubling up to [`LONGEST_WAIT`]. Something that
//! fails every time costs one try a minute.

use std::cell::Cell;
use std::time::Duration;

use ljos_reactive::NodeId;
use ljos_runtime::{Fault, Time, TimerId};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::value::Value;

/// How long after a try a fault counts as that try's own, and makes the
/// next one wait.
pub const SECOND_CHANCE: Duration = Duration::from_secs(10);

/// The wait before the try that follows a fault soon after the last try;
/// each such fault after it doubles the wait.
pub const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between tries: what fails each time is tried once in
/// this long, and never given up on.
pub const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// When to try again after faults that are not the code's own: at once
/// after the first, and after a wait for each that comes within
/// [`SECOND_CHANCE`] of the try before it, from [`FIRST_WAIT`] doubling
/// up to [`LONGEST_WAIT`]. There is no last time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Backoff {
    /// When it was last tried again for such a fault.
    last_try: Option<Time>,
    /// How many such faults in a row came soon after the try before.
    streak: u32,
}

impl Backoff {
    /// A fault at `at` to try again for: how long to wait first.
    pub fn fault(&mut self, at: Time) -> Duration {
        let soon = self
            .last_try
            .is_some_and(|t| at.saturating_since(t) < SECOND_CHANCE);
        self.streak = if soon {
            self.streak.saturating_add(1)
        } else {
            0
        };
        wait_for(self.streak)
    }

    /// It is tried again at `at`.
    pub fn trying(&mut self, at: Time) {
        self.last_try = Some(at);
    }
}

/// The wait before a try that follows `streak` faults in a row, each soon
/// after the try before: none for the first.
pub fn wait_for(streak: u32) -> Duration {
    match streak {
        0 => Duration::ZERO,
        n => FIRST_WAIT
            .saturating_mul(1_u32.checked_shl(n - 1).unwrap_or(u32::MAX))
            .min(LONGEST_WAIT),
    }
}

/// `1 s`, `1.5 s`: a wait as the log words it.
pub fn seconds(d: Duration) -> String {
    format!("{} s", d.as_secs_f64())
}

impl EngineError {
    /// Whether this is a fault the code did not cause and a later try may
    /// not meet: a full script heap, an interrupted script, a panic in
    /// native work. What it stopped is tried again with a [`Backoff`].
    pub fn is_resource_fault(&self) -> bool {
        matches!(
            self,
            EngineError::JsOutOfMemory(_) | EngineError::JsInterrupted(_) | EngineError::Panic(_)
        )
    }
}

/// Whether `fault` is one the code did not cause
/// ([`EngineError::is_resource_fault`]).
pub fn is_resource_fault(fault: &Fault) -> bool {
    matches!(
        fault,
        Fault::JsOutOfMemory | Fault::JsInterrupted { .. } | Fault::Panic(_)
    )
}

/// The runs a resource fault cut short, run again: every binding whose
/// run met one depends on `node` from that run, and a timer bumps it once
/// the wait its [`Backoff`] says is over. A binding whose script never
/// loaded read nothing else, so nothing else would ever run it again.
#[derive(Default)]
struct RunAgain {
    backoff: Cell<Backoff>,
    node: Cell<Option<NodeId>>,
    timer: Cell<Option<TimerId>>,
    /// A fault since the last bump was a full heap: collect before.
    full: Cell<bool>,
    /// Every resource fault so far that leaves what met it less than
    /// whole (a full heap, a panic), and the last: what made something
    /// tells by them whether one broke it. An interrupted script is not
    /// one: the binding it cut short runs again here, and a script that
    /// always runs over its slice would otherwise be made forever.
    seen: Cell<u64>,
    last: std::cell::RefCell<Option<Fault>>,
}

impl Engine {
    /// A resource fault (`fault`) cut short what runs now: the binding
    /// running, if any, is run again after a wait, as is every other one
    /// cut short until then. Called where every fault is reported.
    pub(crate) fn run_again_after(&self, fault: &Fault) {
        if matches!(fault, Fault::JsOutOfMemory) {
            self.note_heap_ran_out();
        }
        let r = self.extension::<RunAgain>();
        if !is_resource_fault(fault) {
            // While a resource fault waits to be tried again, a script
            // error may be its doing (a type whose script never loaded):
            // the binding it stopped runs again with the rest. One that is
            // the code's own fails again, once a wait, until the waits end.
            if let (Some(node), Some(_)) = (r.node.get(), r.timer.get()) {
                let _ = self.inner.graph.try_get(node);
            }
            return;
        }
        if !matches!(fault, Fault::JsInterrupted { .. }) {
            r.seen.set(r.seen.get() + 1);
            *r.last.borrow_mut() = Some(fault.clone());
        }
        let g = &self.inner.graph;
        // Outside a binding, what met the fault has its own way back (a
        // Loader's, a view's, a singleton's) or is the caller's to retry.
        if !g.is_tracking() {
            return;
        }
        let node = match r.node.get() {
            Some(n) => n,
            None => match g.property(g.root(), Value::Int(0)) {
                Ok(n) => {
                    r.node.set(Some(n));
                    n
                }
                Err(_) => return,
            },
        };
        // Read where it is tracked: the binding cut short depends on it.
        let _ = g.try_get(node);
        if matches!(fault, Fault::JsOutOfMemory) {
            r.full.set(true);
        }
        if r.timer.get().is_some() {
            return;
        }
        let now = self.now();
        let mut backoff = r.backoff.get();
        let wait = backoff.fault(now);
        r.backoff.set(backoff);
        // Never at once: the fault is in the middle of a run.
        let timer = self.call_at(now + wait, move |e| {
            let r = e.extension::<RunAgain>();
            r.timer.set(None);
            // Timers run between scripts, where the heap can be.
            if r.full.take() {
                e.collect_garbage_for_full_heap();
            }
            let mut backoff = r.backoff.get();
            backoff.trying(e.now());
            r.backoff.set(backoff);
            let g = &e.inner.graph;
            let next = match g.get_untracked(node) {
                Value::Int(i) => i.wrapping_add(1),
                _ => 1,
            };
            g.set(node, Value::Int(next));
        });
        r.timer.set(Some(timer));
    }

    /// How many resource faults that break what met them there have
    /// been: compare with a later count by [`Engine::resource_fault_since`].
    pub(crate) fn resource_faults_seen(&self) -> u64 {
        self.extension::<RunAgain>().seen.get()
    }

    /// The last full heap or panic, if there was one after `seen`
    /// ([`Engine::resource_faults_seen`]): what something made since then
    /// met, and is not whole for, though no error came back. Interrupted
    /// scripts are left out: [`Engine::run_again_after`] runs them again.
    pub(crate) fn resource_fault_since(&self, seen: u64) -> Option<EngineError> {
        let r = self.extension::<RunAgain>();
        if r.seen.get() == seen {
            return None;
        }
        Some(match r.last.borrow().as_ref()? {
            Fault::JsOutOfMemory => {
                EngineError::JsOutOfMemory("it ran out of script memory while it was made".into())
            }
            Fault::Panic(m) => EngineError::Panic(format!("a panic while it was made: {m}")),
            _ => return None,
        })
    }

    /// The wait for [`Engine::run_again_after`] dropped once nothing cut
    /// short is left to run again: a boundary that took the fault threw
    /// those bindings away with its tree, and makes its own tries.
    pub(crate) fn drop_idle_run_again(&self) {
        let r = self.extension::<RunAgain>();
        let (Some(node), Some(timer)) = (r.node.get(), r.timer.get()) else {
            return;
        };
        if !self.inner.graph.has_observers(node) {
            self.cancel_call(timer);
            r.timer.set(None);
            r.full.set(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// What happens to something that faults over and over: when, after
    /// each try, the next fault comes.
    #[derive(Clone, Debug)]
    enum Step {
        /// It faults this long after it was tried.
        Faults(Duration),
        /// It stays up this long, then faults.
        Lasts(Duration),
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            (0..SECOND_CHANCE.as_millis() as u64)
                .prop_map(|ms| Step::Faults(Duration::from_millis(ms))),
            (SECOND_CHANCE.as_millis() as u64..10 * SECOND_CHANCE.as_millis() as u64)
                .prop_map(|ms| Step::Lasts(Duration::from_millis(ms))),
        ]
    }

    proptest! {
        /// Run against a model of the state machine: every fault is followed
        /// by a try (never a last time), a fault after something that lasted
        /// is tried again at once, and a streak of quick faults waits twice
        /// as long each time, from a second up to a minute.
        #[test]
        fn it_always_tries_again_and_waits_as_the_model_says(
            steps in proptest::collection::vec(step(), 1..40),
        ) {
            let mut b = Backoff::default();
            let mut now = Time::ZERO + Duration::from_secs(1);
            // The first try is the application's own, not counted.
            let mut model_streak: Option<u32> = None;
            for s in steps {
                let (quick, after) = match s {
                    Step::Faults(d) => (true, d),
                    Step::Lasts(d) => (false, d),
                };
                now += after;
                let wait = b.fault(now);
                let streak = match (model_streak, quick) {
                    (Some(n), true) => n + 1,
                    _ => 0,
                };
                let expected = if streak == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1u64 << (streak - 1).min(6)).min(LONGEST_WAIT)
                };
                prop_assert_eq!(wait, expected);
                prop_assert!(wait <= LONGEST_WAIT);
                now += wait;
                b.trying(now);
                model_streak = Some(streak);
            }
        }

        /// What faults at once every time is tried at most once a minute
        /// once the waits are long, and still tried.
        #[test]
        fn a_runaway_is_tried_at_a_bounded_rate(n in 8u32..200) {
            let mut b = Backoff::default();
            let mut now = Time::ZERO;
            let mut tries = Vec::new();
            for _ in 0..n {
                now += b.fault(now);
                b.trying(now);
                tries.push(now);
            }
            prop_assert_eq!(tries.len() as u32, n);
            for w in tries.windows(2).skip(7) {
                prop_assert_eq!(w[1].saturating_since(w[0]), LONGEST_WAIT);
            }
        }
    }

    /// What happens around a binding that a full heap can cut short.
    #[derive(Clone, Debug)]
    enum Op {
        /// The heap fills: the binding's next run is cut short.
        Fill,
        /// It has room again.
        Free,
        /// Time passes, and the timers due run.
        Wait(u64),
        /// Something reads the binding.
        Read,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            Just(Op::Fill),
            Just(Op::Free),
            (0u64..90_000).prop_map(Op::Wait),
            Just(Op::Read),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Run against a model: a binding a resource fault cut short keeps
        /// its value, is run again (when read) once the wait is over and
        /// not before, always has a try ahead of it no further off than
        /// [`LONGEST_WAIT`], and gives its value once there is room.
        #[test]
        fn a_binding_cut_short_runs_again_as_the_model_says(
            ops in proptest::collection::vec(op(), 1..40),
        ) {
            use std::rc::Rc;

            use ljos_runtime::{Clock, VirtualClock};

            let e = Engine::new();
            let clock = VirtualClock::new();
            e.set_clock(Rc::new(clock.clone()));
            let full = Rc::new(Cell::new(false));
            let runs = Rc::new(Cell::new(0u32));
            let g = &e.inner.graph;
            let (f, r, me) = (full.clone(), runs.clone(), e.clone());
            // Cut short, it reads nothing: what a binding whose script did
            // not load does.
            let b = g
                .binding(g.root(), Value::Int(0), move |_| {
                    r.set(r.get() + 1);
                    if f.get() {
                        me.run_again_after(&Fault::JsOutOfMemory);
                        Value::Int(-1)
                    } else {
                        Value::Int(1)
                    }
                })
                .unwrap();
            let (mut m_full, mut stale, mut value, mut m_runs) = (false, true, 0, 0);
            for op in ops {
                match op {
                    Op::Fill => {
                        full.set(true);
                        m_full = true;
                    }
                    Op::Free => {
                        full.set(false);
                        m_full = false;
                    }
                    Op::Wait(ms) => {
                        let to = clock.now() + Duration::from_millis(ms);
                        let fires = e.next_timer_deadline().is_some_and(|d| d <= to);
                        clock.set(to);
                        e.advance_timers(clock.now());
                        if fires && value == -1 {
                            stale = true;
                        }
                    }
                    Op::Read => {
                        let v = g.get(b);
                        if stale {
                            m_runs += 1;
                            value = if m_full { -1 } else { 1 };
                            stale = false;
                        }
                        prop_assert_eq!(v, Value::Int(value));
                        prop_assert_eq!(runs.get(), m_runs);
                    }
                }
                // Never given up on, never put off past a minute.
                if value == -1 && !stale {
                    let due = e.next_timer_deadline();
                    prop_assert!(due.is_some(), "a binding cut short has no try ahead");
                    prop_assert!(due.unwrap().saturating_since(clock.now()) <= LONGEST_WAIT);
                }
            }
            // Room again: at the next try, it gives its value.
            full.set(false);
            if let Some(due) = e.next_timer_deadline() {
                clock.set(due);
                e.advance_timers(clock.now());
            }
            prop_assert_eq!(g.get(b), Value::Int(1));
        }
    }

    #[test]
    fn waits_double_from_a_second_to_a_minute() {
        let waits: Vec<u64> = (0..10).map(|n| wait_for(n).as_secs()).collect();
        assert_eq!(waits, [0, 1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(wait_for(u32::MAX), LONGEST_WAIT);
    }
}
