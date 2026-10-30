//! Shapes, lights, materials, blending, culling and multisampling, drawn
//! on a device of its own and read back. An orthographic camera at a
//! magnification of 1 makes one scene unit one pixel, with the origin in
//! the middle of the picture.

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::{Offscreen, Rgba, read_back};
use ljos_3d::{
    AlphaMode, Background, Camera, Color, CullMode, DefaultMaterial, Environment, Fade, Light,
    Lighting, Material, MeshSource, Model, Node, NodeId, NodeKind, Primitive, PrincipledMaterial,
    Scene, Target, Transform,
};

const SIDE: u32 = 200;
const CLEAR: Color = Color::rgb(0.0, 0.0, 0.0);

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

/// A black scene seen from z 600 by an orthographic camera.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(CLEAR),
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
    scene
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: Vec3::new(x, y, z),
        ..Transform::default()
    }
}

fn model(scene: &mut Scene, p: Primitive, material: Material, transform: Transform) -> NodeId {
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(p),
            material,
        }))
        .with_transform(transform),
    )
}

fn unlit(color: Color) -> Material {
    Material::Default(DefaultMaterial {
        diffuse_color: color,
        lighting: Lighting::None,
        ..DefaultMaterial::default()
    })
}

/// A white plain surface 400 across facing the camera.
fn wall(scene: &mut Scene) -> NodeId {
    model(
        scene,
        Primitive::Rectangle,
        Material::Default(DefaultMaterial::default()),
        Transform {
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    )
}

fn light(scene: &mut Scene, light: Light, transform: Transform) -> NodeId {
    scene.add(
        None,
        Node::new(NodeKind::Light(light)).with_transform(transform),
    )
}

fn point(color: Color, fade: Fade) -> Light {
    Light::Point {
        color,
        ambient_color: Color::BLACK,
        brightness: 1.0,
        fade,
    }
}

fn draw(gpu: &mut Offscreen, scene: &Scene) -> Rgba {
    gpu.render(scene, SIDE, SIDE).unwrap()
}

/// The sRGB byte a linear value is written as.
fn srgb(linear: f32) -> f32 {
    let c = linear.clamp(0.0, 1.0);
    255.0
        * if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
}

fn near(got: [u8; 4], want: [f32; 3], tolerance: f32) -> bool {
    (0..3).all(|i| (f32::from(got[i]) - want[i]).abs() <= tolerance)
}

#[test]
fn the_built_in_shapes_stand_where_qt_puts_them() {
    let Some(mut gpu) = offscreen() else { return };
    let red = Color::rgb(1.0, 0.0, 0.0);
    let is_red = |p: [u8; 4]| p == [255, 0, 0, 255];
    let is_clear = |p: [u8; 4]| p == [0, 0, 0, 255];

    let mut s = scene();
    model(&mut s, Primitive::Sphere, unlit(red), Transform::default());
    let pic = draw(&mut gpu, &s);
    // Diameter 100 about the middle.
    assert!(is_red(pic.pixel(100, 100)) && is_red(pic.pixel(100, 145)));
    assert!(is_clear(pic.pixel(100, 155)) && is_clear(pic.pixel(140, 140)));

    let mut s = scene();
    model(
        &mut s,
        Primitive::Cylinder,
        unlit(red),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s);
    // From the side, a 100 by 100 square.
    assert!(is_red(pic.pixel(55, 55)) && is_red(pic.pixel(145, 145)));
    assert!(is_clear(pic.pixel(100, 45)) && is_clear(pic.pixel(155, 100)));

    let mut s = scene();
    model(&mut s, Primitive::Cone, unlit(red), Transform::default());
    let pic = draw(&mut gpu, &s);
    // Its base on the middle row, its point 100 above.
    assert!(is_red(pic.pixel(100, 95)) && is_red(pic.pixel(100, 5)) && is_red(pic.pixel(55, 98)));
    assert!(is_clear(pic.pixel(100, 105)) && is_clear(pic.pixel(60, 20)));
}

#[test]
fn a_point_light_fades_as_its_factors_say() {
    let Some(mut gpu) = offscreen() else { return };
    for (fade, z) in [
        (Fade::default(), 100.0),
        (Fade::default(), 300.0),
        (
            Fade {
                constant: 1.0,
                linear: 1.0,
                quadratic: 0.0,
            },
            100.0,
        ),
        (
            Fade {
                constant: 2.0,
                linear: 0.0,
                quadratic: 0.0,
            },
            50.0,
        ),
    ] {
        let mut s = scene();
        wall(&mut s);
        light(&mut s, point(Color::WHITE, fade), at(0.0, 0.0, z));
        let got = draw(&mut gpu, &s).pixel(100, 100);
        let want = srgb(fade.at(z));
        assert!(
            near(got, [want; 3], 2.0),
            "{fade:?} at {z}: {got:?}, want {want}"
        );
    }
}

#[test]
fn a_spot_light_lights_only_its_cone() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = scene();
    wall(&mut s);
    light(
        &mut s,
        Light::Spot {
            color: Color::WHITE,
            ambient_color: Color::BLACK,
            brightness: 1.0,
            fade: Fade {
                constant: 1.0,
                linear: 0.0,
                quadratic: 0.0,
            },
            cone_angle: 40.0,
            inner_cone_angle: 30.0,
        },
        at(0.0, 0.0, 100.0),
    );
    let pic = draw(&mut gpu, &s);
    assert!(near(pic.pixel(100, 100), [255.0; 3], 1.0));
    // 20 off the axis at 100: 11°, inside the inner cone.
    let lambert = (20f32.atan2(100.0)).cos();
    assert!(near(pic.pixel(120, 100), [srgb(lambert); 3], 2.0));
    // Between 15° and 20° it falls off; past 20° nothing.
    let edge = pic.pixel(130, 100)[0];
    assert!(edge > 0 && edge < 240, "{edge}");
    assert_eq!(pic.pixel(140, 100), [0, 0, 0, 255]);
}

