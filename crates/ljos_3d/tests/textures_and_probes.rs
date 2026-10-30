//! Textures on materials and a light probe around the scene, drawn on a
//! device of its own and read back. The orthographic scenes show one scene
//! unit as one pixel, the origin in the middle of the picture.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ljos_3d::math::Vec3;
use ljos_3d::mesh::Mesh;
use ljos_3d::offscreen::Offscreen;
use ljos_3d::{
    Background, Camera, Color, ComponentType, CustomGeometry, DefaultMaterial, Environment,
    LightProbe, Lighting, Material, MeshSource, Model, Node, NodeKind, PixelEncoding, Primitive,
    PrincipledMaterial, Scene, Semantic, Texture, TextureImage, TexturePixels, Tiling, Tonemap,
    Transform,
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

#[derive(Debug)]
struct Pixels {
    id: u64,
    size: [u32; 2],
    encoding: PixelEncoding,
    data: Vec<u8>,
}

impl TexturePixels for Pixels {
    fn id(&self) -> u64 {
        self.id
    }
    fn size(&self) -> [u32; 2] {
        self.size
    }
    fn encoding(&self) -> PixelEncoding {
        self.encoding
    }
    fn opaque(&self) -> bool {
        self.data.chunks(4).all(|p| p[3] == 255)
    }
    fn with_pixels(&self, f: &mut dyn FnMut(&[u8])) {
        f(&self.data);
    }
}

fn image(
    w: u32,
    h: u32,
    encoding: PixelEncoding,
    pixel: impl Fn(u32, u32) -> [u8; 4],
) -> TextureImage {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            data.extend(pixel(x, y));
        }
    }
    TextureImage::new(Pixels {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        size: [w, h],
        encoding,
        data,
    })
}

/// Red top left, green top right, blue bottom left, white bottom right.
fn quadrants(side: u32) -> TextureImage {
    image(side, side, PixelEncoding::Srgb8Straight, |x, y| {
        match (x < side / 2, y < side / 2) {
            (true, true) => [255, 0, 0, 255],
            (false, true) => [0, 255, 0, 255],
            (true, false) => [0, 0, 255, 255],
            (false, false) => [255; 4],
        }
    })
}

/// A Radiance probe of 0.5 everywhere, or above the horizon only.
fn probe(w: u32, h: u32, lower: bool) -> TextureImage {
    image(w, h, PixelEncoding::Rgbe, |_, y| {
        if lower || y < h / 2 {
            [128, 128, 128, 128]
        } else {
            [0; 4]
        }
    })
}

fn scene(camera: Camera, z: f32) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::BLACK),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(camera)).with_transform(at(0.0, 0.0, z)),
    );
    scene.set_camera(Some(camera));
    scene
}

fn ortho() -> Scene {
    scene(
        Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: false,
        },
        600.0,
    )
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: Vec3::new(x, y, z),
        ..Transform::default()
    }
}

fn square(scene: &mut Scene, material: Material) {
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Rectangle),
            material,
        }))
        .with_transform(Transform {
            scale: Vec3::new(2.0, 2.0, 1.0),
            ..Transform::default()
        }),
    );
}

fn mapped(texture: Texture) -> Material {
    Material::Default(DefaultMaterial {
        lighting: Lighting::None,
        diffuse_map: Some(texture),
        ..DefaultMaterial::default()
    })
}

fn near(got: [u8; 4], want: [u8; 3], tolerance: u8) -> bool {
    (0..3).all(|i| got[i].abs_diff(want[i]) <= tolerance)
}

#[test]
fn a_diffuse_map_lies_upright_on_a_rectangle() {
    let Some(mut gpu) = offscreen() else { return };
    let mut s = ortho();
    square(&mut s, mapped(Texture::new(quadrants(64))));
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    assert!(
        near(p.pixel(50, 50), [255, 0, 0], 2),
        "{:?}",
        p.pixel(50, 50)
    );
    assert!(
        near(p.pixel(150, 50), [0, 255, 0], 2),
        "{:?}",
        p.pixel(150, 50)
    );
    assert!(
        near(p.pixel(50, 150), [0, 0, 255], 2),
        "{:?}",
        p.pixel(50, 150)
    );
    assert!(near(p.pixel(150, 150), [255, 255, 255], 2));
}

