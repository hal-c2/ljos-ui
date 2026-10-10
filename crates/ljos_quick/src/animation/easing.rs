//! Easing curves: the `easing` group of a property animation and the
//! `Easing` enum.
//!
//! The curves are the documented ones (the Penner equations, plus Qt's
//! `InCurve`/`OutCurve`/`SineCurve`/`CosineCurve` and a cubic Bezier
//! spline). The unit test checks every curve against values measured on
//! qml6.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::enums::EnumTable;

/// `Easing.Type`, in the documented order.
pub const NAMES: [&str; 46] = [
    "Linear",
    "InQuad",
    "OutQuad",
    "InOutQuad",
    "OutInQuad",
    "InCubic",
    "OutCubic",
    "InOutCubic",
    "OutInCubic",
    "InQuart",
    "OutQuart",
    "InOutQuart",
    "OutInQuart",
    "InQuint",
    "OutQuint",
    "InOutQuint",
    "OutInQuint",
    "InSine",
    "OutSine",
    "InOutSine",
    "OutInSine",
    "InExpo",
    "OutExpo",
    "InOutExpo",
    "OutInExpo",
    "InCirc",
    "OutCirc",
    "InOutCirc",
    "OutInCirc",
    "InElastic",
    "OutElastic",
    "InOutElastic",
    "OutInElastic",
    "InBack",
    "OutBack",
    "InOutBack",
    "OutInBack",
    "InBounce",
    "OutBounce",
    "InOutBounce",
    "OutInBounce",
    "InCurve",
    "OutCurve",
    "SineCurve",
    "CosineCurve",
    "BezierSpline",
];

/// `Easing.BezierSpline`.
pub const BEZIER_SPLINE: i32 = 45;

pub(crate) fn easing_enums() -> EnumTable {
    vec![(
        "Type",
        NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i as i64))
            .collect(),
    )]
}

/// One easing curve with its parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// `Easing.Type`.
    pub ty: i32,
    pub amplitude: f64,
    pub overshoot: f64,
    pub period: f64,
    /// `bezierCurve`: `c1x, c1y, c2x, c2y, endx, endy` per segment.
    pub bezier: Vec<f64>,
}

impl Default for Curve {
    fn default() -> Self {
        Curve {
            ty: 0,
            amplitude: 1.0,
            overshoot: 1.70158,
            period: 0.3,
            bezier: Vec::new(),
        }
    }
}

