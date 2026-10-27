//! The `top` overlay: "which component ate the frame" (ARCHITECTURE.md
//! §3.4, §9.1, §19.10). This is the model and the text; painting it is the
//! host's, so every host says the same.
//!
//! How it updates: a host gives [`Top::record`] the [`FrameReport`] of each
//! frame it ran anyway, and paints [`Top::view`] in that same frame. `Top`
//! never asks for a frame and holds no timer. While the application is
//! idle no frame runs, so nothing is recorded and the overlay goes on
//! showing the last frame's figures; that is the truth about an idle
//! application. While hidden, `record` returns at once and no totals are
//! kept, so the overlay costs one branch per frame.

use std::collections::HashMap;
use std::time::Duration;

use ljos_engine::{APPLICATION_PROC, Engine};
use ljos_runtime::scheduler::TOP_OWNERS;
use ljos_runtime::{FrameReport, Lane, ProcId};

/// The owner the inspector's own work is charged to, so that looking at an
/// application is not mistaken for the application's work. No allocator
/// hands this id out: indices grow from 0.
pub const INSPECTOR_PROC: ProcId = ProcId::new(u32::MAX, 0);

/// The lanes as tools and the overlay name them, in priority order.
pub const LANE_NAMES: [&str; Lane::COUNT] = [
    "input", "critical", "visible", "prefetch", "deferred", "idle",
];

/// How many owners the totals keep. An application has a handful of
/// boundaries; past this the smallest is dropped so a long session with
/// many short-lived boundaries cannot grow the table without end.
const MAX_OWNERS: usize = 64;

/// Totals over the frames recorded so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Frames the host ran.
    pub frames: u64,
    /// Frames whose deadline had passed when their work was done.
    pub overruns: u64,
    pub last_frame: Duration,
    pub max_frame: Duration,
    pub total_frame: Duration,
    /// Time inside jobs and the inbox, per lane (index with `Lane::index`).
    pub per_lane: [Duration; Lane::COUNT],
    /// Jobs still queued per lane when the last frame ended.
    pub pending: [usize; Lane::COUNT],
    pub jobs_polled: u64,
    pub jobs_done: u64,
    pub jobs_yielded: u64,
    pub jobs_failed: u64,
    /// Messages taken from the inbox, and the time that took.
    pub drained: u64,
    pub drain_time: Duration,
    owners: HashMap<ProcId, Duration>,
}

impl FrameStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a frame the host ran.
    pub fn record(&mut self, report: &FrameReport) {
        self.frames += 1;
        self.overruns += u64::from(report.overrun);
        self.last_frame = report.elapsed;
        self.max_frame = self.max_frame.max(report.elapsed);
        self.total_frame += report.elapsed;
        self.pending = report.pending;
        self.add_work(report);
    }

    /// Adds work done outside a frame of the host (the inspector answering
    /// a request): lanes, owners and jobs, but no frame.
    pub fn record_work(&mut self, report: &FrameReport) {
        self.add_work(report);
    }

    fn add_work(&mut self, report: &FrameReport) {
        for (total, t) in self.per_lane.iter_mut().zip(report.per_lane) {
            *total += t;
        }
        self.jobs_polled += report.jobs_polled as u64;
        self.jobs_done += report.jobs_done as u64;
        self.jobs_yielded += report.jobs_yielded as u64;
        self.jobs_failed += report.jobs_failed as u64;
        self.drained += report.drained as u64;
        self.drain_time += report.drain_time;
        for (owner, t) in &report.top_owners {
            *self.owners.entry(*owner).or_default() += *t;
        }
        while self.owners.len() > MAX_OWNERS {
            let least = self
                .owners
                .iter()
                .min_by_key(|(owner, t)| (**t, **owner))
                .map(|(owner, _)| *owner);
            match least {
                Some(owner) => self.owners.remove(&owner),
                None => break,
            };
        }
    }

    /// The `n` owners charged the most time, largest first. Ties go by id
    /// so the order does not change between two calls.
    pub fn owners(&self, n: usize) -> Vec<(ProcId, Duration)> {
        let mut all: Vec<(ProcId, Duration)> = self.owners.iter().map(|(o, t)| (*o, *t)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        all.truncate(n);
        all
    }
}

