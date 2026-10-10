//! Lane scheduler: runs queued [`Job`]s inside a frame budget (ARCHITECTURE.md 9.1).
//!
//! Lanes are served in priority order. `Input` and `Critical` get one poll per
//! queued job even if the frame is already over budget; every other lane stops
//! as soon as the budget is spent. `Prefetch` and `Deferred` rotate across
//! owners so one busy component cannot starve the rest; the other lanes are
//! FIFO. Jobs run on the calling (UI) thread and are cooperative: a job that
//! ignores its [`crate::SliceBudget`] is a bug the supervisor cannot preempt.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use smallvec::SmallVec;

use crate::budget::{FrameBudget, SliceBudget};
use crate::fault::Fault;
use crate::inbox::DrainReport;
use crate::job::{Job, JobCx, JobPoll};
use crate::lane::{Lane, ProcId};
use crate::time::Clock;

/// How many owners `FrameReport::top_owners` keeps.
pub const TOP_OWNERS: usize = 8;

/// What a frame did; the dev overlay's "which component ate the frame".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameReport {
    /// Time spent inside `Job::poll`, per lane (index with `Lane::index`).
    pub per_lane: [Duration; Lane::COUNT],
    /// Owners ordered by time charged this frame, largest first.
    pub top_owners: SmallVec<[(ProcId, Duration); TOP_OWNERS]>,
    /// Inbox drain time and count. Filled by [`FrameReport::record_drain`].
    pub drain_time: Duration,
    pub drained: usize,
    pub jobs_polled: usize,
    pub jobs_done: usize,
    pub jobs_yielded: usize,
    pub jobs_failed: usize,
    /// Failures for the supervisor, in the order they happened.
    pub faults: Vec<(ProcId, Fault)>,
    /// Jobs still queued per lane when the frame ended.
    pub pending: [usize; Lane::COUNT],
    /// Wall time of `run_frame`.
    pub elapsed: Duration,
    /// The frame deadline had passed when lane work finished.
    pub overrun: bool,
}

impl FrameReport {
    /// Adds an inbox drain (the Critical-lane step 1 of the frame pipeline).
    pub fn record_drain(&mut self, drain: &DrainReport) {
        self.drain_time += drain.elapsed;
        self.drained += drain.drained;
        self.per_lane[Lane::Critical.index()] += drain.elapsed;
    }

    pub fn pending_jobs(&self) -> usize {
        self.pending.iter().sum()
    }

    pub fn lane_time(&self, lane: Lane) -> Duration {
        self.per_lane[lane.index()]
    }
}

type JobBox<C> = Box<dyn Job<C>>;

struct LaneQueue<C> {
    round_robin: bool,
    fifo: VecDeque<Box<dyn Job<C>>>,
    owners: Vec<(ProcId, VecDeque<JobBox<C>>)>,
    cursor: usize,
    len: usize,
}

impl<C> LaneQueue<C> {
    fn new(round_robin: bool) -> Self {
        Self {
            round_robin,
            fifo: VecDeque::new(),
            owners: Vec::new(),
            cursor: 0,
            len: 0,
        }
    }

    fn push(&mut self, job: Box<dyn Job<C>>) {
        self.len += 1;
        if !self.round_robin {
            self.fifo.push_back(job);
            return;
        }
        let owner = job.owner();
        match self.owners.iter_mut().find(|(o, _)| *o == owner) {
            Some((_, q)) => q.push_back(job),
            None => self.owners.push((owner, VecDeque::from([job]))),
        }
    }

    fn pop(&mut self) -> Option<Box<dyn Job<C>>> {
        if !self.round_robin {
            let job = self.fifo.pop_front()?;
            self.len -= 1;
            return Some(job);
        }
        if self.owners.is_empty() {
            return None;
        }
        self.cursor %= self.owners.len();
        let (_, q) = &mut self.owners[self.cursor];
        let job = q.pop_front();
        if q.is_empty() {
            // The next owner slides into `cursor`.
            self.owners.remove(self.cursor);
        } else {
            self.cursor += 1;
        }
        self.len -= 1;
        job
    }

    fn cancel_owner(&mut self, owner: ProcId) -> usize {
        let before = self.len;
        if self.round_robin {
            if let Some(i) = self.owners.iter().position(|(o, _)| *o == owner) {
                self.len -= self.owners[i].1.len();
                self.owners.remove(i);
                if i < self.cursor {
                    self.cursor -= 1;
                }
            }
        } else {
            self.fifo.retain(|j| j.owner() != owner);
            self.len = self.fifo.len();
        }
        before - self.len
    }
}

