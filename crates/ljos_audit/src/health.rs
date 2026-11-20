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
use ljos_drive::{DriveError, DriveState, Driver, Host};
use ljos_engine::{DEFAULT_TURN_BUDGET, Engine, FaultReport, ObjectId, fault_kind};
use ljos_quick::{Loader, QuickScene};
use ljos_runtime::{Fault, Time};

use crate::Options;
use crate::cx::{Cx, is_a, place_of};
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

/// Counts the frames a settle makes.
#[derive(Default)]
struct Frames(u32);

impl Host for Frames {
    fn after_frame(&mut self, _engine: &Engine, _scene: &mut QuickScene) {
        self.0 += 1;
    }
}

/// What letting the application run showed.
#[derive(Default)]
struct Ran {
    /// The heap after a collection: before the first window and after
    /// each.
    heap: Vec<usize>,
    /// Frames made in the last window, and whether something animated at
    /// its end.
    frames: Option<(u32, bool)>,
    /// The settle that never came to rest, and when.
    unsettled: Option<(usize, Time)>,
}

/// The heap after a collection, if a script ever ran.
fn heap_now(engine: &Engine) -> Option<usize> {
    engine.collect_garbage();
    engine.script_memory().map(|m| m.bytes)
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
        let mut frames = Frames::default();
        let step = Driver::new(engine, scene, clock, &mut state)
            .hosted(&mut frames)
            .advance(u64::from(HEAP_WINDOW_MS));
        match step {
            Ok(()) => {}
            Err(DriveError::Unsettled { frames }) => {
                ran.unsettled = Some((frames, ljos_runtime::Clock::now(clock)));
                break;
            }
            // What else stops a run (a timer with no interval, a real
            // clock) is no finding of these rules.
            Err(_) => break,
        }
        ran.heap.extend(heap_now(engine));
        let animating = ljos_quick::running_animation_count(engine) > 0;
        ran.frames = Some((frames.0, animating));
    }
    ran
}

/// Runs the health checks `options` asks for once the rest of the audit is
/// done, so that the turns its acting checks set off are judged too. The
/// time the application's clock reached.
pub(crate) fn run(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    options: &Options,
) -> (Vec<Finding>, Time) {
    if !CODES.iter().any(|c| options.wants(c)) {
        return (Vec::new(), now);
    }
    // The windows run for any health code: what goes wrong on a timer or
    // a stream's news (a heavy turn, a fault, a Loader that fails) comes
    // only once time passes.
    let running = options.act;
    let ran = match (&options.clock.0, running) {
        (Some(clock), true) => Some(run_windows(engine, scene, clock, now)),
        _ => None,
    };
    let until = options
        .clock
        .0
        .as_ref()
        .filter(|_| ran.is_some())
        .map_or(now, |c| ljos_runtime::Clock::now(c).max(now));
    let mut cx = Cx::new(engine, scene, options);
    turns(&mut cx);
    faults(&mut cx);
    loaders(&mut cx);
    heap(&mut cx, ran.as_ref());
    if let Some(ran) = &ran {
        settles(&mut cx, ran);
    }
    (cx.out, until)
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
        if !e.is_alive(obj) || !is_a::<Loader>(e, obj) {
            continue;
        }
        let read = |name| e.untrack(|| e.read_by_name(obj, name));
        let status = read("status").to_number();
        let active = read("active").to_boolean();
        let item = read("item");
        let wanted = !read("source").to_js_string().as_str().is_empty()
            || !read("sourceComponent").is_nullish();
        let error = status == f64::from(ljos_quick::enums::loader_status::ERROR);
        let empty = active && wanted && item.is_nullish();
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
fn heap(cx: &mut Cx, ran: Option<&Ran>) {
    let e = cx.engine;
    // The heap is the whole application's: the finding is about its root.
    let root = cx.scene.root();
    let samples: Vec<usize> = match ran {
        Some(r) if !r.heap.is_empty() => r.heap.clone(),
        _ => heap_now(e).into_iter().collect(),
    };
    if let (Some(limit), Some(&most)) = (e.script_memory_limit(), samples.iter().max()) {
        #[allow(clippy::cast_precision_loss)]
        let free = 1.0 - most as f64 / limit as f64;
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
    if ran.is_none() || samples.len() < 3 {
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
fn settles(cx: &mut Cx, ran: &Ran) {
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
    if let Some((frames, false)) = ran.frames
        && frames >= IDLE_FRAMES_MAX
    {
        let message = format!(
            "With no animation running, the scene was framed {frames} times in {HEAP_WINDOW_MS} \
             ms: something changes in every frame"
        );
        cx.report("health-never-settles", root, message).measured =
            Some(format!("{frames} frames >= {IDLE_FRAMES_MAX}"));
    }
}
