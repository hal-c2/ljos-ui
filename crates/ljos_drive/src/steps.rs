//! Settling and letting time pass one step at a time, for a host that
//! must let go of the scene between frames: one whose test code, run from
//! a timer or a `Qt.callLater`, sends input of its own. [`Driver`]
//! (crate::Driver) runs the same steps for everyone else.

use std::time::{Duration, Instant};

use ljos_engine::Engine;
use ljos_platform::Platform;
use ljos_quick::{ImageStore, QuickScene};
use ljos_runtime::Time;

use crate::{DriveError, DriveState, SceneClock};

/// Frames a scene may need to come to rest before something is wrong with
/// it.
pub const MAX_SETTLE_FRAMES: usize = 64;
/// Stops one advance may make (timers firing, frames an animation asks
/// for) before a timer with no interval is blamed for time not passing.
pub const MAX_TIMER_STEPS: usize = 100_000;
/// Rounds of answers that ask for more answers (a feed whose rows ask for
/// their pictures) before the rest is left for later.
pub const MAX_ANSWER_ROUNDS: usize = 8;

/// One settle under way: frames until the scene is at rest.
#[derive(Debug)]
pub struct Settling {
    frames: usize,
    deadline: Option<Instant>,
}

impl Settling {
    pub fn new(clock: &dyn SceneClock) -> Self {
        Self {
            frames: 0,
            deadline: clock.settle_wall().map(|d| Instant::now() + d),
        }
    }

    /// Makes one frame at the present time. `Ok(true)` once the scene is
    /// at rest: it needs no frame now, and nothing is due before its next
    /// wake, which `state` keeps.
    pub fn frame(
        &mut self,
        engine: &Engine,
        scene: &mut QuickScene,
        clock: &dyn SceneClock,
        state: &mut DriveState,
    ) -> Result<bool, DriveError> {
        if self.frames >= MAX_SETTLE_FRAMES || self.deadline.is_some_and(|d| Instant::now() > d) {
            return Err(DriveError::Unsettled {
                frames: self.frames,
            });
        }
        let now = clock.now();
        let out = scene.frame(engine, now);
        self.frames += 1;
        state.frames += 1;
        if out.changed {
            state.shown += 1;
        }
        let rest = !scene.needs_frame()
            && out.next_wake.is_none_or(|t| t > now)
            && !engine.has_due_calls();
        if rest {
            state.wake = out.next_wake;
        }
        Ok(rest)
    }
}

/// One advance under way: the stops between now and its end.
#[derive(Debug)]
pub struct Advance {
    end: Time,
    ms: u64,
    stops: usize,
    done: bool,
}

impl Advance {
    /// Lets `ms` pass from now. Only a test clock lets time pass when
    /// asked.
    pub fn new(clock: &dyn SceneClock, ms: u64) -> Result<Self, DriveError> {
        let now = clock.now();
        // Asked before anything moves, so that a real clock refuses here.
        clock.move_to(now)?;
        Ok(Self {
            end: now + Duration::from_millis(ms),
            ms,
            stops: 0,
            done: false,
        })
    }

    /// Where it ends.
    pub fn end(&self) -> Time {
        self.end
    }

    /// Moves the clock to the next stop and fires the timers due there:
    /// the earliest of the engine's next timer, the scene's `wake` and
    /// `also` (a deadline of the caller's own) that is not after the end,
    /// or else the end. `None` once the end has been reached; the caller
    /// settles after each stop.
    pub fn stop(
        &mut self,
        engine: &Engine,
        clock: &dyn SceneClock,
        wake: Option<Time>,
        also: Option<Time>,
    ) -> Result<Option<Time>, DriveError> {
        if self.done {
            return Ok(None);
        }
        self.stops += 1;
        if self.stops > MAX_TIMER_STEPS {
            return Err(DriveError::Timers { ms: self.ms });
        }
        let next = [engine.next_timer_deadline(), wake, also]
            .into_iter()
            .flatten()
            .min();
        let at = match next {
            Some(t) if t <= self.end => t,
            _ => {
                self.done = true;
                self.end
            }
        };
        clock.move_to(at)?;
        let now = clock.now();
        engine.advance_timers(now);
        Ok(Some(now))
    }
}

/// Waits for what the scene was promised: pictures being decoded and the
/// platform's answers (a fetch, a file read off the Engine thread). They
/// take real time the test clock knows nothing of, so they have arrived
/// by the time any time has passed, as they would by the time a user
/// looks: waited for, not slept for. `serve` is called while waiting,
/// for what this thread serves that they wait on (a fake server);
/// `settle` brings the scene to rest after each round.
pub fn finish_promised<'f, E>(
    engine: &Engine,
    mut serve: Option<&mut (dyn FnMut() + 'f)>,
    mut settle: impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    let images = ImageStore::of(engine);
    let platform = Platform::of(engine);
    for _ in 0..MAX_ANSWER_ROUNDS {
        if images.pending() == 0 && platform.pending() == 0 {
            return Ok(());
        }
        match serve.as_deref_mut() {
            Some(serve) => platform.finish_with(engine, serve),
            None => platform.finish(engine),
        }
        // Left unanswered only when it gave up (and said so): waiting
        // again would only give up again.
        let gave_up = platform.pending() > 0;
        settle()?;
        images.finish(engine);
        settle()?;
        if gave_up || images.pending() > 0 {
            return Ok(());
        }
    }
    Ok(())
}
