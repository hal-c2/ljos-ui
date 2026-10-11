//! When the loop wakes by itself: never, or at one time. There is no
//! third answer (the loop does not poll).

use std::time::Instant;

/// How the loop waits after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sleep {
    /// Until the platform or a waker has something.
    Forever,
    /// Until then at the latest.
    Until(Instant),
}

/// The one deadline of a loop.
#[derive(Clone, Copy, Debug, Default)]
pub struct Schedule {
    deadline: Option<Instant>,
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the deadline.
    pub fn set(&mut self, deadline: Option<Instant>) {
        self.deadline = deadline;
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Whether the deadline has come. It is taken: the application is told
    /// once and sets the next one itself.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(at) if at <= now => {
                self.deadline = None;
                true
            }
            _ => false,
        }
    }

    /// How to wait now. A deadline that has passed is still a deadline:
    /// the loop comes straight back and [`Schedule::take_due`] reports it.
    pub fn sleep(&self) -> Sleep {
        match self.deadline {
            Some(at) => Sleep::Until(at),
            None => Sleep::Forever,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn without_a_deadline_the_loop_sleeps_until_something_happens() {
        let mut s = Schedule::new();
        assert_eq!(s.sleep(), Sleep::Forever);
        assert!(!s.take_due(Instant::now()));
    }

    #[test]
    fn a_deadline_is_reported_once_when_it_has_come() {
        let now = Instant::now();
        let mut s = Schedule::new();
        s.set(Some(now + Duration::from_millis(10)));
        assert_eq!(s.sleep(), Sleep::Until(now + Duration::from_millis(10)));
        assert!(!s.take_due(now));
        assert!(s.take_due(now + Duration::from_millis(10)));
        // Taken: nothing wakes the loop again until the application asks.
        assert_eq!(s.sleep(), Sleep::Forever);
        assert!(!s.take_due(now + Duration::from_secs(1)));
    }

    #[test]
    fn a_later_call_replaces_the_deadline() {
        let now = Instant::now();
        let mut s = Schedule::new();
        s.set(Some(now + Duration::from_millis(10)));
        s.set(Some(now + Duration::from_millis(50)));
        assert!(!s.take_due(now + Duration::from_millis(20)));
        s.set(None);
        assert_eq!(s.sleep(), Sleep::Forever);
    }
}