/// Owns every queued job. `C` is the engine context handed to jobs.
pub struct Scheduler<C> {
    lanes: [LaneQueue<C>; Lane::COUNT],
}

impl<C> std::fmt::Debug for Scheduler<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lens: Vec<_> = Lane::ALL.iter().map(|l| self.lane_len(*l)).collect();
        f.debug_struct("Scheduler").field("queued", &lens).finish()
    }
}

impl<C> Default for Scheduler<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> Scheduler<C> {
    pub fn new() -> Self {
        Self {
            lanes: Lane::ALL.map(|l| LaneQueue::new(l.round_robin_by_owner())),
        }
    }

    pub fn spawn(&mut self, job: Box<dyn Job<C>>) {
        self.lanes[job.lane().index()].push(job);
    }

    /// Drops every queued job of `owner` (a component was destroyed). A job
    /// currently being polled is not affected; it ends with its poll.
    pub fn cancel_owner(&mut self, owner: ProcId) -> usize {
        self.lanes.iter_mut().map(|l| l.cancel_owner(owner)).sum()
    }

    pub fn lane_len(&self, lane: Lane) -> usize {
        self.lanes[lane.index()].len
    }

    pub fn len(&self) -> usize {
        self.lanes.iter().map(|l| l.len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Runs queued jobs under `budget`. See the module docs for the rules.
    pub fn run_frame(
        &mut self,
        ctx: &mut C,
        budget: FrameBudget,
        clock: &dyn Clock,
    ) -> FrameReport {
        let mut report = FrameReport::default();
        let run_start = clock.now();
        let mut owner_time: HashMap<ProcId, Duration> = HashMap::new();

        for lane in Lane::ALL {
            let i = lane.index();
            let mut guaranteed = if lane.always_runs() {
                self.lanes[i].len
            } else {
                0
            };
            // Jobs are polled in rounds: each job queued when a round starts
            // gets one poll. A round in which every poll yielded without the
            // clock moving cannot make progress this frame (a job yielding
            // for the next frame, or a virtual clock in tests), so the lane
            // stops instead of spinning until the budget expires.
            let mut round_left = self.lanes[i].len;
            let mut round_start = clock.now();
            let mut round_progress = false;
            while self.lanes[i].len > 0 {
                if round_left == 0 {
                    if !round_progress && clock.now() == round_start {
                        break;
                    }
                    round_left = self.lanes[i].len;
                    round_start = clock.now();
                    round_progress = false;
                }
                round_left -= 1;
                let now = clock.now();
                let over = budget.expired(now);
                if over && guaranteed == 0 {
                    break;
                }
                guaranteed = guaranteed.saturating_sub(1);
                let Some(mut job) = self.lanes[i].pop() else {
                    break;
                };
                let owner = job.owner();
                // An over-budget turn still gets a slice, or it could not do
                // any work at all.
                let slice_end = now + budget.max_slice;
                let deadline = if over {
                    slice_end
                } else {
                    slice_end.min(budget.deadline)
                };
                let slice = SliceBudget::until(clock, now, deadline);
                let mut cx = JobCx::new(ctx, &slice);
                // AssertUnwindSafe: a panicking job is dropped and reported,
                // and the engine context is only observed through the
                // supervisor's restart path afterwards.
                let outcome = catch_unwind(AssertUnwindSafe(|| job.poll(&mut cx)));
                let spawned = cx.take_spawned();
                let spent = clock.now() - now;

                report.jobs_polled += 1;
                report.per_lane[i] += spent;
                *owner_time.entry(owner).or_default() += spent;
                let poll = match outcome {
                    Ok(p) => p,
                    Err(payload) => JobPoll::Failed(Fault::from_panic(payload)),
                };
                round_progress |= !matches!(poll, JobPoll::Yield);
                match poll {
                    JobPoll::Done => report.jobs_done += 1,
                    JobPoll::Yield => {
                        report.jobs_yielded += 1;
                        self.spawn(job);
                    }
                    JobPoll::Failed(fault) => {
                        report.jobs_failed += 1;
                        report.faults.push((owner, fault));
                    }
                }
                for j in spawned {
                    self.spawn(j);
                }
            }
        }

        let end = clock.now();
        report.elapsed = end - run_start;
        report.overrun = end > budget.deadline;
        for lane in Lane::ALL {
            report.pending[lane.index()] = self.lane_len(lane);
        }
        let mut owners: Vec<_> = owner_time.into_iter().collect();
        // ProcId breaks ties so the report is deterministic.
        owners.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        report
            .top_owners
            .extend(owners.into_iter().take(TOP_OWNERS));
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::FnJob;
    use crate::time::{Time, VirtualClock};

    struct Ctx {
        clock: VirtualClock,
        log: Vec<String>,
    }

    fn ctx() -> Ctx {
        Ctx {
            clock: VirtualClock::new(),
            log: Vec::new(),
        }
    }

    fn pid(n: u32) -> ProcId {
        ProcId::new(n, 0)
    }

    fn frame(clock: &VirtualClock, ms: u64) -> FrameBudget {
        FrameBudget::new(clock.now(), Duration::from_millis(ms))
    }

    fn once(lane: Lane, owner: ProcId, tag: &'static str) -> Box<dyn Job<Ctx>> {
        Box::new(FnJob::new(lane, owner, move |cx: &mut JobCx<'_, Ctx>| {
            cx.ctx.log.push(tag.to_string());
            JobPoll::Done
        }))
    }

    fn run(s: &mut Scheduler<Ctx>, c: &mut Ctx, ms: u64) -> FrameReport {
        let clock = c.clock.clone();
        s.run_frame(c, frame(&clock, ms), &clock)
    }

    #[test]
    fn lanes_run_in_priority_order() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        // Spawned in reverse so insertion order cannot explain the result.
        for (lane, tag) in [
            (Lane::Idle, "idle"),
            (Lane::Deferred, "deferred"),
            (Lane::Prefetch, "prefetch"),
            (Lane::Visible, "visible"),
            (Lane::Critical, "critical"),
            (Lane::Input, "input"),
        ] {
            s.spawn(once(lane, pid(1), tag));
        }
        let r = run(&mut s, &mut c, 16);
        assert_eq!(
            c.log,
            [
                "input", "critical", "visible", "prefetch", "deferred", "idle"
            ]
        );
        assert_eq!((r.jobs_polled, r.jobs_done), (6, 6));
        assert!(s.is_empty() && !r.overrun);
    }

    #[test]
    fn deferred_and_prefetch_rotate_across_owners() {
        for lane in [Lane::Prefetch, Lane::Deferred] {
            let mut s = Scheduler::new();
            let mut c = ctx();
            for (owner, n) in [(1, 3), (2, 1), (3, 2)] {
                for _ in 0..n {
                    s.spawn(once(lane, pid(owner), ["a", "b", "c"][owner as usize - 1]));
                }
            }
            run(&mut s, &mut c, 16);
            assert_eq!(c.log, ["a", "b", "c", "a", "c", "a"], "{lane:?}");
        }
    }

    #[test]
    fn yielding_jobs_keep_their_place_in_the_rotation() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        for (n, tag) in [(1, "a"), (2, "b"), (3, "c")] {
            let mut polls = 0;
            s.spawn(Box::new(FnJob::new(
                Lane::Deferred,
                pid(n),
                move |cx: &mut JobCx<'_, Ctx>| {
                    cx.ctx.log.push(tag.to_string());
                    cx.ctx.clock.advance(Duration::from_micros(10));
                    polls += 1;
                    if polls == 2 {
                        JobPoll::Done
                    } else {
                        JobPoll::Yield
                    }
                },
            )));
        }
        let r = run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["a", "b", "c", "a", "b", "c"]);
        assert_eq!(r.jobs_yielded, 3);
        assert_eq!(r.jobs_done, 3);
    }

    #[test]
    fn fifo_lanes_do_not_rotate_by_owner() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(once(Lane::Visible, pid(1), "a1"));
        s.spawn(once(Lane::Visible, pid(1), "a2"));
        s.spawn(once(Lane::Visible, pid(2), "b1"));
        run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["a1", "a2", "b1"]);
    }

    /// A job that burns 100 us per step until its slice runs out.
    fn busy(lane: Lane, owner: ProcId, steps: u32, tag: &'static str) -> Box<dyn Job<Ctx>> {
        let mut left = steps;
        Box::new(FnJob::new(lane, owner, move |cx: &mut JobCx<'_, Ctx>| {
            cx.ctx.log.push(format!("{tag}:start"));
            while left > 0 {
                if cx.budget.check() {
                    return JobPoll::Yield;
                }
                cx.ctx.clock.advance(Duration::from_micros(100));
                left -= 1;
            }
            JobPoll::Done
        }))
    }

    #[test]
    fn slices_are_bounded_and_work_resumes_next_frame() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        // 100 steps x 100 us = 10 ms of work, 2 ms slices, 5 ms frames.
        s.spawn(busy(Lane::Visible, pid(1), 100, "v"));
        let r = run(&mut s, &mut c, 5);
        // Slices of 2, 2 and then the 1 ms left of the frame.
        assert_eq!(r.jobs_polled, 3);
        assert_eq!(r.jobs_yielded, 3);
        assert_eq!(r.pending[Lane::Visible.index()], 1);
        assert_eq!(r.elapsed, Duration::from_millis(5));
        assert!(!r.overrun);
        // The remaining 5 ms of work fits exactly into the second frame.
        let r = run(&mut s, &mut c, 5);
        assert_eq!((r.jobs_polled, r.jobs_done), (3, 1));
        assert!(s.is_empty());
    }

    #[test]
    fn spent_budget_skips_lower_lanes_but_not_input_or_critical() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(once(Lane::Input, pid(1), "input"));
        s.spawn(once(Lane::Critical, pid(1), "critical"));
        s.spawn(busy(Lane::Visible, pid(2), 1000, "visible"));
        s.spawn(once(Lane::Deferred, pid(3), "deferred"));
        s.spawn(once(Lane::Idle, pid(4), "idle"));
        let r = run(&mut s, &mut c, 3);
        assert_eq!(c.log.first().map(String::as_str), Some("input"));
        assert!(c.log.iter().any(|l| l == "critical"));
        assert!(!c.log.iter().any(|l| l == "deferred" || l == "idle"));
        assert_eq!(r.pending[Lane::Deferred.index()], 1);
        assert_eq!(r.pending[Lane::Idle.index()], 1);

        // Zero budget: only the guaranteed lanes run, once each.
        let mut c = ctx();
        let mut s = Scheduler::new();
        s.spawn(once(Lane::Input, pid(1), "input"));
        s.spawn(once(Lane::Visible, pid(1), "visible"));
        let clock = c.clock.clone();
        let zero = FrameBudget::new(clock.now(), Duration::ZERO);
        let r = s.run_frame(&mut c, zero, &clock);
        assert_eq!(c.log, ["input"]);
        assert_eq!(r.pending_jobs(), 1);
    }

    #[test]
    fn guaranteed_lanes_poll_each_job_once_not_forever() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        // Always yields without using time, and the budget is zero.
        s.spawn(Box::new(FnJob::new(
            Lane::Input,
            pid(1),
            |cx: &mut JobCx<'_, Ctx>| {
                cx.ctx.log.push("spin".into());
                JobPoll::Yield
            },
        )));
        let clock = c.clock.clone();
        let r = s.run_frame(
            &mut c,
            FrameBudget::new(clock.now(), Duration::ZERO),
            &clock,
        );
        assert_eq!(c.log.len(), 1);
        assert_eq!(r.pending[Lane::Input.index()], 1);
    }

    #[test]
    fn zero_time_yields_end_the_lane_instead_of_spinning() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        // Yields without using time, under a virtual clock that never moves
        // and a generous budget: before rounds, this spun forever.
        s.spawn(Box::new(FnJob::new(
            Lane::Deferred,
            pid(1),
            |cx: &mut JobCx<'_, Ctx>| {
                cx.ctx.log.push("wait".into());
                JobPoll::Yield
            },
        )));
        s.spawn(once(Lane::Idle, pid(2), "idle"));
        let r = run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["wait", "idle"]);
        assert_eq!(r.pending[Lane::Deferred.index()], 1);
    }

    #[test]
    fn idle_lane_runs_only_with_leftover_budget() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(busy(Lane::Idle, pid(1), 1, "gc"));
        let r = run(&mut s, &mut c, 4);
        assert_eq!(r.jobs_done, 1);
        s.spawn(busy(Lane::Visible, pid(2), 100, "v"));
        s.spawn(once(Lane::Idle, pid(1), "gc"));
        let r = run(&mut s, &mut c, 2);
        assert_eq!(r.jobs_done, 0);
        assert_eq!(r.pending[Lane::Idle.index()], 1);
    }

    #[test]
    fn failures_and_panics_are_reported_and_isolated() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(Box::new(FnJob::new(
            Lane::Visible,
            pid(1),
            |_: &mut JobCx<'_, Ctx>| JobPoll::Failed(Fault::JsOutOfMemory),
        )));
        s.spawn(Box::new(FnJob::new(
            Lane::Visible,
            pid(2),
            |_: &mut JobCx<'_, Ctx>| -> JobPoll { panic!("handler blew up") },
        )));
        s.spawn(once(Lane::Visible, pid(3), "survivor"));
        let r = run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["survivor"]);
        assert_eq!((r.jobs_failed, r.jobs_done), (2, 1));
        assert_eq!(
            r.faults,
            vec![
                (pid(1), Fault::JsOutOfMemory),
                (pid(2), Fault::Panic("handler blew up".into())),
            ]
        );
        assert!(s.is_empty(), "failed jobs are dropped, not retried");
    }

    #[test]
    fn report_charges_time_to_lanes_and_owners() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        let cost = |ms: u64, lane, owner| -> Box<dyn Job<Ctx>> {
            Box::new(FnJob::new(lane, owner, move |cx: &mut JobCx<'_, Ctx>| {
                cx.ctx.clock.advance_ms(ms);
                JobPoll::Done
            }))
        };
        s.spawn(cost(1, Lane::Visible, pid(1)));
        s.spawn(cost(3, Lane::Visible, pid(2)));
        s.spawn(cost(2, Lane::Deferred, pid(1)));
        s.spawn(cost(1, Lane::Deferred, pid(3)));
        let mut r = run(&mut s, &mut c, 16);
        assert_eq!(r.lane_time(Lane::Visible), Duration::from_millis(4));
        assert_eq!(r.lane_time(Lane::Deferred), Duration::from_millis(3));
        assert_eq!(r.lane_time(Lane::Input), Duration::ZERO);
        assert_eq!(
            r.top_owners.as_slice(),
            &[
                (pid(1), Duration::from_millis(3)),
                (pid(2), Duration::from_millis(3)),
                (pid(3), Duration::from_millis(1)),
            ]
        );
        assert_eq!(r.elapsed, Duration::from_millis(7));

        r.record_drain(&DrainReport {
            drained: 5,
            remaining: 0,
            elapsed: Duration::from_millis(2),
        });
        assert_eq!((r.drained, r.drain_time), (5, Duration::from_millis(2)));
        assert_eq!(r.lane_time(Lane::Critical), Duration::from_millis(2));
    }

    #[test]
    fn overrun_is_flagged() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(Box::new(FnJob::new(
            Lane::Input,
            pid(1),
            |cx: &mut JobCx<'_, Ctx>| {
                cx.ctx.clock.advance_ms(10);
                JobPoll::Done
            },
        )));
        assert!(run(&mut s, &mut c, 4).overrun);
    }

    #[test]
    fn cancel_owner_removes_queued_jobs_in_every_lane() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(once(Lane::Visible, pid(1), "v1"));
        s.spawn(once(Lane::Visible, pid(2), "v2"));
        s.spawn(once(Lane::Deferred, pid(1), "d1"));
        s.spawn(once(Lane::Deferred, pid(2), "d2"));
        s.spawn(once(Lane::Deferred, pid(1), "d1b"));
        assert_eq!(s.cancel_owner(pid(1)), 3);
        assert_eq!(s.len(), 2);
        assert_eq!(s.cancel_owner(pid(1)), 0);
        run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["v2", "d2"]);
    }

    #[test]
    fn jobs_can_spawn_followups_and_move_lanes() {
        let mut s = Scheduler::new();
        let mut c = ctx();
        s.spawn(Box::new(FnJob::new(
            Lane::Visible,
            pid(1),
            |cx: &mut JobCx<'_, Ctx>| {
                cx.ctx.log.push("parent".into());
                cx.spawn(once(Lane::Deferred, pid(1), "child"));
                JobPoll::Done
            },
        )));
        let r = run(&mut s, &mut c, 16);
        assert_eq!(c.log, ["parent", "child"]);
        assert_eq!(r.jobs_polled, 2);
    }

    #[test]
    fn frame_budget_time_zero_start_is_supported() {
        // Guards the `Time::ZERO` start used by every virtual-clock test.
        let b = FrameBudget::new(Time::ZERO, Duration::from_millis(1));
        assert!(!b.expired(Time::ZERO));
    }
}
