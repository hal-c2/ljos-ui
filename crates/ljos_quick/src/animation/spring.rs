//! The way a `SpringAnimation` takes: pulled towards the value, past it
//! and back until it rests there.
//!
//! The Qt documentation names the properties and gives no formula.
//! Everything here was measured on qml6 (the recorded values are in
//! `fixtures/spring_qt.txt`, and the test below holds this module to
//! them):
//!
//! - With a `spring` the property moves in steps of 16 milliseconds,
//!   however far apart the frames are: a frame shows as many steps as fit
//!   in the time since the animation began. Each step first changes the
//!   speed by `(spring * distance - damping * speed) / mass`, with the
//!   speed in units per second, then limits it to `velocity` if there is
//!   one, then moves the property by 16 milliseconds at that speed.
//! - It rests, at the value itself, after the first step that leaves both
//!   the speed and the distance below `epsilon`.
//! - Without a `spring` it moves at `velocity`, by the time that has
//!   passed, and without that either it leaves the property alone.
//! - A `modulus` makes the value wrap round and the shorter way the one
//!   taken. A spring rests at the wrapped value; at a plain velocity the
//!   property ends at the value as it was given.
//! - It begins at the speed the property already has.

use super::timeline::FOREVER;

/// One step of a spring, in microseconds and in seconds.
pub(crate) const STEP_MICROS: i64 = 16_000;
const STEP_SECS: f64 = 0.016;

/// A spring that has not come to rest after this many steps (an hour) is
/// taken to swing without end, which is what one without damping does.
const MAX_STEPS: i64 = 225_000;

/// What the animation object says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Params {
    pub spring: f64,
    pub damping: f64,
    pub mass: f64,
    /// Units per second; zero or less for "no limit".
    pub velocity: f64,
    pub epsilon: f64,
    /// Zero or less for none.
    pub modulus: f64,
}

impl Params {
    /// How far it is from `x` to `to`, the short way round a modulus.
    fn distance(&self, to: f64, x: f64) -> f64 {
        let d = to - x;
        if self.modulus > 0.0 {
            let m = self.modulus;
            let d = d % m;
            if d > m / 2.0 {
                d - m
            } else if d < -m / 2.0 {
                d + m
            } else {
                d
            }
        } else {
            d
        }
    }

