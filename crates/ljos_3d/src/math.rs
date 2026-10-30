//! The little linear algebra a scene needs, in Qt Quick 3D's conventions:
//! a right-handed space with Y up, and Euler angles in degrees applied
//! about Z, then X, then Y.
//!
//! Its own rather than a math crate's because the scene needs a dozen
//! operations, and every one of them is checked against what Qt documents.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// The zero vector stays zero rather than becoming NaN.
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 0.0 { self * (1.0 / l) } else { self }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// A unit quaternion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn from_axis_angle(axis: Vec3, radians: f32) -> Quat {
        let a = axis.normalized();
        let (s, c) = (radians * 0.5).sin_cos();
        Quat {
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
            w: c,
        }
    }

    /// `Node.eulerRotation`: degrees about X, Y and Z, which Qt documents as
    /// applied in the order Z, X, Y.
    pub fn from_euler_degrees(e: Vec3) -> Quat {
        let rx = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), e.x.to_radians());
        let ry = Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), e.y.to_radians());
        let rz = Quat::from_axis_angle(Vec3::new(0.0, 0.0, 1.0), e.z.to_radians());
        ry * rx * rz
    }

    /// The `eulerRotation` that gives this rotation: degrees about X, Y
    /// and Z, X within ±90°. Where X is ±90° the Z turn is folded into Y.
    pub fn to_euler_degrees(self) -> Vec3 {
        let Quat { x, y, z, w } = self;
        // The rotation matrix's entries that Y·X·Z leaves simple.
        let m12 = 2.0 * (y * z - w * x);
        let m02 = 2.0 * (x * z + w * y);
        let m22 = 1.0 - 2.0 * (x * x + y * y);
        let m10 = 2.0 * (x * y + w * z);
        let m11 = 1.0 - 2.0 * (x * x + z * z);
        let ax = (-m12).clamp(-1.0, 1.0).asin();
        let (ay, az) = if m12.abs() < 0.999_999 {
            (m02.atan2(m22), m10.atan2(m11))
        } else {
            let m20 = 2.0 * (x * z - w * y);
            let m00 = 1.0 - 2.0 * (y * y + z * z);
            ((-m20).atan2(m00), 0.0)
        };
        Vec3::new(ax.to_degrees(), ay.to_degrees(), az.to_degrees())
    }

    /// The inverse of a unit quaternion: the same turn the other way.
    pub fn conjugate(self) -> Quat {
        Quat {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: self.w,
        }
    }

    pub fn rotate(self, v: Vec3) -> Vec3 {
        let u = Vec3::new(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }
}

impl Mul for Quat {
    type Output = Quat;
    fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
}

