//! Shadow maps: where each shadowing light's maps look from, and the
//! depth passes that draw the casters into them.
//!
//! Every map of a target is a layer of one depth texture array, which the
//! lit shader reads through one comparison sampler: WebGL2 has room for
//! no fifth bind group and few more textures, so a light's maps cannot
//! each be a binding of their own. A directional light has one layer, or
//! one per cascade; a spot light one; a point light six, the faces of a
//! cube. Only lights whose `castsShadow` is true have layers, and the
//! layers are drawn again only when what they would show changes: a
//! caster moving, a light moving, or, for cascades, the camera.

use bytemuck::{Pod, Zeroable};

use crate::math::{Mat4, Quat, Vec3};
use crate::scene::{Camera, Light, Shadow};

/// The most lights that cast shadows; the shadows of those after it in
/// the tree are left out. Four point lights take 24 layers, which keeps
/// the frame's uniforms within WebGL2's smallest uniform block and the
/// maps' memory bounded.
pub const MAX_SHADOW_LIGHTS: usize = 4;
/// Six cube faces for each of the most lights that cast shadows.
pub const MAX_LAYERS: usize = MAX_SHADOW_LIGHTS * 6;
/// Float depth: a perspective map 5000 units deep keeps hundredths of a
/// unit apart at its far end, and WebGL2 samples it for comparison.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The most bytes a target's maps take; larger requests are drawn at a
/// halved side until they fit.
const BUDGET: u64 = 64 << 20;

/// One map: what a caster's position becomes in it, and for a cascade the
/// stretch of the camera's view depth it covers.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq, Debug)]
pub struct Layer {
    pub view_proj: [[f32; 4]; 4],
    /// x the view depth a cascade ends at, y the one it starts at.
    pub params: [f32; 4],
}

/// A light that casts shadows, as the renderer found it.
pub struct Caster<'a> {
    pub light: &'a Light,
    pub world: &'a Mat4,
    pub shadow: &'a Shadow,
}

/// What the camera sees, for cascades.
pub struct View<'a> {
    pub camera: &'a Camera,
    pub view_proj: &'a Mat4,
}

/// The layers of every shadowing light, in order, and each light's share.
#[derive(Default, Debug, PartialEq)]
pub struct Plan {
    pub layers: Vec<Layer>,
    /// Per light, its first layer and how many it has.
    pub lights: Vec<(usize, usize)>,
    /// The side every layer is drawn at.
    pub size: u32,
}

