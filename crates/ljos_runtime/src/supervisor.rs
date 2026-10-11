//! Supervision (ARCHITECTURE.md 9.4).
//!
//! - The policy, here: restart strategy, restart kind and the
//!   restart-intensity window, as pure decisions ([`Supervisor`]).
//! - The tree, in [`tree`]: supervisors and their children, child specs,
//!   start and stop ([`SupervisorTree`]). The engine keeps its UI boundaries
//!   in one; time comes from the caller's [`Clock`](crate::Clock), so tests
//!   run on virtual time.
//! - tokio tasks, in [`tasks`] (cargo feature `tasks`): a tree whose
//!   children are tasks in a `JoinSet`, each with a `CancellationToken`.
//!
//! Pinned plugin threads (watched by heartbeat) and the development tools'
//! supervisor are later milestones; [`ChildKind`] is open for them.

use std::collections::VecDeque;
use std::time::Duration;

use thiserror::Error;

use crate::fault::Fault;
use crate::time::Time;

#[cfg(feature = "tasks")]
pub mod tasks;
pub mod tree;

pub use tree::{
    APP_INTENSITY, ChildInfo, ChildKind, ChildSpec, Outcome, PLUGIN_INTENSITY, SERVICE_INTENSITY,
    StandardSups, StartError, SupId, SupervisorTree,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// Restart only the child that exited.
    OneForOne,
    /// Restart every child.
    OneForAll,
    /// Restart the child that exited and every child started after it.
    RestForOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Restart {
    /// Always restarted.
    Permanent,
    /// Restarted only after a fault.
    Transient,
    /// Never restarted, and forgotten once it exits.
    Temporary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Normal,
    Shutdown,
    /// Replaced by hot reload; not a failure.
    Reload,
    Fault(Fault),
}

impl ExitReason {
    pub fn is_fault(&self) -> bool {
        matches!(self, ExitReason::Fault(_))
    }
}

impl Restart {
    /// Whether a child of this type is restarted after exiting for `reason`.
    pub fn should_restart(self, reason: &ExitReason) -> bool {
        match self {
            Restart::Permanent => true,
            Restart::Transient => reason.is_fault(),
            Restart::Temporary => false,
        }
    }
}

/// At most `max_restarts` restarts in any `within` window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intensity {
    pub max_restarts: u32,
    pub within: Duration,
}

