//! An image is decoded once, however many devices draw it, and kept no
//! longer than they need.
//!
//! A decoded image may let its pixels go once a renderer has its own copy,
//! and makes them again from its file when something asks later. A process
//! can draw on more than one device at once: a window's, and the off-screen
//! one that draws a view inside an effect's source. Each uploads for itself,
//! so pixels let go after the first device's upload were decoded again for
//! the next, on the thread that draws. These pictures count how often that
//! happens, and whether the pixels are still there.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ljos_3d::gpu::Renderer;
use ljos_3d::math::Vec3;
use ljos_3d::offscreen::Offscreen;
use ljos_3d::{
    Background, Camera, Color, DefaultMaps, DefaultMaterial, Environment, LightProbe, Lighting,
    Material, MeshSource, Model, Node, NodeKind, PixelEncoding, Primitive, Scene, Texture,
    TextureImage, TexturePixels, Transform,
};

mod device;
use device::offscreen;

const SIDE: u32 = 64;

/// Pixels as a decoded file holds them: let go once uploaded, and decoded
/// again, counted, when asked for after that.
#[derive(Debug)]
struct Decoded {
    id: u64,
    side: u32,
    encoding: PixelEncoding,
    pixels: Mutex<Option<Vec<u8>>>,
    decodes: Arc<AtomicUsize>,
    /// How long reading the pixels takes: a renderer that reads them
    /// is that much longer in its frame.
    slow: Duration,
}

impl Decoded {
    fn decode(&self) -> Vec<u8> {
        self.decodes.fetch_add(1, Ordering::Relaxed);
        let n = (self.side * self.side) as usize;
        match self.encoding {
            PixelEncoding::Rgbe => [128, 128, 128, 128].repeat(n),
            _ => [200, 40, 40, 255].repeat(n),
        }
    }

    fn resident(&self) -> bool {
        self.pixels.lock().unwrap().is_some()
    }

    /// Whether the pixels are let go within a few seconds: they are kept a
    /// second at most for a device that has no copy, and a loaded machine
    /// may be late.
    fn let_go_soon(&self) -> bool {
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if !self.resident() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl TexturePixels for Decoded {
    fn id(&self) -> u64 {
        self.id
    }
    fn size(&self) -> [u32; 2] {
        [self.side, self.side]
    }
    fn encoding(&self) -> PixelEncoding {
        self.encoding
    }
    fn opaque(&self) -> bool {
        true
    }
    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        std::thread::sleep(self.slow);
        let mut pixels = self.pixels.lock().unwrap();
        let pixels = pixels.get_or_insert_with(|| self.decode());
        f(pixels);
    }
    fn uploaded(&self) {
        *self.pixels.lock().unwrap() = None;
    }
}

/// An image decoded once already, and the count of decodes after that.
fn decoded(encoding: PixelEncoding) -> (TextureImage, Arc<Decoded>, Arc<AtomicUsize>) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    wrapped(
        NEXT.fetch_add(1, Ordering::Relaxed),
        encoding,
        Duration::ZERO,
    )
}

/// Pixels with the id `id`, decoded once already, read in `slow`.
fn wrapped(
    id: u64,
    encoding: PixelEncoding,
    slow: Duration,
) -> (TextureImage, Arc<Decoded>, Arc<AtomicUsize>) {
    let decodes = Arc::new(AtomicUsize::new(0));
    let image = Decoded {
        id: id << 32,
        side: 16,
        encoding,
        pixels: Mutex::new(None),
        decodes: Arc::clone(&decodes),
        slow,
    };
    *image.pixels.lock().unwrap() = Some(image.decode());
    decodes.store(0, Ordering::Relaxed);
    let image = Arc::new(image);
    let texture = TextureImage(Arc::clone(&image) as Arc<dyn TexturePixels>);
    (texture, image, decodes)
}

fn scene(material: Material, probe: Option<TextureImage>) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::BLACK),
        light_probe: probe.map(|p| LightProbe::new(Texture::new(p))),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Rectangle),
            material,
        })),
    );
    scene
}

fn mapped(map: &TextureImage) -> Material {
    Material::Default(DefaultMaterial {
        lighting: Lighting::None,
        diffuse_map: Some(Texture::new(map.clone())),
        ..DefaultMaterial::default()
    })
}

/// A second renderer on the test's device: to the renderers, a device of
/// its own, with uploads of its own.
fn another(gpu: &Offscreen) -> Offscreen {
    Offscreen {
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
        adapter: gpu.adapter.clone(),
        renderer: Renderer::new(&gpu.device),
    }
}

#[test]
fn a_map_and_a_probe_two_devices_draw_are_decoded_once() {
    let Some(mut first) = offscreen() else { return };
    let mut second = another(&first);
    let (map, map_pixels, map_decodes) = decoded(PixelEncoding::Srgb8Straight);
    let (probe, probe_pixels, probe_decodes) = decoded(PixelEncoding::Rgbe);
    let s = scene(mapped(&map), Some(probe));

    // A host frame: the window's device, then the one that draws pictures
    // of items, as a shell draws a view and an effect that shows it.
    first.render(&s, SIDE, SIDE).unwrap();
    second.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(
        map_decodes.load(Ordering::Relaxed),
        0,
        "the second device decoded the map again"
    );
    assert_eq!(
        probe_decodes.load(Ordering::Relaxed),
        0,
        "the second device decoded the light probe again"
    );
    // Both have their copies: nothing is left to keep them for.
    assert!(!map_pixels.resident(), "the map's pixels are kept");
    assert!(!probe_pixels.resident(), "the probe's pixels are kept");
}