impl Curve {
    /// The eased progress at `t` in 0..=1.
    pub fn value(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let (a, p, s) = (self.amplitude, self.period, self.overshoot);
        match self.ty {
            1 => t * t,
            2 => out(t, |t| t * t),
            3 => in_out(t, |t| t * t),
            4 => out_in(t, |t| t * t),
            5 => t.powi(3),
            6 => out(t, |t| t.powi(3)),
            7 => in_out(t, |t| t.powi(3)),
            8 => out_in(t, |t| t.powi(3)),
            9 => t.powi(4),
            10 => out(t, |t| t.powi(4)),
            11 => in_out(t, |t| t.powi(4)),
            12 => out_in(t, |t| t.powi(4)),
            13 => t.powi(5),
            14 => out(t, |t| t.powi(5)),
            15 => in_out(t, |t| t.powi(5)),
            16 => out_in(t, |t| t.powi(5)),
            17 => in_sine(t),
            18 => out(t, in_sine),
            19 => in_out(t, in_sine),
            20 => out_in(t, in_sine),
            21 => in_expo(t),
            22 => out_expo(t),
            23 => in_out_expo(t),
            24 => {
                if t < 0.5 {
                    out_expo(2.0 * t) / 2.0
                } else {
                    in_expo(2.0 * t - 1.0) / 2.0 + 0.5
                }
            }
            25 => in_circ(t),
            26 => out(t, in_circ),
            27 => in_out(t, in_circ),
            28 => out_in(t, in_circ),
            29 => in_elastic(t, 0.0, 1.0, a, p),
            30 => out_elastic(t, 1.0, a, p),
            31 => in_out_elastic(t, a, p),
            32 => {
                if t < 0.5 {
                    out_elastic(2.0 * t, 0.5, a, p)
                } else {
                    in_elastic(2.0 * t - 1.0, 0.5, 0.5, a, p)
                }
            }
            33 => in_back(t, s),
            34 => out_back(t, s),
            35 => in_out_back(t, s),
            36 => {
                if t < 0.5 {
                    out_back(2.0 * t, s) / 2.0
                } else {
                    in_back(2.0 * t - 1.0, s) / 2.0 + 0.5
                }
            }
            37 => in_bounce(t, a),
            38 => bounce(t, 1.0, a),
            39 => {
                if t < 0.5 {
                    in_bounce(2.0 * t, a) / 2.0
                } else if t == 1.0 {
                    1.0
                } else {
                    bounce(2.0 * t - 1.0, 1.0, a) / 2.0 + 0.5
                }
            }
            40 => {
                if t < 0.5 {
                    bounce(2.0 * t, 0.5, a)
                } else {
                    1.0 - bounce(2.0 - 2.0 * t, 0.5, a)
                }
            }
            41 => in_curve(t),
            42 => 1.0 - in_curve(1.0 - t),
            43 => ((t * TAU - FRAC_PI_2).sin() + 1.0) / 2.0,
            44 => ((t * TAU - FRAC_PI_2).cos() + 1.0) / 2.0,
            BEZIER_SPLINE => bezier(&self.bezier, t),
            // Linear, and what is not a curve.
            _ => t,
        }
    }
}

/// The mirrored curve: fast where `f` is slow.
fn out(t: f64, f: impl Fn(f64) -> f64) -> f64 {
    1.0 - f(1.0 - t)
}

/// `f` over the first half, its mirror over the second.
fn in_out(t: f64, f: impl Fn(f64) -> f64) -> f64 {
    if t < 0.5 {
        f(2.0 * t) / 2.0
    } else {
        1.0 - f(2.0 - 2.0 * t) / 2.0
    }
}

/// The mirror over the first half, `f` over the second.
fn out_in(t: f64, f: impl Fn(f64) -> f64) -> f64 {
    if t < 0.5 {
        out(2.0 * t, &f) / 2.0
    } else {
        f(2.0 * t - 1.0) / 2.0 + 0.5
    }
}

fn in_sine(t: f64) -> f64 {
    if t == 1.0 {
        1.0
    } else {
        1.0 - (t * FRAC_PI_2).cos()
    }
}

fn in_circ(t: f64) -> f64 {
    1.0 - (1.0 - t * t).max(0.0).sqrt()
}

fn in_expo(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        t
    } else {
        2f64.powf(10.0 * (t - 1.0)) - 0.001
    }
}

fn out_expo(t: f64) -> f64 {
    if t == 1.0 {
        1.0
    } else {
        1.001 * (1.0 - 2f64.powf(-10.0 * t))
    }
}

fn in_out_expo(t: f64) -> f64 {
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let t = 2.0 * t;
    if t < 1.0 {
        0.5 * 2f64.powf(10.0 * (t - 1.0)) - 0.0005
    } else {
        0.5 * 1.0005 * (2.0 - 2f64.powf(-10.0 * (t - 1.0)))
    }
}

/// The amplitude in use and the phase shift that makes the wave start at
/// the right height.
fn elastic_shape(a: f64, c: f64, p: f64) -> (f64, f64) {
    if a < c.abs() {
        (c, p / 4.0)
    } else {
        (a, p / TAU * (c / a).asin())
    }
}

/// A wave that swells from `b` to `b + c`.
fn in_elastic(t: f64, b: f64, c: f64, a: f64, p: f64) -> f64 {
    if t == 0.0 {
        return b;
    }
    if t == 1.0 {
        return b + c;
    }
    let (a, s) = elastic_shape(a, c, p);
    let t = t - 1.0;
    -(a * 2f64.powf(10.0 * t) * ((t - s) * TAU / p).sin()) + b
}