impl Intensity {
    pub const fn new(max_restarts: u32, within: Duration) -> Self {
        Self {
            max_restarts,
            within,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntensityVerdict {
    Within,
    /// More than `max_restarts` restarts fell inside the window: stop
    /// restarting and escalate to the parent.
    Exceeded,
}

/// Sliding window of restart times (OTP semantics: the supervisor gives up when
/// restart number `max_restarts + 1` happens within `within` of the earlier ones).
#[derive(Clone, Debug)]
pub struct IntensityTracker {
    intensity: Intensity,
    restarts: VecDeque<Time>,
}

impl IntensityTracker {
    pub fn new(intensity: Intensity) -> Self {
        Self {
            intensity,
            restarts: VecDeque::new(),
        }
    }

    pub fn intensity(&self) -> Intensity {
        self.intensity
    }

    /// Records a restart at `now` (non-decreasing across calls). A restart
    /// exactly `within` old is already outside the window.
    pub fn record(&mut self, now: Time) -> IntensityVerdict {
        while let Some(&oldest) = self.restarts.front() {
            if now - oldest >= self.intensity.within {
                self.restarts.pop_front();
            } else {
                break;
            }
        }
        self.restarts.push_back(now);
        if self.restarts.len() as u64 > u64::from(self.intensity.max_restarts) {
            IntensityVerdict::Exceeded
        } else {
            IntensityVerdict::Within
        }
    }

    pub fn recent(&self) -> usize {
        self.restarts.len()
    }

    /// Changes the limit. Restarts already counted stay counted.
    pub fn set_intensity(&mut self, intensity: Intensity) {
        self.intensity = intensity;
    }

    /// Forgets the restarts counted so far.
    pub fn clear(&mut self) {
        self.restarts.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChildId(pub u64);

/// What the owner of a [`Supervisor`] must do after an exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Directive {
    /// Leave the child stopped.
    NoRestart,
    /// Stop `terminate` (in the given order, which is reverse start order), then
    /// start `start` (in start order; includes the exited child).
    Restart {
        terminate: Vec<ChildId>,
        start: Vec<ChildId>,
    },
    /// Intensity exceeded: this supervisor gives up and its parent decides.
    Escalate,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SupervisorError {
    #[error("unknown child {0:?}")]
    UnknownChild(ChildId),
    #[error("duplicate child {0:?}")]
    DuplicateChild(ChildId),
    #[error("unknown supervisor")]
    UnknownSupervisor,
    #[error("child {0:?} is a supervisor, not a leaf")]
    NotALeaf(ChildId),
}

#[derive(Debug)]
pub struct Supervisor {
    strategy: Strategy,
    tracker: IntensityTracker,
    /// In start order.
    children: Vec<(ChildId, Restart)>,
}

impl Supervisor {
    pub fn new(strategy: Strategy, intensity: Intensity) -> Self {
        Self {
            strategy,
            tracker: IntensityTracker::new(intensity),
            children: Vec::new(),
        }
    }

    pub fn strategy(&self) -> Strategy {
        self.strategy
    }

    pub fn children(&self) -> impl Iterator<Item = ChildId> + '_ {
        self.children.iter().map(|&(id, _)| id)
    }

    pub fn intensity(&self) -> Intensity {
        self.tracker.intensity()
    }

    /// Changes the limit. Restarts already counted stay counted.
    pub fn set_intensity(&mut self, intensity: Intensity) {
        self.tracker.set_intensity(intensity);
    }

    /// Restarts counted inside the window as of the last exit.
    pub fn recent_restarts(&self) -> usize {
        self.tracker.recent()
    }

    /// Forgets the restarts counted so far (the supervisor itself was
    /// restarted).
    pub fn reset(&mut self) {
        self.tracker.clear();
    }

    /// Forgets `id`. False when there is no such child.
    pub fn remove_child(&mut self, id: ChildId) -> bool {
        let before = self.children.len();
        self.children.retain(|&(c, _)| c != id);
        self.children.len() != before
    }

    pub fn add_child(&mut self, id: ChildId, restart: Restart) -> Result<(), SupervisorError> {
        if self.children.iter().any(|&(c, _)| c == id) {
            return Err(SupervisorError::DuplicateChild(id));
        }
        self.children.push((id, restart));
        Ok(())
    }

    /// Decides what follows `child` exiting at `now`.
    pub fn on_exit(
        &mut self,
        child: ChildId,
        reason: &ExitReason,
        now: Time,
    ) -> Result<Directive, SupervisorError> {
        let idx = self
            .children
            .iter()
            .position(|&(c, _)| c == child)
            .ok_or(SupervisorError::UnknownChild(child))?;
        let restart = self.children[idx].1;
        if !restart.should_restart(reason) {
            if restart == Restart::Temporary {
                self.children.remove(idx);
            }
            return Ok(Directive::NoRestart);
        }
        if self.tracker.record(now) == IntensityVerdict::Exceeded {
            return Ok(Directive::Escalate);
        }
        let affected = match self.strategy {
            Strategy::OneForOne => idx..idx + 1,
            Strategy::OneForAll => 0..self.children.len(),
            Strategy::RestForOne => idx..self.children.len(),
        };
        // Temporary siblings are stopped but never come back.
        let terminate: Vec<ChildId> = self.children[affected.clone()]
            .iter()
            .rev()
            .map(|&(c, _)| c)
            .filter(|&c| c != child)
            .collect();
        let start: Vec<ChildId> = self.children[affected]
            .iter()
            .filter(|&&(c, r)| c == child || r != Restart::Temporary)
            .map(|&(c, _)| c)
            .collect();
        self.children
            .retain(|&(c, r)| r != Restart::Temporary || c == child || !terminate.contains(&c));
        Ok(Directive::Restart { terminate, start })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert_eq, proptest};

    fn ms(n: u64) -> Time {
        Time::from_millis(n)
    }

    fn fault() -> ExitReason {
        ExitReason::Fault(Fault::JsOutOfMemory)
    }

    fn tracker(max: u32, within_ms: u64) -> IntensityTracker {
        IntensityTracker::new(Intensity::new(max, Duration::from_millis(within_ms)))
    }

    #[test]
    fn intensity_allows_max_then_exceeds() {
        let mut t = tracker(3, 1000);
        assert_eq!(t.record(ms(0)), IntensityVerdict::Within);
        assert_eq!(t.record(ms(100)), IntensityVerdict::Within);
        assert_eq!(t.record(ms(200)), IntensityVerdict::Within);
        assert_eq!(t.record(ms(300)), IntensityVerdict::Exceeded);
    }

    #[test]
    fn intensity_window_slides() {
        let mut t = tracker(2, 1000);
        t.record(ms(0));
        t.record(ms(500));
        // The restart at 0 ms is exactly `within` old, so it no longer counts.
        assert_eq!(t.record(ms(1000)), IntensityVerdict::Within);
        assert_eq!(t.recent(), 2);
        // 500 ms and 1000 ms are inside the window of this one.
        assert_eq!(t.record(ms(1200)), IntensityVerdict::Exceeded);
        // Much later, everything has aged out.
        assert_eq!(t.record(ms(10_000)), IntensityVerdict::Within);
        assert_eq!(t.recent(), 1);
    }

    #[test]
    fn zero_restarts_allowed_escalates_immediately() {
        let mut t = tracker(0, 1000);
        assert_eq!(t.record(ms(0)), IntensityVerdict::Exceeded);
    }

    proptest! {
        #[test]
        fn verdict_matches_a_naive_count(
            max in 0u32..6,
            within in 1u64..500,
            gaps in proptest::collection::vec(0u64..300, 1..40),
        ) {
            let mut t = tracker(max, within);
            let mut now = 0u64;
            let mut times: Vec<u64> = Vec::new();
            for g in gaps {
                now += g;
                times.push(now);
                let in_window = times.iter().filter(|&&x| now - x < within).count() as u32;
                let expect = if in_window > max {
                    IntensityVerdict::Exceeded
                } else {
                    IntensityVerdict::Within
                };
                prop_assert_eq!(t.record(ms(now)), expect);
            }
        }
    }

    #[test]
    fn restart_types() {
        assert!(Restart::Permanent.should_restart(&ExitReason::Normal));
        assert!(!Restart::Transient.should_restart(&ExitReason::Normal));
        assert!(!Restart::Transient.should_restart(&ExitReason::Reload));
        assert!(Restart::Transient.should_restart(&fault()));
        assert!(!Restart::Temporary.should_restart(&fault()));
    }

    fn sup(strategy: Strategy) -> Supervisor {
        let mut s = Supervisor::new(strategy, Intensity::new(10, Duration::from_secs(60)));
        for i in 1..=3 {
            s.add_child(ChildId(i), Restart::Permanent).unwrap();
        }
        s
    }

    fn ids(v: &[u64]) -> Vec<ChildId> {
        v.iter().map(|&i| ChildId(i)).collect()
    }

    #[test]
    fn strategies_pick_the_affected_children() {
        let d = sup(Strategy::OneForOne)
            .on_exit(ChildId(2), &fault(), ms(0))
            .unwrap();
        assert_eq!(
            d,
            Directive::Restart {
                terminate: vec![],
                start: ids(&[2])
            }
        );
        let d = sup(Strategy::OneForAll)
            .on_exit(ChildId(2), &fault(), ms(0))
            .unwrap();
        assert_eq!(
            d,
            Directive::Restart {
                terminate: ids(&[3, 1]),
                start: ids(&[1, 2, 3])
            }
        );
        let d = sup(Strategy::RestForOne)
            .on_exit(ChildId(2), &fault(), ms(0))
            .unwrap();
        assert_eq!(
            d,
            Directive::Restart {
                terminate: ids(&[3]),
                start: ids(&[2, 3])
            }
        );
    }

    #[test]
    fn temporary_children_are_dropped_not_restarted() {
        let mut s = Supervisor::new(
            Strategy::OneForAll,
            Intensity::new(10, Duration::from_secs(60)),
        );
        s.add_child(ChildId(1), Restart::Permanent).unwrap();
        s.add_child(ChildId(2), Restart::Temporary).unwrap();
        assert_eq!(
            s.on_exit(ChildId(2), &fault(), ms(0)).unwrap(),
            Directive::NoRestart
        );
        assert_eq!(s.children().collect::<Vec<_>>(), ids(&[1]));
        s.add_child(ChildId(3), Restart::Temporary).unwrap();
        let d = s.on_exit(ChildId(1), &fault(), ms(1)).unwrap();
        assert_eq!(
            d,
            Directive::Restart {
                terminate: ids(&[3]),
                start: ids(&[1])
            }
        );
        assert_eq!(s.children().collect::<Vec<_>>(), ids(&[1]));
    }

    #[test]
    fn exceeding_intensity_escalates() {
        let mut s = Supervisor::new(
            Strategy::OneForOne,
            Intensity::new(2, Duration::from_secs(30)),
        );
        s.add_child(ChildId(1), Restart::Permanent).unwrap();
        for t in [0, 1000] {
            assert!(matches!(
                s.on_exit(ChildId(1), &fault(), ms(t)).unwrap(),
                Directive::Restart { .. }
            ));
        }
        assert_eq!(
            s.on_exit(ChildId(1), &fault(), ms(2000)).unwrap(),
            Directive::Escalate
        );
    }

    #[test]
    fn normal_exits_do_not_count_against_intensity() {
        let mut s = Supervisor::new(
            Strategy::OneForOne,
            Intensity::new(1, Duration::from_secs(30)),
        );
        s.add_child(ChildId(1), Restart::Transient).unwrap();
        for t in 0..5 {
            assert_eq!(
                s.on_exit(ChildId(1), &ExitReason::Normal, ms(t)).unwrap(),
                Directive::NoRestart
            );
        }
    }

    #[test]
    fn unknown_and_duplicate_children() {
        let mut s = sup(Strategy::OneForOne);
        assert_eq!(
            s.on_exit(ChildId(99), &fault(), ms(0)),
            Err(SupervisorError::UnknownChild(ChildId(99)))
        );
        assert_eq!(
            s.add_child(ChildId(1), Restart::Permanent),
            Err(SupervisorError::DuplicateChild(ChildId(1)))
        );
    }
}
