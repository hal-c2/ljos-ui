//! The `health-*` rules: how the application runs, rather than what it
//! shows. A window that freezes for a third of a second at every update,
//! a handler that throws, a heap that fills until scripts stop: none of it
//! is on a still frame, and all of it is what a user meets first.
//!
//! What is judged is what the engine recorded while the application ran:
//! its turns ([`ljos_engine::TurnRecord`], from [`crate::watch`] on), its
//! faults, its Loaders and its script heap. A turn's work is counted in
//! the interpreter's polls, not timed, so a turn over the bound is over it
//! on every machine; only `health-turn-slow` reads a clock, and is a hint.
//!
//! The heap and the frames are read while the audit lets the application
//! run on its own clock ([`crate::Options::clock`]): a few windows of
//! application time, the heap sampled after a collection before and after
//! each. That runs the application's timers and the host's work, so it is
//! done only when the audit may act.

use std::collections::BTreeMap;

use ljos_design::limits::health::{
    HEAP_GROWTH_MIN_BYTES, HEAP_HEADROOM_MIN, HEAP_WINDOW_MS, HEAP_WINDOWS, IDLE_FRAMES_MAX,
    POLL_MS, TURN_WORK_MAX,
};
use ljos_drive::{DriveError, DriveState, Driver};
use ljos_engine::{DEFAULT_TURN_BUDGET, Engine, FaultReport, ObjectId, fault_kind};
use ljos_quick::{Loader, QuickScene};
use ljos_runtime::{Fault, Time};

use crate::Options;
use crate::cx::{Cx, belongs_to, is_a, place_of};
use crate::finding::Finding;

/// The codes only letting the application run can check.
pub(crate) const RUNNING: &[&str] = &["health-heap-growth", "health-never-settles"];

/// Every code this module reports.
pub(crate) const CODES: &[&str] = &[
    "health-turn-over-budget",
    "health-turn-slow",
    "health-fault",
    "health-loader-failed",
    "health-heap-headroom",
    "health-heap-growth",
    "health-never-settles",
];

/// What letting the application run showed.
#[derive(Default)]
struct Ran {
    /// The heap after a collection: before the first window and after
    /// each.
    heap: Vec<usize>,
    /// The frames that showed something new in the last whole window
    /// while nothing animated.
    idle_frames: Option<u64>,
    /// The settle that never came to rest, and when.
    unsettled: Option<(usize, Time)>,
}

/// What the health checks found, the time the application's clock
/// reached, and the codes they could not judge, with why.
pub(crate) struct Health {
    pub found: Vec<Finding>,
    pub until: Time,
    pub not_checked: Vec<(&'static str, &'static str)>,
}

/// Why the checks that let the application run did not: not let act.
const NOT_LET_RUN: &str = "these let the application run on its own clock for a few seconds, \
                           which runs its timers, and this audit was not asked to act";
/// Why they did not: no clock.
const NO_CLOCK: &str = "these let the application run on its own clock for a few seconds, and \
                        the audit was given none it may move (an application on the wall clock \
                        runs on time no audit can let pass)";
/// Why one did not: the run stopped first.
const CUT_SHORT: &str = "the application's run stopped before this could be judged";
/// Why the heap's headroom was not checked.
const NOT_COLLECTED: &str = "the heap is judged after a collection, which can run the \
                             application's destruction handlers, and this audit was not asked \
                             to act; uncollected, the heap was too full to tell";

/// The heap after a collection, if a script ever ran.
fn heap_now(engine: &Engine) -> Option<usize> {
    engine.collect_garbage();
    heap_as_is(engine)
}

/// The heap as it is, garbage and all: at least what a collection leaves.
fn heap_as_is(engine: &Engine) -> Option<usize> {
    engine.script_memory().map(|m| m.bytes)
}

fn animating(engine: &Engine) -> bool {
    ljos_quick::running_animation_count(engine) > 0
}

