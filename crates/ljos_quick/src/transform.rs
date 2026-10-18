//! `Item.transform`: `Rotation`, `Scale`, `Translate`, `Matrix4x4` and
//! `Shear`.
//!
//! Measured on qml6 (features/items/transform.feature): an item maps a
//! point by its own `scale` and `rotation` about `transformOrigin` first,
//! then by the list from its first entry to its last, then by `x`/`y`.
//! A turn about the x or y axis is a perspective: the point is turned in
//! 3D and divided by `1 - z / distanceToPlane`, with the z of `origin`
//! playing no part. `Matrix4x4` maps `(x, y, 0, 1)` and divides by `w`.
//!
//! A point mapped with `mapToItem` goes through that exactly
//! ([`Projective`]). The render tree only has affine transforms, so the
//! picture of an item whose list has perspective is the affine map that
//! comes closest to it over the item's four corners ([`Projective::fit`]):
//! exact for every list without a perspective, and for a turn about the
//! y axis the parallelogram nearest the trapezoid Qt draws.

use ljos_engine::{
    Cx, Engine, LogLevel, MetaRef, ObjectId, QmlObject, QmlType, QtObject, Value, Vector3d,
    Version, qml_impl,
};
use ljos_render::kurbo::{Affine, Point};

use crate::item::{Item, compose_transform, is_a, num};

/// What `Rotation` divides by when it says nothing else.
pub const DEFAULT_DISTANCE_TO_PLANE: f64 = 1024.0;

/// A plane projective map, rows of a 3×3 matrix acting on `(x, y, 1)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective(pub [[f64; 3]; 3]);

impl Projective {
    pub const IDENTITY: Projective =
        Projective([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    /// `*self == Projective::IDENTITY`, as nine plain comparisons rather
    /// than a loop over each row: asked of every item that moves, every
    /// frame.
    pub fn is_identity(&self) -> bool {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.0;
        a == 1.0
            && e == 1.0
            && i == 1.0
            && b == 0.0
            && c == 0.0
            && d == 0.0
            && f == 0.0
            && g == 0.0
            && h == 0.0
    }

    pub fn from_affine(a: Affine) -> Projective {
        let [a, b, c, d, e, f] = a.as_coeffs();
        Projective([[a, c, e], [b, d, f], [0.0, 0.0, 1.0]])
    }

    fn translate(x: f64, y: f64) -> Projective {
        Projective::from_affine(Affine::translate((x, y)))
    }

    /// Whether the map keeps parallel lines parallel.
    pub fn is_affine(&self) -> bool {
        let r = self.0[2];
        r[0] == 0.0 && r[1] == 0.0 && r[2] == 1.0
    }

    /// `self` after `other`: `(self * other)(p) = self(other(p))`.
    #[allow(clippy::needless_range_loop)]
    pub fn then_after(&self, other: &Projective) -> Projective {
        let (a, b) = (&self.0, &other.0);
        let mut m = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
            }
        }
        Projective(m)
    }

    /// Where `p` goes; `None` for a point the map sends to infinity or
    /// behind the viewer.
    pub fn apply(&self, p: Point) -> Option<Point> {
        let m = &self.0;
        let w = m[2][0] * p.x + m[2][1] * p.y + m[2][2];
        if w.abs() < 1e-12 {
            return None;
        }
        Some(Point::new(
            (m[0][0] * p.x + m[0][1] * p.y + m[0][2]) / w,
            (m[1][0] * p.x + m[1][1] * p.y + m[1][2]) / w,
        ))
    }

