//! Putting a path together, and what can be asked of one that is: where it
//! lies, a point part of the way along it, and what it encloses.
//!
//! Nothing here knows of QML. The rules are Qt's, each checked on qml6:
//! a move straight after a move replaces it, drawing after a close starts a
//! new part where the closed one started, and an arc that cannot be an arc
//! is a line.

use ljos_render::kurbo::{
    Arc, BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveExtrema, PathEl, PathSeg,
    Point, QuadBez, Rect, Shape, SvgArc, Vec2,
};

/// How far the cubic curves an arc is made of may lie from it. Small next
/// to what is painted, because the bounds of a path are compared with Qt's
/// to a hundredth.
const ARC_TOLERANCE: f64 = 1e-3;

/// How much of a circle's radius a control point lies from the end of a
/// quarter of it.
const QUARTER: f64 = 0.552_284_749_830_793_4;

/// A path as it is drawn, one element after another.
#[derive(Debug)]
pub(crate) struct Builder {
    path: BezPath,
    /// Where the next element starts.
    pub cur: Point,
    /// Where the part being drawn started, which a close goes back to.
    sub: Point,
    /// The part was closed and nothing was drawn since.
    closed: bool,
}

impl Builder {
    pub fn new(start: Point) -> Self {
        let mut path = BezPath::new();
        path.move_to(start);
        Self {
            path,
            cur: start,
            sub: start,
            closed: false,
        }
    }

    pub fn finish(self) -> BezPath {
        self.path
    }

    /// How many elements the path has so far: where something that is not
    /// drawn (a `PathAttribute`) comes in it.
    pub fn len(&self) -> usize {
        self.path.elements().len()
    }

    pub fn move_to(&mut self, p: Point) {
        match self.path.elements_mut().last_mut() {
            Some(last @ PathEl::MoveTo(_)) => *last = PathEl::MoveTo(p),
            _ => self.path.move_to(p),
        }
        self.cur = p;
        self.sub = p;
        self.closed = false;
    }

    /// What is drawn after a close is a part of its own.
    fn begin(&mut self) {
        if std::mem::take(&mut self.closed) {
            self.path.move_to(self.cur);
        }
    }

    pub fn line_to(&mut self, p: Point) {
        self.begin();
        self.path.line_to(p);
        self.cur = p;
    }

    pub fn quad_to(&mut self, c: Point, p: Point) {
        self.begin();
        self.path.quad_to(c, p);
        self.cur = p;
    }

