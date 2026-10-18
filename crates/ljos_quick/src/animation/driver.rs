//! The animation driver: what is running, and when a frame is next needed
//! for it (ARCHITECTURE §7.2 step 5, §8.1, §8.2).
//!
//! There is one [`Driver`] per engine. A started animation becomes a
//! [`Run`]: its [`Timed`] timeline and the time it has been going. The
//! scene calls [`tick`] once per frame with the frame's time; every run is
//! then brought to that moment. Time comes from nowhere else, so a test
//! (or the BDD clock) that hands out frame times decides what an animation
//! shows.
//!
//! After the frame the scene asks [`next_wake`]. A run whose values show
//! asks for the next frame tick. A run that is between two moving parts, or
//! that moves only what nobody sees, asks for one wake at the moment
//! something happens (a part begins, a script is due, it ends). A paused
//! run and an endless run nobody sees ask for nothing. With no runs the
//! answer is `None`: nothing polls.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{Engine, ObjectId, PropIndex, Value};
use ljos_reactive::WriteKind;
use ljos_runtime::{FrameScheduler, Time, TimerId, TimerKind};
use rustc_hash::FxHashMap;

use super::timeline::{Ctx, Defaults, FOREVER, Timed, warn};
use super::types::{Animation, is_managed};

/// Called when a driven run ends by itself.
pub(crate) type Done = Rc<dyn Fn(&Engine)>;

#[derive(Default)]
pub(crate) struct Driver {
    /// In the order they started, which is the order they advance in.
    /// A removed run stays, cancelled, until [`Driver::compact`]: a
    /// restart or an end then costs the same however many runs there are.
    runs: RefCell<Vec<Rc<Run>>>,
    /// Some of `runs` are cancelled.
    stale: Cell<bool>,
    by_anim: RefCell<FxHashMap<ObjectId, Rc<Run>>>,
    /// Turns "a frame per tick" and "a wake at this moment" into one time.
    sched: RefCell<FrameScheduler>,
    /// The wake asked for by the runs that need no frames.
    boundary: Cell<Option<TimerId>>,
    /// The last frame's time and the engine clock's time then: what "now"
    /// is between frames is worked out from the two.
    last_frame: Cell<Option<(Time, Time)>>,
    /// The frame time, while [`tick`] runs.
    ticking: Cell<Option<Time>>,
    /// [`super::register`] ran.
    pub(crate) registered: Cell<bool>,
    /// What [`later`] was given, until the next frame.
    later: RefCell<Vec<Deferred>>,
}

/// Something to do in the next frame.
type Deferred = Box<dyn FnOnce(&Engine)>;

struct Run {
    anim: ObjectId,
    /// Taken out while it advances: a script in it may come back here.
    /// Boxed, as taking it out and putting it back is every frame's.
    timeline: RefCell<Option<Box<Timed>>>,
    /// Microseconds it has been going, pauses left out.
    elapsed: Cell<i64>,
    /// The frame time `elapsed` is of. `None`: it begins with the next
    /// frame.
    last: Cell<Option<Time>>,
    paused: Cell<bool>,
    /// Stopped or ended: nothing more of it may happen.
    cancelled: Cell<bool>,
    /// `stop()` with `alwaysRunToEnd`: `running` is false already.
    finishing: Cell<bool>,
    /// `complete()` came from one of its own scripts.
    want_complete: Cell<bool>,
    /// Reduced motion took it to the end of its pass, where it stays for
    /// as long as motion is reduced. Only a run without end rests: one
    /// with an end is over instead.
    rested: Cell<bool>,
    /// `stop()` on the way emits `finished` too.
    finishes_when_stopped: bool,
    /// Something else runs it (a `Behavior`; later a `Transition`): it
    /// shows `running` and emits nothing.
    driven: bool,
    /// A driven run of something that is no animation (the animations of a
    /// transition between states, under the transition): it has no
    /// `running` of its own to show.
    silent: bool,
    done: Option<Done>,
}

fn driver(engine: &Engine) -> Rc<Driver> {
    engine.extension::<Driver>()
}

fn micros(d: Duration) -> i64 {
    i64::try_from(d.as_micros()).map_or(FOREVER, |us| us.min(FOREVER))
}

/// Shows a state on the animation object. Like an animated value, it
/// leaves a binding on the property in place (`running: mouse.pressed`).
fn show(engine: &Engine, anim: ObjectId, prop: PropIndex, v: bool) {
    let _ = engine.write_internal(anim, prop, Value::Bool(v), WriteKind::Host);
}