    pub fn inverse(&self) -> Option<Projective> {
        let m = &self.0;
        let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
        let c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2];
        let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
        let det = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
        if det.abs() < 1e-15 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Projective([
            [
                c00 * inv,
                (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
                (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
            ],
            [
                c01 * inv,
                (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
                (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
            ],
            [
                c02 * inv,
                (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
                (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
            ],
        ]))
    }

    /// The affine map nearest this one over the corners of a `w` by `h`
    /// rectangle at the origin (least squares). An item with no area is
    /// fitted over a unit square, so it still turns the way it would.
    pub fn fit(&self, w: f64, h: f64) -> Affine {
        if self.is_affine() {
            let m = &self.0;
            return Affine::new([m[0][0], m[1][0], m[0][1], m[1][1], m[0][2], m[1][2]]);
        }
        let (w, h) = (
            if w > 1e-9 { w } else { 1.0 },
            if h > 1e-9 { h } else { 1.0 },
        );
        let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
        let mut out = Vec::with_capacity(4);
        for (x, y) in corners {
            match self.apply(Point::new(x, y)) {
                // A corner behind the viewer: nothing sensible to fit, so
                // the perspective is left out.
                Some(q) if self.w_at(x, y) > 0.0 => out.push(q),
                _ => {
                    let m = &self.0;
                    return Affine::new([m[0][0], m[1][0], m[0][1], m[1][1], m[0][2], m[1][2]]);
                }
            }
        }
        // The four corners of a rectangle make the normal equations of a
        // least-squares affine fit separable: with u = x/w and v = y/h
        // centred on the rectangle, the columns are orthogonal.
        let cx = out.iter().map(|q| q.x).sum::<f64>() / 4.0;
        let cy = out.iter().map(|q| q.y).sum::<f64>() / 4.0;
        // Signs of u and v at each corner, from the centre.
        let su = [-1.0, 1.0, -1.0, 1.0];
        let sv = [-1.0, -1.0, 1.0, 1.0];
        let du = |f: &dyn Fn(&Point) -> f64| {
            out.iter().zip(su).map(|(q, s)| f(q) * s).sum::<f64>() / 4.0
        };
        let dv = |f: &dyn Fn(&Point) -> f64| {
            out.iter().zip(sv).map(|(q, s)| f(q) * s).sum::<f64>() / 4.0
        };
        // q ≈ centre + a * (x - w/2) + b * (y - h/2), with a = du / (w/2).
        let ax = du(&|q| q.x) / (w / 2.0);
        let ay = du(&|q| q.y) / (w / 2.0);
        let bx = dv(&|q| q.x) / (h / 2.0);
        let by = dv(&|q| q.y) / (h / 2.0);
        let ex = cx - ax * w / 2.0 - bx * h / 2.0;
        let ey = cy - ay * w / 2.0 - by * h / 2.0;
        Affine::new([ax, ay, bx, by, ex, ey])
    }

    fn w_at(&self, x: f64, y: f64) -> f64 {
        let m = &self.0;
        m[2][0] * x + m[2][1] * y + m[2][2]
    }
}

/// The base of the transforms an item lists. Qt does not name it in QML.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "Transform")]
pub struct Transform {
    #[qml(base)]
    pub object: QtObject,
}

#[qml_impl]
impl Transform {
    /// `update()`: the transforms here are read when they change already.
    #[qml(invokable, name = "update")]
    fn update(_cx: &Cx) {}
}

#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Rotation", extends = Transform)]
pub struct Rotation {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify)]
    pub origin: Vector3d,
    #[qml(notify)]
    pub angle: f64,
    #[qml(notify)]
    pub axis: Vector3d,
    #[qml(notify)]
    pub distance_to_plane: f64,
}

impl Default for Rotation {
    fn default() -> Self {
        Self {
            transform: Transform::default(),
            origin: Vector3d::default(),
            angle: 0.0,
            axis: Vector3d {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            distance_to_plane: DEFAULT_DISTANCE_TO_PLANE,
        }
    }
}

#[qml_impl]
impl Rotation {}

#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Scale", extends = Transform)]
pub struct Scale {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify)]
    pub origin: Vector3d,
    #[qml(notify)]
    pub x_scale: f64,
    #[qml(notify)]
    pub y_scale: f64,
    #[qml(notify)]
    pub z_scale: f64,
}

impl Default for Scale {
    fn default() -> Self {
        Self {
            transform: Transform::default(),
            origin: Vector3d::default(),
            x_scale: 1.0,
            y_scale: 1.0,
            z_scale: 1.0,
        }
    }
}

#[qml_impl]
impl Scale {
    /// Any of the three factors changed.
    #[qml(signal, name = "scaleChanged")]
    fn scale_changed();
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Translate", extends = Transform)]
pub struct Translate {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
}

#[qml_impl]
impl Translate {}

/// `matrix` holds what `Qt.matrix4x4()` makes: an object with the members
/// `m11` to `m44`, row by row.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Matrix4x4", extends = Transform)]
pub struct Matrix4x4 {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify)]
    pub matrix: Value,
}

