//! An application's own geometry: uploaded once per revision, dropped with
//! the last scene that holds it, and culled by its bounds. An orthographic
//! camera at magnification 1 makes one scene unit one pixel.

use std::sync::Arc;

use ljos_3d::math::Vec3;
use ljos_3d::offscreen::{Offscreen, Rgba};
use ljos_3d::{
    Background, Camera, Color, ComponentType, CustomGeometry, DefaultMaterial, Environment,
    Lighting, Material, MeshSource, Model, Node, NodeKind, PrimitiveType, Scene, Semantic,
};

const SIDE: u32 = 100;
const RED: [u8; 4] = [255, 0, 0, 255];

fn offscreen() -> Option<Offscreen> {
    match Offscreen::new() {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

/// A square of side `size` about the origin, facing +Z, as a fan.
fn square(size: f32) -> CustomGeometry {
    let h = size / 2.0;
    let mut g = CustomGeometry::new();
    g.set_vertex_data(
        [-h, -h, 0.0, h, -h, 0.0, h, h, 0.0, -h, h, 0.0]
            .iter()
            .flat_map(|f: &f32| f.to_le_bytes())
            .collect(),
    );
    g.set_stride(12);
    g.add_attribute(Semantic::Position, 0, ComponentType::F32);
    g.set_primitive_type(PrimitiveType::TriangleFan);
    g.update();
    g
}

fn scene(geometry: &Arc<CustomGeometry>, culling: bool) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.0, 0.0, 0.0)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::Orthographic {
            clip_near: 10.0,
            clip_far: 10000.0,
            horizontal_magnification: 1.0,
            vertical_magnification: 1.0,
            frustum_culling: culling,
        }))
        .with_transform(ljos_3d::Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Default::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Custom(geometry.clone()),
            material: Material::Default(DefaultMaterial {
                diffuse_color: Color::rgb(1.0, 0.0, 0.0),
                lighting: Lighting::None,
                ..DefaultMaterial::default()
            }),
        })),
    );
    scene
}

fn draw(gpu: &mut Offscreen, scene: &Scene) -> Rgba {
    gpu.render(scene, SIDE, SIDE).unwrap()
}

#[test]
fn a_geometry_is_decoded_and_drawn_where_its_positions_say() {
    let Some(mut gpu) = offscreen() else { return };
    let g = Arc::new(square(60.0));
    let picture = draw(&mut gpu, &scene(&g, false));
    assert_eq!(picture.pixel(50, 50), RED);
    assert_eq!(picture.pixel(25, 25), RED);
    assert_eq!(picture.pixel(10, 50), [0, 0, 0, 255], "outside the square");
}

#[test]
fn a_revision_is_uploaded_again_and_an_unchanged_one_is_not() {
    let Some(mut gpu) = offscreen() else { return };
    let g = Arc::new(square(30.0));
    let first = draw(&mut gpu, &scene(&g, false));
    assert_eq!(first.pixel(25, 50), [0, 0, 0, 255]);
    assert_eq!(gpu.renderer.custom_uploads(), 1);

    // The same revision in another scene is the same buffers.
    draw(&mut gpu, &scene(&g, false));
    assert_eq!(gpu.renderer.custom_uploads(), 1);

    let mut bigger = (*g).clone();
    bigger.set_vertex_data(square(60.0).vertex_data().to_vec());
    bigger.update();
    let picture = draw(&mut gpu, &scene(&Arc::new(bigger), false));
    assert_eq!(gpu.renderer.custom_uploads(), 2);
    assert_eq!(picture.pixel(25, 50), RED, "the new vertices");
    assert_eq!(gpu.renderer.custom_meshes(), 1, "one geometry, one entry");
}

#[test]
fn a_geometry_no_scene_holds_is_dropped() {
    let Some(mut gpu) = offscreen() else { return };
    let g = Arc::new(square(30.0));
    let s = scene(&g, false);
    draw(&mut gpu, &s);
    assert_eq!(gpu.renderer.custom_meshes(), 1);
    drop(s);
    drop(g);
    // Another scene is drawn, as the next frame of some view is.
    let other = Arc::new(square(20.0));
    draw(&mut gpu, &scene(&other, false));
    assert_eq!(gpu.renderer.custom_meshes(), 1, "only the one in use");
}

#[test]
fn bounds_decide_whether_culling_keeps_the_model() {
    let Some(mut gpu) = offscreen() else { return };
    let mut wrong = square(60.0);
    // Claims to lie far off to the side while its vertices are at the origin.
    wrong.set_bounds(Vec3::new(4000.0, 0.0, 0.0), Vec3::new(4010.0, 10.0, 10.0));
    wrong.update();
    let culled = draw(&mut gpu, &scene(&Arc::new(wrong), true));
    assert_eq!(culled.pixel(50, 50), [0, 0, 0, 255]);

    let mut right = square(60.0);
    right.set_bounds(Vec3::new(-30.0, -30.0, 0.0), Vec3::new(30.0, 30.0, 0.0));
    right.update();
    let kept = draw(&mut gpu, &scene(&Arc::new(right), true));
    assert_eq!(kept.pixel(50, 50), RED);
}

#[test]
fn a_geometry_that_cannot_be_decoded_draws_nothing_and_is_not_retried() {
    let Some(mut gpu) = offscreen() else { return };
    let mut broken = square(60.0);
    broken.set_primitive_type(PrimitiveType::Lines);
    broken.update();
    let g = Arc::new(broken);
    let picture = draw(&mut gpu, &scene(&g, false));
    assert_eq!(picture.pixel(50, 50), [0, 0, 0, 255]);
    draw(&mut gpu, &scene(&g, false));
    assert_eq!(gpu.renderer.custom_uploads(), 1);
}
