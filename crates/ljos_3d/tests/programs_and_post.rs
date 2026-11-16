//! Custom materials, effects, ExtendedSceneEnvironment's built-in effects
//! and ambient occlusion, drawn on a device of its own and read back. An
//! orthographic camera at a magnification of 1 makes one scene unit one
//! pixel, with the origin in the middle of the picture.
//!
//! With `LJOS_3D_PICTURES` naming a directory, each picture is also
//! written there as a PNG.

use std::sync::Arc;

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::{Offscreen, Rgba};
use ljos_3d::{
    AmbientOcclusion, Background, Blend, Camera, Color, ComponentType, CustomGeometry,
    CustomMaterial, DefaultMaterial, DepthOfField, Effect, EffectBuffer, Environment, Glow,
    GlowBlend, Light, Lighting, Material, MeshSource, Model, MorphTexels, Morphing, Morphs, Node,
    NodeKind, Pass, PostProcess, Primitive, PrimitiveType, Program, Scene, Semantic, Shadow,
    Transform, UniformKind, UniformValue, Vignette, snippet,
};

const SIDE: u32 = 200;

mod device;
use device::offscreen;

fn scene(environment: Environment) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(environment);
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 1.0,
            clip_far: 1000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: false,
        }))
        .with_transform(at(0.0, 0.0, 600.0)),
    );
    scene.set_camera(Some(camera));
    scene
}

fn background(c: Color) -> Environment {
    Environment {
        background: Background::Color(c),
        ..Environment::default()
    }
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: Vec3::new(x, y, z),
        ..Transform::default()
    }
}

fn add(scene: &mut Scene, p: Primitive, material: Material, transform: Transform) {
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(p),
            material,
        }))
        .with_transform(transform),
    );
}

fn unlit(color: Color) -> Material {
    Material::Default(DefaultMaterial {
        diffuse_color: color,
        lighting: Lighting::None,
        ..DefaultMaterial::default()
    })
}

fn draw(gpu: &mut Offscreen, scene: &Scene, name: &str) -> Rgba {
    let pic = gpu.render(scene, SIDE, SIDE).unwrap();
    if let Some(dir) = std::env::var_os("LJOS_3D_PICTURES") {
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), SIDE, SIDE);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pic.pixels).unwrap();
    }
    pic
}

fn near(got: [u8; 4], want: [u8; 3], tolerance: u8) -> bool {
    (0..3).all(|i| got[i].abs_diff(want[i]) <= tolerance)
}

#[track_caller]
fn expect(pic: &Rgba, x: u32, y: u32, want: [u8; 3], tolerance: u8) {
    let got = pic.pixel(x, y);
    assert!(
        near(got, want, tolerance),
        "pixel {x},{y} is {got:?}, not within {tolerance} of {want:?}"
    );
}

fn material(
    vertex: Option<&str>,
    fragment: &str,
    shaded: bool,
    uniforms: &[(&str, UniformKind)],
) -> CustomMaterial {
    let uniforms: Vec<(String, UniformKind)> =
        uniforms.iter().map(|(n, k)| (n.to_string(), *k)).collect();
    let program = snippet::material(vertex, Some(fragment), shaded, &uniforms, &[])
        .unwrap_or_else(|e| panic!("{e}"));
    CustomMaterial::new(Arc::new(program))
}

fn effect_pass(
    fragment: &str,
    uniforms: &[(&str, UniformKind)],
    values: Vec<UniformValue>,
) -> Pass {
    let uniforms: Vec<(String, UniformKind)> =
        uniforms.iter().map(|(n, k)| (n.to_string(), *k)).collect();
    let program: Program =
        snippet::effect(None, fragment, &uniforms, &[]).unwrap_or_else(|e| panic!("{e}"));
    Pass {
        leaves: None,
        program: Arc::new(program),
        values,
        textures: Vec::new(),
        input: None,
        output: None,
    }
}