#[qml_impl]
impl Matrix4x4 {}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Shear", extends = Transform)]
pub struct Shear {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify)]
    pub origin: Vector3d,
    #[qml(notify)]
    pub x_factor: f64,
    #[qml(notify)]
    pub y_factor: f64,
    #[qml(notify)]
    pub x_angle: f64,
    #[qml(notify)]
    pub y_angle: f64,
}

#[qml_impl]
impl Shear {}

/// A mirror about the centre of the item that lists it: what a
/// `Flipable` gives its back. Not a QML type.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "QQuickLocalTransform", extends = Transform)]
pub struct LocalTransform {
    #[qml(base)]
    pub transform: Transform,
    #[qml(notify, readonly)]
    pub mirror_x: bool,
    #[qml(notify, readonly)]
    pub mirror_y: bool,
}

#[qml_impl]
impl LocalTransform {}

fn vec3(e: &Engine, obj: ObjectId, p: ljos_engine::PropIndex) -> Vector3d {
    match e.read(obj, p) {
        Value::Vector3d(v) => v,
        _ => Vector3d::default(),
    }
}

/// The 2D picture of a turn of `angle` degrees about `axis` through the
/// plane's origin, seen from `distance` away.
pub fn rotation(angle: f64, axis: [f64; 3], distance: f64) -> Projective {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len < 1e-12 || angle == 0.0 {
        return Projective::IDENTITY;
    }
    let (x, y, z) = (axis[0] / len, axis[1] / len, axis[2] / len);
    let (s, c) = angle.to_radians().sin_cos();
    let t = 1.0 - c;
    // Rodrigues: the right-handed turn about (x, y, z).
    let r = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ];
    // The point's own z is 0; only its x and y columns matter, and the
    // depth it is turned to becomes the divisor.
    let d = if distance.abs() < 1e-12 {
        DEFAULT_DISTANCE_TO_PLANE
    } else {
        distance
    };
    Projective([
        [r[0][0], r[0][1], 0.0],
        [r[1][0], r[1][1], 0.0],
        [-r[2][0] / d, -r[2][1] / d, 1.0],
    ])
}

fn about(origin: Vector3d, m: Projective) -> Projective {
    let (ox, oy) = (f64::from(origin.x), f64::from(origin.y));
    Projective::translate(ox, oy)
        .then_after(&m)
        .then_after(&Projective::translate(-ox, -oy))
}

/// The members `m11`..`m44` of a matrix value; `None` when it is not one.
fn matrix_of(e: &Engine, v: &Value) -> Option<[f64; 16]> {
    let mut m = [0.0; 16];
    match v {
        Value::List(l) if l.len() == 16 => {
            for (i, x) in l.iter().enumerate() {
                m[i] = x.to_number();
            }
        }
        Value::Js(_) => {
            for r in 0..4 {
                for c in 0..4 {
                    m[r * 4 + c] = e.js_member(v, &format!("m{}{}", r + 1, c + 1))?.to_number();
                }
            }
        }
        _ => return None,
    }
    Some(m)
}

