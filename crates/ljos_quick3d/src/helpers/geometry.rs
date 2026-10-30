//! The Helpers' shape geometries: `CuboidGeometry`, `SphereGeometry`,
//! `CylinderGeometry`, `ConeGeometry`, `PlaneGeometry`, `TorusGeometry`,
//! `CapsuleGeometry` and `GridGeometry`.
//!
//! Each makes its mesh (`shapes.rs`) when it is complete and again when a
//! property changes, and commits it to the `Geometry` it is, so the models
//! that draw it are drawn again. The meshes are generated at once: with
//! `asynchronous` set, `status` is `Ready` by the time the object is, which
//! is a state the documentation allows, and `Error` stands for parameters
//! that make no shape.

use ljos::prelude::*;

use super::mesh::Built;
use super::shapes::{self, Capsule, Plane, UvProfile};
use crate::node::Geometry;

/// `status`'s values, the same on every geometry that has one.
pub(super) fn status_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Status",
        vec![("Null", 0), ("Loading", 1), ("Ready", 2), ("Error", 3)],
    )]
}

pub(super) const NULL: i32 = 0;
pub(super) const READY: i32 = 2;
pub(super) const ERROR: i32 = 3;

/// What a generated geometry shares: where its `Geometry` and its
/// "complete" flag are, and how it makes its mesh from its properties.
pub(super) trait Generated: QmlType + 'static {
    /// `None` for a geometry that has no `status`.
    const STATUS: Option<Prop<i32>>;
    fn geometry(&mut self) -> &mut Geometry;
    fn complete(&mut self) -> &mut bool;
    /// `None` when a property could not be read.
    fn built(cx: &Cx) -> Option<Built>;
}

/// Makes the mesh again and commits it, once the object is complete.
pub(super) fn rebuild<T: Generated>(cx: &Cx) {
    if !cx.with_mut(|t: &mut T| *t.complete()).unwrap_or(false) {
        return;
    }
    let Some(built) = T::built(cx) else {
        return;
    };
    let status = if built.is_empty() { ERROR } else { READY };
    let _ = cx.with_mut(|t: &mut T| built.into_geometry(t.geometry()));
    Geometry::update(cx.engine, cx.this);
    if let Some(p) = T::STATUS {
        let _ = cx.set(p, status);
    }
}

pub(super) fn changed<T: Generated>(cx: &Cx, _: PropIndex) {
    rebuild::<T>(cx);
}

pub(super) fn complete<T: Generated>(cx: &Cx) {
    let _ = cx.with_mut(|t: &mut T| *t.complete() = true);
    rebuild::<T>(cx);
}

/// Wires a type's `Generated` impl to its two fields.
macro_rules! generated {
    ($ty:ident, $status:expr, |$cx:ident| $built:expr) => {
        impl Generated for $ty {
            const STATUS: Option<Prop<i32>> = $status;
            fn geometry(&mut self) -> &mut Geometry {
                &mut self.geometry
            }
            fn complete(&mut self) -> &mut bool {
                &mut self.complete
            }
            fn built($cx: &Cx) -> Option<Built> {
                Some($built)
            }
        }

        #[qml_impl]
        impl $ty {
            #[qml(on_complete)]
            fn completed(cx: &Cx) {
                complete::<$ty>(cx);
            }
        }
    };
}

/// `CuboidGeometry`: a box centred on the origin.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "CuboidGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct CuboidGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    x_extent: f64,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    y_extent: f64,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    z_extent: f64,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    xy_mesh_resolution: Size,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    xz_mesh_resolution: Size,
    #[qml(notify, change = changed::<CuboidGeometry>)]
    yz_mesh_resolution: Size,
}

impl Default for CuboidGeometry {
    fn default() -> Self {
        let two = Size {
            width: 2.0,
            height: 2.0,
        };
        CuboidGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            x_extent: 100.0,
            y_extent: 100.0,
            z_extent: 100.0,
            xy_mesh_resolution: two,
            xz_mesh_resolution: two,
            yz_mesh_resolution: two,
        }
    }
}

generated!(CuboidGeometry, Some(CuboidGeometry::STATUS), |cx| {
    let e = |p| cx.get(p).unwrap_or_default() as f32;
    let res = |p| {
        let s: Size = cx.get(p).unwrap_or_default();
        (s.width, s.height)
    };
    shapes::cuboid(
        [
            e(CuboidGeometry::X_EXTENT),
            e(CuboidGeometry::Y_EXTENT),
            e(CuboidGeometry::Z_EXTENT),
        ],
        res(CuboidGeometry::XY_MESH_RESOLUTION),
        res(CuboidGeometry::XZ_MESH_RESOLUTION),
        res(CuboidGeometry::YZ_MESH_RESOLUTION),
    )
});

/// `SphereGeometry`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "SphereGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct SphereGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<SphereGeometry>)]
    radius: f64,
    #[qml(notify, change = changed::<SphereGeometry>)]
    rings: i32,
    #[qml(notify, change = changed::<SphereGeometry>)]
    segments: i32,
}

