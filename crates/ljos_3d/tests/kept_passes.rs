//! That the post-processing chain keeps what its passes bind from one
//! frame to the next, and that what it keeps shows what the frame asked
//! for, not what the last one did.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

use std::sync::Arc;

use ljos_3d::math::Vec3;
use ljos_3d::offscreen::read_back;
use ljos_3d::{
    AmbientOcclusion, Background, Camera, Color, DefaultMaterial, DepthOfField, Effect,
    EffectBuffer, Environment, Glow, GlowBlend, Gpu, Lighting, Material, MeshSource, Model, Node,
    NodeKind, Pass, PostProcess, Primitive, Program, Scene, Target, Transform, UniformKind,
    UniformValue, Vignette, snippet, stats,
};

const SIDE: u32 = 160;

const DIM: &str = "void MAIN() { FRAGCOLOR = texture(INPUT, INPUT_UV) * vec4(vec3(factor), 1.0); }";

/// The effect's program, made once: a program no scene holds any more is
/// let go of, and one made again would be compiled again.
fn dim() -> Arc<Program> {
    let uniforms = vec![("factor".to_string(), UniformKind::Float)];
    Arc::new(snippet::effect(None, DIM, &uniforms, &[]).unwrap_or_else(|e| panic!("{e}")))
}

fn effect_pass(program: &Arc<Program>, factor: f32) -> Pass {
    Pass {
        leaves: None,
        program: program.clone(),
        values: vec![UniformValue::Float(factor)],
        textures: Vec::new(),
        input: None,
        output: None,
    }
}

/// Every pass the chain has, near enough: ambient occlusion, an effect of
/// two passes through a buffer of its own, depth of field, glow, the
/// vignette and FXAA in the last pass.
fn environment(dim: &Arc<Program>, exposure: f32, factor: f32) -> Environment {
    Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.3)),
        exposure,
        ambient_occlusion: Some(AmbientOcclusion {
            strength: 50.0,
            distance: 20.0,
            softness: 10.0,
            sample_rate: 2,
            bias: 0.0,
            dither: false,
        }),
        effects: vec![Effect {
            buffers: vec![EffectBuffer {
                format: ljos_3d::BufferFormat::Rgba16F,
                ..EffectBuffer::default()
            }],
            passes: vec![
                Pass {
                    output: Some(0),
                    ..effect_pass(dim, factor)
                },
                Pass {
                    input: Some(0),
                    ..effect_pass(dim, 1.0)
                },
            ],
        }],
        post: PostProcess {
            on: true,
            glow: Some(Glow {
                hdr_minimum: 0.0,
                bloom: 1.0,
                blend: GlowBlend::Additive,
                levels: 0b111,
                ..Glow::default()
            }),
            depth_of_field: Some(DepthOfField {
                focus_distance: 100.0,
                focus_range: 10.0,
                blur_amount: 4.0,
            }),
            vignette: Some(Vignette {
                color: [1.0, 0.0, 0.0],
                radius: 0.35,
                strength: 5.0,
            }),
            fxaa: true,
            ..PostProcess::default()
        },
        ..Environment::default()
    }
}

/// A square at `x` in front of a wider one, seen by an orthographic camera
/// where a scene unit is a pixel.
fn scene(dim: &Arc<Program>, x: f32, exposure: f32, factor: f32) -> Scene {
    let mut s = Scene::new();
    s.set_environment(environment(dim, exposure, factor));
    let camera = s.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 1.0,
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
    for (position, scale, color) in [
        (
            Vec3::new(0.0, 0.0, -50.0),
            Vec3::new(1.2, 1.2, 1.0),
            Color::rgb(0.0, 0.0, 1.0),
        ),
        (
            Vec3::new(x, 0.0, 0.0),
            Vec3::new(0.4, 0.4, 1.0),
            Color::WHITE,
        ),
    ] {
        s.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(Primitive::Rectangle),
                material: Material::Default(DefaultMaterial {
                    diffuse_color: color,
                    lighting: Lighting::None,
                    ..DefaultMaterial::default()
                }),
            }))
            .with_transform(Transform {
                position,
                scale,
                ..Transform::default()
            }),
        );
    }
    s
}

