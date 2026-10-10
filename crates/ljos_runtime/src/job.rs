//! The unit of schedulable work.

use std::fmt;
use std::marker::PhantomData;

use crate::budget::SliceBudget;
use crate::fault::Fault;
use crate::lane::{Lane, ProcId};

/// Result of one `Job::poll`.
#[derive(Debug)]
pub enum JobPoll {
    /// Finished; the scheduler drops the job.
    Done,
    /// Out of slice (or voluntarily pausing) with work left. Requeued at the
    /// back of its lane and owner, so others get a turn first.
    Yield,
    /// Failed. The job is dropped and the fault goes into
    /// `FrameReport::faults` for the supervisor.
    Failed(Fault),
}

/// What a job sees while running. `C` is the engine's own context (scene, JS
/// host, ...); this crate does not know what it is.
pub struct JobCx<'a, C> {
    /// The caller-provided context passed to `Scheduler::run_frame`.
    pub ctx: &'a mut C,
    /// The job must return [`JobPoll::Yield`] once `budget.check()` is true.
    pub budget: &'a SliceBudget<'a>,
    spawned: Vec<Box<dyn Job<C>>>,
}

impl<'a, C> JobCx<'a, C> {
    pub(crate) fn new(ctx: &'a mut C, budget: &'a SliceBudget<'a>) -> Self {
        Self {
            ctx,
            budget,
            spawned: Vec::new(),
        }
    }

    /// Queues a follow-up job; it is enqueued when this poll returns, which
    /// avoids the scheduler being borrowed while its own job runs.
    pub fn spawn(&mut self, job: Box<dyn Job<C>>) {
        self.spawned.push(job);
    }

    pub(crate) fn take_spawned(&mut self) -> Vec<Box<dyn Job<C>>> {
        std::mem::take(&mut self.spawned)
    }
}

impl<C> fmt::Debug for JobCx<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobCx")
            .field("budget", &self.budget)
            .field("spawned", &self.spawned.len())
            .finish()
    }
}

/// A resumable piece of work. State lives in the job; `poll` advances it until
/// done or until `cx.budget.check()` says to stop.
///
/// A yielded job is polled again in the same frame while budget remains, after
/// the other jobs of its lane. If a whole round of a lane yields without the
/// clock moving, the lane stops for this frame, so a job waiting for the next
/// frame may simply return `Yield`.
pub trait Job<C> {
    /// Re-read after every `Yield`, so a job may move lanes (a Visible
    /// incubation that scrolls off becomes Prefetch).
    fn lane(&self) -> Lane;
    /// Whom the time is charged to and whom round-robin rotates over.
    fn owner(&self) -> ProcId;
    fn poll(&mut self, cx: &mut JobCx<'_, C>) -> JobPoll;
}

/// A [`Job`] from a closure; convenient for small tasks and tests.
pub struct FnJob<C, F> {
    lane: Lane,
    owner: ProcId,
    f: F,
    _ctx: PhantomData<fn(&mut C)>,
}

impl<C, F> FnJob<C, F>
where
    F: FnMut(&mut JobCx<'_, C>) -> JobPoll,
{
    pub fn new(lane: Lane, owner: ProcId, f: F) -> Self {
        Self {
            lane,
            owner,
            f,
            _ctx: PhantomData,
        }
    }
}

impl<C, F> fmt::Debug for FnJob<C, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FnJob")
            .field("lane", &self.lane)
            .field("owner", &self.owner)
            .finish()
    }
}

impl<C, F> Job<C> for FnJob<C, F>
where
    F: FnMut(&mut JobCx<'_, C>) -> JobPoll,
{
    fn lane(&self) -> Lane {
        self.lane
    }

    fn owner(&self) -> ProcId {
        self.owner
    }

    fn poll(&mut self, cx: &mut JobCx<'_, C>) -> JobPoll {
        (self.f)(cx)
    }
}
