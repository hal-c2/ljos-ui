//! The Android host's pump, which carries the activity's events to the
//! application: when it ends, and what it holds for the activity while it
//! runs. Only the Android host uses it; it builds
//! everywhere so that its tests run on every host.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Whether the pump goes on. Once it has asked the activity to finish, it
/// goes on after the application's loop has ended, until the activity is
/// destroyed: what follows `android_main` must not race its teardown.
pub(crate) fn keeps_pumping(destroyed: bool, closed: bool, finishing: bool) -> bool {
    !destroyed && (!closed || finishing)
}

/// The pump's `closed` and `finishing` once the application asked to exit.
/// The application is ending, so nothing more goes to it. A finish the
/// activity did not take brings no Destroy to wait for: the pump ends now,
/// not at the next event, and `main` ends the process.
pub(crate) fn exiting(finish_taken: bool) -> (bool, bool) {
    (true, finish_taken)
}

/// What wakes the activity's thread when the application asks something of
/// it.
pub(crate) trait Wakes {
    fn wake(&self);
}

/// What the application asked of the activity, kept until the activity's
/// thread gets to it. The activity changes; the queue stays.
pub(crate) struct Requests<R, W> {
    queue: Vec<R>,
    waker: Option<W>,
    /// An activity's `android_main` has arrived and has not yet returned.
    attached: bool,
}

impl<R, W> Default for Requests<R, W> {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            waker: None,
            attached: false,
        }
    }
}

impl<R, W: Wakes> Requests<R, W> {
    /// Keeps `request` for the activity's thread, waking it if one pumps.
    pub(crate) fn ask(&mut self, request: R) {
        self.queue.push(request);
        if let Some(waker) = &self.waker {
            waker.wake();
        }
    }

    /// What was asked since the last time, oldest first.
    pub(crate) fn take(&mut self) -> Vec<R> {
        std::mem::take(&mut self.queue)
    }

    /// An activity's `android_main` started and will pump.
    pub(crate) fn arriving(&mut self) {
        self.attached = true;
    }

    pub(crate) fn attached(&self) -> bool {
        self.attached
    }
}

/// What the rest of a turn of the pump hands the activity once its events
/// are in: its input is read, then what the application asked of it is
/// done, oldest first. `None` once one of those events destroyed it: what
/// GameActivity holds (its input buffers, its editor and the text that
/// holds) went with it, and it is touched no more. What was asked of it is
/// dropped: a new activity comes with a new surface, and the application
/// tells it all anew.
pub(crate) fn for_activity<R, W: Wakes>(
    destroyed: bool,
    requests: &Mutex<Requests<R, W>>,
) -> Option<Vec<R>> {
    let asked = lock(requests).take();
    Some(asked)
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An activity's pump, attached for as long as this lives. However the
/// pump ends, by returning or by unwinding, the application's requests
/// stop waking it and a quit no longer leaves the process's end to it.
pub(crate) struct Pumping<'a, R, W: Wakes> {
    requests: &'a Mutex<Requests<R, W>>,
}

impl<'a, R, W: Wakes> Pumping<'a, R, W> {
    pub(crate) fn new(requests: &'a Mutex<Requests<R, W>>, waker: W) -> Self {
        let mut r = lock(requests);
        r.attached = true;
        r.waker = Some(waker);
        drop(r);
        Self { requests }
    }
}

impl<R, W: Wakes> Drop for Pumping<'_, R, W> {
    fn drop(&mut self) {
        let mut r = lock(self.requests);
        r.waker = None;
        r.attached = false;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{Pumping, Requests, Wakes, exiting, for_activity, keeps_pumping, lock};

    #[derive(Default)]
    struct Count(std::sync::Arc<AtomicU32>);

    impl Wakes for Count {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn waker() -> (Count, std::sync::Arc<AtomicU32>) {
        let count = Count::default();
        let woken = std::sync::Arc::clone(&count.0);
        (count, woken)
    }

    #[test]
    fn a_pump_that_returns_leaves_no_activity_attached() {
        let requests = Mutex::new(Requests::<u8, Count>::default());
        let (count, woken) = waker();
        {
            let _pumping = Pumping::new(&requests, count);
            assert!(lock(&requests).attached());
            lock(&requests).ask(1);
            assert_eq!(woken.load(Ordering::Relaxed), 1);
        }
        let mut r = lock(&requests);
        assert!(!r.attached());
        // Asked with no activity: kept for the next one, and wakes nothing.
        r.ask(2);
        assert_eq!(woken.load(Ordering::Relaxed), 1);
        assert_eq!(r.take(), [1, 2]);
    }

    #[test]
    fn a_pump_that_unwinds_leaves_no_activity_attached() {
        let requests = Mutex::new(Requests::<u8, Count>::default());
        let (count, woken) = waker();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _pumping = Pumping::new(&requests, count);
            panic!("the activity's thread");
        }));
        assert!(unwound.is_err());
        let mut r = lock(&requests);
        assert!(!r.attached());
        r.ask(1);
        assert_eq!(woken.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn an_activity_arriving_is_attached_before_it_pumps() {
        let requests = Mutex::new(Requests::<u8, Count>::default());
        lock(&requests).arriving();
        assert!(lock(&requests).attached());
        let (count, _) = waker();
        drop(Pumping::new(&requests, count));
        assert!(!lock(&requests).attached());
    }

    #[test]
    fn an_exit_the_activity_did_not_finish_ends_the_pump() {
        let (closed, finishing) = exiting(false);
        assert!(!keeps_pumping(false, closed, finishing));
        let (closed, finishing) = exiting(true);
        assert!(keeps_pumping(false, closed, finishing));
        assert!(!keeps_pumping(true, closed, finishing));
    }

    #[test]
    fn a_pump_that_asked_for_finish_waits_for_destroy_after_the_application_ends() {
        assert!(keeps_pumping(false, true, true));
        assert!(!keeps_pumping(true, true, true));
    }

    #[test]
    fn an_activity_is_handed_what_was_asked_oldest_first() {
        let requests = Mutex::new(Requests::<u8, Count>::default());
        lock(&requests).ask(1);
        lock(&requests).ask(2);
        assert_eq!(for_activity(false, &requests), Some(vec![1, 2]));
        assert_eq!(for_activity(false, &requests), Some(vec![]));
    }

    #[test]
    fn a_destroyed_activity_is_not_touched_and_what_was_asked_of_it_is_dropped() {
        let requests = Mutex::new(Requests::<u8, Count>::default());
        lock(&requests).ask(1);
        assert_eq!(for_activity(true, &requests), None);
        // Not kept for the next activity, which is told anew.
        assert!(lock(&requests).take().is_empty());
        // What is asked once it is gone is kept for the next one.
        lock(&requests).ask(2);
        assert_eq!(lock(&requests).take(), [2]);
    }

    #[test]
    fn a_pump_ends_when_the_application_ends_without_a_finish() {
        assert!(!keeps_pumping(false, true, false));
        assert!(keeps_pumping(false, false, false));
    }
}
