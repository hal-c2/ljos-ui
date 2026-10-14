//! The slow native call watchdog (ARCHITECTURE.md §3.4, §9): the
//! observer half of the budgeting story for native methods. A native
//! method runs synchronously on the UI thread and cannot be preempted,
//! so this does not interrupt anything — it names the call that froze
//! the frame after the fact. Script methods are the JS budget's
//! business and are not timed here.
//!
//! The watchdog is off unless a budget is set
//! ([`Engine::set_slow_call_budget`]); while off, [`Engine::invoke`]
//! pays one branch on the budget and never reads a clock.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::atom::Atom;
use crate::engine::Engine;

/// Calls kept for [`Engine::take_slow_calls`].
const MAX_KEPT: usize = 64;
/// How often one method may be named in the log. Records are not
/// limited; this only keeps a hot loop from flooding the log.
const LOG_EVERY: Duration = Duration::from_secs(1);

/// One native call that ran over the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlowCall {
    /// The QML type the method belongs to.
    pub ty: String,
    pub method: String,
    pub took: Duration,
}

#[derive(Default)]
pub(crate) struct SlowState {
    budget: Cell<Option<Duration>>,
    kept: RefCell<VecDeque<SlowCall>>,
    count: Cell<u64>,
    /// When each (type, method) was last logged, for the rate limit.
    /// There is one entry per native method a type declares, so this
    /// cannot grow with the number of calls.
    last_logged: RefCell<HashMap<(Atom, Atom), Instant>>,
}

impl SlowState {
    pub(crate) fn budget(&self) -> Option<Duration> {
        self.budget.get()
    }
}

impl Engine {
    /// Times native calls against `budget` and records the ones that run
    /// over it ([`Engine::take_slow_calls`], [`Engine::slow_call_count`]),
    /// naming each in the log at most once a second per method. `None`
    /// (the default) turns the watchdog off: no clock is read and
    /// nothing is recorded. Neither setting clears the calls kept so
    /// far or the count.
    pub fn set_slow_call_budget(&self, budget: Option<Duration>) {
        self.inner.slow.budget.set(budget);
    }

    /// The budget native calls are timed against, if the watchdog is on.
    pub fn slow_call_budget(&self) -> Option<Duration> {
        self.inner.slow.budget.get()
    }

    /// Drains the calls that ran over budget, the oldest first. At most
    /// the last 64 are kept; [`Engine::slow_call_count`] has them all.
    pub fn take_slow_calls(&self) -> Vec<SlowCall> {
        self.inner.slow.kept.borrow_mut().drain(..).collect()
    }

    /// How many native calls have run over budget since the engine was
    /// made. Never reset by [`Engine::take_slow_calls`].
    pub fn slow_call_count(&self) -> u64 {
        self.inner.slow.count.get()
    }

    /// Holds one over-budget call: kept, counted, and logged unless the
    /// same method was logged within the last second.
    pub(crate) fn record_slow_call(
        &self,
        ty: Atom,
        method: Atom,
        took: Duration,
        budget: Duration,
    ) {
        let state = &self.inner.slow;
        state.count.set(state.count.get().saturating_add(1));
        {
            let mut kept = state.kept.borrow_mut();
            if kept.len() == MAX_KEPT {
                kept.pop_front();
            }
            kept.push_back(SlowCall {
                ty: ty.to_string(),
                method: method.to_string(),
                took,
            });
        }
        // The borrows are released before logging: a log handler may run
        // more of the engine.
        let now = Instant::now();
        let log = {
            let mut last = state.last_logged.borrow_mut();
            match last.get(&(ty.clone(), method.clone())) {
                Some(at) if now.duration_since(*at) < LOG_EVERY => false,
                _ => {
                    last.insert((ty.clone(), method.clone()), now);
                    true
                }
            }
        };
        if log {
            self.warn(format!(
                "slow native call: {ty}.{method} took {} ms (budget {} ms)",
                took.as_millis(),
                budget.as_millis()
            ));
        }
    }
}
