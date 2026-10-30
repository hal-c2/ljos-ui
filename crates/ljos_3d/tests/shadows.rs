//! Shadow maps, drawn on a device of its own and read back. A white wall
//! 400 across faces an orthographic camera at a magnification of 1, one
//! scene unit to a pixel with the origin in the middle; a cube 100 across
//! stands 100 in front of it, and a light turned 45° about y throws the
//! cube's shadow onto the wall to its left.

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::{Offscreen, Rgba, read_back};
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Fade, Light, Material, MeshSource,
    Model, Node, NodeId, NodeKind, Primitive, Scene, Shadow, Target, Transform,
};

const SIDE: u32 = 200;

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: Vec3::new(x, y, z),
        ..Transform::default()
    }
}

fn turned(t: Transform, y_degrees: f32) -> Transform {
    Transform {
        rotation: Quat::from_euler_degrees(Vec3::new(0.0, y_degrees, 0.0)),
        ..t
    }
}

struct Stage {
    scene: Scene,
    camera: NodeId,
    wall: NodeId,
    cube: NodeId,
    light: NodeId,
}

fn white() -> Material {
    Material::Default(DefaultMaterial::default())
}

fn sun() -> Light {
    Light::Directional {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
    }
}

/// The wall, the cube and `light` at `place`, casting `shadow`.
fn stage(light: Light, place: Transform, shadow: Option<Shadow>) -> Stage {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::BLACK),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: false,
        }))
        .with_transform(at(0.0, 0.0, 600.0)),
    );
    scene.set_camera(Some(camera));
    let model = |scene: &mut Scene, p, transform| {
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(p),
                material: white(),
            }))
            .with_transform(transform),
        )
    };
    let wall = model(
        &mut scene,
        Primitive::Rectangle,
        Transform {
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    );
    let cube = model(&mut scene, Primitive::Cube, at(0.0, 0.0, 100.0));
    let mut node = Node::new(NodeKind::Light(light)).with_transform(place);
    node.shadow = shadow;
    let light = scene.add(None, node);
    Stage {
        scene,
        camera,
        wall,
        cube,
        light,
    }
}

fn sunlit(shadow: Option<Shadow>) -> Stage {
    stage(sun(), turned(Transform::default(), 45.0), shadow)
}

fn draw(gpu: &mut Offscreen, scene: &Scene) -> Rgba {
    gpu.render(scene, SIDE, SIDE).unwrap()
}

fn grey(p: [u8; 4]) -> f32 {
    f32::from(p[0])
}

/// A white surface the light meets at 45°: 0.71 of white, in sRGB.
const LIT: f32 = 219.0;
/// What a shadowFactor of 75 leaves of it.
const SHADOWED: f32 = 117.0;
/// In the cube's shadow, and beside it in the open.
const IN: (u32, u32) = (25, 100);
const OPEN: (u32, u32) = (175, 100);

fn close(got: [u8; 4], want: f32, tolerance: f32) -> bool {
    (grey(got) - want).abs() <= tolerance
}

#[test]
fn a_cube_shadows_the_wall_behind_it() {
    let Some(mut gpu) = offscreen() else { return };
    let s = sunlit(None);
    let pic = draw(&mut gpu, &s.scene);
    assert!(
        close(pic.pixel(IN.0, IN.1), LIT, 3.0),
        "{:?}",
        pic.pixel(25, 100)
    );

    let s = sunlit(Some(Shadow::default()));
    let pic = draw(&mut gpu, &s.scene);
    let (shade, open) = (pic.pixel(IN.0, IN.1), pic.pixel(OPEN.0, OPEN.1));
    assert!(close(shade, SHADOWED, 8.0), "in the shadow: {shade:?}");
    assert!(close(open, LIT, 3.0), "in the open: {open:?}");
    // The shadow's far edge: the cube's left side, 50 from its middle,
    // thrown 150 further left by the light's slant across the 150 between
    // the cube's front and the wall.
    assert!(
        close(pic.pixel(5, 100), SHADOWED, 8.0),
        "{:?}",
        pic.pixel(5, 100)
    );
    assert!(close(pic.pixel(100, 30), LIT, 3.0), "above the shadow");
}

#[test]
fn a_shadow_factor_says_how_much_light_a_shadow_keeps_away() {
    let Some(mut gpu) = offscreen() else { return };
    for (factor, want) in [(0.0, LIT), (1.0, 0.0)] {
        let s = sunlit(Some(Shadow {
            factor,
            ..Shadow::default()
        }));
        let got = draw(&mut gpu, &s.scene).pixel(IN.0, IN.1);
        assert!(close(got, want, 4.0), "factor {factor}: {got:?}");
    }
}

#[test]
fn a_model_that_receives_no_shadows_is_lit_through_one() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = sunlit(Some(Shadow::default()));
    s.scene.node_mut(s.wall).unwrap().receives_shadows = false;
    let got = draw(&mut gpu, &s.scene).pixel(IN.0, IN.1);
    assert!(close(got, LIT, 3.0), "{got:?}");
}

#[test]
fn a_model_that_casts_no_shadows_casts_none() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = sunlit(Some(Shadow::default()));
    s.scene.node_mut(s.cube).unwrap().casts_shadows = false;
    let got = draw(&mut gpu, &s.scene).pixel(IN.0, IN.1);
    assert!(close(got, LIT, 3.0), "{got:?}");
}

/// The wall in the open, a row of pixels away from the cube's shadow: how
/// many are darker than a lit surface by more than rounding.
fn acne(pic: &Rgba) -> usize {
    (110..200)
        .flat_map(|x| [(x, 20), (x, 100), (x, 180)])
        .filter(|&(x, y)| grey(pic.pixel(x, y)) < LIT - 4.0)
        .count()
}

