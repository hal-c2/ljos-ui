//! A picture drawn bottom row first, as GL keeps one, shows the right way
//! up once the blit turns it, its faces wound the other way still facing
//! the camera; and one something reads by where on the screen it is stays
//! top row first.

#[allow(dead_code)]
mod spike_scene;

use ljos_3d::math::Vec3;
use ljos_3d::offscreen::read_back;
use ljos_3d::{
    Color, DefaultMaterial, Environment, Gpu, Material, MeshSource, Model, Node, NodeKind,
    PostProcess, Primitive, Target, Transform,
};
use spike_scene::{HEIGHT, WIDTH, grey, lit_cube};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Where the ball's middle shows: 120 above the cube under a 60° view from
/// 300 away.
const BALL: (u32, u32) = (WIDTH / 2, HEIGHT / 2 - 83);

/// The lit cube, drawn over four samples as an edge is in a View3D with
/// multisampling, and a red ball above it: a picture that is not the same
/// upside down.
fn scene(environment: Environment) -> ljos_3d::Scene {
    let mut scene = lit_cube(environment.background);
    scene.set_environment(environment);
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Sphere),
            material: Material::Default(DefaultMaterial {
                diffuse_color: Color::rgb(1.0, 0.0, 0.0),
                ..DefaultMaterial::default()
            }),
        }))
        .with_transform(Transform {
            position: Vec3::new(0.0, 120.0, 0.0),
            scale: Vec3::new(0.3, 0.3, 0.3),
            ..Transform::default()
        }),
    );
    scene
}

/// Draws `scene` into a target, drawn bottom up or not, and shows it in a
/// window through the blit: the window's pixels, and whether the picture
/// lay bottom row first.
fn shown(gpu: &Gpu, scene: &ljos_3d::Scene, bottom_up: bool) -> (Vec<u8>, bool) {
    gpu.renderer().prepare_blit(gpu.device(), FORMAT);
    let mut target = Target::new(gpu.device(), &gpu.renderer(), [WIDTH, HEIGHT]);
    target.draw_bottom_up(bottom_up);
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    gpu.renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, &mut target);
    let window = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = window.create_view(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("window"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        gpu.renderer().blit(&mut pass, FORMAT, &target);
    }
    gpu.queue().submit([encoder.finish()]);
    let rgba = read_back(gpu.device(), gpu.queue(), &window).expect("read back");
    (rgba.pixels, target.drawn_bottom_up())
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * WIDTH + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

fn gpu() -> Option<Gpu> {
    match pollster::block_on(Gpu::new()) {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

#[test]
fn a_picture_drawn_bottom_up_shows_the_right_way_up() {
    let Some(gpu) = gpu() else { return };
    let scene = scene(Environment {
        background: grey(),
        msaa_samples: 4,
        ..Environment::default()
    });
    let (top_down, turned) = shown(&gpu, &scene, false);
    assert!(!turned);
    let (bottom_up, turned) = shown(&gpu, &scene, true);
    assert!(turned, "a picture nothing reads is drawn bottom row first");

    // The ball is above the cube in both: red at the top, none at the
    // bottom.
    for pixels in [&top_down, &bottom_up] {
        let [r, g, b, _] = pixel(pixels, BALL.0, BALL.1);
        assert!(r > 120 && g < 60 && b < 60, "the ball: {r} {g} {b}");
    }
    // Faces wound the other way still face the camera: the cube's middle
    // is its lit front, not the grey behind it.
    assert_eq!(
        pixel(&bottom_up, WIDTH / 2, HEIGHT / 2),
        pixel(&top_down, WIDTH / 2, HEIGHT / 2)
    );
    // Only where an edge meets the samples does a pixel differ.
    let differ = top_down
        .chunks(4)
        .zip(bottom_up.chunks(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
        .count();
    assert!(
        differ < (WIDTH * HEIGHT / 50) as usize,
        "{differ} pixels differ"
    );
}

#[test]
fn a_picture_post_processing_reads_stays_top_down() {
    let Some(gpu) = gpu() else { return };
    let scene = scene(Environment {
        background: grey(),
        post: PostProcess {
            on: true,
            ..PostProcess::default()
        },
        ..Environment::default()
    });
    let (pixels, turned) = shown(&gpu, &scene, true);
    assert!(!turned, "post-processing counts rows from the top");
    let [r, g, b, _] = pixel(&pixels, BALL.0, BALL.1);
    assert!(r > 120 && g < 60 && b < 60, "the ball: {r} {g} {b}");
}