fn emit(engine: &Engine, anim: ObjectId, signal: &str) {
    let _ = engine.emit_by_name(anim, signal, &[]);
}

impl Driver {
    fn find(&self, anim: ObjectId) -> Option<Rc<Run>> {
        self.by_anim.borrow().get(&anim).cloned()
    }

    fn add(&self, run: &Rc<Run>) {
        self.runs.borrow_mut().push(Rc::clone(run));
        self.by_anim.borrow_mut().insert(run.anim, Rc::clone(run));
    }

    /// Ends `run`. What its Animators show is written now: where they got
    /// to, as Qt does.
    fn remove(&self, engine: &Engine, run: &Rc<Run>) {
        run.cancelled.set(true);
        // Inside its own step (a script that stops it) the timeline is out:
        // the step releases it when it puts it back.
        if let Ok(mut t) = run.timeline.try_borrow_mut()
            && let Some(t) = t.as_mut()
        {
            t.release(engine);
        }
        self.stale.set(true);
        let mut by_anim = self.by_anim.borrow_mut();
        if by_anim.get(&run.anim).is_some_and(|r| Rc::ptr_eq(r, run)) {
            by_anim.remove(&run.anim);
        }
    }

    /// Drops the cancelled runs; each is passed over until then.
    fn compact(&self) {
        if self.stale.replace(false) {
            self.runs.borrow_mut().retain(|r| !r.cancelled.get());
        }
    }

    /// The frame clock's time now, as far as it is known.
    fn now(&self, engine: &Engine) -> Option<Time> {
        self.ticking.get().or_else(|| {
            self.last_frame
                .get()
                .map(|(frame, clock)| frame + engine.now().saturating_since(clock))
        })
    }
}

impl Run {
    /// Brings the timeline to `t`.
    fn step(&self, engine: &Engine, t: i64) {
        let Some(mut timeline) = self.timeline.borrow_mut().take() else {
            return;
        };
        let cx = Ctx {
            engine,
            cancelled: &self.cancelled,
            scripts: true,
        };
        timeline.advance(t, &cx);
        if self.cancelled.get() {
            timeline.release(engine);
        }
        *self.timeline.borrow_mut() = Some(timeline);
    }

    fn is_done(&self) -> bool {
        self.timeline
            .borrow()
            .as_deref()
            .is_some_and(Timed::is_done)
    }

    /// Where reduced motion takes the run in this frame: to its end, or,
    /// without one, to the end of the pass it is in, once. `None`: it
    /// rests there already.
    fn calm_end(&self) -> Option<i64> {
        let timeline = self.timeline.borrow();
        let timeline = timeline.as_ref()?;
        if !timeline.is_endless() {
            return Some(timeline.total());
        }
        if self.rested.replace(true) {
            return None;
        }
        Some(timeline.end_of_pass(self.elapsed.get()))
    }
}

/// `start()`, or `running` set to true.
pub(crate) fn start(engine: &Engine, anim: ObjectId) {
    let d = driver(engine);
    if is_managed(engine, anim) {
        warn(
            engine,
            anim,
            format_args!("setRunning() cannot be used on non-root animation nodes."),
        );
        return;
    }
    if let Some(run) = d.find(anim) {
        // Started again while running out after a stop: it is simply
        // running again.
        if run.finishing.replace(false) {
            show(engine, anim, Animation::RUNNING.index, true);
        }
        return;
    }
    let defaults = engine
        .with::<Animation, _>(anim, |a| a.on)
        .ok()
        .flatten()
        .map(|(obj, prop)| Defaults {
            obj,
            prop,
            to: None,
        });
    begin(engine, &d, anim, defaults.as_ref(), false, None);
}

/// Starts `anim` for something that runs it itself: it shows `running`
/// but emits no `started`, `stopped` or `finished`, and `done` is called
/// when it ends by itself. A run of it that is under way is replaced
/// without `running` changing.
pub(crate) fn start_driven(
    engine: &Engine,
    anim: ObjectId,
    defaults: &Defaults,
    done: Option<Done>,
) {
    let d = driver(engine);
    engine.batch(|| {
        if let Some(old) = d.find(anim) {
            d.remove(engine, &old);
        }
        begin(engine, &d, anim, Some(defaults), true, done);
    });
}

/// Ends a driven run where it is.
pub(crate) fn stop_driven(engine: &Engine, anim: ObjectId) {
    if forget_driven(engine, anim) {
        show(engine, anim, Animation::RUNNING.index, false);
    }
}