/// The map one entry of a `transform` list makes; `None` for an object
/// that is not a transform.
pub fn of_transform(e: &Engine, t: ObjectId) -> Option<Projective> {
    let meta = e.meta_of(t)?;
    if meta.inherits(Translate::static_meta()) {
        let x = num(e, t, Translate::X.index);
        let y = num(e, t, Translate::Y.index);
        return Some(Projective::translate(x, y));
    }
    if meta.inherits(Scale::static_meta()) {
        let sx = num(e, t, Scale::X_SCALE.index);
        let sy = num(e, t, Scale::Y_SCALE.index);
        let o = vec3(e, t, Scale::ORIGIN.index);
        return Some(about(
            o,
            Projective::from_affine(Affine::scale_non_uniform(sx, sy)),
        ));
    }
    if meta.inherits(Rotation::static_meta()) {
        let angle = num(e, t, Rotation::ANGLE.index);
        let a = vec3(e, t, Rotation::AXIS.index);
        let d = num(e, t, Rotation::DISTANCE_TO_PLANE.index);
        let o = vec3(e, t, Rotation::ORIGIN.index);
        let axis = [f64::from(a.x), f64::from(a.y), f64::from(a.z)];
        return Some(about(o, rotation(angle, axis, d)));
    }
    if meta.inherits(Shear::static_meta()) {
        // A factor and an angle add up (measured on qml6).
        let fx =
            num(e, t, Shear::X_FACTOR.index) + num(e, t, Shear::X_ANGLE.index).to_radians().tan();
        let fy =
            num(e, t, Shear::Y_FACTOR.index) + num(e, t, Shear::Y_ANGLE.index).to_radians().tan();
        let o = vec3(e, t, Shear::ORIGIN.index);
        return Some(about(
            o,
            Projective([[1.0, fx, 0.0], [fy, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        ));
    }
    if meta.inherits(Matrix4x4::static_meta()) {
        let v = e.read(t, Matrix4x4::MATRIX.index);
        let Some(m) = matrix_of(e, &v) else {
            return Some(Projective::IDENTITY);
        };
        // `(x, y, 0, 1)`: the third row and column drop out.
        return Some(Projective([
            [m[0], m[1], m[3]],
            [m[4], m[5], m[7]],
            [m[12], m[13], m[15]],
        ]));
    }
    None
}

/// The map of an item's `transform` list, the first entry applied first.
/// Tracked: a binding or effect that asks follows every entry.
pub fn list_map(e: &Engine, obj: ObjectId) -> Projective {
    let Value::List(list) = e.read(obj, Item::TRANSFORM.index) else {
        return Projective::IDENTITY;
    };
    let mut m = Projective::IDENTITY;
    for t in list.iter().filter_map(Value::as_object) {
        let step = if is_a::<LocalTransform>(e, t) {
            let sx = if e.read(t, LocalTransform::MIRROR_X.index).to_boolean() {
                -1.0
            } else {
                1.0
            };
            let sy = if e.read(t, LocalTransform::MIRROR_Y.index).to_boolean() {
                -1.0
            } else {
                1.0
            };
            if sx == 1.0 && sy == 1.0 {
                continue;
            }
            let (w, h) = (
                num(e, obj, Item::WIDTH.index),
                num(e, obj, Item::HEIGHT.index),
            );
            Some(about(
                Vector3d {
                    x: (w / 2.0) as f32,
                    y: (h / 2.0) as f32,
                    z: 0.0,
                },
                Projective::from_affine(Affine::scale_non_uniform(sx, sy)),
            ))
        } else {
            of_transform(e, t)
        };
        if let Some(step) = step {
            m = step.then_after(&m);
        }
    }
    m
}

/// The map of an item's `rotation`, `scale` and `transform` together,
/// without its place in its parent. Tracked.
pub(crate) fn own_map(e: &Engine, obj: ObjectId) -> Projective {
    let w = num(e, obj, Item::WIDTH.index);
    let h = num(e, obj, Item::HEIGHT.index);
    let rotation = num(e, obj, Item::ROTATION.index);
    let scale = num(e, obj, Item::SCALE.index);
    let origin = num(e, obj, Item::TRANSFORM_ORIGIN.index) as i32;
    let own = Projective::from_affine(compose_transform(0.0, 0.0, w, h, rotation, scale, origin));
    list_map(e, obj).then_after(&own)
}

/// The affine map of an item to its parent, with the list fitted (see the
/// module's documentation) when it has a perspective.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compose(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
    scale: f64,
    origin: i32,
    list: &Projective,
) -> Affine {
    if list.is_identity() {
        return compose_transform(x, y, w, h, rotation, scale, origin);
    }
    let own = Projective::from_affine(compose_transform(0.0, 0.0, w, h, rotation, scale, origin));
    Affine::translate((x, y)) * list.then_after(&own).fit(w, h)
}

/// The exact map of an item to its parent.
pub(crate) fn local_map(e: &Engine, obj: ObjectId) -> Projective {
    let x = num(e, obj, Item::X.index);
    let y = num(e, obj, Item::Y.index);
    Projective::translate(x, y).then_after(&own_map(e, obj))
}

/// The exact map of an item to the scene.
pub(crate) fn scene_map(e: &Engine, obj: ObjectId) -> Projective {
    let mut t = Projective::IDENTITY;
    let mut cur = Some(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let Some(o) = cur else { break };
        t = local_map(e, o).then_after(&t);
        cur = e.visual_parent(o);
    }
    t
}

/// `Qt.matrix4x4()`: what `Matrix4x4.matrix` takes. With no arguments the
/// identity; with sixteen numbers (or an array of them) row by row.
const INSTALL: &str = r#"(function () {
  if ("matrix4x4" in Qt) return;
  const names = [];
  for (let r = 1; r <= 4; r++) for (let c = 1; c <= 4; c++) names.push("m" + r + c);
  Object.defineProperty(Qt, "matrix4x4", {
    enumerable: true,
    value: function () {
      let v = arguments.length === 1 && Array.isArray(arguments[0])
        ? arguments[0] : Array.prototype.slice.call(arguments);
      if (v.length !== 16) v = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
      const m = {};
      for (let i = 0; i < 16; i++) m[names[i]] = Number(v[i]);
      Object.defineProperty(m, "toString", {
        value: function () {
          return "QMatrix4x4(" + names.map((n) => this[n]).join(", ") + ")";
        },
      });
      return m;
    },
  });
})"#;

