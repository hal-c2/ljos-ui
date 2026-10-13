//! What scrolled content does when nothing holds it: flings, overshoot
//! and rebound (ARCHITECTURE §10.3, "`ScrollPhysics` as data").
//!
//! A [`ScrollPhysics`] is asked at the moments content can start to move
//! by itself: the finger let go, the wheel turned, `flick()` was called,
//! the content was found outside its bounds. Its answer is a [`Motion`]:
//! stay, be there now, or follow a [`Segment`]. A segment is a curve in
//! time, worked out once; when it ends the physics is asked again
//! ([`ScrollPhysics::next`]) from where it left the content. So the
//! physics is plain data and pure functions, and the frame driver
//! (`fling.rs`) only reads curves.
//!
//! [`Kinetic::DESKTOP`] has the values a running Qt 6.11 shows, measured
//! frame by frame on `qml6` (Flickable's `flick()`, drags let go at known
//! speeds, pulls past the ends).

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
    /// It moves by itself along this curve, which starts now.
    Start(Segment),
}

/// What a Flickable's own properties say about moving by itself
/// (`flickDeceleration`, `maximumFlickVelocity`, `boundsBehavior`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlickSettings {
    /// Pixels per second per second a fling slows down by.
    pub deceleration: f64,
    /// The fastest a fling starts, in pixels per second.
    pub maximum_velocity: f64,
    /// A drag may pull the content past its bounds (`DragOverBounds`).
    pub drag_over: bool,
    /// A fling may carry the content past its bounds (`OvershootBounds`).
    pub overshoot: bool,
}

impl Default for FlickSettings {
    fn default() -> Self {
        Self {
            deceleration: 1500.0,
            maximum_velocity: 2500.0,
            drag_over: true,
            overshoot: true,
        }
    }
}

/// One piece of content moving by itself: a curve of position in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Slowing down evenly from `from` at `velocity` (pixels per second,
    /// positive towards the end) by `rate`, for `duration`. A segment that
    /// is cut short by a bound ends before the content stops.
    Decelerate {
        from: f64,
        velocity: f64,
        rate: f64,
        duration: Duration,
    },
    /// Back from past a bound to the bound: speeding up to halfway, then
    /// settling exponentially ([`Kinetic::rebound_in`] and `rebound_out`).
    Rebound { from: f64, to: f64 },
    /// Easing to `to`, fast first: a wheel that glides.
    Glide {
        from: f64,
        to: f64,
        duration: Duration,
    },
    /// Straight to `to`: content that a fling carried as far past a
    /// bound as it may go is put back at once (measured on qml6).
    Snap { to: f64 },
}

/// `1 - 2^(-10x)`, which reaches 1 at `x = 1` (it is 0.999 there).
fn out_expo(x: f64) -> f64 {
    if x >= 1.0 {
        1.0
    } else {
        1.0 - 2f64.powf(-10.0 * x)
    }
}

fn out_expo_slope(x: f64) -> f64 {
    if x >= 1.0 {
        0.0
    } else {
        10.0 * std::f64::consts::LN_2 * 2f64.powf(-10.0 * x)
    }
}

impl Segment {
    /// How long it lasts.
    pub fn duration(&self, k: &Kinetic) -> Duration {
        match *self {
            Segment::Decelerate { duration, .. } | Segment::Glide { duration, .. } => duration,
            Segment::Rebound { .. } => k.rebound_in + k.rebound_out,
            Segment::Snap { .. } => Duration::ZERO,
        }
    }