#[test]
fn lights_add_up_to_the_cap_and_no_further() {
    let Some(mut gpu) = offscreen() else { return };
    let none = Fade {
        constant: 1.0,
        linear: 0.0,
        quadratic: 0.0,
    };
    let mut s = scene();
    wall(&mut s);
    light(
        &mut s,
        point(Color::rgb(1.0, 0.0, 0.0), none),
        at(0.0, 0.0, 50.0),
    );
    light(
        &mut s,
        point(Color::rgb(0.0, 1.0, 0.0), none),
        at(0.0, 0.0, 50.0),
    );
    assert_eq!(draw(&mut gpu, &s).pixel(100, 100), [255, 255, 0, 255]);

    // Sixteen dark lights fill the cap; a seventeenth bright one is not
    // there.
    let mut s = scene();
    wall(&mut s);
    let dark = Light::Directional {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 0.0,
    };
    for _ in 0..ljos_3d::gpu::MAX_LIGHTS {
        light(&mut s, dark, Transform::default());
    }
    light(&mut s, Light::DIRECTIONAL, Transform::default());
    assert_eq!(draw(&mut gpu, &s).pixel(100, 100), [0, 0, 0, 255]);
}

#[test]
fn every_lights_ambient_colour_lights_what_faces_away() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = scene();
    wall(&mut s);
    let grey = Color::rgb(0.25, 0.25, 0.25);
    for _ in 0..2 {
        light(
            &mut s,
            Light::Directional {
                color: Color::WHITE,
                ambient_color: grey,
                brightness: 1.0,
            },
            // Shining from behind the wall.
            Transform {
                rotation: Quat::from_euler_degrees(Vec3::new(0.0, 180.0, 0.0)),
                ..Transform::default()
            },
        );
    }
    let want = srgb(2.0 * grey.to_linear()[0]);
    assert!(near(draw(&mut gpu, &s).pixel(100, 100), [want; 3], 2.0));
}

fn principled(m: PrincipledMaterial) -> Material {
    Material::Principled(m)
}

/// A white principled surface turned `degrees` about Y under a white sun
/// shining down the camera's axis.
fn tilted(gpu: &mut Offscreen, degrees: f32, metalness: f32, roughness: f32) -> f32 {
    let mut s = scene();
    model(
        &mut s,
        Primitive::Rectangle,
        principled(PrincipledMaterial {
            metalness,
            roughness,
            ..PrincipledMaterial::default()
        }),
        Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(0.0, degrees, 0.0)),
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    );
    light(&mut s, Light::DIRECTIONAL, Transform::default());
    f32::from(draw(gpu, &s).pixel(100, 100)[0])
}

#[test]
fn metal_and_roughness_shape_the_highlight() {
    let Some(mut gpu) = offscreen() else { return };
    // Head on, a smooth metal mirrors the light whole.
    assert_eq!(tilted(&mut gpu, 0.0, 1.0, 0.1), 255.0);
    // Turned away, a smooth metal sends the light past the camera; a rough
    // one still scatters some back, and a rough dielectric more, its
    // diffuse half included.
    let smooth_metal = tilted(&mut gpu, 60.0, 1.0, 0.1);
    let rough_metal = tilted(&mut gpu, 60.0, 1.0, 1.0);
    let rough_dielectric = tilted(&mut gpu, 60.0, 0.0, 1.0);
    assert!(smooth_metal < 4.0, "{smooth_metal}");
    assert!(rough_metal > 40.0, "{rough_metal}");
    assert!(
        rough_dielectric > rough_metal,
        "{rough_dielectric} {rough_metal}"
    );
    // A dielectric is mostly its diffuse half: about cos 60° of the light.
    assert!(
        (rough_dielectric - srgb(0.5)).abs() < 12.0,
        "{rough_dielectric}"
    );
}