const TINT: &str = "void MAIN() { FRAGCOLOR = vec4(tint.rgb * level, 1.0); }";
const SHIFT: &str = "void MAIN() { VERTEX.x += offset; POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0); }";
const DARK: Color = Color::rgb(
    0x20 as f32 / 255.0,
    0x20 as f32 / 255.0,
    0x20 as f32 / 255.0,
);

#[test]
fn an_unshaded_material_writes_its_colour_and_moves_its_vertices() {
    let Some(mut gpu) = offscreen() else { return };
    let uniforms = [
        ("tint", UniformKind::Vec4),
        ("level", UniformKind::Float),
        ("offset", UniformKind::Float),
    ];
    let mut m = material(Some(SHIFT), TINT, false, &uniforms);
    m.values = vec![
        UniformValue::color(Color::rgb(1.0, 0.0, 0.0)),
        UniformValue::Float(1.0),
        UniformValue::Float(0.0),
    ];
    let mut s = scene(background(DARK));
    add(
        &mut s,
        Primitive::Rectangle,
        Material::Custom(m.clone()),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "custom-unshaded");
    expect(&pic, 100, 100, [255, 0, 0], 2);
    expect(&pic, 20, 20, [0x20; 3], 2);

    m.values[2] = UniformValue::Float(60.0);
    m.values[1] = UniformValue::Float(0.5);
    let mut s = scene(background(DARK));
    add(
        &mut s,
        Primitive::Rectangle,
        Material::Custom(m.clone()),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "custom-shifted");
    expect(&pic, 80, 100, [0x20; 3], 2);
    expect(&pic, 180, 100, [0x80, 0, 0], 2);

    // Added over the background.
    m.source_blend = Blend::One;
    m.destination_blend = Blend::One;
    m.values[2] = UniformValue::Float(0.0);
    let mut s = scene(background(DARK));
    add(
        &mut s,
        Primitive::Rectangle,
        Material::Custom(m),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "custom-additive");
    expect(&pic, 100, 100, [0xa0, 0x20, 0x20], 3);
}

#[test]
fn a_shaded_material_is_lit_by_its_light_functions_and_tonemapped() {
    let Some(mut gpu) = offscreen() else { return };
    let glow = material(
        None,
        "void MAIN() { BASE_COLOR = vec4(0.0, 0.0, 0.0, 1.0); EMISSIVE_COLOR = vec3(0.5); }",
        true,
        &[],
    );
    let mut s = scene(background(DARK));
    add(
        &mut s,
        Primitive::Rectangle,
        Material::Custom(glow),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "custom-emissive");
    expect(&pic, 100, 100, [0xbc; 3], 3);

    let flat = material(
        None,
        "void MAIN() {}\nvoid DIRECTIONAL_LIGHT() { DIFFUSE += LIGHT_COLOR * 0.25; }\nvoid SPECULAR_LIGHT() {}",
        true,
        &[],
    );
    let mut s = scene(background(DARK));
    s.add(
        None,
        Node::new(NodeKind::Light(Light::Directional {
            color: Color::WHITE,
            ambient_color: Color::BLACK,
            brightness: 1.0,
        })),
    );
    add(
        &mut s,
        Primitive::Rectangle,
        Material::Custom(flat),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "custom-light-function");
    expect(&pic, 100, 100, [0x89; 3], 3);
}

const INVERT: &str =
    "void MAIN() { vec4 c = texture(INPUT, INPUT_UV); FRAGCOLOR = vec4(vec3(1.0) - c.rgb, c.a); }";
const DIM: &str = "void MAIN() { FRAGCOLOR = texture(INPUT, INPUT_UV) * vec4(vec3(factor), 1.0); }";
const DEPTH: &str = "void MAIN() { float d = texture(DEPTH_TEXTURE, INPUT_UV).r; FRAGCOLOR = d < 1.0 ? vec4(1.0) : vec4(0.0, 0.0, 0.0, 1.0); }";