    /// Position and velocity `t` after it started; at its end once `t` is
    /// past its duration.
    pub fn at(&self, k: &Kinetic, t: Duration) -> (f64, f64) {
        let t = t.min(self.duration(k));
        match *self {
            Segment::Decelerate {
                from,
                velocity,
                rate,
                ..
            } => {
                let dir = velocity.signum();
                let stop = if rate > 0.0 {
                    velocity.abs() / rate
                } else {
                    f64::INFINITY
                };
                let mut s = t.as_secs_f64();
                // A duration is whole nanoseconds: a segment that runs to
                // the stop ends at rest, not a rounding short of it.
                if t >= self.duration(k) && stop - s < 1e-6 {
                    s = stop;
                }
                let s = s.min(stop);
                let v = if s >= stop {
                    0.0
                } else {
                    velocity - dir * rate * s
                };
                (from + velocity * s - dir * rate * s * s / 2.0, v)
            }
            Segment::Rebound { from, to } => {
                let mid = (from + to) / 2.0;
                let first = k.rebound_in.as_secs_f64().max(f64::EPSILON);
                let second = k.rebound_out.as_secs_f64().max(f64::EPSILON);
                let s = t.as_secs_f64();
                if s < first {
                    let x = s / first;
                    (from + (mid - from) * x * x, (mid - from) * 2.0 * x / first)
                } else if t >= self.duration(k) {
                    (to, 0.0)
                } else {
                    let x = (s - first) / second;
                    (
                        mid + (to - mid) * out_expo(x),
                        (to - mid) * out_expo_slope(x) / second,
                    )
                }
            }
            Segment::Snap { to } => (to, 0.0),
            Segment::Glide { from, to, duration } => {
                let d = duration.as_secs_f64().max(f64::EPSILON);
                if t >= duration {
                    return (to, 0.0);
                }
                let x = t.as_secs_f64() / d;
                (
                    from + (to - from) * out_expo(x),
                    (to - from) * out_expo_slope(x) / d,
                )
            }
        }
    }
}

/// The values of a physics in which content keeps moving when it is let
/// go, slows down evenly, and gives way past its bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kinetic {
    /// Past a bound a fling slows down this many times faster.
    pub overshoot_brake: f64,
    /// The furthest a fling carries the content past a bound, in pixels.
    pub max_overshoot: f64,
    /// Dragged past a bound, the content moves this share of what the
    /// pointer moves.
    pub drag_resistance: f64,
    /// The rebound reaches halfway in this long, speeding up.
    pub rebound_in: Duration,
    /// Then it settles in this long.
    pub rebound_out: Duration,
    /// A pointer that rested this long before it let go starts no fling.
    pub release_rest: Duration,
    /// How long the content glides to where a wheel turn sends it; `None`
    /// puts it there at once.
    pub wheel_glide: Option<Duration>,
}

impl Kinetic {
    /// A desktop Qt 6.11's values. The wheel does not glide: Qt eases a
    /// notch over about 300 ms; Ljós puts it there at once until the
    /// scenarios that read the wheel's end position wait for it.
    pub const DESKTOP: Kinetic = Kinetic {
        overshoot_brake: 8.0,
        max_overshoot: 150.0,
        drag_resistance: 0.5,
        rebound_in: Duration::from_millis(100),
        rebound_out: Duration::from_millis(300),
        release_rest: Duration::from_millis(100),
        wheel_glide: None,
    };
}

impl Default for Kinetic {
    fn default() -> Self {
        Kinetic::DESKTOP
    }
}

/// Positions this close to a bound are at it: the end of a curve that was
/// cut at a bound must not start another one a hair short of it.
const AT_BOUND: f64 = 1e-6;

impl Kinetic {
    /// The curve the content follows from `axis` (position and velocity
    /// now); `None` when it is inside its bounds and still.
    pub fn plan(&self, axis: &AxisState, s: &FlickSettings) -> Option<Segment> {
        let (min, max) = (axis.min, axis.max.max(axis.min));
        let mut p = axis.position;
        if (p - min).abs() < AT_BOUND {
            p = min;
        } else if (p - max).abs() < AT_BOUND {
            p = max;
        }
        let v = axis.velocity;
        let outside = p < min || p > max;
        let near = p.clamp(min, max);
        let rebound = || outside.then_some(Segment::Rebound { from: p, to: near });
        let rate = s.deceleration.max(1.0);
        let outward = (p >= max && v > 0.0) || (p <= min && v < 0.0);
        if outward {
            if !s.overshoot {
                return rebound();
            }
            let brake = rate * self.overshoot_brake.max(1.0);
            let room = self.max_overshoot - (p - near).abs();
            if room <= AT_BOUND {
                // Still going out at the furthest it may: no rebound.
                return outside.then_some(Segment::Snap { to: near });
            }
            let speed = v.abs();
            let stop = speed * speed / (2.0 * brake);
            let secs = if stop <= room {
                speed / brake
            } else {
                (speed - (speed * speed - 2.0 * brake * room).max(0.0).sqrt()) / brake
            };
            return Some(Segment::Decelerate {
                from: p,
                velocity: v,
                rate: brake,
                duration: Duration::from_secs_f64(secs),
            });
        }
        if v == 0.0 {
            return rebound();
        }
        let speed = v.abs();
        let reach = speed * speed / (2.0 * rate);
        let end = p + v.signum() * reach;
        if outside && ((p > max && end >= max) || (p < min && end <= min)) {
            // Too slow to come back in by itself.
            return rebound();
        }
        let far = if v > 0.0 { max } else { min };
        let secs = if (v > 0.0 && end > far) || (v < 0.0 && end < far) {
            let d = (far - p).abs();
            (speed - (speed * speed - 2.0 * rate * d).max(0.0).sqrt()) / rate
        } else {
            speed / rate
        };
        Some(Segment::Decelerate {
            from: p,
            velocity: v,
            rate,
            duration: Duration::from_secs_f64(secs),
        })
    }
}

