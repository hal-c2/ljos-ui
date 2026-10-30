//! Picking on the CPU: which models a ray meets, and where, and how a
//! point of a view maps to the scene and back.
//!
//! On the CPU rather than by reading an ID buffer back from the GPU
//! because a pick is asked for in a click handler and wants its answer at
//! once; a read-back would wait a frame. It works on the same [`Scene`]
//! the renderer draws and through the same [`Camera::projection`], so a
//! model is picked exactly where it is drawn. Nothing here needs a GPU,
//! so it builds for the web without the `gpu` feature.

use std::sync::OnceLock;

use crate::instancing::{instance_worlds, skin_point};
use crate::math::{Mat4, Vec3};
use crate::mesh::Mesh;
use crate::scene::{Camera, CullMode, MeshSource, NodeId, NodeKind, Primitive, Scene};

/// A half-line from `origin` along `direction`, which is of unit length so
/// that distances along it are scene units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    /// `None` when `direction` has no length: a ray that points nowhere
    /// meets nothing.
    pub fn new(origin: Vec3, direction: Vec3) -> Option<Ray> {
        let length = direction.length();
        (length > f32::EPSILON).then(|| Ray {
            origin,
            direction: direction * (1.0 / length),
        })
    }

    pub fn at(&self, distance: f32) -> Vec3 {
        self.origin + self.direction * distance
    }
}

/// Where a ray met a model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub node: NodeId,
    /// From the ray's origin, in scene units.
    pub distance: f32,
    pub scene_position: Vec3,
    /// In the model's own space.
    pub position: Vec3,
    /// Unit length, in scene space.
    pub scene_normal: Vec3,
    /// Unit length, in the model's own space.
    pub normal: Vec3,
    /// The mesh's texture coordinates where it was hit.
    pub uv: [f32; 2],
    /// Which copy of an instanced model was hit.
    pub instance: Option<usize>,
}

/// Every visible model `pickable` accepts that `ray` meets, nearest first,
/// each where the ray first meets it. A face the model's material culls is
/// not met, as it is not drawn. An instanced model is met where its
/// nearest copy is; a skinned one where its joints have bent it.
pub fn pick(scene: &Scene, ray: &Ray, pickable: impl Fn(NodeId) -> bool) -> Vec<Hit> {
    let mut hits: Vec<Hit> = scene
        .walk()
        .into_iter()
        .filter_map(|(id, node, world)| {
            let NodeKind::Model(model) = &node.kind else {
                return None;
            };
            if !pickable(id) {
                return None;
            }
            let cull = model.material.cull_mode();
            let mut bounds = model.mesh.bounds();
            // Decoded for each pick: picks follow input, and keeping a
            // decoded copy would double what a large geometry holds.
            let custom;
            let mut mesh = match &model.mesh {
                MeshSource::Primitive(p) => primitive_mesh(*p),
                MeshSource::Custom(g) => {
                    custom = g.to_mesh().ok()?;
                    &custom
                }
            };
            let skinned;
            let mut world = world;
            if let Some(skin) = &node.skin
                && !mesh.joints.is_empty()
            {
                skinned = skin_mesh(mesh, &skin.palette(scene));
                mesh = &skinned;
                world = Mat4::IDENTITY;
                bounds = sphere_of(&mesh.positions);
            }
            match &node.instancing {
                None => hit_mesh(id, mesh, bounds, cull, &world, ray),
                Some(instancing) => {
                    let global = instancing.global(scene, id, &world);
                    instance_worlds(global, world, instancing.drawn())
                        .enumerate()
                        .filter_map(|(i, w)| {
                            let mut hit = hit_mesh(id, mesh, bounds, cull, &w, ray)?;
                            hit.instance = Some(i);
                            Some(hit)
                        })
                        .min_by(|a, b| a.distance.total_cmp(&b.distance))
                }
            }
        })
        .collect();
    hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    hits
}

