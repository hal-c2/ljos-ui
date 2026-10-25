//! Values that change over time: keyframes, easing and holds.
//!
//! A Lottie property is either one value or a list of keyframes. Between
//! two keyframes the value moves along a cubic bezier easing curve given
//! by the first key's out tangent (`o`) and the second's in tangent
//! (`i`), possibly one curve per component. A key marked `h` holds its
//! value until the next. A position can also travel a curved path
//! (`to`, `ti`), at the eased fraction of that path's length.

use std::borrow::Cow;

use ljos::item::kurbo::{CubicBez, ParamCurve, ParamCurveArclen, Point};

/// One easing tangent: a point of the unit square per component; empty
/// for linear.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ease {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

impl Ease {
    fn at(&self, c: usize) -> Option<(f64, f64)> {
        let x = self.x.get(c).or(self.x.first())?;
        let y = self.y.get(c).or(self.y.first())?;
        Some((*x, *y))
    }
}

/// A keyframe: the value from time `t` on, and how it goes to the next.
#[derive(Clone, Debug, PartialEq)]
pub struct Key<T> {
    pub t: f64,
    pub start: T,
    /// The value it reaches at the next key, in files that say so on the
    /// key itself (older exporters); otherwise the next key's start.
    pub end: Option<T>,
    pub hold: bool,
    pub out_ease: Ease,
    pub in_ease: Ease,
    /// Tangents of a curved motion path, relative to its ends.
    pub spatial: Option<([f64; 2], [f64; 2])>,
}

/// A value that may change over time.
#[derive(Clone, Debug, PartialEq)]
pub enum Anim<T> {
    Static(T),
    Keyed(Vec<Key<T>>),
}

/// What can be blended between two keyframes. `progress(c)` is how far
/// component `c` has gone, eased.
pub trait Blend: Clone {
    fn blend(&self, to: &Self, progress: &dyn Fn(usize) -> f64) -> Self;
}

impl Blend for Vec<f64> {
    fn blend(&self, to: &Self, progress: &dyn Fn(usize) -> f64) -> Self {
        self.iter()
            .enumerate()
            .map(|(c, a)| {
                let b = to.get(c).copied().unwrap_or(*a);
                a + (b - a) * progress(c)
            })
            .collect()
    }
}

impl<T: Blend + Default> Default for Anim<T> {
    fn default() -> Self {
        Anim::Static(T::default())
    }
}

impl<T: Blend> Anim<T> {
    /// Whether the value never changes.
    pub fn is_static(&self) -> bool {
        match self {
            Anim::Static(_) => true,
            Anim::Keyed(keys) => keys.len() < 2,
        }
    }

    /// The value at frame `t`.
    pub fn at(&self, t: f64) -> Cow<'_, T> {
        let keys = match self {
            Anim::Static(v) => return Cow::Borrowed(v),
            Anim::Keyed(keys) => keys,
        };
        let Some(first) = keys.first() else {
            unreachable!("a keyed value is parsed with at least one key")
        };
        if keys.len() == 1 || t <= first.t {
            return Cow::Borrowed(&first.start);
        }
        // The segment whose start is the last at or before `t`.
        let i = keys.partition_point(|k| k.t <= t).saturating_sub(1);
        let key = &keys[i];
        let Some(next) = keys.get(i + 1) else {
            // Past the last key: what the one before it ended on, if the
            // last key carries no value of its own.
            return Cow::Borrowed(&key.start);
        };
        let to = key.end.as_ref().unwrap_or(&next.start);
        if key.hold || next.t <= key.t {
            return Cow::Borrowed(&key.start);
        }
        let u = (t - key.t) / (next.t - key.t);
        let progress = |c: usize| match (key.out_ease.at(c), next_in(key, next, c)) {
            (Some((x1, y1)), Some((x2, y2))) => bezier_ease(x1, y1, x2, y2, u),
            _ => u,
        };
        Cow::Owned(key.start.blend(to, &progress))
    }
}

/// The in tangent of a segment: on the next key in today's files, on
/// the key itself in older ones.
fn next_in<T>(key: &Key<T>, next: &Key<T>, c: usize) -> Option<(f64, f64)> {
    next.in_ease.at(c).or_else(|| key.in_ease.at(c))
}

impl Anim<Vec<f64>> {
    /// Component `c` at `t`, or `or` when the value has none.
    pub fn num(&self, t: f64, c: usize, or: f64) -> f64 {
        self.at(t).get(c).copied().unwrap_or(or)
    }