/// The built-in rectangle again, as an application's geometry with its
/// UVs: an application's texture coordinates reach the material as the
/// built-in mesh's do.
fn rectangle_as_geometry() -> CustomGeometry {
    let mesh = Mesh::primitive(Primitive::Rectangle);
    let mut vertices = Vec::new();
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        vertices.extend(p.iter().chain(uv).flat_map(|f| f.to_le_bytes()));
    }
    let mut g = CustomGeometry::new();
    g.set_vertex_data(vertices);
    g.set_stride(20);
    g.add_attribute(Semantic::Position, 0, ComponentType::F32);
    g.add_attribute(Semantic::TexCoord0, 12, ComponentType::F32);
    g.set_index_data(mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect());
    g.add_attribute(Semantic::Index, 0, ComponentType::U32);
    g.update();
    g
}

#[test]
fn an_application_s_geometry_is_mapped_by_its_own_uvs() {
    let Some(mut gpu) = offscreen() else { return };
    let mut built_in = ortho();
    square(&mut built_in, mapped(Texture::new(quadrants(64))));
    let want = gpu.render(&built_in, SIDE, SIDE).unwrap();

    let mut s = ortho();
    s.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Custom(Arc::new(rectangle_as_geometry())),
            material: mapped(Texture::new(quadrants(64))),
        }))
        .with_transform(Transform {
            scale: Vec3::new(2.0, 2.0, 1.0),
            ..Transform::default()
        }),
    );
    let got = gpu.render(&s, SIDE, SIDE).unwrap();
    for (x, y) in [(50, 50), (150, 50), (50, 150), (150, 150)] {
        assert!(
            near(
                got.pixel(x, y),
                want.pixel(x, y)[..3].try_into().unwrap(),
                2
            ),
            "at {x},{y}: {:?}, the built-in mesh drew {:?}",
            got.pixel(x, y),
            want.pixel(x, y)
        );
    }
    // Not one colour everywhere, as a geometry whose UVs were lost would be.
    assert_ne!(got.pixel(50, 50), got.pixel(150, 150));
}

#[test]
fn tiling_says_what_lies_beyond_the_edge() {
    let Some(mut gpu) = offscreen() else { return };
    // With scaleU 3, x 110 reads u 1.65 and x 150 reads u 2.25.
    for (tiling, at_110, at_150) in [
        (Tiling::Repeat, [0, 255, 0], [255, 0, 0]),
        (Tiling::MirroredRepeat, [255, 0, 0], [255, 0, 0]),
        (Tiling::ClampToEdge, [0, 255, 0], [0, 255, 0]),
    ] {
        let mut texture = Texture::new(quadrants(64));
        texture.uv.scale = [3.0, 1.0];
        texture.sampling.tiling_u = tiling;
        let mut s = ortho();
        square(&mut s, mapped(texture));
        let p = gpu.render(&s, SIDE, SIDE).unwrap();
        assert!(
            near(p.pixel(110, 50), at_110, 2),
            "{tiling:?} {:?}",
            p.pixel(110, 50)
        );
        assert!(
            near(p.pixel(150, 50), at_150, 2),
            "{tiling:?} {:?}",
            p.pixel(150, 50)
        );
    }
}

#[test]
fn one_image_is_uploaded_once_for_every_material() {
    let Some(mut gpu) = offscreen() else { return };
    let shared = quadrants(64);
    let mut s = ortho();
    square(&mut s, mapped(Texture::new(shared.clone())));
    square(&mut s, mapped(Texture::new(shared.clone())));
    gpu.render(&s, SIDE, SIDE).unwrap();
    assert_eq!(gpu.renderer.texture_bytes().maps, 64 * 64 * 4);
    // Once no scene holds the image, the next frame lets it go.
    drop((s, shared));
    gpu.render(&ortho(), SIDE, SIDE).unwrap();
    assert_eq!(gpu.renderer.texture_bytes().maps, 0);
}