fn hit_mesh(
    id: NodeId,
    mesh: &Mesh,
    (centre, radius): (Vec3, f32),
    cull: CullMode,
    world: &Mat4,
    ray: &Ray,
) -> Option<Hit> {
    // Most models are nowhere near the ray: one test against the sphere
    // that holds the mesh leaves their triangles alone.
    if !meets_sphere(
        ray,
        world.transform_point(centre),
        radius * largest_scale(world),
    ) {
        return None;
    }
    let inverse = world.inverse()?;
    // In the model's own space, the ray's parameter still counts scene
    // units along the scene ray, because one is the affine image of the
    // other: distances need no converting back.
    let origin = inverse.transform_point(ray.origin);
    let direction = inverse.transform_vector(ray.direction);
    // A mirroring scale turns the winding, and with it which side is front.
    let mirrored = determinant3(world) < 0.0;
    let (triangles, _) = mesh.indices.as_chunks::<3>();
    let mut nearest: Option<(f32, f32, f32, [u32; 3])> = None;
    for &tri in triangles {
        let [a, b, c] = tri.map(|i| vec(mesh.positions[i as usize]));
        let (e1, e2) = (b - a, c - a);
        let facing = e1.cross(e2).dot(direction);
        let front = (facing < 0.0) != mirrored;
        let culled = match cull {
            CullMode::Back => !front,
            CullMode::Front => front,
            CullMode::None => false,
        };
        if culled {
            continue;
        }
        if let Some((d, u, v)) = intersect(origin, direction, a, e1, e2)
            && nearest.is_none_or(|(best, ..)| d < best)
        {
            nearest = Some((d, u, v, tri));
        }
    }
    let (distance, u, v, tri) = nearest?;
    let [i0, i1, i2] = tri.map(|i| i as usize);
    let w = 1.0 - u - v;
    let blend = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| vec(a) * w + vec(b) * u + vec(c) * v;
    let normal = blend(mesh.normals[i0], mesh.normals[i1], mesh.normals[i2]).normalized();
    let uv = [0, 1].map(|k| mesh.uvs[i0][k] * w + mesh.uvs[i1][k] * u + mesh.uvs[i2][k] * v);
    Some(Hit {
        node: id,
        distance,
        scene_position: ray.at(distance),
        position: origin + direction * distance,
        scene_normal: world.normal_matrix().transform_vector(normal).normalized(),
        normal,
        uv,
        instance: None,
    })
}

/// The mesh as its joints have bent it, in the scene's space.
fn skin_mesh(mesh: &Mesh, palette: &[Mat4]) -> Mesh {
    let bend = |p: [f32; 3], j: usize, normal: bool| {
        let (joints, weights) = (mesh.joints[j], mesh.weights[j]);
        let q = skin_point(palette, joints, weights, vec(p));
        if normal {
            let o = skin_point(palette, joints, weights, Vec3::ZERO);
            let n = (q - o).normalized();
            [n.x, n.y, n.z]
        } else {
            [q.x, q.y, q.z]
        }
    };
    Mesh {
        positions: (0..mesh.positions.len())
            .map(|i| bend(mesh.positions[i], i, false))
            .collect(),
        normals: (0..mesh.normals.len())
            .map(|i| bend(mesh.normals[i], i, true))
            .collect(),
        ..mesh.clone()
    }
}

/// A sphere around the points.
fn sphere_of(points: &[[f32; 3]]) -> (Vec3, f32) {
    let Some(first) = points.first() else {
        return (Vec3::ZERO, 0.0);
    };
    let (min, max) = points.iter().fold((vec(*first), vec(*first)), |(lo, hi), p| {
        (
            Vec3::new(lo.x.min(p[0]), lo.y.min(p[1]), lo.z.min(p[2])),
            Vec3::new(hi.x.max(p[0]), hi.y.max(p[1]), hi.z.max(p[2])),
        )
    });
    let centre = (min + max) * 0.5;
    (centre, (max - centre).length())
}

/// Möller–Trumbore: the ray parameter and the barycentric weights of `b`
/// and `c` where the ray meets the triangle, from either side.
fn intersect(
    origin: Vec3,
    direction: Vec3,
    a: Vec3,
    e1: Vec3,
    e2: Vec3,
) -> Option<(f32, f32, f32)> {
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = direction.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some((t, u, v))
}