#[test]
fn the_bias_keeps_a_lit_surface_from_shadowing_itself() {
    let Some(mut gpu) = offscreen() else { return };
    // The wall is in its own map: with no bias, its depth there and its
    // depth here differ by rounding alone, and it shadows itself in
    // patches.
    let s = sunlit(Some(Shadow {
        bias: 0.0,
        samples: 1,
        ..Shadow::default()
    }));
    let bare = acne(&draw(&mut gpu, &s.scene));
    let s = sunlit(Some(Shadow {
        samples: 1,
        ..Shadow::default()
    }));
    let biased = acne(&draw(&mut gpu, &s.scene));
    assert!(
        bare > 20,
        "no bias, yet only {bare} pixels shadow themselves"
    );
    assert_eq!(biased, 0, "with the bias");
}

#[test]
fn spot_and_point_lights_cast_shadows() {
    let Some(mut gpu) = offscreen() else { return };
    let spot = Light::Spot {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
        fade: Fade {
            constant: 1.0,
            linear: 0.0,
            quadratic: 0.0,
        },
        cone_angle: 90.0,
        inner_cone_angle: 0.0,
    };
    let point = Light::Point {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
        fade: Fade {
            constant: 1.0,
            linear: 0.0,
            quadratic: 0.0,
        },
    };
    let place = turned(at(300.0, 0.0, 300.0), 45.0);
    for (name, light) in [("spot", spot), ("point", point)] {
        let lit = draw(&mut gpu, &stage(light, place, None).scene);
        let shadowed = draw(
            &mut gpu,
            &stage(
                light,
                place,
                Some(Shadow {
                    factor: 1.0,
                    ..Shadow::default()
                }),
            )
            .scene,
        );
        let (before, after) = (lit.pixel(IN.0, IN.1), shadowed.pixel(IN.0, IN.1));
        assert!(grey(before) > 150.0, "{name} lights the wall: {before:?}");
        assert!(grey(after) < 4.0, "{name} shadows it: {after:?}");
        let open = shadowed.pixel(OPEN.0, OPEN.1);
        assert!(
            close(open, grey(lit.pixel(OPEN.0, OPEN.1)), 3.0),
            "{name} in the open: {open:?}"
        );
    }
}

/// Draws `scene` into `target`, as a host does each frame, and whether it
/// drew.
fn frame(gpu: &mut Offscreen, scene: &Scene, target: &mut Target) -> bool {
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let drew = gpu
        .renderer
        .render(&gpu.device, &gpu.queue, &mut encoder, scene, target);
    gpu.queue.submit([encoder.finish()]);
    drew
}

#[test]
fn shadow_maps_are_drawn_again_only_when_what_they_show_changes() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = sunlit(Some(Shadow::default()));
    let mut target = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    assert!(frame(&mut gpu, &s.scene, &mut target));
    assert_eq!(target.shadow_passes(), 1, "one map for the sun");

    // The same revision draws nothing at all.
    assert!(!frame(&mut gpu, &s.scene, &mut target));
    assert_eq!(target.shadow_passes(), 1);

    // A change the maps do not show draws the picture, not the maps: the
    // camera moving, as the sun's one map covers the scene wherever the
    // camera is.
    s.scene.node_mut(s.camera).unwrap().transform = at(10.0, 0.0, 600.0);
    assert!(frame(&mut gpu, &s.scene, &mut target));
    assert_eq!(target.shadow_passes(), 1, "the camera moved");

    // A caster moving draws them again.
    s.scene.node_mut(s.cube).unwrap().transform = at(0.0, 20.0, 100.0);
    assert!(frame(&mut gpu, &s.scene, &mut target));
    assert_eq!(target.shadow_passes(), 2, "the cube moved");

    // And so does the light turning.
    s.scene.node_mut(s.light).unwrap().transform = turned(Transform::default(), 40.0);
    assert!(frame(&mut gpu, &s.scene, &mut target));
    assert_eq!(target.shadow_passes(), 3, "the light turned");

    let pic = read_back(&gpu.device, &gpu.queue, target.texture()).unwrap();
    assert!(
        grey(pic.pixel(IN.0, IN.1)) < 160.0,
        "{:?}",
        pic.pixel(IN.0, IN.1)
    );
}

#[test]
fn a_light_without_shadows_draws_no_maps() {
    let Some(mut gpu) = offscreen() else { return };
    let s = sunlit(None);
    let mut target = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    frame(&mut gpu, &s.scene, &mut target);
    assert_eq!(target.shadow_passes(), 0);
}

#[test]
fn a_point_light_draws_six_faces() {
    let Some(mut gpu) = offscreen() else { return };
    let point = Light::Point {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
        fade: Fade::default(),
    };
    let s = stage(point, at(300.0, 0.0, 300.0), Some(Shadow::default()));
    let mut target = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    frame(&mut gpu, &s.scene, &mut target);
    assert_eq!(target.shadow_passes(), 6);
}

#[test]
fn cascades_split_a_directional_lights_map() {
    let Some(mut gpu) = offscreen() else { return };
    let mut shadow = Shadow::default();
    shadow.cascades.splits = 2;
    let s = sunlit(Some(shadow));
    let mut target = Target::new(&gpu.device, &gpu.renderer, [SIDE, SIDE]);
    frame(&mut gpu, &s.scene, &mut target);
    assert_eq!(target.shadow_passes(), 3, "a map per cascade");
    let pic = read_back(&gpu.device, &gpu.queue, target.texture()).unwrap();
    let (shade, open) = (pic.pixel(IN.0, IN.1), pic.pixel(OPEN.0, OPEN.1));
    assert!(close(shade, SHADOWED, 10.0), "in the shadow: {shade:?}");
    assert!(close(open, LIT, 3.0), "in the open: {open:?}");
}
