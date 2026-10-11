//! Sliced incubation (ARCHITECTURE.md §6.3): what a frame gives creation
//! jobs, who is charged for it, and that nothing is scheduled afterwards.
//!
//! Time here is the engine's clock, and that clock is one on which every
//! look costs a fixed amount: a job that checks its slice once per step
//! spends that much per step, so the tests count steps and never measure
//! the machine. A step is one object applied, one binding run, one native
//! completion hook or one object's `Component.onCompleted`.
//!
//! What the clock cannot show: the objects of a component are allocated
//! in one step however many they are (`CreationJob`, phase `Allocate`),
//! and a step is never interrupted, so one long binding or handler is as
//! long as it is.

mod common;

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use common::*;
use ljos_engine::{APPLICATION_PROC, CreationJob, Engine, ObjectId, Value};
use ljos_quick::{INCUBATION_BUDGET, MIN_INCUBATION_BUDGET, QuickScene, incubation_budget};
use ljos_runtime::{Clock, FrameReport, JobPoll, Lane, ProcId, Time};

/// What one look at the clock costs.
const LOOK: Duration = Duration::from_micros(100);

#[derive(Debug)]
struct BusyClock {
    at: AtomicU64,
    look: u64,
}

impl BusyClock {
    /// The time without spending any.
    fn peek(&self) -> Duration {
        Duration::from_micros(self.at.load(Ordering::Relaxed))
    }
}

impl Clock for BusyClock {
    fn now(&self) -> Time {
        Time::from_micros(self.at.fetch_add(self.look, Ordering::Relaxed))
    }
}

/// An engine on a clock a look at which costs `look`. Bindings run as
/// part of the creation, as in Qt; left lazy they run when first read and
/// are no part of the job.
fn busy(look: Duration) -> (Engine, Rc<BusyClock>) {
    let e = engine();
    e.set_eager_bindings(true);
    let clock = Rc::new(BusyClock {
        at: AtomicU64::new(0),
        look: look.as_micros() as u64,
    });
    e.set_clock(clock.clone());
    (e, clock)
}

fn busy_engine() -> Engine {
    busy(LOOK).0
}

/// A component of `n` items with a binding each, for `start(parent)` to
/// make with `incubateObject` and for an asynchronous Loader.
fn source(n: usize) -> String {
    let kids: String = (0..n)
        .map(|i| format!("            Item {{ objectName: \"k{i}\"; width: big.width + {i} }}\n"))
        .collect();
    format!(
        r#"
import QtQuick
import Ljos
Item {{
    id: root
    objectName: "root"
    width: 200; height: 200
    property var inc: null
    property var c: comp
    Component {{
        id: comp
        Item {{
            id: big
            objectName: "big"
            width: 10
{kids}        }}
    }}
    ErrorBoundary {{
        objectName: "boundary"
        Item {{ objectName: "inside" }}
    }}
    Loader {{ id: loader; objectName: "loader"; asynchronous: true; active: false; sourceComponent: comp }}
    function start(parent) {{ inc = comp.incubateObject(parent) }}
    function load() {{ loader.active = true }}
}}
"#
    )
}

/// Runs frames until no incubation is left and returns what each frame's
/// jobs reported.
fn run(s: &mut QuickScene, e: &Engine) -> Vec<FrameReport> {
    let mut reports = Vec::new();
    loop {
        let out = frame(s, e);
        let Some(report) = s.take_incubation_report() else {
            assert_eq!(
                out.next_wake, None,
                "a frame without jobs asks for no other"
            );
            return reports;
        };
        reports.push(report);
        assert!(reports.len() < 10_000, "the incubation never ends");
    }
}

/// No frame gave its jobs more than `budget`, but for the step that
/// found the budget spent; `step` is that step on a clock a look at which
/// costs `look`.
fn assert_within(reports: &[FrameReport], budget: Duration, step: Duration, look: Duration) {
    for (i, r) in reports.iter().enumerate() {
        let polled = r.lane_time(Lane::Visible);
        assert!(
            polled <= budget + step,
            "frame {i} gave its jobs {polled:?} of {budget:?}"
        );
        // The frame is that and the scheduler's own looks, which this
        // clock charges for: one a lane and four a poll.
        let looks = (Lane::COUNT + 4 * r.jobs_polled) as u32;
        assert!(
            r.elapsed <= budget + step + look * looks,
            "frame {i} spent {:?} of {budget:?} with {} polls",
            r.elapsed,
            r.jobs_polled
        );
        assert!(polled <= r.elapsed, "frame {i}");
        // Every poll was charged to somebody, and nothing else was.
        let charged: Duration = r.top_owners.iter().map(|(_, d)| *d).sum();
        assert_eq!(charged, polled, "frame {i}");
    }
}