/// Ends a driven run without touching a property, for where nothing may
/// be written. True if there was one; the caller owes the animation its
/// `running: false` ([`stop_driven`] does both).
pub(crate) fn forget_driven(engine: &Engine, anim: ObjectId) -> bool {
    let d = driver(engine);
    match d.find(anim) {
        Some(run) if run.driven => {
            d.remove(engine, &run);
            true
        }
        _ => false,
    }
}

/// Runs a timeline built elsewhere under `key`, which need not be an
/// animation: nothing is shown on it and nothing is emitted. `done` is
/// called when the timeline ends by itself, which may be before this
/// returns. A run under `key` that is under way is replaced; end one with
/// [`forget_driven`].
pub(crate) fn start_timeline(engine: &Engine, key: ObjectId, timeline: Timed, done: Option<Done>) {
    let d = driver(engine);
    if let Some(old) = d.find(key) {
        d.remove(engine, &old);
    }
    let run = Rc::new(Run {
        anim: key,
        finishes_when_stopped: false,
        timeline: RefCell::new(Some(Box::new(timeline))),
        elapsed: Cell::new(0),
        last: Cell::new(d.ticking.get()),
        paused: Cell::new(false),
        cancelled: Cell::new(false),
        finishing: Cell::new(false),
        want_complete: Cell::new(false),
        rested: Cell::new(false),
        driven: true,
        silent: true,
        done,
    });
    d.add(&run);
    engine.batch(|| run.step(engine, 0));
    settle(engine, &d, &run);
    if !run.cancelled.get() {
        engine.request_wake();
    }
}

/// Calls `f` at the start of the next frame, and asks for that frame.
/// For what must not happen before the code that caused it has returned.
pub(crate) fn later(engine: &Engine, f: Box<dyn FnOnce(&Engine)>) {
    driver(engine).later.borrow_mut().push(f);
    engine.request_wake();
}

/// Shows `running: false` on an animation whose driven run was forgotten,
/// unless it runs again by now.
pub(crate) fn settle_driven(engine: &Engine, anim: ObjectId) {
    if !has_run(engine, anim) {
        show(engine, anim, Animation::RUNNING.index, false);
    }
}

/// Whether a run of `anim` exists, driven or not.
pub(crate) fn has_run(engine: &Engine, anim: ObjectId) -> bool {
    driver(engine).find(anim).is_some()
}

fn begin(
    engine: &Engine,
    d: &Rc<Driver>,
    anim: ObjectId,
    defaults: Option<&Defaults>,
    driven: bool,
    done: Option<Done>,
) {
    let mut timeline = Timed::build(engine, anim, defaults);
    if timeline.loops() == 0 {
        // Qt runs such an animation without end and without effect.
        timeline = Timed::inert();
    }
    let run = Rc::new(Run {
        anim,
        finishes_when_stopped: timeline.finishes_when_stopped(),
        timeline: RefCell::new(Some(Box::new(timeline))),
        elapsed: Cell::new(0),
        // Inside a frame it begins with that frame, else with the next.
        last: Cell::new(d.ticking.get()),
        paused: Cell::new(false),
        cancelled: Cell::new(false),
        finishing: Cell::new(false),
        want_complete: Cell::new(false),
        rested: Cell::new(false),
        driven,
        silent: false,
        done,
    });
    // Known to the driver before `running` says so: whoever follows
    // `running` must find the two agree.
    d.add(&run);
    engine.batch(|| {
        show(engine, anim, Animation::RUNNING.index, true);
        if !driven {
            emit(engine, anim, "started");
        }
        // The first moment: `from` values, and whatever takes no time.
        run.step(engine, 0);
    });
    settle(engine, d, &run);
    if !run.cancelled.get() {
        engine.request_wake();
    }
}

/// After a step: ends the run if that step was its last.
fn settle(engine: &Engine, d: &Rc<Driver>, run: &Rc<Run>) {
    if run.cancelled.get() {
        return;
    }
    if run.want_complete.take() {
        complete_run(engine, d, run);
    } else if run.is_done() {
        finish(engine, d, run);
    }
}

/// The run reached its end.
fn finish(engine: &Engine, d: &Rc<Driver>, run: &Rc<Run>) {
    let anim = run.anim;
    d.remove(engine, run);
    if run.paused.replace(false) {
        show(engine, anim, Animation::PAUSED.index, false);
    }
    if run.driven {
        if !run.silent {
            show(engine, anim, Animation::RUNNING.index, false);
        }
        if let Some(done) = &run.done {
            done(engine);
        }
        return;
    }
    if run.finishing.get() {
        emit(engine, anim, "stopped");
    } else {
        engine.batch(|| {
            show(engine, anim, Animation::RUNNING.index, false);
            emit(engine, anim, "stopped");
        });
    }
    emit(engine, anim, "finished");
}

