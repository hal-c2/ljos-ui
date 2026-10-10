//! The way a `SmoothedAnimation` takes: up to a speed, on at that speed,
//! and down to a stop at the value.
//!
//! The Qt documentation names the properties and says little about the
//! motion. Everything here was measured on qml6 (the recorded values are
//! in `fixtures/smoothed_qt.txt`, and the test below holds this module to
//! them):
//!
//! - The time for the whole way is the distance over `velocity`, or the
//!   `duration` if that is shorter.
//! - Without a `maximumEasingTime` (or with one longer than the way takes)
//!   there is no stretch at one speed: it speeds up and slows down at the
//!   same rate, and from a standstill the two take half the time each.
//! - With one, speeding up and slowing down take half of it each, and the
//!   speed in between is whatever covers the distance in the time.
//! - It begins at the speed the property already has, which may point the
//!   other way ([`Reversing`]).

/// `SmoothedAnimation.reversingMode`: what a new value does that lies the
/// other way from where the property is heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reversing {
    /// Slows down, turns round and speeds up again.
    Eased,
    /// Turns round at once, from a standstill.
    Immediate,
    /// Takes the new value at once.
    Sync,
}

impl Reversing {
    pub(crate) fn of(mode: f64) -> Reversing {
        match mode as i32 {
            1 => Reversing::Immediate,
            2 => Reversing::Sync,
            _ => Reversing::Eased,
        }
    }
}

/// What the animation object says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Params {
    /// Units per second; negative for "none".
    pub velocity: f64,
    /// Milliseconds; negative for "none".
    pub duration: f64,
    /// Milliseconds; negative for "none".
    pub max_easing: f64,
    pub reversing: Reversing,
}

/// One property's way to its value. Distances are measured from `from`
/// towards `to`, times in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Track {
    pub from: f64,
    pub to: f64,
    /// 1 upwards, -1 downwards.
    dir: f64,
    /// The whole distance.
    s: f64,
    /// The speed it begins with; negative when it was heading away.
    v0: f64,
    /// The speed between speeding up and slowing down.
    vp: f64,
    /// How fast it speeds up, and how fast it slows down.
    a: f64,
    d: f64,
    /// When it stops speeding up, when it begins to slow down, and when it
    /// is there.
    tp: f64,
    td: f64,
    tf: f64,
}

impl Track {
    /// The way from `from` to `to` for a property moving at `speed` (units
    /// per second, signed like the values) when it sets out. `forced`
    /// fixes the time the way takes. `None`: the animation leaves the
    /// property alone, which is what a velocity of zero does.
    pub(crate) fn plan(
        from: f64,
        to: f64,
        speed: f64,
        p: &Params,
        forced: Option<f64>,
    ) -> Option<Track> {
        if p.velocity == 0.0 || !from.is_finite() || !to.is_finite() {
            return None;
        }
        let dir = if to < from { -1.0 } else { 1.0 };
        let s = (to - from).abs();
        let mut v0 = if speed.is_finite() { speed * dir } else { 0.0 };
        let mut tf = match forced {
            Some(t) => t,
            None => {
                let by_speed = (p.velocity > 0.0).then(|| s / p.velocity);
                let by_time = (p.duration >= 0.0).then(|| p.duration / 1000.0);
                match (by_speed, by_time) {
                    (Some(a), Some(b)) => a.min(b),
                    (Some(t), None) | (None, Some(t)) => t,
                    // Nothing says how long: it is there at once.
                    (None, None) => 0.0,
                }
            }
        };
        if v0 < 0.0 {
            match p.reversing {
                Reversing::Eased => {}
                Reversing::Immediate => v0 = 0.0,
                Reversing::Sync => tf = 0.0,
            }
        }
        let mut track = Track {
            from,
            to,
            dir,
            s,
            v0,
            vp: 0.0,
            a: 0.0,
            d: 0.0,
            tp: 0.0,
            td: 0.0,
            tf: if s > 0.0 && tf.is_finite() {
                tf.max(0.0)
            } else {
                0.0
            },
        };
        let tf = track.tf;
        if tf == 0.0 {
            return Some(track);
        }
        let easing = p.max_easing / 1000.0;
        if easing == 0.0 {
            // One speed all the way.
            track.vp = s / tf;
            track.td = tf;
        } else if easing > 0.0 && easing < tf {
            let tp = easing / 2.0;
            track.tp = tp;
            track.td = tf - tp;
            track.vp = (s - v0 * tp / 2.0) / (tf - tp);
            track.a = (track.vp - v0) / tp;
            track.d = track.vp / tp;
        } else {
            // No stretch at one speed: the top speed is the one that makes
            // speeding up to it and slowing down from it, at one rate,
            // cover the distance in the time.
            let root = (s * s - tf * s * v0 + tf * tf * v0 * v0 / 2.0)
                .max(0.0)
                .sqrt();
            track.vp = (s + root) / tf;
            track.a = (2.0 * track.vp - v0) / tf;
            track.d = track.a;
            // Qt works this out without asking whether it makes sense. A
            // property too fast to stop within the distance gets a turning
            // point before the start, and with it the small jump at the
            // start that Qt shows.
            track.tp = if track.a == 0.0 {
                0.0
            } else {
                (track.vp - v0) / track.a
            };
            track.td = track.tp;
        }
        Some(track)
    }

