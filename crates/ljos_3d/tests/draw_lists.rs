//! A frame of a thousand models whose camera alone moves makes its draw
//! lists in what the frames before it left: the renderer asks the
//! allocator a few dozen times for it, not once for each model, cell or
//! group, and not for the lists again, which for a thousand models and
//! their uniforms are megabytes. Drawn as one or each on its own. What it
//! still asks for is the scene's walk and the commands wgpu records.
//!
//! One test, in a process of its own: the allocator counted is the
//! process's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Gpu, Light, Material, MeshSource,
    Model, Node, NodeId, NodeKind, Primitive, Scene, Target, Transform,
};

/// The bytes asked of the allocator on the thread that counts, while it
/// counts. Other threads, wgpu's and the driver's, are not the renderer's
/// lists.
struct Counting;

static BYTES: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

fn note(size: usize) {
    if COUNTING.with(Cell::get) {
        BYTES.fetch_add(size, Ordering::Relaxed);
        CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

// SAFETY: every call is the system allocator's, with what it was given.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        // SAFETY: the caller's layout, as `GlobalAlloc::alloc` has it.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        // SAFETY: as `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(new_size);
        // SAFETY: a block this allocator gave, as `GlobalAlloc::realloc` has it.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: a block this allocator gave, with its layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SIZE: [u32; 2] = [320, 200];
const COLUMNS: usize = 40;
const ROWS: usize = 25;

/// A thousand cubes in a grid, each its own colour, and in front of them
/// a sphere seen through: a list for what is opaque and one for what blends.
fn scene() -> (Scene, NodeId) {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(None, Node::new(NodeKind::Camera(Camera::PERSPECTIVE)));
    scene.set_camera(Some(camera));
    scene.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
    for i in 0..COLUMNS * ROWS {
        let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(Primitive::Cube),
                material: Material::Default(DefaultMaterial {
                    diffuse_color: Color::rgb(c / COLUMNS as f32, r / ROWS as f32, 0.5),
                    ..DefaultMaterial::default()
                }),
            }))
            .with_transform(Transform {
                position: Vec3::new(-390.0 + 20.0 * c, -240.0 + 20.0 * r, 0.0),
                scale: Vec3::new(0.12, 0.12, 0.12),
                ..Transform::default()
            }),
        );
    }
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Sphere),
            material: Material::Default(DefaultMaterial {
                opacity: 0.5,
                ..DefaultMaterial::default()
            }),
        }))
        .with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 60.0),
            ..Transform::default()
        }),
    );
    (scene, camera)
}

/// Puts the camera `x` across, 700 in front of the grid.
fn look_from(scene: &mut Scene, camera: NodeId, x: f32) {
    scene.node_mut(camera).unwrap().transform = Transform {
        position: Vec3::new(x, 0.0, 700.0),
        rotation: Quat::IDENTITY,
        ..Transform::default()
    };
}

/// Draws `scene` into `target` and submits; returns the bytes and calls
/// the renderer asked the allocator for while it prepared and recorded.
fn frame(gpu: &Gpu, target: &mut Target, scene: &Scene) -> (usize, usize) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut renderer = gpu.renderer();
    let (bytes, calls) = (BYTES.load(Ordering::Relaxed), CALLS.load(Ordering::Relaxed));
    COUNTING.with(|c| c.set(true));
    let drew = renderer.render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    COUNTING.with(|c| c.set(false));
    drop(renderer);
    assert!(drew, "a scene whose camera moved is drawn again");
    let asked = (
        BYTES.load(Ordering::Relaxed) - bytes,
        CALLS.load(Ordering::Relaxed) - calls,
    );
    gpu.queue().submit([encoder.finish()]);
    asked
}

#[test]
fn a_frame_whose_camera_moves_makes_its_lists_in_the_last_ones() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let (mut scene, camera) = scene();
    let asked: Vec<(bool, usize, usize)> = [true, false]
        .into_iter()
        .map(|batching| {
            gpu.renderer().set_batching(batching);
            let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
            // The first frames make the pipelines, meshes and buffers a
            // frame keeps, and the lists' room.
            for x in [0.0, 5.0, 10.0] {
                look_from(&mut scene, camera, x);
                frame(&gpu, &mut target, &scene);
            }
            look_from(&mut scene, camera, 15.0);
            let (bytes, calls) = frame(&gpu, &mut target, &scene);
            eprintln!("batching {batching}: {bytes} bytes in {calls} calls");
            (batching, bytes, calls)
        })
        .collect();
    for (batching, bytes, calls) in asked {
        // A thousand models' uniforms are 900 KB, a list of them made again
        // twice that as it grows; the scene's walk is 150 KB a time and
        // wgpu's commands for a thousand draws 230 KB.
        assert!(
            calls < 100 && bytes < 1024 * 1024,
            "batching {batching}: the frame asked for {bytes} bytes in {calls} calls"
        );
    }
}