impl Default for SphereGeometry {
    fn default() -> Self {
        SphereGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            radius: 100.0,
            rings: 16,
            segments: 32,
        }
    }
}

generated!(SphereGeometry, Some(SphereGeometry::STATUS), |cx| {
    shapes::sphere(
        cx.get(SphereGeometry::RADIUS).unwrap_or_default() as f32,
        cx.get(SphereGeometry::RINGS).unwrap_or_default(),
        cx.get(SphereGeometry::SEGMENTS).unwrap_or_default(),
    )
});

/// `CylinderGeometry`: along the Y axis, centred on the origin.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "CylinderGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct CylinderGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<CylinderGeometry>)]
    radius: f64,
    #[qml(notify, change = changed::<CylinderGeometry>)]
    length: f64,
    #[qml(notify, change = changed::<CylinderGeometry>)]
    rings: i32,
    #[qml(notify, change = changed::<CylinderGeometry>)]
    segments: i32,
}

impl Default for CylinderGeometry {
    fn default() -> Self {
        CylinderGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            radius: 100.0,
            length: 200.0,
            rings: 0,
            segments: 32,
        }
    }
}

generated!(CylinderGeometry, Some(CylinderGeometry::STATUS), |cx| {
    let r = cx.get(CylinderGeometry::RADIUS).unwrap_or_default() as f32;
    shapes::cone(
        r,
        r,
        cx.get(CylinderGeometry::LENGTH).unwrap_or_default() as f32,
        cx.get(CylinderGeometry::RINGS).unwrap_or_default(),
        cx.get(CylinderGeometry::SEGMENTS).unwrap_or_default(),
    )
});

/// `ConeGeometry`: a cone, or with a top radius a truncated one, along the
/// Y axis.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "ConeGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct ConeGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<ConeGeometry>)]
    top_radius: f64,
    #[qml(notify, change = changed::<ConeGeometry>)]
    bottom_radius: f64,
    #[qml(notify, change = changed::<ConeGeometry>)]
    length: f64,
    #[qml(notify, change = changed::<ConeGeometry>)]
    rings: i32,
    #[qml(notify, change = changed::<ConeGeometry>)]
    segments: i32,
}

impl Default for ConeGeometry {
    fn default() -> Self {
        // The documentation gives no defaults; these draw a cone as
        // `#Cone` does.
        ConeGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            top_radius: 0.0,
            bottom_radius: 100.0,
            length: 200.0,
            rings: 0,
            segments: 32,
        }
    }
}

generated!(ConeGeometry, Some(ConeGeometry::STATUS), |cx| shapes::cone(
    cx.get(ConeGeometry::TOP_RADIUS).unwrap_or_default() as f32,
    cx.get(ConeGeometry::BOTTOM_RADIUS).unwrap_or_default() as f32,
    cx.get(ConeGeometry::LENGTH).unwrap_or_default() as f32,
    cx.get(ConeGeometry::RINGS).unwrap_or_default(),
    cx.get(ConeGeometry::SEGMENTS).unwrap_or_default(),
));

fn plane_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let mut e = status_enums();
    e.push(("Plane", vec![("XY", 0), ("XZ", 1), ("ZY", 2)]));
    e
}

/// `PlaneGeometry`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "PlaneGeometry",
    extends = Geometry,
    enums = plane_enums
)]
pub struct PlaneGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<PlaneGeometry>)]
    width: f64,
    #[qml(notify, change = changed::<PlaneGeometry>)]
    height: f64,
    #[qml(notify, change = changed::<PlaneGeometry>)]
    mesh_resolution: Size,
    #[qml(notify, enumeration = "Plane", change = changed::<PlaneGeometry>)]
    plane: i32,
    #[qml(notify, change = changed::<PlaneGeometry>)]
    reversed: bool,
    #[qml(notify, change = changed::<PlaneGeometry>)]
    mirrored: bool,
}

impl Default for PlaneGeometry {
    fn default() -> Self {
        PlaneGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            width: 100.0,
            height: 100.0,
            mesh_resolution: Size {
                width: 2.0,
                height: 2.0,
            },
            plane: 0,
            reversed: false,
            mirrored: false,
        }
    }
}

generated!(PlaneGeometry, Some(PlaneGeometry::STATUS), |cx| {
    let res = cx.get(PlaneGeometry::MESH_RESOLUTION).unwrap_or_default();
    shapes::plane(
        cx.get(PlaneGeometry::WIDTH).unwrap_or_default() as f32,
        cx.get(PlaneGeometry::HEIGHT).unwrap_or_default() as f32,
        (res.width, res.height),
        match cx.get(PlaneGeometry::PLANE).unwrap_or_default() {
            1 => Plane::Xz,
            2 => Plane::Zy,
            _ => Plane::Xy,
        },
        cx.get(PlaneGeometry::REVERSED).unwrap_or_default(),
        cx.get(PlaneGeometry::MIRRORED).unwrap_or_default(),
    )
});