    /// How long the way takes, in microseconds.
    pub(crate) fn micros(&self) -> i64 {
        (self.tf * 1e6).ceil() as i64
    }

    /// Whether it is there at `t` seconds.
    pub(crate) fn is_over(&self, t: f64) -> bool {
        t >= self.tf
    }

    /// The value and the speed (signed like the values) at `t` seconds.
    pub(crate) fn at(&self, t: f64) -> (f64, f64) {
        if self.is_over(t) {
            return (self.to, 0.0);
        }
        let Track {
            v0,
            vp,
            a,
            d,
            tp,
            td,
            ..
        } = *self;
        let sp = v0 * tp + 0.5 * a * tp * tp;
        let (way, speed) = if t < tp {
            (v0 * t + 0.5 * a * t * t, v0 + a * t)
        } else if t < td {
            (sp + vp * (t - tp), vp)
        } else {
            let u = t - td;
            let sd = sp + vp * (td - tp);
            (sd + vp * u - 0.5 * d * u * u, vp - d * u)
        };
        (self.from + self.dir * way, self.dir * speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// qml6 prints five decimals; its ticks are whole milliseconds.
    const TOLERANCE: f64 = 2e-5;

    /// Each line of the fixture is one run on qml6 of a property with a
    /// `Behavior` holding a `SmoothedAnimation`: velocity, duration,
    /// maximumEasingTime, reversingMode, then when a second value was set
    /// (-1: never) and that value. The property starts at 0 and is set to
    /// 100. What follows are the values Qt showed, as `milliseconds:value`,
    /// with a `|` where the second value came.
    #[test]
    fn the_way_is_the_one_qt_takes() {
        let fixture = include_str!("fixtures/smoothed_qt.txt");
        let mut checked = 0;
        for line in fixture.lines().filter(|l| !l.trim().is_empty()) {
            let mut words = line.split_whitespace();
            let mut head = || words.next().unwrap().parse::<f64>().unwrap();
            let p = Params {
                velocity: head(),
                duration: head(),
                max_easing: head(),
                reversing: Reversing::of(head()),
            };
            let (_, second) = (head(), head());
            let mut track = Track::plan(0.0, 100.0, 0.0, &p, None).unwrap();
            let mut began = 0.0;
            let mut last = (0.0, (0.0, 0.0));
            for word in words {
                if word == "|" {
                    // The new way begins where the last tick left the
                    // property, at the speed it had then.
                    let (ms, (value, speed)) = last;
                    track = Track::plan(value, second, speed, &p, None).unwrap();
                    began = ms;
                    continue;
                }
                let (ms, value) = word.split_once(':').unwrap();
                let ms = ms.parse::<f64>().unwrap().round();
                let value = value.parse::<f64>().unwrap();
                let got = track.at((ms - began) / 1000.0);
                assert!(
                    (got.0 - value).abs() <= TOLERANCE,
                    "{line:.40}: at {ms} ms Qt shows {value}, this shows {}",
                    got.0
                );
                last = (ms, got);
                checked += 1;
            }
        }
        assert!(checked > 1000, "only {checked} values in the fixture");
    }

    #[test]
    fn a_velocity_of_zero_leaves_the_property_alone() {
        let p = Params {
            velocity: 0.0,
            duration: 100.0,
            max_easing: -1.0,
            reversing: Reversing::Eased,
        };
        assert_eq!(Track::plan(0.0, 100.0, 0.0, &p, None), None);
    }

    #[test]
    fn the_end_is_the_value_itself_and_a_standstill() {
        let p = Params {
            velocity: 30.0,
            duration: -1.0,
            max_easing: -1.0,
            reversing: Reversing::Eased,
        };
        let track = Track::plan(0.1, 0.3, 0.0, &p, None).unwrap();
        let end = track.micros() as f64 / 1e6;
        assert_eq!(track.at(end), (0.3, 0.0));
    }
}
