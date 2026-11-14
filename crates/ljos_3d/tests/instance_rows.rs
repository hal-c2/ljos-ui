//! What a frame uploads when the node above an instanced model turns: the
//! model's uniforms, not its copies'. A table is in the model's own space,
//! so it is written when it changes, not when what holds it moves; and
//! copies that blend are sorted again, and written again, only where their
//! order changed.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

use std::sync::Arc;

use ljos_3d::math::{Mat4, Quat, Vec3};
use ljos_3d::offscreen::read_back;
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Gpu, Instance, InstanceTable,
    Instancing, Light, Material, MeshSource, Model, Node, NodeKind, Primitive, Scene, Target,
    Transform, stats,
};

const SIZE: [u32; 2] = [160, 120];
/// Opaque copies: 40 by 50, so that their rows are far more than any
/// frame's uniforms.
const COLUMNS: usize = 40;
const ROWS: usize = 50;
const BLENDED: usize = 24;
/// What an opaque copy's row is: three rows of a matrix, a colour, data.
const ROW_BYTES: u64 = 80;

fn table(opaque: bool) -> Arc<InstanceTable> {
    let entries = if opaque {
        (0..COLUMNS * ROWS)
            .map(|i| {
                let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
                Instance {
                    transform: Mat4::from_trs(
                        Vec3::new(-390.0 + 20.0 * c, -245.0 + 10.0 * r, 0.0),
                        Quat::from_euler_degrees(Vec3::new(0.0, 0.0, 7.0 * c)),
                        Vec3::new(0.05, 0.03, 0.05),
                    ),
                    color: [c / COLUMNS as f32, r / ROWS as f32, 0.6, 1.0],
                    ..Instance::default()
                }
            })
            .collect()
    } else {
        // In a ring round the parent's axis: a turn changes which is in
        // front, so the order they blend in.
        (0..BLENDED)
            .map(|i| {
                let a = i as f32 / BLENDED as f32 * std::f32::consts::TAU;
                Instance {
                    transform: Mat4::from_trs(
                        Vec3::new(160.0 * a.cos(), 120.0, 160.0 * a.sin()),
                        Quat::IDENTITY,
                        Vec3::new(0.6, 0.6, 0.6),
                    ),
                    color: [
                        1.0 - i as f32 / BLENDED as f32,
                        0.3,
                        i as f32 / BLENDED as f32,
                        0.5,
                    ],
                    ..Instance::default()
                }
            })
            .collect()
    };
    Arc::new(InstanceTable::new(entries))
}

/// The two instanced models under a node turned `turn` degrees round y,
/// each with a turn and scale of its own that the copies take, seen from
/// `distance` in front.
fn scene(
    opaque: &Arc<InstanceTable>,
    blended: &Arc<InstanceTable>,
    turn: f32,
    distance: f32,
) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 100.0, distance),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -30.0, 0.0)),
            ..Transform::default()
        }),
    );
    let parent = scene.add(
        None,
        Node::new(NodeKind::Group).with_transform(Transform {
            position: Vec3::new(20.0, -10.0, 0.0),
            rotation: Quat::from_euler_degrees(Vec3::new(0.0, turn, 0.0)),
            ..Transform::default()
        }),
    );
    for (table, transparent) in [(opaque, false), (blended, true)] {
        let mut node = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Cube),
            material: Material::Default(DefaultMaterial::default()),
        }))
        .with_transform(Transform {
            position: Vec3::new(0.0, 30.0, 0.0),
            rotation: Quat::from_euler_degrees(Vec3::new(10.0, 0.0, 0.0)),
            scale: Vec3::new(1.2, 1.0, 1.0),
        });
        let mut instancing = Instancing::new(table.clone());
        instancing.transparent = transparent;
        instancing.depth_sort = transparent;
        node.instancing = Some(instancing);
        scene.add(Some(parent), node);
    }
    scene
}

/// Draws `scene` into `target` as a host does, ending the frame after its
/// submit.
fn frame(gpu: &Gpu, target: &mut Target, scene: &Scene) -> stats::FrameCounters {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let drew = gpu
        .renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    assert!(drew, "a scene built anew is drawn");
    gpu.queue().submit([encoder.finish()]);
    stats::frame_end(0.0);
    stats::snapshot().last
}

#[test]
fn a_turning_parent_moves_its_copies_without_writing_their_rows() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let (opaque, blended) = (table(true), table(false));
    let opaque_rows = (COLUMNS * ROWS) as u64 * ROW_BYTES;
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);

    let first = frame(&gpu, &mut target, &scene(&opaque, &blended, 0.0, 900.0));
    assert!(
        first.buffer_write_bytes > opaque_rows,
        "the first frame writes the copies: {first:?}"
    );

    for turn in [25.0, 50.0, 140.0] {
        let turned = frame(&gpu, &mut target, &scene(&opaque, &blended, turn, 900.0));
        // The opaque copies are where they were in the model's space; the
        // blended ones are at most written again in their new order.
        assert!(
            turned.buffer_write_bytes < (BLENDED as u64 + 1) * ROW_BYTES + 8 * 1024,
            "turned {turn}°, only uniforms and the blended copies' order are written: \
             {turned:?}"
        );
        let picture = read_back(gpu.device(), gpu.queue(), target.texture()).unwrap();

        // And the copies are where a view drawing that scene first puts
        // them, blended in the order it does.
        let mut fresh = Target::new(gpu.device(), &gpu.renderer(), SIZE);
        frame(&gpu, &mut fresh, &scene(&opaque, &blended, turn, 900.0));
        assert_eq!(
            read_back(gpu.device(), gpu.queue(), fresh.texture()).unwrap(),
            picture,
            "turned {turn}°: the picture of a view drawing it first"
        );
    }

    // A frame where nothing moved but the camera's distance writes no row:
    // the blended copies are still in the order they were.
    let nudged = frame(&gpu, &mut target, &scene(&opaque, &blended, 140.0, 950.0));
    assert!(
        nudged.buffer_write_bytes < 8 * 1024,
        "no copy's row is written for a camera that leaves their order: {nudged:?}"
    );
}
