//! Where a frame's time went inside the engine, for a frame timeline
//! (ARCHITECTURE.md §19.4, `app_frames`): the time spent in bindings and
//! how many ran, and whether a timer fired.
//!
//! Bindings run wherever something reads what they give: in a timer's
//! handler, an animation's step, a layout, the scene's sync. So they are
//! timed where they run and not by pass, and a binding that reads another
//! is timed once, as the outermost run. While nothing times them a run
//! pays one look at a flag and reads no clock.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::engine::Engine;

#[derive(Default)]
pub(crate) struct Timing {
    on: Cell<bool>,
    /// Bindings running now, the outermost first.
    depth: Cell<u32>,
    spent: Cell<Duration>,
    runs: Cell<u64>,
    /// A timer fired, or a call queued with a delay ran.
    fired: Cell<bool>,
}

/// What the engine did since it was last asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineTimes {
    /// Inside bindings, counted once where one reads another.
    pub bindings: Duration,
    /// Bindings that ran, the outermost only.
    pub binding_runs: u64,
    pub timer_fired: bool,
}

/// One outermost run of a binding being timed.
pub(crate) struct BindingRun<'a> {
    timing: &'a Timing,
    started: Option<Instant>,
}

impl Drop for BindingRun<'_> {
    fn drop(&mut self) {
        let t = self.timing;
        t.depth.set(t.depth.get() - 1);
        if let Some(started) = self.started {
            t.spent.set(t.spent.get() + started.elapsed());
            t.runs.set(t.runs.get() + 1);
        }
    }
}

impl Timing {
    /// The start of a binding's run, timed if it is the outermost one
    /// while bindings are timed.
    pub(crate) fn binding(&self) -> Option<BindingRun<'_>> {
        if !self.on.get() {
            return None;
        }
        let depth = self.depth.get();
        self.depth.set(depth + 1);
        Some(BindingRun {
            timing: self,
            started: (depth == 0).then(Instant::now),
        })
    }

    pub(crate) fn timer_fired(&self) {
        self.fired.set(true);
    }
}

impl Engine {
    /// Times bindings and notes timers firing until turned off, for
    /// [`Engine::take_times`]. Off by default; turning it either way
    /// forgets what was counted.
    pub fn set_timed(&self, on: bool) {
        let t = &self.inner.timing;
        t.on.set(on);
        self.take_times();
    }

    pub fn is_timed(&self) -> bool {
        self.inner.timing.on.get()
    }

    /// What was counted since the last call, which starts counting again.
    pub fn take_times(&self) -> EngineTimes {
        let t = &self.inner.timing;
        EngineTimes {
            bindings: t.spent.take(),
            binding_runs: t.runs.take(),
            timer_fired: t.fired.take(),
        }
    }
}