    pub fn curve_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.begin();
        self.path.curve_to(c1, c2, p);
        self.cur = p;
    }

    pub fn close(&mut self) {
        if !self.closed {
            self.path.close_path();
        }
        self.cur = self.sub;
        self.closed = true;
    }

    /// The curves of `arc`, ending at `end` to the last bit: where an arc
    /// ends is worked out from angles, and a path is closed only if it ends
    /// exactly where it started.
    fn arc(&mut self, arc: &Arc, end: Point) {
        self.begin();
        let before = self.path.elements().len();
        for el in arc.append_iter(ARC_TOLERANCE) {
            self.path.push(el);
        }
        if self.path.elements().len() == before {
            self.path.line_to(end);
        } else if let Some(PathEl::CurveTo(_, _, p)) = self.path.elements_mut().last_mut() {
            *p = end;
        }
        self.cur = end;
    }

    /// An arc as SVG has it: from the current point to `to`, on an ellipse
    /// with `radii` turned by `rotation` degrees. `large` picks the longer
    /// way round and `sweep` the clockwise one.
    pub fn arc_to(&mut self, radii: Vec2, rotation: f64, large: bool, sweep: bool, to: Point) {
        let from = self.cur;
        if from == to {
            return;
        }
        let radii = Vec2::new(radii.x.abs(), radii.y.abs());
        let usable = radii.x > 0.0 && radii.y > 0.0 && radii.x.is_finite() && radii.y.is_finite();
        let arc = usable
            .then(|| {
                Arc::from_svg_arc(&SvgArc {
                    from,
                    to,
                    radii,
                    x_rotation: rotation.to_radians(),
                    large_arc: large,
                    sweep,
                })
            })
            .flatten();
        match arc {
            Some(arc) => self.arc(&arc, to),
            None => self.line_to(to),
        }
    }

    /// Part of an ellipse round `center`, from `start` degrees on for
    /// `sweep` degrees: 0 is at three o'clock and the angle grows clockwise
    /// on the screen. With `move_to_start` the arc is a part of its own,
    /// otherwise a line leads to where it starts.
    pub fn angle_arc(
        &mut self,
        center: Point,
        radii: Vec2,
        start: f64,
        sweep: f64,
        move_to_start: bool,
    ) {
        let at = |degrees: f64| {
            let a = degrees.to_radians();
            Point::new(center.x + radii.x * a.cos(), center.y + radii.y * a.sin())
        };
        // More than a whole turn draws the ellipse once.
        let sweep = if sweep.is_finite() {
            sweep.clamp(-360.0, 360.0)
        } else {
            0.0
        };
        let from = at(start);
        if move_to_start {
            self.move_to(from);
        } else {
            self.line_to(from);
        }
        if sweep == 0.0 {
            return;
        }
        let end = if sweep.abs() == 360.0 {
            from
        } else {
            at(start + sweep)
        };
        let arc = Arc::new(center, radii, start.to_radians(), sweep.to_radians(), 0.0);
        self.arc(&arc, end);
    }

    /// A rectangle as a part of its own. Each corner is cut by its radius:
    /// round, or straight where `bevel` says so. Corners go clockwise from
    /// the top left: top left, top right, bottom right, bottom left.
    pub fn rectangle(&mut self, rect: Rect, radii: [f64; 4], bevel: [bool; 4]) {
        let (w, h) = (rect.width(), rect.height());
        if !(w > 0.0 && h > 0.0) {
            return;
        }
        let most = w.min(h) / 2.0;
        let r = radii.map(|r| {
            if r.is_finite() {
                r.clamp(0.0, most)
            } else {
                0.0
            }
        });
        let (x0, y0, x1, y1) = (rect.x0, rect.y0, rect.x1, rect.y1);
        // Each corner: the corner itself, where its cut starts and ends.
        let corners = [
            (
                Point::new(x1, y0),
                Point::new(x1 - r[1], y0),
                Point::new(x1, y0 + r[1]),
                1,
            ),
            (
                Point::new(x1, y1),
                Point::new(x1, y1 - r[2]),
                Point::new(x1 - r[2], y1),
                2,
            ),
            (
                Point::new(x0, y1),
                Point::new(x0 + r[3], y1),
                Point::new(x0, y1 - r[3]),
                3,
            ),
            (
                Point::new(x0, y0),
                Point::new(x0, y0 + r[0]),
                Point::new(x0 + r[0], y0),
                0,
            ),
        ];
        self.move_to(Point::new(x0 + r[0], y0));
        for (corner, from, to, which) in corners {
            // Not a line of no length: a stroke would put a join on it.
            if from != self.cur {
                self.line_to(from);
            }
            if r[which] <= 0.0 {
                continue;
            }
            if bevel[which] {
                self.line_to(to);
            } else {
                self.curve_to(
                    from + (corner - from) * QUARTER,
                    to + (corner - to) * QUARTER,
                    to,
                );
            }
        }
        self.close();
    }

    /// Lines through `points`, as a part of its own. Fewer than two points
    /// are no line.
    pub fn polyline(&mut self, points: &[Point]) {
        let [first, rest @ ..] = points else {
            return;
        };
        if rest.is_empty() {
            return;
        }
        self.move_to(*first);
        for p in rest {
            self.line_to(*p);
        }
    }
}

/// Where a path lies: every point it goes to and the curves between them,
/// by where they go and not by their control points. A move that nothing
/// follows counts, as it does in Qt. A path of nothing lies at the origin.
pub(crate) fn bounds_of(path: &BezPath) -> Rect {
    let mut acc: Option<Rect> = None;
    let mut add = |r: Rect| acc = Some(acc.map_or(r, |a| a.union(r)));
    let (mut cur, mut sub) = (Point::ZERO, Point::ZERO);
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                add(Rect::from_points(p, p));
                cur = p;
                sub = p;
            }
            PathEl::LineTo(p) => {
                add(Rect::from_points(p, p));
                cur = p;
            }
            PathEl::QuadTo(c, p) => {
                add(ParamCurveExtrema::bounding_box(&QuadBez::new(cur, c, p)));
                cur = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                add(ParamCurveExtrema::bounding_box(&CubicBez::new(
                    cur, c1, c2, p,
                )));
                cur = p;
            }
            PathEl::ClosePath => cur = sub,
        }
    }
    acc.unwrap_or(Rect::ZERO)
}

