//! A lit cube drawn on a device of its own and read back.

mod spike_scene;

use ljos_3d::Target;
use ljos_3d::offscreen::Offscreen;
use spike_scene::{CLEAR, HEIGHT, WIDTH, grey, lit_cube};

/// No adapter (a machine with no GPU driver and no software Vulkan or GL)
/// is said, not failed: the scene is not what is wrong there.
fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

#[test]
fn a_lit_cube_under_a_perspective_camera() {
    let Some(mut gpu) = offscreen() else { return };
    let picture = gpu.render(&lit_cube(grey()), WIDTH, HEIGHT).unwrap();
    let ctx = format!("on {:?} ({:?})", gpu.adapter.name, gpu.adapter.backend);

    for (x, y) in [
        (0, 0),
        (WIDTH - 1, 0),
        (0, HEIGHT - 1),
        (WIDTH - 1, HEIGHT - 1),
    ] {
        assert_eq!(picture.pixel(x, y), CLEAR, "corner ({x}, {y}) {ctx}");
    }
    let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
    // The three faces the camera sees, each lit at its own angle: the top
    // face, tipped towards the camera, the left one facing the light and
    // the right one turned from it, which no ambient light reaches.
    let faces = [
        picture.pixel(cx + 5, cy - 35),
        picture.pixel(cx - 30, cy - 10),
        picture.pixel(cx + 25, cy + 15),
    ];
    for f in faces {
        assert_ne!(f, CLEAR, "the cube covers the centre {ctx}: {faces:?}");
        assert!(
            f[2] >= f[1] && f[1] >= f[0],
            "blue stays blue {ctx}: {faces:?}"
        );
        assert_eq!(f[3], 255, "the cube is opaque {ctx}");
    }
    assert!(
        faces[0] != faces[1] && faces[1] != faces[2] && faces[0] != faces[2],
        "each face is lit differently {ctx}: {faces:?}"
    );
    // Outside the cube's reach the background is untouched.
    assert_eq!(picture.pixel(cx - 100, cy), CLEAR, "{ctx}");
}

#[test]
fn a_transparent_background_leaves_the_rest_clear() {
    let Some(mut gpu) = offscreen() else { return };
    let picture = gpu
        .render(&lit_cube(ljos_3d::Background::Transparent), WIDTH, HEIGHT)
        .unwrap();
    assert_eq!(picture.pixel(0, 0), [0, 0, 0, 0]);
    assert_eq!(picture.pixel(WIDTH / 2, HEIGHT / 2)[3], 255);
}

#[test]
fn an_unchanged_scene_is_not_drawn_again() {
    let Some(gpu) = offscreen() else { return };
    let Offscreen {
        device,
        queue,
        mut renderer,
        ..
    } = gpu;
    let mut scene = lit_cube(grey());
    let mut target = Target::new(&device, &renderer, [WIDTH, HEIGHT]);
    let mut draw = |scene: &ljos_3d::Scene, target: &mut Target| {
        let mut encoder = device.create_command_encoder(&Default::default());
        let drew = renderer.render(&device, &queue, &mut encoder, scene, target);
        queue.submit([encoder.finish()]);
        drew
    };
    assert!(draw(&scene, &mut target), "the first frame draws");
    assert!(
        !draw(&scene, &mut target),
        "the same revision draws nothing"
    );
    scene.set_camera(None);
    assert!(
        !draw(&scene, &mut target),
        "a change to what is already there is none"
    );
    scene.set_environment(ljos_3d::Environment::default());
    assert!(draw(&scene, &mut target), "a real change draws");
}