#[test]
fn a_device_slow_to_reach_an_image_is_waited_for() {
    let Some(mut first) = offscreen() else { return };
    let mut second = another(&first);
    // A renderer reads the probe before the maps: each device spends
    // longer than pixels are kept for an idle one before it gets to the
    // map, as a slow device does compiling what it draws.
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let (probe, _, _) = wrapped(
        NEXT.fetch_add(1, Ordering::Relaxed) | 1 << 31,
        PixelEncoding::Rgbe,
        Duration::from_millis(1500),
    );
    let (map, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    let s = scene(mapped(&map), Some(probe));
    first.render(&s, SIDE, SIDE).unwrap();
    second.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(
        decodes.load(Ordering::Relaxed),
        0,
        "let go while the second device was drawing"
    );
    assert!(!pixels.resident(), "kept after both have their copies");
}

#[test]
fn an_image_two_devices_read_as_a_map_and_a_probe_is_decoded_once() {
    let Some(mut first) = offscreen() else { return };
    let mut second = another(&first);
    let (image, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    let s = scene(mapped(&image), Some(image.clone()));
    first.render(&s, SIDE, SIDE).unwrap();
    second.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(
        decodes.load(Ordering::Relaxed),
        0,
        "let go as a probe while the map waited"
    );
    assert!(!pixels.resident(), "kept after both have their copies");
}

#[test]
fn the_same_pixels_in_two_wrappers_are_let_go_together() {
    let Some(mut first) = offscreen() else { return };
    let mut second = another(&first);
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed) | 1 << 30;
    let (one, one_pixels, _) = wrapped(id, PixelEncoding::Srgb8Straight, Duration::ZERO);
    let (other, other_pixels, decodes) = wrapped(id, PixelEncoding::Srgb8Straight, Duration::ZERO);
    first
        .render(&scene(mapped(&one), None), SIDE, SIDE)
        .unwrap();
    second
        .render(&scene(mapped(&other), None), SIDE, SIDE)
        .unwrap();
    assert_eq!(decodes.load(Ordering::Relaxed), 0);
    // Both devices have the pixels: neither wrapper keeps them.
    assert!(
        !one_pixels.resident(),
        "the first wrapper's pixels are kept"
    );
    assert!(!other_pixels.resident(), "the second's are kept");
}

#[test]
fn pixels_are_kept_a_while_for_a_device_that_has_not_drawn_them() {
    let Some(mut first) = offscreen() else { return };
    let _second = another(&first);
    let (map, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    first
        .render(&scene(mapped(&map), None), SIDE, SIDE)
        .unwrap();
    assert!(pixels.resident(), "let go before the second device drew");
    // It may never draw the image: they are not kept for good.
    assert!(pixels.let_go_soon(), "kept for a device that does not draw");
    assert_eq!(decodes.load(Ordering::Relaxed), 0);
}

#[test]
fn a_device_that_goes_is_not_waited_for() {
    let Some(mut first) = offscreen() else { return };
    let second = another(&first);
    let (map, pixels, _) = decoded(PixelEncoding::Srgb8Straight);
    first
        .render(&scene(mapped(&map), None), SIDE, SIDE)
        .unwrap();
    drop(second);
    assert!(!pixels.resident(), "kept for a device that is gone");
}

#[test]
fn one_image_read_two_ways_on_one_device_is_decoded_once() {
    let Some(mut gpu) = offscreen() else { return };
    let (map, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    // As a colour (sRGB) and as numbers (roughness): two uploads of it.
    let material = Material::Default(DefaultMaterial {
        lighting: Lighting::None,
        diffuse_map: Some(Texture::new(map.clone())),
        maps: Box::new(DefaultMaps {
            roughness: Some(Texture::new(map.clone())),
            ..DefaultMaps::default()
        }),
        ..DefaultMaterial::default()
    });
    gpu.render(&scene(material, None), SIDE, SIDE).unwrap();
    assert_eq!(decodes.load(Ordering::Relaxed), 0, "decoded for each use");
    assert!(!pixels.resident(), "kept after the upload");
}

#[test]
fn a_device_alone_lets_the_pixels_go_at_the_upload() {
    let Some(mut gpu) = offscreen() else { return };
    let (map, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    let (probe, probe_pixels, _) = decoded(PixelEncoding::Rgbe);
    let s = scene(mapped(&map), Some(probe));
    gpu.render(&s, SIDE, SIDE).unwrap();
    assert!(!pixels.resident(), "a lone device's upload keeps the map");
    assert!(
        !probe_pixels.resident(),
        "a lone device's upload keeps the probe"
    );
    // Drawn again from its own copy: nothing is decoded for that.
    gpu.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(decodes.load(Ordering::Relaxed), 0);
}

#[test]
fn an_image_let_go_and_drawn_by_another_device_later_is_decoded_for_it() {
    let Some(mut gpu) = offscreen() else { return };
    let mut late = another(&gpu);
    let (map, pixels, decodes) = decoded(PixelEncoding::Srgb8Straight);
    let s = scene(mapped(&map), None);
    gpu.render(&s, SIDE, SIDE).unwrap();
    assert!(pixels.let_go_soon());
    // Long after, the picture is still right: made again from the file.
    late.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(decodes.load(Ordering::Relaxed), 1);
    assert!(pixels.let_go_soon(), "kept after the late upload");
}