/// Column-major, as WGSL reads a `mat4x4<f32>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Default for Mat4 {
    fn default() -> Self {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    /// Scale, then rotate, then translate: a `Node`'s local transform.
    pub fn from_trs(t: Vec3, r: Quat, s: Vec3) -> Mat4 {
        let x = r.rotate(Vec3::new(s.x, 0.0, 0.0));
        let y = r.rotate(Vec3::new(0.0, s.y, 0.0));
        let z = r.rotate(Vec3::new(0.0, 0.0, s.z));
        Mat4([
            [x.x, x.y, x.z, 0.0],
            [y.x, y.y, y.z, 0.0],
            [z.x, z.y, z.z, 0.0],
            [t.x, t.y, t.z, 1.0],
        ])
    }

    /// A right-handed perspective onto wgpu's clip space, whose depth runs
    /// from 0 at `near` to 1 at `far`. `fov_y` in radians.
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let r = far / (near - far);
        Mat4([
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, r, -1.0],
            [0.0, 0.0, r * near, 0.0],
        ])
    }

    /// A right-handed orthographic projection with depth 0 at `near` and 1
    /// at `far`, `width` by `height` scene units across.
    pub fn orthographic(width: f32, height: f32, near: f32, far: f32) -> Mat4 {
        let r = 1.0 / (near - far);
        Mat4([
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, 2.0 / height, 0.0, 0.0],
            [0.0, 0.0, r, 0.0],
            [0.0, 0.0, r * near, 1.0],
        ])
    }

    pub fn translation(&self) -> Vec3 {
        Vec3::new(self.0[3][0], self.0[3][1], self.0[3][2])
    }

    /// Where the local -Z axis points: the way a camera looks and a
    /// directional light shines.
    pub fn forward(&self) -> Vec3 {
        -Vec3::new(self.0[2][0], self.0[2][1], self.0[2][2]).normalized()
    }

    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let m = &self.0;
        Vec3::new(
            m[0][0] * v.x + m[1][0] * v.y + m[2][0] * v.z,
            m[0][1] * v.x + m[1][1] * v.y + m[2][1] * v.z,
            m[0][2] * v.x + m[1][2] * v.y + m[2][2] * v.z,
        )
    }

    /// A point through an affine transform: rotated, scaled and moved.
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        self.transform_vector(p) + self.translation()
    }

    /// A point through a projection: its homogeneous coordinates, before
    /// the divide by `w`, which is the caller's to do or refuse.
    pub fn project(&self, p: Vec3) -> [f32; 4] {
        let m = &self.0;
        std::array::from_fn(|r| m[0][r] * p.x + m[1][r] * p.y + m[2][r] * p.z + m[3][r])
    }

    /// `None` for a singular matrix: a node scaled to nothing in some
    /// axis, which draws nothing and lights nothing. Only a determinant of
    /// exactly nothing counts: a projection's is tiny but sound (an
    /// orthographic one 200 pixels wide with a far plane at 10000 has
    /// 2e-8), and so is a node's scaled to a thousandth.
    pub fn inverse(&self) -> Option<Mat4> {
        let m = self.0;
        let a = |c: usize, r: usize| m[c][r];
        let mut inv = [[0.0f32; 4]; 4];
        // Cofactors, written out: a 4×4 inverse is small enough not to
        // need a decomposition.
        let s0 = a(0, 0) * a(1, 1) - a(1, 0) * a(0, 1);
        let s1 = a(0, 0) * a(1, 2) - a(1, 0) * a(0, 2);
        let s2 = a(0, 0) * a(1, 3) - a(1, 0) * a(0, 3);
        let s3 = a(0, 1) * a(1, 2) - a(1, 1) * a(0, 2);
        let s4 = a(0, 1) * a(1, 3) - a(1, 1) * a(0, 3);
        let s5 = a(0, 2) * a(1, 3) - a(1, 2) * a(0, 3);
        let c5 = a(2, 2) * a(3, 3) - a(3, 2) * a(2, 3);
        let c4 = a(2, 1) * a(3, 3) - a(3, 1) * a(2, 3);
        let c3 = a(2, 1) * a(3, 2) - a(3, 1) * a(2, 2);
        let c2 = a(2, 0) * a(3, 3) - a(3, 0) * a(2, 3);
        let c1 = a(2, 0) * a(3, 2) - a(3, 0) * a(2, 2);
        let c0 = a(2, 0) * a(3, 1) - a(3, 0) * a(2, 1);
        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
        if !det.is_normal() {
            return None;
        }
        let d = 1.0 / det;
        inv[0][0] = (a(1, 1) * c5 - a(1, 2) * c4 + a(1, 3) * c3) * d;
        inv[0][1] = (-a(0, 1) * c5 + a(0, 2) * c4 - a(0, 3) * c3) * d;
        inv[0][2] = (a(3, 1) * s5 - a(3, 2) * s4 + a(3, 3) * s3) * d;
        inv[0][3] = (-a(2, 1) * s5 + a(2, 2) * s4 - a(2, 3) * s3) * d;
        inv[1][0] = (-a(1, 0) * c5 + a(1, 2) * c2 - a(1, 3) * c1) * d;
        inv[1][1] = (a(0, 0) * c5 - a(0, 2) * c2 + a(0, 3) * c1) * d;
        inv[1][2] = (-a(3, 0) * s5 + a(3, 2) * s2 - a(3, 3) * s1) * d;
        inv[1][3] = (a(2, 0) * s5 - a(2, 2) * s2 + a(2, 3) * s1) * d;
        inv[2][0] = (a(1, 0) * c4 - a(1, 1) * c2 + a(1, 3) * c0) * d;
        inv[2][1] = (-a(0, 0) * c4 + a(0, 1) * c2 - a(0, 3) * c0) * d;
        inv[2][2] = (a(3, 0) * s4 - a(3, 1) * s2 + a(3, 3) * s0) * d;
        inv[2][3] = (-a(2, 0) * s4 + a(2, 1) * s2 - a(2, 3) * s0) * d;
        inv[3][0] = (-a(1, 0) * c3 + a(1, 1) * c1 - a(1, 2) * c0) * d;
        inv[3][1] = (a(0, 0) * c3 - a(0, 1) * c1 + a(0, 2) * c0) * d;
        inv[3][2] = (-a(3, 0) * s3 + a(3, 1) * s1 - a(3, 2) * s0) * d;
        inv[3][3] = (a(2, 0) * s3 - a(2, 1) * s1 + a(2, 2) * s0) * d;
        Some(Mat4(inv))
    }

    /// The matrix that carries normals: the inverse transpose of the upper
    /// 3×3, so a non-uniform scale does not tilt them. Padded to 4×4 for
    /// WGSL's alignment.
    pub fn normal_matrix(&self) -> Mat4 {
        let Some(inv) = self.inverse() else {
            return Mat4::IDENTITY;
        };
        let i = inv.0;
        Mat4([
            [i[0][0], i[1][0], i[2][0], 0.0],
            [i[0][1], i[1][1], i[2][1], 0.0],
            [i[0][2], i[1][2], i[2][2], 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut r = [[0.0f32; 4]; 4];
        for (c, col) in r.iter_mut().enumerate() {
            for (row, cell) in col.iter_mut().enumerate() {
                *cell = (0..4).map(|k| self.0[k][row] * o.0[c][k]).sum();
            }
        }
        Mat4(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn yaw_turns_forward_towards_minus_x() {
        // A camera turned 90° about Y (eulerRotation.y: 90) looks along -X,
        // as Qt's right-handed space has it.
        let m = Mat4::from_trs(
            Vec3::ZERO,
            Quat::from_euler_degrees(Vec3::new(0.0, 90.0, 0.0)),
            Vec3::ONE,
        );
        assert!(
            close(m.forward(), Vec3::new(-1.0, 0.0, 0.0)),
            "{:?}",
            m.forward()
        );
    }

    #[test]
    fn euler_angles_come_back_from_the_quaternion() {
        for e in [
            Vec3::new(30.0, 45.0, 10.0),
            Vec3::new(-80.0, 170.0, -120.0),
            Vec3::new(0.0, -90.0, 0.0),
        ] {
            let back = Quat::from_euler_degrees(e).to_euler_degrees();
            assert!(
                (back.x - e.x).abs() < 1e-3
                    && (back.y - e.y).abs() < 1e-3
                    && (back.z - e.z).abs() < 1e-3,
                "{e:?} came back as {back:?}"
            );
        }
    }

    #[test]
    fn euler_order_is_z_then_x_then_y() {
        let q = Quat::from_euler_degrees(Vec3::new(90.0, 90.0, 0.0));
        // X first turns +Y into +Z, Y then turns +Z into +X.
        assert!(close(
            q.rotate(Vec3::new(0.0, 1.0, 0.0)),
            Vec3::new(1.0, 0.0, 0.0)
        ));
    }

    #[test]
    fn inverse_undoes_a_transform() {
        let m = Mat4::from_trs(
            Vec3::new(10.0, -4.0, 300.0),
            Quat::from_euler_degrees(Vec3::new(30.0, 45.0, 10.0)),
            Vec3::new(2.0, 1.0, 0.5),
        );
        let id = m * m.inverse().unwrap();
        for c in 0..4 {
            for r in 0..4 {
                let want = if c == r { 1.0 } else { 0.0 };
                assert!((id.0[c][r] - want).abs() < 1e-4, "{id:?}");
            }
        }
    }
}