/// Brings a run to the present before something is done to it between
/// two frames. Without this a run that needs no frames would be stopped
/// or paused where the last frame left it, however long ago that was.
fn catch_up(engine: &Engine, d: &Rc<Driver>, run: &Rc<Run>) {
    if run.paused.get() || run.cancelled.get() || run.rested.get() {
        return;
    }
    let (Some(now), Some(last)) = (d.now(engine), run.last.get()) else {
        return;
    };
    if now <= last {
        return;
    }
    let t = run
        .elapsed
        .get()
        .saturating_add(micros(now.saturating_since(last)));
    run.elapsed.set(t);
    run.last.set(Some(now));
    run.step(engine, t);
    settle(engine, d, run);
}

/// `stop()`, or `running` set to false.
pub(crate) fn stop(engine: &Engine, anim: ObjectId) {
    let d = driver(engine);
    let Some(run) = d.find(anim) else {
        return;
    };
    if run.driven || run.finishing.get() {
        return;
    }
    catch_up(engine, &d, &run);
    if run.cancelled.get() {
        return;
    }
    if run.paused.replace(false) {
        run.last.set(d.ticking.get());
        show(engine, anim, Animation::PAUSED.index, false);
    }
    let to_end = engine
        .untrack(|| engine.read(anim, Animation::ALWAYS_RUN_TO_END.index))
        .to_boolean();
    if to_end && let Some(timeline) = run.timeline.borrow_mut().as_mut() {
        timeline.end_after_pass(run.elapsed.get());
        run.finishing.set(true);
        show(engine, anim, Animation::RUNNING.index, false);
        engine.request_wake();
        return;
    }
    d.remove(engine, &run);
    if run.finishes_when_stopped {
        // Qt reports an animation without end as finished when stopped.
        emit(engine, anim, "finished");
    }
    engine.batch(|| {
        show(engine, anim, Animation::RUNNING.index, false);
        emit(engine, anim, "stopped");
    });
}

/// `pause()`, `resume()`, or `paused` set.
pub(crate) fn set_paused(engine: &Engine, anim: ObjectId, want: bool) {
    let d = driver(engine);
    let Some(run) = d.find(anim).filter(|r| !r.driven) else {
        if want {
            warn(
                engine,
                anim,
                format_args!("setPaused() cannot be used when animation isn't running."),
            );
        }
        return;
    };
    if want == run.paused.get() {
        return;
    }
    if want {
        catch_up(engine, &d, &run);
        if run.cancelled.get() {
            return;
        }
        run.paused.set(true);
        show(engine, anim, Animation::PAUSED.index, true);
    } else {
        run.paused.set(false);
        // Goes on from the frame it is resumed in.
        run.last.set(d.ticking.get());
        show(engine, anim, Animation::PAUSED.index, false);
        engine.request_wake();
    }
}

/// `complete()`.
pub(crate) fn complete(engine: &Engine, anim: ObjectId) {
    let d = driver(engine);
    if let Some(run) = d.find(anim).filter(|r| !r.driven) {
        complete_run(engine, &d, &run);
    }
}

fn complete_run(engine: &Engine, d: &Rc<Driver>, run: &Rc<Run>) {
    let end = match run.timeline.borrow().as_ref() {
        // Without an end, the end of the pass it is in.
        Some(t) if t.is_endless() => t.end_of_pass(run.elapsed.get()),
        Some(t) => t.total(),
        None => {
            // Called by one of its own scripts: when that returns.
            run.want_complete.set(true);
            return;
        }
    };
    run.elapsed.set(end);
    run.step(engine, end);
    if !run.cancelled.get() {
        finish(engine, d, run);
    }
}

/// Whether `running` should read true.
pub(crate) fn is_running(engine: &Engine, anim: ObjectId) -> bool {
    driver(engine)
        .find(anim)
        .is_some_and(|r| !r.finishing.get())
}

pub(crate) fn is_paused(engine: &Engine, anim: ObjectId) -> bool {
    driver(engine).find(anim).is_some_and(|r| r.paused.get())
}

/// The display's refresh period, for the frames a visible animation asks
/// for. 60 Hz until set.
pub(crate) fn set_frame_period(engine: &Engine, period: Duration) {
    driver(engine).sched.borrow_mut().set_frame_period(period);
}

pub(crate) fn frame_period(engine: &Engine) -> Duration {
    driver(engine).sched.borrow().frame_period()
}

