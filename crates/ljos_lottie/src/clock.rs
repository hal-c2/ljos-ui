//! Where playback is, as arithmetic on ticks.
//!
//! Qt steps a Lottie animation one frame per tick of `1000 / frameRate`
//! milliseconds (rounded down) and treats the last frame as part of the
//! run: twenty frames at ten a second show frame 20 at 2000 ms and finish
//! at 2100 ms, on the tick that would step past it. A loop starts again on
//! that same tick. The clock here reproduces that from a count of ticks,
//! so the item can jump any number of them at once (after being hidden,
//! or when a deadline fires late) instead of being woken for each.

/// The playback position of one animation.
#[derive(Clone, Debug, PartialEq)]
pub struct Clock {
    /// The first frame of the run (`startFrame`).
    pub start: f64,
    /// The last frame of the run (`endFrame`), shown before a loop ends.
    pub end: f64,
    /// `1` forward, `-1` in reverse.
    pub direction: i32,
    /// How many runs make the whole animation; `-1` for no end.
    pub loops: i32,
    /// The frame on screen.
    pub frame: f64,
    /// Runs completed since the last start.
    pub loops_done: i32,
    /// Whether ticks move the frame.
    pub playing: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Clock {
            start: 0.0,
            end: 0.0,
            direction: 1,
            loops: 1,
            frame: 0.0,
            loops_done: 0,
            playing: false,
        }
    }
}

/// Infinite loops, as `LottieAnimation.Infinite` says.
pub const INFINITE: i32 = -1;

impl Clock {
    fn forward(&self) -> bool {
        self.direction >= 0
    }

    /// The frame a run starts on in the current direction.
    pub fn first(&self) -> f64 {
        if self.forward() { self.start } else { self.end }
    }

    /// The frame a run ends on in the current direction.
    pub fn last(&self) -> f64 {
        if self.forward() { self.end } else { self.start }
    }

    /// Ticks in one whole run: every frame from first to last, then the
    /// tick that wraps.
    fn run_ticks(&self) -> u64 {
        ((self.end - self.start).max(0.0).floor() as u64).saturating_add(1)
    }

    /// Ticks from the frame on screen to the one that wraps the run.
    fn ticks_to_wrap(&self) -> u64 {
        let left = if self.forward() {
            self.end - self.frame
        } else {
            self.frame - self.start
        };
        (left.max(0.0).floor() as u64).saturating_add(1)
    }

    /// Whether the loops still to run are endless.
    fn endless(&self) -> bool {
        self.loops == INFINITE
    }

    /// Runs still to wrap before the animation finishes, counting the one
    /// under way; at least one.
    fn runs_left(&self) -> u64 {
        u64::try_from(self.loops.max(1) - self.loops_done)
            .unwrap_or(0)
            .max(1)
    }

    /// Ticks from now to the one that finishes the animation, or `None`
    /// when it plays for ever or is not playing.
    pub fn ticks_to_finish(&self) -> Option<u64> {
        if !self.playing || self.endless() {
            return None;
        }
        let rest = self.runs_left() - 1;
        Some(
            self.ticks_to_wrap()
                .saturating_add(rest.saturating_mul(self.run_ticks())),
        )
    }

    /// Moves `ticks` ticks on. Returns whether one of them finished the
    /// animation, which leaves it on its last frame and not playing.
    pub fn advance(&mut self, mut ticks: u64) -> bool {
        if !self.playing {
            return false;
        }
        let step = if self.forward() { 1.0 } else { -1.0 };
        let to_wrap = self.ticks_to_wrap();
        if ticks < to_wrap {
            self.frame += step * ticks as f64;
            return false;
        }
        ticks -= to_wrap;
        self.loops_done = self.loops_done.saturating_add(1);
        if !self.endless() && self.loops_done >= self.loops.max(1) {
            self.frame = self.last();
            self.playing = false;
            return true;
        }
        // Whole runs in between change nothing but the count.
        let run = self.run_ticks();
        let mut whole = ticks / run;
        if !self.endless() {
            whole = whole.min(self.runs_left() - 1);
        }
        ticks -= whole * run;
        let whole = i32::try_from(whole).unwrap_or(i32::MAX);
        self.loops_done = if self.endless() {
            // Only the fact that it loops matters; keep the count small.
            0
        } else {
            self.loops_done.saturating_add(whole)
        };
        self.frame = self.first();
        self.advance(ticks)
    }

    /// Back to the first frame of a run and playing, as `start()` does.
    pub fn restart(&mut self) {
        self.frame = self.first();
        self.loops_done = 0;
        self.playing = true;
    }

    /// `frame` kept within the run.
    pub fn clamp(&self, frame: f64) -> f64 {
        frame.clamp(self.start, self.end.max(self.start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twenty() -> Clock {
        Clock {
            end: 20.0,
            playing: true,
            ..Clock::default()
        }
    }

    #[test]
    fn the_last_frame_is_shown_before_the_run_finishes() {
        let mut c = twenty();
        assert!(!c.advance(20));
        assert_eq!(c.frame, 20.0);
        assert!(c.advance(1));
        assert_eq!(c.frame, 20.0);
        assert!(!c.playing);
    }

    #[test]
    fn a_run_finishes_on_its_twenty_first_tick() {
        let c = twenty();
        assert_eq!(c.ticks_to_finish(), Some(21));
        let mut c = twenty();
        assert!(c.advance(500));
        assert_eq!(c.frame, 20.0);
    }

    #[test]
    fn two_loops_take_twice_as_many_ticks() {
        let mut c = Clock {
            loops: 2,
            ..twenty()
        };
        assert_eq!(c.ticks_to_finish(), Some(42));
        assert!(!c.advance(21));
        assert_eq!(c.frame, 0.0);
        assert_eq!(c.loops_done, 1);
        assert!(!c.advance(20));
        assert!(c.advance(1));
    }

    #[test]
    fn endless_loops_never_finish_however_far_they_go() {
        let mut c = Clock {
            loops: INFINITE,
            ..twenty()
        };
        assert_eq!(c.ticks_to_finish(), None);
        assert!(!c.advance(21 * 1_000_000 + 5));
        assert_eq!(c.frame, 5.0);
        assert!(c.playing);
    }

    #[test]
    fn reverse_runs_from_the_last_frame_to_the_first() {
        let mut c = Clock {
            direction: -1,
            ..twenty()
        };
        c.restart();
        assert_eq!(c.frame, 20.0);
        assert!(!c.advance(15));
        assert_eq!(c.frame, 5.0);
        assert!(c.advance(6));
        assert_eq!(c.frame, 0.0);
    }

    #[test]
    fn many_loops_skip_whole_runs_at_once() {
        let mut c = Clock {
            loops: 1_000_000,
            ..twenty()
        };
        assert!(!c.advance(21 * 999_999 + 3));
        assert_eq!(c.loops_done, 999_999);
        assert_eq!(c.frame, 3.0);
        assert_eq!(c.ticks_to_finish(), Some(18));
    }

    #[test]
    fn playing_again_after_the_end_finishes_on_the_next_tick() {
        let mut c = twenty();
        assert!(c.advance(21));
        c.playing = true;
        assert_eq!(c.ticks_to_finish(), Some(1));
        assert!(c.advance(1));
    }

    #[test]
    fn a_paused_clock_does_not_move() {
        let mut c = Clock {
            playing: false,
            ..twenty()
        };
        assert!(!c.advance(100));
        assert_eq!(c.frame, 0.0);
    }
}