/// A wave that dies down at `c`.
fn out_elastic(t: f64, c: f64, a: f64, p: f64) -> f64 {
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return c;
    }
    let (a, s) = elastic_shape(a, c, p);
    a * 2f64.powf(-10.0 * t) * ((t - s) * TAU / p).sin() + c
}

fn in_out_elastic(t: f64, a: f64, p: f64) -> f64 {
    if t == 0.0 {
        return 0.0;
    }
    let t = 2.0 * t;
    if t == 2.0 {
        return 1.0;
    }
    let (a, s) = elastic_shape(a, 1.0, p);
    let t = t - 1.0;
    if t < 0.0 {
        -0.5 * (a * 2f64.powf(10.0 * t) * ((t - s) * TAU / p).sin())
    } else {
        a * 2f64.powf(-10.0 * t) * ((t - s) * TAU / p).sin() * 0.5 + 1.0
    }
}

fn in_back(t: f64, s: f64) -> f64 {
    t * t * ((s + 1.0) * t - s)
}

fn out_back(t: f64, s: f64) -> f64 {
    let t = t - 1.0;
    t * t * ((s + 1.0) * t + s) + 1.0
}

fn in_out_back(t: f64, s: f64) -> f64 {
    let s = s * 1.525;
    let t = 2.0 * t;
    if t < 1.0 {
        0.5 * (t * t * ((s + 1.0) * t - s))
    } else {
        let t = t - 2.0;
        0.5 * (t * t * ((s + 1.0) * t + s) + 2.0)
    }
}

/// A ball dropped on `c`: each bounce is `a` high.
fn bounce(t: f64, c: f64, a: f64) -> f64 {
    const K: f64 = 7.5625;
    if t == 1.0 {
        c
    } else if t < 4.0 / 11.0 {
        c * K * t * t
    } else if t < 8.0 / 11.0 {
        let t = t - 6.0 / 11.0;
        -a * (1.0 - (K * t * t + 0.75)) + c
    } else if t < 10.0 / 11.0 {
        let t = t - 9.0 / 11.0;
        -a * (1.0 - (K * t * t + 0.9375)) + c
    } else {
        let t = t - 21.0 / 22.0;
        -a * (1.0 - (K * t * t + 0.984375)) + c
    }
}

fn in_bounce(t: f64, a: f64) -> f64 {
    1.0 - bounce(1.0 - t, 1.0, a)
}

/// Half a sine wave that blends into the straight line.
fn in_curve(t: f64) -> f64 {
    let sine = ((t * PI - FRAC_PI_2).sin() + 1.0) / 2.0;
    let mix = (1.0 - 2.0 * t + 0.3).clamp(0.0, 1.0);
    sine * mix + t * (1.0 - mix)
}

