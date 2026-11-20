//! Content a resource fault took is made again (ARCHITECTURE.md §9.4).
//!
//! A full script heap or an interrupted script is not a bug in what was
//! being made: what filled the heap may let go, a collection may make
//! room, and what ran long may not run long again. So whatever owns made
//! content (a Loader, a view's delegates, a singleton, a boundary, a
//! `Task`) makes it again by itself after such a fault, and never gives
//! up for good: Qt has neither limit (a script that never returns hangs
//! it, and running out of memory ends it), so this is Ljós's own.
//!
//! A fault soon after the last try ([`SECOND_CHANCE`]) makes the owner
//! wait before the next, [`FIRST_WAIT`] and twice as long each time after,
//! up to [`LONGEST_WAIT`] ([`Backoff`]): content that runs out each time
//! costs one try in that long. A code bug (an exception, a panic, a
//! compile error) says the same each time and is not retried this way.
//!
//! Owners with a timer of their own to cancel keep a [`Retry`]; one that
//! is asked for its content (a singleton) keeps a [`Backoff`] and looks at
//! the time when asked, since nothing must poll.

use std::time::Duration;

use ljos_runtime::{Fault, Time, TimerId};

use crate::engine::Engine;
use crate::error::{EngineError, JsError};
use crate::{ObjectId, Value};

/// How long after a try another resource fault counts as the content's
/// own doing, and the owner waits before it tries again.
pub const SECOND_CHANCE: Duration = Duration::from_secs(10);

/// The wait before the try that follows a fault soon after the last try;
/// each such fault after it doubles the wait.
pub const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between tries: content that runs out of time or
/// memory each time it is made is made once in this long, and never
/// given up on.
pub const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// A fault that is the machine's state, not the code's: trying again
/// may work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceFault {
    /// A script ran past its time and was stopped.
    Interrupted,
    /// The script heap was full.
    OutOfMemory,
}

impl ResourceFault {
    /// What `fault` is, when it is a resource fault.
    pub fn of(fault: &Fault) -> Option<Self> {
        match fault {
            Fault::JsInterrupted { .. } => Some(Self::Interrupted),
            Fault::JsOutOfMemory => Some(Self::OutOfMemory),
            _ => None,
        }
    }

    /// The error of content that `self` cut off, saying `message`.
    pub fn error(self, message: impl Into<String>) -> EngineError {
        match self {
            Self::Interrupted => EngineError::JsInterrupted(message.into()),
            Self::OutOfMemory => EngineError::JsOutOfMemory(message.into()),
        }
    }

    /// `"a script was interrupted"`, for a log line about what was cut.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Interrupted => "a script was interrupted",
            Self::OutOfMemory => "the script heap was full",
        }
    }
}

impl EngineError {
    /// What this error is, when a resource fault caused it.
    pub fn resource_fault(&self) -> Option<ResourceFault> {
        match self {
            Self::JsInterrupted(_) => Some(ResourceFault::Interrupted),
            Self::JsOutOfMemory(_) => Some(ResourceFault::OutOfMemory),
            _ => None,
        }
    }
}

impl JsError {
    /// What cut the call off, when a resource fault did.
    pub fn resource_fault(&self) -> Option<ResourceFault> {
        match self {
            Self::Stopped { cause, .. } => Some(*cause),
            _ => None,
        }
    }
}

impl Engine {
    /// Calls `f` as `owner`'s code, for an owner that deals with a
    /// resource fault in it itself (a `Task` starting its run again after
    /// a wait): an error is given back, nothing is logged, and an
    /// interrupt that also cuts off the dispatch the call is nested in (a
    /// promise job) is not that dispatch's fault.
    pub fn try_call_js_as(
        &self,
        owner: ObjectId,
        f: &Value,
        args: &[Value],
    ) -> Result<Value, JsError> {
        let r = {
            let _origin = self.origin(Some(owner));
            self.try_call_js(f, args)
        };
        if let Err(err) = &r
            && err.resource_fault() == Some(ResourceFault::Interrupted)
            && self.js_interrupted()
        {
            self.inner.faults.dealt_with.borrow_mut().push(owner);
        }
        r
    }

    /// Whether `owner` dealt with the interrupt of its run already
    /// ([`Engine::try_call_js_as`]), forgetting it.
    pub(crate) fn interrupt_dealt_with(&self, owner: ObjectId) -> bool {
        let mut dealt = self.inner.faults.dealt_with.borrow_mut();
        let at = dealt.iter().position(|o| *o == owner);
        at.map(|i| dealt.swap_remove(i)).is_some()
    }
}

/// When an owner tries again after resource faults: at once after the
/// first, and after a wait for each that comes within [`SECOND_CHANCE`] of
/// the try before it, from [`FIRST_WAIT`] doubling up to [`LONGEST_WAIT`].
/// There is no last time: the fault may have been something else's
/// (another item filling the heap), and what caused it may go away.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Backoff {
    /// When the owner last tried again for such a fault.
    last_load: Option<Time>,
    /// How many such faults in a row came soon after the try before.
    streak: u32,
}

