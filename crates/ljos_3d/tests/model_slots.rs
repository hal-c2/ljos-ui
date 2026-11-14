//! What a frame uploads when only the camera moves: the frame's own
//! uniforms, not the models'. Opaque models are drawn nearest first, so a
//! camera going round them changes the order they are drawn in every
//! frame; a model's uniforms must stay where they are all the same. A
//! model that comes in or leaves is written to its own slot, or to none.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::read_back;
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Gpu, Light, Material, MeshSource,
    Model, Node, NodeId, NodeKind, Primitive, Scene, Target, Transform, stats,
};

const SIZE: [u32; 2] = [160, 120];
const MODELS: usize = 12;

/// A row of cubes along z, each its own colour, seen from in front
/// (`behind` false) or from behind: one order of drawing reverses the other.
fn row(behind: bool) -> (Scene, NodeId, Vec<NodeId>) {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(None, Node::new(NodeKind::Camera(Camera::PERSPECTIVE)));
    look(&mut scene, camera, behind);
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -30.0, 0.0)),
            ..Transform::default()
        }),
    );
    let cubes = (0..MODELS).map(|i| scene.add(None, cube(i))).collect();
    (scene, camera, cubes)
}

fn look(scene: &mut Scene, camera: NodeId, behind: bool) {
    let (z, turn) = if behind {
        (-600.0, 180.0)
    } else {
        (600.0, 0.0)
    };
    scene.node_mut(camera).unwrap().transform = Transform {
        position: Vec3::new(0.0, 0.0, z),
        rotation: Quat::from_euler_degrees(Vec3::new(0.0, turn, 0.0)),
        ..Transform::default()
    };
}

/// The `i`th cube of the row; past the row's end they go on along it.
fn cube(i: usize) -> Node {
    let t = (i % MODELS) as f32 / (MODELS - 1) as f32;
    Node::new(NodeKind::Model(Model {
        mesh: MeshSource::Primitive(Primitive::Cube),
        material: Material::Default(DefaultMaterial {
            diffuse_color: Color::rgb(t, 0.4, 1.0 - t),
            ..DefaultMaterial::default()
        }),
    }))
    .with_transform(Transform {
        // Spread on x as well, so every cube shows.
        position: Vec3::new(-220.0 + 40.0 * i as f32, 0.0, -300.0 + 50.0 * i as f32),
        scale: Vec3::new(0.3, 0.3, 0.3),
        ..Transform::default()
    })
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
fn a_moving_camera_uploads_its_frame_and_leaves_still_models_where_they_are() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    // Each cube drawn on its own: drawn as one, their places would be rows
    // of the instance buffer rather than slots (batched_draws.rs).
    gpu.renderer().set_batching(false);
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let picture = |target: &Target| read_back(gpu.device(), gpu.queue(), target.texture()).unwrap();
    // The picture a view drawing `scene` first makes.
    let fresh = |scene: &Scene| {
        let mut fresh = Target::new(gpu.device(), &gpu.renderer(), SIZE);
        frame(&gpu, &mut fresh, scene);
        picture(&fresh)
    };

    let (mut scene, camera, cubes) = row(false);
    let first = frame(&gpu, &mut target, &scene);
    assert_eq!(first.models_drawn, MODELS as u64, "{first:?}");
    let front = picture(&target);

    // Seen from behind, every model is drawn in the other order, and not
    // one of them moved: the frame's uniforms are all that changed.
    look(&mut scene, camera, true);
    let turned = frame(&gpu, &mut target, &scene);
    assert_eq!(turned.models_drawn, MODELS as u64, "{turned:?}");
    assert_eq!(
        turned.buffer_writes, 1,
        "only the frame's uniforms are written when only the camera moved: {turned:?}"
    );
    assert!(
        turned.buffer_write_bytes < first.buffer_write_bytes / 2,
        "the models' uniforms are not written again: {turned:?} after {first:?}"
    );
    // And each model is still drawn with its own uniforms: the pictures
    // are those of a view that drew each scene first.
    assert_eq!(picture(&target), fresh(&scene), "the picture from behind");
    look(&mut scene, camera, false);
    let again = frame(&gpu, &mut target, &scene);
    assert_eq!(again.buffer_writes, 1, "{again:?}");
    assert_eq!(picture(&target), front, "the first picture again");

    // A model that comes in is written to a slot of its own, and only it:
    // the camera stayed, so the frame's uniforms are as they were.
    scene.add(None, cube(MODELS));
    let came = frame(&gpu, &mut target, &scene);
    assert_eq!(came.models_drawn, MODELS as u64 + 1, "{came:?}");
    assert_eq!(came.buffer_writes, 1, "the newcomer's alone: {came:?}");
    assert!(
        came.buffer_write_bytes * 4 < first.buffer_write_bytes,
        "one model's uniforms: {came:?} after {first:?}"
    );
    assert_eq!(picture(&target), fresh(&scene), "with a cube more");

    // Past the buffer's room (16 models), it is made anew with them all:
    // every model's uniforms are written to it, the newcomers' and those
    // of the models that were there, in one run.
    for i in MODELS + 1..MODELS + 6 {
        scene.add(None, cube(i));
    }
    let grew = frame(&gpu, &mut target, &scene);
    assert_eq!(grew.models_drawn, MODELS as u64 + 6, "{grew:?}");
    assert_eq!(grew.buffer_writes, 1, "{grew:?}");
    assert!(
        grew.buffer_write_bytes >= (MODELS as u64 + 6) * came.buffer_write_bytes,
        "all {} models' uniforms: {grew:?}",
        MODELS + 6
    );
    assert_eq!(picture(&target), fresh(&scene), "with the buffer made anew");

    // One that leaves takes nothing else with it: no model is moved into
    // its slot, and with the camera where it was nothing is written.
    scene.remove(cubes[3]);
    let left = frame(&gpu, &mut target, &scene);
    assert_eq!(left.models_drawn, MODELS as u64 + 5, "{left:?}");
    assert_eq!(left.buffer_writes, 0, "{left:?}");
    assert_eq!(picture(&target), fresh(&scene), "with a cube gone");

    // The slot it left is the next newcomer's, written as its own. A cube
    // like the one gone would find its uniforms there already.
    scene.add(None, cube(MODELS + 6));
    let next = frame(&gpu, &mut target, &scene);
    assert_eq!(next.buffer_writes, 1, "{next:?}");
    assert_eq!(
        picture(&target),
        fresh(&scene),
        "with a cube in the slot left"
    );
}
