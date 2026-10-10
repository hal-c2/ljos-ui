//! Time budgets for one frame and for one job slice.

use std::cell::Cell;
use std::time::Duration;

use crate::time::{Clock, Time};

/// Time allotted to a whole frame's lane work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameBudget {
    pub start: Time,
    pub deadline: Time,
    /// Longest single `Job::poll` slice. Keeps one job from eating the whole
    /// budget and gives round-robin lanes a chance to rotate.
    pub max_slice: Duration,
}

impl FrameBudget {
    /// Slice length used when none is given: roughly the JS interrupt period
    /// times a few, so incubation yields several times per frame.
    pub const DEFAULT_SLICE: Duration = Duration::from_millis(2);
    /// Never plan for less than this, or no job could make progress.
    pub const MIN_BUDGET: Duration = Duration::from_millis(1);

    pub fn new(start: Time, budget: Duration) -> Self {
        Self {
            start,
            deadline: start + budget,
            max_slice: Self::DEFAULT_SLICE,
        }
    }

    /// Adaptive budget: the refresh period minus what the last frames spent
    /// rendering (encode + present), floored at [`Self::MIN_BUDGET`].
    pub fn adaptive(start: Time, refresh_period: Duration, render_cost: Duration) -> Self {
        let budget = refresh_period
            .saturating_sub(render_cost)
            .max(Self::MIN_BUDGET);
        Self::new(start, budget)
    }

    pub fn with_max_slice(mut self, max_slice: Duration) -> Self {
        self.max_slice = max_slice;
        self
    }

    pub fn expired(&self, now: Time) -> bool {
        now >= self.deadline
    }

    pub fn remaining(&self, now: Time) -> Duration {
        self.deadline - now
    }
}

/// How long one job may run before it must return `JobPoll::Yield`. The Erlang
/// "reductions" of this runtime. `!Sync` (it caches expiry in a `Cell`) because
/// a slice belongs to the UI thread's current turn.
///
/// The JS host copies [`SliceBudget::deadline`] into its interrupt handler.
pub struct SliceBudget<'a> {
    clock: &'a dyn Clock,
    start: Time,
    deadline: Time,
    expired: Cell<bool>,
}

impl std::fmt::Debug for SliceBudget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SliceBudget")
            .field("start", &self.start)
            .field("deadline", &self.deadline)
            .field("expired", &self.expired.get())
            .finish()
    }
}

impl<'a> SliceBudget<'a> {
    pub fn new(clock: &'a dyn Clock, slice: Duration) -> Self {
        let start = clock.now();
        Self::until(clock, start, start + slice)
    }

    pub fn until(clock: &'a dyn Clock, start: Time, deadline: Time) -> Self {
        Self {
            clock,
            start,
            deadline,
            expired: Cell::new(false),
        }
    }

    pub fn deadline(&self) -> Time {
        self.deadline
    }

    /// True when the job must yield. One clock read until the deadline passes,
    /// then a cached flag: cheap enough to call per object created or per loop
    /// iteration. Sticky, so a job never sees "expired" turn back into "fine".
    #[inline]
    pub fn check(&self) -> bool {
        if self.expired.get() {
            return true;
        }
        if self.clock.now() >= self.deadline {
            self.expired.set(true);
            return true;
        }
        false
    }

    pub fn remaining(&self) -> Duration {
        self.deadline - self.clock.now()
    }

    pub fn spent(&self) -> Duration {
        self.clock.now() - self.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::VirtualClock;

    #[test]
    fn slice_expires_at_deadline_and_stays_expired() {
        let c = VirtualClock::new();
        let s = SliceBudget::new(&c, Duration::from_millis(2));
        assert!(!s.check());
        assert_eq!(s.remaining(), Duration::from_millis(2));
        c.advance_ms(1);
        assert!(!s.check());
        c.advance_ms(1);
        assert!(s.check());
        assert_eq!(s.remaining(), Duration::ZERO);
        assert_eq!(s.spent(), Duration::from_millis(2));
    }

    #[test]
    fn frame_budget_adapts_to_render_cost() {
        let t = Time::from_millis(100);
        let b = FrameBudget::adaptive(t, Duration::from_micros(16_667), Duration::from_millis(6));
        assert_eq!(b.deadline - t, Duration::from_micros(10_667));
        // A render that ate the whole period still leaves the floor.
        let b = FrameBudget::adaptive(t, Duration::from_millis(8), Duration::from_millis(20));
        assert_eq!(b.deadline - t, FrameBudget::MIN_BUDGET);
        assert!(b.expired(b.deadline));
        assert!(!b.expired(t));
    }
}
