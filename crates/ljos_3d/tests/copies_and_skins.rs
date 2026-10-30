//! Instanced models, skinned meshes and the second UV set, drawn on a
//! device of its own and read back. One scene unit is one pixel, the
//! origin in the middle of the picture. With `LJOS_3D_PICTURES` set to a
//! directory, each picture is also saved there.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ljos_3d::math::{Mat4, Quat, Vec3};
use ljos_3d::mesh::Mesh;
use ljos_3d::offscreen::{Offscreen, Rgba};
use ljos_3d::{
    Background, Camera, Color, ComponentType, CustomGeometry, DefaultMaterial, Environment,
    Instance, InstanceTable, Instancing, Light, Lighting, Material, MeshSource, Model, Node,
    NodeKind, PixelEncoding, Primitive, Scene, Semantic, Shadow, Skin, Target, Texture,
    TextureImage, TexturePixels, Transform,
};

const SIDE: u32 = 200;
const CLEAR: [u8; 3] = [32, 32, 32];

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn save(name: &str, picture: &Rgba) {
    let Some(dir) = std::env::var_os("LJOS_3D_PICTURES") else {
        return;
    };
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&picture.pixels).unwrap();
}

fn ortho() -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(32.0 / 255.0, 32.0 / 255.0, 32.0 / 255.0)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: true,
        }))
        .with_transform(at(0.0, 0.0, 600.0)),
    );
    scene.set_camera(Some(camera));
    scene
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: Vec3::new(x, y, z),
        ..Transform::default()
    }
}

fn unlit(color: Color) -> Material {
    Material::Default(DefaultMaterial {
        lighting: Lighting::None,
        diffuse_color: color,
        ..DefaultMaterial::default()
    })
}

fn near(got: [u8; 4], want: [u8; 3], tolerance: u8) -> bool {
    (0..3).all(|i| got[i].abs_diff(want[i]) <= tolerance)
}

#[track_caller]
fn assert_pixel(p: &Rgba, x: u32, y: u32, want: [u8; 3]) {
    let got = p.pixel(x, y);
    assert!(near(got, want, 2), "at {x},{y}: {got:?}, not {want:?}");
}

fn entry(x: f32, y: f32, color: [f32; 4]) -> Instance {
    Instance {
        transform: Mat4::from_trs(Vec3::new(x, y, 0.0), Quat::IDENTITY, Vec3::ONE),
        color,
        ..Instance::default()
    }
}

fn small_square(scene: &mut Scene, instancing: Instancing) -> ljos_3d::NodeId {
    small_square_in(scene, None, instancing)
}

fn small_square_in(
    scene: &mut Scene,
    parent: Option<ljos_3d::NodeId>,
    instancing: Instancing,
) -> ljos_3d::NodeId {
    let mut node = Node::new(NodeKind::Model(Model {
        mesh: MeshSource::Primitive(Primitive::Rectangle),
        material: unlit(Color::WHITE),
    }))
    .with_transform(Transform {
        scale: Vec3::new(0.2, 0.2, 1.0),
        ..Transform::default()
    });
    node.instancing = Some(instancing);
    scene.add(parent, node)
}

#[test]
fn each_copy_is_drawn_where_and_in_the_colour_its_entry_says() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = ortho();
    let table = InstanceTable::new(vec![
        entry(-50.0, 0.0, [1.0, 0.0, 0.0, 1.0]),
        entry(50.0, 0.0, [0.0, 0.0, 1.0, 1.0]),
    ]);
    small_square(&mut s, Instancing::new(Arc::new(table)));
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    save("instancing-two-copies.png", &p);
    assert_pixel(&p, 50, 100, [255, 0, 0]);
    assert_pixel(&p, 150, 100, [0, 0, 255]);
    assert_pixel(&p, 100, 100, CLEAR);
}

#[test]
fn ten_thousand_copies_are_one_draw() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = ortho();
    let entries = (0..10_000)
        .map(|i| {
            let (x, y) = ((i % 100) as f32 * 2.0 - 99.0, (i / 100) as f32 * 2.0 - 99.0);
            Instance {
                transform: Mat4::from_trs(
                    Vec3::new(x, y, 0.0),
                    Quat::IDENTITY,
                    Vec3::new(0.05, 0.05, 1.0),
                ),
                color: [0.0, 1.0, 0.0, 1.0],
                ..Instance::default()
            }
        })
        .collect();
    let mut instancing = Instancing::new(Arc::new(InstanceTable::new(entries)));
    instancing.count = Some(5_000);
    small_square(&mut s, instancing);
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    save("instancing-count-override.png", &p);
    let mut target = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    gpu.renderer
        .render(&gpu.device, &gpu.queue, &mut encoder, &s, &mut target);
    assert_eq!(target.drawn_models(), 1);
    // The first 5000 entries fill the lower half of the grid.
    assert_pixel(&p, 100, 150, [0, 255, 0]);
    assert_pixel(&p, 100, 50, CLEAR);
}