    fn wrapped(&self, x: f64) -> f64 {
        if self.modulus > 0.0 {
            x.rem_euclid(self.modulus)
        } else {
            x
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Way {
    /// Pulled by the spring. `steps`: how many it takes to come to rest,
    /// `None` when it never does.
    Spring { steps: Option<i64> },
    /// At one speed (signed like the values) for `secs` seconds.
    Even { speed: f64, secs: f64 },
}

/// One property's way to its value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Track {
    pub from: f64,
    pub to: f64,
    v0: f64,
    p: Params,
    way: Way,
    /// Where the spring is after `step` steps, and how fast. Kept so that
    /// a frame continues from the frame before instead of from the start.
    step: i64,
    x: f64,
    v: f64,
}

impl Track {
    /// The way from `from` to `to` for a property moving at `speed` (units
    /// per second, signed like the values) when it sets out. `None`: the
    /// animation leaves the property alone, which is what one without a
    /// spring and without a velocity does.
    pub(crate) fn plan(from: f64, to: f64, speed: f64, p: &Params) -> Option<Track> {
        if !from.is_finite() || !to.is_finite() {
            return None;
        }
        let mut p = *p;
        if p.mass.is_nan() || p.mass <= 0.0 {
            p.mass = 1.0;
        }
        let v0 = if speed.is_finite() { speed } else { 0.0 };
        let way = if p.spring > 0.0 {
            Way::Spring { steps: None }
        } else if p.velocity > 0.0 {
            let d = p.distance(to, from);
            Way::Even {
                speed: p.velocity.copysign(d),
                secs: d.abs() / p.velocity,
            }
        } else {
            return None;
        };
        let mut track = Track {
            from,
            to,
            v0,
            p,
            way,
            step: 0,
            x: from,
            v: v0,
        };
        if matches!(way, Way::Spring { .. }) {
            let mut steps = None;
            for n in 1..=MAX_STEPS {
                if track.advance() {
                    steps = Some(n);
                    break;
                }
            }
            track.way = Way::Spring { steps };
            track.rewind();
        }
        Some(track)
    }

    fn rewind(&mut self) {
        self.step = 0;
        self.x = self.from;
        self.v = self.v0;
    }

    /// One step of the spring. Returns whether it rests after it.
    fn advance(&mut self) -> bool {
        let p = &self.p;
        let pull = p.spring * p.distance(self.to, self.x);
        self.v += (pull - p.damping * self.v) / p.mass;
        if p.velocity > 0.0 {
            self.v = self.v.clamp(-p.velocity, p.velocity);
        }
        self.x = p.wrapped(self.x + self.v * STEP_SECS);
        self.step += 1;
        // A value that got out of hand (a negative damping, say) ends the
        // way instead of showing nonsense for an hour.
        let lost = !self.x.is_finite() || !self.v.is_finite();
        lost || (self.v.abs() < p.epsilon && p.distance(self.to, self.x).abs() < p.epsilon)
    }

    /// How long the way takes; [`FOREVER`] for a spring that never rests.
    pub(crate) fn micros(&self) -> i64 {
        match self.way {
            Way::Spring { steps: Some(n) } => n.saturating_mul(STEP_MICROS),
            Way::Spring { steps: None } => FOREVER,
            Way::Even { secs, .. } => (secs * 1e6).ceil() as i64,
        }
    }

    /// Where the property is when it is there.
    pub(crate) fn rest(&self) -> f64 {
        match self.way {
            Way::Spring { .. } => self.p.wrapped(self.to),
            // Measured on Qt: at a plain velocity the value is not wrapped
            // in the end, though it is on the way.
            Way::Even { .. } => self.to,
        }
    }

    /// The value and the speed (signed like the values) `micros` after it
    /// began.
    pub(crate) fn at(&mut self, micros: i64) -> (f64, f64) {
        if micros >= self.micros() {
            return (self.rest(), 0.0);
        }
        match self.way {
            Way::Even { speed, .. } => {
                let t = micros.max(0) as f64 / 1e6;
                (self.p.wrapped(self.from + speed * t), speed)
            }
            Way::Spring { .. } => {
                let n = micros.max(0) / STEP_MICROS;
                if n < self.step {
                    self.rewind();
                }
                while self.step < n {
                    self.advance();
                }
                (self.x, self.v)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// qml6 prints five decimals; its ticks are whole milliseconds.
    const TOLERANCE: f64 = 2e-5;

    fn spring(spring: f64, damping: f64) -> Params {
        Params {
            spring,
            damping,
            mass: 1.0,
            velocity: 0.0,
            epsilon: 0.01,
            modulus: 0.0,
        }
    }

    /// Each line of the fixture is one run on qml6 of a property with a
    /// `Behavior` holding a `SpringAnimation`: spring, damping, mass,
    /// velocity, epsilon, modulus and the value the property starts at.
    /// Then, after each `|`, a value the property was set to and what Qt
    /// showed from there on as `milliseconds:value`; `end:` is the tick
    /// at which Qt said the animation was over (`-`: it was still going).
    #[test]
    fn the_way_is_the_one_qt_takes() {
        let fixture = include_str!("fixtures/spring_qt.txt");
        let mut checked = 0;
        for line in fixture.lines().filter(|l| !l.trim().is_empty()) {
            let mut words = line.split_whitespace();
            let mut head = || words.next().unwrap().parse::<f64>().unwrap();
            let p = Params {
                spring: head(),
                damping: head(),
                mass: head(),
                velocity: head(),
                epsilon: head(),
                modulus: head(),
            };
            let start = head();
            let mut track: Option<Track> = None;
            let mut began = 0;
            // The tick, the value and the speed last shown.
            let mut last = (0, start, 0.0);
            while let Some(word) = words.next() {
                if word == "|" {
                    // The new way begins where the last tick left the
                    // property, at the speed it had then.
                    let to = words.next().unwrap().parse::<f64>().unwrap();
                    track = Track::plan(last.1, to, last.2, &p);
                    began = last.0;
                    continue;
                }
                let track = track.as_mut().unwrap();
                if let Some(end) = word.strip_prefix("end:") {
                    if let Ok(end) = end.parse::<i64>() {
                        // Qt notices at the first tick at or after the end.
                        let over = began * 1000 + track.micros();
                        assert!(
                            over <= end * 1000 && over > (end - 18) * 1000,
                            "{line:.40}: Qt is there at {end} ms, this at {over} µs"
                        );
                    } else {
                        assert!(track.micros() > (last.0 - began) * 1000, "{line:.40}");
                    }
                    continue;
                }
                let (ms, value) = word.split_once(':').unwrap();
                let ms = ms.parse::<i64>().unwrap();
                let value = value.parse::<f64>().unwrap();
                let got = track.at((ms - began) * 1000);
                assert!(
                    (got.0 - value).abs() <= TOLERANCE,
                    "{line:.40}: at {ms} ms Qt shows {value}, this shows {}",
                    got.0
                );
                last = (ms, got.0, got.1);
                checked += 1;
            }
        }
        assert!(checked > 1000, "only {checked} values in the fixture");
    }

    #[test]
    fn without_a_spring_and_a_velocity_the_property_is_left_alone() {
        assert_eq!(Track::plan(0.0, 100.0, 0.0, &spring(0.0, 0.2)), None);
    }

    #[test]
    fn the_end_is_the_value_itself_and_a_standstill() {
        let mut track = Track::plan(0.1, 0.3, 0.0, &spring(2.0, 0.2)).unwrap();
        let end = track.micros();
        assert!(end < FOREVER);
        assert_ne!(track.at(end - 1).0, 0.3);
        assert_eq!(track.at(end), (0.3, 0.0));
    }

    #[test]
    fn a_frame_earlier_than_the_last_one_shows_what_it_showed_then() {
        let mut track = Track::plan(0.0, 100.0, 0.0, &spring(2.0, 0.2)).unwrap();
        let early = track.at(5 * STEP_MICROS);
        track.at(40 * STEP_MICROS);
        assert_eq!(track.at(5 * STEP_MICROS), early);
    }

    #[test]
    fn a_spring_without_damping_never_rests() {
        let track = Track::plan(0.0, 100.0, 0.0, &spring(2.0, 0.0)).unwrap();
        assert_eq!(track.micros(), FOREVER);
    }

    #[test]
    fn a_mass_of_nothing_counts_as_one() {
        let mut odd = spring(2.0, 0.2);
        odd.mass = 0.0;
        let mut a = Track::plan(0.0, 100.0, 0.0, &odd).unwrap();
        let mut b = Track::plan(0.0, 100.0, 0.0, &spring(2.0, 0.2)).unwrap();
        assert_eq!(a.at(10 * STEP_MICROS), b.at(10 * STEP_MICROS));
    }
}