fn meets_sphere(ray: &Ray, centre: Vec3, radius: f32) -> bool {
    let to_centre = centre - ray.origin;
    let along = to_centre.dot(ray.direction);
    let off_sq = to_centre.dot(to_centre) - along * along;
    off_sq <= radius * radius && along >= -radius
}

fn largest_scale(m: &Mat4) -> f32 {
    (0..3)
        .map(|c| vec([m.0[c][0], m.0[c][1], m.0[c][2]]).length())
        .fold(0.0, f32::max)
}

fn determinant3(m: &Mat4) -> f32 {
    let [x, y, z] = [0, 1, 2].map(|c| vec([m.0[c][0], m.0[c][1], m.0[c][2]]));
    x.cross(y).dot(z)
}

fn vec([x, y, z]: [f32; 3]) -> Vec3 {
    Vec3::new(x, y, z)
}

/// Built once and kept: the built-in meshes never change, and a pick in a
/// pointer handler should not build a sphere each time.
fn primitive_mesh(p: Primitive) -> &'static Mesh {
    static MESHES: [OnceLock<Mesh>; 5] = [const { OnceLock::new() }; 5];
    let slot = match p {
        Primitive::Cube => 0,
        Primitive::Rectangle => 1,
        Primitive::Sphere => 2,
        Primitive::Cylinder => 3,
        Primitive::Cone => 4,
    };
    MESHES[slot].get_or_init(|| Mesh::primitive(p))
}

/// A view `width` by `height` pixels seen through a camera: the mapping
/// between its points and the scene. A point of the view is in pixels
/// from its top left; its depth is the distance past the camera's near
/// clip plane, as QtQuick3D gives it.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    world: Mat4,
    view: Mat4,
    projection: Mat4,
    unprojection: Mat4,
    clip_near: f32,
    width: f32,
    height: f32,
}

impl Viewport {
    /// `None` for a view with no area or a camera scaled to nothing.
    pub fn new(camera: &Camera, world: Mat4, width: f32, height: f32) -> Option<Viewport> {
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let projection = camera.projection(width, height);
        let clip_near = match *camera {
            Camera::Perspective { clip_near, .. } | Camera::Orthographic { clip_near, .. } => {
                clip_near
            }
        };
        Some(Viewport {
            world,
            view: world.inverse()?,
            projection,
            unprojection: projection.inverse()?,
            clip_near,
            width,
            height,
        })
    }

    /// The view `scene` is drawn in, through its active camera.
    pub fn of_scene(scene: &Scene, width: f32, height: f32) -> Option<Viewport> {
        let (camera, world) = scene.active_camera()?;
        Viewport::new(&camera, world, width, height)
    }

    /// The ray through a point of the view. It starts in the camera's
    /// plane: at the camera for a perspective camera, so distances along
    /// it are from the camera, and under the point for an orthographic
    /// one, whose rays are parallel.
    pub fn ray(&self, x: f32, y: f32) -> Option<Ray> {
        let (near, far) = self.line(x, y)?;
        // Where the line through the near and far points crosses the
        // camera's plane, z = 0 in view space.
        let s = -near.z / (far.z - near.z);
        let origin = near + (far - near) * s;
        Ray::new(
            self.world.transform_point(origin),
            self.world.transform_vector(far - near),
        )
    }

    /// A scene position as a point of the view and its depth past the near
    /// plane; the depth is negative behind it. `None` where the camera's
    /// projection cannot place it: in its own plane, for a perspective one.
    pub fn from_scene(&self, p: Vec3) -> Option<Vec3> {
        let v = self.view.transform_point(p);
        let [cx, cy, _, w] = self.projection.project(v);
        if w.abs() < f32::EPSILON {
            return None;
        }
        Some(Vec3::new(
            (cx / w + 1.0) * 0.5 * self.width,
            (1.0 - cy / w) * 0.5 * self.height,
            -v.z - self.clip_near,
        ))
    }

