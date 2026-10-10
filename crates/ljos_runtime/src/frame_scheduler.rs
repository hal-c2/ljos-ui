//! `FrameScheduler`: decides when the UI thread must next wake (ARCHITECTURE.md 8.1).
//!
//! ```text
//! next_wake = min(animation tick, timers, request_repaint_after, pending work)
//! ```
//!
//! `None` means sleep until an external event (input, a [`crate::UiInbox`]
//! wake hook): that is the 0 % idle CPU guarantee. The scheduler itself never
//! sleeps or spawns anything. The shell asks [`FrameScheduler::next_wake`]
//! after each frame and arms a single OS timer (or `request_repaint_after`).
//!
//! # Timer coalescing
//!
//! A timer asks for `at`. Non-animation timers are rounded up to 1 ms. Each
//! kind also has a *slack*: how late the timer may fire. The wake time is the
//! earliest `deadline + slack` over all timers, the latest moment that breaks
//! nobody's window, and every timer whose rounded deadline has passed fires in
//! that wake. Timers are never fired early (JS `setTimeout` must not elapse
//! early). Only [`TimerKind::Timer`] has slack, up to 4 ms.
//!
//! # Animations
//!
//! Animations follow frame ticks rather than wall-clock sleeps. While anything
//! animates it calls [`FrameScheduler::request_animation_frame`] every frame;
//! the next wake is then one frame period after the previous
//! [`FrameScheduler::begin_frame`]. Stop requesting and the ticks stop.
//! [`TimerKind::Animation`] deadlines (a `PauseAnimation` end, say) are snapped
//! up to the next tick, since no frame can run between ticks anyway.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use bitflags::bitflags;

use crate::time::{Time, duration_micros};

/// Handle for cancelling a timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(u64);

/// What a timer is for. The kind sets rounding, slack and suppression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimerKind {
    /// Animation deadline. Snapped up to the next frame tick, no slack.
    Animation,
    /// Coarse application timer (QML `Timer`, `setTimeout`) with long
    /// intervals. Rounded to 1 ms, may fire up to 4 ms late so nearby timers
    /// share one wake. Use [`TimerKind::for_interval`] to pick.
    Timer,
    /// Timer that must fire on its 1 ms boundary, no slack: short intervals.
    Precise,
    /// `request_repaint_after`. Precise, and reported as [`WakeReasons::REPAINT`].
    Repaint,
    /// Text-cursor blink. Precise, and **ignored while the window is
    /// unfocused** (see [`FrameScheduler::set_focused`]). The policy is at most
    /// 2 wakeups per second: re-arm with [`CURSOR_BLINK_PERIOD`] from the
    /// moment of the last toggle, never faster, and only while a text control
    /// has active focus.
    CursorBlink,
}

/// Longest slack any timer gets.
pub const MAX_TIMER_SLACK: Duration = Duration::from_millis(4);
/// Timer deadlines are rounded up to this.
pub const TIMER_GRANULARITY: Duration = Duration::from_millis(1);
/// Cursor blink interval: 500 ms between toggles is 2 wakeups per second.
pub const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);

impl TimerKind {
    pub fn slack(self) -> Duration {
        match self {
            TimerKind::Timer => MAX_TIMER_SLACK,
            _ => Duration::ZERO,
        }
    }

    /// Picks [`TimerKind::Timer`] only when the slack is at most 20 % of the
    /// interval (4 ms of slack on a 20 ms interval); shorter intervals stay
    /// [`TimerKind::Precise`] so a 5 ms timer is not turned into a 9 ms one.
    pub fn for_interval(interval: Duration) -> TimerKind {
        if interval >= MAX_TIMER_SLACK * 5 {
            TimerKind::Timer
        } else {
            TimerKind::Precise
        }
    }
}

bitflags! {
    /// Why a frame is needed. Pending reasons make [`FrameScheduler::next_wake`]
    /// return "now"; [`FrameScheduler::begin_frame`] returns and clears them.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct WakeReasons: u8 {
        const INPUT = 1;
        /// The inbox still holds messages (a drain was budget-limited).
        const MESSAGES = 1 << 1;
        /// Something marked itself dirty.
        const DIRTY = 1 << 2;
        /// A lane still has runnable jobs.
        const JOBS = 1 << 3;
        const TIMER = 1 << 4;
        const ANIMATION = 1 << 5;
        const REPAINT = 1 << 6;
    }
}

/// A timer that has come due, as yielded by [`FrameScheduler::due`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DueTimer {
    pub id: TimerId,
    pub kind: TimerKind,
    /// The effective (rounded or snapped) deadline it was scheduled for.
    pub deadline: Time,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    kind: TimerKind,
    deadline: Time,
    latest: Time,
}