pub(crate) fn register(engine: &Engine) {
    for meta in [
        Rotation::static_meta(),
        Scale::static_meta(),
        Translate::static_meta(),
        Matrix4x4::static_meta(),
        Shear::static_meta(),
    ] {
        engine.register_type("QtQuick", Version::new(2, 15), MetaRef::Static(meta));
    }
    engine.on_js_started(|e| {
        let r = e.eval_js(INSTALL).map(|f| e.call_js(&f, &[]));
        if let Err(err) = r {
            e.log(
                LogLevel::Warning,
                crate::scene::SCENE_CATEGORY,
                format!("Qt.matrix4x4 is not available: {err}"),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(p: Point, x: f64, y: f64) {
        assert!(
            (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3,
            "{p:?} is not ({x}, {y})"
        );
    }

    #[test]
    fn a_turn_about_y_is_a_perspective() {
        let m = about(
            Vector3d {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            rotation(60.0, [0.0, 1.0, 0.0], 1024.0),
        );
        // What qml6 maps these corners to.
        close(m.apply(Point::new(0.0, 0.0)).unwrap(), 23.896, -2.208);
        close(m.apply(Point::new(100.0, 0.0)).unwrap(), 73.986, 2.029);
    }

    #[test]
    fn a_turn_about_a_slanted_axis() {
        let m = rotation(45.0, [1.0, 1.0, 0.0], 1024.0);
        close(m.apply(Point::new(100.0, 0.0)).unwrap(), 81.382, 13.963);
        close(m.apply(Point::new(0.0, 100.0)).unwrap(), 15.396, 89.737);
    }

    #[test]
    fn a_nearer_plane_is_a_stronger_perspective() {
        let m = about(
            Vector3d {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            rotation(60.0, [0.0, 1.0, 0.0], 200.0),
        );
        close(m.apply(Point::new(0.0, 0.0)).unwrap(), 18.092, -13.817);
    }

    #[test]
    fn the_inverse_undoes_the_map() {
        let m = rotation(30.0, [1.0, 0.0, 0.0], 1024.0);
        let back = m.inverse().unwrap();
        let p = Point::new(12.0, 34.0);
        close(back.apply(m.apply(p).unwrap()).unwrap(), 12.0, 34.0);
    }

    #[test]
    fn an_affine_map_is_fitted_exactly() {
        let a = Affine::translate((3.0, 4.0)) * Affine::rotate(0.3) * Affine::scale(2.0);
        let f = Projective::from_affine(a).fit(10.0, 20.0);
        for (c, d) in a.as_coeffs().iter().zip(f.as_coeffs()) {
            assert!((c - d).abs() < 1e-12);
        }
    }

    #[test]
    fn a_perspective_is_fitted_close_to_its_corners() {
        let m = about(
            Vector3d {
                x: 50.0,
                y: 50.0,
                z: 0.0,
            },
            rotation(60.0, [0.0, 1.0, 0.0], 1024.0),
        );
        let fit = m.fit(100.0, 100.0);
        let mut worst: f64 = 0.0;
        for (x, y) in [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0), (100.0, 100.0)] {
            let p = Point::new(x, y);
            worst = worst.max((fit * p - m.apply(p).unwrap()).hypot());
        }
        // A corner of the 100-point square is off by about two points.
        assert!(worst < 2.5, "{worst}");
        // The centre, which the turn keeps, drifts by about a point.
        let c = fit * Point::new(50.0, 50.0);
        assert!((c - Point::new(50.0, 50.0)).hypot() < 1.5, "{c:?}");
    }
}