fn probed(image: TextureImage, tonemap: Tonemap, f: impl FnOnce(&mut LightProbe)) -> Scene {
    let mut s = scene(Camera::PERSPECTIVE, 300.0);
    let mut probe = LightProbe::new(Texture::new(image));
    f(&mut probe);
    s.set_environment(Environment {
        background: Background::SkyBox,
        light_probe: Some(probe),
        tonemap,
        ..Environment::default()
    });
    s
}

#[test]
fn the_sky_box_shows_the_probe_under_each_tone_mapping() {
    let Some(mut gpu) = offscreen() else { return };
    let image = probe(64, 32, true);
    for (tonemap, want) in [
        (Tonemap::None, 0x80),
        (Tonemap::Linear, 0xbc),
        (Tonemap::Aces, 0xce),
        (Tonemap::HejlDawson, 0xba),
        (Tonemap::Filmic, 0x96),
    ] {
        let p = gpu
            .render(&probed(image.clone(), tonemap, |_| {}), SIDE, SIDE)
            .unwrap();
        assert!(
            near(p.pixel(10, 10), [want; 3], 3),
            "{tonemap:?} {:?}",
            p.pixel(10, 10)
        );
    }
}

#[test]
fn up_in_the_probe_is_up_in_the_scene_until_it_is_turned() {
    let Some(mut gpu) = offscreen() else { return };
    let image = probe(64, 32, false);
    let p = gpu
        .render(&probed(image.clone(), Tonemap::Linear, |_| {}), SIDE, SIDE)
        .unwrap();
    assert!(
        near(p.pixel(100, 20), [0xbc; 3], 3),
        "{:?}",
        p.pixel(100, 20)
    );
    assert!(
        near(p.pixel(100, 180), [0; 3], 3),
        "{:?}",
        p.pixel(100, 180)
    );
    let turned = probed(image, Tonemap::Linear, |p| {
        p.orientation = Vec3::new(180.0, 0.0, 0.0)
    });
    let p = gpu.render(&turned, SIDE, SIDE).unwrap();
    assert!(near(p.pixel(100, 20), [0; 3], 3), "{:?}", p.pixel(100, 20));
    assert!(
        near(p.pixel(100, 180), [0xbc; 3], 3),
        "{:?}",
        p.pixel(100, 180)
    );
}

#[test]
fn the_probe_lights_a_rough_white_surface_with_its_radiance() {
    let Some(mut gpu) = offscreen() else { return };
    let image = probe(64, 32, true);
    let mut s = probed(image.clone(), Tonemap::Linear, |_| {});
    let rough = PrincipledMaterial {
        roughness: 1.0,
        ..PrincipledMaterial::default()
    };
    square(&mut s, Material::Principled(rough.clone()));
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    assert!(
        near(p.pixel(100, 100), [0xbc; 3], 12),
        "{:?}",
        p.pixel(100, 100)
    );

    let mut s = probed(image, Tonemap::Linear, |p| p.horizon = 1.0);
    square(&mut s, Material::Principled(rough));
    let p = gpu.render(&s, SIDE, SIDE).unwrap();
    assert!(
        near(p.pixel(100, 100), [0x89; 3], 12),
        "{:?}",
        p.pixel(100, 100)
    );
}

/// Not a check but a measure: what a 2k probe costs on the device and how
/// long its maps take, printed for the record.
#[test]
fn a_2k_probes_maps_and_what_they_cost() {
    let Some(mut gpu) = offscreen() else { return };
    let image = probe(2048, 1024, true);
    let s = probed(image.clone(), Tonemap::Linear, |_| {});
    let plain = gpu.render(&ortho(), SIDE, SIDE).unwrap();
    drop(plain);
    let start = Instant::now();
    gpu.render(&s, SIDE, SIDE).unwrap();
    let first = start.elapsed();
    let start = Instant::now();
    gpu.render(&probed(image, Tonemap::Aces, |_| {}), SIDE, SIDE)
        .unwrap();
    let again = start.elapsed();
    let bytes = gpu.renderer.texture_bytes().probes;
    eprintln!(
        "2048x1024 probe on {}: maps {bytes} bytes; first frame {first:?}, \
         a frame with the maps made {again:?}",
        gpu.adapter.name
    );
    assert_eq!(bytes, 16_822_272);
}