impl Backoff {
    /// A fault at `at` the owner tries again for: how long it waits first.
    pub fn fault(&mut self, at: Time) -> Duration {
        let soon = self
            .last_load
            .is_some_and(|t| at.saturating_since(t) < SECOND_CHANCE);
        self.streak = if soon {
            self.streak.saturating_add(1)
        } else {
            0
        };
        wait_for(self.streak)
    }

    /// The owner tries again at `at`.
    pub fn loading(&mut self, at: Time) {
        self.last_load = Some(at);
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

/// `1 s`, `1.5 s`: a wait, for the log.
pub fn seconds(d: Duration) -> String {
    format!("{} s", d.as_secs_f64())
}

/// A [`Backoff`] and the timer of the try it waits for: what an owner of
/// made content keeps to make it again after a resource fault.
#[derive(Debug, Default)]
pub struct Retry {
    backoff: Backoff,
    timer: Option<TimerId>,
}

impl Retry {
    /// After `cause`, runs `again` once the backoff's wait is over, never
    /// in the turn that faulted (the content that failed may be on the
    /// stack). Replaces a try that was waiting. Gives the wait, for the
    /// log. A full heap is collected first, so the try finds what the
    /// collector frees.
    pub fn schedule(
        &mut self,
        e: &Engine,
        cause: ResourceFault,
        again: impl FnOnce(&Engine) + 'static,
    ) -> Duration {
        let now = e.now();
        let wait = self.backoff.fault(now);
        // Counted from when the try is due, as the owner tries then; one
        // told to make its content sooner drops the wait with `cancel`.
        self.backoff.loading(now + wait);
        self.cancel(e);
        self.timer = Some(e.call_at(now + wait, move |e| {
            if cause == ResourceFault::OutOfMemory {
                e.collect_garbage();
            }
            again(e);
        }));
        wait
    }

    /// Drops the try that was waiting: the owner was told to make its
    /// content, or is going. The backoff stays, so a fault soon after
    /// still waits longer.
    pub fn cancel(&mut self, e: &Engine) {
        if let Some(t) = self.timer.take() {
            e.cancel_call(t);
        }
    }

    /// Whether a try is waiting. A timer that ran is forgotten by the
    /// owner's `again`, through [`Retry::ran`].
    pub fn waiting(&self) -> bool {
        self.timer.is_some()
    }

    /// The try ran: there is no timer to cancel.
    pub fn ran(&mut self) {
        self.timer = None;
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// What happens to an owner whose content faults over and over: when,
    /// after each try, the next fault comes.
    #[derive(Clone, Debug)]
    enum Step {
        /// The content faults this long after it was made.
        Faults(Duration),
        /// The content stays up this long, then faults.
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
        /// by a try (never a last time), a fault after content that lasted
        /// is tried again at once, and a streak of quick faults waits
        /// twice as long each time, from a second up to a minute.
        #[test]
        fn an_owner_always_tries_again_and_waits_as_the_model_says(
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
                b.loading(now);
                model_streak = Some(streak);
            }
        }

        /// Content that faults at once every time is made at most once a
        /// minute once the waits are long, and still made.
        #[test]
        fn runaway_content_is_tried_at_a_bounded_rate(n in 8u32..200) {
            let mut b = Backoff::default();
            let mut now = Time::ZERO;
            let mut tries = Vec::new();
            for _ in 0..n {
                now += b.fault(now);
                b.loading(now);
                tries.push(now);
            }
            prop_assert_eq!(tries.len() as u32, n);
            for w in tries.windows(2).skip(7) {
                prop_assert_eq!(w[1].saturating_since(w[0]), LONGEST_WAIT);
            }
        }
    }

    #[test]
    fn waits_double_from_a_second_to_a_minute() {
        let waits: Vec<u64> = (0..10).map(|n| wait_for(n).as_secs()).collect();
        assert_eq!(waits, [0, 1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(wait_for(u32::MAX), LONGEST_WAIT);
    }

    #[test]
    fn only_a_full_heap_and_an_interrupt_are_resource_faults() {
        let interrupted = Fault::JsInterrupted {
            slice: Duration::from_millis(5),
        };
        assert_eq!(
            ResourceFault::of(&interrupted),
            Some(ResourceFault::Interrupted)
        );
        assert_eq!(
            ResourceFault::of(&Fault::JsOutOfMemory),
            Some(ResourceFault::OutOfMemory)
        );
        assert_eq!(ResourceFault::of(&Fault::Panic("x".into())), None);
        assert_eq!(ResourceFault::of(&Fault::Startup("x".into())), None);
        assert_eq!(
            EngineError::JsOutOfMemory(String::new()).resource_fault(),
            Some(ResourceFault::OutOfMemory)
        );
        assert_eq!(EngineError::Js(String::new()).resource_fault(), None);
    }
}