/// Lets the application run [`HEAP_WINDOWS`] windows on `clock`.
fn run_windows(
    engine: &Engine,
    scene: &mut QuickScene,
    clock: &ljos_runtime::VirtualClock,
    now: Time,
) -> Ran {
    let mut ran = Ran::default();
    // The scene was framed at `now`; its clock does not run backwards.
    clock.set(now);
    ran.heap.extend(heap_now(engine));
    let mut state = DriveState::default();
    for _ in 0..HEAP_WINDOWS {
        // The frames that showed something new, a stop of the clock at a
        // time; those of a stop with an animation running at either end
        // are the animation's.
        let mut idle = 0;
        let mut shown = state.shown;
        let mut moving = animating(engine);
        let step = Driver::new(engine, scene, clock, &mut state).advance_until(
            u64::from(HEAP_WINDOW_MS),
            None,
            |d| {
                let now_moving = animating(d.engine);
                if !(moving || now_moving) {
                    idle += d.state.shown - shown;
                }
                shown = d.state.shown;
                moving = now_moving;
                Ok(false)
            },
        );
        match step {
            Ok(_) => {}
            Err(DriveError::Unsettled { frames }) => {
                ran.unsettled = Some((frames, ljos_runtime::Clock::now(clock)));
                break;
            }
            // What else stops a run (a timer with no interval, a real
            // clock) is no finding of these rules.
            Err(_) => break,
        }
        ran.heap.extend(heap_now(engine));
        ran.idle_frames = Some(idle);
    }
    ran
}

/// Runs the health checks `options` asks for once the rest of the audit is
/// done, so that the turns its acting checks set off are judged too.
pub(crate) fn run(engine: &Engine, scene: &mut QuickScene, now: Time, options: &Options) -> Health {
    let mut health = Health {
        found: Vec::new(),
        until: now,
        not_checked: Vec::new(),
    };
    if !CODES.iter().any(|c| options.wants(c)) {
        return health;
    }
    // The windows run for any health code: what goes wrong on a timer or
    // a stream's news (a heavy turn, a fault, a Loader that fails) comes
    // only once time passes.
    let ran = match (&options.clock.0, options.act) {
        (Some(clock), true) => {
            let ran = run_windows(engine, scene, clock, now);
            health.until = ljos_runtime::Clock::now(clock).max(now);
            Some(ran)
        }
        (None, true) => {
            health
                .not_checked
                .extend(RUNNING.iter().map(|c| (*c, NO_CLOCK)));
            None
        }
        (_, false) => {
            health
                .not_checked
                .extend(RUNNING.iter().map(|c| (*c, NOT_LET_RUN)));
            None
        }
    };
    let mut cx = Cx::new(engine, scene, options);
    turns(&mut cx);
    faults(&mut cx);
    loaders(&mut cx);
    heap(&mut cx, ran.as_ref(), &mut health.not_checked);
    if let Some(ran) = &ran {
        settles(&mut cx, ran, &mut health.not_checked);
    }
    health.found = cx.out;
    health
}

/// `polls` as an optimised build's milliseconds.
fn ms(polls: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let p = polls as f64;
    (p * POLL_MS).round()
}

fn mib(bytes: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    (b / (1024.0 * 1024.0) * 10.0).round() / 10.0
}

/// `health-turn-over-budget` and `health-turn-slow`.
fn turns(cx: &mut Cx) {
    let Some(record) = cx.engine.recorded_turns() else {
        return;
    };
    // The turns over the bound, by where their work came in: the
    // heaviest handler's place, else what started the turn.
    let mut by_place: BTreeMap<String, Vec<&ljos_engine::RecordedTurn>> = BTreeMap::new();
    for t in record.heaviest.iter().filter(|t| t.work > TURN_WORK_MAX) {
        let key = t
            .heaviest
            .as_ref()
            .and_then(|h| h.at.clone())
            .unwrap_or_else(|| t.started_by.clone());
        by_place.entry(key).or_default().push(t);
    }
    let full = record.heaviest.len() == ljos_engine::KEPT_TURNS
        && record.heaviest.iter().all(|t| t.work > TURN_WORK_MAX);
    let mut found: Vec<_> = by_place.into_iter().collect();
    // The worst first.
    found.sort_by_key(|(_, ts)| std::cmp::Reverse(ts.iter().map(|t| t.work).max()));
    for (_, ts) in found {
        let worst = ts
            .iter()
            .max_by_key(|t| t.work)
            .expect("a group has a turn");
        let many = match (ts.len(), full) {
            (1, _) => "A turn".to_string(),
            (n, false) => format!("{n} turns"),
            (n, true) => format!("At least {n} turns"),
        };
        let mut message = format!(
            "{many} of the UI thread ran more script than a turn may, and the window froze while \
             they ran: the heaviest, {}, ran {} polls of script, about {} ms in an optimised build",
            worst.started_by,
            worst.work,
            ms(worst.work)
        );
        if let Some(h) = &worst.heaviest {
            let of = h
                .object
                .as_deref()
                .map(|o| format!(" of {o}"))
                .unwrap_or_default();
            let at =
                h.at.as_deref()
                    .map(|a| format!(" at {a}"))
                    .unwrap_or_default();
            message.push_str(&format!(
                "; {} polls of it in handler {}{of}{at}",
                h.work, h.name
            ));
        }
        let place = worst.heaviest.as_ref().and_then(|h| h.at.clone());
        let f = cx.report("health-turn-over-budget", None, message);
        f.place = place;
        f.measured = Some(format!("{} polls > {TURN_WORK_MAX}", worst.work));
    }
    let budget = cx.engine.turn_budget().unwrap_or(DEFAULT_TURN_BUDGET);
    let slow: Vec<_> = record.slowest.iter().filter(|t| t.took > budget).collect();
    if let Some(worst) = slow.first() {
        let build = if cfg!(debug_assertions) {
            "a debug build, which runs script several times slower"
        } else {
            "an optimised build"
        };
        let message = format!(
            "{} turns of the UI thread took longer than the budget of {} ms on this machine's \
             clock, in {build}: the slowest, {}, took {} ms",
            slow.len(),
            budget.as_millis(),
            worst.started_by,
            worst.took.as_millis()
        );
        let place = worst.heaviest.as_ref().and_then(|h| h.at.clone());
        let f = cx.report("health-turn-slow", None, message);
        f.place = place;
        f.measured = Some(format!(
            "{} ms > {} ms",
            worst.took.as_millis(),
            budget.as_millis()
        ));
    }
}