/// The point `part` of the way along `path` by length, `part` held between
/// 0 and 1. Moves have no length.
pub(crate) fn point_at(path: &BezPath, part: f64) -> Point {
    const ACCURACY: f64 = 1e-6;
    let part = if part.is_nan() {
        0.0
    } else {
        part.clamp(0.0, 1.0)
    };
    let segments: Vec<_> = path
        .segments()
        .map(|s| (s, s.arclen(ACCURACY)))
        .filter(|(_, len)| *len > 0.0)
        .collect();
    let Some((last, _)) = segments.last() else {
        return match path.elements().first() {
            Some(PathEl::MoveTo(p)) => *p,
            _ => Point::ZERO,
        };
    };
    let total: f64 = segments.iter().map(|(_, len)| len).sum();
    let mut left = part * total;
    for (seg, len) in &segments {
        if left <= *len {
            return seg.eval(seg.inv_arclen(left, ACCURACY));
        }
        left -= len;
    }
    last.end()
}

/// How long `path` is up to the end of each of its elements, by length
/// as [`point_at`] measures it.
pub(crate) fn lengths(path: &BezPath) -> Vec<f64> {
    const ACCURACY: f64 = 1e-6;
    let mut out = Vec::with_capacity(path.elements().len());
    let (mut total, mut last, mut start) = (0.0, Point::ZERO, Point::ZERO);
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                (start, last) = (p, p);
                None
            }
            PathEl::LineTo(p) => Some(PathSeg::Line(Line::new(last, p))),
            PathEl::QuadTo(c, p) => Some(PathSeg::Quad(QuadBez::new(last, c, p))),
            PathEl::CurveTo(a, b, p) => Some(PathSeg::Cubic(CubicBez::new(last, a, b, p))),
            PathEl::ClosePath => Some(PathSeg::Line(Line::new(last, start))),
        };
        if let Some(seg) = seg {
            total += seg.arclen(ACCURACY);
            last = seg.end();
        }
        out.push(total);
    }
    out
}

