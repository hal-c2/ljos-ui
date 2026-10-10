//! Monotonic time in whole microseconds, and the clocks that produce it.
//!
//! The runtime never calls `Instant::now()` directly. Everything asks a
//! [`Clock`], so tests and BDD `advance` steps drive time by hand
//! ([`VirtualClock`]) and the scheduler stays deterministic.
//!
//! Microseconds are enough: the finest thing scheduled is a 1 ms timer, and
//! JS interrupt slices are ~266 us. A `u64` of microseconds lasts half a
//! million years, so arithmetic saturates instead of checking overflow.

use std::ops::{Add, AddAssign, Sub};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A point on a [`Clock`]'s monotonic timeline, in microseconds since the
/// clock's epoch. Times from different clocks must not be mixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(u64);

impl Time {
    pub const ZERO: Time = Time(0);
    pub const MAX: Time = Time(u64::MAX);

    pub const fn from_micros(us: u64) -> Time {
        Time(us)
    }

    pub const fn from_millis(ms: u64) -> Time {
        Time(ms.saturating_mul(1000))
    }

    pub const fn as_micros(self) -> u64 {
        self.0
    }

    pub fn saturating_add(self, d: Duration) -> Time {
        Time(self.0.saturating_add(duration_micros(d)))
    }

    pub fn saturating_sub(self, d: Duration) -> Time {
        Time(self.0.saturating_sub(duration_micros(d)))
    }

    /// Time elapsed since `earlier`; zero if `earlier` is in the future.
    pub fn saturating_since(self, earlier: Time) -> Duration {
        Duration::from_micros(self.0.saturating_sub(earlier.0))
    }

    /// Rounds up to a multiple of `granularity` (no-op for a zero granularity).
    pub fn round_up_to(self, granularity: Duration) -> Time {
        let g = duration_micros(granularity);
        if g == 0 {
            return self;
        }
        Time(self.0.div_ceil(g).saturating_mul(g))
    }
}

impl Add<Duration> for Time {
    type Output = Time;
    fn add(self, rhs: Duration) -> Time {
        self.saturating_add(rhs)
    }
}

impl AddAssign<Duration> for Time {
    fn add_assign(&mut self, rhs: Duration) {
        *self = self.saturating_add(rhs);
    }
}

impl Sub<Time> for Time {
    type Output = Duration;
    fn sub(self, rhs: Time) -> Duration {
        self.saturating_since(rhs)
    }
}

pub(crate) fn duration_micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// Source of monotonic time. `Send + Sync` because inbox senders and tests
/// read it from other threads.
pub trait Clock: Send + Sync {
    fn now(&self) -> Time;
}

impl<T: Clock + ?Sized> Clock for Arc<T> {
    fn now(&self) -> Time {
        (**self).now()
    }
}

/// The real monotonic clock. Its epoch is its construction instant.
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    epoch: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Time {
        Time(duration_micros(self.epoch.elapsed()))
    }
}

/// A clock that only moves when told to. Clones share one timeline, so a test
/// can hand a clone to the code under test and keep one to `advance`.
#[derive(Clone, Debug, Default)]
pub struct VirtualClock {
    now: Arc<AtomicU64>,
}

impl VirtualClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn starting_at(t: Time) -> Self {
        Self {
            now: Arc::new(AtomicU64::new(t.0)),
        }
    }

    pub fn advance(&self, d: Duration) {
        let us = duration_micros(d);
        // A saturating add keeps the clock monotonic even on absurd input.
        let _ = self
            .now
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                Some(v.saturating_add(us))
            });
    }

    pub fn advance_ms(&self, ms: u64) {
        self.advance(Duration::from_millis(ms));
    }

    /// Jumps forward to `t`. Time never runs backwards: an earlier `t` is ignored.
    pub fn set(&self, t: Time) {
        self.now.fetch_max(t.0, Ordering::AcqRel);
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Time {
        Time(self.now.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_arithmetic_saturates() {
        let t = Time::from_millis(5);
        assert_eq!(t.as_micros(), 5000);
        assert_eq!(t + Duration::from_millis(1), Time::from_micros(6000));
        assert_eq!(Time::from_millis(1) - Time::from_millis(5), Duration::ZERO);
        assert_eq!(Time::MAX + Duration::from_secs(1), Time::MAX);
        assert_eq!(t.saturating_sub(Duration::from_secs(1)), Time::ZERO);
    }

    #[test]
    fn rounds_up_to_granularity() {
        let ms = Duration::from_millis(1);
        assert_eq!(
            Time::from_micros(1).round_up_to(ms),
            Time::from_micros(1000)
        );
        assert_eq!(
            Time::from_micros(1000).round_up_to(ms),
            Time::from_micros(1000)
        );
        assert_eq!(
            Time::from_micros(1001).round_up_to(ms),
            Time::from_micros(2000)
        );
        assert_eq!(
            Time::from_micros(7).round_up_to(Duration::ZERO),
            Time::from_micros(7)
        );
    }

    #[test]
    fn virtual_clock_is_shared_and_monotonic() {
        let a = VirtualClock::new();
        let b = a.clone();
        assert_eq!(b.now(), Time::ZERO);
        a.advance_ms(3);
        assert_eq!(b.now(), Time::from_millis(3));
        b.set(Time::from_millis(1));
        assert_eq!(a.now(), Time::from_millis(3));
        b.set(Time::from_millis(10));
        assert_eq!(a.now(), Time::from_millis(10));
    }

    #[test]
    fn system_clock_moves_forward() {
        let c = SystemClock::new();
        let t0 = c.now();
        std::thread::sleep(Duration::from_millis(2));
        assert!(c.now() - t0 >= Duration::from_millis(1));
    }
}