/// Where a fault is: `file:line:column`, as a finding's place.
fn fault_place(r: &FaultReport) -> Option<String> {
    if let Fault::JsException(info) = &r.fault
        && let Some(file) = &info.file
    {
        return Some(format!("{file}:{}:{}", info.line, info.column));
    }
    let file = r.file.as_ref()?;
    Some(format!("{file}:{}:0", r.line))
}

/// `health-fault`: one finding a kind of fault and place, saying how
/// often it came.
fn faults(cx: &mut Cx) {
    let all = cx.engine.faults();
    let filed = cx.engine.faults_reported();
    let mut seen: Vec<(&'static str, Option<String>, usize, FaultReport)> = Vec::new();
    for r in all {
        let kind = fault_kind(&r.fault);
        let place = fault_place(&r);
        match seen
            .iter_mut()
            .find(|(k, p, _, _)| *k == kind && *p == place)
        {
            Some(s) => s.2 += 1,
            None => seen.push((kind, place, 1, r)),
        }
    }
    let kept = seen.iter().map(|s| s.2).sum::<usize>() as u64;
    for (kind, place, n, r) in seen {
        let who = r
            .origin_name
            .as_deref()
            .map(|o| format!(" in {o}"))
            .unwrap_or_default();
        let times = if n == 1 {
            String::new()
        } else {
            format!(" ({n} times)")
        };
        let message = format!("The application faulted{who}{times}: {}", r.fault);
        let f = cx.report("health-fault", None, message);
        f.place = place;
        f.measured = Some(kind.to_string());
    }
    if filed > kept {
        let message = format!(
            "{} more faults came before the {kept} the engine keeps",
            filed - kept
        );
        cx.report("health-fault", None, message).measured = Some(format!("{filed} faults"));
    }
}

/// `health-loader-failed`: by the place in the source that makes the
/// Loaders, as every delegate's Loader is the same fix.
fn loaders(cx: &mut Cx) {
    let e = cx.engine;
    let mut by_place: BTreeMap<Option<String>, (Vec<ObjectId>, usize, usize)> = BTreeMap::new();
    for obj in e.object_ids() {
        if !e.is_alive(obj)
            || !is_a::<Loader>(e, obj)
            || cx
                .options
                .within
                .is_some_and(|root| !belongs_to(e, obj, root))
        {
            continue;
        }
        let read = |name| e.untrack(|| e.read_by_name(obj, name));
        let status = read("status").to_number();
        let active = read("active").to_boolean();
        let item = read("item");
        let wanted = !read("source").to_js_string().as_str().is_empty()
            || !read("sourceComponent").is_nullish();
        let error = status == f64::from(ljos_quick::enums::loader_status::ERROR);
        // An asynchronous Loader has no item until its job is done.
        let loading = status == f64::from(ljos_quick::enums::loader_status::LOADING);
        let empty = active && wanted && !loading && item.is_nullish();
        let entry = by_place.entry(place_of(e, obj)).or_default();
        entry.2 += 1;
        if error || empty {
            entry.0.push(obj);
            if error {
                entry.1 += 1;
            }
        }
    }
    for (place, (failed, errors, all)) in by_place {
        let Some(&first) = failed.first() else {
            continue;
        };
        let what = if errors == failed.len() {
            "is at `Loader.Error`"
        } else if errors == 0 {
            "is active with a source and made no item"
        } else {
            "is at `Loader.Error` or made no item"
        };
        let message = if all == 1 {
            format!("A Loader {what}")
        } else {
            format!(
                "{} of the {all} Loaders made here: each {what}",
                failed.len()
            )
        };
        let f = cx.report("health-loader-failed", Some(first), message);
        if place.is_some() {
            f.place = place;
        }
        f.measured = Some(format!("{} of {all} failed", failed.len()));
    }
}

/// `health-heap-headroom` and `health-heap-growth`.
fn heap(cx: &mut Cx, ran: Option<&Ran>, not_checked: &mut Vec<(&'static str, &'static str)>) {
    let e = cx.engine;
    // The heap is the whole application's: the finding is about its root.
    let root = cx.scene.root();
    let free = |bytes: usize, limit: usize| {
        #[allow(clippy::cast_precision_loss)]
        let f = 1.0 - bytes as f64 / limit as f64;
        f
    };
    let samples: Vec<usize> = match ran {
        Some(r) if !r.heap.is_empty() => r.heap.clone(),
        // A collection frees what the application dropped, and its
        // destruction handlers then run: that is acting. Without it, the
        // heap as it is says enough when it has room.
        _ if !cx.options.act => {
            let as_is = heap_as_is(e);
            if let (Some(limit), Some(bytes)) = (e.script_memory_limit(), as_is)
                && free(bytes, limit) < HEAP_HEADROOM_MIN
            {
                not_checked.push(("health-heap-headroom", NOT_COLLECTED));
                Vec::new()
            } else {
                as_is.into_iter().collect()
            }
        }
        _ => heap_now(e).into_iter().collect(),
    };
    if let (Some(limit), Some(&most)) = (e.script_memory_limit(), samples.iter().max()) {
        let free = free(most, limit);
        if free < HEAP_HEADROOM_MIN {
            let message = format!(
                "The script heap holds {} MiB after a collection, of a cap of {} MiB: a little \
                 more and scripts fail with out of memory",
                mib(most),
                mib(limit)
            );
            cx.report("health-heap-headroom", root, message).measured = Some(format!(
                "{:.0} % free < {:.0} %",
                free * 100.0,
                HEAP_HEADROOM_MIN * 100.0
            ));
        }
    }
    let Some(ran) = ran else {
        return;
    };
    // Growth is judged over every window, or not at all.
    if ran.heap.len() <= HEAP_WINDOWS as usize {
        not_checked.push(("health-heap-growth", CUT_SHORT));
        return;
    }
    let grew = samples.windows(2).all(|w| w[1] > w[0]);
    let first = samples[0];
    let last = *samples.last().expect("samples");
    if grew && last - first >= HEAP_GROWTH_MIN_BYTES {
        let message = format!(
            "The script heap grew after every one of {} windows of {} ms the application ran, \
             from {} MiB to {} MiB after a collection: something is kept on every update",
            samples.len() - 1,
            HEAP_WINDOW_MS,
            mib(first),
            mib(last)
        );
        cx.report("health-heap-growth", root, message).measured =
            Some(format!("+{} MiB", mib(last - first)));
    }
}

/// `health-never-settles`.
fn settles(cx: &mut Cx, ran: &Ran, not_checked: &mut Vec<(&'static str, &'static str)>) {
    let root = cx.scene.root();
    if let Some((frames, at)) = ran.unsettled {
        let message = format!(
            "The scene asked for {frames} frames at one instant ({} ms on the application's \
             clock) without coming to rest: something changes in every frame",
            at.as_micros() / 1000
        );
        cx.report("health-never-settles", root, message).measured =
            Some(format!("{frames} frames at one time"));
        return;
    }
    let Some(frames) = ran.idle_frames else {
        not_checked.push(("health-never-settles", CUT_SHORT));
        return;
    };
    if frames >= u64::from(IDLE_FRAMES_MAX) {
        let message = format!(
            "With no animation running, the scene showed something new {frames} times in \
             {HEAP_WINDOW_MS} ms: something changes in every frame"
        );
        cx.report("health-never-settles", root, message).measured =
            Some(format!("{frames} frames >= {IDLE_FRAMES_MAX}"));
    }
}
