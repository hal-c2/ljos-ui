//! A scene driven on a virtual clock as a host woken at each deadline
//! drives it: what `render` takes a picture after and `testing` runs a
//! test on.

use std::time::Duration;

use ljos_engine::Engine;
use ljos_platform::Platform;
use ljos_quick::{ImageStore, QuickScene};
use ljos_runtime::{Clock as _, Time, VirtualClock};

/// Frames a scene may need to come to rest before something is wrong
/// with it, and timer firings before the time to let pass is given up on.
const MAX_SETTLE_FRAMES: usize = 64;
const MAX_TIMER_STEPS: usize = 100_000;
/// Rounds of answers that ask for more answers (a feed whose rows ask for
/// their pictures) before the rest is left for later.
const MAX_ANSWER_ROUNDS: usize = 8;

/// The scene and the clock it runs on, being driven as a host drives them.
pub(crate) struct Run<'a> {
    pub(crate) engine: &'a Engine,
    pub(crate) scene: &'a mut QuickScene,
    pub(crate) clock: &'a VirtualClock,
    /// When the scene last said it next needs a frame.
    pub(crate) wake: &'a mut Option<Time>,
}

impl Run<'_> {
    /// Frames until the scene is idle: layout, anchors, deferred deletes.
    pub(crate) fn settle(&mut self) -> Result<(), String> {
        for _ in 0..MAX_SETTLE_FRAMES {
            let now = self.clock.now();
            let out = self.scene.frame(self.engine, now);
            if !self.scene.needs_frame() && out.next_wake.is_none_or(|t| t > now) {
                *self.wake = out.next_wake;
                return Ok(());
            }
        }
        Err(format!(
            "the scene did not settle in {MAX_SETTLE_FRAMES} frames"
        ))
    }

    /// Lets `ms` pass, firing each timer at its own deadline and settling in
    /// between, as a host woken at each deadline would.
    pub(crate) fn advance(&mut self, ms: u64) -> Result<(), String> {
        self.finish()?;
        let end = self.clock.now() + Duration::from_millis(ms);
        for _ in 0..MAX_TIMER_STEPS {
            let next = [self.engine.next_timer_deadline(), *self.wake]
                .into_iter()
                .flatten()
                .min();
            match next {
                Some(t) if t <= end => {
                    if t > self.clock.now() {
                        self.clock.set(t);
                    }
                    self.engine.advance_timers(self.clock.now());
                    self.settle()?;
                }
                _ => {
                    self.clock.set(end);
                    self.engine.advance_timers(end);
                    self.settle()?;
                    return self.finish();
                }
            }
        }
        Err(format!(
            "timers fired more than {MAX_TIMER_STEPS} times in {ms} ms"
        ))
    }

    /// Waits for what the scene was promised: pictures being decoded and
    /// the platform's answers (a fetch, a file read off the Engine thread).
    /// They take real time the virtual clock knows nothing of, so they
    /// have arrived by the time any virtual time has passed, as they would
    /// by the time a user looks: waited for, not slept for.
    fn finish(&mut self) -> Result<(), String> {
        let images = ImageStore::of(self.engine);
        let platform = Platform::of(self.engine);
        for _ in 0..MAX_ANSWER_ROUNDS {
            if images.pending() == 0 && platform.pending() == 0 {
                return Ok(());
            }
            platform.finish(self.engine);
            // Left unanswered only when it gave up (and said so): waiting
            // again would only give up again.
            let gave_up = platform.pending() > 0;
            self.settle()?;
            images.finish(self.engine);
            if gave_up || images.pending() > 0 {
                return self.settle();
            }
            self.settle()?;
        }
        Ok(())
    }
}