#[derive(Debug)]
pub struct FrameScheduler {
    timers: HashMap<TimerId, Entry>,
    by_deadline: BTreeSet<(Time, TimerId)>,
    by_latest: BTreeSet<(Time, TimerId)>,
    next_id: u64,
    pending: WakeReasons,
    animating: bool,
    frame_period: Duration,
    last_tick: Option<Time>,
    focused: bool,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameScheduler {
    /// A scheduler assuming a 60 Hz display until [`Self::set_frame_period`].
    pub fn new() -> Self {
        Self {
            timers: HashMap::new(),
            by_deadline: BTreeSet::new(),
            by_latest: BTreeSet::new(),
            next_id: 0,
            pending: WakeReasons::empty(),
            animating: false,
            frame_period: Duration::from_micros(16_667),
            last_tick: None,
            focused: true,
        }
    }

    pub fn frame_period(&self) -> Duration {
        self.frame_period
    }

    /// Display refresh period (from the shell; 120 Hz is ~8.3 ms).
    pub fn set_frame_period(&mut self, period: Duration) {
        self.frame_period = period;
    }

    /// Window focus. While unfocused, [`TimerKind::CursorBlink`] timers neither
    /// wake the app nor come due; they stay registered and resume on refocus.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Registers a one-shot timer. Recurring timers re-register from their
    /// handler, which keeps drift and pausing under the caller's control.
    pub fn add_timer(&mut self, at: Time, kind: TimerKind) -> TimerId {
        let deadline = match kind {
            TimerKind::Animation => self.snap_to_tick(at),
            _ => at.round_up_to(TIMER_GRANULARITY),
        };
        let latest = deadline + kind.slack();
        let id = TimerId(self.next_id);
        self.next_id += 1;
        self.timers.insert(
            id,
            Entry {
                kind,
                deadline,
                latest,
            },
        );
        self.by_deadline.insert((deadline, id));
        self.by_latest.insert((latest, id));
        id
    }

    /// `request_repaint_after`: wake no earlier than `at` to run a frame.
    pub fn request_repaint_at(&mut self, at: Time) -> TimerId {
        self.add_timer(at, TimerKind::Repaint)
    }

    /// Convenience for [`Self::request_repaint_at`] with a relative delay.
    pub fn request_repaint_after(&mut self, now: Time, delay: Duration) -> TimerId {
        self.request_repaint_at(now + delay)
    }

    /// Returns false if the timer already fired or was cancelled.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        let Some(e) = self.timers.remove(&id) else {
            return false;
        };
        self.by_deadline.remove(&(e.deadline, id));
        self.by_latest.remove(&(e.latest, id));
        true
    }

    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }

    /// Asks for a frame at the next tick. Animations call this every frame
    /// they are still running; the request is forgotten by [`Self::begin_frame`].
    pub fn request_animation_frame(&mut self) {
        self.animating = true;
    }

    /// Marks a reason a frame is needed right now (input arrived, dirty items,
    /// leftover messages, runnable jobs).
    pub fn mark(&mut self, reasons: WakeReasons) {
        self.pending |= reasons;
    }

    /// Starts a frame at `now`: returns why we are awake, clears the pending
    /// marks and animation request, and anchors the frame-tick grid. Timers are
    /// not consumed here; drain them with [`Self::due`].
    pub fn begin_frame(&mut self, now: Time) -> WakeReasons {
        let mut reasons = std::mem::take(&mut self.pending);
        if std::mem::take(&mut self.animating) {
            reasons |= WakeReasons::ANIMATION;
        }
        for &(_, id) in self.by_deadline.range(..=(now, TimerId(u64::MAX))) {
            let e = &self.timers[&id];
            if self.is_active(e) {
                reasons |= match e.kind {
                    TimerKind::Repaint => WakeReasons::REPAINT,
                    TimerKind::Animation => WakeReasons::ANIMATION,
                    _ => WakeReasons::TIMER,
                };
            }
        }
        self.last_tick = Some(now);
        reasons
    }

    /// Removes and yields every active timer whose deadline is `<= now`, in
    /// deadline order. Lazy: timers not yielded stay registered if the
    /// iterator is dropped early. Timers added by the loop body for a time
    /// `<= now` are picked up too, so a handler can cascade within one frame.
    pub fn due(&mut self, now: Time) -> Due<'_> {
        Due { sched: self, now }
    }

    /// When the app must next wake, or `None` to sleep until external input.
    /// `Some(t)` with `t <= now` means "run a frame immediately"; pending marks
    /// report `Time::ZERO`.
    pub fn next_wake(&self) -> Option<Time> {
        if !self.pending.is_empty() {
            return Some(Time::ZERO);
        }
        let tick = self
            .animating
            .then(|| self.last_tick.map_or(Time::ZERO, |t| t + self.frame_period));
        let timer = self
            .by_latest
            .iter()
            .find(|(_, id)| self.is_active(&self.timers[id]))
            .map(|&(t, _)| t);
        match (tick, timer) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn is_active(&self, e: &Entry) -> bool {
        self.focused || e.kind != TimerKind::CursorBlink
    }

    fn snap_to_tick(&self, at: Time) -> Time {
        let period = duration_micros(self.frame_period);
        match self.last_tick {
            Some(t) if at > t && period > 0 => {
                let k = duration_micros(at - t).div_ceil(period);
                Time::from_micros(t.as_micros().saturating_add(k.saturating_mul(period)))
            }
            _ => at,
        }
    }
}

