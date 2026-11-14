//! A thousand models that differ only in where they are and their colour
//! are drawn as one: one draw call, the picture of drawing each on its own
//! to the rounding of their normals, and a click meets the model drawn
//! where it clicked. A camera that moves, even one that changes which batch
//! is the nearer, writes none of their rows.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::{Rgba, read_back};
use ljos_3d::pick::{Viewport, pick};
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Gpu, Light, Material, MeshSource,
    Model, Node, NodeId, NodeKind, Primitive, Scene, Target, Transform, stats,
};

const SIZE: [u32; 2] = [320, 200];
const COLUMNS: usize = 40;
const ROWS: usize = 25;

fn color(i: usize) -> Color {
    let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
    Color::rgb(
        c / COLUMNS as f32,
        r / ROWS as f32,
        1.0 - c / COLUMNS as f32,
    )
}

/// The cubes in a grid, each turned its own way, and two spheres in front
/// of them: another mesh, so another batch. Seen from `camera`.
fn scene(camera: Transform) -> (Scene, Vec<NodeId>) {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(camera),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -40.0, 0.0)),
            ..Transform::default()
        }),
    );
    let cubes = (0..COLUMNS * ROWS)
        .map(|i| {
            let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
            scene.add(
                None,
                Node::new(NodeKind::Model(Model {
                    mesh: MeshSource::Primitive(Primitive::Cube),
                    material: Material::Default(DefaultMaterial {
                        diffuse_color: color(i),
                        ..DefaultMaterial::default()
                    }),
                }))
                .with_transform(Transform {
                    position: Vec3::new(-390.0 + 20.0 * c, -240.0 + 20.0 * r, 0.0),
                    rotation: Quat::from_euler_degrees(Vec3::new(9.0 * r, 7.0 * c, 0.0)),
                    scale: Vec3::new(0.12, 0.12, 0.12),
                }),
            )
        })
        .collect();
    for x in [-200.0, 200.0] {
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(Primitive::Sphere),
                material: Material::Default(DefaultMaterial::default()),
            }))
            .with_transform(Transform {
                position: Vec3::new(x, 0.0, 60.0),
                ..Transform::default()
            }),
        );
    }
    (scene, cubes)
}

/// A camera at `x` across, 700 in front of the grid, looking at it.
fn front(x: f32) -> Transform {
    Transform {
        position: Vec3::new(x, 0.0, 700.0),
        ..Transform::default()
    }
}

/// Draws `scene` into `target` as a host does, ending the frame after its
/// submit.
fn frame(gpu: &Gpu, target: &mut Target, scene: &Scene) -> (stats::FrameCounters, Rgba) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let drew = gpu
        .renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    assert!(drew, "a scene built anew is drawn");
    gpu.queue().submit([encoder.finish()]);
    stats::frame_end(0.0);
    let counts = stats::snapshot().last;
    (
        counts,
        read_back(gpu.device(), gpu.queue(), target.texture()).unwrap(),
    )
}

#[test]
fn models_alike_but_for_place_and_colour_are_one_draw() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let models = COLUMNS as u64 * ROWS as u64 + 2;
    let (still, cubes) = scene(front(0.0));
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, batched) = frame(&gpu, &mut target, &still);
    assert_eq!(
        counts.draw_calls, 2,
        "the cubes and the spheres: {counts:?}"
    );
    assert_eq!(counts.models_drawn, models, "{counts:?}");

    // A camera that moves leaves the rows as they were: from in front the
    // spheres are the nearer batch, from behind the cubes are.
    let behind = Transform {
        position: Vec3::new(0.0, 0.0, -700.0),
        rotation: Quat::from_euler_degrees(Vec3::new(0.0, 180.0, 0.0)),
        ..Transform::default()
    };
    for camera in [front(15.0), behind] {
        let (moved, _) = frame(&gpu, &mut target, &scene(camera).0);
        assert_eq!(
            (moved.draw_calls, moved.models_drawn),
            (2, models),
            "{moved:?}"
        );
        assert!(
            moved.buffer_write_bytes < 8 * 1024,
            "only the camera's uniforms are written for {camera:?}: {moved:?}"
        );
    }

    // Drawn each on its own, the picture is the same but where a normal
    // came out a rounding apart.
    gpu.renderer().set_batching(false);
    let mut alone = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, each) = frame(&gpu, &mut alone, &still);
    gpu.renderer().set_batching(true);
    assert_eq!(counts.draw_calls, models, "{counts:?}");
    let far: Vec<_> = batched
        .pixels
        .chunks(4)
        .zip(each.pixels.chunks(4))
        .enumerate()
        .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1))
        .map(|(i, (a, b))| (i % SIZE[0] as usize, i / SIZE[0] as usize, a, b))
        .collect();
    assert!(far.is_empty(), "pixels more than a level apart: {far:?}");

    // A click meets the cube drawn where it clicked.
    let viewport = Viewport::of_scene(&still, SIZE[0] as f32, SIZE[1] as f32).unwrap();
    let background = batched.pixel(1, 1);
    for i in [0, 41, 517, 999] {
        let centre = still.world(cubes[i]).unwrap().translation();
        let at = viewport
            .from_scene(centre)
            .expect("the cube is in front of the camera");
        let ray = viewport.ray(at.x, at.y).unwrap();
        let hit = pick(&still, &ray, |_| true);
        assert_eq!(
            hit.first().map(|h| h.node),
            Some(cubes[i]),
            "cube {i} at {at:?}"
        );
        assert_ne!(
            batched.pixel(at.x as u32, at.y as u32),
            background,
            "cube {i} is drawn where a click meets it, at {at:?}"
        );
    }
}
