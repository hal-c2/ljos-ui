//! Which frames' GPU time is read. A host sums the readings of the targets
//! it drew into one frame's GPU time, so the targets drawn in a frame are
//! timed together, whatever each was drawn in the frames before.
//!
//! One test, in a process of its own: the frames are the process's, and a
//! second test ending frames beside it would add to them. Its parts follow
//! on from the frames the one before ended.

use ljos_3d::math::Vec3;
use ljos_3d::offscreen::Offscreen;
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Material, MeshSource, Model, Node,
    NodeKind, Primitive, Scene, TIMED_EVERY, Target, Transform, stats,
};

const SIDE: u32 = 64;

mod device;
use device::offscreen;

fn scene(x: f32) -> Scene {
    let mut s = Scene::new();
    s.set_environment(Environment {
        background: Background::Color(Color::BLACK),
        ..Environment::default()
    });
    let camera = s.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 1000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: false,
        }))
        .with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 600.0),
            ..Transform::default()
        }),
    );
    s.set_camera(Some(camera));
    s.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Cube),
            material: Material::Default(DefaultMaterial::default()),
        }))
        .with_transform(Transform {
            position: Vec3::new(x, 0.0, 0.0),
            ..Transform::default()
        }),
    );
    s
}

fn draw(gpu: &mut Offscreen, scene: &Scene, target: &mut Target) {
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(
        gpu.renderer
            .render(&gpu.device, &gpu.queue, &mut encoder, scene, target)
    );
    gpu.queue.submit([encoder.finish()]);
}

/// Two views, one drawn every frame and one that skips five frames early
/// on, as a view whose scene stood still would. Counted by its own
/// drawings, the second would be timed five frames after the first from
/// then on, and a frame's summed GPU time would hold one view or the other.
#[test]
fn the_targets_drawn_in_a_frame_are_timed_together() {
    let Some(mut gpu) = offscreen() else { return };
    if !gpu
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        eprintln!(
            "SKIPPED: {} has no timestamps read without waiting",
            gpu.adapter.name
        );
        return;
    }
    let every = TIMED_EVERY as usize;
    let mut each = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    let mut some = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    let (mut read_each, mut read_some) = (Vec::new(), Vec::new());
    for frame in 0..2 * every + 3 {
        let s = scene(frame as f32);
        draw(&mut gpu, &s, &mut each);
        if !(3..8).contains(&frame) {
            draw(&mut gpu, &s, &mut some);
        }
        stats::frame_end(0.0);
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        if each.take_gpu_time().is_some() {
            read_each.push(frame);
        }
        if some.take_gpu_time().is_some() {
            read_some.push(frame);
        }
    }
    // A timed frame's readings are mapped when its targets are next drawn
    // and read the time after.
    let want = [2, every + 2, 2 * every + 2];
    assert_eq!(read_each, want, "frames 0, {every} and {} timed", 2 * every);
    assert_eq!(read_some, want, "timed in the same frames as the other");

    // Two windows of one process, each drawing its view and ending its
    // frame in turn, as the shell repaints them. Counted by the frames the
    // process ended, a period the turns divide would time one window's
    // frames and never the other's. Each is timed one in so many of its own
    // frames.
    let mut windows = [
        Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]),
        Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]),
    ];
    let mut read: [Vec<usize>; 2] = Default::default();
    for frame in 0..6 * every + 6 {
        let (w, own) = (frame % 2, frame / 2);
        draw(&mut gpu, &scene(frame as f32), &mut windows[w]);
        stats::frame_end(0.0);
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        if windows[w].take_gpu_time().is_some() {
            read[w].push(own);
        }
    }
    for (w, read) in read.iter().enumerate() {
        assert!(read.len() >= 3, "window {w} read at its frames {read:?}");
        assert!(
            read.windows(2).all(|r| r[1] - r[0] == every),
            "window {w} read at its frames {read:?}, not one in {every}"
        );
    }
}