/// The model is 40 of a half-scaled root's units right of it. By default
/// a copy is moved by its entry within the model's parent, so by half of
/// it; with the root as instance root, by all of it, and the 20 scene
/// units between root and model are kept beside each copy.
#[test]
fn copies_are_placed_in_the_instance_root_s_space() {
    let Some(mut gpu) = offscreen() else { return };
    let draw = |gpu: &mut Offscreen, by_root: bool| {
        let mut s = ortho();
        let root = s.add(
            None,
            Node::new(NodeKind::Group).with_transform(Transform {
                position: Vec3::new(0.0, 50.0, 0.0),
                scale: Vec3::new(0.5, 0.5, 0.5),
                ..Transform::default()
            }),
        );
        let table = InstanceTable::new(vec![entry(-50.0, 0.0, [1.0, 0.0, 0.0, 1.0])]);
        let mut instancing = Instancing::new(Arc::new(table));
        if by_root {
            instancing.root = Some(root);
        }
        let model = small_square_in(&mut s, Some(root), instancing);
        s.node_mut(model).unwrap().transform = Transform {
            position: Vec3::new(40.0, 0.0, 0.0),
            scale: Vec3::new(0.4, 0.4, 1.0),
            ..Transform::default()
        };
        gpu.render(&s, SIDE, SIDE).unwrap()
    };
    let p = draw(&mut gpu, false);
    assert_pixel(&p, 95, 50, [255, 0, 0]);
    assert_pixel(&p, 70, 50, CLEAR);
    let p = draw(&mut gpu, true);
    save("instancing-root.png", &p);
    assert_pixel(&p, 70, 50, [255, 0, 0]);
    assert_pixel(&p, 95, 50, CLEAR);
}

#[test]
fn a_copy_s_alpha_blends_only_where_the_copies_have_transparency() {
    let Some(mut gpu) = offscreen() else { return };
    let half = [1.0, 0.0, 0.0, 0.5];
    let mut s = ortho();
    let table = Arc::new(InstanceTable::new(vec![entry(0.0, 0.0, half)]));
    let model = small_square(&mut s, Instancing::new(table.clone()));
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    assert_pixel(&p, 100, 100, [255, 0, 0]);

    let mut instancing = Instancing::new(table);
    instancing.transparent = true;
    s.node_mut(model).unwrap().instancing = Some(instancing);
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    let got = p.pixel(100, 100);
    assert!(got[0] > 100 && got[0] < 230 && got[2] < 60, "{got:?}");
}

/// 20 units wide and 100 tall, from the origin up: the rows at 0 and 50
/// follow joint 0, the top row joint 1.
fn strip() -> CustomGeometry {
    let mut bytes = Vec::new();
    for (y, joint) in [(0.0f32, 0u32), (50.0, 0), (100.0, 1)] {
        for x in [-10.0f32, 10.0] {
            for f in [x, y, 0.0, 0.0, 0.0, 1.0] {
                bytes.extend(f.to_le_bytes());
            }
            for j in [joint, 0, 0, 0] {
                bytes.extend(j.to_le_bytes());
            }
            for w in [1.0f32, 0.0, 0.0, 0.0] {
                bytes.extend(w.to_le_bytes());
            }
        }
    }
    let indices: [u32; 12] = [0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4];
    let mut g = CustomGeometry::new();
    g.set_vertex_data(bytes);
    g.set_stride(56);
    g.add_attribute(Semantic::Position, 0, ComponentType::F32);
    g.add_attribute(Semantic::Normal, 12, ComponentType::F32);
    g.add_attribute(Semantic::Joint, 24, ComponentType::U32);
    g.add_attribute(Semantic::Weight, 40, ComponentType::F32);
    g.set_index_data(indices.iter().flat_map(|i| i.to_le_bytes()).collect());
    g.add_attribute(Semantic::Index, 0, ComponentType::U32);
    g.update();
    g
}

fn skinned(turn: f32, shadows: bool) -> Scene {
    let mut s = ortho();
    let hip = s.add(None, Node::new(NodeKind::Group));
    let knee = s.add(
        Some(hip),
        Node::new(NodeKind::Group).with_transform(Transform {
            position: Vec3::new(0.0, 50.0, 0.0),
            rotation: Quat::from_euler_degrees(Vec3::new(0.0, 0.0, turn)),
            ..Transform::default()
        }),
    );
    let mut node = Node::new(NodeKind::Model(Model {
        mesh: MeshSource::Custom(Arc::new(strip())),
        material: unlit(Color::rgb(1.0, 0.0, 0.0)),
    }))
    // Ignored: the joints place a skinned mesh.
    .with_transform(at(60.0, 0.0, 0.0));
    node.skin = Some(Skin {
        joints: vec![hip, knee],
        inverse_bind: vec![
            Mat4::IDENTITY,
            Mat4::from_trs(Vec3::new(0.0, -50.0, 0.0), Quat::IDENTITY, Vec3::ONE),
        ],
    });
    node.casts_shadows = shadows;
    s.add(None, node);
    if shadows {
        let mut sun = Node::new(NodeKind::Light(Light::Directional {
            color: Color::WHITE,
            ambient_color: Color::BLACK,
            brightness: 1.0,
        }));
        sun.shadow = Some(Shadow::default());
        s.add(None, sun);
    }
    s
}

