//! What scrolled content does when nothing holds it: the seam for flings,
//! overshoot and rebound (ARCHITECTURE §10.3, "`ScrollPhysics` as data").
//!
//! A [`ScrollPhysics`] is asked at the three moments content can start to
//! move by itself: the finger let go, the wheel turned, the content was
//! found outside its bounds. Its answer is a [`Motion`]. Today there is
//! one physics, [`ScrollPhysics::Immediate`], whose answers are "stay" and
//! "be there now": content stops when the finger or the wheel stops. A
//! fling is a further `Motion` and a further physics, and nothing that
//! asks has to change.

use ljos_engine::Engine;
use ljos_runtime::Time;
use std::cell::Cell;
use std::time::Duration;

/// How far one notch of a mouse wheel scrolls, in logical pixels: three
/// lines of 24, which is what Qt 6.11 scrolls with the desktop's default
/// of three lines per notch.
pub const WHEEL_NOTCH_PIXELS: f64 = 72.0;

/// One direction of a scrollable surface, as the physics sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisState {
    /// `contentX` or `contentY`.
    pub position: f64,
    /// The position at the beginning.
    pub min: f64,
    /// The position at the end; never less than `min`.
    pub max: f64,
    /// How fast the content was moving when it was let go, in pixels per
    /// second. Zero when nothing dragged it.
    pub velocity: f64,
}

impl AxisState {
    /// `v` brought inside the bounds.
    pub fn clamp(&self, v: f64) -> f64 {
        v.clamp(self.min, self.max.max(self.min))
    }
}

/// What the content does next.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Motion {
    /// It stays where it is.
    Rest,
    /// It is at this position at once.
    Jump(f64),
}

/// How scrolled content moves by itself. Data, so a platform's values can
/// be given, compared and stored.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum ScrollPhysics {
    /// Content follows the finger and the wheel and stops when they stop.
    /// It never leaves its bounds by being dragged.
    #[default]
    Immediate,
}

impl ScrollPhysics {
    /// Where a drag that asks for `wanted` puts the content. Resistance
    /// past the bounds is decided here.
    pub fn dragged(&self, axis: &AxisState, wanted: f64) -> f64 {
        match self {
            ScrollPhysics::Immediate => axis.clamp(wanted),
        }
    }

    /// The finger let go, with `axis.velocity`.
    pub fn released(&self, _axis: &AxisState) -> Motion {
        match self {
            ScrollPhysics::Immediate => Motion::Rest,
        }
    }

    /// The wheel asks the content to move by `distance` (positive: towards
    /// the end). Content already past a bound is not moved further out,
    /// and not pulled in either: only something that asks for the bounds
    /// ([`ScrollPhysics::settle`]) does that.
    pub fn wheel(&self, axis: &AxisState, distance: f64) -> Motion {
        match self {
            ScrollPhysics::Immediate => {
                let p = axis.position;
                let max = axis.max.max(axis.min);
                let to = if distance > 0.0 {
                    (p + distance).min(max.max(p))
                } else {
                    (p + distance).max(axis.min.min(p))
                };
                if to == p {
                    Motion::Rest
                } else {
                    Motion::Jump(to)
                }
            }
        }
    }

    /// The content is asked back inside its bounds (`returnToBounds`, a
    /// content size that changed).
    pub fn settle(&self, axis: &AxisState) -> Motion {
        match self {
            ScrollPhysics::Immediate => {
                let to = axis.clamp(axis.position);
                if to == axis.position {
                    Motion::Rest
                } else {
                    Motion::Jump(to)
                }
            }
        }
    }

    /// How long after the last turn of a wheel without phases the content
    /// counts as moving. A wheel does not say when it stopped.
    pub fn wheel_rest(&self) -> Duration {
        match self {
            ScrollPhysics::Immediate => Duration::from_millis(100),
        }
    }
}

#[derive(Default)]
struct Chosen(Cell<ScrollPhysics>);

/// The physics every `Flickable` of this engine uses.
pub fn scroll_physics(engine: &Engine) -> ScrollPhysics {
    engine.extension::<Chosen>().0.get()
}

/// Chooses the physics for every `Flickable` of this engine: what a host
/// does once, with its platform's values.
pub fn set_scroll_physics(engine: &Engine, physics: ScrollPhysics) {
    engine.extension::<Chosen>().0.set(physics);
}

/// How long back a velocity is measured over.
const WINDOW: Duration = Duration::from_millis(100);
const SAMPLES: usize = 16;

/// The speed of a dragged position: the slope of the least-squares line
/// through the samples of the last 100 ms (ARCHITECTURE §10.3). One slow
/// or late event then does not decide how far a fling goes.
#[derive(Clone, Copy, Debug)]
pub struct VelocityTracker {
    samples: [(Time, f64); SAMPLES],
    len: usize,
    next: usize,
}

