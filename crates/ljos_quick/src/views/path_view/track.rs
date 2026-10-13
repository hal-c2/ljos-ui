//! The path a `PathView` lays its items along, measured by length: where a
//! point some way along it is, how much of the items the path holds up to
//! there (`PathPercent`) and what the `PathAttribute`s say there.

use std::sync::Arc;

use ljos_engine::{Atom, Engine, ObjectId};
use ljos_render::kurbo::{
    BezPath, ParamCurve, ParamCurveArclen, ParamCurveNearest, PathEl, PathSeg, Point,
};

use crate::item::is_a;
use crate::shapes::{PathAttribute, PathPercent, path_geometry};

const ACCURACY: f64 = 1e-6;

/// A value that changes with the length along the path: what it is at
/// each knot, and in between by length.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Knots(Vec<(f64, f64)>);

impl Knots {
    fn at(&self, len: f64) -> f64 {
        let k = &self.0;
        let Some(first) = k.first() else {
            return 0.0;
        };
        if len <= first.0 {
            return first.1;
        }
        for w in k.windows(2) {
            let ((l0, v0), (l1, v1)) = (w[0], w[1]);
            if len <= l1 {
                let span = l1 - l0;
                return if span > 0.0 {
                    v0 + (v1 - v0) * (len - l0) / span
                } else {
                    v1
                };
            }
        }
        k.last().map_or(0.0, |l| l.1)
    }
}

/// The path, ready to be walked along.
#[derive(Debug)]
pub(crate) struct Track {
    /// The outline it was made from: two tracks of the same outline and
    /// marks lay items out the same.
    path: Arc<BezPath>,
    /// Each segment that has a length, with the length before it.
    segs: Vec<(f64, f64, PathSeg)>,
    pub total: f64,
    /// Where the path starts: where everything is on a path of no length.
    start: Point,
    pub closed: bool,
    /// From length to how much of the items lie before it, rising from
    /// (0, 0) to (total, 1).
    percents: Vec<(f64, f64)>,
    /// Each attribute's name, in the order the path first names it.
    pub attrs: Vec<(Atom, Knots)>,
}

impl PartialEq for Track {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.path, &o.path) && self.percents == o.percents && self.attrs == o.attrs
    }
}

impl Track {
    /// The track of `path`, a `Path`. Tracked: reads the geometry and the
    /// marks' values. `None` for no path.
    pub(crate) fn of(e: &Engine, path: ObjectId) -> Option<Track> {
        let g = path_geometry(e, path)?;
        let mut segs = Vec::new();
        let mut total = 0.0;
        for seg in g.path.segments() {
            let len = seg.arclen(ACCURACY);
            if len > 0.0 {
                segs.push((total, len, seg));
                total += len;
            }
        }
        let start = match g.path.elements().first() {
            Some(PathEl::MoveTo(p)) => *p,
            _ => Point::ZERO,
        };
        let mut percents = vec![(0.0, 0.0)];
        // Each attribute's values where the path names them.
        let mut named: Vec<(Atom, Vec<(f64, f64)>)> = Vec::new();
        for (mark, at) in &g.marks {
            let at = at.clamp(0.0, total);
            if is_a::<PathPercent>(e, *mark) {
                let value = e.get(*mark, PathPercent::VALUE).unwrap_or(0.0);
                percents.push((at, value));
            } else if let (Ok(name), Ok(value)) = (
                e.get(*mark, PathAttribute::NAME),
                e.get(*mark, PathAttribute::VALUE),
            ) {
                if name == Atom::EMPTY {
                    continue;
                }
                match named.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, k)) => k.push((at, value)),
                    None => named.push((name, vec![(at, value)])),
                }
            }
        }
        if percents.last().is_none_or(|l| l.0 < total || l.1 < 1.0) {
            percents.push((total, 1.0));
        }
        let attrs = named
            .into_iter()
            .map(|(name, given)| {
                // Where the path does not say, an attribute is 0 at the
                // start and at the end what it was at the start (qml6).
                let first = given.first().filter(|k| k.0 <= 0.0).map_or(0.0, |k| k.1);
                let last = given.last().filter(|k| k.0 >= total).map_or(first, |k| k.1);
                let mut k = vec![(0.0, first)];
                k.extend(given.iter().filter(|g| g.0 > 0.0 && g.0 < total));
                k.push((total, last));
                (name, Knots(k))
            })
            .collect();
        Some(Track {
            path: g.path.clone(),
            segs,
            total,
            start,
            closed: g.closed,
            percents,
            attrs,
        })
    }

    /// The point `len` along the path.
    pub(crate) fn point(&self, len: f64) -> Point {
        let Some(last) = self.segs.last() else {
            return self.start;
        };
        let i = self.segs.partition_point(|s| s.0 + s.1 < len);
        let (before, l, seg) = self.segs.get(i).copied().unwrap_or(*last);
        let along = (len - before).clamp(0.0, l);
        seg.eval(seg.inv_arclen(along, ACCURACY))
    }

    /// How far along the path the share `p` of the items is.
    pub(crate) fn len_of_percent(&self, p: f64) -> f64 {
        let k = &self.percents;
        for w in k.windows(2) {
            let ((l0, p0), (l1, p1)) = (w[0], w[1]);
            if p <= p1 {
                let span = p1 - p0;
                return if span > 0.0 {
                    l0 + (l1 - l0) * ((p - p0) / span).clamp(0.0, 1.0)
                } else {
                    l0
                };
            }
        }
        self.total
    }

    /// The share of the items that lies before `len`.
    pub(crate) fn percent_of_len(&self, len: f64) -> f64 {
        let k = &self.percents;
        for w in k.windows(2) {
            let ((l0, p0), (l1, p1)) = (w[0], w[1]);
            if len <= l1 {
                let span = l1 - l0;
                return if span > 0.0 {
                    p0 + (p1 - p0) * ((len - l0) / span).clamp(0.0, 1.0)
                } else {
                    p1
                };
            }
        }
        1.0
    }

    /// What attribute `i` is at `len`.
    pub(crate) fn attr(&self, i: usize, len: f64) -> f64 {
        self.attrs.get(i).map_or(0.0, |a| a.1.at(len))
    }

    /// The length along the path of the point nearest `p`, and how far
    /// `p` is from it.
    pub(crate) fn nearest(&self, p: Point) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64)> = None;
        for (before, _, seg) in &self.segs {
            let n = seg.nearest(p, ACCURACY);
            if best.is_none_or(|b| n.distance_sq < b.1) {
                let len = before + seg.subsegment(0.0..n.t).arclen(ACCURACY);
                best = Some((len, n.distance_sq));
            }
        }
        if self.segs.is_empty() {
            return Some((0.0, (p - self.start).hypot2()));
        }
        best.map(|(len, d2)| (len, d2.sqrt()))
    }
}
