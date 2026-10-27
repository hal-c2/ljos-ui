//! `motion-dropped-frames`: while an animation the application declared
//! runs, the scene's own work for a frame takes longer than one frame of
//! the display (`limits::motion::FRAME_BUDGET_MS`), so frames are dropped.
//!
//! What is timed is the scene's work for each frame (bindings, script,
//! layout, the paint list), on this machine's CPU, while an animation the
//! application declared steps: what runs on its own at rest, and each open
//! popup's way in. Painting to the screen is not counted, so a frame that
//! passes here can still be dropped on a device, and one that fails may
//! not be on a faster one: hence a hint, which says the build it was
//! measured with. The middle frame is taken, so that one slow frame of a
//! busy machine is not a finding.

use ljos_design::limits::motion::FRAME_BUDGET_MS;
use ljos_engine::ObjectId;

use crate::acting::Act;
use crate::cx::declared_by_app;
use crate::motion_watch::{anim_words, frames_for, no_probe, reopened, with_motion};

/// How long what runs at rest is timed for, on the scene's clock.
const RESTING_MS: u32 = 500;

/// Fewer frames than this say nothing of a middle one.
const FEWEST: usize = 5;

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.len() < FEWEST {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

fn build() -> &'static str {
    if cfg!(debug_assertions) {
        "a debug build"
    } else {
        "a release build"
    }
}

pub(crate) fn check(act: &mut Act) {
    let e = act.engine;
    let mut found: Vec<(ObjectId, String, f64)> = Vec::new();
    act.rest();
    let resting: Vec<ObjectId> = ljos_quick::running_animations(e)
        .into_iter()
        .filter(|(a, paused)| !paused && declared_by_app(e, *a))
        .map(|(a, _)| a)
        .collect();
    if let Some(&anim) = resting.first() {
        let w = with_motion(act, false, |act| frames_for(act, RESTING_MS, &no_probe));
        if let Some(ms) = median(w.work_ms) {
            found.push((anim, "while it runs".into(), ms));
        }
    }
    for r in reopened(act).iter() {
        let Some(anim) = r.enter.own_runs(e).next().map(|run| run.anim) else {
            continue;
        };
        if let Some(ms) = median(r.enter.work_ms.clone()) {
            let words = crate::cx::words(&act.cx(), r.popup);
            found.push((anim, format!("opening {words}"), ms));
        }
    }
    for (anim, what, ms) in found {
        if ms <= f64::from(FRAME_BUDGET_MS) {
            continue;
        }
        let message = format!(
            "{what}, {} takes the scene about {ms:.1} ms of work a frame, where a frame at 60 Hz \
             is {FRAME_BUDGET_MS} ms: frames are dropped. Measured on this machine's CPU in {}, \
             painting not counted",
            anim_words(e, anim),
            build()
        );
        let f = act.report("motion-dropped-frames", Some(anim), message);
        f.measured = Some(format!("{ms:.1} ms > {FRAME_BUDGET_MS} ms"));
    }
}