/// The most a frame's jobs can go beyond its budget on a clock a look at
/// which costs `look`: the longest step of a creation job (the job is
/// asked to stop between steps, never interrupted in one), the look that
/// finds the slice spent and the scheduler's look after the poll.
fn one_step_at(look: Duration) -> Duration {
    let (e, clock) = busy(look);
    let root = load(&e, &source(8));
    let comp = get(&e, root, "c").as_object().unwrap();
    let (mut job, handle) = CreationJob::for_component(
        &e,
        comp,
        Some(root),
        vec![],
        Lane::Visible,
        APPLICATION_PROC,
    );
    let mut longest = Duration::ZERO;
    let mut steps = 0;
    loop {
        let before = clock.peek();
        // Asked to stop after every step, without a look at the clock.
        let poll = job.step(&|| true);
        longest = longest.max(clock.peek() - before);
        steps += 1;
        if !matches!(poll, JobPoll::Yield) {
            break;
        }
    }
    // Nine objects: each applied and completed, eight bindings run.
    assert!(steps >= 9 + 8 + 9, "{steps} steps");
    assert!(handle.result().unwrap().is_ok());
    longest + look * 2
}

fn one_step() -> Duration {
    one_step_at(LOOK)
}

/// Starts `incubateObject` under `parent`.
fn start(e: &Engine, root: ObjectId, parent: ObjectId) {
    e.invoke_by_name(root, "start", &[Value::Object(parent)])
        .unwrap();
}

fn owner(reports: &[FrameReport]) -> ProcId {
    let first = reports[0].top_owners[0].0;
    for r in reports {
        assert_eq!(r.top_owners.len(), 1);
        assert_eq!(r.top_owners[0].0, first);
    }
    first
}

fn made(e: &Engine, root: ObjectId) -> ObjectId {
    let inc = get(e, root, "inc").as_object().expect("an incubator");
    assert_eq!(num(e, inc, "status"), 1.0);
    get(e, inc, "object").as_object().expect("the object")
}

const LARGE: usize = 600;

#[test]
fn no_slice_of_a_large_component_runs_longer_than_its_budget() {
    let step = one_step();
    let e = busy_engine();
    let root = load(&e, &source(LARGE));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    assert_eq!(s.incubation_budget(), INCUBATION_BUDGET);
    assert!(step * 4 < INCUBATION_BUDGET, "a step is {step:?}");

    start(&e, root, root);
    assert!(s.needs_frame(), "queueing a job asks for a frame");
    let reports = run(&mut s, &e);
    assert_within(&reports, INCUBATION_BUDGET, step, LOOK);
    // Applying the objects, running their bindings and completing them
    // are all cut into steps: a frame has no room for more than
    // `per_frame` of them, so the three passes over the items need at
    // least this many frames.
    let per_frame = ((INCUBATION_BUDGET + step).as_micros() / LOOK.as_micros()) as usize;
    assert!(
        reports.len() * per_frame >= 3 * LARGE,
        "{} frames of {per_frame} steps",
        reports.len()
    );
    // Every frame but the last used what it was given.
    for r in &reports[..reports.len() - 1] {
        assert!(r.elapsed + step >= INCUBATION_BUDGET, "{:?}", r.elapsed);
        assert_eq!(r.jobs_done, 0);
        assert_eq!(r.pending_jobs(), 1);
    }
    let last = reports.last().unwrap();
    assert_eq!(last.jobs_done, 1);
    assert_eq!(last.pending_jobs(), 0);
    assert_eq!(owner(&reports), APPLICATION_PROC);

    let big = made(&e, root);
    assert_eq!(e.children(big).len(), LARGE);
    assert_eq!(num(&e, find(&e, big, "k7"), "width"), 17.0);
    assert!(
        s.node_of(big).is_some(),
        "shown in the frame that finished it"
    );
    // 0 % CPU when idle: nothing is queued, nothing asks for a frame.
    assert!(!e.has_incubations());
    assert!(!e.has_half_made());
    assert!(!s.needs_frame());
    let out = frame(&mut s, &e);
    assert_eq!(out.next_wake, None);
    assert!(!out.changed);
    assert!(s.take_incubation_report().is_none());
    assert!(e.next_timer_deadline().is_none());
    assert!(e.take_warnings().is_empty());
}