fn frame(gpu: &Gpu, target: &mut Target, scene: &Scene) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(
        gpu.renderer()
            .render(gpu.device(), gpu.queue(), &mut encoder, scene, target)
    );
    gpu.queue().submit([encoder.finish()]);
    stats::frame_end(0.0);
}

fn pixels(gpu: &Gpu, target: &Target) -> Vec<u8> {
    read_back(gpu.device(), gpu.queue(), target.texture())
        .unwrap()
        .pixels
}

#[test]
fn a_moving_scene_through_every_pass_makes_no_buffers_or_groups_once_warm() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let dim = dim();
    let mut target = Target::new(gpu.device(), &gpu.renderer(), [SIDE, SIDE]);

    // The first frames make the pipelines, the chain's pictures and what
    // its passes bind.
    for x in 0..3 {
        frame(&gpu, &mut target, &scene(&dim, x as f32, 1.0, 1.0));
    }
    // The square moves, so the frame is drawn again with every pass.
    frame(&gpu, &mut target, &scene(&dim, 3.0, 1.0, 1.0));
    let warm = stats::snapshot().last;
    assert!(warm.render_passes >= 10, "{warm:?}");
    assert_eq!(warm.buffers_created, 0, "{warm:?}");
    assert_eq!(warm.bind_groups_created, 0, "{warm:?}");
    assert_eq!(warm.textures_created, 0, "{warm:?}");

    // What a pass keeps is written again when its uniforms change: the
    // kept target, drawn darker and through a dimmer effect, shows what a
    // target drawn so from the start shows.
    frame(&gpu, &mut target, &scene(&dim, 3.0, 0.5, 0.5));
    let changed = stats::snapshot().last;
    assert_eq!(changed.buffers_created, 0, "{changed:?}");
    assert_eq!(changed.bind_groups_created, 0, "{changed:?}");
    let kept = pixels(&gpu, &target);

    let mut fresh = Target::new(gpu.device(), &gpu.renderer(), [SIDE, SIDE]);
    frame(&gpu, &mut fresh, &scene(&dim, 3.0, 0.5, 0.5));
    let want = pixels(&gpu, &fresh);
    assert!(
        kept == want,
        "the kept passes drew another picture than fresh ones"
    );

    // And back: the same target, asked for the first look again.
    frame(&gpu, &mut target, &scene(&dim, 3.0, 1.0, 1.0));
    let mut first = Target::new(gpu.device(), &gpu.renderer(), [SIDE, SIDE]);
    frame(&gpu, &mut first, &scene(&dim, 3.0, 1.0, 1.0));
    assert!(pixels(&gpu, &target) == pixels(&gpu, &first));
    assert!(
        pixels(&gpu, &first) != want,
        "the exposure and the effect change the picture"
    );

    // The chain changes shape: a frame without the effect moves every pass
    // after it to another place, and the next frame, with the effect back,
    // moves them home. What each place keeps is matched with what its pass
    // binds, so the target draws what a fresh one draws.
    let mut bare = scene(&dim, 3.0, 0.5, 0.5);
    let mut env = bare.environment().clone();
    env.effects.clear();
    bare.set_environment(env);
    frame(&gpu, &mut target, &bare);
    let mut fresh_bare = Target::new(gpu.device(), &gpu.renderer(), [SIDE, SIDE]);
    frame(&gpu, &mut fresh_bare, &bare);
    let without = pixels(&gpu, &fresh_bare);
    assert!(
        pixels(&gpu, &target) == without,
        "the kept passes drew another picture without the effect"
    );
    frame(&gpu, &mut target, &scene(&dim, 3.0, 0.5, 0.5));
    let back = pixels(&gpu, &target);
    assert!(
        back == want,
        "the kept passes drew another picture with the effect back"
    );
    assert!(without != want, "the effect changes the picture");
}