/// How scrolled content moves by itself. Data, so a platform's values can
/// be given, compared and stored.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ScrollPhysics {
    /// Content follows the finger and the wheel and stops when they stop.
    /// It never leaves its bounds by being dragged.
    Immediate,
    /// Content keeps going when it is let go, slows down, and gives way
    /// past its bounds as [`FlickSettings`] allow. The default.
    Kinetic(Kinetic),
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        ScrollPhysics::Kinetic(Kinetic::DESKTOP)
    }
}

impl ScrollPhysics {
    /// The kinetic values, if content moves by itself.
    pub fn kinetic(&self) -> Option<&Kinetic> {
        match self {
            ScrollPhysics::Immediate => None,
            ScrollPhysics::Kinetic(k) => Some(k),
        }
    }

    /// Where a drag that asks for `wanted` puts the content. Resistance
    /// past the bounds is decided here.
    pub fn dragged(&self, axis: &AxisState, wanted: f64, s: &FlickSettings) -> f64 {
        match self {
            ScrollPhysics::Kinetic(k) if s.drag_over => {
                let (min, max) = (axis.min, axis.max.max(axis.min));
                if wanted < min {
                    min - (min - wanted) * k.drag_resistance
                } else if wanted > max {
                    max + (wanted - max) * k.drag_resistance
                } else {
                    wanted
                }
            }
            _ => axis.clamp(wanted),
        }
    }

    /// What a drag would have to ask for to put the content where it is
    /// ([`ScrollPhysics::dragged`] backwards), so that a drag that starts
    /// past a bound does not jump.
    pub fn undragged(&self, axis: &AxisState, s: &FlickSettings) -> f64 {
        let p = axis.position;
        match self {
            ScrollPhysics::Kinetic(k) if s.drag_over && k.drag_resistance > 0.0 => {
                let (min, max) = (axis.min, axis.max.max(axis.min));
                if p < min {
                    min - (min - p) / k.drag_resistance
                } else if p > max {
                    max + (p - max) / k.drag_resistance
                } else {
                    p
                }
            }
            _ => p,
        }
    }

    /// The finger let go, with `axis.velocity`: a fling, a rebound, or
    /// nothing.
    pub fn released(&self, axis: &AxisState, s: &FlickSettings) -> Motion {
        match self {
            ScrollPhysics::Immediate => Motion::Rest,
            ScrollPhysics::Kinetic(k) => {
                let mut a = *axis;
                let cap = s.maximum_velocity.max(0.0);
                a.velocity = a.velocity.clamp(-cap, cap);
                k.plan(&a, s).map_or(Motion::Rest, Motion::Start)
            }
        }
    }

    /// What follows a segment that ended at `axis`; `None` when the
    /// content comes to rest there.
    pub fn next(&self, axis: &AxisState, s: &FlickSettings) -> Option<Segment> {
        self.kinetic().and_then(|k| k.plan(axis, s))
    }

    /// The wheel asks the content to move by `distance` (positive: towards
    /// the end). Content already past a bound is not moved further out,
    /// and not pulled in either: only something that asks for the bounds
    /// ([`ScrollPhysics::settle`]) does that.
    pub fn wheel(&self, axis: &AxisState, distance: f64) -> Motion {
        let p = axis.position;
        let max = axis.max.max(axis.min);
        let to = if distance > 0.0 {
            (p + distance).min(max.max(p))
        } else {
            (p + distance).max(axis.min.min(p))
        };
        if to == p {
            return Motion::Rest;
        }
        match self.kinetic().and_then(|k| k.wheel_glide) {
            Some(duration) => Motion::Start(Segment::Glide {
                from: p,
                to,
                duration,
            }),
            None => Motion::Jump(to),
        }
    }

