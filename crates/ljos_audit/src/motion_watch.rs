//! Following what a change starts, frame by frame on the scene's own
//! clock: which animations it started, how long each ran, what a value
//! did on the way, and how long the scene's own work took for each frame.
//!
//! The clock is the audit's: each frame is one display period after the
//! last, whatever the machine, so a length is the same on every run. A run
//! is seen to end at the first frame after it did, so a length is at most
//! one period long; the rules give that period to the application before
//! they report. The work of a frame is the one thing timed by the machine's
//! clock, and only for `motion-dropped-frames`.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ljos_design::limits::audit::ANIMATION_WAIT_MS;
use ljos_design::limits::motion::FRAME_BUDGET_MS;
use ljos_engine::{Engine, ObjectId};

use crate::acting::Act;
use crate::cx::{declared_by_app, place_of};

/// One frame of the display, as the audit's clock steps it.
pub(crate) fn period() -> Duration {
    Duration::from_micros((f64::from(FRAME_BUDGET_MS) * 1000.0) as u64)
}

pub(crate) fn period_ms() -> f64 {
    f64::from(FRAME_BUDGET_MS)
}

/// An animation a change started.
pub(crate) struct Run {
    pub anim: ObjectId,
    /// How long it ran, in ms; `None` when it still ran when the audit
    /// stopped looking.
    pub ms: Option<f64>,
}

/// What a change started, and what it did.
pub(crate) struct Watched {
    pub runs: Vec<Run>,
    /// Each frame's time since the change, in ms, with what the probe read
    /// then.
    pub samples: Vec<(f64, Vec<f64>)>,
    /// The scene's own work for each frame in which one of the runs the
    /// application declared stepped, in ms of the machine's clock.
    pub work_ms: Vec<f64>,
}

impl Watched {
    /// The runs of animations the application declared, and not its
    /// style: what the application can change.
    pub(crate) fn own_runs<'a>(&'a self, e: &'a Engine) -> impl Iterator<Item = &'a Run> + 'a {
        self.runs.iter().filter(move |r| declared_by_app(e, r.anim))
    }
}

fn running(e: &Engine) -> HashSet<ObjectId> {
    ljos_quick::running_animations(e)
        .into_iter()
        .filter(|(_, paused)| !paused)
        .map(|(a, _)| a)
        .collect()
}

/// Runs `f` with the system's reduced motion setting at `reduced`, and
/// puts the setting back afterwards.
pub(crate) fn with_motion<R>(act: &mut Act, reduced: bool, f: impl FnOnce(&mut Act) -> R) -> R {
    let e = act.engine;
    let held = ljos_quick::hold_system_prefs(e);
    let mut prefs = ljos_quick::system_prefs(e);
    let changed = prefs.reduced_motion != reduced;
    if changed {
        prefs.reduced_motion = reduced;
        ljos_quick::set_system_prefs(e, prefs);
        act.settle();
    }
    let out = f(act);
    // What is still running ends before the setting comes back, so that
    // the change back starts nothing.
    act.rest();
    ljos_quick::restore_system_prefs(e, held);
    act.settle();
    out
}

/// Makes the change `trigger` makes, and follows what it starts for up to
/// `limits::audit::ANIMATION_WAIT_MS`, reading `probe` at every frame.
pub(crate) fn watch(
    act: &mut Act,
    trigger: impl FnOnce(&mut Act),
    probe: &dyn Fn(&Engine) -> Vec<f64>,
) -> Watched {
    let e = act.engine;
    let before = running(e);
    trigger(act);
    let t0 = act.now;
    let end = t0 + Duration::from_millis(u64::from(ANIMATION_WAIT_MS));
    let mut started: Vec<(ObjectId, ljos_runtime::Time, Option<f64>)> = Vec::new();
    let mut samples = Vec::new();
    let mut work_ms = Vec::new();
    let mut first = true;
    loop {
        let clock = Instant::now();
        act.scene.frame(e, act.now);
        let work = clock.elapsed();
        let now_running = running(e);
        for a in &now_running {
            if !before.contains(a) && !started.iter().any(|(s, _, _)| s == a) {
                started.push((*a, act.now, None));
            }
        }
        let mut stepped = false;
        for (a, at, ms) in &mut started {
            if ms.is_none() {
                if now_running.contains(a) {
                    stepped |= declared_by_app(e, *a);
                } else {
                    *ms = Some(act.now.saturating_since(*at).as_secs_f64() * 1000.0);
                }
            }
        }
        if stepped && !first {
            work_ms.push(work.as_secs_f64() * 1000.0);
        }
        let since = act.now.saturating_since(t0).as_secs_f64() * 1000.0;
        samples.push((since, probe(e)));
        let all_ended = started.iter().all(|(_, _, ms)| ms.is_some());
        if (all_ended && !act.scene.needs_frame()) || act.now >= end {
            break;
        }
        first = false;
        act.now = (act.now + period()).min(end);
    }
    act.settle();
    Watched {
        runs: started
            .into_iter()
            .map(|(anim, _, ms)| Run { anim, ms })
            .collect(),
        samples,
        work_ms,
    }
}

