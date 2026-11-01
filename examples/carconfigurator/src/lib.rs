//! The Rust backend of the qtdoc **car-configurator** example
//! (`corpus/qtdoc/examples/demos/car-configurator`).
//!
//! Its snow keeps out of the car by `ShapeAffector3D`, a native type of the
//! module `Quick3DAssets.Snow` its QML files belong to. The interface comes
//! from what those files set (CLEAN_ROOM.md: the C++ was never opened): a
//! shape of 100 across, placed as the affector is, and what becomes of a
//! particle inside it, by default that it cannot be seen.
//!
//! The shape is read when the affector is made and when one of its own
//! properties changes: the example never moves one.

use std::sync::Arc;

use ljos::item::ljos_3d::math::{Mat4, Quat, Vec3};
use ljos::item::ljos_3d::particles::{Current, CustomAffect};
use ljos::prelude::*;
use ljos_quick3d::Affector3D;

/// Half the size of a shape, in the affector's own space.
const REACH: f32 = 50.0;

const POSITION: i32 = 1;
const ROTATION: i32 = 4;
const SCALE: i32 = 8;
const COLOR: i32 = 16;

/// Registers `ShapeAffector3D` on `engine`.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Quick3DAssets.Snow" 1.0 => [ShapeAffector3D]));
}

fn enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ShapeType",
            vec![("Sphere", 0), ("Box", 1), ("Cylinder", 2)],
        ),
        (
            "AffectFlag",
            vec![
                ("AffectNone", 0),
                ("AffectPosition", i64::from(POSITION)),
                ("AffectVelocity", 2),
                ("AffectRotation", i64::from(ROTATION)),
                ("AffectScale", i64::from(SCALE)),
                ("AffectColor", i64::from(COLOR)),
            ],
        ),
    ]
}

/// `ShapeAffector3D`. `targetVelocity` is kept without effect: a particle's
/// place is worked out from its start, so nothing it is told now can change
/// what it had been moving at.
#[derive(Debug, QmlObject)]
#[qml(module = "Quick3DAssets.Snow", name = "ShapeAffector3D", extends = Affector3D, enums = enums)]
pub struct ShapeAffector3D {
    #[qml(base)]
    pub affector: Affector3D,
    #[qml(notify, change = changed)]
    pub shape_type: i32,
    #[qml(notify, change = changed)]
    pub inverted: bool,
    #[qml(notify, change = changed)]
    pub target_position: Vector3d,
    #[qml(notify)]
    pub target_velocity: Vector3d,
    #[qml(notify, change = changed)]
    pub target_rotation: Vector3d,
    #[qml(notify, change = changed)]
    pub target_scale: Vector3d,
    #[qml(notify, change = changed)]
    pub target_color: Color,
    #[qml(notify, change = changed)]
    pub affect_flags: i32,
}

impl Default for ShapeAffector3D {
    fn default() -> Self {
        ShapeAffector3D {
            affector: Affector3D::default(),
            shape_type: 0,
            inverted: false,
            target_position: Vector3d::default(),
            target_velocity: Vector3d::default(),
            target_rotation: Vector3d::default(),
            target_scale: Vector3d {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            target_color: Color::TRANSPARENT,
            affect_flags: COLOR,
        }
    }
}

#[qml_impl]
impl ShapeAffector3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        install(cx.engine, cx.this);
        Ok(())
    }
}

fn changed(cx: &Cx<'_>, _: PropIndex) {
    install(cx.engine, cx.this);
}

fn vec3(v: Vector3d) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Whether a point in the shape's own space is in it.
fn inside(shape: i32, p: Vec3) -> bool {
    let round = |a: f32, b: f32| a * a + b * b <= REACH * REACH;
    match shape {
        0 => p.x * p.x + p.y * p.y + p.z * p.z <= REACH * REACH,
        1 => p.x.abs() <= REACH && p.y.abs() <= REACH && p.z.abs() <= REACH,
        _ => round(p.x, p.z) && p.y.abs() <= REACH,
    }
}

fn install(e: &Engine, me: ObjectId) {
    let Ok(made) = e.with(me, |a: &ShapeAffector3D| {
        let node = &a.affector.node;
        let place = Mat4::from_trs(
            vec3(node.position),
            Quat::from_euler_degrees(vec3(node.euler_rotation)),
            vec3(node.scale),
        );
        (
            place.inverse(),
            a.shape_type,
            a.inverted,
            vec3(a.target_position),
            vec3(a.target_rotation),
            a.target_scale.x,
            a.target_color,
            a.affect_flags,
        )
    }) else {
        return;
    };
    let (Some(to_shape), shape, inverted, position, rotation, scale, color, flags) = made else {
        // A shape scaled to nothing has no inside.
        return;
    };
    let color = [
        (color.r >> 8) as u8,
        (color.g >> 8) as u8,
        (color.b >> 8) as u8,
        (color.a >> 8) as u8,
    ];
    let affect = move |_: u32, _: &_, c: &mut Current| {
        if inside(shape, to_shape.transform_point(c.position)) == inverted {
            return;
        }
        if flags & POSITION != 0 {
            c.position = position;
        }
        if flags & ROTATION != 0 {
            c.rotation = rotation;
        }
        if flags & SCALE != 0 {
            c.scale = scale;
        }
        if flags & COLOR != 0 {
            c.color = color;
        }
    };
    Affector3D::set_custom(e, me, CustomAffect(Arc::new(affect)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapes_are_100_across() {
        let at = |x, y, z| Vec3::new(x, y, z);
        assert!(inside(0, at(28.0, 28.0, 28.0)));
        assert!(!inside(0, at(40.0, 40.0, 0.0)));
        assert!(inside(1, at(49.0, 49.0, 49.0)));
        assert!(!inside(1, at(51.0, 0.0, 0.0)));
        assert!(inside(2, at(30.0, 49.0, 30.0)));
        assert!(!inside(2, at(40.0, 0.0, 40.0)));
        assert!(!inside(2, at(0.0, 51.0, 0.0)));
    }
}