impl Default for VelocityTracker {
    fn default() -> Self {
        Self {
            samples: [(Time::ZERO, 0.0); SAMPLES],
            len: 0,
            next: 0,
        }
    }
}

impl VelocityTracker {
    pub fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }

    pub fn push(&mut self, time: Time, position: f64) {
        self.samples[self.next] = (time, position);
        self.next = (self.next + 1) % SAMPLES;
        self.len = (self.len + 1).min(SAMPLES);
    }

    /// Pixels per second at `now`; zero with fewer than two samples in the
    /// window, or when they all have the same time.
    pub fn velocity(&self, now: Time) -> f64 {
        let recent: Vec<(f64, f64)> = self.samples[..self.len]
            .iter()
            .filter(|(t, _)| *t <= now && now.saturating_since(*t) <= WINDOW)
            .map(|(t, p)| (-(now.saturating_since(*t).as_secs_f64()), *p))
            .collect();
        if recent.len() < 2 {
            return 0.0;
        }
        let n = recent.len() as f64;
        let mt = recent.iter().map(|s| s.0).sum::<f64>() / n;
        let mp = recent.iter().map(|s| s.1).sum::<f64>() / n;
        let var: f64 = recent.iter().map(|s| (s.0 - mt).powi(2)).sum();
        if var <= f64::EPSILON {
            return 0.0;
        }
        recent.iter().map(|s| (s.0 - mt) * (s.1 - mp)).sum::<f64>() / var
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(position: f64) -> AxisState {
        AxisState {
            position,
            min: 0.0,
            max: 300.0,
            velocity: 0.0,
        }
    }

    #[test]
    fn a_drag_stops_at_the_bounds() {
        let p = ScrollPhysics::Immediate;
        assert_eq!(p.dragged(&axis(10.0), -40.0), 0.0);
        assert_eq!(p.dragged(&axis(10.0), 120.0), 120.0);
        assert_eq!(p.dragged(&axis(10.0), 900.0), 300.0);
    }

    #[test]
    fn letting_go_leaves_the_content_where_it_is() {
        let mut a = axis(50.0);
        a.velocity = 4000.0;
        assert_eq!(ScrollPhysics::Immediate.released(&a), Motion::Rest);
    }

    #[test]
    fn the_wheel_stops_at_the_bounds_and_says_when_nothing_moves() {
        let p = ScrollPhysics::Immediate;
        assert_eq!(p.wheel(&axis(0.0), 72.0), Motion::Jump(72.0));
        assert_eq!(p.wheel(&axis(280.0), 72.0), Motion::Jump(300.0));
        assert_eq!(p.wheel(&axis(300.0), 72.0), Motion::Rest);
        assert_eq!(p.wheel(&axis(0.0), -72.0), Motion::Rest);
        assert_eq!(p.wheel(&axis(30.0), -72.0), Motion::Jump(0.0));
    }

    #[test]
    fn the_wheel_does_not_pull_content_that_is_outside_back_in() {
        let p = ScrollPhysics::Immediate;
        assert_eq!(p.wheel(&axis(900.0), 72.0), Motion::Rest);
        assert_eq!(p.wheel(&axis(900.0), -72.0), Motion::Jump(828.0));
        assert_eq!(p.wheel(&axis(-50.0), -72.0), Motion::Rest);
    }

    #[test]
    fn settling_brings_the_content_inside() {
        let p = ScrollPhysics::Immediate;
        assert_eq!(p.settle(&axis(900.0)), Motion::Jump(300.0));
        assert_eq!(p.settle(&axis(-5.0)), Motion::Jump(0.0));
        assert_eq!(p.settle(&axis(5.0)), Motion::Rest);
    }

    #[test]
    fn content_smaller_than_the_view_has_one_place() {
        let a = AxisState {
            position: 30.0,
            min: 0.0,
            max: 0.0,
            velocity: 0.0,
        };
        assert_eq!(ScrollPhysics::Immediate.settle(&a), Motion::Jump(0.0));
    }

    #[test]
    fn velocity_is_the_slope_of_the_recent_samples() {
        let mut v = VelocityTracker::default();
        for i in 0..6u64 {
            v.push(Time::from_millis(i * 10), i as f64 * 5.0);
        }
        let got = v.velocity(Time::from_millis(50));
        assert!((got - 500.0).abs() < 1e-6, "{got}");
    }

    #[test]
    fn old_samples_do_not_count() {
        let mut v = VelocityTracker::default();
        v.push(Time::from_millis(0), 0.0);
        v.push(Time::from_millis(10), 100.0);
        assert_eq!(v.velocity(Time::from_millis(500)), 0.0);
        v.clear();
        assert_eq!(v.velocity(Time::from_millis(10)), 0.0);
    }
}