/// A red square in front of a blue backdrop, with `effects`.
fn stage(effects: Vec<Effect>) -> Scene {
    let mut s = scene(Environment {
        effects,
        ..background(DARK)
    });
    add(
        &mut s,
        Primitive::Rectangle,
        unlit(Color::rgb(0.0, 0.0, 1.0)),
        Transform {
            position: Vec3::new(0.0, 0.0, -50.0),
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    );
    add(
        &mut s,
        Primitive::Rectangle,
        unlit(Color::rgb(1.0, 0.0, 0.0)),
        Transform::default(),
    );
    s
}

#[test]
fn effects_run_in_order_through_their_buffers() {
    let Some(mut gpu) = offscreen() else { return };
    let factor = [("factor", UniformKind::Float)];
    let invert = Effect {
        buffers: Vec::new(),
        passes: vec![effect_pass(INVERT, &[], Vec::new())],
    };
    let dim = Effect {
        buffers: vec![EffectBuffer {
            format: ljos_3d::BufferFormat::Rgba16F,
            ..EffectBuffer::default()
        }],
        passes: vec![
            Pass {
                output: Some(0),
                ..effect_pass(DIM, &factor, vec![UniformValue::Float(0.5)])
            },
            Pass {
                input: Some(0),
                ..effect_pass(DIM, &factor, vec![UniformValue::Float(1.0)])
            },
        ],
    };
    let pic = draw(&mut gpu, &stage(vec![invert.clone()]), "effect-invert");
    expect(&pic, 100, 100, [0, 255, 255], 2);
    expect(&pic, 20, 20, [255, 255, 0], 2);

    let pic = draw(
        &mut gpu,
        &stage(vec![invert, dim]),
        "effect-invert-then-dim",
    );
    expect(&pic, 100, 100, [0, 0xbc, 0xbc], 3);

    let depth = Effect {
        buffers: Vec::new(),
        passes: vec![effect_pass(DEPTH, &[], Vec::new())],
    };
    let mut s = scene(Environment {
        effects: vec![depth],
        ..background(DARK)
    });
    add(
        &mut s,
        Primitive::Rectangle,
        unlit(Color::rgb(1.0, 0.0, 0.0)),
        Transform::default(),
    );
    let pic = draw(&mut gpu, &s, "effect-depth");
    expect(&pic, 100, 100, [255; 3], 2);
    expect(&pic, 20, 20, [0; 3], 2);
}

/// A square of `color` on black, with `post`.
fn square(color: Color, post: PostProcess, exposure: f32) -> Scene {
    let mut s = scene(Environment {
        post,
        exposure,
        ..background(Color::BLACK)
    });
    add(
        &mut s,
        Primitive::Rectangle,
        unlit(color),
        Transform::default(),
    );
    s
}

fn on() -> PostProcess {
    PostProcess {
        on: true,
        ..PostProcess::default()
    }
}

#[test]
fn the_extended_environment_tonemaps_and_grades_the_picture() {
    let Some(mut gpu) = offscreen() else { return };
    let grey = Color::rgb(
        0xbc as f32 / 255.0,
        0xbc as f32 / 255.0,
        0xbc as f32 / 255.0,
    );
    let pic = draw(&mut gpu, &square(grey, on(), 1.0), "ese-plain");
    expect(&pic, 100, 100, [0xbc; 3], 3);
    expect(&pic, 20, 20, [0; 3], 2);
    let pic = draw(&mut gpu, &square(grey, on(), 0.5), "ese-exposure-half");
    expect(&pic, 100, 100, [0x89; 3], 3);

    let pic = draw(
        &mut gpu,
        &square(
            Color::WHITE,
            PostProcess {
                vignette: Some(Vignette {
                    color: [1.0, 0.0, 0.0],
                    radius: 0.35,
                    strength: 15.0,
                }),
                ..on()
            },
            1.0,
        ),
        "ese-vignette",
    );
    // The little of the red that reaches the middle shows through Qt's fit.
    expect(&pic, 100, 100, [255; 3], 6);
    expect(&pic, 1, 1, [20, 0, 0], 40);

    let glow = PostProcess {
        glow: Some(Glow {
            hdr_minimum: 0.0,
            bloom: 1.0,
            blend: GlowBlend::Additive,
            levels: 0b111,
            ..Glow::default()
        }),
        ..on()
    };
    let pic = draw(&mut gpu, &square(Color::WHITE, glow, 1.0), "ese-glow");
    assert_ne!(
        pic.pixel(46, 100),
        [0, 0, 0, 255],
        "glow reaches past the edge"
    );

    let dof = |focus: f32, range: f32, blur: f32| PostProcess {
        depth_of_field: Some(DepthOfField {
            focus_distance: focus,
            focus_range: range,
            blur_amount: blur,
        }),
        ..on()
    };
    let pic = draw(
        &mut gpu,
        &square(Color::WHITE, dof(600.0, 100.0, 4.0), 1.0),
        "ese-dof-in-focus",
    );
    expect(&pic, 53, 100, [255; 3], 2);
    let pic = draw(
        &mut gpu,
        &square(Color::WHITE, dof(100.0, 10.0, 10.0), 1.0),
        "ese-dof-out-of-focus",
    );
    assert_ne!(pic.pixel(47, 100), [0, 0, 0, 255]);
    assert_ne!(pic.pixel(53, 100), [255, 255, 255, 255]);

    for (name, post) in [
        ("ese-fxaa", PostProcess { fxaa: true, ..on() }),
        (
            "ese-dithering",
            PostProcess {
                dithering: true,
                ..on()
            },
        ),
    ] {
        let pic = draw(&mut gpu, &square(grey, post, 1.0), name);
        expect(&pic, 100, 100, [0xbc; 3], 4);
    }
}

#[test]
fn ambient_occlusion_darkens_a_wall_beside_a_box() {
    let Some(mut gpu) = offscreen() else { return };
    let lit = |ao: Option<AmbientOcclusion>| {
        let mut s = scene(Environment {
            ambient_occlusion: ao,
            ..background(Color::BLACK)
        });
        s.add(
            None,
            Node::new(NodeKind::Light(Light::Directional {
                color: Color::WHITE,
                ambient_color: Color::BLACK,
                brightness: 1.0,
            })),
        );
        let white = || Material::Default(DefaultMaterial::default());
        add(
            &mut s,
            Primitive::Rectangle,
            white(),
            Transform {
                position: Vec3::new(0.0, 0.0, -60.0),
                scale: Vec3::new(4.0, 4.0, 1.0),
                ..Transform::default()
            },
        );
        add(&mut s, Primitive::Cube, white(), Transform::default());
        s
    };
    let pic = draw(&mut gpu, &lit(None), "ao-off");
    expect(&pic, 45, 100, [255; 3], 2);
    let ao = AmbientOcclusion {
        strength: 100.0,
        distance: 50.0,
        softness: 50.0,
        sample_rate: 2,
        bias: 0.0,
        dither: false,
    };
    let pic = draw(&mut gpu, &lit(Some(ao)), "ao-on");
    assert_ne!(
        pic.pixel(45, 100),
        [255, 255, 255, 255],
        "the wall by the box is darker"
    );
    expect(&pic, 5, 5, [255; 3], 2);
}

/// A square 100 across about the origin facing +Z, `by` along x, as the six
/// corners of two triangles: the mesh keeps their order, a texel a corner.
fn corners(by: f32) -> Vec<[f32; 3]> {
    [
        (-50.0, -50.0),
        (50.0, -50.0),
        (50.0, 50.0),
        (-50.0, -50.0),
        (50.0, 50.0),
        (-50.0, 50.0),
    ]
    .iter()
    .map(|&(x, y)| [x + by, y, 0.0])
    .collect()
}

fn square_geometry(by: f32) -> CustomGeometry {
    let mut g = CustomGeometry::new();
    g.set_vertex_data(
        corners(by)
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect(),
    );
    g.set_stride(12);
    g.add_attribute(Semantic::Position, 0, ComponentType::F32);
    g.set_primitive_type(PrimitiveType::Triangles);
    g.update();
    g
}

/// The square at weight 1 of a target 100 along x, drawn red by a program
/// that takes each corner to the target itself, 100 in front of a white
/// wall; and the targets, to ask whether the shape was made on the CPU.
fn moved_square(scene: &mut Scene) -> Morphs {
    let mut texels: Vec<f32> = corners(100.0)
        .iter()
        .flat_map(|c| [c[0], c[1], c[2], 0.0])
        .collect();
    texels.extend((0..6).flat_map(|_| [0.0, 0.0, 1.0, 0.0]));
    let move_by = Vec3::new(100.0, 0.0, 0.0);
    let texels = Arc::new(MorphTexels::new(1, 6, 1, texels).moving(vec![(move_by, move_by)]));
    let morphs = Morphs::new(texels, vec![1.0], || Arc::new(square_geometry(100.0)));
    let mut m = material(
        Some("void MAIN() { VERTEX = MORPH_POSITION(0); }"),
        "void MAIN() { FRAGCOLOR = vec4(1.0, 0.0, 0.0, 1.0); }",
        false,
        &[],
    );
    assert_eq!(m.program.morphing(), Morphing::Itself);
    m.morphs = Some(morphs.clone());
    add(
        scene,
        Primitive::Rectangle,
        Material::Default(DefaultMaterial::default()),
        Transform {
            scale: Vec3::new(4.0, 4.0, 1.0),
            ..Transform::default()
        },
    );
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Custom(Arc::new(square_geometry(0.0))),
            material: Material::Custom(m),
        }))
        .with_transform(at(0.0, 0.0, 100.0)),
    );
    morphs
}