/// Adds the lanes, owners and jobs of `work` to `report`: what a tool did
/// in a frame, next to what the application did in it. The frame's time
/// and its queues stay the frame's.
pub fn add_work(report: &mut FrameReport, work: &FrameReport) {
    for (total, t) in report.per_lane.iter_mut().zip(work.per_lane) {
        *total += t;
    }
    report.jobs_polled += work.jobs_polled;
    report.jobs_done += work.jobs_done;
    report.jobs_yielded += work.jobs_yielded;
    report.jobs_failed += work.jobs_failed;
    report.drained += work.drained;
    report.drain_time += work.drain_time;
    // Largest first, as the scheduler leaves them; ties by id so the order
    // does not change between two frames.
    let order = |a: &(ProcId, Duration), b: &(ProcId, Duration)| b.1.cmp(&a.1).then(a.0.cmp(&b.0));
    for (owner, t) in &work.top_owners {
        let owners = &mut report.top_owners;
        if let Some((_, total)) = owners.iter_mut().find(|(o, _)| o == owner) {
            *total += *t;
        } else if owners.len() < TOP_OWNERS {
            owners.push((*owner, *t));
        } else if let Some(least) = owners.iter_mut().max_by(|a, b| order(a, b))
            && order(&(*owner, *t), least).is_lt()
        {
            // The report keeps the largest few, as the scheduler does, in
            // place: one more would move them to the heap.
            *least = (*owner, *t);
        }
    }
    report.top_owners.sort_by(order);
}

/// What to call an owner: `application`, `inspector`, the boundary it
/// stands for (`ErrorBoundary "editor"`), or `proc 3.1` for one that is
/// gone.
pub fn owner_name(engine: &Engine, proc: ProcId) -> String {
    if proc == APPLICATION_PROC {
        return "application".to_owned();
    }
    if proc == INSPECTOR_PROC {
        return "inspector".to_owned();
    }
    let gone = || format!("proc {}.{}", proc.index(), proc.generation());
    let Some(obj) = engine.proc_owner(proc) else {
        return gone();
    };
    let Some(report) = engine.inspect_object(obj) else {
        return gone();
    };
    let ty = report.types.first().cloned().unwrap_or_default();
    let object_name = engine
        .untrack(|| engine.read_by_name(obj, "objectName"))
        .to_js_string()
        .to_string();
    if !object_name.is_empty() && object_name != "undefined" {
        format!("{ty} \"{object_name}\"")
    } else if let Some(id) = report.id {
        format!("{ty}#{id}")
    } else {
        ty
    }
}

/// One row of the overlay: a lane or an owner, the time it took in the
/// last frame and its share of that frame's work.
#[derive(Clone, Debug, PartialEq)]
pub struct TopRow {
    pub name: String,
    pub time: Duration,
    /// 0 to 1: the row's part of the time the frame spent working.
    pub share: f64,
    /// Jobs still queued (lanes only).
    pub pending: usize,
}

/// What the overlay shows.
#[derive(Clone, Debug, PartialEq)]
pub struct TopView {
    /// `frame 412  3.20 ms  (max 9.10 ms, 2 over budget)`.
    pub title: String,
    /// Every lane, in priority order.
    pub lanes: Vec<TopRow>,
    /// The owners of the last frame, largest first.
    pub owners: Vec<TopRow>,
}

impl TopView {
    /// The overlay as lines of text of equal structure, for a host that
    /// paints with a monospace font and for tests.
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![self.title.clone()];
        for row in &self.lanes {
            let pending = if row.pending == 0 {
                String::new()
            } else {
                format!("  {} queued", row.pending)
            };
            out.push(format!(
                "{:<9}{:>10}{:>5.0}%{pending}",
                row.name,
                millis(row.time),
                row.share * 100.0
            ));
        }
        if self.owners.is_empty() {
            out.push("no owner was charged in this frame".to_owned());
        }
        for row in &self.owners {
            out.push(format!(
                "{:>10}{:>5.0}%  {}",
                millis(row.time),
                row.share * 100.0,
                row.name
            ));
        }
        out
    }
}