/// Whether `path` encloses `p`, every part of it closed by a line and a
/// part gone round twice not enclosed, whatever rule it is filled by: that
/// is what Qt's `Shape.contains` answers.
pub(crate) fn encloses(path: &BezPath, p: Point) -> bool {
    if !bounds_of(path).contains(p) {
        return false;
    }
    // Winding counts open parts as they are, so each is closed first.
    let mut closed = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) && std::mem::take(&mut open) {
            closed.close_path();
        }
        open = !matches!(el, PathEl::ClosePath);
        closed.push(*el);
    }
    if open {
        closed.close_path();
    }
    closed.winding(p) % 2 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn near(a: Rect, b: Rect, by: f64) -> bool {
        (a.x0 - b.x0).abs() <= by
            && (a.y0 - b.y0).abs() <= by
            && (a.x1 - b.x1).abs() <= by
            && (a.y1 - b.y1).abs() <= by
    }

    #[test]
    fn a_move_after_the_start_replaces_it() {
        let mut b = Builder::new(p(10.0, 10.0));
        b.move_to(p(50.0, 50.0));
        b.move_to(p(60.0, 60.0));
        let path = b.finish();
        assert_eq!(path.elements(), &[PathEl::MoveTo(p(60.0, 60.0))]);
        assert_eq!(bounds_of(&path), Rect::new(60.0, 60.0, 60.0, 60.0));
    }

    #[test]
    fn a_trailing_move_counts_in_the_bounds() {
        let mut b = Builder::new(p(10.0, 10.0));
        b.line_to(p(40.0, 30.0));
        b.move_to(p(100.0, 100.0));
        assert_eq!(bounds_of(&b.finish()), Rect::new(10.0, 10.0, 100.0, 100.0));
    }

    #[test]
    fn drawing_after_a_close_starts_where_the_part_started() {
        let mut b = Builder::new(p(10.0, 10.0));
        b.line_to(p(20.0, 10.0));
        b.line_to(p(20.0, 20.0));
        b.close();
        assert_eq!(b.cur, p(10.0, 10.0));
        b.line_to(p(0.0, 0.0));
        assert_eq!(
            b.finish().elements(),
            &[
                PathEl::MoveTo(p(10.0, 10.0)),
                PathEl::LineTo(p(20.0, 10.0)),
                PathEl::LineTo(p(20.0, 20.0)),
                PathEl::ClosePath,
                PathEl::MoveTo(p(10.0, 10.0)),
                PathEl::LineTo(p(0.0, 0.0)),
            ]
        );
    }

    #[test]
    fn a_curve_is_bounded_by_where_it_goes() {
        let mut b = Builder::new(p(10.0, 10.0));
        b.quad_to(p(60.0, 110.0), p(110.0, 10.0));
        assert!(near(
            bounds_of(&b.finish()),
            Rect::new(10.0, 10.0, 110.0, 60.0),
            1e-9
        ));
    }

    #[test]
    fn an_arc_ends_where_it_is_told_to() {
        let mut b = Builder::new(p(10.0, 50.0));
        b.arc_to(Vec2::new(40.0, 40.0), 0.0, false, true, p(90.0, 50.0));
        assert_eq!(b.cur, p(90.0, 50.0));
        // Clockwise from nine o'clock to three goes over the top.
        assert!(near(
            bounds_of(&b.finish()),
            Rect::new(10.0, 10.0, 90.0, 50.0),
            0.01
        ));
    }

    #[test]
    fn radii_too_small_are_made_to_reach() {
        let mut b = Builder::new(p(10.0, 50.0));
        b.arc_to(Vec2::new(10.0, 10.0), 0.0, false, false, p(90.0, 50.0));
        assert!(near(
            bounds_of(&b.finish()),
            Rect::new(10.0, 50.0, 90.0, 90.0),
            0.01
        ));
    }

    #[test]
    fn an_arc_that_cannot_be_one_is_a_line_or_nothing() {
        let mut b = Builder::new(p(10.0, 50.0));
        b.arc_to(Vec2::new(0.0, 40.0), 0.0, false, true, p(90.0, 50.0));
        b.arc_to(Vec2::new(40.0, 40.0), 0.0, true, true, p(90.0, 50.0));
        assert_eq!(
            b.finish().elements(),
            &[PathEl::MoveTo(p(10.0, 50.0)), PathEl::LineTo(p(90.0, 50.0))]
        );
    }

    #[test]
    fn a_whole_turn_ends_where_it_started() {
        let mut b = Builder::new(p(90.0, 50.0));
        b.angle_arc(p(50.0, 50.0), Vec2::new(40.0, 40.0), 0.0, 720.0, true);
        assert_eq!(b.cur, p(90.0, 50.0));
        assert!(near(
            bounds_of(&b.finish()),
            Rect::new(10.0, 10.0, 90.0, 90.0),
            0.01
        ));
    }

    #[test]
    fn a_quarter_turn_goes_clockwise_on_the_screen() {
        let mut b = Builder::new(p(0.0, 0.0));
        b.angle_arc(p(50.0, 50.0), Vec2::new(40.0, 40.0), 0.0, 90.0, true);
        assert!((b.cur - p(50.0, 90.0)).hypot() < 1e-9);
        assert!(near(
            bounds_of(&b.finish()),
            Rect::new(50.0, 50.0, 90.0, 90.0),
            0.01
        ));
    }

    #[test]
    fn an_arc_without_a_move_is_led_to_by_a_line() {
        let mut b = Builder::new(p(0.0, 0.0));
        b.angle_arc(p(50.0, 50.0), Vec2::new(40.0, 40.0), 0.0, 0.0, false);
        assert_eq!(
            b.finish().elements(),
            &[PathEl::MoveTo(p(0.0, 0.0)), PathEl::LineTo(p(90.0, 50.0))]
        );
    }

    #[test]
    fn a_rectangle_starts_and_ends_after_its_first_corner() {
        let mut b = Builder::new(p(5.0, 5.0));
        b.rectangle(Rect::new(10.0, 20.0, 60.0, 50.0), [10.0; 4], [false; 4]);
        assert_eq!(b.cur, p(20.0, 20.0));
        let path = b.finish();
        assert_eq!(path.elements()[0], PathEl::MoveTo(p(20.0, 20.0)));
        assert!(near(
            bounds_of(&path),
            Rect::new(10.0, 20.0, 60.0, 50.0),
            1e-9
        ));
    }

    #[test]
    fn a_rectangle_of_no_size_is_nothing() {
        let mut b = Builder::new(p(5.0, 5.0));
        b.rectangle(Rect::new(10.0, 20.0, 10.0, 50.0), [0.0; 4], [false; 4]);
        assert_eq!(b.finish().elements(), &[PathEl::MoveTo(p(5.0, 5.0))]);
    }

    #[test]
    fn a_bevel_cuts_a_corner_straight() {
        let mut b = Builder::new(p(0.0, 0.0));
        b.rectangle(
            Rect::new(0.0, 0.0, 40.0, 40.0),
            [10.0, 0.0, 0.0, 0.0],
            [true, false, false, false],
        );
        assert_eq!(
            b.finish().elements(),
            &[
                PathEl::MoveTo(p(10.0, 0.0)),
                PathEl::LineTo(p(40.0, 0.0)),
                PathEl::LineTo(p(40.0, 40.0)),
                PathEl::LineTo(p(0.0, 40.0)),
                PathEl::LineTo(p(0.0, 10.0)),
                PathEl::LineTo(p(10.0, 0.0)),
                PathEl::ClosePath,
            ]
        );
    }

    #[test]
    fn a_polyline_needs_two_points() {
        let mut b = Builder::new(p(3.0, 4.0));
        b.polyline(&[p(10.0, 10.0)]);
        b.polyline(&[]);
        assert_eq!(b.cur, p(3.0, 4.0));
        b.polyline(&[p(10.0, 10.0), p(40.0, 20.0)]);
        assert_eq!(b.cur, p(40.0, 20.0));
        assert_eq!(
            b.finish().elements(),
            &[PathEl::MoveTo(p(10.0, 10.0)), PathEl::LineTo(p(40.0, 20.0))]
        );
    }

    #[test]
    fn a_point_along_a_path_is_by_length() {
        let mut b = Builder::new(p(0.0, 0.0));
        b.line_to(p(100.0, 0.0));
        b.line_to(p(100.0, 100.0));
        let path = b.finish();
        for (part, want) in [
            (0.25, p(50.0, 0.0)),
            (0.5, p(100.0, 0.0)),
            (0.75, p(100.0, 50.0)),
            (2.0, p(100.0, 100.0)),
            (-1.0, p(0.0, 0.0)),
        ] {
            assert!((point_at(&path, part) - want).hypot() < 1e-6, "{part}");
        }
        assert_eq!(
            point_at(&Builder::new(p(7.0, 8.0)).finish(), 0.5),
            p(7.0, 8.0)
        );
    }

    #[test]
    fn an_open_path_encloses_what_a_closing_line_would() {
        let mut b = Builder::new(p(50.0, 10.0));
        b.line_to(p(90.0, 90.0));
        b.line_to(p(10.0, 90.0));
        let path = b.finish();
        assert!(encloses(&path, p(50.0, 60.0)));
        assert!(!encloses(&path, p(12.0, 12.0)));
        assert!(!encloses(&path, p(50.0, 93.0)));
    }

    #[test]
    fn what_a_path_goes_round_twice_is_not_enclosed() {
        let mut b = Builder::new(p(50.0, 10.0));
        for q in [
            p(74.0, 82.0),
            p(12.0, 38.0),
            p(88.0, 38.0),
            p(26.0, 82.0),
            p(50.0, 10.0),
        ] {
            b.line_to(q);
        }
        let path = b.finish();
        assert!(!encloses(&path, p(50.0, 50.0)));
        assert!(encloses(&path, p(50.0, 20.0)));
    }

    #[test]
    fn each_part_is_closed_on_its_own() {
        let mut b = Builder::new(p(0.0, 0.0));
        b.line_to(p(10.0, 0.0));
        b.line_to(p(10.0, 10.0));
        b.move_to(p(50.0, 50.0));
        b.line_to(p(60.0, 50.0));
        b.line_to(p(60.0, 60.0));
        let path = b.finish();
        assert!(encloses(&path, p(8.0, 4.0)));
        assert!(encloses(&path, p(58.0, 54.0)));
        assert!(!encloses(&path, p(30.0, 30.0)));
    }
}
