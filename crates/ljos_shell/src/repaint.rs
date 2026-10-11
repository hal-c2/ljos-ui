//! When each window is next drawn.
//!
//! egui says when it wants a pass (`request_repaint`, now or after a
//! delay); the shell keeps the earliest time asked for each window and
//! tells the host when the first of them comes. Nothing is drawn before
//! that: a window nobody asked about is not in here, and the loop sleeps.

use std::time::Instant;

/// The pending redraws, by window.
#[derive(Debug)]
pub(crate) struct RepaintSchedule<K> {
    due: Vec<(K, Instant)>,
}

impl<K> Default for RepaintSchedule<K> {
    fn default() -> Self {
        Self { due: Vec::new() }
    }
}

impl<K: Copy + PartialEq> RepaintSchedule<K> {
    /// `key` is drawn at `when`, or earlier if it was asked for earlier.
    pub(crate) fn request(&mut self, key: K, when: Instant) {
        match self.due.iter_mut().find(|(k, _)| *k == key) {
            Some((_, at)) => *at = (*at).min(when),
            None => self.due.push((key, when)),
        }
    }

    /// The windows whose time has come, in the order they were asked for.
    /// They are no longer pending.
    pub(crate) fn take_due(&mut self, now: Instant) -> Vec<K> {
        let mut out = Vec::new();
        self.due.retain(|(k, at)| {
            let due = *at <= now;
            if due {
                out.push(*k);
            }
            !due
        });
        out
    }

    /// When the loop has to wake for the next of them; `None` sleeps.
    pub(crate) fn next(&self) -> Option<Instant> {
        self.due.iter().map(|(_, at)| *at).min()
    }

    /// The window is gone.
    pub(crate) fn forget(&mut self, key: K) {
        self.due.retain(|(k, _)| *k != key);
    }

    /// Nothing is pending: the loop may sleep until something happens.
    #[cfg(test)]
    pub(crate) fn is_idle(&self) -> bool {
        self.due.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nothing_asked_means_the_loop_sleeps() {
        let s = RepaintSchedule::<u64>::default();
        assert!(s.is_idle());
        assert_eq!(s.next(), None);
    }

    #[test]
    fn the_earliest_time_asked_for_a_window_is_kept() {
        let t = Instant::now();
        let mut s = RepaintSchedule::default();
        s.request(1u64, t + ms(100));
        s.request(1, t + ms(20));
        s.request(1, t + ms(500));
        assert_eq!(s.next(), Some(t + ms(20)));
    }

    #[test]
    fn the_loop_wakes_for_the_first_window_that_is_due() {
        let t = Instant::now();
        let mut s = RepaintSchedule::default();
        s.request(1u64, t + ms(100));
        s.request(2, t + ms(16));
        assert_eq!(s.next(), Some(t + ms(16)));
    }

    #[test]
    fn a_window_is_drawn_once_for_what_was_asked() {
        let t = Instant::now();
        let mut s = RepaintSchedule::default();
        s.request(1u64, t);
        s.request(1, t);
        assert_eq!(s.take_due(t), vec![1]);
        assert!(s.is_idle(), "drawn windows are not pending any more");
        assert_eq!(s.take_due(t + ms(1000)), Vec::<u64>::new());
    }

    #[test]
    fn a_window_that_is_not_due_stays_pending() {
        let t = Instant::now();
        let mut s = RepaintSchedule::default();
        s.request(1u64, t);
        s.request(2, t + ms(50));
        assert_eq!(s.take_due(t + ms(10)), vec![1]);
        assert_eq!(s.next(), Some(t + ms(50)));
        assert_eq!(s.take_due(t + ms(50)), vec![2]);
        assert!(s.is_idle());
    }

    #[test]
    fn a_window_that_went_is_not_woken_for() {
        let t = Instant::now();
        let mut s = RepaintSchedule::default();
        s.request(1u64, t + ms(5));
        s.forget(1);
        assert!(s.is_idle());
        assert_eq!(s.next(), None);
    }
}