#[test]
fn a_skinned_mesh_follows_its_joints() {
    let Some(mut gpu) = offscreen() else { return };
    let p = gpu.render(&skinned(0.0, false), SIDE, SIDE).unwrap();
    save("skin-bind-pose.png", &p);
    assert_pixel(&p, 100, 25, [255, 0, 0]);
    assert_pixel(&p, 100, 75, [255, 0, 0]);
    assert_pixel(&p, 160, 75, CLEAR);

    let p = gpu.render(&skinned(90.0, false), SIDE, SIDE).unwrap();
    save("skin-knee-turned.png", &p);
    assert_pixel(&p, 100, 75, [255, 0, 0]);
    assert_pixel(&p, 100, 15, CLEAR);
    assert_pixel(&p, 60, 50, [255, 0, 0]);
}

#[test]
fn a_skinned_mesh_casts_its_shadow_bent() {
    let Some(mut gpu) = offscreen() else { return };
    // The shadow pass bends the mesh with the same joints; unlit, the
    // strip looks as it does without one.
    let p = gpu.render(&skinned(90.0, true), SIDE, SIDE).unwrap();
    assert_pixel(&p, 60, 50, [255, 0, 0]);
    assert_pixel(&p, 100, 15, CLEAR);
}

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct Pixels {
    id: u64,
    data: Vec<u8>,
}

impl TexturePixels for Pixels {
    fn id(&self) -> u64 {
        self.id
    }
    fn size(&self) -> [u32; 2] {
        [64, 64]
    }
    fn encoding(&self) -> PixelEncoding {
        PixelEncoding::Srgb8Straight
    }
    fn opaque(&self) -> bool {
        true
    }
    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.data);
    }
}

/// Red top left, green top right, blue bottom left, white bottom right.
fn quadrants() -> TextureImage {
    let mut data = Vec::new();
    for y in 0..64 {
        for x in 0..64 {
            data.extend(match (x < 32, y < 32) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255; 4],
            });
        }
    }
    TextureImage::new(Pixels {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        data,
    })
}

/// The built-in rectangle, its first UV set all in the red quadrant, its
/// second the rectangle's own.
fn two_uv_square() -> CustomGeometry {
    let mesh = Mesh::primitive(Primitive::Rectangle);
    let mut vertices = Vec::new();
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        let floats = p.iter().chain(&[0.25, 0.25]).chain(uv);
        vertices.extend(floats.flat_map(|f| f.to_le_bytes()));
    }
    let mut g = CustomGeometry::new();
    g.set_vertex_data(vertices);
    g.set_stride(28);
    g.add_attribute(Semantic::Position, 0, ComponentType::F32);
    g.add_attribute(Semantic::TexCoord0, 12, ComponentType::F32);
    g.add_attribute(Semantic::TexCoord1, 20, ComponentType::F32);
    g.set_index_data(mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect());
    g.add_attribute(Semantic::Index, 0, ComponentType::U32);
    g.update();
    g
}

#[test]
fn a_texture_on_the_second_uv_set_reads_it() {
    let Some(mut gpu) = offscreen() else { return };
    let geometry = Arc::new(two_uv_square());
    let draw = |gpu: &mut Offscreen, uv_set| {
        let mut texture = Texture::new(quadrants());
        texture.uv_set = uv_set;
        let mut s = ortho();
        s.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Custom(geometry.clone()),
                material: Material::Default(DefaultMaterial {
                    lighting: Lighting::None,
                    diffuse_map: Some(texture),
                    ..DefaultMaterial::default()
                }),
            })),
        );
        gpu.render(&s, SIDE, SIDE).unwrap()
    };
    let p = draw(&mut gpu, 0);
    assert_pixel(&p, 125, 125, [255, 0, 0]);
    let p = draw(&mut gpu, 1);
    save("uv-set-1.png", &p);
    assert_pixel(&p, 75, 75, [255, 0, 0]);
    assert_pixel(&p, 125, 75, [0, 255, 0]);
    assert_pixel(&p, 75, 125, [0, 0, 255]);
    assert_pixel(&p, 125, 125, [255, 255, 255]);
}
