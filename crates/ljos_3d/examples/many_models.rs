//! A hundred lit cubes, one of which moves every frame: what a frame of a
//! busy scene costs, on the CPU to record it and in all until the device
//! is done. `cargo run --release -p ljos_3d --example many_models [frames]`.

use std::time::{Duration, Instant};

use ljos_3d::math::Vec3;
use ljos_3d::offscreen::Offscreen;
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Light, Material, MeshSource, Model,
    Node, NodeKind, Primitive, Scene, Target, Transform,
};

const SIDE: u32 = 512;

fn main() {
    let frames: u32 = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(500);
    let gpu = Offscreen::new().expect("an adapter");
    println!("on {} ({:?})", gpu.adapter.name, gpu.adapter.backend);
    let Offscreen {
        device,
        queue,
        mut renderer,
        ..
    } = gpu;

    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 1500.0),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
    let mut cubes = Vec::new();
    for i in 0..100 {
        let (x, y) = ((i % 10) as f32 - 4.5, (i / 10) as f32 - 4.5);
        cubes.push(
            scene.add(
                None,
                Node::new(NodeKind::Model(Model {
                    mesh: MeshSource::Primitive(Primitive::Cube),
                    material: Material::Default(DefaultMaterial {
                        diffuse_color: Color::rgb(0.2 + 0.008 * i as f32, 0.5, 0.8),
                        ..DefaultMaterial::default()
                    }),
                }))
                .with_transform(Transform {
                    position: Vec3::new(x * 120.0, y * 120.0, 0.0),
                    scale: Vec3::new(0.6, 0.6, 0.6),
                    ..Transform::default()
                }),
            ),
        );
    }

    let mut target = Target::new(&device, &renderer, [SIDE, SIDE]);
    let mut record = Duration::ZERO;
    let mut total = Duration::ZERO;
    for frame in 0..frames + 20 {
        let moving = cubes[frame as usize % cubes.len()];
        if let Some(node) = scene.node_mut(moving) {
            node.transform.position.z = (frame % 7) as f32 * 10.0;
        }
        let start = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(&device, &queue, &mut encoder, &scene, &mut target);
        let recorded = start.elapsed();
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device answers");
        // The first frames make pipelines and meshes; they are not what a
        // running scene costs.
        if frame >= 20 {
            record += recorded;
            total += start.elapsed();
        }
    }
    println!(
        "{frames} frames of 100 models at {SIDE}x{SIDE}: {:.1} µs to record, {:.1} µs to the device's end",
        record.as_secs_f64() * 1e6 / f64::from(frames),
        total.as_secs_f64() * 1e6 / f64::from(frames),
    );
}
