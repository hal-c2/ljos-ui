//! What a frame of a scene is counted to cost (`ljos_3d::stats`), and
//! that a scene a view already shows is not drawn again.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

// Shared with the tests that compare pixels; this one counts instead.
#[allow(dead_code)]
mod spike_scene;

use ljos_3d::offscreen::read_back;
use ljos_3d::{Gpu, Target, stats};
use spike_scene::{HEIGHT, WIDTH, grey, lit_cube};

/// Draws `scene` into `target` as a host does, ending the frame after its
/// submit; returns whether the renderer drew.
fn frame(gpu: &Gpu, target: &mut Target, scene: &ljos_3d::Scene) -> bool {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let drew = gpu
        .renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    gpu.queue().submit([encoder.finish()]);
    stats::frame_end(0.0);
    drew
}

#[test]
fn a_frame_counts_what_it_drew_and_a_still_scene_is_drawn_once() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let mut target = Target::new(gpu.device(), &gpu.renderer(), [WIDTH, HEIGHT]);
    let scene = lit_cube(grey());

    assert!(frame(&gpu, &mut target, &scene));
    let first = stats::snapshot();
    assert_eq!(first.frames, 1);
    let f = first.last;
    assert_eq!(f.views, 1);
    assert_eq!(f.models_drawn, 1);
    assert!(f.draw_calls >= 1, "{f:?}");
    assert!(
        f.vertices >= 36,
        "a cube has 36 corners of triangles: {f:?}"
    );
    assert!(f.render_passes >= 1 && f.pipeline_switches >= 1, "{f:?}");
    assert!(f.bind_group_sets >= 1 && f.buffer_writes >= 1, "{f:?}");
    assert!(
        f.pipelines_created >= 1 && f.shader_modules_created >= 1,
        "{f:?}"
    );
    assert!(f.prepare_seconds >= 0.0 && f.encode_seconds >= 0.0, "{f:?}");

    // The same scene again: nothing is drawn, and no frame is counted.
    assert!(!frame(&gpu, &mut target, &scene));
    let still = stats::snapshot();
    assert_eq!(still.frames, 1);
    assert_eq!(still.last, f);

    // The same scene built anew draws again, with what the first frame made.
    let again = lit_cube(grey());
    assert!(frame(&gpu, &mut target, &again));
    let second = stats::snapshot();
    assert_eq!(second.frames, 2);
    let s = second.last;
    assert_eq!((s.draw_calls, s.vertices), (f.draw_calls, f.vertices));
    assert_eq!(s.pipelines_created, 0, "{s:?}");
    assert_eq!(s.shader_modules_created, 0, "{s:?}");
    assert_eq!(s.texture_uploads, 0, "{s:?}");
    assert_eq!(second.total.draw_calls, f.draw_calls * 2);

    // A read back is the test's, not the frame's.
    let _ = read_back(gpu.device(), gpu.queue(), target.texture()).unwrap();
    assert_eq!(stats::snapshot().last, s);
}