impl Plan {
    /// `bounds` are the spheres of every model that casts or receives
    /// shadows; a directional light's map is fitted to them.
    pub fn new(
        lights: &[Caster<'_>],
        bounds: &[(Vec3, f32)],
        view: Option<View<'_>>,
        max_side: u32,
    ) -> Plan {
        let mut plan = Plan::default();
        let mut size = 1;
        for c in lights.iter().take(MAX_SHADOW_LIGHTS) {
            let first = plan.layers.len();
            let pos = c.world.translation();
            let s = c.shadow;
            size = size.max(s.map_size);
            match c.light {
                Light::Directional { .. } => {
                    let dir = c.world.forward();
                    match &view {
                        Some(v) if s.cascades.splits > 0 => {
                            cascades(&mut plan.layers, dir, s, bounds, v, s.map_size);
                        }
                        _ => plan.layers.push(fitted(dir, bounds)),
                    }
                }
                Light::Spot { cone_angle, .. } => {
                    let fov = (cone_angle.clamp(1.0, 170.0) + 4.0).min(175.0);
                    let proj = Mat4::perspective(fov.to_radians(), 1.0, near(s.far), s.far);
                    plan.layers
                        .push(layer(proj * look_along(pos, c.world.forward())));
                }
                Light::Point { .. } => {
                    let proj =
                        Mat4::perspective(90f32.to_radians(), 1.0, near(s.far), s.far.max(1.0));
                    for (dir, up) in CUBE_FACES {
                        plan.layers.push(layer(proj * look_along_up(pos, dir, up)));
                    }
                }
            }
            plan.lights.push((first, plan.layers.len() - first));
        }
        plan.size = fit_size(size, plan.layers.len() as u64, max_side);
        plan
    }
}

/// The side the maps are drawn at: the largest asked for, halved while
/// all the layers would take more than the budget.
fn fit_size(wanted: u32, layers: u64, max_side: u32) -> u32 {
    let mut size = wanted.clamp(1, max_side.max(1));
    while size > 256 && u64::from(size) * u64::from(size) * 4 * layers > BUDGET {
        size /= 2;
    }
    size
}

/// +X, -X, +Y, -Y, +Z, -Z, as the lit shader picks them by a direction's
/// largest axis.
const CUBE_FACES: [(Vec3, Vec3); 6] = [
    (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
    (Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
    (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    (Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
    (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
    (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0)),
];

/// A near plane close enough not to clip a caster beside the light and
/// far enough that float depth keeps the far end apart.
fn near(far: f32) -> f32 {
    (far * 1e-3).clamp(0.01, 1.0)
}

fn layer(m: Mat4) -> Layer {
    Layer {
        view_proj: m.0,
        params: [f32::MAX, 0.0, 0.0, 0.0],
    }
}

fn look_along(eye: Vec3, dir: Vec3) -> Mat4 {
    let up = if dir.y.abs() > 0.99 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    look_along_up(eye, dir, up)
}

/// The view of an eye at `eye` looking along `dir`, its -Z.
fn look_along_up(eye: Vec3, dir: Vec3, up: Vec3) -> Mat4 {
    let f = dir.normalized();
    let r = f.cross(up).normalized();
    let u = r.cross(f);
    Mat4([
        [r.x, u.x, -f.x, 0.0],
        [r.y, u.y, -f.y, 0.0],
        [r.z, u.z, -f.z, 0.0],
        [-r.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
    ])
}

/// The box, across the light, holding every sphere: `(min, max)` in the
/// light's view, whose z grows towards the light.
fn light_box(view: &Mat4, points: impl Iterator<Item = (Vec3, f32)>) -> Option<(Vec3, Vec3)> {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;
    for (c, r) in points {
        let p = view.transform_point(c);
        lo = Vec3::new(lo.x.min(p.x - r), lo.y.min(p.y - r), lo.z.min(p.z - r));
        hi = Vec3::new(hi.x.max(p.x + r), hi.y.max(p.y + r), hi.z.max(p.z + r));
        any = true;
    }
    any.then_some((lo, hi))
}

/// An orthographic map over `lo..hi` of the light's view `turn`.
fn ortho(turn: Mat4, lo: Vec3, hi: Vec3) -> Mat4 {
    let w = (hi.x - lo.x).max(1e-3);
    let h = (hi.y - lo.y).max(1e-3);
    let centre = Vec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, hi.z + 1.0);
    let shift = Mat4::from_trs(-centre, Quat::IDENTITY, Vec3::ONE);
    Mat4::orthographic(w, h, 0.0, hi.z - lo.z + 2.0) * shift * turn
}

/// One map over everything that casts or receives shadows. It does not
/// follow the camera, so a camera moving alone draws no map again.
fn fitted(dir: Vec3, bounds: &[(Vec3, f32)]) -> Layer {
    let turn = look_along(Vec3::ZERO, dir);
    let Some((lo, hi)) = light_box(&turn, bounds.iter().copied()) else {
        return layer(Mat4::orthographic(1.0, 1.0, 0.0, 1.0) * turn);
    };
    layer(ortho(turn, lo, hi))
}

fn clip_range(camera: &Camera) -> (f32, f32) {
    match *camera {
        Camera::Perspective {
            clip_near,
            clip_far,
            ..
        }
        | Camera::Orthographic {
            clip_near,
            clip_far,
            ..
        } => (clip_near, clip_far),
    }
}

/// A map per stretch of the camera's view depth, each fitted to what the
/// camera sees in it and reaching back to every caster in front of it.
fn cascades(
    out: &mut Vec<Layer>,
    dir: Vec3,
    s: &Shadow,
    bounds: &[(Vec3, f32)],
    view: &View<'_>,
    size: u32,
) {
    let turn = look_along(Vec3::ZERO, dir);
    let (n, f) = clip_range(view.camera);
    let end = f.min(s.far).max(n + 1e-3);
    let inv = view.view_proj.inverse().unwrap_or(Mat4::IDENTITY);
    let corner = |x: f32, y: f32, z: f32| {
        let p = inv.project(Vec3::new(x, y, z));
        Vec3::new(p[0], p[1], p[2]) * (1.0 / p[3])
    };
    let edges: Vec<(Vec3, Vec3)> = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .map(|(x, y)| (corner(x, y, 0.0), corner(x, y, 1.0)))
        .collect();
    // A point on an edge at view depth `d`: depth runs evenly along it.
    let at = |d: f32| {
        let t = (d - n) / (f - n).max(1e-6);
        edges.iter().map(move |&(a, b)| (a + (b - a) * t, 0.0))
    };
    let splits = s.cascades.splits.min(3) as usize;
    let mut cuts = vec![n];
    cuts.extend(
        s.cascades.at[..splits]
            .iter()
            .map(|t| n + (end - n) * t.clamp(0.0, 1.0)),
    );
    cuts.push(end);
    let scene = light_box(&turn, bounds.iter().copied());
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let corners: Vec<(Vec3, f32)> = at(a).chain(at(b)).collect();
        let Some((mut lo, mut hi)) = light_box(&turn, corners.iter().copied()) else {
            continue;
        };
        if s.cascades.lock_texels {
            // A square as wide as the slice's bounding sphere, whatever
            // the camera's turn, moved by whole texels.
            let centre = corners.iter().fold(Vec3::ZERO, |sum, (p, _)| sum + *p)
                * (1.0 / corners.len() as f32);
            let r = corners
                .iter()
                .map(|(p, _)| (*p - centre).length())
                .fold(0.0, f32::max)
                .max(1e-3);
            let texel = 2.0 * r / size.max(1) as f32;
            let c = turn.transform_point(centre);
            let snap = |v: f32| (v / texel).floor() * texel;
            let (cx, cy) = (snap(c.x), snap(c.y));
            lo = Vec3::new(cx - r, cy - r, lo.z);
            hi = Vec3::new(cx + r, cy + r, hi.z);
        }
        // Casters outside the slice still throw shadows into it.
        if let Some((slo, shi)) = scene {
            lo.z = lo.z.min(slo.z);
            hi.z = hi.z.max(shi.z);
        }
        let mut l = layer(ortho(turn, lo, hi));
        l.params = [b, a, 0.0, 0.0];
        out.push(l);
    }
}

/// The bind group layout of the frame: its uniforms, the shadow maps and
/// the sampler that compares with them.
pub fn frame_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ljos_3d frame"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

/// A target's maps: `layers` layers `size` texels square.
pub struct Maps {
    pub size: u32,
    pub layers: u32,
    pub array: wgpu::TextureView,
    pub views: Vec<wgpu::TextureView>,
}

/// How many layers an array for `wanted` maps has. GL takes an array of
/// one layer for a plain texture, so there are always two; and a square
/// array of a multiple of six layers is taken for cubes (by GL, and by
/// wgpu, which marks it cube-compatible on Vulkan), whose layers then read
/// back as nothing through an array's view, so such a count gets one more.
pub fn layer_count(wanted: usize) -> u32 {
    let n = (wanted as u32).max(2);
    if n.is_multiple_of(6) { n + 1 } else { n }
}

/// An array of depth layers; `layers` from [`layer_count`].
pub fn maps(device: &wgpu::Device, size: u32, layers: u32) -> Maps {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ljos_3d shadow maps"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("ljos_3d shadow maps"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let views = (0..layers)
        .map(|i| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("ljos_3d shadow map"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    Maps {
        size,
        layers,
        array,
        views,
    }
}

/// Linear filtering compares the four nearest texels and blends what
/// they say, which softens even a hard shadow's edge by a texel.
pub fn sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("ljos_3d shadow"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun(turn_y: f32) -> Mat4 {
        Mat4::from_trs(
            Vec3::ZERO,
            Quat::from_euler_degrees(Vec3::new(0.0, turn_y, 0.0)),
            Vec3::ONE,
        )
    }

    #[test]
    fn a_directional_map_holds_every_caster_and_receiver() {
        let world = sun(45.0);
        let light = Light::Directional {
            color: crate::Color::WHITE,
            ambient_color: crate::Color::TRANSPARENT,
            brightness: 1.0,
        };
        let shadow = Shadow::default();
        let bounds = [(Vec3::ZERO, 283.0), (Vec3::new(0.0, 0.0, 100.0), 87.0)];
        let plan = Plan::new(
            &[Caster {
                light: &light,
                world: &world,
                shadow: &shadow,
            }],
            &bounds,
            None,
            8192,
        );
        assert_eq!(plan.lights, vec![(0, 1)]);
        let m = Mat4(plan.layers[0].view_proj);
        for (c, r) in bounds {
            for d in [Vec3::new(r, 0.0, 0.0), Vec3::new(0.0, -r, 0.0)] {
                let p = m.project(c + d);
                assert!(p[0].abs() <= 1.0001 && p[1].abs() <= 1.0001, "{p:?}");
                assert!((0.0..=1.0).contains(&p[2]), "{p:?}");
            }
        }
    }

    #[test]
    fn a_point_light_has_six_faces_and_a_spot_light_one() {
        let world = Mat4::IDENTITY;
        let point = Light::Point {
            color: crate::Color::WHITE,
            ambient_color: crate::Color::TRANSPARENT,
            brightness: 1.0,
            fade: Default::default(),
        };
        let shadow = Shadow::default();
        let c = |light| Caster {
            light,
            world: &world,
            shadow: &shadow,
        };
        let plan = Plan::new(&[c(&point), c(&point)], &[], None, 8192);
        assert_eq!(plan.lights, vec![(0, 6), (6, 6)]);
        // What lies along +X is in the first face.
        let p = Mat4(plan.layers[0].view_proj).project(Vec3::new(100.0, 10.0, 0.0));
        assert!(p[3] > 0.0 && (p[0] / p[3]).abs() < 1.0 && (p[1] / p[3]).abs() < 1.0);
    }

    #[test]
    fn more_lights_than_the_cap_leave_the_last_unshadowed() {
        let world = Mat4::IDENTITY;
        let light = Light::Directional {
            color: crate::Color::WHITE,
            ambient_color: crate::Color::TRANSPARENT,
            brightness: 1.0,
        };
        let shadow = Shadow::default();
        let all: Vec<_> = (0..MAX_SHADOW_LIGHTS + 2)
            .map(|_| Caster {
                light: &light,
                world: &world,
                shadow: &shadow,
            })
            .collect();
        let plan = Plan::new(&all, &[(Vec3::ZERO, 1.0)], None, 8192);
        assert_eq!(plan.lights.len(), MAX_SHADOW_LIGHTS);
    }

    #[test]
    fn maps_too_large_for_the_budget_are_drawn_smaller() {
        assert_eq!(fit_size(1024, 1, 8192), 1024);
        assert_eq!(fit_size(4096, 1, 8192), 4096);
        assert_eq!(fit_size(4096, 24, 8192), 512);
        assert_eq!(fit_size(4096, 1, 2048), 2048, "the device's largest");
        assert_eq!(fit_size(256, 24, 8192), 256, "never below low");
    }

    #[test]
    fn an_array_of_maps_is_never_taken_for_cubes() {
        assert_eq!(layer_count(0), 2);
        assert_eq!(layer_count(1), 2);
        assert_eq!(layer_count(3), 3);
        assert_eq!(layer_count(6), 7);
        assert_eq!(layer_count(12), 13);
        assert_eq!(layer_count(24), 25);
    }
}
