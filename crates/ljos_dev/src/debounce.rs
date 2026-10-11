//! The quiet time between a save and the reload it causes.
//!
//! An editor writes a file in several steps (truncate, write, rename, a
//! backup beside it), and a "save all" touches several files. Compiling at
//! the first event would compile half a save, so the reload waits until
//! nothing has changed for [`QUIET`].

use std::{ops::Add, time::Duration};

/// ARCHITECTURE.md §12.1: 120 ms without a file event.
pub const QUIET: Duration = Duration::from_millis(120);

/// One deadline that every event pushes back. `I` is an instant of
/// whatever clock the owner has: `std::time::Instant` on the watcher's
/// thread, `ljos_runtime::Time` where time is virtual.
#[derive(Debug, Clone, Copy)]
pub struct Debounce<I> {
    quiet: Duration,
    due: Option<I>,
}

impl<I> Default for Debounce<I> {
    fn default() -> Self {
        Self {
            quiet: QUIET,
            due: None,
        }
    }
}

impl<I: Copy + Ord + Add<Duration, Output = I>> Debounce<I> {
    /// A debounce with the standard quiet time.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_quiet(quiet: Duration) -> Self {
        Self { quiet, due: None }
    }

    /// Something changed at `now`: the quiet time starts again.
    pub fn touch(&mut self, now: I) {
        self.due = Some(now + self.quiet);
    }

    /// When the owner must look again; None while nothing is waiting, and
    /// then the owner has nothing to wake for.
    #[must_use]
    pub fn deadline(&self) -> Option<I> {
        self.due
    }

    #[must_use]
    pub fn pending(&self) -> bool {
        self.due.is_some()
    }

    /// True once, when the quiet time has passed.
    pub fn fire(&mut self, now: I) -> bool {
        match self.due {
            Some(due) if due <= now => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    /// Fires now, whatever the time: true when something was waiting.
    pub fn flush(&mut self) -> bool {
        self.due.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ljos_runtime::Time;

    fn at(ms: u64) -> Time {
        Time::ZERO + Duration::from_millis(ms)
    }

    #[test]
    fn nothing_waits_until_something_changes() {
        let mut d: Debounce<Time> = Debounce::new();
        assert_eq!(d.deadline(), None);
        assert!(!d.fire(at(1_000)));
        assert!(!d.flush());
    }

    #[test]
    fn it_fires_once_when_the_quiet_time_has_passed() {
        let mut d = Debounce::new();
        d.touch(at(0));
        assert_eq!(d.deadline(), Some(at(120)));
        assert!(!d.fire(at(119)));
        assert!(d.fire(at(120)));
        assert!(!d.fire(at(500)));
        assert!(!d.pending());
    }

    #[test]
    fn every_change_starts_the_quiet_time_again() {
        let mut d = Debounce::new();
        d.touch(at(0));
        d.touch(at(100));
        assert!(!d.fire(at(120)));
        assert!(d.fire(at(220)));
    }

    #[test]
    fn it_works_on_the_systems_clock() {
        let mut d = Debounce::with_quiet(Duration::from_millis(5));
        let now = std::time::Instant::now();
        d.touch(now);
        assert!(!d.fire(now));
        assert!(d.fire(now + Duration::from_millis(5)));
    }

    #[test]
    fn a_flush_does_not_wait() {
        let mut d = Debounce::new();
        d.touch(at(0));
        assert!(d.flush());
        assert!(!d.pending());
    }
}
