//! When a shell whose loop the host runs wakes by itself next. Only the
//! Android shell uses it; it builds everywhere so that its tests run on
//! every host.

use std::sync::Mutex;
use std::time::Instant;

use crate::desktop::HEARTBEAT;

/// What the loop does before it waits.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Next {
    /// A pass now: what is due is too near to sleep for.
    Now,
    /// Sleep until then, or until something arrives.
    At(Option<Instant>),
}

/// The earlier of the scene's next frame (`asked`) and the turn another
/// thread asked for (`posted`). What was posted stays until a pass meets
/// it ([`posted_met`]): the host keeps only the deadline it was told last,
/// and a turn that asks for nothing new would otherwise tell it none.
pub(crate) fn next_wake(
    asked: Option<Instant>,
    posted: &Mutex<Option<Instant>>,
    now: Instant,
) -> Next {
    let posted = *crate::lock(posted);
    let due = match (asked, posted) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    match due {
        Some(at) if at <= now + HEARTBEAT / 4 => Next::Now,
        other => Next::At(other),
    }
}

/// A pass that `began` then meets what was posted for then or before, and
/// for as near after as [`next_wake`] does not sleep for.
pub(crate) fn posted_met(posted: &Mutex<Option<Instant>>, began: Instant) {
    let mut posted = crate::lock(posted);
    if posted.is_some_and(|at| at <= began + HEARTBEAT / 4) {
        *posted = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{Next, next_wake, posted_met};
    use crate::desktop::HEARTBEAT;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    fn a_posted_wake_outlives_a_turn_that_asks_for_nothing() {
        let now = Instant::now();
        let at = now + HEARTBEAT;
        let posted = Mutex::new(Some(at));
        assert_eq!(next_wake(None, &posted, now), Next::At(Some(at)));
        // Memory pressure, say: a turn with no pass and nothing posted.
        assert_eq!(next_wake(None, &posted, now), Next::At(Some(at)));
        // A pass before it leaves it.
        posted_met(&posted, now);
        assert_eq!(next_wake(None, &posted, now), Next::At(Some(at)));
    }

    #[test]
    fn a_pass_meets_the_posted_wake_once_it_is_due() {
        let now = Instant::now();
        let posted = Mutex::new(Some(now + Duration::from_millis(1)));
        assert_eq!(next_wake(None, &posted, now), Next::Now);
        posted_met(&posted, now);
        assert_eq!(next_wake(None, &posted, now), Next::At(None));
    }

    #[test]
    fn the_scenes_frame_and_a_posted_wake_take_the_earlier() {
        let now = Instant::now();
        let frame = now + HEARTBEAT;
        let posted = Mutex::new(Some(now + 2 * HEARTBEAT));
        assert_eq!(next_wake(Some(frame), &posted, now), Next::At(Some(frame)));
        assert_eq!(
            next_wake(None, &posted, now),
            Next::At(Some(now + 2 * HEARTBEAT))
        );
    }
}