    /// A point at `t`, following a curved motion path where the keys
    /// give one.
    pub fn point(&self, t: f64) -> Point {
        if let Some(p) = self.along_path(t) {
            return p;
        }
        let v = self.at(t);
        Point::new(
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
        )
    }

    fn along_path(&self, t: f64) -> Option<Point> {
        let Anim::Keyed(keys) = self else {
            return None;
        };
        let i = keys.partition_point(|k| k.t <= t).checked_sub(1)?;
        let (key, next) = (&keys[i], keys.get(i + 1)?);
        let (to, ti) = key.spatial?;
        if key.hold || next.t <= key.t || (to == [0.0; 2] && ti == [0.0; 2]) {
            return None;
        }
        let end = key.end.as_ref().unwrap_or(&next.start);
        let a = Point::new(*key.start.first()?, *key.start.get(1)?);
        let b = Point::new(*end.first()?, *end.get(1)?);
        let curve = CubicBez::new(a, a + (to[0], to[1]), b + (ti[0], ti[1]), b);
        let u = (t - key.t) / (next.t - key.t);
        let eased = match (key.out_ease.at(0), next_in(key, next, 0)) {
            (Some((x1, y1)), Some((x2, y2))) => bezier_ease(x1, y1, x2, y2, u),
            _ => u,
        };
        let length = curve.arclen(1e-3);
        let s = curve.inv_arclen(length * eased.clamp(0.0, 1.0), 1e-3);
        Some(curve.eval(s))
    }
}

/// `y` at `x` on the easing curve from (0, 0) to (1, 1) through
/// (`x1`, `y1`) and (`x2`, `y2`), as CSS's `cubic-bezier` is solved.
pub fn bezier_ease(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
    let cubic = |a: f64, b: f64, s: f64| {
        let r = 1.0 - s;
        3.0 * r * r * s * a + 3.0 * r * s * s * b + s * s * s
    };
    // x(s) rises from 0 to 1 when both control xs are in the unit range,
    // so bisection always finds the one s; a few Newton steps first make
    // it fast in the common case.
    let mut s = x;
    for _ in 0..8 {
        let err = cubic(x1, x2, s) - x;
        if err.abs() < 1e-7 {
            return cubic(y1, y2, s);
        }
        let r = 1.0 - s;
        let slope = 3.0 * r * r * x1 + 6.0 * r * s * (x2 - x1) + 3.0 * s * s * (1.0 - x2);
        if slope.abs() < 1e-6 {
            break;
        }
        s = (s - err / slope).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    s = x;
    for _ in 0..60 {
        let v = cubic(x1, x2, s);
        if (v - x).abs() < 1e-9 {
            break;
        }
        if v < x {
            lo = s;
        } else {
            hi = s;
        }
        s = 0.5 * (lo + hi);
    }
    cubic(y1, y2, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f64, v: f64) -> Key<Vec<f64>> {
        Key {
            t,
            start: vec![v],
            end: None,
            hold: false,
            out_ease: Ease::default(),
            in_ease: Ease::default(),
            spatial: None,
        }
    }

    #[test]
    fn linear_keys_blend_evenly() {
        let a = Anim::Keyed(vec![key(0.0, 10.0), key(20.0, 90.0)]);
        assert_eq!(a.num(-5.0, 0, 0.0), 10.0);
        assert_eq!(a.num(5.0, 0, 0.0), 30.0);
        assert_eq!(a.num(25.0, 0, 0.0), 90.0);
    }

    #[test]
    fn a_held_key_keeps_its_value_until_the_next() {
        let mut first = key(0.0, 10.0);
        first.hold = true;
        let a = Anim::Keyed(vec![first, key(20.0, 90.0)]);
        assert_eq!(a.num(15.0, 0, 0.0), 10.0);
        assert_eq!(a.num(20.0, 0, 0.0), 90.0);
    }

    #[test]
    fn easing_follows_the_curve() {
        // Slow start: the curve through (1, 0) and (1, 1).
        let y = bezier_ease(1.0, 0.0, 1.0, 1.0, 0.75);
        assert!((y - 0.3094).abs() < 1e-3, "{y}");
        assert!((bezier_ease(0.0, 0.0, 1.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
        let linear = bezier_ease(0.3, 0.3, 0.7, 0.7, 0.4);
        assert!((linear - 0.4).abs() < 1e-6);
    }

    #[test]
    fn an_old_key_carries_its_own_end() {
        let mut first = key(0.0, 0.0);
        first.end = Some(vec![100.0]);
        let a = Anim::Keyed(vec![first, key(10.0, 5.0)]);
        assert_eq!(a.num(5.0, 0, 0.0), 50.0);
    }
}