#[test]
fn emissive_light_shows_whatever_lights_the_surface() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = scene();
    model(
        &mut s,
        Primitive::Rectangle,
        principled(PrincipledMaterial {
            base_color: Color::BLACK,
            emissive: [0.0, 1.0, 0.0],
            ..PrincipledMaterial::default()
        }),
        Transform::default(),
    );
    assert_eq!(draw(&mut gpu, &s).pixel(100, 100), [0, 255, 0, 255]);
}

fn red_over_blue(gpu: &mut Offscreen, front: PrincipledMaterial) -> [u8; 4] {
    let mut s = scene();
    model(
        &mut s,
        Primitive::Rectangle,
        unlit(Color::rgb(0.0, 0.0, 1.0)),
        Transform {
            position: Vec3::new(0.0, 0.0, -50.0),
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    );
    model(
        &mut s,
        Primitive::Rectangle,
        principled(PrincipledMaterial {
            base_color: Color::rgb(1.0, 0.0, 0.0),
            lighting: Lighting::None,
            ..front
        }),
        Transform::default(),
    );
    draw(gpu, &s).pixel(100, 100)
}

#[test]
fn alpha_modes_blend_ignore_or_cut_the_opacity() {
    let Some(mut gpu) = offscreen() else { return };
    let half = PrincipledMaterial {
        opacity: 0.5,
        ..PrincipledMaterial::default()
    };
    let blended = red_over_blue(&mut gpu, half);
    assert!(blended[0] > 100 && blended[2] > 100, "{blended:?}");
    let opaque = PrincipledMaterial {
        alpha_mode: AlphaMode::Opaque,
        ..half
    };
    assert_eq!(red_over_blue(&mut gpu, opaque), [255, 0, 0, 255]);
    let mask = |opacity, alpha_cutoff| PrincipledMaterial {
        opacity,
        alpha_cutoff,
        alpha_mode: AlphaMode::Mask,
        ..PrincipledMaterial::default()
    };
    assert_eq!(red_over_blue(&mut gpu, mask(0.6, 0.5)), [255, 0, 0, 255]);
    assert_eq!(red_over_blue(&mut gpu, mask(0.4, 0.5)), [0, 0, 255, 255]);
    // Blend blends even what is wholly opaque, which then hides all.
    let blend = PrincipledMaterial {
        alpha_mode: AlphaMode::Blend,
        ..PrincipledMaterial::default()
    };
    assert_eq!(red_over_blue(&mut gpu, blend), [255, 0, 0, 255]);
}

#[test]
fn cull_mode_chooses_which_side_is_drawn() {
    let Some(mut gpu) = offscreen() else { return };
    let turned = |cull_mode| {
        let mut s = scene();
        model(
            &mut s,
            Primitive::Rectangle,
            principled(PrincipledMaterial {
                cull_mode,
                lighting: Lighting::None,
                ..PrincipledMaterial::default()
            }),
            Transform {
                rotation: Quat::from_euler_degrees(Vec3::new(0.0, 180.0, 0.0)),
                ..Transform::default()
            },
        );
        s
    };
    assert_eq!(
        draw(&mut gpu, &turned(CullMode::Back)).pixel(100, 100),
        [0, 0, 0, 255]
    );
    assert_eq!(
        draw(&mut gpu, &turned(CullMode::None)).pixel(100, 100),
        [255; 4]
    );
    assert_eq!(
        draw(&mut gpu, &turned(CullMode::Front)).pixel(100, 100),
        [255; 4]
    );
}

#[test]
fn blended_models_are_drawn_back_to_front_whatever_their_order() {
    let Some(mut gpu) = offscreen() else { return };
    let half = |c| {
        principled(PrincipledMaterial {
            base_color: c,
            opacity: 0.5,
            lighting: Lighting::None,
            ..PrincipledMaterial::default()
        })
    };
    let picture = |red_z: f32| {
        let mut s = scene();
        model(
            &mut s,
            Primitive::Rectangle,
            half(Color::rgb(1.0, 0.0, 0.0)),
            at(0.0, 0.0, red_z),
        );
        model(
            &mut s,
            Primitive::Rectangle,
            half(Color::rgb(0.0, 0.0, 1.0)),
            at(0.0, 0.0, 0.0),
        );
        s
    };
    // Whichever is in front shows more of itself.
    let red_in_front = draw(&mut gpu, &picture(50.0)).pixel(100, 100);
    assert!(red_in_front[0] > red_in_front[2], "{red_in_front:?}");
    let blue_in_front = draw(&mut gpu, &picture(-50.0)).pixel(100, 100);
    assert!(blue_in_front[2] > blue_in_front[0], "{blue_in_front:?}");
}

#[test]
fn frustum_culling_leaves_out_what_the_camera_cannot_see() {
    let Some(gpu) = offscreen() else { return };
    let Offscreen {
        device,
        queue,
        mut renderer,
        ..
    } = gpu;
    let mut s = scene();
    let white = Material::Default(DefaultMaterial::default());
    model(&mut s, Primitive::Cube, white, Transform::default());
    model(&mut s, Primitive::Cube, white, at(2000.0, 0.0, 0.0));
    let mut target = Target::new(&device, &renderer, [SIDE, SIDE]);
    let mut drawn = |s: &Scene, target: &mut Target| {
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(&device, &queue, &mut encoder, s, target);
        queue.submit([encoder.finish()]);
        target.drawn_models()
    };
    assert_eq!(drawn(&s, &mut target), 2, "off by default, as in Qt");
    let camera = s
        .walk()
        .iter()
        .find(|(_, n, _)| matches!(n.kind, NodeKind::Camera(_)))
        .map(|(id, ..)| *id)
        .unwrap();
    if let Some(node) = s.node_mut(camera)
        && let NodeKind::Camera(Camera::Orthographic {
            frustum_culling, ..
        }) = &mut node.kind
    {
        *frustum_culling = true;
    }
    assert_eq!(drawn(&s, &mut target), 1);
}

#[test]
fn multisampling_softens_an_edge_and_resolves_into_the_picture() {
    let Some(gpu) = offscreen() else { return };
    let Offscreen {
        device,
        queue,
        mut renderer,
        ..
    } = gpu;
    let diamond = |samples| {
        let mut s = scene();
        s.set_environment(Environment {
            background: Background::Color(CLEAR),
            msaa_samples: samples,
        });
        model(
            &mut s,
            Primitive::Rectangle,
            unlit(Color::rgb(1.0, 0.0, 0.0)),
            Transform {
                rotation: Quat::from_euler_degrees(Vec3::new(0.0, 0.0, 30.0)),
                ..Transform::default()
            },
        );
        s
    };
    let mut picture = |s: &Scene| {
        let mut target = Target::new(&device, &renderer, [SIDE, SIDE]);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(&device, &queue, &mut encoder, s, &mut target);
        queue.submit([encoder.finish()]);
        (
            read_back(&device, &queue, target.texture()).unwrap(),
            target.samples(),
        )
    };
    let between = |pic: &Rgba| {
        pic.pixels
            .chunks(4)
            .filter(|p| p[0] > 10 && p[0] < 245)
            .count()
    };
    let (hard, one) = picture(&diamond(1));
    assert_eq!(one, 1);
    assert_eq!(between(&hard), 0, "without it every pixel is in or out");
    let (soft, samples) = picture(&diamond(4));
    assert!(samples >= 4, "{samples}");
    assert!(between(&soft) > 100, "{}", between(&soft));
    assert_eq!(soft.pixel(100, 100), [255, 0, 0, 255]);
}

#[test]
fn a_moved_model_moves_in_a_kept_target() {
    let Some(gpu) = offscreen() else { return };
    let Offscreen {
        device,
        queue,
        mut renderer,
        ..
    } = gpu;
    let mut s = scene();
    let red = unlit(Color::rgb(1.0, 0.0, 0.0));
    let ids: Vec<NodeId> = (0..20)
        .map(|i| {
            let mut place = at(-95.0 + i as f32 * 10.0, -80.0, 0.0);
            place.scale = Vec3::new(0.08, 0.08, 0.08);
            model(&mut s, Primitive::Cube, red, place)
        })
        .collect();
    let mut target = Target::new(&device, &renderer, [SIDE, SIDE]);
    let mut picture = |s: &Scene| {
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render(&device, &queue, &mut encoder, s, &mut target);
        queue.submit([encoder.finish()]);
        read_back(&device, &queue, target.texture()).unwrap()
    };
    let before = picture(&s);
    assert_eq!(before.pixel(100, 40), [0, 0, 0, 255]);
    assert_eq!(before.pixel(75, 180), [255, 0, 0, 255]);
    // Only the slot of the moved model is written; the rest stay as they
    // were and still draw.
    if let Some(node) = s.node_mut(ids[7]) {
        node.transform.position = Vec3::new(0.0, 60.0, 0.0);
    }
    let after = picture(&s);
    assert_eq!(after.pixel(100, 40), [255, 0, 0, 255]);
    assert_eq!(after.pixel(75, 180), [0, 0, 0, 255], "it left its place");
    assert_eq!(after.pixel(5, 180), [255, 0, 0, 255], "the first stays");
}