    /// The content is asked back inside its bounds (a content size that
    /// changed): at once.
    pub fn settle(&self, axis: &AxisState) -> Motion {
        let to = axis.clamp(axis.position);
        if to == axis.position {
            Motion::Rest
        } else {
            Motion::Jump(to)
        }
    }

    /// `returnToBounds()`: the rebound, or at once without motion.
    pub fn return_to_bounds(&self, axis: &AxisState) -> Motion {
        match self {
            ScrollPhysics::Kinetic(_) if axis.clamp(axis.position) != axis.position => {
                Motion::Start(Segment::Rebound {
                    from: axis.position,
                    to: axis.clamp(axis.position),
                })
            }
            _ => self.settle(axis),
        }
    }

    /// How long a pointer may rest before it lets go and still fling.
    pub fn release_rest(&self) -> Duration {
        match self {
            ScrollPhysics::Immediate => Duration::ZERO,
            ScrollPhysics::Kinetic(k) => k.release_rest,
        }
    }

    /// How long after the last turn of a wheel without phases the content
    /// counts as moving. A wheel does not say when it stopped.
    pub fn wheel_rest(&self) -> Duration {
        let rest = Duration::from_millis(100);
        match self.kinetic().and_then(|k| k.wheel_glide) {
            Some(glide) => glide.max(rest),
            None => rest,
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

    /// The time of the newest sample.
    pub fn last_time(&self) -> Option<Time> {
        (self.len > 0).then(|| self.samples[(self.next + SAMPLES - 1) % SAMPLES].0)
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

    const S: FlickSettings = FlickSettings {
        deceleration: 1500.0,
        maximum_velocity: 2500.0,
        drag_over: true,
        overshoot: true,
    };
    const K: Kinetic = Kinetic::DESKTOP;

    fn moving(position: f64, velocity: f64) -> AxisState {
        AxisState {
            velocity,
            ..axis(position)
        }
    }

    fn end(seg: &Segment) -> (f64, f64) {
        seg.at(&K, seg.duration(&K))
    }

    #[test]
    fn a_drag_stops_at_the_bounds() {
        let p = ScrollPhysics::Immediate;
        assert_eq!(p.dragged(&axis(10.0), -40.0, &S), 0.0);
        assert_eq!(p.dragged(&axis(10.0), 120.0, &S), 120.0);
        assert_eq!(p.dragged(&axis(10.0), 900.0, &S), 300.0);
    }

    #[test]
    fn letting_go_leaves_the_content_where_it_is() {
        let mut a = axis(50.0);
        a.velocity = 4000.0;
        assert_eq!(ScrollPhysics::Immediate.released(&a, &S), Motion::Rest);
    }

    #[test]
    fn the_desktop_is_the_default() {
        assert_eq!(ScrollPhysics::default(), ScrollPhysics::Kinetic(K));
    }

    #[test]
    fn past_a_bound_a_drag_moves_the_content_half_as_far() {
        let p = ScrollPhysics::default();
        assert_eq!(p.dragged(&axis(10.0), -90.0, &S), -45.0);
        assert_eq!(p.dragged(&axis(10.0), 400.0, &S), 350.0);
        assert_eq!(p.dragged(&axis(10.0), 120.0, &S), 120.0);
        let stop = FlickSettings {
            drag_over: false,
            ..S
        };
        assert_eq!(p.dragged(&axis(10.0), -90.0, &stop), 0.0);
    }

    #[test]
    fn a_fling_goes_v_squared_over_twice_the_deceleration() {
        // flick(0, -800) on qml6 rests 213.33 further on.
        let a = AxisState {
            max: 5000.0,
            ..moving(0.0, 800.0)
        };
        let seg = K.plan(&a, &S).expect("a fling");
        let (at, v) = end(&seg);
        assert!((at - 213.333).abs() < 0.01, "{at}");
        assert!(v.abs() < 1e-9);
        assert_eq!(seg.duration(&K), Duration::from_secs_f64(800.0 / 1500.0));
        // A quarter of the way in time it has gone 7/16 of the way.
        let (q, _) = seg.at(&K, Duration::from_secs_f64(800.0 / 1500.0 / 4.0));
        assert!((q - 213.333 * 7.0 / 16.0).abs() < 0.01, "{q}");
    }

    #[test]
    fn a_fast_release_is_capped_at_the_maximum_velocity() {
        // A 3000 px/s drag on qml6 goes 2083.33 on, as 2500 px/s does.
        let a = AxisState {
            max: 9000.0,
            ..moving(120.0, 3000.0)
        };
        let Motion::Start(seg) = ScrollPhysics::default().released(&a, &S) else {
            panic!("no fling");
        };
        assert!((end(&seg).0 - 2203.33).abs() < 0.01);
    }

    #[test]
    fn a_fling_that_stops_at_bounds_is_cut_there() {
        let stop = FlickSettings {
            overshoot: false,
            ..S
        };
        let seg = K.plan(&moving(250.0, 1200.0), &stop).expect("a fling");
        let (at, v) = end(&seg);
        assert!((at - 300.0).abs() < 1e-6 && v > 0.0, "{at} {v}");
        // At the bound and still going out: nothing follows.
        assert_eq!(K.plan(&moving(at, v), &stop), None);
    }

    #[test]
    fn past_a_bound_a_fling_brakes_hard_then_comes_back() {
        let seg = K.plan(&moving(300.0, 1200.0), &S).expect("a brake");
        let (at, v) = end(&seg);
        // 1200² / (2 · 8 · 1500) = 60 past the end.
        assert!((at - 360.0).abs() < 1e-6 && v.abs() < 1e-9, "{at} {v}");
        let back = K.plan(&moving(at, v), &S).expect("a rebound");
        assert_eq!(
            back,
            Segment::Rebound {
                from: at,
                to: 300.0
            }
        );
        assert_eq!(back.duration(&K), Duration::from_millis(400));
        let (half, _) = back.at(&K, Duration::from_millis(100));
        assert!((half - 330.0).abs() < 1e-9);
        assert_eq!(end(&back), (300.0, 0.0));
        assert_eq!(K.plan(&moving(300.0, 0.0), &S), None);
    }

    #[test]
    fn overshoot_is_never_more_than_150() {
        let seg = K.plan(&moving(300.0, 9000.0), &S).expect("a brake");
        let (at, v) = end(&seg);
        assert!((at - 450.0).abs() < 1e-6, "{at}");
        assert!(v > 0.0);
        // qml6 puts it back at the bound at once from there.
        assert_eq!(
            K.plan(&moving(at, v), &S),
            Some(Segment::Snap { to: 300.0 })
        );
    }

    #[test]
    fn content_let_go_outside_comes_back() {
        assert_eq!(
            K.plan(&moving(-45.0, 0.0), &S),
            Some(Segment::Rebound {
                from: -45.0,
                to: 0.0
            })
        );
        // Thrown back in fast enough, it flings in.
        assert!(matches!(
            K.plan(&moving(-45.0, 900.0), &S),
            Some(Segment::Decelerate { .. })
        ));
    }

    #[test]
    fn return_to_bounds_is_a_rebound_or_nothing() {
        let p = ScrollPhysics::default();
        assert!(matches!(
            p.return_to_bounds(&axis(900.0)),
            Motion::Start(Segment::Rebound { .. })
        ));
        assert_eq!(p.return_to_bounds(&axis(5.0)), Motion::Rest);
        assert_eq!(
            ScrollPhysics::Immediate.return_to_bounds(&axis(900.0)),
            Motion::Jump(300.0)
        );
    }

    #[test]
    fn a_wheel_that_glides_eases_out() {
        let glide = ScrollPhysics::Kinetic(Kinetic {
            wheel_glide: Some(Duration::from_millis(300)),
            ..K
        });
        let Motion::Start(seg) = glide.wheel(&axis(0.0), 72.0) else {
            panic!("no glide");
        };
        // qml6 shows 26.67 of 72 after 21 ms of a notch.
        let (at, _) = seg.at(&K, Duration::from_millis(21));
        assert!((at - 72.0 * (1.0 - 2f64.powf(-0.7))).abs() < 1e-9);
        assert_eq!(end(&seg), (72.0, 0.0));
        assert_eq!(glide.wheel_rest(), Duration::from_millis(300));
    }

    #[test]
    fn the_newest_sample_is_known() {
        let mut v = VelocityTracker::default();
        assert_eq!(v.last_time(), None);
        for i in 0..20u64 {
            v.push(Time::from_millis(i), 0.0);
        }
        assert_eq!(v.last_time(), Some(Time::from_millis(19)));
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
