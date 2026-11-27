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

/// The same for a shell with nothing to draw into (an activity in the
/// background, its surface gone): its passes still run the engine's timers
/// and deliver the messages and the inspector's calls, not drawn, and no
/// nearer together than [`HEARTBEAT`] after the last one (`last`), as the
/// desktop shell paces a hidden window's.
pub(crate) fn undrawn(next: Next, last: Option<Instant>, now: Instant) -> Next {
    let paced = |at: Instant| last.map_or(at, |last| at.max(last + HEARTBEAT));
    match next {
        Next::Now => match paced(now) {
            at if at <= now => Next::Now,
            at => Next::At(Some(at)),
        },
        Next::At(at) => Next::At(at.map(paced)),
    }
}

/// A shell turning without a surface, as [`undrawn_turn`] drives it.
pub(crate) trait Undrawn {
    /// What is due as of `now`, before it is paced.
    fn due(&mut self, now: Instant) -> Next;
    /// Runs one pass that is not drawn; true when it asked to quit.
    fn pass(&mut self) -> bool;
}

/// How a turn without a surface ends.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Turned {
    /// Back to the host's loop, to be woken then (or by what arrives).
    Sleep(Option<Instant>),
    /// A pass asked the application to quit.
    Quit,
}

/// One turn of a shell with nothing to draw into: at most one undrawn
/// pass, if one is due ([`undrawn`]), `last` the start of the latest. A
/// pass that leaves the next due at once still goes back to the host's
/// loop first, so what arrived meanwhile (the surface coming back, the
/// activity resumed) is heard before it.
pub(crate) fn undrawn_turn(
    shell: &mut impl Undrawn,
    last: &mut Option<Instant>,
    clock: impl Fn() -> Instant,
) -> Turned {
    let now = clock();
    if undrawn(shell.due(now), *last, now) == Next::Now {
        *last = Some(now);
        if shell.pass() {
            return Turned::Quit;
        }
    }
    let now = clock();
    match undrawn(shell.due(now), *last, now) {
        Next::Now => Turned::Sleep(Some(now)),
        Next::At(at) => Turned::Sleep(at),
    }
}

#[cfg(test)]
mod tests {
    use super::{Next, Turned, Undrawn, next_wake, posted_met, undrawn, undrawn_turn};
    use crate::desktop::HEARTBEAT;
    use std::cell::Cell;
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

    #[test]
    fn an_undrawn_pass_runs_at_once_after_a_quiet_spell() {
        let now = Instant::now();
        assert_eq!(undrawn(Next::Now, None, now), Next::Now);
        assert_eq!(
            undrawn(Next::Now, Some(now - 2 * HEARTBEAT), now),
            Next::Now
        );
    }

    #[test]
    fn undrawn_passes_are_a_heartbeat_apart() {
        let now = Instant::now();
        let last = now - HEARTBEAT / 2;
        assert_eq!(
            undrawn(Next::Now, Some(last), now),
            Next::At(Some(last + HEARTBEAT))
        );
        let soon = now + Duration::from_millis(1);
        assert_eq!(
            undrawn(Next::At(Some(soon)), Some(last), now),
            Next::At(Some(last + HEARTBEAT))
        );
    }

    #[test]
    fn an_application_with_nothing_due_sleeps_without_a_surface() {
        let now = Instant::now();
        assert_eq!(undrawn(Next::At(None), Some(now), now), Next::At(None));
        let later = now + 10 * HEARTBEAT;
        assert_eq!(
            undrawn(Next::At(Some(later)), Some(now), now),
            Next::At(Some(later))
        );
    }

    /// Always has something due, and each pass takes `took` of a clock
    /// that only passes move.
    struct Busy<'a> {
        clock: &'a Cell<Instant>,
        took: Duration,
        passes: usize,
    }

    impl Undrawn for Busy<'_> {
        fn due(&mut self, _: Instant) -> Next {
            Next::Now
        }

        fn pass(&mut self) -> bool {
            self.passes += 1;
            // A turn that never ends is caught here rather than hanging.
            assert!(self.passes < 100, "the turn never went back to the loop");
            self.clock.set(self.clock.get() + self.took);
            false
        }
    }

    #[test]
    fn a_slow_undrawn_pass_goes_back_to_the_loop_before_the_next() {
        let clock = Cell::new(Instant::now());
        let mut busy = Busy {
            clock: &clock,
            took: 2 * HEARTBEAT,
            passes: 0,
        };
        let mut last = None;
        let turned = undrawn_turn(&mut busy, &mut last, || clock.get());
        // What arrived during the pass (the surface coming back, say) is
        // heard before the next pass, which is due at once.
        assert_eq!(busy.passes, 1);
        assert_eq!(turned, Turned::Sleep(Some(clock.get())));
    }

    #[test]
    fn a_quick_undrawn_pass_sleeps_until_a_heartbeat_after_it() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let mut busy = Busy {
            clock: &clock,
            took: HEARTBEAT / 4,
            passes: 0,
        };
        let mut last = None;
        let turned = undrawn_turn(&mut busy, &mut last, || clock.get());
        assert_eq!(busy.passes, 1);
        assert_eq!(turned, Turned::Sleep(Some(start + HEARTBEAT)));
    }
}