/// Frame step 5: brings every run to `now`, the frame's time.
pub(crate) fn tick(engine: &Engine, now: Time) {
    let d = driver(engine);
    let later = std::mem::take(&mut *d.later.borrow_mut());
    for f in later {
        f(engine);
    }
    d.compact();
    if d.runs.borrow().is_empty() {
        d.last_frame.set(Some((now, engine.now())));
        return;
    }
    d.ticking.set(Some(now));
    let runs: Vec<Rc<Run>> = d.runs.borrow().clone();
    let calm = crate::system_prefs::reduced_motion_now(engine);
    // One batch: what follows several animated values (a layout, a
    // binding on two of them) is worked out once per frame.
    engine.batch(|| {
        for run in &runs {
            if run.cancelled.get() || run.paused.get() {
                continue;
            }
            if !engine.is_alive(run.anim) {
                d.remove(engine, run);
                continue;
            }
            // A frozen page's run stands still with its clock: on thaw it
            // goes on from where it would be by then (§8.3).
            if engine.is_frozen(run.anim) {
                continue;
            }
            if calm {
                run.last.set(Some(now));
                if let Some(end) = run.calm_end() {
                    run.elapsed.set(end);
                    run.step(engine, end);
                }
                continue;
            }
            // At rest no time passed for it: it goes on from where it
            // rests, not from where it would be by now.
            let last = run.last.replace(Some(now));
            let last = if run.rested.replace(false) {
                None
            } else {
                last
            };
            let t = match last {
                Some(last) => run
                    .elapsed
                    .get()
                    .saturating_add(micros(now.saturating_since(last))),
                None => run.elapsed.get(),
            };
            run.elapsed.set(t);
            run.step(engine, t);
        }
    });
    for run in &runs {
        settle(engine, &d, run);
    }
    d.ticking.set(None);
    d.last_frame.set(Some((now, engine.now())));
}

/// When the runs next need a frame, asked at the end of the frame at
/// `now`. `None`: not before something else changes.
pub(crate) fn next_wake(engine: &Engine, now: Time) -> Option<Time> {
    let d = driver(engine);
    if !d.later.borrow().is_empty() {
        // Asked for after this frame's tick: one more frame, now.
        return Some(now);
    }
    d.compact();
    if d.runs.borrow().is_empty() && d.boundary.get().is_none() {
        return None;
    }
    let mut sched = d.sched.borrow_mut();
    sched.begin_frame(now);
    if let Some(id) = d.boundary.take() {
        sched.cancel(id);
    }
    let mut animating = false;
    let mut soonest: Option<Time> = None;
    let calm = crate::system_prefs::reduced_motion_now(engine);
    for run in d.runs.borrow().iter() {
        // A frame is asked for already: what is left is the start of a run
        // that has none, not two hundred more looks at the engine.
        if animating && run.last.get().is_some() {
            continue;
        }
        if run.paused.get() || engine.is_frozen(run.anim) {
            continue;
        }
        // Started in this frame after its tick: it began with this frame.
        let last = run.last.get().unwrap_or(now);
        run.last.set(Some(last));
        if calm {
            // One more frame takes it to its end, seen or not; at rest it
            // asks for nothing.
            animating |= !run.rested.get();
            continue;
        }
        if run.rested.get() {
            // Motion is no longer reduced: the next frame moves it again.
            animating = true;
            continue;
        }
        let timeline = run.timeline.borrow();
        let Some(timeline) = timeline.as_ref() else {
            continue;
        };
        if animating {
            continue;
        }
        if timeline.wants_frames(engine) {
            animating = true;
        } else if let Some(at) = timeline.next_event(engine, run.elapsed.get()) {
            let wait = u64::try_from(at.saturating_sub(run.elapsed.get())).unwrap_or(0);
            let at = last + Duration::from_micros(wait);
            soonest = Some(soonest.map_or(at, |s| s.min(at)));
        }
    }
    if animating {
        sched.request_animation_frame();
    } else if let Some(at) = soonest {
        d.boundary
            .set(Some(sched.add_timer(at, TimerKind::Animation)));
    }
    sched.next_wake()
}

/// The number of animations under way, paused ones included.
pub(crate) fn run_count(engine: &Engine) -> usize {
    let d = driver(engine);
    d.compact();
    d.runs.borrow().len()
}

pub(crate) fn runs(engine: &Engine) -> Vec<(ObjectId, bool)> {
    driver(engine)
        .runs
        .borrow()
        .iter()
        .filter(|r| !r.cancelled.get())
        .map(|r| (r.anim, r.paused.get()))
        .collect()
}