fn culling_camera(scene: &mut Scene, x: f32) {
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 1.0,
            clip_far: 1000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: true,
        }))
        .with_transform(at(x, 0.0, 600.0)),
    );
    scene.set_camera(Some(camera));
}

#[test]
fn a_shape_its_program_moves_into_view_is_drawn_and_not_made_on_the_cpu() {
    let Some(mut gpu) = offscreen() else { return };
    // The camera sees x 100 to 300: the square's mesh is out of it, where
    // the program takes it, 50 to 150, is in it.
    let mut scene = Scene::new();
    scene.set_environment(background(Color::BLACK));
    culling_camera(&mut scene, 200.0);
    let morphs = moved_square(&mut scene);
    let pic = draw(&mut gpu, &scene, "morph_culled");
    expect(&pic, 25, 100, [255, 0, 0], 2);
    // Nothing that does not run the program draws: no shadow, no depth
    // read, so the shape gone towards the targets is never made.
    assert!(!morphs.shaped(), "made with no pass to draw it");
}

#[test]
fn a_shape_its_program_moves_casts_its_shadow_where_it_goes() {
    let Some(mut gpu) = offscreen() else { return };
    let mut scene = Scene::new();
    scene.set_environment(background(Color::BLACK));
    culling_camera(&mut scene, 0.0);
    let morphs = moved_square(&mut scene);
    let mut sun = Node::new(NodeKind::Light(Light::Directional {
        color: Color::WHITE,
        ambient_color: Color::BLACK,
        brightness: 1.0,
    }))
    .with_transform(Transform {
        rotation: Quat::from_euler_degrees(Vec3::new(0.0, 45.0, 0.0)),
        ..Transform::default()
    });
    sun.shadow = Some(Shadow::default());
    scene.add(None, sun);
    let pic = draw(&mut gpu, &scene, "morph_shadow");
    assert!(
        morphs.shaped(),
        "the shadow pass draws the shape made on the CPU"
    );
    // The sun throws the square's shadow 100 to its left: over x -50 to 50,
    // not -150 to -50 where the mesh's own square would throw it.
    expect(&pic, 175, 100, [255, 0, 0], 2);
    expect(&pic, 100, 100, [117, 117, 117], 8);
    expect(&pic, 25, 100, [219, 219, 219], 4);
}
