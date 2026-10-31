//! `Quaternion`: the turns QML makes by name, which the `quaternion` value
//! has no way to say of itself.

use std::sync::Arc;

use ljos::item::ljos_3d::math::{Quat, Vec3};
use ljos::prelude::*;

/// `Quaternion` (QtQuick3D).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "Quaternion")]
#[qml_singleton]
pub struct Quaternion {}

fn number(v: &Value) -> f32 {
    v.as_number().unwrap_or(0.0) as f32
}

fn vector(v: &Value) -> Option<Vec3> {
    match v {
        Value::Vector3d(v) => Some(Vec3::new(v.x, v.y, v.z)),
        _ => None,
    }
}

fn quaternion(q: Quat) -> Value {
    Value::Quaternion(Arc::new(ljos::prelude::Quaternion {
        scalar: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }))
}

/// A turn about `axis`; about nothing for an axis of no length.
fn turn(axis: Vec3, degrees: f32) -> Quat {
    if axis.length() == 0.0 {
        return Quat::IDENTITY;
    }
    Quat::from_axis_angle(axis, degrees.to_radians())
}

#[qml_impl]
impl Quaternion {
    /// `fromEulerAngles(vector3d)` or `fromEulerAngles(x, y, z)`.
    #[qml(invokable, name = "fromEulerAngles")]
    fn from_euler_angles(_cx: &Cx, x: Value, y: Value, z: Value) -> Value {
        let angles = vector(&x).unwrap_or_else(|| Vec3::new(number(&x), number(&y), number(&z)));
        quaternion(Quat::from_euler_degrees(angles))
    }

    /// `fromAxisAndAngle(vector3d, angle)` or `fromAxisAndAngle(x, y, z, angle)`.
    #[qml(invokable, name = "fromAxisAndAngle")]
    fn from_axis_and_angle(_cx: &Cx, a: Value, b: Value, c: Value, d: Value) -> Value {
        quaternion(match vector(&a) {
            Some(axis) => turn(axis, number(&b)),
            None => turn(Vec3::new(number(&a), number(&b), number(&c)), number(&d)),
        })
    }

    /// Two turns or three, the first of them made first.
    #[qml(invokable, name = "fromAxesAndAngles")]
    fn from_axes_and_angles(
        _cx: &Cx,
        axis1: Value,
        angle1: Value,
        axis2: Value,
        angle2: Value,
        axis3: Value,
        angle3: Value,
    ) -> Value {
        let mut made = Quat::IDENTITY;
        for (axis, angle) in [(axis1, angle1), (axis2, angle2), (axis3, angle3)] {
            if let Some(axis) = vector(&axis) {
                made = turn(axis, number(&angle)) * made;
            }
        }
        quaternion(made)
    }

    /// The turn that points what faces `forward` from `source` at `target`.
    /// Facing straight at it or straight away there is no one line to turn
    /// about, and it is `up`.
    #[qml(invokable, name = "lookAt")]
    fn look_at(_cx: &Cx, source: Value, target: Value, forward: Value, up: Value) -> Value {
        let (Some(source), Some(target)) = (vector(&source), vector(&target)) else {
            return quaternion(Quat::IDENTITY);
        };
        let forward = vector(&forward).unwrap_or(Vec3::new(0.0, 0.0, -1.0));
        let up = vector(&up).unwrap_or(Vec3::new(0.0, 1.0, 0.0));
        let toward = (target - source).normalized();
        let ahead = forward.normalized();
        let mut axis = ahead.cross(toward);
        if axis.length() <= 0.00001 {
            axis = up;
        }
        let cosine = ahead.dot(toward).clamp(-1.0, 1.0);
        quaternion(turn(axis, cosine.acos().to_degrees()))
    }
}