/// `TorusGeometry`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "TorusGeometry",
    extends = Geometry,
    enums = status_enums
)]
pub struct TorusGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<TorusGeometry>)]
    radius: f64,
    #[qml(notify, change = changed::<TorusGeometry>)]
    tube_radius: f64,
    #[qml(notify, change = changed::<TorusGeometry>)]
    rings: i32,
    #[qml(notify, change = changed::<TorusGeometry>)]
    segments: i32,
}

impl Default for TorusGeometry {
    fn default() -> Self {
        TorusGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: false,
            status: NULL,
            radius: 100.0,
            tube_radius: 10.0,
            rings: 50,
            segments: 50,
        }
    }
}

generated!(TorusGeometry, Some(TorusGeometry::STATUS), |cx| {
    shapes::torus(
        cx.get(TorusGeometry::RADIUS).unwrap_or_default() as f32,
        cx.get(TorusGeometry::TUBE_RADIUS).unwrap_or_default() as f32,
        cx.get(TorusGeometry::RINGS).unwrap_or_default(),
        cx.get(TorusGeometry::SEGMENTS).unwrap_or_default(),
    )
});

fn capsule_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let mut e = status_enums();
    e.push((
        "UVProfile",
        vec![("Fixed", 0), ("Aspect", 1), ("Uniform", 2)],
    ));
    e
}

/// `CapsuleGeometry`: along the X axis.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "CapsuleGeometry",
    extends = Geometry,
    enums = capsule_enums
)]
pub struct CapsuleGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify)]
    asynchronous: bool,
    #[qml(notify, readonly, enumeration = "Status")]
    status: i32,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    height: f64,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    diameter: f64,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    latitudes: i32,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    longitudes: i32,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    rings: i32,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    enable_normals: bool,
    #[qml(notify, change = changed::<CapsuleGeometry>)]
    enable_u_v: bool,
    #[qml(notify, enumeration = "UVProfile", change = changed::<CapsuleGeometry>)]
    uv_profile: i32,
}

impl Default for CapsuleGeometry {
    fn default() -> Self {
        CapsuleGeometry {
            geometry: Geometry::default(),
            complete: false,
            asynchronous: true,
            status: NULL,
            height: 100.0,
            diameter: 100.0,
            latitudes: 16,
            longitudes: 32,
            rings: 1,
            enable_normals: true,
            enable_u_v: false,
            uv_profile: 0,
        }
    }
}

generated!(CapsuleGeometry, Some(CapsuleGeometry::STATUS), |cx| {
    let i = |p| cx.get(p).unwrap_or_default();
    Capsule {
        height: cx.get(CapsuleGeometry::HEIGHT).unwrap_or_default() as f32,
        diameter: cx.get(CapsuleGeometry::DIAMETER).unwrap_or_default() as f32,
        latitudes: i(CapsuleGeometry::LATITUDES),
        longitudes: i(CapsuleGeometry::LONGITUDES),
        rings: i(CapsuleGeometry::RINGS),
        normals: cx.get(CapsuleGeometry::ENABLE_NORMALS).unwrap_or_default(),
        uv: cx.get(CapsuleGeometry::ENABLE_U_V).unwrap_or_default(),
        profile: match i(CapsuleGeometry::UV_PROFILE) {
            1 => UvProfile::Aspect,
            2 => UvProfile::Uniform,
            _ => UvProfile::Fixed,
        },
    }
    .build()
});

/// `GridGeometry`: line segments, in the XY plane, centred on the origin.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "GridGeometry", extends = Geometry)]
pub struct GridGeometry {
    #[qml(base)]
    geometry: Geometry,
    #[qml(skip)]
    complete: bool,
    #[qml(notify, change = changed::<GridGeometry>)]
    horizontal_lines: i32,
    #[qml(notify, change = changed::<GridGeometry>)]
    horizontal_step: f64,
    #[qml(notify, change = changed::<GridGeometry>)]
    vertical_lines: i32,
    #[qml(notify, change = changed::<GridGeometry>)]
    vertical_step: f64,
}

impl Default for GridGeometry {
    fn default() -> Self {
        GridGeometry {
            geometry: Geometry::default(),
            complete: false,
            horizontal_lines: 1000,
            horizontal_step: 0.1,
            vertical_lines: 1000,
            vertical_step: 0.1,
        }
    }
}

generated!(GridGeometry, None, |cx| shapes::grid(
    cx.get(GridGeometry::HORIZONTAL_LINES).unwrap_or_default(),
    cx.get(GridGeometry::HORIZONTAL_STEP).unwrap_or_default() as f32,
    cx.get(GridGeometry::VERTICAL_LINES).unwrap_or_default(),
    cx.get(GridGeometry::VERTICAL_STEP).unwrap_or_default() as f32,
));