fn millis(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

/// The overlay's state. See the module docs for how it updates.
#[derive(Debug, Default)]
pub struct Top {
    visible: bool,
    last: FrameReport,
    stats: FrameStats,
    /// The owners of the last frame, named when it was recorded: naming
    /// reads the engine, and a paint must not.
    owners: Vec<TopRow>,
}

impl Top {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Shows or hides the overlay. Hiding forgets what was recorded, so a
    /// hidden overlay holds nothing.
    pub fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            *self = Top {
                visible,
                ..Top::default()
            };
        }
    }

    pub fn toggle(&mut self) {
        self.set_visible(!self.visible);
    }

    /// The totals since the overlay was shown.
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    /// Records a frame the host ran. Does nothing while hidden.
    pub fn record(&mut self, engine: &Engine, report: &FrameReport) {
        if !self.visible {
            return;
        }
        self.stats.record(report);
        let work = work_of(report);
        self.owners = report
            .top_owners
            .iter()
            .map(|(owner, t)| TopRow {
                name: owner_name(engine, *owner),
                time: *t,
                share: share(*t, work),
                pending: 0,
            })
            .collect();
        self.last = report.clone();
    }

    /// What to paint; `None` while hidden.
    pub fn view(&self) -> Option<TopView> {
        if !self.visible {
            return None;
        }
        let s = &self.stats;
        let title = if s.frames == 0 {
            "no frame yet".to_owned()
        } else {
            let over = match s.overruns {
                0 => String::new(),
                n => format!(", {n} over budget"),
            };
            format!(
                "frame {}  {}  (max {}{over})",
                s.frames,
                millis(s.last_frame),
                millis(s.max_frame)
            )
        };
        let work = work_of(&self.last);
        let lanes = Lane::ALL
            .iter()
            .map(|lane| {
                let t = self.last.lane_time(*lane);
                TopRow {
                    name: LANE_NAMES[lane.index()].to_owned(),
                    time: t,
                    share: share(t, work),
                    pending: self.last.pending[lane.index()],
                }
            })
            .collect();
        Some(TopView {
            title,
            lanes,
            owners: self.owners.clone(),
        })
    }
}

/// The time a frame spent working: its lanes, which include the inbox.
fn work_of(report: &FrameReport) -> Duration {
    report.per_lane.iter().sum()
}

