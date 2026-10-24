//! Where the time of the scene's frames went, while a frame timeline
//! records (ARCHITECTURE.md §19.4, `app_frames`). Untimed, a frame reads
//! no clock for it.

use std::time::{Duration, Instant};

use ljos_engine::Engine;

use crate::scene::QuickScene;

/// What woke a frame. More than one can, and none of these when the
/// frame was asked for otherwise (a write, a picture decoded, a resize).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Woke {
    /// A pointer, a key, the wheel or the input method reached the scene.
    pub input: bool,
    /// A timer fired, or a call queued with a delay ran.
    pub timer: bool,
    /// An animation was due a step.
    pub animation: bool,
}

/// The frames of a scene since they were last taken, the times of each
/// step added up. Bindings and text are timed wherever they ran, in a
/// step or between frames (an input handler), so they overlap the steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTimes {
    /// Frames folded in: a host's pass runs one, but an agent that moves
    /// the clock runs one at each deadline it passes.
    pub frames: u32,
    pub woke: Woke,
    /// Timers and what their handlers did (step 3).
    pub timers: Duration,
    /// Stepping animations, flicks and canvases (step 5).
    pub animations: Duration,
    /// Polishing: positioners, layouts, anchors that wait on sizes (step 6).
    pub layout: Duration,
    /// Making what Loaders and views create in the background (step 7).
    pub create: Duration,
    /// Building the scene for the painter: syncing the items to the render
    /// tree and composing it (steps 8 and 9).
    pub scene: Duration,
    /// The rest of the frame: pictures and answers delivered, deletes.
    pub other: Duration,
    /// The whole of the frames.
    pub total: Duration,
    pub bindings: Duration,
    pub binding_runs: u64,
    /// Laying out text that was not cached.
    pub text: Duration,
    pub text_layouts: u64,
}

/// The time since the last lap.
pub(crate) struct Laps {
    started: Instant,
    last: Instant,
}

impl Laps {
    pub(crate) fn start() -> Self {
        let now = Instant::now();
        Laps {
            started: now,
            last: now,
        }
    }

    pub(crate) fn lap(&mut self) -> Duration {
        let now = Instant::now();
        let took = now - self.last;
        self.last = now;
        took
    }

    pub(crate) fn total(&self) -> Duration {
        self.started.elapsed()
    }
}

/// Times one frame's steps, if the scene is timed.
pub(crate) fn lap(laps: &mut Option<Laps>, into: &mut Duration) {
    if let Some(l) = laps {
        *into += l.lap();
    }
}

impl QuickScene {
    /// Times each frame from now on, with the bindings and the text laid
    /// out in it, for [`QuickScene::take_frame_times`]; or stops. The
    /// engine and the thread's text engine are timed with the scene; they
    /// are shared by every scene of the thread, so a second scene timed
    /// leaves what they gathered for the first.
    pub fn set_timed(&mut self, engine: &Engine, on: bool) {
        self.timing = on.then(FrameTimes::default);
        self.saw_input.set(false);
        if engine.is_timed() != on {
            engine.set_timed(on);
            self.text_engine().borrow_mut().set_timed(on);
        }
    }

    pub fn is_timed(&self) -> bool {
        self.timing.is_some()
    }

    /// The frames since the last call, if the scene is timed and ran any.
    pub fn take_frame_times(&mut self) -> Option<FrameTimes> {
        let t = self.timing.as_mut()?;
        (t.frames > 0).then(|| std::mem::take(t))
    }

    /// Input reached the scene: the next frame is woken by it.
    pub(crate) fn note_input(&self) {
        self.saw_input.set(true);
    }

    /// Adds a frame's steps, and the bindings and text since the last.
    pub(crate) fn add_frame_times(
        &mut self,
        engine: &Engine,
        laps: Laps,
        steps: FrameTimes,
        animating: bool,
    ) {
        let input = self.saw_input.take();
        let e = engine.take_times();
        let text = self.text_engine().borrow_mut().take_times();
        let Some(t) = self.timing.as_mut() else {
            return;
        };
        t.frames += 1;
        t.woke.input |= input;
        t.woke.timer |= e.timer_fired;
        t.woke.animation |= animating;
        t.timers += steps.timers;
        t.animations += steps.animations;
        t.layout += steps.layout;
        t.create += steps.create;
        t.scene += steps.scene;
        t.other += steps.other;
        t.total += laps.total();
        t.bindings += e.bindings;
        t.binding_runs += e.binding_runs;
        t.text += text.spent;
        t.text_layouts += text.layouts;
    }
}