/// Iterator returned by [`FrameScheduler::due`].
#[derive(Debug)]
pub struct Due<'a> {
    sched: &'a mut FrameScheduler,
    now: Time,
}

impl Iterator for Due<'_> {
    type Item = DueTimer;

    fn next(&mut self) -> Option<DueTimer> {
        let s = &mut *self.sched;
        let &(_, id) = s
            .by_deadline
            .range(..=(self.now, TimerId(u64::MAX)))
            .find(|(_, id)| s.is_active(&s.timers[id]))?;
        let e = s.timers.remove(&id)?;
        s.by_deadline.remove(&(e.deadline, id));
        s.by_latest.remove(&(e.latest, id));
        Some(DueTimer {
            id,
            kind: e.kind,
            deadline: e.deadline,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Time {
        Time::from_millis(n)
    }

    fn due_ids(s: &mut FrameScheduler, now: Time) -> Vec<TimerId> {
        s.due(now).map(|d| d.id).collect()
    }

    #[test]
    fn idle_scheduler_sleeps_forever() {
        let mut s = FrameScheduler::new();
        assert_eq!(s.next_wake(), None);
        // A frame with nothing requested leaves it idle: 0 wakeups.
        assert!(s.begin_frame(ms(10)).is_empty());
        assert_eq!(s.next_wake(), None);
        assert_eq!(s.due(ms(1000)).count(), 0);
    }

    #[test]
    fn precise_timer_rounds_up_to_one_ms() {
        let mut s = FrameScheduler::new();
        let id = s.add_timer(Time::from_micros(10_300), TimerKind::Precise);
        assert_eq!(s.next_wake(), Some(ms(11)));
        assert!(due_ids(&mut s, Time::from_micros(10_999)).is_empty());
        assert_eq!(due_ids(&mut s, ms(11)), vec![id]);
        assert_eq!(s.next_wake(), None);
    }

    #[test]
    fn coarse_timers_coalesce_within_slack() {
        let mut s = FrameScheduler::new();
        let a = s.add_timer(ms(10), TimerKind::Timer);
        let b = s.add_timer(ms(13), TimerKind::Timer);
        // One wake, at the latest time that still honours a's slack.
        assert_eq!(s.next_wake(), Some(ms(14)));
        assert_eq!(due_ids(&mut s, ms(14)), vec![a, b]);
        assert_eq!(s.next_wake(), None);
    }

    #[test]
    fn coarse_timer_beyond_slack_is_not_dragged_in() {
        let mut s = FrameScheduler::new();
        let a = s.add_timer(ms(10), TimerKind::Timer);
        let b = s.add_timer(ms(20), TimerKind::Timer);
        assert_eq!(s.next_wake(), Some(ms(14)));
        assert_eq!(due_ids(&mut s, ms(14)), vec![a]);
        assert_eq!(s.next_wake(), Some(ms(24)));
        assert_eq!(due_ids(&mut s, ms(24)), vec![b]);
    }

    #[test]
    fn timers_never_fire_early() {
        let mut s = FrameScheduler::new();
        s.add_timer(ms(10), TimerKind::Timer);
        assert!(due_ids(&mut s, Time::from_micros(9_999)).is_empty());
    }

    #[test]
    fn precise_timer_caps_the_wake_of_coarse_ones() {
        let mut s = FrameScheduler::new();
        let coarse = s.add_timer(ms(10), TimerKind::Timer);
        let precise = s.add_timer(ms(12), TimerKind::Precise);
        // The 12 ms wake is earlier than 14 ms, and the coarse timer rides along.
        assert_eq!(s.next_wake(), Some(ms(12)));
        assert_eq!(due_ids(&mut s, ms(12)), vec![coarse, precise]);
    }

    #[test]
    fn cancel_removes_timer_and_updates_next_wake() {
        let mut s = FrameScheduler::new();
        let a = s.add_timer(ms(5), TimerKind::Precise);
        let b = s.add_timer(ms(9), TimerKind::Precise);
        assert_eq!(s.next_wake(), Some(ms(5)));
        assert!(s.cancel(a));
        assert!(!s.cancel(a));
        assert_eq!(s.next_wake(), Some(ms(9)));
        assert!(s.cancel(b));
        assert_eq!(s.next_wake(), None);
        assert_eq!(s.timer_count(), 0);
    }

    #[test]
    fn due_is_lazy_and_in_deadline_order() {
        let mut s = FrameScheduler::new();
        let late = s.add_timer(ms(3), TimerKind::Precise);
        let early = s.add_timer(ms(2), TimerKind::Precise);
        let first = s.due(ms(5)).next().unwrap();
        assert_eq!(first.id, early);
        // Iterator dropped early: the other timer stays registered.
        assert_eq!(s.timer_count(), 1);
        assert_eq!(due_ids(&mut s, ms(5)), vec![late]);
    }

    #[test]
    fn pending_marks_wake_immediately_and_clear_on_begin_frame() {
        let mut s = FrameScheduler::new();
        s.mark(WakeReasons::INPUT | WakeReasons::DIRTY);
        assert_eq!(s.next_wake(), Some(Time::ZERO));
        let r = s.begin_frame(ms(1));
        assert_eq!(r, WakeReasons::INPUT | WakeReasons::DIRTY);
        assert_eq!(s.next_wake(), None);
    }

    #[test]
    fn animation_follows_frame_ticks_and_stops_when_not_requested() {
        let mut s = FrameScheduler::new();
        s.set_frame_period(Duration::from_millis(16));
        s.begin_frame(ms(100));
        s.request_animation_frame();
        assert_eq!(s.next_wake(), Some(ms(116)));
        let r = s.begin_frame(ms(116));
        assert_eq!(r, WakeReasons::ANIMATION);
        // The animation finished this frame and did not re-request.
        assert_eq!(s.next_wake(), None);
    }

    #[test]
    fn animation_deadlines_snap_up_to_the_next_tick() {
        let mut s = FrameScheduler::new();
        s.set_frame_period(Duration::from_millis(16));
        s.begin_frame(ms(100));
        let id = s.add_timer(ms(105), TimerKind::Animation);
        assert_eq!(s.next_wake(), Some(ms(116)));
        let id2 = s.add_timer(ms(133), TimerKind::Animation);
        assert!(due_ids(&mut s, ms(115)).is_empty());
        assert_eq!(due_ids(&mut s, ms(116)), vec![id]);
        assert_eq!(s.next_wake(), Some(ms(148)));
        assert_eq!(due_ids(&mut s, ms(148)), vec![id2]);
    }

    #[test]
    fn repaint_after_wakes_with_repaint_reason() {
        let mut s = FrameScheduler::new();
        s.request_repaint_after(ms(10), Duration::from_millis(30));
        assert_eq!(s.next_wake(), Some(ms(40)));
        assert!(s.begin_frame(ms(39)).is_empty());
        assert_eq!(s.begin_frame(ms(40)), WakeReasons::REPAINT);
    }

    #[test]
    fn begin_frame_reports_timer_reason() {
        let mut s = FrameScheduler::new();
        s.add_timer(ms(5), TimerKind::Precise);
        assert_eq!(s.begin_frame(ms(5)), WakeReasons::TIMER);
    }

    #[test]
    fn cursor_blink_is_silent_while_unfocused() {
        let mut s = FrameScheduler::new();
        let blink = s.add_timer(ms(500), TimerKind::CursorBlink);
        assert_eq!(s.next_wake(), Some(ms(500)));
        s.set_focused(false);
        assert_eq!(s.next_wake(), None, "no wakeups for an unfocused window");
        assert!(s.due(ms(600)).next().is_none());
        assert!(s.begin_frame(ms(600)).is_empty());
        // A real timer still works, and the blink resumes on refocus.
        let other = s.add_timer(ms(700), TimerKind::Precise);
        assert_eq!(s.next_wake(), Some(ms(700)));
        s.set_focused(true);
        assert_eq!(due_ids(&mut s, ms(700)), vec![blink, other]);
    }

    #[test]
    fn for_interval_keeps_slack_under_a_fifth_of_the_interval() {
        assert_eq!(
            TimerKind::for_interval(Duration::from_millis(5)),
            TimerKind::Precise
        );
        assert_eq!(
            TimerKind::for_interval(Duration::from_millis(20)),
            TimerKind::Timer
        );
        assert_eq!(
            TimerKind::for_interval(Duration::from_secs(1)),
            TimerKind::Timer
        );
        assert_eq!(CURSOR_BLINK_PERIOD * 2, Duration::from_secs(1));
    }
}