#[test]
fn the_budget_is_scaled_down_by_the_refresh_rate() {
    let step = one_step();
    let mut frames = Vec::new();
    for hz in [60.0, 120.0, 240.0] {
        let e = busy_engine();
        let root = load(&e, &source(LARGE));
        let mut s = scene(&e, root, 200.0, 200.0);
        frame(&mut s, &e);
        s.set_refresh_rate(hz);
        let budget = incubation_budget(hz);
        assert_eq!(s.incubation_budget(), budget);
        start(&e, root, root);
        let reports = run(&mut s, &e);
        assert_within(&reports, budget, step, LOOK);
        assert_eq!(e.children(made(&e, root)).len(), LARGE);
        frames.push(reports.len());
    }
    assert_eq!(incubation_budget(120.0), INCUBATION_BUDGET / 2);
    assert_eq!(incubation_budget(240.0), INCUBATION_BUDGET / 4);
    // Half the time a frame, about twice the frames.
    assert!(frames[1] * 10 > frames[0] * 17, "{frames:?}");
    assert!(frames[2] * 10 > frames[1] * 17, "{frames:?}");
}

#[test]
fn the_time_is_charged_to_the_boundary_the_object_is_made_in() {
    let e = busy_engine();
    let root = load(&e, &source(200));
    let boundary = find(&e, root, "boundary");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    start(&e, root, find(&e, root, "inside"));
    let reports = run(&mut s, &e);
    assert!(reports.len() > 3);
    let charged = owner(&reports);
    assert_ne!(charged, APPLICATION_PROC);
    assert_eq!(charged, e.boundary_proc(boundary));
    assert_eq!(e.proc_owner(charged), Some(boundary));
    assert_eq!(e.owner(made(&e, root)), Some(find(&e, root, "inside")));
}

#[test]
fn a_loader_is_charged_for_what_it_loads() {
    let step = one_step();
    let e = busy_engine();
    let root = load(&e, &source(200));
    let loader = find(&e, root, "loader");
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    e.invoke_by_name(root, "load", &[]).unwrap();
    let reports = run(&mut s, &e);
    assert_within(&reports, INCUBATION_BUDGET, step, LOOK);
    assert!(reports.len() > 3);
    assert_eq!(owner(&reports), e.boundary_proc(loader));
    assert_eq!(num(&e, loader, "status"), 1.0);
    assert!(!s.needs_frame());
}

#[test]
fn two_incubations_share_one_frames_budget() {
    let step = one_step();
    let e = busy_engine();
    let root = load(&e, &source(300));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    start(&e, root, find(&e, root, "inside"));
    e.invoke_by_name(root, "load", &[]).unwrap();
    let reports = run(&mut s, &e);
    // Two jobs, and still no frame gives more than the one budget.
    assert_within(&reports, INCUBATION_BUDGET, step, LOOK);
    assert!(reports.iter().any(|r| r.top_owners.len() == 2));
    assert_eq!(num(&e, find(&e, root, "loader"), "status"), 1.0);
    made(&e, root);
    assert!(!e.has_incubations());
    assert!(!s.needs_frame());
}

#[test]
fn force_completion_takes_the_work_out_of_the_frames() {
    let e = busy_engine();
    let root = load(&e, &source(LARGE));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    start(&e, root, root);
    frame(&mut s, &e);
    let first = s.take_incubation_report().unwrap();
    assert_eq!(first.jobs_done, 0);
    let inc = get(&e, root, "inc").as_object().unwrap();
    assert_eq!(num(&e, inc, "status"), 2.0);
    e.invoke_by_name(inc, "forceCompletion", &[]).unwrap();
    let big = made(&e, root);
    assert_eq!(e.children(big).len(), LARGE);
    assert!(s.node_of(big).is_none(), "half made, it was left out");
    // The queued job finds nothing left to do, and then nothing is queued.
    let reports = run(&mut s, &e);
    assert!(reports.len() <= 1, "{} frames", reports.len());
    assert!(
        s.node_of(big).is_some(),
        "shown though no frame finished it"
    );
    assert!(!e.has_incubations());
    assert!(!s.needs_frame());
}

#[test]
fn a_rate_no_display_has_still_gets_the_objects_made() {
    // A finer clock: the floor is shorter than three looks at the other.
    let look = Duration::from_micros(5);
    let step = one_step_at(look);
    let (e, _clock) = busy(look);
    let root = load(&e, &source(400));
    let mut s = scene(&e, root, 200.0, 200.0);
    frame(&mut s, &e);
    s.set_refresh_rate(1.0e9);
    assert_eq!(s.incubation_budget(), MIN_INCUBATION_BUDGET);
    start(&e, root, root);
    let reports = run(&mut s, &e);
    assert_within(&reports, MIN_INCUBATION_BUDGET, step, look);
    assert!(reports.len() > 3, "{} frames", reports.len());
    assert_eq!(e.children(made(&e, root)).len(), 400);
    assert!(!s.needs_frame());
}