/// The spline's height where its x is `x`. The spline starts at (0, 0);
/// each segment is two control points and an end point. A list that is not
/// whole segments ending at (1, 1) is no curve, and reads as linear.
fn bezier(points: &[f64], x: f64) -> f64 {
    let whole = !points.is_empty() && points.len().is_multiple_of(6);
    if !whole || points[points.len() - 2..] != [1.0, 1.0] {
        return x;
    }
    let mut start = (0.0, 0.0);
    let segments = points.len() / 6;
    for (i, seg) in points.as_chunks::<6>().0.iter().enumerate() {
        let end = (seg[4], seg[5]);
        if x <= end.0 || i + 1 == segments {
            let cubic = |p0: f64, p1: f64, p2: f64, p3: f64, u: f64| {
                let v = 1.0 - u;
                v * v * v * p0 + 3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u * p3
            };
            // x grows with the parameter along a segment of an easing
            // curve, so halving the interval finds it.
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if cubic(start.0, seg[0], seg[2], end.0, mid) < x {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return cubic(start.1, seg[1], seg[3], end.1, (lo + hi) / 2.0);
        }
        start = end;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// qml6's values are printed with six decimals.
    const TOLERANCE: f64 = 2e-6;
    /// Qt finds the point on a spline less exactly than the halving here
    /// does: its values are off the curve by up to 0.0003 in places (the
    /// one-segment CSS `ease` curve at 0.25 is 0.408511; qml6 prints
    /// 0.408495).
    const SPLINE_TOLERANCE: f64 = 5e-4;

    fn check(curve: &Curve, label: &str, times: &[f64], values: &[f64]) {
        assert_eq!(times.len(), values.len(), "{label}");
        let tolerance = if curve.ty == BEZIER_SPLINE {
            SPLINE_TOLERANCE
        } else {
            TOLERANCE
        };
        for (t, want) in times.iter().zip(values) {
            let got = curve.value(*t);
            assert!(
                (got - want).abs() <= tolerance,
                "{label} at {t}: {got}, Qt has {want}"
            );
        }
    }

    fn numbers(s: &str) -> Vec<f64> {
        s.split_whitespace().map_while(|w| w.parse().ok()).collect()
    }

    /// `"amplitude":2,"period":0.5` in a fixture label.
    fn parameter(label: &str, name: &str) -> Option<f64> {
        let rest = &label[label.find(&format!("\"{name}\":"))? + name.len() + 3..];
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        rest[..end].parse().ok()
    }

    #[test]
    fn every_curve_matches_qt() {
        let times = [0.0, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 1.0];
        let mut seen = 0;
        for line in include_str!("fixtures/easing_qt.txt").lines() {
            let (label, values) = line.split_once(": ").expect("label: values");
            let mut curve = Curve::default();
            if let Some(name) = label.strip_prefix("Bezier ") {
                curve.ty = BEZIER_SPLINE;
                curve.bezier = if name == "ease" {
                    vec![0.25, 0.1, 0.25, 1.0, 1.0, 1.0]
                } else {
                    vec![0.3, 0.0, 0.2, 0.6, 0.5, 0.5, 0.7, 0.4, 0.9, 1.3, 1.0, 1.0]
                };
            } else {
                let name = label.split(['=', '{']).next().expect("a name");
                curve.ty = NAMES
                    .iter()
                    .position(|n| *n == name)
                    .expect("a known curve") as i32;
                if let Some((_, number)) = label.split_once('=') {
                    assert_eq!(number.parse::<i32>().ok(), Some(curve.ty), "{label}");
                }
                if let Some(a) = parameter(label, "amplitude") {
                    curve.amplitude = a;
                }
                if let Some(p) = parameter(label, "period") {
                    curve.period = p;
                }
                if let Some(o) = parameter(label, "overshoot") {
                    curve.overshoot = o;
                }
            }
            check(&curve, label, &times, &numbers(values));
            seen += 1;
        }
        assert_eq!(seen, 60);
    }

    #[test]
    fn the_curves_with_corners_match_qt_all_along() {
        let times: Vec<f64> = (0..=40).map(|j| f64::from(j) / 40.0).collect();
        let mut seen = 0;
        for line in include_str!("fixtures/easing_dense_qt.txt").lines() {
            let (name, values) = line.split_once(": ").expect("name: values");
            let curve = Curve {
                ty: NAMES
                    .iter()
                    .position(|n| *n == name)
                    .expect("a known curve") as i32,
                ..Curve::default()
            };
            check(&curve, name, &times, &numbers(values));
            seen += 1;
        }
        assert_eq!(seen, 16);
    }

    #[test]
    fn a_list_that_is_no_spline_is_linear() {
        let mut curve = Curve {
            ty: BEZIER_SPLINE,
            ..Curve::default()
        };
        assert_eq!(curve.value(0.3), 0.3);
        curve.bezier = vec![0.2, 0.2, 0.8, 0.8, 0.5, 0.5];
        assert_eq!(curve.value(0.3), 0.3);
    }
}
