//! Files made by exporters rather than by hand: the Khronos glTF samples
//! with skins and morph targets, loaded and drawn. The files are not in
//! the repository; `LJOS_3D_ASSETS` names the directory holding them, and
//! a test whose file is missing says SKIPPED. With `LJOS_3D_PICTURES` set
//! to a directory, each picture is also saved there.

#![cfg(feature = "gltf")]

use std::path::PathBuf;

use ljos_3d::gltf::{self, Asset};
use ljos_3d::math::Vec3;
use ljos_3d::offscreen::{Offscreen, Rgba};
use ljos_3d::{Background, Camera, Color, Environment, Light, Node, NodeKind, Scene, Transform};

const SIDE: u32 = 200;
const CLEAR: [u8; 3] = [32, 32, 32];

fn asset_path(name: &str) -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("LJOS_3D_ASSETS") else {
        eprintln!("SKIPPED: LJOS_3D_ASSETS is not set");
        return None;
    };
    let path = PathBuf::from(dir).join(name);
    if !path.is_file() {
        eprintln!("SKIPPED: {} is missing", path.display());
        return None;
    }
    Some(path)
}

fn load(name: &str) -> Option<Asset> {
    let path = asset_path(name)?;
    let bytes = std::fs::read(&path).unwrap();
    let dir = path.parent().unwrap().to_owned();
    let mut fetch = |uri: &str| std::fs::read(dir.join(uri)).map_err(|e| format!("{uri}: {e}"));
    let asset = gltf::parse(&bytes).unwrap().build(&mut fetch).unwrap();
    Some(asset)
}

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn save(name: &str, picture: &Rgba) {
    let Some(dir) = std::env::var_os("LJOS_3D_PICTURES") else {
        return;
    };
    let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&picture.pixels).unwrap();
}

/// The asset under an orthographic camera that fits its box, lit from the
/// camera's side. Exporters use metres, so the box, not a fixed scale,
/// says how near to look.
fn framed(asset: &Asset) -> (Scene, Vec<ljos_3d::NodeId>) {
    let (lo, hi) = asset.bounds.expect("the asset draws something");
    let centre = (lo + hi) * 0.5;
    let extent = (hi.x - lo.x).max(hi.y - lo.y).max(1e-3);
    let magnification = SIDE as f32 / (extent * 1.25);
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(32.0 / 255.0, 32.0 / 255.0, 32.0 / 255.0)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 0.01,
            clip_far: 1000.0,
            horizontal_magnification: magnification,
            vertical_magnification: magnification,
            frustum_culling: true,
        }))
        .with_transform(Transform {
            position: Vec3::new(centre.x, centre.y, hi.z + 10.0),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::Directional {
            color: Color::WHITE,
            ambient_color: Color::rgb(0.3, 0.3, 0.3),
            brightness: 1.0,
        })),
    );
    let root = scene.add(None, Node::new(NodeKind::Group));
    let placed = asset.add_to(&mut scene, root);
    let models = placed.models.iter().map(|&(_, m)| m).collect();
    (scene, models)
}

/// Pixels that are not the background.
fn drawn(p: &Rgba) -> usize {
    (0..p.height)
        .flat_map(|y| (0..p.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let got = p.pixel(x, y);
            (0..3).any(|i| got[i].abs_diff(CLEAR[i]) > 2)
        })
        .count()
}

fn draw(name: &str, asset: &Asset) -> Option<usize> {
    let mut gpu = offscreen()?;
    let (scene, _) = framed(asset);
    let p = gpu.render(&scene, SIDE, SIDE).unwrap();
    save(&format!("exported-{name}.png"), &p);
    Some(drawn(&p))
}

fn assert_skinned(file: &str, joints: usize) {
    let Some(asset) = load(file) else { return };
    assert_eq!(asset.skins.len(), 1, "{file}");
    assert_eq!(asset.skins[0].joints.len(), joints, "{file}");
    assert_eq!(asset.skins[0].inverse_bind.len(), joints, "{file}");
    let (scene, models) = framed(&asset);
    assert!(
        models
            .iter()
            .any(|&m| scene.node(m).is_some_and(|n| n.skin.is_some())),
        "{file}: no model was given its skin"
    );
    if let Some(pixels) = draw(file, &asset) {
        assert!(pixels > 500, "{file}: {pixels} pixels drawn");
    }
}

#[test]
fn simple_skin_is_skinned_and_drawn() {
    assert_skinned("SimpleSkin.gltf", 2);
}

#[test]
fn rigged_simple_is_skinned_and_drawn() {
    assert_skinned("RiggedSimple.glb", 2);
}

/// SimpleMorph's triangle reaches y = 0.5; its node weighs both targets by
/// a half, which lifts it to y = 1, so a box taller than the base mesh's
/// shows the targets were applied.
#[test]
fn simple_morph_is_morphed_by_its_weights() {
    let Some(asset) = load("SimpleMorph.gltf") else {
        return;
    };
    let (lo, hi) = asset.bounds.unwrap();
    assert!(hi.y > 0.9 && lo.y > -0.01, "bounds {lo:?} to {hi:?}");
    if let Some(pixels) = draw("SimpleMorph.gltf", &asset) {
        assert!(pixels > 500, "{pixels} pixels drawn");
    }
}

#[test]
fn animated_morph_cube_is_drawn() {
    let Some(asset) = load("AnimatedMorphCube.glb") else {
        return;
    };
    assert!(!asset.meshes.is_empty());
    if let Some(pixels) = draw("AnimatedMorphCube.glb", &asset) {
        assert!(pixels > 500, "{pixels} pixels drawn");
    }
}