/// Nothing to read at each frame.
pub(crate) fn no_probe(_: &Engine) -> Vec<f64> {
    Vec::new()
}

/// How a message names an animation: its type and where it is declared.
pub(crate) fn anim_words(e: &Engine, anim: ObjectId) -> String {
    let ty = e
        .meta_of(anim)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| "animation".into());
    match place_of(e, anim) {
        Some(p) => format!("the {ty} at {p}"),
        None => format!("the {ty}"),
    }
}

/// A run's length less the one frame it may have ended before it was
/// seen to: what it certainly ran.
pub(crate) fn surely(ms: f64) -> f64 {
    (ms - period_ms()).max(0.0)
}

/// Frames the scene for `ms` on its clock, reading `probe` at every frame,
/// with nothing changed: what keeps moving by itself. Every frame's work
/// is timed.
pub(crate) fn frames_for(act: &mut Act, ms: u32, probe: &dyn Fn(&Engine) -> Vec<f64>) -> Watched {
    let e = act.engine;
    let t0 = act.now;
    let end = t0 + Duration::from_millis(u64::from(ms));
    let mut samples = Vec::new();
    let mut work_ms = Vec::new();
    loop {
        let clock = Instant::now();
        act.scene.frame(e, act.now);
        work_ms.push(clock.elapsed().as_secs_f64() * 1000.0);
        let since = act.now.saturating_since(t0).as_secs_f64() * 1000.0;
        samples.push((since, probe(e)));
        if act.now >= end {
            break;
        }
        act.now = (act.now + period()).min(end);
    }
    // The first frame does what was waiting before the audit looked.
    work_ms.remove(0);
    Watched {
        runs: Vec::new(),
        samples,
        work_ms,
    }
}

/// A popup the audit closed and opened again, with what each way did.
pub(crate) struct Reopened {
    pub popup: ObjectId,
    pub exit: Watched,
    /// Read at each frame: the opacity, scale, x and y of the item the
    /// popup is drawn as.
    pub enter: Watched,
}

/// What [`Reopened::enter`] reads, in order.
pub(crate) const ENTER_PROBES: [&str; 4] = ["opacity", "scale", "x", "y"];

/// Each open popup the application declared, closed and opened again
/// with motion unreduced, both ways followed: done by the first rule that
/// asks, and shared with the others, so that a popup's transitions run
/// once an audit and not once a rule. The scene is as it was after the
/// first time, so a second would see the same.
pub(crate) fn reopened(act: &mut Act) -> Rc<Vec<Reopened>> {
    if let Some(seen) = &act.reopened {
        return Rc::clone(seen);
    }
    let seen = Rc::new(reopen_popups(act));
    act.reopened = Some(Rc::clone(&seen));
    seen
}

/// Closes each open popup the application declared and opens it again,
/// with motion as the system has it unreduced, following both ways. Focus
/// is put back afterwards.
fn reopen_popups(act: &mut Act) -> Vec<Reopened> {
    act.rest();
    let e = act.engine;
    let popups: Vec<(ObjectId, ObjectId)> = {
        let cx = act.cx();
        crate::surfaces::open_surfaces(&cx)
            .into_iter()
            .filter(|s| declared_by_app(e, s.popup))
            .filter(|s| e.untrack(|| e.read_by_name(s.popup, "opened")).to_boolean())
            .map(|s| (s.popup, s.item))
            .collect()
    };
    if popups.is_empty() {
        return Vec::new();
    }
    let held = crate::acting::hold_focus(act);
    let out = with_motion(act, false, |act| {
        let mut out = Vec::new();
        for (popup, item) in popups {
            if !e.is_alive(popup) {
                continue;
            }
            let exit = watch(
                act,
                |a| {
                    let _ = a.engine.invoke_by_name(popup, "close", &[]);
                },
                &no_probe,
            );
            let read = move |e: &Engine| -> Vec<f64> {
                ENTER_PROBES
                    .iter()
                    .map(|n| {
                        if e.is_alive(item) {
                            e.untrack(|| e.read_by_name(item, n)).to_number()
                        } else {
                            f64::NAN
                        }
                    })
                    .collect()
            };
            let enter = watch(
                act,
                |a| {
                    let _ = a.engine.invoke_by_name(popup, "open", &[]);
                },
                &read,
            );
            out.push(Reopened { popup, exit, enter });
        }
        out
    });
    crate::acting::restore_focus(act, &held);
    out
}
