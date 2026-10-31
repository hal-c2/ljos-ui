//! Two views drawn on one shared device.
//!
//! A test run has no window, so no surface: this draws the two views'
//! targets the way [`ljos_3d::SurfaceView`] does, off the [`Gpu`]'s one
//! renderer, and leaves presenting to a surface to the hosts that have one.

mod spike_scene;

use ljos_3d::offscreen::{Offscreen, read_back};
use ljos_3d::{Gpu, Target};
use spike_scene::{CLEAR, HEIGHT, WIDTH, grey, lit_cube};

fn gpu() -> Option<Gpu> {
    match pollster::block_on(Gpu::new()) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn draw(gpu: &Gpu, target: &mut Target, scene: &ljos_3d::Scene) -> ljos_3d::offscreen::Rgba {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    gpu.renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    gpu.queue().submit([encoder.finish()]);
    read_back(gpu.device(), gpu.queue(), target.texture()).unwrap()
}

#[test]
fn two_views_on_one_gpu_draw_what_a_device_of_their_own_draws() {
    let Some(gpu) = gpu() else { return };
    let other = gpu.clone();
    let scene = lit_cube(grey());
    let mut first = Target::new(gpu.device(), &gpu.renderer(), [WIDTH, HEIGHT]);
    let mut second = Target::new(other.device(), &other.renderer(), [WIDTH, HEIGHT]);

    let a = draw(&gpu, &mut first, &scene);
    let b = draw(&other, &mut second, &scene);

    assert_eq!(a.pixel(0, 0), CLEAR);
    assert_ne!(a.pixel(WIDTH / 2, HEIGHT / 2), CLEAR, "no cube drawn");
    assert!(a.pixels == b.pixels, "the two views drew differently");
    let alone = Offscreen::new()
        .unwrap()
        .render(&scene, WIDTH, HEIGHT)
        .unwrap();
    assert!(a.pixels == alone.pixels, "a shared device drew differently");
}