fn share(t: Duration, of: Duration) -> f64 {
    if of.is_zero() {
        0.0
    } else {
        t.as_secs_f64() / of.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(visible_us: u64, owners: &[(ProcId, u64)]) -> FrameReport {
        let mut r = FrameReport::default();
        r.per_lane[Lane::Visible.index()] = Duration::from_micros(visible_us);
        r.elapsed = Duration::from_micros(visible_us + 100);
        r.jobs_polled = 3;
        r.jobs_done = 2;
        r.jobs_yielded = 1;
        r.pending[Lane::Deferred.index()] = 4;
        for (o, t) in owners {
            r.top_owners.push((*o, Duration::from_micros(*t)));
        }
        r
    }

    #[test]
    fn a_tools_work_is_added_to_the_frame_it_ran_in() {
        let mut frame = report(900, &[(APPLICATION_PROC, 900)]);
        let mut looked = FrameReport::default();
        looked.per_lane[Lane::Deferred.index()] = Duration::from_micros(1500);
        looked
            .top_owners
            .push((INSPECTOR_PROC, Duration::from_micros(1500)));
        looked
            .top_owners
            .push((APPLICATION_PROC, Duration::from_micros(50)));
        looked.jobs_done = 1;
        // What the frame itself took is not the tool's to say.
        looked.elapsed = Duration::from_secs(1);
        add_work(&mut frame, &looked);
        assert_eq!(frame.lane_time(Lane::Visible), Duration::from_micros(900));
        assert_eq!(frame.lane_time(Lane::Deferred), Duration::from_micros(1500));
        assert_eq!(frame.jobs_done, 3);
        assert_eq!(frame.elapsed, Duration::from_micros(1000));
        assert_eq!(
            frame.top_owners.to_vec(),
            [
                (INSPECTOR_PROC, Duration::from_micros(1500)),
                (APPLICATION_PROC, Duration::from_micros(950)),
            ]
        );
    }

    #[test]
    fn a_tools_work_keeps_the_largest_owners_only() {
        let owners: Vec<(ProcId, u64)> = (0..TOP_OWNERS as u32)
            .map(|i| (ProcId::new(i + 10, 0), 100 + u64::from(i)))
            .collect();
        let mut frame = report(900, &owners);
        let mut looked = FrameReport::default();
        looked
            .top_owners
            .push((INSPECTOR_PROC, Duration::from_micros(1500)));
        looked
            .top_owners
            .push((ProcId::new(99, 0), Duration::from_micros(1)));
        add_work(&mut frame, &looked);
        assert_eq!(frame.top_owners.len(), TOP_OWNERS);
        assert!(!frame.top_owners.spilled());
        assert_eq!(frame.top_owners[0].0, INSPECTOR_PROC);
        // The smallest of the frame's own gave way.
        assert!(
            !frame
                .top_owners
                .iter()
                .any(|(o, _)| *o == ProcId::new(10, 0))
        );
    }

    #[test]
    fn a_hidden_overlay_records_nothing() {
        let engine = Engine::new();
        let mut top = Top::new();
        top.record(&engine, &report(900, &[(APPLICATION_PROC, 900)]));
        assert_eq!(top.stats(), &FrameStats::default());
        assert_eq!(top.view(), None);
    }

    #[test]
    fn it_shows_the_last_frame_by_lane_and_by_owner() {
        let engine = Engine::new();
        let mut top = Top::new();
        top.set_visible(true);
        top.record(
            &engine,
            &report(1000, &[(APPLICATION_PROC, 750), (INSPECTOR_PROC, 250)]),
        );
        let view = top.view().unwrap();
        assert_eq!(view.lanes.len(), Lane::COUNT);
        let visible = &view.lanes[Lane::Visible.index()];
        assert_eq!(visible.name, "visible");
        assert_eq!(visible.time, Duration::from_micros(1000));
        assert!((visible.share - 1.0).abs() < 1e-9);
        assert_eq!(view.lanes[Lane::Deferred.index()].pending, 4);
        assert_eq!(view.owners[0].name, "application");
        assert!((view.owners[0].share - 0.75).abs() < 1e-9);
        assert_eq!(view.owners[1].name, "inspector");
        let lines = view.lines();
        assert!(lines[0].starts_with("frame 1  1.10 ms"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("4 queued")), "{lines:?}");
        assert!(lines.last().unwrap().ends_with("inspector"), "{lines:?}");
    }

    #[test]
    fn hiding_forgets_and_an_idle_overlay_keeps_the_last_frame() {
        let engine = Engine::new();
        let mut top = Top::new();
        top.set_visible(true);
        top.record(&engine, &report(500, &[]));
        let first = top.view();
        // No frame ran: the same figures, and nothing asked for one.
        assert_eq!(top.view(), first);
        top.toggle();
        assert_eq!(top.view(), None);
        top.toggle();
        assert_eq!(top.stats().frames, 0);
        assert_eq!(top.view().unwrap().title, "no frame yet");
    }

    #[test]
    fn totals_add_up_and_owners_are_ordered_and_bounded() {
        let mut stats = FrameStats::new();
        let a = ProcId::new(1, 0);
        let b = ProcId::new(2, 0);
        stats.record(&report(100, &[(a, 60), (b, 40)]));
        stats.record(&report(300, &[(b, 300)]));
        stats.record_work(&report(50, &[(INSPECTOR_PROC, 50)]));
        assert_eq!(stats.frames, 2);
        assert_eq!(stats.jobs_polled, 9);
        assert_eq!(stats.max_frame, Duration::from_micros(400));
        assert_eq!(
            stats.per_lane[Lane::Visible.index()],
            Duration::from_micros(450)
        );
        assert_eq!(
            stats.owners(2),
            vec![
                (b, Duration::from_micros(340)),
                (a, Duration::from_micros(60))
            ]
        );
        for i in 0..200 {
            stats.record_work(&report(1, &[(ProcId::new(100 + i, 0), 1)]));
        }
        assert!(stats.owners(usize::MAX).len() <= MAX_OWNERS);
        assert_eq!(stats.owners(1)[0].0, b);
    }

    #[test]
    fn an_owner_that_is_gone_is_named_by_its_id() {
        let engine = Engine::new();
        assert_eq!(owner_name(&engine, ProcId::new(7, 2)), "proc 7.2");
    }
}