    /// The scene position of a point of the view at a depth past the near
    /// plane: [`Viewport::from_scene`] undone.
    pub fn to_scene(&self, p: Vec3) -> Option<Vec3> {
        let (near, far) = self.line(p.x, p.y)?;
        let depth = -(self.clip_near + p.z);
        let s = (depth - near.z) / (far.z - near.z);
        s.is_finite()
            .then(|| self.world.transform_point(near + (far - near) * s))
    }

    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// The points of the near and far clip planes under a point of the
    /// view, in view space.
    fn line(&self, x: f32, y: f32) -> Option<(Vec3, Vec3)> {
        let ndc_x = x / self.width * 2.0 - 1.0;
        let ndc_y = 1.0 - y / self.height * 2.0;
        let unproject = |depth: f32| {
            let [px, py, pz, w] = self.unprojection.project(Vec3::new(ndc_x, ndc_y, depth));
            (w.abs() > f32::EPSILON).then(|| Vec3::new(px / w, py / w, pz / w))
        };
        let (near, far) = (unproject(0.0)?, unproject(1.0)?);
        ((far.z - near.z).abs() > f32::EPSILON).then_some((near, far))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Quat;
    use crate::scene::{DefaultMaterial, Material, Model, Node, Transform};

    fn model(scene: &mut Scene, p: Primitive, at: Vec3, cull: CullMode) -> NodeId {
        let material = Material::Default(DefaultMaterial {
            cull_mode: cull,
            ..DefaultMaterial::default()
        });
        let node = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(p),
            material,
        }))
        .with_transform(Transform {
            position: at,
            ..Transform::default()
        });
        scene.add(None, node)
    }

    fn camera(scene: &mut Scene, camera: Camera, z: f32) -> NodeId {
        let node = Node::new(NodeKind::Camera(camera)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, z),
            ..Transform::default()
        });
        scene.add(None, node)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn an_application_s_geometry_is_met_where_its_triangles_are() {
        use crate::geometry::{ComponentType, CustomGeometry, PrimitiveType, Semantic};
        // A square of side 100 about the origin, facing +Z, as a fan: met
        // inside it, missed beside it.
        let mut g = CustomGeometry::new();
        g.set_vertex_data(
            [
                -50.0f32, -50.0, 0.0, 50.0, -50.0, 0.0, 50.0, 50.0, 0.0, -50.0, 50.0, 0.0,
            ]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect(),
        );
        g.set_stride(12);
        g.add_attribute(Semantic::Position, 0, ComponentType::F32);
        g.set_primitive_type(PrimitiveType::TriangleFan);
        g.update();
        let mut s = Scene::new();
        let node = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Custom(std::sync::Arc::new(g)),
            material: Material::Default(DefaultMaterial::default()),
        }))
        .with_transform(Transform {
            position: Vec3::new(20.0, 0.0, 0.0),
            ..Transform::default()
        });
        let square = s.add(None, node);
        let towards = Vec3::new(0.0, 0.0, -1.0);
        let inside = Ray::new(Vec3::new(65.0, 10.0, 100.0), towards).unwrap();
        let hits = pick(&s, &inside, |_| true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node, square);
        assert!(close(hits[0].distance, 100.0));
        assert!(close(hits[0].position.x, 45.0));
        let beside = Ray::new(Vec3::new(-35.0, 10.0, 100.0), towards).unwrap();
        assert!(pick(&s, &beside, |_| true).is_empty());
    }

    #[test]
    fn a_ray_down_the_middle_meets_the_cube_s_front_face() {
        let mut s = Scene::new();
        let cube = model(&mut s, Primitive::Cube, Vec3::ZERO, CullMode::Back);
        camera(&mut s, Camera::PERSPECTIVE, 300.0);
        let ray = Viewport::of_scene(&s, 100.0, 100.0)
            .unwrap()
            .ray(50.0, 50.0)
            .unwrap();
        let hits = pick(&s, &ray, |_| true);
        assert_eq!(hits.len(), 1);
        let h = hits[0];
        assert_eq!(h.node, cube);
        assert!(close(h.distance, 250.0), "{h:?}");
        assert!(
            close(h.scene_position.z, 50.0) && close(h.normal.z, 1.0),
            "{h:?}"
        );
        assert!(close(h.uv[0], 0.5) && close(h.uv[1], 0.5), "{h:?}");
    }

    #[test]
    fn hits_come_nearest_first_and_culled_faces_are_passed_over() {
        let mut s = Scene::new();
        let near = model(
            &mut s,
            Primitive::Cube,
            Vec3::new(0.0, 0.0, 200.0),
            CullMode::Back,
        );
        let far = model(&mut s, Primitive::Sphere, Vec3::ZERO, CullMode::Back);
        let back = model(
            &mut s,
            Primitive::Rectangle,
            Vec3::new(0.0, 0.0, -200.0),
            CullMode::Back,
        );
        let down = Ray::new(Vec3::new(0.0, 0.0, 600.0), Vec3::new(0.0, 0.0, -1.0)).unwrap();
        let ids: Vec<_> = pick(&s, &down, |_| true).iter().map(|h| h.node).collect();
        assert_eq!(ids, [near, far, back]);
        let up = Ray::new(Vec3::new(0.0, 0.0, -600.0), Vec3::new(0.0, 0.0, 1.0)).unwrap();
        let ids: Vec<_> = pick(&s, &up, |_| true).iter().map(|h| h.node).collect();
        assert_eq!(ids, [far, near]);
        let only_far: Vec<_> = pick(&s, &down, |id| id == far)
            .iter()
            .map(|h| h.node)
            .collect();
        assert_eq!(only_far, [far]);
    }

    #[test]
    fn a_turned_and_scaled_model_is_met_where_it_is() {
        let mut s = Scene::new();
        let node = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Rectangle),
            material: Material::Default(DefaultMaterial::default()),
        }))
        .with_transform(Transform {
            position: Vec3::new(100.0, 0.0, 0.0),
            rotation: Quat::from_euler_degrees(Vec3::new(0.0, 90.0, 0.0)),
            scale: Vec3::new(2.0, 2.0, 2.0),
        });
        let id = s.add(None, node);
        // The rectangle now faces +X, 200 units across, at x 100.
        let ray = Ray::new(Vec3::new(400.0, 90.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)).unwrap();
        let h = pick(&s, &ray, |_| true)[0];
        assert_eq!(h.node, id);
        assert!(close(h.distance, 300.0), "{h:?}");
        assert!(close(h.scene_normal.x, 1.0), "{h:?}");
        assert!(close(h.position.y, 45.0), "{h:?}");
    }

    #[test]
    fn a_view_maps_to_the_scene_and_back() {
        let cam = Camera::PERSPECTIVE;
        let world = Mat4::from_trs(Vec3::new(0.0, 0.0, 300.0), Quat::IDENTITY, Vec3::ONE);
        let v = Viewport::new(&cam, world, 200.0, 100.0).unwrap();
        let o = v.from_scene(Vec3::ZERO).unwrap();
        assert!(
            close(o.x, 100.0) && close(o.y, 50.0) && close(o.z, 290.0),
            "{o:?}"
        );
        let up = v.from_scene(Vec3::new(0.0, 100.0, 0.0)).unwrap();
        assert!((up.y - 21.132).abs() < 1e-2, "{up:?}");
        let back = v.to_scene(Vec3::new(37.0, 81.0, 123.0)).unwrap();
        let again = v.from_scene(back).unwrap();
        assert!(close(again.x, 37.0) && close(again.y, 81.0) && close(again.z, 123.0));
    }

    #[test]
    fn an_orthographic_ray_starts_under_the_point() {
        let cam = Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: false,
        };
        let world = Mat4::from_trs(Vec3::new(0.0, 0.0, 300.0), Quat::IDENTITY, Vec3::ONE);
        let r = Viewport::new(&cam, world, 200.0, 100.0)
            .unwrap()
            .ray(85.0, 50.0)
            .unwrap();
        assert!(
            close(r.origin.x, -15.0) && close(r.origin.z, 300.0),
            "{r:?}"
        );
        assert!(close(r.direction.z, -1.0), "{r:?}");
    }
}
